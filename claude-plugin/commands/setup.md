---
description: First-run setup for Frugal Tokenomics — verify runtime, wire the status line, choose profile and budgets
---

Guide the user through Frugal Tokenomics Community Edition setup. Frugal starts in
SHADOW mode: observe-only, no workflow changes. Never modify configuration
without showing the change and getting approval.

1. **Verify the runtime**: run `python "${CLAUDE_PLUGIN_ROOT}/bin/frugal.py" doctor`
   and show the result.
2. **Wire the status line** (if doctor shows it unwired): run
   `python "${CLAUDE_PLUGIN_ROOT}/bin/frugal.py" setup-statusline`, show the
   user the settings.json snippet, and apply it only with their approval.
3. **Profile**: explain the options (shadow = observe only, conservative,
   balanced, aggressive) and set their choice via
   `python "${CLAUDE_PLUGIN_ROOT}/bin/frugal.py" profile set <name>`.
   Recommend staying in shadow for the first few sessions to build a baseline.
4. **Budgets** (optional): offer task/session/daily USD budgets via
   `python "${CLAUDE_PLUGIN_ROOT}/bin/frugal.py" budget set <scope> <usd>`.
5. Confirm everything with a final `doctor` run and tell the user telemetry
   accumulates locally in ~/.frugal (no cloud, no account).
