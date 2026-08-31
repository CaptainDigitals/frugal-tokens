//! Frugal Tokenomics local data plane: SQLite at `~/.frugal/frugal.db`.
//!
//! The schema is identical to the Python v0.1 reference runtime
//! (`claude-plugin/bin/frugal.py`) so both runtimes share one ledger.

use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::PathBuf;

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    started_at TEXT,
    updated_at TEXT,
    model TEXT,
    cwd TEXT,
    cost_usd REAL DEFAULT 0,
    context_pct REAL,
    lines_added INTEGER DEFAULT 0,
    lines_removed INTEGER DEFAULT 0
);
CREATE TABLE IF NOT EXISTS tool_calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT,
    ts TEXT,
    tool TEXT,
    fingerprint TEXT,
    duplicate INTEGER DEFAULT 0,
    est_tokens INTEGER DEFAULT 0
);
CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT,
    ts TEXT,
    kind TEXT,
    detail TEXT
);
CREATE TABLE IF NOT EXISTS checkpoints (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT,
    ts TEXT,
    path TEXT,
    note TEXT
);
CREATE INDEX IF NOT EXISTS idx_tool_session ON tool_calls(session_id);
CREATE INDEX IF NOT EXISTS idx_tool_fp ON tool_calls(session_id, fingerprint);
"#;

pub fn frugal_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("FRUGAL_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".frugal")
}

pub fn now() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

pub fn today_prefix() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn month_prefix() -> String {
    chrono::Local::now().format("%Y-%m").to_string()
}

pub fn open() -> Result<Connection> {
    let dir = frugal_dir();
    std::fs::create_dir_all(&dir)?;
    let conn = Connection::open(dir.join("frugal.db"))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

#[derive(Debug, Clone, Default)]
pub struct SessionRow {
    pub id: String,
    pub model: Option<String>,
    pub cost_usd: Option<f64>,
    pub context_pct: Option<f64>,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub lines_added: i64,
    pub lines_removed: i64,
}

#[derive(Debug, Clone)]
pub struct ToolRow {
    pub tool: String,
    pub calls: i64,
    pub duplicates: i64,
    pub est_tokens: i64,
}

#[derive(Debug, Clone)]
pub struct EventRow {
    pub ts: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct CheckpointRow {
    pub ts: String,
    pub note: String,
    pub path: String,
}

pub fn touch_session(conn: &Connection, id: &str, cwd: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO sessions(id, started_at, updated_at, cwd) VALUES(?1,?2,?2,?3) \
         ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at",
        params![id, now(), cwd],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn snapshot_session(
    conn: &Connection,
    id: &str,
    model: &str,
    cwd: &str,
    cost_usd: Option<f64>,
    context_pct: Option<f64>,
    lines_added: i64,
    lines_removed: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO sessions(id, started_at, updated_at, model, cwd, cost_usd, \
         context_pct, lines_added, lines_removed) \
         VALUES(?1,?2,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET \
         updated_at=excluded.updated_at, model=excluded.model, \
         cost_usd=COALESCE(excluded.cost_usd, cost_usd), \
         context_pct=COALESCE(excluded.context_pct, context_pct), \
         lines_added=excluded.lines_added, lines_removed=excluded.lines_removed",
        params![
            id,
            now(),
            model,
            cwd,
            cost_usd,
            context_pct,
            lines_added,
            lines_removed
        ],
    )?;
    Ok(())
}

/// Records a tool call; returns how many identical prior calls existed
/// (0 means this call is not a duplicate).
pub fn record_tool_call(
    conn: &Connection,
    session_id: &str,
    tool: &str,
    fingerprint: &str,
    est_tokens: i64,
) -> Result<i64> {
    let prior: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tool_calls WHERE session_id=?1 AND fingerprint=?2",
        params![session_id, fingerprint],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO tool_calls(session_id, ts, tool, fingerprint, duplicate, est_tokens) \
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            session_id,
            now(),
            tool,
            fingerprint,
            (prior > 0) as i64,
            est_tokens
        ],
    )?;
    Ok(prior)
}

pub fn record_event(conn: &Connection, session_id: &str, kind: &str, detail: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO events(session_id, ts, kind, detail) VALUES(?1,?2,?3,?4)",
        params![session_id, now(), kind, detail],
    )?;
    Ok(())
}

pub fn record_checkpoint(
    conn: &Connection,
    session_id: &str,
    path: &str,
    note: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO checkpoints(session_id, ts, path, note) VALUES(?1,?2,?3,?4)",
        params![session_id, now(), path, note],
    )?;
    Ok(())
}

pub fn latest_session(conn: &Connection) -> Result<Option<SessionRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, model, cost_usd, context_pct, started_at, updated_at, \
         lines_added, lines_removed FROM sessions ORDER BY updated_at DESC LIMIT 1",
    )?;
    let mut rows = stmt.query_map([], row_to_session)?;
    Ok(rows.next().transpose()?)
}

pub fn recent_sessions(conn: &Connection, limit: i64) -> Result<Vec<SessionRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, model, cost_usd, context_pct, started_at, updated_at, \
         lines_added, lines_removed FROM sessions ORDER BY updated_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], row_to_session)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

fn row_to_session(r: &rusqlite::Row) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: r.get(0)?,
        model: r.get(1)?,
        cost_usd: r.get(2)?,
        context_pct: r.get(3)?,
        started_at: r.get(4)?,
        updated_at: r.get(5)?,
        lines_added: r.get::<_, Option<i64>>(6)?.unwrap_or(0),
        lines_removed: r.get::<_, Option<i64>>(7)?.unwrap_or(0),
    })
}

pub fn session_tools(conn: &Connection, session_id: &str) -> Result<Vec<ToolRow>> {
    tools_where(conn, "session_id=?1", session_id)
}

pub fn tools_today(conn: &Connection) -> Result<Vec<ToolRow>> {
    let prefix = format!("{}%", today_prefix());
    tools_where(conn, "ts LIKE ?1", &prefix)
}

fn tools_where(conn: &Connection, clause: &str, arg: &str) -> Result<Vec<ToolRow>> {
    let sql = format!(
        "SELECT tool, COUNT(*), COALESCE(SUM(duplicate),0), COALESCE(SUM(est_tokens),0) \
         FROM tool_calls WHERE {clause} GROUP BY tool ORDER BY SUM(est_tokens) DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![arg], |r| {
        Ok(ToolRow {
            tool: r.get(0)?,
            calls: r.get(1)?,
            duplicates: r.get(2)?,
            est_tokens: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn session_duplicates(conn: &Connection, session_id: &str) -> Result<(i64, i64)> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(duplicate),0), COALESCE(SUM(CASE WHEN duplicate=1 \
         THEN est_tokens ELSE 0 END),0) FROM tool_calls WHERE session_id=?1",
        params![session_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
}

/// (cost, sessions, duplicate_calls, duplicate_tokens) for a ts/updated_at prefix.
pub fn period_totals(conn: &Connection, prefix: &str) -> Result<(f64, i64, i64, i64)> {
    let like = format!("{prefix}%");
    let (cost, sessions): (f64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(cost_usd),0), COUNT(*) FROM sessions WHERE updated_at LIKE ?1",
        params![like],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (dups, dup_tokens): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(duplicate),0), COALESCE(SUM(CASE WHEN duplicate=1 \
         THEN est_tokens ELSE 0 END),0) FROM tool_calls WHERE ts LIKE ?1",
        params![like],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok((cost, sessions, dups, dup_tokens))
}

pub fn recent_events(conn: &Connection, limit: i64) -> Result<Vec<EventRow>> {
    let mut stmt = conn.prepare("SELECT ts, kind, detail FROM events ORDER BY id DESC LIMIT ?1")?;
    let rows = stmt.query_map(params![limit], |r| {
        Ok(EventRow {
            ts: r.get(0)?,
            kind: r.get(1)?,
            detail: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn list_checkpoints(conn: &Connection, limit: i64) -> Result<Vec<CheckpointRow>> {
    let mut stmt =
        conn.prepare("SELECT ts, note, path FROM checkpoints ORDER BY id DESC LIMIT ?1")?;
    let rows = stmt.query_map(params![limit], |r| {
        Ok(CheckpointRow {
            ts: r.get(0)?,
            note: r.get(1)?,
            path: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn session_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?)
}
