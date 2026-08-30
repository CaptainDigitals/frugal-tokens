# Frugal Tokens — Claude Code Plugin (Community Edition)

Local-first AI FinOps + Context Intelligence for Claude Code. Free, open
source, no account, no cloud. Full product spec:
[Community Edition PRD](../docs/Frugal_Community_Edition_PRD.md).

## What the plugin adds

| Component | What it does |
|---|---|
| **Hooks** | Every tool call is fingerprinted into a local SQLite ledger (`~/.frugal/frugal.db`); identical repeated calls are flagged as duplicates; `PreCompact` writes an automatic semantic checkpoint before Claude compacts |
| **Status line** | `◈ FRUGAL │ CTX 43% │ $1.42 │ dup 3 │ SHADOW ✓` — context %, session cost, duplicate count, profile, budget health (`✓`/`!`/`X`) |
| **Commands** | `/frugal-tokens:setup`, `:stats`, `:dashboard`, `:doctor`, `:checkpoint`, `:budget`, `:report`, `:why`, `:safe` |
| **Skill** | The full frugal-tokens discipline (context tiering, cheap-first navigation, output firewall, model routing, provider framework) is bundled under `skills/` |
| **Runtime** | `bin/frugal.py` — stdlib-only Python, the v0.1 reference implementation of Frugal Core (the Rust workspace is the v1.0 target per the PRD) |

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

## CLI

The runtime also works standalone:

```bash
python bin/frugal.py stats        # current session economics
python bin/frugal.py dashboard    # terminal dashboard
python bin/frugal.py report month
python bin/frugal.py budget set session 10
python bin/frugal.py checkpoint "before refactor"
python bin/frugal.py doctor
python bin/frugal.py safe         # observe-only, zero intervention
```

## Guarantees

- **Fail-open** (PRD §85): every hook and status-line entry point swallows its
  own errors — a Frugal failure can never block Claude Code.
- **Local-only** (PRD §73): all telemetry lives in `~/.frugal/`. No accounts,
  no uploads, no external services.
- **No raw secrets** (PRD §75): the ledger stores tool names, fingerprints
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
