//! SQLite persistence and content-addressed artifacts for Meno.
//!
//! [`Store`] wraps a single-threaded [`rusqlite::Connection`] and is not [`Sync`].

mod artifacts;
mod bundle;
mod connections;
mod db;
mod error;
mod evidence;
mod sync;

pub use bundle::{BundleReport, BUNDLE_VERSION};
pub use connections::ConnectionRecord;
pub use db::{open, Store, CURRENT_SCHEMA_VERSION};
pub use error::{Result, StoreError};
pub use evidence::{SubjectRow, VerdictRow};
pub use meno_core::envelope::ArtifactRef;
pub use sync::{ClaimRecord, SyncReport};

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    const TABLES: &[&str] = &[
        "adapter_runs",
        "artifacts",
        "audit_events",
        "claim_evidence",
        "claim_revisions",
        "claims",
        "connections",
        "evidence",
        "evidence_artifacts",
        "observations",
        "policies",
        "policy_revisions",
        "projects",
        "schema_migrations",
        "subjects",
        "verdicts",
    ];

    fn table_names(store: &Store) -> Vec<String> {
        let mut stmt = store
            .conn
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
                 ORDER BY name",
            )
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn migrate_empty_to_current_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("meno.db");

        let store = Store::open(&db).unwrap();
        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);

        let wal: String = store
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(wal.to_lowercase(), "wal");
        let fks: i64 = store
            .conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(fks, 1);
        assert_eq!(table_names(&store), TABLES);

        let applied: String = store
            .conn
            .query_row(
                "SELECT name FROM schema_migrations WHERE version = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(applied, "001_initial");
        drop(store);

        let store = Store::open(&db).unwrap();
        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        assert_eq!(table_names(&store), TABLES);
    }

    #[test]
    fn put_get_verify_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open_project(&tmp.path().join(".meno")).unwrap();
        let bytes = b"meno artifact payload";
        let refer = store.put(bytes, "text/plain").unwrap();

        assert_eq!(refer.relative_path, format!("artifacts/{}", refer.sha256));
        assert_eq!(refer.media_type.as_deref(), Some("text/plain"));
        assert_eq!(refer.size, bytes.len() as u64);
        assert_eq!(store.get(&refer.sha256).unwrap(), bytes);
        store.verify(&refer.sha256).unwrap();

        let again = store.put(bytes, "text/plain").unwrap();
        assert_eq!(again.sha256, refer.sha256);
    }

    #[test]
    fn mutated_file_fails_verify() {
        let tmp = tempfile::tempdir().unwrap();
        let meno_dir = tmp.path().join(".meno");
        let store = Store::open_project(&meno_dir).unwrap();
        let refer = store.put(b"original", "text/plain").unwrap();
        std::fs::write(meno_dir.join("artifacts").join(&refer.sha256), b"mutated").unwrap();
        let err = store.verify(&refer.sha256).unwrap_err();
        assert!(matches!(err, StoreError::IntegrityMismatch { .. }), "{err}");
    }

    #[test]
    fn reject_pem_private_key() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open_project(&tmp.path().join(".meno")).unwrap();
        let pem = b"-----BEGIN PRIVATE KEY-----\nMIIBVgIBADANBgkqhkiG9w0BAQEFAASCAUAwggE8AgEAAkEA\n-----END PRIVATE KEY-----\n";
        let err = store.put(pem, "application/x-pem-file").unwrap_err();
        assert!(matches!(err, StoreError::RedactionRejected), "{err}");
    }

    #[test]
    fn refuse_connection_config_with_password() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("meno.db")).unwrap();
        let err = store
            .insert_connection("conn-1", "command", "postgres://local?password=secret")
            .unwrap_err();
        assert!(matches!(err, StoreError::CredentialInConfig), "{err}");

        store
            .insert_connection("conn-1", "command", r#"{"timeout_ms": 30}"#)
            .unwrap();
    }

    fn sample_connection(config_json: &str) -> ConnectionRecord {
        ConnectionRecord {
            id: "command:unit".into(),
            adapter: "command".into(),
            name: "unit".into(),
            config_json: config_json.into(),
            can_collect: true,
            can_invoke: true,
            side_effect_level: "none".into(),
            requires_confirmation: false,
        }
    }

    #[test]
    fn upsert_list_connection_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("meno.db")).unwrap();
        let rec = sample_connection(r#"{"argv":["true"]}"#);
        store.upsert_connection(&rec).unwrap();

        let listed = store.list_connections().unwrap();
        assert_eq!(listed, vec![rec.clone()]);
        assert!(listed[0].can_invoke);
        assert_eq!(listed[0].side_effect_level, "none");

        let replaced = ConnectionRecord {
            can_invoke: false,
            side_effect_level: "consequential".into(),
            requires_confirmation: true,
            config_json: r#"{"argv":["false"]}"#.into(),
            ..rec
        };
        store.upsert_connection(&replaced).unwrap();
        let listed = store.list_connections().unwrap();
        assert_eq!(listed, vec![replaced]);
        assert!(!listed[0].can_invoke);
        assert_eq!(listed[0].side_effect_level, "consequential");
    }

    #[test]
    fn upsert_connection_refuses_password_in_config() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("meno.db")).unwrap();
        let rec = sample_connection("password=secret");
        let err = store.upsert_connection(&rec).unwrap_err();
        assert!(matches!(err, StoreError::CredentialInConfig), "{err}");
        assert!(store.list_connections().unwrap().is_empty());
    }

    #[test]
    fn claim_evidence_relation_and_verdict_checks() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("meno.db")).unwrap();
        let now = "2026-09-11T00:00:00Z";
        store
            .conn
            .execute(
                "INSERT INTO claims (id, statement, state, origin_kind, current_revision, created_at, updated_at)
                 VALUES ('C1', 'stmt', 'draft', 'human', 1, ?1, ?1)",
                params![now],
            )
            .unwrap();
        store
            .conn
            .execute(
                "INSERT INTO subjects (id, identity_version, captured_at, created_at) VALUES ('s1', 1, ?1, ?1)",
                params![now],
            )
            .unwrap();
        store
            .conn
            .execute(
                "INSERT INTO evidence (id, kind, subject_id, envelope_json, integrity_alg, integrity_digest, captured_at, created_at)
                 VALUES ('E1', 'command.result', 's1', '{}', 'sha256', 'abc', ?1, ?1)",
                params![now],
            )
            .unwrap();

        store
            .conn
            .execute(
                "INSERT INTO claim_evidence (claim_id, evidence_id, relation, created_at)
                 VALUES ('C1', 'E1', 'nope', ?1)",
                params![now],
            )
            .unwrap_err();
        store
            .conn
            .execute(
                "INSERT INTO verdicts (claim_id, subject_id, verdict, evaluation_version, subject_identity_version, explanation_json, evaluated_at)
                 VALUES ('C1', 's1', 'maybe', 1, 1, '{}', ?1)",
                params![now],
            )
            .unwrap_err();
    }

    fn write_claim(root: &std::path::Path, id: &str, statement: &str, state: &str) {
        let dir = root.join("claims");
        std::fs::create_dir_all(&dir).unwrap();
        let yaml = format!(
            "id: {id}\nstatement: \"{statement}\"\nstate: {state}\norigin:\n  kind: human\npolicy:\n  version: 1\n  requires: []\n  contradicted_by: []\n"
        );
        std::fs::write(dir.join(format!("{id}.yaml")), yaml).unwrap();
    }

    fn sample_envelope(subject_id: &str) -> meno_core::Envelope {
        let mut env = meno_core::Envelope {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            kind: "command.result".into(),
            subject_id: subject_id.into(),
            source: meno_core::Source {
                name: "cmd".into(),
                version: None,
                argv: None,
                config_digest: None,
            },
            observations: vec![meno_core::Observation {
                type_name: "command.exit".into(),
                fields: serde_json::json!({"exit_code": 0}),
            }],
            artifact_refs: vec![],
            captured_at: "2024-01-02T03:04:05Z".into(),
            provenance: meno_core::Provenance {
                actor: None,
                producer: "meno-test".into(),
                producer_version: None,
                host: None,
                cwd: None,
            },
            integrity: None,
            trust: meno_core::Trust::human_confirmation(),
            source_metadata: serde_json::json!({}),
        };
        meno_core::seal_envelope(&mut env).unwrap();
        env
    }

    #[test]
    fn sync_two_claims_then_load() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_claim(root, "C1", "first claim", "draft");
        write_claim(root, "C2", "second claim", "draft");

        let store = Store::open(&root.join("meno.db")).unwrap();
        let report = store.sync_contracts(root).unwrap();
        assert_eq!(report.claims_upserted, 2);
        assert_eq!(report.policies_upserted, 2);

        let loaded = store.load_claims().unwrap();
        assert_eq!(loaded.len(), 2);
        let ids: Vec<&str> = loaded.iter().map(|c| c.document.id.as_str()).collect();
        assert!(ids.contains(&"C1"), "{ids:?}");
        assert!(ids.contains(&"C2"), "{ids:?}");
        assert!(loaded.iter().all(|c| c.document.policy_ref.is_some()));
        assert!(loaded.iter().all(|c| c.revision >= 1));
        assert!(store.load_policy_for_claim("C1").unwrap().is_some());
        assert!(store.load_policy_for_claim("C2").unwrap().is_some());
    }

    #[test]
    fn frozen_claim_statement_change_records_revision_and_audit() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_claim(root, "C1", "original statement", "frozen");

        let store = Store::open(&root.join("meno.db")).unwrap();
        store.sync_contracts(root).unwrap();

        write_claim(root, "C1", "edited statement", "frozen");
        let report = store.sync_contracts(root).unwrap();
        assert!(
            report.revisions_recorded >= 1,
            "expected a revision, got {report:?}"
        );

        let mut stmt = store
            .conn
            .prepare("SELECT action FROM audit_events")
            .unwrap();
        let actions: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(
            actions
                .iter()
                .any(|a| a == "claim_changed" || a == "claim_frozen" || a == "claim_proposed"),
            "unexpected audit actions: {actions:?}"
        );
        assert!(
            actions.iter().any(|a| a == "claim_changed"),
            "expected claim_changed after frozen statement edit: {actions:?}"
        );

        let loaded = store.load_claims().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].document.statement, "edited statement");
        assert!(loaded[0].revision >= 2);
    }

    #[test]
    fn insert_evidence_then_update_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("meno.db")).unwrap();
        let subject_id = "ab".repeat(32);
        store.upsert_subject(&subject_id, 1, None, None).unwrap();
        let env = sample_envelope(&subject_id);
        store.insert_evidence(&env).unwrap();

        let loaded = store.load_evidence().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, env.id);

        let err = store
            .conn
            .execute(
                "UPDATE evidence SET kind = 'x' WHERE id = ?1",
                params![env.id],
            )
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("immutable") || msg.contains("evidence"),
            "{msg}"
        );
    }

    #[test]
    fn upsert_subject_twice_same_id_is_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("meno.db")).unwrap();
        let id = "cd".repeat(32);
        store
            .upsert_subject(&id, 1, Some("https://example.com/a"), Some("aaa"))
            .unwrap();
        store
            .upsert_subject(&id, 2, Some("https://example.com/b"), Some("bbb"))
            .unwrap();

        let origin: Option<String> = store
            .conn
            .query_row(
                "SELECT origin FROM subjects WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(origin.as_deref(), Some("https://example.com/a"));
        let count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM subjects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
