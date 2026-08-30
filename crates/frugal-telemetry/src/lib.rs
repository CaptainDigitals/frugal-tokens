//! Telemetry writers + semantic checkpoints.
//!
//! Sensitive-data rule (PRD s75): only tool names, fingerprints, token
//! estimates, costs, and event descriptions are persisted — never raw
//! source, prompts, or credentials.

use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;

pub use frugal_storage as storage;
use frugal_storage::{record_checkpoint, record_event};

const CHECKPOINT_TEMPLATE: &str = "# Frugal Checkpoint — {ts}\n\n\
note: {note}\nsession: {session}\n\n\
objective: <what we're trying to accomplish>\n\
current_state: <where things stand>\n\
decisions: <choices made and WHY>\n\
modified_files: <paths + one-line what-changed>\n\
unresolved_errors: <exact error text>\n\
constraints: <Tier 0 — contracts, security rules, requirements>\n\
test_status: <passes / fails>\n\
next_steps: <ordered, specific>\n";

pub fn write_checkpoint(conn: &Connection, session_id: &str, note: &str) -> Result<PathBuf> {
    let dir = frugal_storage::frugal_dir().join("checkpoints");
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono_stamp();
    let path = dir.join(format!("{stamp}.md"));
    let body = CHECKPOINT_TEMPLATE
        .replace("{ts}", &frugal_storage::now())
        .replace("{note}", note)
        .replace("{session}", session_id);
    std::fs::write(&path, body)?;
    record_checkpoint(conn, session_id, &path.to_string_lossy(), note)?;
    Ok(path)
}

fn chrono_stamp() -> String {
    // Millisecond suffix avoids collisions when checkpoints land in the same second.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{}-{:03}",
        frugal_storage::now().replace([':', 'T', '+'], "-"),
        now.subsec_millis()
    )
}

pub fn log_event(conn: &Connection, session_id: &str, kind: &str, detail: &str) -> Result<()> {
    record_event(conn, session_id, kind, detail)
}
