# Context Tiers & Context Value Score

## Context Value Score (CVS)

Before adding anything to context, estimate its value-per-token:

```text
CVS = (relevance × dependency_importance × uniqueness × task_criticality × freshness) / token_cost
```

You don't need to compute this numerically — use it as a mental model. Calibration
examples:

| Context | CVS |
|---|---:|
| Function actively being modified | .99 |
| Interface it implements | .94 |
| Its direct test file | .91 |
| Imported dependency it calls | .85 |
| Architecture rule relevant to the task | .82 |
| Similar neighboring component | .56 |
| Old build output | .21 |
| node_modules / vendored content | .01 |

Rule of thumb: if you can't say *which decision* a piece of context will inform,
its CVS is too low to load.

## Tier Policies in Detail

### Tier 0 — Immutable Critical

Explicit user requirements, security constraints, API contracts, acceptance
criteria, active errors, active implementation decisions.

- Never removed, never lossy-summarized.
- When compacting, these are copied verbatim into the checkpoint.
- If a compression step would touch Tier 0, reject the compression.

### Tier 1 — Active Working Context

Files being edited, immediate dependencies, failing tests, relevant interfaces.

- Read fully. Keep in context for the duration of the task.
- On change, prefer the diff over re-reading the whole file.

### Tier 2 — Supporting Context

Neighboring modules, documentation, patterns used elsewhere in the repo.

- Load on demand, targeted sections only (offset/limit reads, single symbols).
- Loaded only when a Tier-1-only attempt failed or is clearly insufficient.

### Tier 3 — Historical Context

Completed debugging attempts, superseded plans, resolved errors.

- Summarize to conclusions: what was tried, what was ruled out, what was decided.
- The transcript of *how* is disposable; the *outcome* is not.

### Tier 4 — Disposable

Repetitive build logs, passing-test output, duplicate search results, install
progress.

- Filtered before entering context (see tool-output-firewall.md).
- If already in context, first target for pruning at YELLOW pressure.

## Information Retention Check

Any time you compress or summarize, verify: every Tier 0/1 fact present before is
present after. If a critical fact would be lost, the compression is rejected —
regardless of the token savings.

```text
Critical facts before: 28 → retained: 28  → Retention: 100%  → OK
Noncritical retained: 51%, token reduction: 63%              → good trade
```

## Diff-First Context

For a file already analyzed in this session:

```text
send: previous understanding + git diff
not:  the entire file again
```

Only fall back to a full re-read when the diff is too tangled to reason about.

## Blast-Radius Scoping

When file X changes, the context that matters is:

```text
X → its direct callers → their tests
```

Not X's whole package, not the whole layer. Use LSP references / AST queries to
compute the blast radius instead of guessing wide.
