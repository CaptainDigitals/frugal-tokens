//! Frugal Core: session economics aggregation and the health score.

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

pub use frugal_policy as policy;
pub use frugal_storage as storage;

#[derive(Debug, Serialize, Default)]
pub struct SessionStats {
    pub session_id: String,
    pub model: Option<String>,
    pub profile: String,
    pub cost_usd: Option<f64>,
    pub context_pct: Option<f64>,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub tools: Vec<ToolStat>,
    pub duplicate_calls: i64,
    pub duplicate_tokens: i64,
    pub budget_health: String,
}

#[derive(Debug, Serialize)]
pub struct ToolStat {
    pub tool: String,
    pub calls: i64,
    pub duplicates: i64,
    pub est_tokens: i64,
}

#[derive(Debug, Serialize)]
pub struct TodayStats {
    pub spend_usd: f64,
    pub sessions: i64,
    pub duplicate_calls: i64,
    pub duplicate_tokens: i64,
}

pub fn session_stats(conn: &Connection) -> Result<Option<SessionStats>> {
    let Some(session) = storage::latest_session(conn)? else {
        return Ok(None);
    };
    let cfg = policy::load();
    let tools = storage::session_tools(conn, &session.id)?;
    let (dups, dup_tokens) = storage::session_duplicates(conn, &session.id)?;
    let health = policy::budget_health(session.cost_usd, cfg.budgets.session_usd);
    Ok(Some(SessionStats {
        session_id: session.id,
        model: session.model,
        profile: cfg.profile,
        cost_usd: session.cost_usd,
        context_pct: session.context_pct,
        lines_added: session.lines_added,
        lines_removed: session.lines_removed,
        tools: tools
            .into_iter()
            .map(|t| ToolStat {
                tool: t.tool,
                calls: t.calls,
                duplicates: t.duplicates,
                est_tokens: t.est_tokens,
            })
            .collect(),
        duplicate_calls: dups,
        duplicate_tokens: dup_tokens,
        budget_health: health.to_string(),
    }))
}

pub fn today_stats(conn: &Connection) -> Result<TodayStats> {
    let (cost, sessions, dups, dup_tokens) =
        storage::period_totals(conn, &storage::today_prefix())?;
    Ok(TodayStats {
        spend_usd: cost,
        sessions,
        duplicate_calls: dups,
        duplicate_tokens: dup_tokens,
    })
}

/// Health score 0-100 from context pressure, duplicate waste, and budgets.
pub fn health_score(conn: &Connection) -> Result<i64> {
    let mut score: i64 = 100;
    let cfg = policy::load();
    if let Some(session) = storage::latest_session(conn)? {
        if let Some(ctx) = session.context_pct {
            score -= match ctx {
                c if c >= 90.0 => 30,
                c if c >= 75.0 => 20,
                c if c >= 60.0 => 10,
                c if c >= 40.0 => 5,
                _ => 0,
            };
        }
        let (dups, _) = storage::session_duplicates(conn, &session.id)?;
        score -= (dups * 2).min(20);
        match policy::budget_health(session.cost_usd, cfg.budgets.session_usd) {
            'X' => score -= 20,
            '!' => score -= 10,
            _ => {}
        }
    }
    Ok(score.max(0))
}

/// Waste summary: where avoidable context went (duplicates by tool, today).
pub fn waste_summary(conn: &Connection) -> Result<serde_json::Value> {
    let tools = storage::tools_today(conn)?;
    let waste: Vec<serde_json::Value> = tools
        .iter()
        .filter(|t| t.duplicates > 0)
        .map(|t| {
            serde_json::json!({
                "tool": t.tool,
                "duplicate_calls": t.duplicates,
                "est_wasted_tokens": t.est_tokens * t.duplicates / t.calls.max(1),
            })
        })
        .collect();
    Ok(serde_json::json!({
        "date": storage::today_prefix(),
        "duplicate_waste_by_tool": waste,
        "note": "duplicate tool calls = identical tool+input repeated within a session",
    }))
}
