# Cache Discipline

Prompt caching makes *stable* context nearly free and *churned* context
expensive. An optimizer that shaves 500 tokens by rewriting a stable prefix can
increase real billed cost by invalidating a 18K-token cached prefix.

## Effective Cost, Not Token Count

```text
effective_cost = uncached_input + cache_write + cache_read + output + reasoning
```

Cache reads are typically ~10× cheaper than uncached input. A 20K-token stable
prefix read from cache costs less than a 5K-token prefix rewritten every turn.

## Prompt Layering

Order content by stability — stable first, volatile last:

```text
[Stable system / rules]         ← never touch mid-session
[Stable project facts]          ← repo structure, conventions, key paths
[Task context]                  ← changes per task
[Dynamic tool results]          ← changes per turn
[Current request]               ← changes per turn
```

Any edit to an early layer invalidates the cache for everything after it.

## Rules

- Don't rewrite, reorder, or "tidy" stable instructions mid-session for marginal
  savings — the cache invalidation costs more.
- Don't toggle skills/MCPs/rules on and off between turns; each toggle churns
  the prefix.
- Batch configuration changes at session boundaries, not mid-task.
- When adding project facts worth reusing, append rather than restructure.

## Fixed Context Tax Audit

Every always-loaded skill, MCP schema, rule file, and agent definition is a
recurring per-request tax. Periodically ask:

- Which loaded skills/MCPs were actually used in the last N sessions?
- Is CLAUDE.md carrying rules that belong in trigger-loaded files?
- Are there duplicate or contradictory rules inflating the prefix?

Prefer progressive disclosure: metadata always loaded, full content loaded only
when relevant (this skill's own references/ directory follows that pattern).

## Trigger-Based Rule Loading

Instead of permanently loading every domain rule file, map triggers to rules:

```yaml
authentication: [rules/security.md, rules/auth.md]
react:          [rules/frontend.md]
migration:      [rules/database.md]
```

Load a rule file when its trigger appears in the task, not by default.
