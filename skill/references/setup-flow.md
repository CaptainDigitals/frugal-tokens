# Guided Provider Setup Flow

Follow this flow when the user asks to "set up Frugal Tokenomics", "run frugal
setup", "install the recommended optimizers", or similar. It is the skill-level
implementation of the PRD's installation journey: assess → baseline → recommend
→ transactional apply → verify, with rollback at every step.

**Prime directive: never silently modify the user's Claude configuration.**
Every change is proposed, approved, backed up, applied, and verified — in that
order.

## Stage 0 — Assess (read-only)

Inventory the environment. Modify nothing.

Collect:

- OS, Claude Code version (`claude --version`)
- project language(s), repo size (`git ls-files | wc -l`, LOC estimate via
  `scripts/estimate_tokens.py .`)
- monorepo or single package
- installed skills (`ls ~/.claude/skills/` and `.claude/skills/`)
- installed MCPs (`.mcp.json`, `~/.claude.json` mcpServers)
- CLAUDE.md size (user + project) — flag if > ~500 lines combined
- existing hooks (`settings.json` hooks blocks)
- available navigation tools — one command does it:
  `python scripts/frugal_providers.py status`

Present the inventory to the user before proceeding.

## Stage 1 — Baseline

Record 3–10 normal tasks into the ROI ledger before enabling anything:

```bash
python scripts/frugal_roi.py record --phase baseline --task "<desc>" \
    --input-tokens N --output-tokens N --cache-read N --cost X.XX \
    --retries N --success
```

Pull the numbers from /cost, statusline tracking, or OTEL telemetry. If no
usage data exists, tell the user recommendations will be heuristic rather than
measured. Do not fake a baseline.

## Stage 2 — Waste Map

From the assessment, rank likely waste sources, e.g.:

```text
Repository re-exploration     large repo, no AST/graph tooling
Fixed context tax             N always-loaded skills, M MCP schemas
Oversized CLAUDE.md           1,200 lines, duplicated rules
Tool output flooding          verbose test/build commands in hooks
```

## Stage 3 — Recommend

Run the adaptive recommender first — it profiles the repo and overlays any
measured ROI data:

```bash
python scripts/frugal_setup.py recommend
```

Then sanity-check its output against the waste map (see providers.md),
respecting workload fit:

| Signal | Recommendation |
|---|---|
| Large/monorepo, repeated exploration | ast-grep + LSP; Graphify if revisited across many sessions |
| Big docs/specs loaded often | token-compact (with retention checks) |
| No cost visibility | token-saver's billing-audit approach |
| Small repo, few sessions | **nothing** — core skill alone; say so explicitly |
| Oversized CLAUDE.md | modularization + trigger-based loading (cache-discipline.md) |

Present as a recommendation list with a one-line reason and expected benefit
each — including what is *not* recommended and why. Wait for the user to
approve specific items. Never install unapproved providers.

## Stage 4 — Transactional Apply

For each approved change:

```text
SCAN      → confirm current state of files to be touched
VALIDATE  → run the conflict solver on the proposed active set:
            python scripts/frugal_conflicts.py --enable <ids...>
            (exit 0 required — drops or unsatisfied requirements block APPLY)
BACKUP    → copy any file to be modified to ~/.frugal/backups/<timestamp>/
PLAN      → state exactly what will change (paths, commands)
APPLY     → one provider at a time, never batched
VERIFY    → provider responds (version check / trivial query); Claude Code
            still starts; no skill/MCP conflicts introduced
COMMIT    → record what was installed and why
```

On any verification failure: restore from backup immediately and report what
failed. A partially applied setup is worse than none.

## Stage 5 — Verify Economics

After 3–10 tasks with the new provider(s), record them as
`--phase optimized --providers <ids>` and run:

```bash
python scripts/frugal_roi.py report
```

The report applies the acceptance rule (cost improvement ≥ 10%, retry increase
≤ 5%, quality loss ≤ 3%) and gives per-provider CST. Keep providers whose
verdict is ACCEPTED or EXCELLENT; recommend removing the rest. A provider is
never "done installing" until it has proven ROI on real work.

## Profiles

Offer the user an aggressiveness profile up front; it scopes the whole flow:

- **Conservative** — navigation providers only, no compression, quality first.
- **Balanced** (default) — navigation + measurement; compression only where the
  waste map demands it.
- **Aggressive** — everything applicable; user accepts more validation burden.

Risk still overrides profile: auth/crypto/migration work stays conservative
regardless (model-routing.md downgrade protection).

## Safe Mode / Rollback

If anything degrades after setup — quality drops, sessions misbehave, costs
rise:

1. Disable the most recently added provider first (reverse chronological):
   `python scripts/frugal_setup.py disable <id>` — reversible, takes effect
   next session (`enable <id>` brings it back).
2. Restore configs from `~/.frugal/backups/` if files were modified.
3. Uninstall providers that measured REJECTED in the ROI report:
   `python scripts/frugal_setup.py uninstall <id>` (recoverable backup;
   `--purge` to delete permanently).
4. Fall back to core skill only — it has no dependencies and cannot break.

Native Claude Code behavior must always remain reachable. No provider is ever a
single point of failure.
