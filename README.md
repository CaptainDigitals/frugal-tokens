<div align="center">

<img src="assets/frugal_tokens_circuit_flow_animated.gif" alt="Frugal Tokens" width="480" />

# Frugal Tokens

**Token & context economy discipline for Claude Code.**

*Less context. Fewer wasted calls. Lower AI spend. Same engineering quality.*

[![Skill](https://img.shields.io/badge/Claude_Code-Skill-d97757?style=flat-square)](SKILL.md)
[![License: MIT](https://img.shields.io/badge/License-MIT-2ea44f?style=flat-square)](LICENSE)
[![PRD](https://img.shields.io/badge/Docs-Full_PRD-4a7bd0?style=flat-square)](docs/Frugal_Tokens_PRD.md)

</div>

---

## The Core Idea

Most token "optimizers" chase raw token reduction. That metric is broken: a
compression that saves 50% of tokens but causes one extra debugging cycle costs
*more* than the unoptimized workflow.

Frugal Tokens optimizes a single objective instead:

```text
CST  =  Total AI Cost / Successfully Completed Tasks      →  minimize
```

Every rule in the skill is subordinate to it. Information the task needs to
succeed is never dropped — frugality that degrades quality is waste, not savings.

## What It Delivers

<div align="center">
<img src="assets/What_FT_Delivers.png" alt="What Frugal Tokens delivers" width="820" />
</div>

The skill installs seven always-on disciplines:

| # | Discipline | What Claude does differently |
|---|---|---|
| 1 | **Context tiering** | Classifies every candidate context by value-per-token (Tier 0 critical → Tier 4 disposable) and refuses low-value loads |
| 2 | **Cheap-first navigation** | LSP/AST → targeted grep → partial read → full read; broad scans become the last resort |
| 3 | **Progressive expansion** | Starts with the minimum context and expands only on concrete evidence of failure |
| 4 | **Tool output firewall** | Extracts errors, failed tests, and project stack frames; never ingests raw build logs or blindly truncates |
| 5 | **Model & delegation routing** | Deterministic work → scripts; exploration → subagents; strongest model reserved for security/architecture |
| 6 | **Cache & session protection** | Keeps stable prompt prefixes stable, checkpoints before compaction, compacts only at task boundaries |
| 7 | **Budget & loop guard** | Detects repeated-error loops and resets strategy instead of paying for identical retries |

## How It Works

<div align="center">
<img src="assets/How_it_Works.png" alt="How Frugal Tokens works" width="820" />
</div>

The skill uses **progressive disclosure** — it practices what it preaches:

```text
frugal-tokens/
├── SKILL.md                              ← always loaded: the 7 disciplines (compact)
├── references/                           ← loaded only when the situation demands it
│   ├── context-tiers.md                  ← CVS formula, tier policies, diff-first context
│   ├── tool-output-firewall.md           ← per-tool filtering recipes (pytest, tsc, docker…)
│   ├── model-routing.md                  ← complexity ladder, subagent delegation rules
│   ├── checkpoints-and-compaction.md     ← token pressure levels, semantic checkpoints
│   ├── cache-discipline.md               ← prompt layering, fixed context tax audit
│   └── budget-and-loop-guard.md          ← loop breaker, escalation budgets
├── scripts/
│   └── estimate_tokens.py                ← cost-preview a file/dir before reading it
└── docs/
    └── Frugal_Tokens_PRD.md              ← the full enterprise PRD this skill distills
```

Only the compact `SKILL.md` occupies context by default. Reference files load
when their trigger appears — a failing build pulls the firewall recipes, rising
context pressure pulls the checkpoint protocol.

## Install

Clone into your Claude Code skills directory:

```bash
git clone https://github.com/CaptainDigitals/frugal-tokens.git ~/.claude/skills/frugal-tokens
```

Windows (PowerShell):

```powershell
git clone https://github.com/CaptainDigitals/frugal-tokens.git "$env:USERPROFILE\.claude\skills\frugal-tokens"
```

Or install for a single project instead:

```bash
git clone https://github.com/CaptainDigitals/frugal-tokens.git .claude/skills/frugal-tokens
```

That's it — the skill is discovered automatically and applies to every session.

## How To Use

The skill is an **encoded preference**: it changes Claude's default behavior on
every task with no invocation needed. You'll notice it as:

- **Targeted reads** — partial file reads and symbol lookups instead of whole-repo ingestion
- **Summarized tool output** — "3 unique errors, 2 modules, root cause candidate" instead of 10,000 log lines
- **Delegated exploration** — "which files control auth?" runs in a subagent and returns paths, not file dumps
- **Checkpoints before compaction** — engineering state survives context resets
- **Loop breaks** — "this is the 4th identical error; resetting strategy" instead of a 5th retry

You can also lean on it explicitly:

```text
"Estimate the token cost of loading src/ before you read anything."
"We're at 70% context — apply the frugal-tokens pressure protocol."
"Checkpoint the session state before compacting."
```

Cost-preview a directory before letting any of it into context:

```bash
python scripts/estimate_tokens.py src/ --top 10
```

```text
 est. tokens  file
      18,742  src/generated/api-client.ts
       9,310  src/auth/session.ts
       ...
----------------------------------------
     104,271  TOTAL across 87 files
```

## What This Is (and Isn't)

This repository is the **skill distillation** of the full
[Frugal Tokens Enterprise PRD](docs/Frugal_Tokens_PRD.md) — an adaptive token &
context optimization platform (context intelligence engine, model economics
router, quality gates, FinOps dashboard, provider marketplace).

The skill captures everything from that design that Claude Code can enforce
**today, behaviorally, with zero infrastructure**. The PRD documents the full
platform vision for readers who want the roadmap.

It is **not** an output truncator, a proxy, or a guaranteed-percentage discount.
It never deletes important context to save money.

## Anti-Patterns It Exists to Kill

- Reading the whole repo "to be safe"
- Truncating logs by line count (the error was on line 4,317)
- Re-reading unchanged files
- Compacting mid-debugging and losing fragile state
- Cheap-model routing for auth, crypto, and migrations
- Celebrating a 50% token cut that caused a 20% retry increase

## License

[MIT](LICENSE) © CaptainDigitals
