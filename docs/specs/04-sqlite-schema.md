# 04 — SQLite schema and migrations

**Version:** 1  
**Status:** Frozen for v1

Project-local file: `.meno/meno.db`. WAL mode. Foreign keys ON. Forward-only transactional migrations. No silent destructive migration.

Backup: copy `meno.db` (and `-wal`/`-shm` if present) plus `.meno/artifacts/`.

## Tables

`schema_migrations`, `projects`, `subjects`, `claims`, `claim_revisions`, `policies`, `policy_revisions`, `evidence`, `observations`, `artifacts`, `evidence_artifacts`, `claim_evidence`, `verdicts`, `connections`, `adapter_runs`, `audit_events`

Physical SQL: `crates/meno-store/migrations/001_initial.sql`.

## Invariants

- `verdicts` are a derived cache with explanation JSON; recomputable from claims + policies + evidence + subject
- `claim_evidence.relation ∈ {support, contradict, related}`
- Evidence rows are immutable. Staleness is query-time (`evidence.subject_id` vs current subject)
- `audit_events` are append-only
- Each verdict stores `evaluation_version` and `subject_identity_version`
- `connections.config_json` MUST NOT contain credential-shaped values; store refuses them
- Secrets never belong in this database
