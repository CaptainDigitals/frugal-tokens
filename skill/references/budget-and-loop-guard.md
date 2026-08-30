# Budget Governor & Loop Guard

Runaway loops are the single largest avoidable spend. A model retrying the same
failed patch five times pays five times for zero progress.

## Loop Detection Signals

Treat any of these as a tripped breaker:

- same error encountered **3+ times** across attempts
- same file re-read without it having changed
- same command re-run expecting a different result
- same patch shape re-attempted after failing
- exploration revisiting paths already ruled out
- alternating between two states (fix A breaks B, fix B breaks A)

## When the Breaker Trips

Stop paying for identical attempts. Explicitly:

1. **Name the loop.** "This is the 4th occurrence of the same TypeScript error
   across 3 patch attempts."
2. **Reset strategy** — pick one:
   - form a *new hypothesis* (the current one is falsified)
   - *expand context* one tier (the failure is evidence — see context-tiers.md)
   - *escalate model/effort* with the failed attempts as input
   - *instrument instead of guess* (add a log/test that discriminates hypotheses)
   - *ask the user* if the blocker is genuinely theirs (missing credentials,
     ambiguous requirement)
3. Never silently retry attempt #5 of the same thing.

## Budget Awareness

Hold a mental budget per task proportional to its value:

- A one-line CSS fix does not justify reading 40 files.
- Escalating to the strongest model for a rename is waste.
- Three verification passes on a doc typo is waste.

When a task's spend feels disproportionate to its size, that *is* the signal —
step back and check for a loop or a wrong approach before continuing.

## Context Escalation Budget

Expansions are bounded and evidence-gated:

```text
initial context:  narrow (Tier 1)
expansion 1:      +Tier 2, cites the failure that demanded it
expansion 2:      +Tier 3 / broad search, cites new evidence
beyond that:      re-plan — the task framing is probably wrong
```

If two expansions haven't produced progress, the problem is the hypothesis, not
the context volume.

## Anomaly Check

If a session's spend pattern changes sharply (suddenly reading whole directories,
output ballooning, repeated tool failures), pause and diagnose the cause —
runaway agent behavior, a malformed tool, or log flooding — before continuing.
