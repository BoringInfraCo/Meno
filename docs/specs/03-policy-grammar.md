# 03 — Minimal policy grammar

**Version:** 1  
**Status:** Frozen for v1  
**Evaluation version:** 1 (stored on every verdict row)

AND-only. No LLM judge. No scores. No OR. Add fields only when a real adapter demonstrates need.

## Document

```yaml
id: P-signup-cmd
version: 1
claim: C17
requires:
  - kind: command.result
    match:
      exit_code: 0
    min_count: 1
    subject_bound: true
contradicted_by:
  - kind: command.result
    match:
      exit_code:
        neq: 0
freshness:
  subject_match: exact
```

### Requirement

| Field | Default | Meaning |
|---|---|---|
| `kind` | required | evidence kind string |
| `match` | `{}` | observation field predicates (all must hold on the same evidence item) |
| `min_count` | 1 | minimum distinct matching evidence ids |
| `subject_bound` | true | must share the current subject |

`match` values are either an exact JSON value or a predicate object `{ eq: ... }` / `{ neq: ... }`.

`freshness.subject_match` is `exact` in v0. Evidence whose `subject_id` differs is stale: historical, not applicable.

## Evaluation (preview; engine lands in v0.1)

Given current subject S, frozen claim, policy, and evidence:

- Fresh applicable supporting evidence satisfies every `requires` entry AND no fresh applicable `contradicted_by` matches → `PROVEN`
- Fresh applicable `contradicted_by` matches AND support is not satisfied → `DISPROVEN`
- Support and contradiction both apply → `UNKNOWN` with conflict explanation
- Missing / stale / inapplicable → `UNKNOWN`
- Adapter or execution failure → run error; claim stays `UNKNOWN` (never auto-DISPROVEN)

Absence of proof is not proof of failure.
