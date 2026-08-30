---
description: Historical Frugal report — today or month rollup of spend and duplicate waste
---

Run `python "${CLAUDE_PLUGIN_ROOT}/bin/frugal.py" report $ARGUMENTS`
(defaults to today; "month" for the monthly rollup) and present the output.
For exports, `python "${CLAUDE_PLUGIN_ROOT}/bin/frugal.py" export` emits JSON
(`--csv` for CSV). All data is local-only.
