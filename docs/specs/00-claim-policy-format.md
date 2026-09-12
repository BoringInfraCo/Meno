# 00 — Claim and policy text format

**Version:** 1  
**Status:** Frozen for v1

Human-readable, version-controlled YAML is authoritative for claims and policies. SQLite stores a normalized runtime copy. Runtime evidence never appears in these files.

## Layout

```text
repo/
├── meno.toml
├── claims/*.yaml
└── policies/*.yaml
```

One claim per file. File name should match `id` (`C17.yaml`) but the `id` field is authoritative.

## Claim document

```yaml
id: C17
statement: "A valid user can create an account on mobile."
state: draft
origin:
  kind: human
  actor: sergio
  source: null
policy:
  version: 1
  requires:
    - kind: command.result
      match:
        exit_code: 0
      min_count: 1
      subject_bound: true
  contradicted_by: []
  freshness:
    subject_match: exact
```

Alternatively, `policy_ref: P17` names a document in `policies/`. A claim MUST provide exactly one of inline `policy` or `policy_ref`.

### Fields

| Field | Required | Rules |
|---|---|---|
| `id` | yes | `^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$` |
| `statement` | yes | non-empty, Unicode text |
| `state` | yes | `draft` \| `frozen` \| `retired` |
| `origin.kind` | yes | `human` \| `imported` \| `agent` |
| `origin.actor` | no | string |
| `origin.source` | no | string (issue URL, file, etc.) |
| `policy` | xor `policy_ref` | see spec 03 |
| `policy_ref` | xor `policy` | policy `id` |

Unknown fields are rejected. Fields named `evidence`, `verdict`, `subject`, or `observations` are forbidden — those are runtime state.

## Policy document

```yaml
id: P17
version: 1
claim: C17
requires: []
contradicted_by: []
freshness:
  subject_match: exact
```

Inline policies inherit `claim` from the enclosing claim `id`. Standalone documents MUST set `claim`.

## Config (`meno.toml`)

```toml
[meno]
version = 1
subject_identity_version = 1
```

No secrets. Adapter connections may be added later; v0.0 does not require them.

## Parsing

- UTF-8 YAML 1.1 via a deterministic parser
- Duplicate keys are an error
- Lifecycle and origin values are case-sensitive lowercase
- IDs are case-sensitive

## Git diffs

Prefer block scalars for long statements. Do not rewrite file order on load. The runtime model does not round-trip comments; comments are for humans.
