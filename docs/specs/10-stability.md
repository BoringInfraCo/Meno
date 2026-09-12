# 10 — v1 stability surface

**Version:** 1  
**Status:** Frozen for v1  
**Date:** 2026-09-12

This is the v1.0 local freeze. Third-party adapters and harnesses may rely on the integers, names, and behaviors below. Changing any frozen integer is a breaking change and requires a new version, not a silent reinterpretation.

Workspace / crate version is `1.0.0`.

## Frozen version integers

| Name | Value | Spec |
|---|---|---|
| `subject_identity_version` | `1` | [01](01-subject-identity.md) |
| policy grammar / `evaluation_version` | `1` | [03](03-policy-grammar.md) |
| `meno_cli_json_version` | `1` | [08](08-cli-json.md) |
| `meno_mcp_version` | `1` | [09](09-mcp.md) |
| `meno_bundle_version` | `1` | [07](07-bundle.md) |
| SQLite `schema_migrations` version | `1` (`001_initial.sql`, forward-only) | [04](04-sqlite-schema.md) |
| envelope magic | `meno-envelope-v1` | [02](02-evidence-envelope.md) |
| subject magic | `meno-subject-v1` | [01](01-subject-identity.md) |

`meno.toml` `version = 1` selects this contract generation.

Migrations are forward-only. There is no silent destructive migration. Physical SQL is `crates/meno-store/migrations/001_initial.sql`.

## Frozen behaviors

These are stable for v1:

| Surface | Contract |
|---|---|
| Claim lifecycle | `draft` → `frozen` → `retired` (spec 00). Frozen claims are the verification contract. |
| Three-valued verdicts | `proven` \| `disproven` \| `unknown`. Conflict (fresh support **and** contradiction) is `unknown`, not a fourth verdict. |
| Exact-subject freshness | Default `subject_match: exact`. Evidence bound to another subject is stale, not deleted. |
| Evidence envelope | Spec 02. Adapters emit envelopes; they never write verdicts. |
| Provenance and trust | Sealed into the envelope digest. Importers must not rewrite origin/basis. |
| Adapter safety defaults | Unknown adapters: `can_collect=true`, `can_invoke=false`, `side_effect_level=consequential`, `requires_confirmation=true` (spec 06). |
| Five-command ceiling | `init`, `connect`, `verify`, `status`, `inspect`. No `meno mcp`. New work is flags, JSON, `connect`, or `inspect`. |
| CLI JSON | Spec 08. `status` never launches tools. |
| MCP | Spec 09. Default authority cannot freeze, weaken frozen claims, mutate trusted policy, delete evidence, rewrite provenance, or submit `human.confirmation`. |
| Skill | [`skills/meno/SKILL.md`](../../skills/meno/SKILL.md) is the behavioral contract. It is not a source of truth. |
| Portable bundle | Spec 07 directory export (`meno inspect --export`). Thin: no in-toto, no SLSA, no signatures. |

`UNKNOWN` is first-class. Agents and adapters must not treat confidence or prose as `proven`.

## Compat

Third parties check compatibility with:

```bash
cargo test --workspace
python3 spec/reference/subject_hash.py --self-test
```

`spec/reference/subject_hash.py` must agree with `meno_core::subject` on golden snapshots and git repositories.
