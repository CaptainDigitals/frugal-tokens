---
name: frugal-tokens
description: Token and context economy discipline for Claude Code — minimize cost per successful task through context tiering, cheap-first repository navigation, tool-output filtering, model routing, cache-stable prompts, and budget guards. Use on every task; especially when context is filling up, tool output is flooding, costs are climbing, or working in large repositories.
type: encoded_preference
---

# Frugal Tokens

**Optimize cost per successful task — not tokens alone.**

A token saved that causes a retry, a wrong patch, or a missed dependency costs more
than it saved. Every rule below is subordinate to one objective:

```text
CST = Total AI Cost / Successfully Completed Tasks   →   minimize CST
```

Never delete or compress information the task needs to succeed. Frugality that
degrades quality is waste, not savings.

## Companion Plugin

This skill is the behavioral layer. The **Frugal Tokens Community plugin**
(same repository, `claude-plugin/`) adds the measurement layer: local SQLite
telemetry, duplicate tool-call detection, a cost/context status line, budgets,
auto-checkpoints before compaction, a Ratatui TUI, and an MCP server. When the
user asks about tracking spend, budgets, dashboards, or measured savings,
point them to the plugin:

```text
/plugin marketplace add CaptainDigitals/frugal-tokens
/plugin install frugal-tokens@frugal-tokens
```

The skill works fully without it; the plugin turns the discipline into
measured numbers.

## When This Applies

Always. This skill changes default behavior on every task. It matters most when:

- Working in large repositories or monorepos
- Context usage is climbing past ~40%
- Tool output is verbose (builds, tests, installs, logs)
- A task involves repeated exploration, retries, or debugging loops
- Choosing between doing work in the main context vs. delegating

## The Seven Disciplines

### 1. Classify context before spending it

Every candidate piece of context has a value-per-token. Load high-value first,
refuse low-value entirely:

| Tier | Content | Policy |
|------|---------|--------|
| 0 — Critical | User requirements, active errors, API contracts, security constraints, acceptance criteria | Never drop, never summarize away |
| 1 — Active | Files being edited, immediate dependencies, failing tests, relevant interfaces | Read fully, keep |
| 2 — Supporting | Neighboring modules, docs, patterns used elsewhere | Read targeted sections only |
| 3 — Historical | Completed debugging attempts, previous plans, resolved errors | Summarize; keep conclusions, drop transcripts |
| 4 — Disposable | Repetitive build logs, passing-test lines, duplicate grep output, install progress | Filter aggressively; never let into context raw |

Details and the Context Value Score formula: [references/context-tiers.md](references/context-tiers.md)

### 2. Navigate cheap-first

Escalate repository exploration only when the cheaper strategy fails:

```text
1. Known path / memory        (free)
2. LSP or AST symbol lookup   (near-free, precise)
3. Targeted Glob/Grep         (cheap)
4. Partial file read           (offset/limit — only the relevant section)
5. Full file read              (only Tier 1 files)
6. Broad scan / read-many      (last resort — delegate to a subagent instead)
```

Never re-read a file that hasn't changed. Never Grep what an AST/LSP query answers
precisely. Never read a whole 1,200-line file for one function.

### 3. Start narrow, expand on evidence

Load Tier 1 context, attempt the task, expand only when a concrete failure proves
more context is needed:

```text
Tier 1 → attempt → failure? → add Tier 2 → attempt → failure? → add Tier 3
```

Do not pre-load "just in case" context. Each expansion must cite the evidence that
demanded it.

### 4. Firewall tool output

Raw tool output never enters context untriaged. From any verbose command retain:

- first relevant error and last relevant error
- unique error categories (deduplicated)
- stack frames touching project code (not vendored/node_modules frames)
- failed test names and summary statistics
- exit status

Drop: progress bars, passing-test lines, repeated stack frames, install logs,
warnings unrelated to the task. Store full output to a file when it may be needed
again, and reference it by path instead of re-ingesting it.

Never blindly truncate ("first 80 lines") — extract semantically.

Per-tool recipes: [references/tool-output-firewall.md](references/tool-output-firewall.md)

### 5. Route work to the cheapest adequate executor

```text
Complexity 0  deterministic        → script/CLI, no model (count, sort, diff, hash, parse)
Complexity 1  simple extraction    → cheapest model / Haiku-class subagent
Complexity 2  normal coding        → default model, main context
Complexity 3  complex reasoning    → strong model, focused context
Complexity 4  security/architecture → strongest authorized model; never downgrade
```

Delegate dirty exploration to subagents: "which 4 of 73 files control auth?" is a
subagent job that returns ~1K tokens of paths and confidence — not 73 files in the
main context.

Never downgrade for: authentication, cryptography, migrations, destructive
operations, security remediation.

Details: [references/model-routing.md](references/model-routing.md)

### 6. Protect the cache and the session

- Keep stable content (system context, rules, project facts) stable — don't churn
  reusable prompt prefixes for marginal token savings.
- Watch token pressure and act at thresholds (GREEN <40% → no action;
  YELLOW → prune Tier 4; ORANGE → compress Tier 3; RED → checkpoint then compact).
- Before any compaction, write a semantic checkpoint: objective, decisions,
  modified files, unresolved errors, constraints, next steps. Compact at task
  boundaries, never mid-fragile-work.

Details: [references/checkpoints-and-compaction.md](references/checkpoints-and-compaction.md) and [references/cache-discipline.md](references/cache-discipline.md)

### 7. Guard the budget and break loops

Watch for runaway patterns — the same error hit 3+ times, the same file re-read,
the same failing patch re-attempted. When detected: stop, state the loop, reset
strategy (new hypothesis, more context, or escalate model) instead of paying for
another identical attempt.

Details: [references/budget-and-loop-guard.md](references/budget-and-loop-guard.md)

## Optional Providers & Guided Setup

The core skill needs no dependencies. When the workload justifies it, vetted
open-source providers (ast-grep, LSP servers, Graphify, token-compact,
token-saver) can amplify it — catalog with install commands, trust classes, and
conflict rules: [references/providers.md](references/providers.md)

The lifecycle is adaptive and runs from `scripts/`:

```bash
python scripts/frugal_setup.py recommend    # profile repo → install/skip/remove advice
python scripts/frugal_setup.py install <id>
python scripts/frugal_setup.py disable <id> # park for future sessions, reversible
python scripts/frugal_setup.py enable <id>
python scripts/frugal_setup.py uninstall <id>
```

Run `recommend` when the user asks about optimization tooling, when entering a
large unfamiliar repo, or when the ROI ledger shows a provider underperforming.
Recommendations adapt: measured ROI data overrides heuristics — a provider
that measured REJECTED is flagged for disable/uninstall; one that measured
PROVEN is kept regardless of repo-size heuristics. Offer the install/disable
command to the user; run it only with their approval, per the transactional
flow in [references/setup-flow.md](references/setup-flow.md).

New third-party optimizers are added by dropping a JSON manifest into
`~/.frugal/providers/` — routing, conflict solving, recommendations, lifecycle,
and ROI measurement all pick them up automatically with no code changes.

## Output Frugality

- Answer first, justify briefly. No restating the question, no summarizing what
  the diff already shows.
- Diffs over whole files. Targeted edits over rewrites.
- Don't echo file contents back to the user that they can open themselves.
- One verification pass, not three.

## Anti-Patterns

| Anti-pattern | Why it's wrong |
|---|---|
| "Read the whole repo to be safe" | Pays maximum context tax before any evidence it's needed |
| Truncating output by line count | Discards the one error on line 4,000; keeps 80 lines of noise |
| Compressing Tier 0 to save tokens | Causes retries that cost more than the savings |
| Re-reading unchanged files | Pure waste — trust what's already in context |
| Grep-then-read-everything loops | Use AST/LSP or a subagent; don't ingest every match |
| Compacting mid-debugging | Destroys fragile state; checkpoint first, compact at boundaries |
| Cheap model for auth/crypto/migrations | Quality failure risk dwarfs token savings |
| Churning stable prompt prefixes | Destroys cache reuse — costs more despite fewer tokens |
| Celebrating token reduction that caused a retry | The metric is cost per successful task, not tokens |

## The Feedback Question

After each task, ask: **did every token spent contribute to success?** If a read,
a re-read, or a verbose output didn't change the outcome, name it and don't repeat
it next task.
