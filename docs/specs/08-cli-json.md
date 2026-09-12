# 08 — CLI JSON (`meno_cli_json_version`)

**Version:** 1  
**Status:** Frozen for v1  
**Field:** `meno_cli_json_version = 1`

Frozen machine-readable object emitted by `meno status --json`, `meno verify --json`, and `meno inspect --json`. Adapters and agents consume this object; they must not infer extra CLI flags from it.

Verdict strings are snake_case JSON: `proven` | `disproven` | `unknown`. Claim `state` is `draft` | `frozen` | `retired`.

## Object

```json
{
  "meno_cli_json_version": 1,
  "subject_id": "<64 lowercase hex>",
  "claims": [
    {
      "id": "C-example",
      "statement": "…",
      "state": "frozen",
      "verdict": "proven",
      "supporting": ["<evidence id>", "…"],
      "contradicting": [],
      "stale": [],
      "missing": [
        { "kind": "command.result", "detail": "need 1 distinct matching evidence, found 0" }
      ],
      "conflict": false
    }
  ]
}
```

| Field | Type | Notes |
|---|---|---|
| `meno_cli_json_version` | integer | always `1` in v1 |
| `subject_id` | string | current exact-subject digest (spec 01) |
| `claims[].id` | string | claim id |
| `claims[].statement` | string | claim statement |
| `claims[].state` | string | `draft` \| `frozen` \| `retired` |
| `claims[].verdict` | string | `proven` \| `disproven` \| `unknown` |
| `claims[].supporting` | string[] | fresh supporting evidence ids |
| `claims[].contradicting` | string[] | fresh contradicting evidence ids |
| `claims[].stale` | string[] | otherwise matching evidence bound to a different subject |
| `claims[].missing` | object[] | unsatisfied requires: `{ kind, detail }` |
| `claims[].conflict` | boolean | `true` when fresh support and contradiction both apply → `unknown` |

Retired claims are omitted. `evidence` is omitted when empty.

## Inspect evidence

`meno inspect <claim-id> --json` may add `evidence[]` for that claim’s supporting, contradicting, and stale envelopes (deduplicated, in that order). List-mode `meno inspect --json` and `status` / `verify` JSON do not include `evidence`.

```json
{
  "meno_cli_json_version": 1,
  "subject_id": "<64 hex>",
  "claims": [ { "id": "C-example" } ],
  "evidence": [
    {
      "id": "<ulid>",
      "kind": "command.result",
      "source": { "name": "unit" },
      "provenance": { "producer": "meno.command" },
      "trust": {
        "origin": "machine",
        "reproducible": true,
        "basis": "observed",
        "relation": "direct"
      },
      "captured_at": "<RFC3339>",
      "artifacts": ["<sha256>", "…"],
      "observations": [
        { "type": "command.exit", "exit_code": 0 }
      ]
    }
  ]
}
```

| Field | Type | Notes |
|---|---|---|
| `id` | string | envelope id |
| `kind` | string | envelope kind (spec 02) |
| `source` | object | `name`; optional `version`, `argv`, `config_digest` |
| `provenance` | object | `producer`; optional `actor`, `producer_version`, `host`, `cwd` |
| `trust` | object | `origin` `machine`\|`human`; `reproducible`; `basis` `observed`\|`inferred`; `relation` `direct`\|`indirect` |
| `captured_at` | string | RFC3339 UTC |
| `artifacts` | string[] | SHA-256 hex of stored artifacts (not full refs) |
| `observations` | object[] | `{ "type": "…" }` plus a compact field subset |

Compact observation keys, when present: `status`, `name`, `classname`, `failed`, `errors`, `skipped`, `exit_code`, `message`, `url`, `route`, `title`, `unexpected`, `statement`, `actor`. `message` is truncated to 80 characters.

This is a summary for inspection, not a resealable envelope. Full envelopes are spec 02 (and MCP `get_evidence`).

## `status` never launches tools

`meno status` and `meno status --json` open the project, evaluate stored evidence against the current subject, and print. They do not invoke command adapters, ingest configured JUnit/Playwright paths, or rewrite evidence files.

`meno verify --json` emits the same object **after** optional collection/ingest. Collection is not part of the JSON contract.

Portable directory export is `meno inspect --export` (spec 07), not this object.
