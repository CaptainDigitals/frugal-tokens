#!/usr/bin/env python3
"""Frugal Tokenomics Community Edition — local runtime (v0.1 reference implementation).

Local-first AI FinOps for Claude Code: SQLite telemetry, cost/context
visibility, duplicate tool-call detection, budgets, semantic checkpoints,
status line, terminal dashboard, doctor. Stdlib only. No cloud, no account.

Fail-open guarantee (PRD section 85): hook and statusline entry points never
raise — a Frugal failure must never break Claude Code.

Commands:
    ingest-hook          consume a Claude Code hook event from stdin (internal)
    statusline           consume statusline JSON from stdin, print status line
    stats                current/latest session summary
    dashboard            terminal dashboard snapshot
    report [month|today] historical rollup
    export [--json|--csv]
    budget show | set <task|session|daily> <usd>
    profile show | set <shadow|conservative|balanced|aggressive|off>
    checkpoint [note]    write a semantic checkpoint template
    checkpoints          list checkpoints
    doctor               health checks
    safe                 switch to safe (observe-only) profile
    setup-statusline     print settings.json snippet wiring the status line
"""
import argparse
import csv
import datetime
import hashlib
import io
import json
import os
import sqlite3
import sys
from pathlib import Path

FRUGAL_DIR = Path(os.environ.get("FRUGAL_DIR", "")) if os.environ.get("FRUGAL_DIR") \
    else Path.home() / ".frugal"
DB_PATH = FRUGAL_DIR / "frugal.db"
CONFIG_PATH = FRUGAL_DIR / "config.json"
CHECKPOINT_DIR = FRUGAL_DIR / "checkpoints"

PROFILES = ("shadow", "conservative", "balanced", "aggressive", "off")
DEFAULT_CONFIG = {"profile": "shadow",
                  "budgets": {"task_usd": None, "session_usd": None,
                              "daily_usd": None}}

SCHEMA = """
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
"""


def now() -> str:
    # Local time (with offset) so "today"/"month" report prefixes match the
    # user's calendar, not UTC's.
    return datetime.datetime.now().astimezone().isoformat(timespec="seconds")


def db() -> sqlite3.Connection:
    FRUGAL_DIR.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(DB_PATH, timeout=5)
    conn.executescript(SCHEMA)
    return conn


def load_config() -> dict:
    try:
        cfg = json.loads(CONFIG_PATH.read_text(encoding="utf-8"))
        return {**DEFAULT_CONFIG, **cfg,
                "budgets": {**DEFAULT_CONFIG["budgets"],
                            **cfg.get("budgets", {})}}
    except (OSError, json.JSONDecodeError):
        return dict(DEFAULT_CONFIG)


def save_config(cfg: dict) -> None:
    FRUGAL_DIR.mkdir(parents=True, exist_ok=True)
    CONFIG_PATH.write_text(json.dumps(cfg, indent=2), encoding="utf-8")


# ----------------------------------------------------------------- ingestion

def _fingerprint(tool: str, tool_input) -> str:
    canon = json.dumps(tool_input, sort_keys=True, default=str)
    return hashlib.sha256((tool + canon).encode("utf-8")).hexdigest()[:16]


def _estimate_tokens(obj) -> int:
    try:
        return int(len(json.dumps(obj, default=str)) / 4)
    except (TypeError, ValueError):
        return 0


def cmd_ingest_hook() -> int:
    """Fail-open: any error exits 0 so Claude Code is never blocked."""
    try:
        payload = json.load(sys.stdin)
    except Exception:
        return 0
    try:
        session_id = payload.get("session_id", "unknown")
        event = payload.get("hook_event_name", "")
        conn = db()
        conn.execute(
            "INSERT INTO sessions(id, started_at, updated_at, cwd) VALUES(?,?,?,?) "
            "ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at",
            (session_id, now(), now(), payload.get("cwd", "")))
        if event == "PostToolUse":
            tool = payload.get("tool_name", "?")
            fp = _fingerprint(tool, payload.get("tool_input", {}))
            dup = conn.execute(
                "SELECT COUNT(*) FROM tool_calls WHERE session_id=? AND fingerprint=?",
                (session_id, fp)).fetchone()[0]
            conn.execute(
                "INSERT INTO tool_calls(session_id, ts, tool, fingerprint, "
                "duplicate, est_tokens) VALUES(?,?,?,?,?,?)",
                (session_id, now(), tool, fp, 1 if dup else 0,
                 _estimate_tokens(payload.get("tool_response", ""))))
            if dup:
                conn.execute(
                    "INSERT INTO events(session_id, ts, kind, detail) VALUES(?,?,?,?)",
                    (session_id, now(), "duplicate_tool_call",
                     f"{tool} repeated with identical input (x{dup + 1})"))
        elif event == "PreCompact":
            path = _write_checkpoint(session_id, "auto: pre-compaction", conn)
            conn.execute(
                "INSERT INTO events(session_id, ts, kind, detail) VALUES(?,?,?,?)",
                (session_id, now(), "pre_compact_checkpoint", str(path)))
        elif event in ("SessionStart", "SessionEnd", "Stop"):
            conn.execute(
                "INSERT INTO events(session_id, ts, kind, detail) VALUES(?,?,?,?)",
                (session_id, now(), event, ""))
        conn.commit()
        conn.close()
    except Exception:
        pass
    return 0


# ---------------------------------------------------------------- statusline

def cmd_statusline() -> int:
    """Reads Claude Code statusline JSON on stdin; prints the Frugal line.
    Also snapshots cost/context into SQLite (statusline runs frequently, so it
    doubles as the economics sampler). Fail-open."""
    line = "◈ FRUGAL"
    try:
        payload = json.load(sys.stdin)
    except Exception:
        print(line)
        return 0
    try:
        session_id = payload.get("session_id", "unknown")
        model = (payload.get("model") or {}).get("display_name", "")
        cost = (payload.get("cost") or {})
        cost_usd = cost.get("total_cost_usd")
        added = cost.get("total_lines_added") or 0
        removed = cost.get("total_lines_removed") or 0
        ctx_pct = None
        for key in ("context_window_usage", "context_usage", "context"):
            value = payload.get(key)
            if isinstance(value, dict):
                used, size = value.get("used_tokens"), value.get("context_window_size")
                if used and size:
                    ctx_pct = 100.0 * used / size
            elif isinstance(value, (int, float)):
                ctx_pct = float(value)

        conn = db()
        conn.execute(
            "INSERT INTO sessions(id, started_at, updated_at, model, cwd, "
            "cost_usd, context_pct, lines_added, lines_removed) "
            "VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET "
            "updated_at=excluded.updated_at, model=excluded.model, "
            "cost_usd=COALESCE(excluded.cost_usd, cost_usd), "
            "context_pct=COALESCE(excluded.context_pct, context_pct), "
            "lines_added=excluded.lines_added, lines_removed=excluded.lines_removed",
            (session_id, now(), now(), model,
             (payload.get("workspace") or {}).get("current_dir", ""),
             cost_usd, ctx_pct, added, removed))
        dups = conn.execute(
            "SELECT COALESCE(SUM(duplicate),0) FROM tool_calls WHERE session_id=?",
            (session_id,)).fetchone()[0]
        conn.commit()
        conn.close()

        cfg = load_config()
        parts = [line]
        if ctx_pct is not None:
            parts.append(f"CTX {ctx_pct:.0f}%")
        if cost_usd is not None:
            parts.append(f"${cost_usd:.2f}")
        session_budget = cfg["budgets"].get("session_usd")
        health = "✓"
        if session_budget and cost_usd is not None:
            if cost_usd >= session_budget:
                health = "X"
                parts.append(f"BUDGET ${session_budget:.0f} EXCEEDED")
            elif cost_usd >= 0.8 * session_budget:
                health = "!"
        if dups:
            parts.append(f"dup {dups}")
        parts.append(f"{cfg['profile'].upper()} {health}")
        line = " │ ".join(parts)
    except Exception:
        pass
    print(line)
    return 0


# --------------------------------------------------------------- checkpoints

CHECKPOINT_TEMPLATE = """# Frugal Checkpoint — {ts}

note: {note}
session: {session}

objective: <what we're trying to accomplish>
current_state: <where things stand>
decisions: <choices made and WHY>
modified_files: <paths + one-line what-changed>
unresolved_errors: <exact error text>
constraints: <Tier 0 — contracts, security rules, requirements>
test_status: <passes / fails>
next_steps: <ordered, specific>
"""


def _write_checkpoint(session_id: str, note: str, conn=None) -> Path:
    CHECKPOINT_DIR.mkdir(parents=True, exist_ok=True)
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    path = CHECKPOINT_DIR / f"{stamp}.md"
    path.write_text(CHECKPOINT_TEMPLATE.format(ts=now(), note=note,
                                               session=session_id),
                    encoding="utf-8")
    owned = conn is None
    conn = conn or db()
    conn.execute("INSERT INTO checkpoints(session_id, ts, path, note) "
                 "VALUES(?,?,?,?)", (session_id, now(), str(path), note))
    if owned:
        conn.commit()
        conn.close()
    return path


def cmd_checkpoint(note: str) -> int:
    path = _write_checkpoint(_latest_session_id() or "manual", note or "manual")
    print(f"checkpoint template written: {path}")
    print("Fill in objective/decisions/errors before compacting.")
    return 0


def cmd_checkpoints() -> int:
    conn = db()
    rows = conn.execute("SELECT ts, note, path FROM checkpoints "
                        "ORDER BY id DESC LIMIT 20").fetchall()
    conn.close()
    if not rows:
        print("no checkpoints yet — create one with: frugal checkpoint")
        return 0
    for ts, note, path in rows:
        print(f"{ts}  {note:<28} {path}")
    return 0


# ------------------------------------------------------------------ reporting

def _latest_session_id():
    conn = db()
    row = conn.execute(
        "SELECT id FROM sessions ORDER BY updated_at DESC LIMIT 1").fetchone()
    conn.close()
    return row[0] if row else None


def _session_summary(conn, session_id):
    session = conn.execute(
        "SELECT model, cost_usd, context_pct, started_at, updated_at, "
        "lines_added, lines_removed FROM sessions WHERE id=?",
        (session_id,)).fetchone()
    tools = conn.execute(
        "SELECT tool, COUNT(*), SUM(duplicate), SUM(est_tokens) FROM tool_calls "
        "WHERE session_id=? GROUP BY tool ORDER BY SUM(est_tokens) DESC",
        (session_id,)).fetchall()
    return session, tools


def cmd_stats() -> int:
    session_id = _latest_session_id()
    if not session_id:
        print("no sessions recorded yet — telemetry starts once the plugin's "
              "hooks/status line run inside Claude Code")
        return 1
    conn = db()
    session, tools = _session_summary(conn, session_id)
    dup_total = sum(r[2] or 0 for r in tools)
    dup_tokens = conn.execute(
        "SELECT COALESCE(SUM(est_tokens),0) FROM tool_calls "
        "WHERE session_id=? AND duplicate=1", (session_id,)).fetchone()[0]
    conn.close()
    cfg = load_config()

    model, cost, ctx, started, updated, added, removed = session
    print("◈ Frugal Tokenomics — SESSION STATS")
    print("=" * 44)
    print(f"session      {session_id[:20]}")
    print(f"model        {model or '-'}")
    print(f"profile      {cfg['profile']}")
    if cost is not None:
        print(f"cost         ${cost:.2f}")
    if ctx is not None:
        print(f"context      {ctx:.0f}%")
    print(f"lines        +{added} / -{removed}")
    print(f"\nTOOL CALLS                calls  dup  est.tokens")
    for tool, calls, dups, tokens in tools[:10]:
        print(f"  {tool:<22} {calls:>5} {dups or 0:>4} {tokens or 0:>10,}")
    if dup_total:
        print(f"\n⚠ {dup_total} duplicate tool calls "
              f"(~{dup_tokens:,} wasted tokens) — identical tool+input "
              "repeated within this session")
    budgets = cfg["budgets"]
    if any(budgets.values()):
        print("\nBUDGETS")
        for scope, limit in budgets.items():
            if limit:
                print(f"  {scope:<12} ${limit}")
    return 0


def _bar(pct: float, width: int = 28) -> str:
    filled = int(width * min(pct, 100) / 100)
    return "█" * filled + "░" * (width - filled)


def cmd_dashboard() -> int:
    conn = db()
    today = datetime.date.today().isoformat()
    sessions = conn.execute(
        "SELECT id, model, cost_usd, context_pct, updated_at FROM sessions "
        "ORDER BY updated_at DESC LIMIT 8").fetchall()
    today_cost = conn.execute(
        "SELECT COALESCE(SUM(cost_usd),0) FROM sessions "
        "WHERE updated_at LIKE ?", (today + "%",)).fetchone()[0]
    today_dups = conn.execute(
        "SELECT COALESCE(SUM(duplicate),0), COALESCE(SUM(est_tokens),0) "
        "FROM tool_calls WHERE ts LIKE ?", (today + "%",)).fetchone()
    top_tools = conn.execute(
        "SELECT tool, COUNT(*), SUM(est_tokens) FROM tool_calls "
        "WHERE ts LIKE ? GROUP BY tool ORDER BY SUM(est_tokens) DESC LIMIT 6",
        (today + "%",)).fetchall()
    recent = conn.execute(
        "SELECT ts, kind, detail FROM events ORDER BY id DESC LIMIT 6").fetchall()
    conn.close()
    cfg = load_config()

    width = 58

    def row(text=""):
        print("│ " + text[:width - 2].ljust(width - 2) + " │")

    def rule(left="├", right="┤"):
        print(left + "─" * width + right)

    rule("╭", "╮")
    row(f"◈ Frugal Tokenomics — Community Edition   profile: {cfg['profile']}")
    rule()
    row(f"TODAY   spend ${today_cost:.2f}   duplicate calls: {today_dups[0]}"
        f"   dup tokens: {today_dups[1]:,}")
    daily = cfg["budgets"].get("daily_usd")
    if daily:
        pct = 100 * today_cost / daily
        row(f"budget  ${today_cost:.2f} / ${daily:.2f}  {_bar(pct)} {pct:.0f}%")
    rule()
    row("RECENT SESSIONS")
    for sid, model, cost, ctx, updated in sessions:
        cost_s = f"${cost:.2f}" if cost is not None else "-"
        ctx_s = f"{ctx:.0f}%" if ctx is not None else "-"
        row(f" {sid[:14]:<16}{(model or '-')[:14]:<16}{cost_s:>8}  ctx {ctx_s}")
    rule()
    row("TOP TOOLS TODAY (est. context tokens)")
    for tool, calls, tokens in top_tools:
        row(f" {tool:<22}{calls:>4} calls {tokens or 0:>12,} tok")
    if recent:
        rule()
        row("RECENT ACTIVITY")
        for ts, kind, detail in recent:
            row(f" {kind}: {detail}")
    rule("╰", "╯")
    return 0


def cmd_report(period: str) -> int:
    conn = db()
    if period == "month":
        prefix = datetime.date.today().strftime("%Y-%m")
    else:
        prefix = datetime.date.today().isoformat()
    cost, sessions = conn.execute(
        "SELECT COALESCE(SUM(cost_usd),0), COUNT(*) FROM sessions "
        "WHERE updated_at LIKE ?", (prefix + "%",)).fetchone()
    dups, dup_tokens = conn.execute(
        "SELECT COALESCE(SUM(duplicate),0), COALESCE(SUM(CASE WHEN duplicate=1 "
        "THEN est_tokens ELSE 0 END),0) FROM tool_calls WHERE ts LIKE ?",
        (prefix + "%",)).fetchone()
    conn.close()
    print(f"FRUGAL REPORT — {prefix}")
    print(f"  sessions               {sessions}")
    print(f"  recorded spend         ${cost:.2f}")
    print(f"  duplicate tool calls   {dups}")
    print(f"  est. duplicated tokens {dup_tokens:,}")
    print("\nLocal only. Nothing leaves this machine.")
    return 0


def cmd_export(fmt: str) -> int:
    conn = db()
    rows = conn.execute(
        "SELECT id, started_at, updated_at, model, cost_usd, context_pct "
        "FROM sessions ORDER BY updated_at DESC").fetchall()
    conn.close()
    cols = ["id", "started_at", "updated_at", "model", "cost_usd", "context_pct"]
    if fmt == "csv":
        buf = io.StringIO()
        writer = csv.writer(buf)
        writer.writerow(cols)
        writer.writerows(rows)
        print(buf.getvalue().rstrip())
    else:
        print(json.dumps([dict(zip(cols, r)) for r in rows], indent=2))
    return 0


# -------------------------------------------------------------- configuration

def cmd_budget(action: str, scope: str, amount) -> int:
    cfg = load_config()
    if action == "show" or not scope:
        print(json.dumps(cfg["budgets"], indent=2))
        return 0
    key = f"{scope}_usd"
    if key not in cfg["budgets"]:
        print(f"error: scope must be task|session|daily", file=sys.stderr)
        return 2
    cfg["budgets"][key] = float(amount) if amount else None
    save_config(cfg)
    print(f"budget {scope} = {cfg['budgets'][key]}")
    return 0


def cmd_profile(action: str, name: str) -> int:
    cfg = load_config()
    if action == "show" or not name:
        print(cfg["profile"])
        return 0
    if name not in PROFILES:
        print(f"error: profile must be one of {PROFILES}", file=sys.stderr)
        return 2
    cfg["profile"] = name
    save_config(cfg)
    print(f"profile = {name}"
          + (" (observe-only)" if name == "shadow" else ""))
    return 0


def cmd_safe() -> int:
    cfg = load_config()
    cfg["profile"] = "shadow"
    save_config(cfg)
    print("SAFE MODE: profile set to shadow — measurement and budget warnings "
          "only; no intervention. Claude Code behavior is unchanged.")
    return 0


# --------------------------------------------------------------------- doctor

def cmd_doctor() -> int:
    checks = []

    def check(name, ok, hint=""):
        checks.append((name, ok, hint))

    check("Python 3.8+", sys.version_info >= (3, 8))
    try:
        conn = db()
        conn.execute("SELECT 1")
        conn.close()
        check("SQLite database", True)
    except Exception as exc:
        check("SQLite database", False, str(exc))
    check("data directory writable", os.access(FRUGAL_DIR, os.W_OK)
          if FRUGAL_DIR.exists() else True)
    cfg = load_config()
    check(f"config (profile: {cfg['profile']})", cfg["profile"] in PROFILES)
    plugin_root = Path(__file__).resolve().parent.parent
    check("plugin manifest",
          (plugin_root / ".claude-plugin" / "plugin.json").is_file())
    check("hooks config", (plugin_root / "hooks" / "hooks.json").is_file())
    settings = Path.home() / ".claude" / "settings.json"
    statusline_wired = False
    try:
        statusline_wired = "frugal" in settings.read_text(encoding="utf-8").lower()
    except OSError:
        pass
    check("status line wired", statusline_wired,
          "run: python frugal.py setup-statusline")
    conn = db()
    session_count = conn.execute("SELECT COUNT(*) FROM sessions").fetchone()[0]
    conn.close()
    check(f"telemetry ({session_count} sessions recorded)", session_count > 0,
          "starts after first Claude Code session with the plugin enabled")

    print("FRUGAL DOCTOR")
    print("=" * 44)
    passed = 0
    for name, ok, hint in checks:
        mark = "✓" if ok else "⚠"
        passed += ok
        line = f"{mark} {name}"
        if not ok and hint:
            line += f"  → {hint}"
        print(line)
    score = int(100 * passed / len(checks))
    print("-" * 44)
    print(f"System Health  {score} / 100")
    return 0 if score >= 70 else 1


def cmd_setup_statusline() -> int:
    script = Path(__file__).resolve()
    print("Add this to ~/.claude/settings.json to enable the Frugal status line:\n")
    print(json.dumps({"statusLine": {
        "type": "command",
        "command": f'python "{script}" statusline'}}, indent=2))
    return 0


# ------------------------------------------------------------------------ cli

def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=[
        "ingest-hook", "statusline", "stats", "dashboard", "report", "export",
        "budget", "profile", "checkpoint", "checkpoints", "doctor", "safe",
        "setup-statusline"])
    parser.add_argument("args", nargs="*")
    parser.add_argument("--csv", action="store_true")
    args = parser.parse_args()
    extra = args.args

    if args.command == "ingest-hook":
        return cmd_ingest_hook()
    if args.command == "statusline":
        return cmd_statusline()
    if args.command == "stats":
        return cmd_stats()
    if args.command == "dashboard":
        return cmd_dashboard()
    if args.command == "report":
        return cmd_report(extra[0] if extra else "today")
    if args.command == "export":
        return cmd_export("csv" if args.csv else "json")
    if args.command == "budget":
        return cmd_budget(extra[0] if extra else "show",
                          extra[1] if len(extra) > 1 else None,
                          extra[2] if len(extra) > 2 else None)
    if args.command == "profile":
        return cmd_profile(extra[0] if extra else "show",
                           extra[1] if len(extra) > 1 else None)
    if args.command == "checkpoint":
        return cmd_checkpoint(" ".join(extra))
    if args.command == "checkpoints":
        return cmd_checkpoints()
    if args.command == "doctor":
        return cmd_doctor()
    if args.command == "safe":
        return cmd_safe()
    if args.command == "setup-statusline":
        return cmd_setup_statusline()
    return 2


if __name__ == "__main__":
    sys.exit(main())
