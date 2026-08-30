---
description: Create a semantic checkpoint of the current engineering state before compaction
---

Create a Frugal semantic checkpoint:

1. Run `python "${CLAUDE_PLUGIN_ROOT}/bin/frugal.py" checkpoint "$ARGUMENTS"`
   to create the checkpoint file.
2. Open the created file and FILL IT IN from the current conversation:
   objective (verbatim user intent), current state, decisions and why,
   modified files, unresolved errors (exact text, not paraphrased),
   constraints, test status, ordered next steps.
3. Confirm to the user that the checkpoint is complete enough that a fresh
   session could resume the work from it alone.

A checkpoint template that stays a template protects nothing — step 2 is the
point.
