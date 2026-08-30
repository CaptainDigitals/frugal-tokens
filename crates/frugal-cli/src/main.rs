//! `frugal` — Frugal Tokens Community Edition CLI.
//!
//! Drop-in superset of the Python v0.1 runtime: same database, same
//! config.json, plus the Ratatui dashboard (`frugal` / `frugal dashboard`)
//! and the MCP server (`frugal mcp`).

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::Value;

#[derive(Parser)]
#[command(
    name = "frugal",
    version,
    about = "AI FinOps + Context Intelligence for Claude Code (Community Edition)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Interactive terminal dashboard (default)
    Dashboard,
    /// Current session economics
    Stats,
    /// Serve the Frugal MCP server over stdio
    Mcp,
    /// Consume a Claude Code hook event from stdin (fail-open)
    IngestHook,
    /// Consume statusline JSON from stdin, print the Frugal status line (fail-open)
    Statusline,
    /// Create a semantic checkpoint
    Checkpoint { note: Vec<String> },
    /// List checkpoints
    Checkpoints,
    /// Show or set budgets: `budget` | `budget set session 10` | `budget clear daily`
    Budget { args: Vec<String> },
    /// Show or set profile: `profile` | `profile set balanced`
    Profile { args: Vec<String> },
    /// Rollup for a period: today (default) or month
    Report { period: Option<String> },
    /// Provider registry with installation status
    Providers,
    /// Health checks
    Doctor,
    /// Safe Mode: shadow profile, observe-only
    Safe,
}

fn main() {
    let cli = Cli::parse();
    let code = match run(cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}");
            1
        }
    };
    std::process::exit(code);
}

fn run(cli: Cli) -> Result<i32> {
    match cli.command.unwrap_or(Command::Dashboard) {
        Command::Dashboard => {
            frugal_tui::run()?;
            Ok(0)
        }
        Command::Stats => cmd_stats(),
        Command::Mcp => {
            frugal_mcp::serve()?;
            Ok(0)
        }
        Command::IngestHook => {
            // Fail-open: never a non-zero exit, never a panic surfaced to Claude.
            let _ = std::panic::catch_unwind(|| {
                if let Ok(payload) = serde_json::from_reader::<_, Value>(std::io::stdin()) {
                    let _ = frugal_hooks::ingest(&payload);
                }
            });
            Ok(0)
        }
        Command::Statusline => {
            let line = std::panic::catch_unwind(statusline).unwrap_or_else(|_| "◈ FRUGAL".into());
            println!("{line}");
            Ok(0)
        }
        Command::Checkpoint { note } => {
            let conn = frugal_storage::open()?;
            let session = frugal_storage::latest_session(&conn)?
                .map(|s| s.id)
                .unwrap_or_else(|| "manual".into());
            let note = if note.is_empty() {
                "manual".into()
            } else {
                note.join(" ")
            };
            let path = frugal_telemetry::write_checkpoint(&conn, &session, &note)?;
            println!("checkpoint template written: {}", path.display());
            println!("Fill in objective/decisions/errors before compacting.");
            Ok(0)
        }
        Command::Checkpoints => {
            let conn = frugal_storage::open()?;
            let rows = frugal_storage::list_checkpoints(&conn, 20)?;
            if rows.is_empty() {
                println!("no checkpoints yet — create one with: frugal checkpoint");
            }
            for c in rows {
                println!("{}  {:<28} {}", c.ts, c.note, c.path);
            }
            Ok(0)
        }
        Command::Budget { args } => cmd_budget(&args),
        Command::Profile { args } => cmd_profile(&args),
        Command::Report { period } => cmd_report(period.as_deref().unwrap_or("today")),
        Command::Providers => cmd_providers(),
        Command::Doctor => cmd_doctor(),
        Command::Safe => {
            frugal_policy::set_profile("shadow")?;
            println!(
                "SAFE MODE: profile set to shadow — measurement and budget warnings \
                 only; no intervention. Claude Code behavior is unchanged."
            );
            Ok(0)
        }
    }
}

fn statusline() -> String {
    let Ok(payload) = serde_json::from_reader::<_, Value>(std::io::stdin()) else {
        return "◈ FRUGAL".into();
    };
    let session_id = payload["session_id"].as_str().unwrap_or("unknown");
    let model = payload["model"]["display_name"].as_str().unwrap_or("");
    let cost = payload["cost"]["total_cost_usd"].as_f64();
    let added = payload["cost"]["total_lines_added"].as_i64().unwrap_or(0);
    let removed = payload["cost"]["total_lines_removed"].as_i64().unwrap_or(0);
    let cwd = payload["workspace"]["current_dir"].as_str().unwrap_or("");
    let ctx_pct = context_pct(&payload);

    let mut dups = 0;
    if let Ok(conn) = frugal_storage::open() {
        let _ = frugal_storage::snapshot_session(
            &conn, session_id, model, cwd, cost, ctx_pct, added, removed,
        );
        if let Ok((d, _)) = frugal_storage::session_duplicates(&conn, session_id) {
            dups = d;
        }
    }

    let cfg = frugal_policy::load();
    let mut parts = vec!["◈ FRUGAL".to_string()];
    if let Some(ctx) = ctx_pct {
        parts.push(format!("CTX {ctx:.0}%"));
    }
    if let Some(cost) = cost {
        parts.push(format!("${cost:.2}"));
    }
    let health = frugal_policy::budget_health(cost, cfg.budgets.session_usd);
    if health == 'X' {
        if let Some(limit) = cfg.budgets.session_usd {
            parts.push(format!("BUDGET ${limit:.0} EXCEEDED"));
        }
    }
    if dups > 0 {
        parts.push(format!("dup {dups}"));
    }
    parts.push(format!("{} {}", cfg.profile.to_uppercase(), health));
    parts.join(" │ ")
}

fn context_pct(payload: &Value) -> Option<f64> {
    for key in ["context_window_usage", "context_usage", "context"] {
        let value = &payload[key];
        if let (Some(used), Some(size)) = (
            value["used_tokens"].as_f64(),
            value["context_window_size"].as_f64(),
        ) {
            if size > 0.0 {
                return Some(100.0 * used / size);
            }
        }
        if let Some(pct) = value.as_f64() {
            return Some(pct);
        }
    }
    None
}

fn cmd_stats() -> Result<i32> {
    let conn = frugal_storage::open()?;
    let Some(stats) = frugal_core::session_stats(&conn)? else {
        println!(
            "no sessions recorded yet — telemetry starts once the plugin's \
             hooks/status line run inside Claude Code"
        );
        return Ok(1);
    };
    println!("◈ FRUGAL TOKENS — SESSION STATS");
    println!("{}", "=".repeat(44));
    println!(
        "session      {}",
        stats.session_id.chars().take(20).collect::<String>()
    );
    println!("model        {}", stats.model.as_deref().unwrap_or("-"));
    println!("profile      {}", stats.profile);
    if let Some(cost) = stats.cost_usd {
        println!("cost         ${cost:.2}");
    }
    if let Some(ctx) = stats.context_pct {
        println!("context      {ctx:.0}%");
    }
    println!(
        "lines        +{} / -{}",
        stats.lines_added, stats.lines_removed
    );
    println!("\nTOOL CALLS                calls  dup  est.tokens");
    for t in stats.tools.iter().take(10) {
        println!(
            "  {:<22} {:>5} {:>4} {:>10}",
            t.tool, t.calls, t.duplicates, t.est_tokens
        );
    }
    if stats.duplicate_calls > 0 {
        println!(
            "\n⚠ {} duplicate tool calls (~{} wasted tokens) — identical \
             tool+input repeated within this session",
            stats.duplicate_calls, stats.duplicate_tokens
        );
    }
    println!(
        "\nhealth score  {}/100  budget [{}]",
        frugal_core::health_score(&conn)?,
        stats.budget_health
    );
    Ok(0)
}

fn cmd_budget(args: &[String]) -> Result<i32> {
    let cfg = frugal_policy::load();
    match args.first().map(String::as_str) {
        None | Some("show") => {
            println!("{}", serde_json::to_string_pretty(&cfg.budgets)?);
        }
        Some("set") => {
            let scope = args.get(1).map(String::as_str).unwrap_or("");
            let usd: f64 = args.get(2).and_then(|v| v.parse().ok()).ok_or_else(|| {
                anyhow::anyhow!("usage: frugal budget set <task|session|daily> <usd>")
            })?;
            let cfg = frugal_policy::set_budget(scope, Some(usd))?;
            println!("{}", serde_json::to_string_pretty(&cfg.budgets)?);
        }
        Some("clear") => {
            let scope = args.get(1).map(String::as_str).unwrap_or("");
            let cfg = frugal_policy::set_budget(scope, None)?;
            println!("{}", serde_json::to_string_pretty(&cfg.budgets)?);
        }
        Some(other) => anyhow::bail!("unknown budget action {other:?} (show|set|clear)"),
    }
    Ok(0)
}

fn cmd_profile(args: &[String]) -> Result<i32> {
    match args.first().map(String::as_str) {
        None | Some("show") => println!("{}", frugal_policy::load().profile),
        Some("set") => {
            let name = args.get(1).map(String::as_str).unwrap_or("");
            let cfg = frugal_policy::set_profile(name)?;
            let note = if cfg.profile == "shadow" {
                " (observe-only)"
            } else {
                ""
            };
            println!("profile = {}{note}", cfg.profile);
        }
        Some(other) => anyhow::bail!("unknown profile action {other:?} (show|set)"),
    }
    Ok(0)
}

fn cmd_report(period: &str) -> Result<i32> {
    let conn = frugal_storage::open()?;
    let prefix = if period == "month" {
        frugal_storage::month_prefix()
    } else {
        frugal_storage::today_prefix()
    };
    let (cost, sessions, dups, dup_tokens) = frugal_storage::period_totals(&conn, &prefix)?;
    println!("FRUGAL REPORT — {prefix}");
    println!("  sessions               {sessions}");
    println!("  recorded spend         ${cost:.2}");
    println!("  duplicate tool calls   {dups}");
    println!("  est. duplicated tokens {dup_tokens}");
    println!("\nLocal only. Nothing leaves this machine.");
    Ok(0)
}

fn cmd_providers() -> Result<i32> {
    println!(
        "{:<22} {:<14} {:<12} {:>8}  install",
        "provider", "trust", "status", "ctx tax"
    );
    for p in frugal_providers::registry() {
        let installed = frugal_providers::installed(&p);
        let status = if installed { "INSTALLED" } else { "available" };
        let hint = if installed { "" } else { p.install.as_str() };
        println!(
            "{:<22} {:<14} {:<12} {:>8}  {}",
            p.id, p.trust, status, p.fixed_context_tax_tokens, hint
        );
    }
    Ok(0)
}

fn cmd_doctor() -> Result<i32> {
    let mut checks: Vec<(String, bool, String)> = Vec::new();
    match frugal_storage::open() {
        Ok(conn) => {
            checks.push(("SQLite database".into(), true, String::new()));
            let sessions = frugal_storage::session_count(&conn).unwrap_or(0);
            checks.push((
                format!("telemetry ({sessions} sessions recorded)"),
                sessions > 0,
                "starts after first Claude Code session with the plugin enabled".into(),
            ));
        }
        Err(err) => checks.push(("SQLite database".into(), false, err.to_string())),
    }
    let cfg = frugal_policy::load();
    checks.push((
        format!("config (profile: {})", cfg.profile),
        frugal_policy::PROFILES.contains(&cfg.profile.as_str()),
        String::new(),
    ));
    checks.push((
        "data directory".into(),
        frugal_storage::frugal_dir().exists()
            || std::fs::create_dir_all(frugal_storage::frugal_dir()).is_ok(),
        String::new(),
    ));
    let provider_count = frugal_providers::registry().len();
    checks.push((
        format!("provider registry ({provider_count} providers)"),
        provider_count > 0,
        String::new(),
    ));

    println!("FRUGAL DOCTOR (rust core v{})", env!("CARGO_PKG_VERSION"));
    println!("{}", "=".repeat(44));
    let mut passed = 0;
    for (name, ok, hint) in &checks {
        let mark = if *ok { "✓" } else { "⚠" };
        if *ok {
            passed += 1;
        }
        if !ok && !hint.is_empty() {
            println!("{mark} {name}  → {hint}");
        } else {
            println!("{mark} {name}");
        }
    }
    let score = 100 * passed / checks.len();
    println!("{}", "-".repeat(44));
    println!("System Health  {score} / 100");
    Ok(if score >= 70 { 0 } else { 1 })
}
