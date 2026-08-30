# Checkpoints & Loss-Aware Compaction

Compaction without a checkpoint is amnesia. Compaction at the wrong moment
destroys fragile work. This file defines when and how.

## Token Pressure Levels

| Level | Context used | Action |
|---|---|---|
| GREEN | < 40% | No intervention |
| YELLOW | 40–60% | Prune Tier 4 (disposable tool output, duplicate results) |
| ORANGE | 60–75% | Compress Tier 3 (historical) to conclusions |
| RED | 75–90% | Write semantic checkpoint; compact at the next task boundary |
| CRITICAL | > 90% | Checkpoint immediately; force recovery |

Avoid starting large-scale refactoring or multi-file features in the last 20% of
the window — finish the current unit, checkpoint, then continue fresh.

## Compact at Boundaries, Never Mid-Fragile-Work

Good moments: after a fix is verified, after tests pass, after a plan is agreed,
after a subtask completes.

Bad moments: mid-debugging with an unresolved hypothesis, mid-refactor with the
build broken, between writing a failing test and making it pass.

If pressure forces action mid-work: checkpoint first, note "build currently
broken because X, next step Y", then compact.

## Semantic Checkpoint Format

Before any compaction, capture engineering state (not conversation history):

```yaml
objective:            # what we're trying to accomplish (verbatim user intent)
current_state:        # where things stand right now
decisions:            # choices made and WHY (these are expensive to re-derive)
modified_files:       # paths + one-line what-changed
unresolved_errors:    # exact error text, not paraphrase
constraints:          # Tier 0 — API contracts, security rules, user requirements
test_status:          # what passes, what fails
next_steps:           # ordered, specific
```

Write it to a file (e.g. `.frugal/checkpoints/<timestamp>.md` or the session
notes location the project uses) so it survives beyond the context window.

## What Compaction May Discard

- exploration transcripts (how files were found — keep *which* files)
- resolved error output
- superseded plans (keep the final decision)
- tool output already summarized

## What Compaction Must Preserve

- everything in Tier 0 (verbatim)
- the checkpoint itself
- unresolved errors (exact text — paraphrased errors can't be grepped)
- decision rationale (re-deriving "why we chose approach B" costs a full
  re-exploration)

## Retention Verification

After compaction, re-read the checkpoint and confirm: could a fresh session
resume this work from the checkpoint alone? If a critical fact is missing, the
compaction lost too much — restore it from the checkpoint file.
