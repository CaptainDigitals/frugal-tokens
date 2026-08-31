//! Frugal MCP server: structured access from Claude to Frugal Core.
//!
//! Speaks MCP over stdio (JSON-RPC 2.0, one message per line). Read tools are
//! unrestricted; mutation tools (frugal_checkpoint, frugal_set_profile,
//! frugal_set_budget) validate their inputs before touching state.

use anyhow::Result;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const PROTOCOL_VERSION: &str = "2024-11-05";

pub fn tool_definitions() -> Value {
    let no_args = json!({"type": "object", "properties": {}});
    json!([
        {"name": "frugal_get_session", "description": "Current/latest Claude Code session: model, cost, context %, lines changed.", "inputSchema": no_args},
        {"name": "frugal_get_stats", "description": "Full session economics: cost, context, per-tool calls, duplicate waste, budget health.", "inputSchema": no_args},
        {"name": "frugal_get_cost", "description": "Spend: current session cost and today's total across sessions.", "inputSchema": no_args},
        {"name": "frugal_get_tools", "description": "Per-tool analytics for today: calls, duplicates, estimated context tokens.", "inputSchema": no_args},
        {"name": "frugal_get_budget", "description": "Configured task/session/daily USD budgets and current health.", "inputSchema": no_args},
        {"name": "frugal_get_health", "description": "Frugal health score (0-100) from context pressure, duplicate waste, and budget state.", "inputSchema": no_args},
        {"name": "frugal_get_waste", "description": "Waste map: where avoidable context went today (duplicate calls by tool).", "inputSchema": no_args},
        {"name": "frugal_get_providers", "description": "Provider registry with trust class, capabilities, and installation status.", "inputSchema": no_args},
        {"name": "frugal_checkpoint", "description": "Create a semantic checkpoint file to fill in before compaction.",
         "inputSchema": {"type": "object", "properties": {"note": {"type": "string", "description": "short label for the checkpoint"}}}},
        {"name": "frugal_set_profile", "description": "Set the optimization profile (shadow|conservative|balanced|aggressive|off).",
         "inputSchema": {"type": "object", "properties": {"profile": {"type": "string"}}, "required": ["profile"]}},
        {"name": "frugal_set_budget", "description": "Set a USD budget. Scope: task|session|daily. Omit usd to clear.",
         "inputSchema": {"type": "object", "properties": {"scope": {"type": "string"}, "usd": {"type": "number"}}, "required": ["scope"]}}
    ])
}

pub fn call_tool(name: &str, args: &Value) -> Result<Value> {
    let conn = frugal_storage::open()?;
    let result = match name {
        "frugal_get_session" => match frugal_storage::latest_session(&conn)? {
            Some(s) => json!({
                "session_id": s.id, "model": s.model, "cost_usd": s.cost_usd,
                "context_pct": s.context_pct, "started_at": s.started_at,
                "updated_at": s.updated_at,
                "lines_added": s.lines_added, "lines_removed": s.lines_removed,
            }),
            None => json!({"error": "no sessions recorded yet"}),
        },
        "frugal_get_stats" => match frugal_core::session_stats(&conn)? {
            Some(stats) => serde_json::to_value(stats)?,
            None => json!({"error": "no sessions recorded yet"}),
        },
        "frugal_get_cost" => {
            let session_cost = frugal_storage::latest_session(&conn)?.and_then(|s| s.cost_usd);
            let today = frugal_core::today_stats(&conn)?;
            json!({"session_cost_usd": session_cost, "today": today})
        }
        "frugal_get_tools" => {
            let tools = frugal_storage::tools_today(&conn)?;
            json!(tools
                .iter()
                .map(|t| json!({
                    "tool": t.tool, "calls": t.calls,
                    "duplicates": t.duplicates, "est_tokens": t.est_tokens,
                }))
                .collect::<Vec<_>>())
        }
        "frugal_get_budget" => {
            let cfg = frugal_policy::load();
            let spend = frugal_storage::latest_session(&conn)?.and_then(|s| s.cost_usd);
            json!({
                "budgets": cfg.budgets,
                "session_spend_usd": spend,
                "session_health": frugal_policy::budget_health(spend, cfg.budgets.session_usd).to_string(),
            })
        }
        "frugal_get_health" => json!({"health_score": frugal_core::health_score(&conn)?}),
        "frugal_get_waste" => frugal_core::waste_summary(&conn)?,
        "frugal_get_providers" => {
            json!(frugal_providers::registry()
                .iter()
                .map(|p| json!({
                    "id": p.id, "trust": p.trust, "capabilities": p.capabilities,
                    "installed": frugal_providers::installed(p),
                    "install": p.install,
                    "fixed_context_tax_tokens": p.fixed_context_tax_tokens,
                }))
                .collect::<Vec<_>>())
        }
        "frugal_checkpoint" => {
            let note = args["note"].as_str().unwrap_or("via MCP");
            let session = frugal_storage::latest_session(&conn)?
                .map(|s| s.id)
                .unwrap_or_else(|| "manual".into());
            let path = frugal_telemetry::write_checkpoint(&conn, &session, note)?;
            json!({
                "checkpoint": path.to_string_lossy(),
                "next": "fill in objective/decisions/errors before compacting",
            })
        }
        "frugal_set_profile" => {
            let profile = args["profile"].as_str().unwrap_or_default();
            let cfg = frugal_policy::set_profile(profile)?;
            json!({"profile": cfg.profile})
        }
        "frugal_set_budget" => {
            let scope = args["scope"].as_str().unwrap_or_default();
            let usd = args["usd"].as_f64();
            let cfg = frugal_policy::set_budget(scope, usd)?;
            json!({"budgets": cfg.budgets})
        }
        other => json!({"error": format!("unknown tool {other}")}),
    };
    Ok(result)
}

fn response(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

pub fn handle_message(msg: &Value) -> Option<Value> {
    let method = msg["method"].as_str().unwrap_or_default();
    let id = msg["id"].clone();
    if id.is_null() {
        return None; // notification — no response
    }
    Some(match method {
        "initialize" => response(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "frugal-tokenomics", "version": env!("CARGO_PKG_VERSION")},
            }),
        ),
        "tools/list" => response(id, json!({"tools": tool_definitions()})),
        "tools/call" => {
            let name = msg["params"]["name"].as_str().unwrap_or_default();
            let args = &msg["params"]["arguments"];
            match call_tool(name, args) {
                Ok(result) => response(
                    id,
                    json!({
                        "content": [{"type": "text", "text": result.to_string()}]
                    }),
                ),
                Err(err) => response(
                    id,
                    json!({
                        "content": [{"type": "text", "text": format!("error: {err}")}],
                        "isError": true
                    }),
                ),
            }
        }
        "ping" => response(id, json!({})),
        other => error_response(id, -32601, &format!("method not found: {other}")),
    })
}

/// Blocking stdio serve loop (line-delimited JSON-RPC).
pub fn serve() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(reply) = handle_message(&msg) {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_and_list_tools() {
        let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
        let reply = handle_message(&init).unwrap();
        assert_eq!(reply["result"]["serverInfo"]["name"], "frugal-tokenomics");

        let list = json!({"jsonrpc":"2.0","id":2,"method":"tools/list"});
        let reply = handle_message(&list).unwrap();
        let tools = reply["result"]["tools"].as_array().unwrap();
        assert!(tools.iter().any(|t| t["name"] == "frugal_get_stats"));
        assert!(tools.len() >= 11);
    }

    #[test]
    fn notifications_get_no_response() {
        let note = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        assert!(handle_message(&note).is_none());
    }
}
