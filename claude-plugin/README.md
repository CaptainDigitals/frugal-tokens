# Frugal Tokens — Claude Code Plugin (Community Edition)

Local-first AI FinOps + Context Intelligence for Claude Code. Free, open
source, no account, no cloud. See the
[Product Overview](../docs/OVERVIEW.md).

## What the plugin adds

| Component | What it does |
|---|---|
| **Hooks** | Every tool call is fingerprinted into a local SQLite ledger (`~/.frugal/frugal.db`); identical repeated calls are flagged as duplicates; `PreCompact` writes an automatic semantic checkpoint before Claude compacts |
| **Status line** | `◈ FRUGAL │ CTX 43% │ $1.42 │ dup 3 │ SHADOW ✓` — context %, session cost, duplicate count, profile, budget health (`✓`/`!`/`X`) |
| **Commands** | `/frugal-tokens:setup`, `:stats`, `:dashboard`, `:doctor`, `:checkpoint`, `:budget`, `:report`, `:why`, `:safe` |
| **Skill** | The full frugal-tokens discipline (context tiering, cheap-first navigation, output firewall, model routing, provider framework) is bundled under `skills/` |
| **Runtime** | Two interchangeable implementations sharing one ledger: `bin/frugal.py` (stdlib Python, zero build) and the **Rust core** (`frugal` binary from `crates/` — adds the Ratatui TUI and the MCP server) |
| **MCP server** | `frugal mcp` — 11 tools (`frugal_get_stats`, `frugal_get_cost`, `frugal_get_waste`, `frugal_checkpoint`, `frugal_set_budget`, ...) giving Claude structured access to Frugal Core |
| **TUI** | `frugal` / `frugal dashboard` — live Ratatui dashboard: Overview, Tools, Budget, Providers, Checkpoints, Audit |

## Install

```bash
/plugin marketplace add CaptainDigitals/frugal-tokens
/plugin install frugal-tokens@frugal-tokens
```

Then inside Claude Code:

```text
/frugal-tokens:setup
```

Setup verifies the runtime, offers to wire the status line, and lets you pick
a profile and budgets. **Default profile is SHADOW: observe-only.** Frugal
measures and reports; it changes nothing until you opt in.

## Rust Core (TUI + MCP)

Build the native runtime from the repo root (or grab a
[release binary](https://github.com/CaptainDigitals/frugal-tokens/releases)):

```bash
cargo build --release        # -> target/release/frugal
```

```bash
frugal                       # interactive Ratatui dashboard (Overview/Tools/
                             # Budget/Providers/Checkpoints/Audit; ←/→, r, ?, q)
frugal stats                 # session economics + health score
frugal mcp                   # MCP server over stdio (11 frugal_* tools)
frugal report month
frugal budget set session 10
frugal checkpoint "before refactor"
frugal providers             # registry + install status
frugal doctor
frugal safe                  # observe-only, zero intervention
```

The binary is a drop-in superset of the Python runtime — same
`~/.frugal/frugal.db`, same `config.json`, same hook/statusline stdin
contracts. With `frugal` on PATH, the plugin's `.mcp.json` registers the MCP
server automatically; without it, Claude Code simply shows the server as
unavailable (fail-open).

## Python CLI (no build required)

```bash
python bin/frugal.py stats        # current session economics
python bin/frugal.py dashboard    # terminal dashboard snapshot
python bin/frugal.py report month
python bin/frugal.py budget set session 10
python bin/frugal.py checkpoint "before refactor"
python bin/frugal.py doctor
python bin/frugal.py safe         # observe-only, zero intervention
```

## Guarantees

- **Fail-open**: every hook and status-line entry point swallows its
  own errors — a Frugal failure can never block Claude Code.
- **Local-only**: all telemetry lives in `~/.frugal/`. No accounts,
  no uploads, no external services.
- **No raw secrets**: the ledger stores tool names, fingerprints
  (SHA-256), token estimates, and costs — not source code or credentials.
- **Transparent**: `/frugal-tokens:why` explains every recorded observation.

## Data

```text
~/.frugal/
├── frugal.db          SQLite: sessions, tool_calls, events, checkpoints
├── config.json        profile + budgets
└── checkpoints/       semantic checkpoint files
```

Override the location with the `FRUGAL_DIR` environment variable.
