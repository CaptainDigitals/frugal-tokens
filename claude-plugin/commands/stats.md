---
description: Show current session cost, context, tool-call and duplicate-read statistics
---

Run `python "${CLAUDE_PLUGIN_ROOT}/bin/frugal.py" stats` and present the
output to the user. If duplicate tool calls are flagged, briefly explain which
tools were repeated with identical input and what to do about it (trust
context already loaded; don't re-read unchanged files). Do not editorialize
beyond what the data shows.
