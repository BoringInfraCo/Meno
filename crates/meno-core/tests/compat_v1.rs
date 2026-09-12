//! v1 compatibility suite. Third parties can run `cargo test -p meno-core --test compat_v1`.

use meno_core::{
    encode_snapshot, hash_snapshot, subject_id_hex, IndexEntry, Snapshot, Verdict, WorktreeEntry,
    WorktreeStatus, ENVELOPE_MAGIC, EVALUATION_VERSION, POLICY_GRAMMAR_VERSION,
    SUBJECT_IDENTITY_VERSION, SUBJECT_MAGIC,
};

/// Same hand-built snapshot as `subject.rs` unit tests. Hash via `encode_snapshot`, not a
/// second encoder, so the frozen digest cannot drift independently of the v1 encode path.
fn frozen_snapshot() -> Snapshot {
    Snapshot {
        origin: b"https://example.com/acme/meno".to_vec(),
        head: b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_vec(),
        index: vec![IndexEntry {
            mode: 33188,
            stage: 0,
            digest: [0x11; 32],
            path: b"src/lib.rs".to_vec(),
        }],
        worktree: vec![WorktreeEntry {
            status: WorktreeStatus::Modified,
            mode: 33188,
            digest: [0x22; 32],
            path: b"src/lib.rs".to_vec(),
        }],
    }
}

#[test]
fn v1_constants_are_frozen() {
    assert_eq!(SUBJECT_IDENTITY_VERSION, 1);
    assert_eq!(EVALUATION_VERSION, 1);
    assert_eq!(POLICY_GRAMMAR_VERSION, 1);
    assert_eq!(SUBJECT_MAGIC, b"meno-subject-v1\n");
    assert_eq!(ENVELOPE_MAGIC, b"meno-envelope-v1\n");
}

#[test]
fn frozen_subject_hash_via_encode_path() {
    let snap = frozen_snapshot();
    let encoded = encode_snapshot(&snap);
    assert!(encoded.starts_with(SUBJECT_MAGIC));
    assert_eq!(
        subject_id_hex(&snap),
        "70c2e9c86aa569c1261b923dd0d6fc49af63edddb7060f3b7897c214b0357ebf"
    );

    let first = hash_snapshot(&snap);
    let second = hash_snapshot(&snap);
    assert_eq!(first, second);
    assert_eq!(subject_id_hex(&snap), subject_id_hex(&snap));
}

#[test]
fn verdict_serde_is_snake_case() {
    assert_eq!(
        serde_json::to_string(&Verdict::Proven).unwrap(),
        "\"proven\""
    );
    assert_eq!(
        serde_json::to_string(&Verdict::Disproven).unwrap(),
        "\"disproven\""
    );
    assert_eq!(
        serde_json::to_string(&Verdict::Unknown).unwrap(),
        "\"unknown\""
    );
    assert_eq!(
        serde_json::from_str::<Verdict>("\"proven\"").unwrap(),
        Verdict::Proven
    );
    assert_eq!(
        serde_json::from_str::<Verdict>("\"disproven\"").unwrap(),
        Verdict::Disproven
    );
    assert_eq!(
        serde_json::from_str::<Verdict>("\"unknown\"").unwrap(),
        Verdict::Unknown
    );
}
