# 02 — Evidence envelope

**Version:** 1  
**Status:** Frozen for v1

Adapters emit this envelope. Core validates it. Adapters never write verdicts. `source_metadata` MUST NOT affect verdict evaluation.

## Fields

| Field | Type | Notes |
|---|---|---|
| `id` | ULID string | assigned at ingest |
| `kind` | string | `command.result`, `junit.report`, `playwright.result`, `human.confirmation`, `generic.envelope` |
| `subject_id` | 64 hex | spec 01 |
| `source` | object | `name`, optional `version`, `argv`, `config_digest` |
| `observations` | array | typed facts, never claims |
| `artifact_refs` | array | `{ sha256, media_type, size, relative_path }` |
| `captured_at` | RFC3339 UTC | |
| `provenance` | object | `actor`, `producer`, `producer_version`, optional `host`, `cwd` |
| `integrity` | object | `{ alg: "sha256", digest }` over canonical envelope **minus this field** |
| `trust` | object | see below |
| `source_metadata` | JSON | opaque; round-tripped; ignored by verdicts |

### Observation

```text
type: string    # e.g. command.exit, junit.case
fields: object  # JSON; keys sorted only in the integrity encoding
```

### Trust

```text
origin: machine | human
reproducible: bool
basis: observed | inferred
relation: direct | indirect
```

Human confirmation: `origin=human`, `reproducible=false`. AI visual judgment MUST use `origin=machine` and `basis=inferred`. It MUST NOT be stored as human confirmation.

## Integrity encoding

Same family as subject encoding (length-prefixed, big-endian):

```text
MAGIC = b"meno-envelope-v1\n"
tag 0x01 id
tag 0x02 kind
tag 0x03 subject_id
tag 0x04 source (canonical JSON UTF-8, object keys sorted)
tag 0x05 each observation in listed order (type, then canonical JSON fields)
tag 0x06 each artifact sha256 lowercase hex, sorted
tag 0x07 captured_at
tag 0x08 provenance canonical JSON
tag 0x09 trust canonical JSON
tag 0x0A source_metadata canonical JSON
```

Canonical JSON: UTF-8, object keys sorted lexicographically, no insignificant whitespace, RFC 8259 number encoding from `serde_json`.

`integrity.digest` is lowercase hex SHA-256 of that byte string. Hash mismatch → evidence cannot support a verdict.
