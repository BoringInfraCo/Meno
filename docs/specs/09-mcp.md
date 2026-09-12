# 09 — MCP (`meno_mcp_version`)

**Version:** 1  
**Status:** Frozen for v1  
**Field:** `meno_mcp_version = 1`

MCP is an interface to the same core as the CLI. It is **not** a sixth top-level command.

Serve:

```bash
meno connect --adapter agent --stdio
```

There is no `meno mcp`. `--stdio` and `--write` are mutually exclusive.

Write harness files (after explicit `--write`):

```bash
meno connect --adapter agent --write
meno connect --adapter agent --write --harness generic|claude
```

`--write` records project `.mcp.json` pointing at `meno connect --adapter agent --stdio` and installs the Skill. It does not start the server.

## Transport

JSON-RPC 2.0, one object per line on stdin/stdout (NDJSON). `initialize` reports `protocolVersion` `2024-11-05`, `serverInfo.name` `meno`, and `capabilities.tools`. Tools are listed with `tools/list` and invoked with `tools/call`.

## Version

`get_verification_state` and `request_evaluation` include both:

```json
{
  "meno_cli_json_version": 1,
  "meno_mcp_version": 1,
  "subject_id": "<64 hex>",
  "claims": [ ]
}
```

Claim objects match spec 08 (`id`, `statement`, `state`, `verdict`, `supporting`, `contradicting`, `stale`, `missing`, `conflict`). Verdicts remain `proven` | `disproven` | `unknown`.

## Tools

Exact names, in list order:

| Tool | Role |
|---|---|
| `get_verification_state` | Current subject + claims (same claim shape as `meno status --json`) |
| `list_claims` | `{ "claims": [ … ] }` |
| `get_claim` | One claim; argument `id` |
| `get_evidence` | All envelopes, or one by `id`; returns the spec 02 envelope |
| `inspect_verdict` | `{ claim_id, verdict, why, conflict }`; argument `claim_id` or `id` |
| `propose_claim` | Write a **draft** claim YAML only |
| `submit_evidence` | Submit a machine evidence envelope |
| `request_evaluation` | Re-evaluate stored evidence; **does not** invoke adapters |

`propose_claim` arguments: `id`, `statement`, `policy` (policy body required). Origin is `agent`. It must not freeze, set `state` to `frozen`, or overwrite an existing frozen claim.

`submit_evidence` argument: `envelope`. Empty `subject_id` is filled with the current subject and the envelope is sealed. Human confirmation is rejected (see authority).

`request_evaluation` is a read of stored evidence. It must not run command adapters or ingest files.

## Default authority denials

Default MCP authority does **not** permit:

| Denied | How |
|---|---|
| Freeze a claim | no `freeze_claim`; `propose_claim` always writes `draft` |
| Weaken a frozen claim | cannot overwrite frozen YAML; no `set_claim_state` / `retire_claim` |
| Policy mutation of trusted/frozen contracts | no `update_policy`; freeze-bypass via policy rewrite is denied |
| Delete evidence | no `delete_evidence` |
| Rewrite provenance | no `rewrite_provenance`; submitted envelopes keep sealed trust |
| `human.confirmation` via MCP | `submit_evidence` rejects `kind == "human.confirmation"` and `trust.origin == human` |

Named tools that exist only to fail closed: `freeze_claim`, `retire_claim`, `delete_evidence`, `update_policy`, `set_claim_state`, `rewrite_provenance`. Calling them is an authority error (`<name> is not permitted under default MCP authority`). They are not listed by `tools/list`.

Human confirmation is CLI: `meno inspect --confirm` (spec 08 / Skill). Credentials never belong in `.mcp.json` or Skill files.

## Skill

Behavioral contract (not a source of truth): [`skills/meno/SKILL.md`](../../skills/meno/SKILL.md).

`--write` installs that document to `skills/meno/SKILL.md` and `.agents/skills/meno/SKILL.md`.

## Core has no MCP types

`meno-core` must not mention MCP. Harness types stay in `meno-mcp` (Gate D). CLI remains fully functional without MCP.
