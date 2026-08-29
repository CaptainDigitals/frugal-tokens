# Tool Output Firewall

Raw tool output never enters context untriaged. The pipeline:

```text
Output → Deduplicate → Noise filter → Error extraction → Semantic grouping → Loss check → Context
```

## Universal Retention Rules

From any verbose command, retain:

- first relevant error and last relevant error
- every *unique* error category (collapse duplicates with a count)
- stack frames that touch project code (drop vendored/framework frames)
- failed test names
- summary statistics (X passed, Y failed, Z skipped)
- exit status

Drop:

- progress bars, spinners, download logs
- passing-test lines
- repeated identical stack frames ("...same frame ×47")
- deprecation warnings unrelated to the task
- timestamps and log prefixes that carry no signal

## Never Blindly Truncate

Wrong: `head -80` / "keep first 80 lines". The error is on line 4,317.

Right: extract semantically, then if the full output might be needed later:

```text
full output stored:  .frugal/artifacts/<hash>.log
context receives:    summary + relevant excerpts + the path
```

Re-open the stored file only if diagnosis demands it.

## Deduplication by Content

If the same failure appears again (same build error, same stack trace), do not
re-ingest it. State: "same failure as before — no new diagnostic information" and
reference the earlier occurrence.

## Per-Tool Recipes

### Test runners (pytest, jest, go test, cargo test, gradle test)

Keep: failed test names, assertion messages, project-code frames, summary line.
Drop: passed tests, setup/teardown logs, collection output.
Flags that help: `pytest -x -q --tb=short`, `jest --silent --onlyFailures`,
`go test -run <Failing>`, `cargo test <name> -- --nocapture` only when needed.

### Package managers (npm, pnpm, yarn, pip, cargo)

Keep: final status, error blocks, peer-dependency conflicts.
Drop: everything else — resolution trees, progress, funding notices.
Prefer `--silent` / `--quiet` flags and pipe through a filter when available.

### Compilers / type checkers (tsc, rustc, javac, go build)

Keep: unique diagnostics with file:line, error count.
Drop: repeated instances of the same diagnostic across files (count them instead).
Flags: `tsc --noEmit --pretty false`, address errors in dependency order — the
first error often causes the next 40.

### Build tools (webpack, vite, docker, maven, gradle, terraform)

Keep: FAILED/ERROR blocks, the failing step, root-cause candidate.
Drop: successful layer/step output, download progress.
Docker: `--progress=quiet`; Terraform: read the plan summary, not every resource.

### Git

- `git diff --stat` first; full diff only for files you're reasoning about.
- `git log --oneline -20`, never unbounded log.
- Never `git diff` against generated/lock files — exclude them
  (`git diff -- . ':!*.lock' ':!dist'`).

### Long-running logs (kubernetes, server logs)

Grep for the window around the failure; never tail thousands of lines into
context. `kubectl logs --tail=100` + grep, expand only on evidence.

## Pre-Filter at the Source

Cheapest filtering happens before capture. Prefer quiet flags, targeted test
selection, `--only-failures` modes, and piping through `grep`/`Select-String`
over ingesting everything and summarizing after.
