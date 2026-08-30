---
description: Show or set task/session/daily USD budgets for Claude Code spend
---

If `$ARGUMENTS` is empty, run
`python "${CLAUDE_PLUGIN_ROOT}/bin/frugal.py" budget show` and present it.

If arguments are given (e.g. "session 10" or "set daily 25"), set the budget
via `python "${CLAUDE_PLUGIN_ROOT}/bin/frugal.py" budget set <scope> <usd>`
and confirm. Valid scopes: task, session, daily. Budgets produce status-line
warnings (! at 80%, X when exceeded) — Community Edition warns, it does not
block.
