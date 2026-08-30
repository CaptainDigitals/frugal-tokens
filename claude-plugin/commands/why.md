---
description: Explain recent Frugal observations — duplicates, checkpoints, budget events
---

Answer "why" questions about Frugal's recent observations. Query the local
database directly (read-only):

```bash
python -c "import sqlite3,os;from pathlib import Path;db=sqlite3.connect(Path.home()/'.frugal'/'frugal.db');[print(*r) for r in db.execute('SELECT ts,kind,detail FROM events ORDER BY id DESC LIMIT 15')]"
```

Then explain each relevant event in plain language: what was observed, why it
matters economically, and what (if anything) the user should change. Frugal
Community observes and explains — it does not silently intervene, so every
event shown is a recorded observation, not a modification.
