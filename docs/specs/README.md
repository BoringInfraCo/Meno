# Meno implementation contracts

These specifications freeze **v1** semantics so third-party adapters can be written against this directory without reading CLI internals.

Each spec is a contract: IDs, envelopes, policy evaluation, storage, JSON, and MCP. Product narrative lives in `docs/internal/`. If a spec disagrees with the PRD, the product docs win and the spec must change — not the other way around.

| Spec | Status | Crate |
|---|---|---|
| [00 Claim/policy text](00-claim-policy-format.md) | Frozen for v1 | `meno-core` |
| [01 Subject identity](01-subject-identity.md) | Frozen for v1 | `meno-core`, `spec/reference` |
| [02 Evidence envelope](02-evidence-envelope.md) | Frozen for v1 | `meno-core` |
| [03 Policy grammar](03-policy-grammar.md) | Frozen for v1 | `meno-core` |
| [04 SQLite schema](04-sqlite-schema.md) | Frozen for v1 | `meno-store` |
| [05 Artifacts](05-artifacts.md) | Frozen for v1 | `meno-core`, `meno-store` |
| [06 Adapter contract](06-adapter-contract.md) | Frozen for v1 | `meno-adapters` |
| [07 Portable bundle](07-bundle.md) | Frozen for v1 | `meno-store` |
| [08 CLI JSON](08-cli-json.md) | Frozen for v1 | `meno-cli` |
| [09 MCP](09-mcp.md) | Frozen for v1 | `meno-mcp` |
| [10 Stability surface](10-stability.md) | Frozen for v1 | workspace |

Skill behavioral contract: [`skills/meno/SKILL.md`](../../skills/meno/SKILL.md).

Cross-implementation check: `spec/reference/subject_hash.py` must agree with `meno_core::subject` on golden snapshots and git repositories. Compat commands are in [10](10-stability.md).
