use meno_core::{ClaimDocument, MatchAtom, PolicyDocument};
use serde_json::Value;
use std::path::PathBuf;

fn fixture(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn loads_c17_inline_policy() {
    let claim = ClaimDocument::from_yaml(&fixture("claims/C17.yaml")).unwrap();
    assert_eq!(claim.id, "C17");
    let policy = claim.resolved_policy().unwrap().expect("inline policy");
    assert_eq!(policy.requires.len(), 1);
    assert_eq!(policy.requires[0].kind, "command.result");
    let atom = policy.requires[0]
        .match_fields
        .get("exit_code")
        .expect("exit_code match");
    match atom {
        MatchAtom::Exact(v) => assert_eq!(v, &Value::from(0)),
        other => panic!("expected exact 0, got {other:?}"),
    }
}

#[test]
fn rejects_claim_with_evidence_field() {
    let err = ClaimDocument::from_yaml(&fixture("claims/invalid_evidence.yaml"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("evidence"), "{err}");
}

#[test]
fn loads_p17_policy() {
    let policy = PolicyDocument::from_yaml(&fixture("policies/P17.yaml")).unwrap();
    assert_eq!(policy.id, "P17");
    assert_eq!(policy.claim, "C17");
    assert!(policy.requires.is_empty());
}
