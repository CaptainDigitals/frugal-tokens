# Model & Delegation Routing

Route every unit of work to the cheapest executor that will succeed.

## Complexity Ladder

### Complexity 0 — No model

Deterministic work goes to scripts and CLI tools, never to model tokens:

counting, sorting, diffing, hashing, parsing, filtering, deduplication,
AST/schema extraction, renaming across files (`sed`/IDE rename), format
conversion.

If code can do it reliably, code does it.

### Complexity 1 — Cheapest model / lightweight subagent

Simple extraction and classification: "which of these files mention X",
log categorization, summarizing a doc, formatting results, boilerplate.

### Complexity 2 — Default model, main context

Normal feature work, bug fixes, refactors within a module.

### Complexity 3 — Strong model, focused context

Subtle race conditions, cross-service debugging, complex refactors,
performance analysis. Give it *less but better* context — Tier 0/1 only.

### Complexity 4 — Strongest authorized model

Architecture, security analysis, cryptography, production migrations.

## Downgrade Protection

Never route to a cheaper model for:

- authentication / authorization logic
- cryptography
- production database migrations
- destructive or irreversible operations
- security remediation
- regulated / compliance-sensitive workflows

The cost of one wrong patch here exceeds a year of token savings.

## Escalation, Not Pre-Escalation

```text
cheap attempt → confidence high? → done
             → confidence low?  → escalate with the cheap attempt's findings
```

Escalate on: ambiguity, repeated test failures, many affected modules, security
surface, a previous failed attempt. The cheap attempt's output becomes input to
the stronger attempt — it's not wasted.

## Dirty-Context Isolation (Subagents)

Exploration contaminates context. Delegate it:

**Wrong:** read 73 files into the main context to find the 4 that control auth.

**Right:** spawn an explorer subagent → it reads 73 files in *its* context →
returns 4 paths + dependencies + confidence (~1K tokens).

Delegate when:

- expected context contamination is high (many files, verbose output)
- the result is compact relative to the exploration (paths, summaries, verdicts)
- the work is parallelizable (N independent lookups → N subagents at once)

Keep in main context when:

- the exploration output *is* the working context (you'll edit those files next)
- the task is small enough that delegation overhead exceeds the savings
- tight iteration between reading and editing is needed

## Subagent Prompt Discipline

A frugal subagent prompt states: the question, the scope (paths/globs), the
expected return shape ("list of paths + one-line reason each"), and what NOT to
return ("do not include file contents"). Vague prompts produce verbose returns
that defeat the purpose.
