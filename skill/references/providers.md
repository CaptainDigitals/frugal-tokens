# Optional Providers — Open-Source Token/Session Optimizers

Frugal Tokens core is dependency-free. These community tools can amplify it for
specific workloads. Recommend them when the workload fits; never treat any as
required. Every claim of savings must be verified against real usage data
(see "Verify Before Trusting" below).

Trust classes (per the PRD supply-chain model):
**VERIFIED** — widely used, actively maintained, auditable.
**COMMUNITY** — useful, review before adopting.
**EXPERIMENTAL** — promising, validate carefully.

---

## Navigation Providers (cut exploration cost)

### ripgrep — VERIFIED (already built in)

Claude Code's Grep tool is ripgrep. No install needed. The discipline is using
it *targeted* (globs, types, head_limit) instead of broad scans.

### ast-grep — VERIFIED

Structural AST search: find *code shapes* ("all calls to `foo` with 2 args"),
not text matches. Near-zero cost vs. an LLM reading files.

```bash
# any of:
cargo install ast-grep --locked
npm i -g @ast-grep/cli
winget install ast-grep.ast-grep
```

When it pays off: refactors, deprecation sweeps, "find all callers of X shaped
like Y" in any sizable codebase. Repo: https://github.com/ast-grep/ast-grep

### tree-sitter CLI — VERIFIED

Parser toolkit underlying ast-grep and most editors. Install directly only if
you're writing custom structural queries:

```bash
npm i -g tree-sitter-cli
```

Repo: https://github.com/tree-sitter/tree-sitter

### Language servers (LSP) — VERIFIED

Precise symbol lookup, references, and rename — the cheapest correct answer to
"who calls this?". Install per language (typescript-language-server, pyright,
rust-analyzer, gopls) and prefer LSP-backed queries over grep-and-read loops.

### Graphify — COMMUNITY

Persistent code knowledge graph with incremental updates — replaces repeated
raw-file exploration with graph queries. Ships as a Claude Code skill.

```bash
git clone https://github.com/DCS-Hub-DCS/Graphify ~/.claude/skills/graphify
```

When it pays off: large repos/monorepos revisited across many sessions, where
repository re-exploration dominates the waste map. Skip for small or
rarely-revisited repos — index maintenance overhead beats the savings.
Repo: https://github.com/DCS-Hub-DCS/Graphify

### DeepWiki — EXPERIMENTAL (MCP service, data egress)

Auto-generated architecture wikis for GitHub repos (Cognition). Querying a
~2K-token wiki page about a subsystem is cheap Tier-2 context vs. exploring
15 files. Ships as a bundled manifest (`providers/deepwiki.json`).

```bash
claude mcp add -s user -t http deepwiki https://mcp.deepwiki.com/mcp
# remove: claude mcp remove deepwiki
```

When it pays off: large *public* repos, onboarding into unfamiliar codebases.
Hard caveats: **data egress** — repo identity and queries go to a third-party
cloud, so policy-deny for private/client code; summaries can be stale vs.
recent commits and are LLM-generated — never trust them for Tier 0/1 decisions
without verifying against source. Service: https://deepwiki.com

---

## Compression Providers (cut ingestion cost)

### token-compact — COMMUNITY

Claude Code skill that compresses documents for LLM token efficiency while
preserving semantic content. Useful for large docs/specs being loaded as
context.

```bash
git clone https://github.com/theosib/token-compact ~/.claude/skills/token-compact
```

Caveat: run the Information Retention check (context-tiers.md) on its output —
never accept a compression that drops Tier 0 facts.
Repo: https://github.com/theosib/token-compact

---

## Measurement Providers (prove the savings)

### token-saver — COMMUNITY

Skill + cheap-model subagents + **cache-aware real-billing audit**. Its key
contribution: community testing showing that claimed output-compression savings
sometimes do *not* reduce real billed cost (cache invalidation eats the gains).
Use its audit approach to validate any optimizer, including this one.

```bash
git clone https://github.com/bryanvine/token-saver ~/.claude/skills/token-saver
```

Repo: https://github.com/bryanvine/token-saver

### tontran/claude-code token-optimization guide — COMMUNITY (docs only)

Reference guidance on compaction at task boundaries and subagent isolation —
aligned with this skill's checkpoint discipline. Nothing to install.
https://github.com/tontran/claude-code/blob/main/docs/token-optimization.md

---

## Runtime Scripts

The framework is executable, not just documented (stdlib-only Python):

```bash
python scripts/frugal_providers.py status          # what's installed, install hints
python scripts/frugal_setup.py recommend           # repo profile → adaptive advice
python scripts/frugal_setup.py install <id>        # clone into skills dir
python scripts/frugal_setup.py disable <id>        # park it (skipped next session)
python scripts/frugal_setup.py enable <id>         # bring it back
python scripts/frugal_setup.py uninstall <id>      # remove (recoverable backup)
python scripts/frugal_conflicts.py --enable a b c  # validate an active set
python scripts/frugal_roi.py report                # baseline vs optimized economics
```

## Adding New Providers — No Code Changes

New third-party optimizers register by dropping a JSON manifest into
`~/.frugal/providers/<id>.json` (same shape as
`frugal_providers.py manifest <id>` output — id, trust, capabilities,
priority, conflicts, requires, detect, repo, fixed_context_tax_tokens).

Everything picks the new provider up automatically:

- **recommendations** are keyed by *capability*, not provider id — a manifest
  declaring `navigation.graph` inherits the large-repo heuristic, one
  declaring `compression.document` inherits the docs heuristic, and
  `compression.output`/`request_proxy` inherit the default-deny rule. A
  manifest can pin its own verdict with `"default_recommendation"` +
  `"recommendation_reason"`.
- **conflict solving** applies the exclusive-capability and priority rules.
- **install/disable/uninstall lifecycle** works for any manifest with a
  `detect.skill` and a `repo` URL.
- **ROI measurement** applies as soon as tasks are recorded with the new id in
  `--providers` — and its measured verdict then overrides the heuristics.

## Conflict Rules

Only one provider per capability may be active (enforced by
`frugal_conflicts.py`):

- **One compression layer.** Stacking compressors (e.g. token-compact plus a
  proxy compressor) compounds retention loss and destroys cache stability.
- **Navigation providers compose** (LSP + ast-grep + Graphify coexist fine) —
  but follow the cheap-first ladder: LSP/AST before graph query before grep.
- **Skill overlap:** if another installed skill also manages compaction or
  output filtering, disable one — conflicting guidance costs more than either
  saves.

## Verify Before Trusting

Every provider must earn its place with measured data, not claims:

1. Note baseline cost/tokens for 3–10 typical tasks *before* enabling it.
2. Enable one provider at a time — never several in the same window.
3. Compare real usage (cache reads included) after 3–10 tasks.
4. If effective cost didn't drop, or quality/retry rate degraded: remove it.

A provider that adds a fixed context tax (always-loaded skill text, MCP schema)
must save more per session than its tax costs — audit with the Fixed Context
Tax rules in cache-discipline.md.
