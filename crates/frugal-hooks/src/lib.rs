//! Claude Code hook ingestion.
//!
//! Fail-open contract (PRD s85): `ingest` returns Result for callers that
//! want detail, but the CLI entry point swallows all errors — a Frugal
//! failure must never block Claude Code.

use anyhow::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Canonical JSON with recursively sorted object keys (fingerprint stability).
fn canonical(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let inner: Vec<String> = keys
                .iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap_or_default(),
                        canonical(&map[*k])
                    )
                })
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(canonical).collect();
            format!("[{}]", inner.join(","))
        }
        other => other.to_string(),
    }
}

pub fn fingerprint(tool: &str, tool_input: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(tool.as_bytes());
    hasher.update(canonical(tool_input).as_bytes());
    let digest = hasher.finalize();
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

pub fn estimate_tokens(value: &Value) -> i64 {
    (serde_json::to_string(value).map(|s| s.len()).unwrap_or(0) / 4) as i64
}

/// Consume one hook event payload. Returns a short description of what was
/// recorded (for logging/tests).
pub fn ingest(payload: &Value) -> Result<String> {
    let session_id = payload["session_id"].as_str().unwrap_or("unknown");
    let event = payload["hook_event_name"].as_str().unwrap_or("");
    let cwd = payload["cwd"].as_str().unwrap_or("");

    let conn = frugal_storage::open()?;
    frugal_storage::touch_session(&conn, session_id, cwd)?;

    match event {
        "PostToolUse" => {
            let tool = payload["tool_name"].as_str().unwrap_or("?");
            let fp = fingerprint(tool, &payload["tool_input"]);
            let tokens = estimate_tokens(&payload["tool_response"]);
            let prior = frugal_storage::record_tool_call(&conn, session_id, tool, &fp, tokens)?;
            if prior > 0 {
                frugal_telemetry::log_event(
                    &conn,
                    session_id,
                    "duplicate_tool_call",
                    &format!("{tool} repeated with identical input (x{})", prior + 1),
                )?;
                return Ok(format!("duplicate {tool} (x{})", prior + 1));
            }
            Ok(format!("recorded {tool}"))
        }
        "PreCompact" => {
            let path =
                frugal_telemetry::write_checkpoint(&conn, session_id, "auto: pre-compaction")?;
            frugal_telemetry::log_event(
                &conn,
                session_id,
                "pre_compact_checkpoint",
                &path.to_string_lossy(),
            )?;
            Ok("pre-compaction checkpoint".into())
        }
        "SessionStart" | "SessionEnd" | "Stop" => {
            frugal_telemetry::log_event(&conn, session_id, event, "")?;
            Ok(format!("recorded {event}"))
        }
        other => Ok(format!("ignored {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fingerprint_is_order_insensitive() {
        let a = json!({"b": 1, "a": {"y": 2, "x": 3}});
        let b = json!({"a": {"x": 3, "y": 2}, "b": 1});
        assert_eq!(fingerprint("Read", &a), fingerprint("Read", &b));
    }

    #[test]
    fn fingerprint_differs_by_tool_and_input() {
        let input = json!({"file_path": "a.ts"});
        assert_ne!(fingerprint("Read", &input), fingerprint("Grep", &input));
        assert_ne!(
            fingerprint("Read", &json!({"file_path": "a.ts"})),
            fingerprint("Read", &json!({"file_path": "b.ts"}))
        );
    }
}
