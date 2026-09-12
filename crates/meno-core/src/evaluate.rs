use crate::envelope::Envelope;
use crate::policy::{MatchAtom, PolicyDocument, Requirement, EVALUATION_VERSION};
use crate::subject::SUBJECT_IDENTITY_VERSION;
use crate::verdict::Verdict;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingEvidence {
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluation {
    pub verdict: Verdict,
    pub claim_id: String,
    pub subject_id: String,
    pub supporting: Vec<String>,
    pub contradicting: Vec<String>,
    pub stale: Vec<String>,
    pub missing: Vec<MissingEvidence>,
    pub conflict: bool,
    pub evaluation_version: u32,
    pub subject_identity_version: u32,
}

/// Deterministic evaluation. Adapters never call this; only core/CLI/store.
pub fn evaluate(
    claim_id: &str,
    policy: &PolicyDocument,
    current_subject_id: &str,
    evidence: &[Envelope],
) -> Evaluation {
    let prepared: Vec<Prepared<'_>> = evidence.iter().map(Prepared::from_envelope).collect();

    let mut stale = BTreeSet::new();
    let mut supporting = BTreeSet::new();
    let mut missing = Vec::new();

    let mut support_ok = true;
    for requirement in &policy.requires {
        let ids = fresh_match_ids(requirement, current_subject_id, &prepared, &mut stale);
        if ids.len() < requirement.min_count as usize {
            support_ok = false;
            missing.push(MissingEvidence {
                kind: requirement.kind.clone(),
                detail: format!(
                    "need {} distinct matching evidence, found {}",
                    requirement.min_count,
                    ids.len()
                ),
            });
        }
        supporting.extend(ids);
    }

    let mut contradicting = BTreeSet::new();
    let mut contradict_ok = false;
    for requirement in &policy.contradicted_by {
        let ids = fresh_match_ids(requirement, current_subject_id, &prepared, &mut stale);
        if ids.len() >= requirement.min_count as usize {
            contradict_ok = true;
        }
        contradicting.extend(ids);
    }

    let (verdict, conflict) = match (support_ok, contradict_ok) {
        (true, false) => (Verdict::Proven, false),
        (false, true) => (Verdict::Disproven, false),
        (true, true) => (Verdict::Unknown, true),
        (false, false) => (Verdict::Unknown, false),
    };

    Evaluation {
        verdict,
        claim_id: claim_id.to_string(),
        subject_id: current_subject_id.to_string(),
        supporting: supporting.into_iter().collect(),
        contradicting: contradicting.into_iter().collect(),
        stale: stale.into_iter().collect(),
        missing,
        conflict,
        evaluation_version: EVALUATION_VERSION,
        subject_identity_version: SUBJECT_IDENTITY_VERSION,
    }
}

struct Prepared<'a> {
    envelope: &'a Envelope,
    fields: Map<String, Value>,
}

impl<'a> Prepared<'a> {
    fn from_envelope(envelope: &'a Envelope) -> Self {
        Self {
            envelope,
            fields: merged_observation_fields(envelope),
        }
    }
}

fn merged_observation_fields(envelope: &Envelope) -> Map<String, Value> {
    let mut fields = Map::new();
    for observation in &envelope.observations {
        let Value::Object(map) = &observation.fields else {
            continue;
        };
        for (key, value) in map {
            fields.insert(key.clone(), value.clone());
        }
    }
    fields
}

fn fresh_match_ids(
    requirement: &Requirement,
    current_subject_id: &str,
    prepared: &[Prepared<'_>],
    stale: &mut BTreeSet<String>,
) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for item in prepared {
        if item.envelope.kind != requirement.kind {
            continue;
        }
        if requirement.subject_bound && item.envelope.subject_id != current_subject_id {
            stale.insert(item.envelope.id.clone());
            continue;
        }
        if envelope_matches_fields(item, requirement) {
            ids.insert(item.envelope.id.clone());
        }
    }
    ids
}

fn envelope_matches_fields(item: &Prepared<'_>, requirement: &Requirement) -> bool {
    if item.envelope.observations.iter().any(|observation| {
        let Value::Object(fields) = &observation.fields else {
            return false;
        };
        match_fields_hold(fields, requirement)
    }) {
        return true;
    }
    match_fields_hold(&item.fields, requirement)
}

fn match_fields_hold(fields: &Map<String, Value>, requirement: &Requirement) -> bool {
    requirement
        .match_fields
        .iter()
        .all(|(key, atom)| field_matches(fields.get(key), atom))
}

fn field_matches(value: Option<&Value>, atom: &MatchAtom) -> bool {
    let Some(value) = value else {
        return false;
    };
    match atom {
        MatchAtom::Exact(expected) => json_equal(value, expected),
        MatchAtom::Pred(pred) => {
            if let Some(expected) = &pred.eq {
                if !json_equal(value, expected) {
                    return false;
                }
            }
            if let Some(unexpected) = &pred.neq {
                if json_equal(value, unexpected) {
                    return false;
                }
            }
            true
        }
    }
}

fn json_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => match (a.as_i64(), b.as_i64()) {
            (Some(x), Some(y)) => x == y,
            _ => a == b,
        },
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| json_equal(x, y))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|other| json_equal(v, other)))
        }
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{
        new_ulid, seal_envelope, Observation, Provenance, Source, Trust, TrustBasis, TrustOrigin,
        TrustRelation,
    };
    use serde_json::json;

    const SUBJECT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OTHER_SUBJECT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn command_policy() -> PolicyDocument {
        PolicyDocument::from_yaml(
            r#"
id: P-cmd
version: 1
claim: C17
requires:
  - kind: command.result
    match:
      exit_code: 0
    min_count: 1
    subject_bound: true
contradicted_by:
  - kind: command.result
    match:
      exit_code:
        neq: 0
freshness:
  subject_match: exact
"#,
        )
        .expect("valid command policy")
    }

    fn envelope(
        kind: &str,
        subject_id: &str,
        fields: Value,
        source_metadata: Value,
        captured_at: &str,
    ) -> Envelope {
        let mut env = Envelope {
            id: new_ulid(),
            kind: kind.into(),
            subject_id: subject_id.into(),
            source: Source {
                name: "cmd".into(),
                version: None,
                argv: None,
                config_digest: None,
            },
            observations: vec![Observation {
                type_name: "command.exit".into(),
                fields,
            }],
            artifact_refs: vec![],
            captured_at: captured_at.into(),
            provenance: Provenance {
                actor: None,
                producer: "meno-test".into(),
                producer_version: None,
                host: None,
                cwd: None,
            },
            integrity: None,
            trust: Trust {
                origin: TrustOrigin::Machine,
                reproducible: true,
                basis: TrustBasis::Observed,
                relation: TrustRelation::Direct,
            },
            source_metadata,
        };
        seal_envelope(&mut env).expect("seal test envelope");
        env
    }

    fn command_result(subject_id: &str, exit_code: i64) -> Envelope {
        envelope(
            "command.result",
            subject_id,
            json!({ "exit_code": exit_code }),
            json!({}),
            "2024-01-02T03:04:05Z",
        )
    }

    #[test]
    fn command_result_exit_code_zero_on_current_subject_is_proven() {
        let policy = command_policy();
        let evidence = command_result(SUBJECT, 0);
        let id = evidence.id.clone();
        let result = evaluate("C17", &policy, SUBJECT, &[evidence]);
        assert_eq!(result.verdict, Verdict::Proven);
        assert!(!result.conflict);
        assert_eq!(result.supporting, vec![id]);
        assert!(result.contradicting.is_empty());
        assert!(result.stale.is_empty());
        assert!(result.missing.is_empty());
        assert_eq!(result.evaluation_version, 1);
        assert_eq!(result.subject_identity_version, 1);
        assert_eq!(result.claim_id, "C17");
        assert_eq!(result.subject_id, SUBJECT);
    }

    #[test]
    fn exit_code_neq_zero_only_is_disproven() {
        let policy = command_policy();
        let evidence = command_result(SUBJECT, 1);
        let id = evidence.id.clone();
        let result = evaluate("C17", &policy, SUBJECT, &[evidence]);
        assert_eq!(result.verdict, Verdict::Disproven);
        assert!(!result.conflict);
        assert!(result.supporting.is_empty());
        assert_eq!(result.contradicting, vec![id]);
        assert!(result.stale.is_empty());
    }

    #[test]
    fn support_and_contradict_fresh_is_unknown_conflict() {
        let policy = command_policy();
        let support = command_result(SUBJECT, 0);
        let contradict = command_result(SUBJECT, 1);
        let mut expected_support = vec![support.id.clone()];
        let mut expected_contradict = vec![contradict.id.clone()];
        expected_support.sort();
        expected_contradict.sort();
        let result = evaluate("C17", &policy, SUBJECT, &[contradict, support]);
        assert_eq!(result.verdict, Verdict::Unknown);
        assert!(result.conflict);
        assert_eq!(result.supporting, expected_support);
        assert_eq!(result.contradicting, expected_contradict);
        assert!(result.missing.is_empty());
    }

    #[test]
    fn different_subject_id_is_stale_unknown() {
        let policy = command_policy();
        let evidence = command_result(OTHER_SUBJECT, 0);
        let id = evidence.id.clone();
        let result = evaluate("C17", &policy, SUBJECT, &[evidence]);
        assert_eq!(result.verdict, Verdict::Unknown);
        assert!(!result.conflict);
        assert!(result.supporting.is_empty());
        assert_eq!(result.stale, vec![id]);
        assert!(!result.missing.is_empty());
        assert_eq!(result.missing[0].kind, "command.result");
    }

    #[test]
    fn captured_at_does_not_affect_freshness() {
        // File/timestamp identity is not evaluated here (see subject.rs).
        let policy = command_policy();
        let a = envelope(
            "command.result",
            SUBJECT,
            json!({ "exit_code": 0 }),
            json!({}),
            "2020-01-01T00:00:00Z",
        );
        let b = envelope(
            "command.result",
            SUBJECT,
            json!({ "exit_code": 0 }),
            json!({}),
            "2025-12-31T23:59:59Z",
        );
        let ra = evaluate("C17", &policy, SUBJECT, &[a]);
        let rb = evaluate("C17", &policy, SUBJECT, &[b]);
        assert_eq!(ra.verdict, Verdict::Proven);
        assert_eq!(rb.verdict, Verdict::Proven);
    }

    #[test]
    fn same_inputs_same_evaluation() {
        let policy = command_policy();
        let support = command_result(SUBJECT, 0);
        let contradict = command_result(SUBJECT, 2);
        let evidence = [support, contradict];
        let first = evaluate("C17", &policy, SUBJECT, &evidence);
        let second = evaluate("C17", &policy, SUBJECT, &evidence);
        assert_eq!(first, second);
        let reversed = evaluate(
            "C17",
            &policy,
            SUBJECT,
            &[evidence[1].clone(), evidence[0].clone()],
        );
        assert_eq!(first, reversed);
    }

    #[test]
    fn unknown_does_not_become_proven_without_matching_evidence() {
        let policy = command_policy();
        let noise = envelope(
            "junit.report",
            SUBJECT,
            json!({ "failed": 0 }),
            json!({}),
            "2024-01-02T03:04:05Z",
        );
        let more_noise = envelope(
            "junit.report",
            SUBJECT,
            json!({ "failed": 0 }),
            json!({}),
            "2024-01-02T03:04:05Z",
        );
        let first = evaluate("C17", &policy, SUBJECT, std::slice::from_ref(&noise));
        let second = evaluate("C17", &policy, SUBJECT, &[noise.clone(), more_noise]);
        assert_eq!(first.verdict, Verdict::Unknown);
        assert_eq!(second.verdict, Verdict::Unknown);
        assert!(!first.conflict);
        assert!(!second.conflict);

        let matching = command_result(SUBJECT, 0);
        let proven = evaluate("C17", &policy, SUBJECT, &[noise, matching]);
        assert_eq!(proven.verdict, Verdict::Proven);
    }

    #[test]
    fn empty_evidence_is_unknown_with_missing_not_disproven() {
        let policy = command_policy();
        let result = evaluate("C17", &policy, SUBJECT, &[]);
        assert_eq!(result.verdict, Verdict::Unknown);
        assert_ne!(result.verdict, Verdict::Disproven);
        assert!(!result.conflict);
        assert!(result.supporting.is_empty());
        assert!(result.contradicting.is_empty());
        assert!(result.stale.is_empty());
        assert_eq!(result.missing.len(), 1);
        assert_eq!(result.missing[0].kind, "command.result");
        assert!(result.missing[0].detail.contains("found 0"));
    }

    #[test]
    fn source_metadata_does_not_affect_verdict() {
        let policy = command_policy();
        let a = envelope(
            "command.result",
            SUBJECT,
            json!({ "exit_code": 0 }),
            json!({ "region": "us" }),
            "2024-01-02T03:04:05Z",
        );
        let mut b = a.clone();
        b.source_metadata = json!({ "region": "eu", "extra": true });
        b.integrity = None;
        seal_envelope(&mut b).expect("reseal");
        let ra = evaluate("C17", &policy, SUBJECT, &[a]);
        let rb = evaluate("C17", &policy, SUBJECT, &[b]);
        assert_eq!(ra.verdict, rb.verdict);
        assert_eq!(ra.verdict, Verdict::Proven);
        assert_eq!(ra.supporting, rb.supporting);
        assert_eq!(ra.conflict, rb.conflict);
    }

    #[test]
    fn later_observations_overlay_earlier_fields() {
        let policy = PolicyDocument::from_yaml(
            r#"
id: P-cmd
version: 1
claim: C17
requires:
  - kind: command.result
    match:
      exit_code: 0
      duration_ms: 10
    min_count: 1
    subject_bound: true
contradicted_by: []
freshness:
  subject_match: exact
"#,
        )
        .expect("valid overlay policy");
        let mut env = command_result(SUBJECT, 0);
        env.observations.push(Observation {
            type_name: "command.timing".into(),
            fields: json!({ "duration_ms": 10 }),
        });
        env.integrity = None;
        seal_envelope(&mut env).expect("reseal overlay");
        let result = evaluate("C17", &policy, SUBJECT, &[env]);
        assert_eq!(result.verdict, Verdict::Proven);
    }

    #[test]
    fn unbound_requirement_accepts_other_subject() {
        let policy = PolicyDocument::from_yaml(
            r#"
id: P-unbound
version: 1
claim: C17
requires:
  - kind: command.result
    match:
      exit_code: 0
    subject_bound: false
contradicted_by: []
freshness:
  subject_match: exact
"#,
        )
        .expect("valid unbound policy");
        let evidence = command_result(OTHER_SUBJECT, 0);
        let result = evaluate("C17", &policy, SUBJECT, &[evidence]);
        assert_eq!(result.verdict, Verdict::Proven);
        assert!(result.stale.is_empty());
    }

    fn junit_policy(match_yaml: &str) -> PolicyDocument {
        PolicyDocument::from_yaml(&format!(
            "
id: P-junit
version: 1
claim: C-junit
requires:
  - kind: junit.report
    match:
{match_yaml}
    min_count: 1
    subject_bound: true
contradicted_by: []
freshness:
  subject_match: exact
"
        ))
        .expect("valid junit policy")
    }

    fn junit_report() -> Envelope {
        let mut env = Envelope {
            id: new_ulid(),
            kind: "junit.report".into(),
            subject_id: SUBJECT.into(),
            source: Source {
                name: "junit".into(),
                version: None,
                argv: None,
                config_digest: None,
            },
            observations: vec![
                Observation {
                    type_name: "junit.summary".into(),
                    fields: json!({
                        "tests": 4,
                        "passed": 2,
                        "failed": 1,
                        "errors": 1,
                        "skipped": 1
                    }),
                },
                Observation {
                    type_name: "junit.case".into(),
                    fields: json!({ "name": "ok", "status": "pass" }),
                },
                Observation {
                    type_name: "junit.case".into(),
                    fields: json!({ "name": "bad", "status": "fail" }),
                },
                Observation {
                    type_name: "junit.case".into(),
                    fields: json!({ "name": "boom", "status": "error" }),
                },
                Observation {
                    type_name: "junit.case".into(),
                    fields: json!({ "name": "nah", "status": "skipped" }),
                },
            ],
            artifact_refs: vec![],
            captured_at: "2024-01-02T03:04:05Z".into(),
            provenance: Provenance {
                actor: None,
                producer: "meno-test".into(),
                producer_version: None,
                host: None,
                cwd: None,
            },
            integrity: None,
            trust: Trust {
                origin: TrustOrigin::Machine,
                reproducible: true,
                basis: TrustBasis::Observed,
                relation: TrustRelation::Direct,
            },
            source_metadata: json!({}),
        };
        seal_envelope(&mut env).expect("seal test envelope");
        env
    }

    #[test]
    fn junit_summary_failed_count_matches_that_observation() {
        let evidence = junit_report();
        let none = evaluate(
            "C-junit",
            &junit_policy("      failed: 0"),
            SUBJECT,
            std::slice::from_ref(&evidence),
        );
        assert_eq!(none.verdict, Verdict::Unknown);
        assert!(none.supporting.is_empty());
        assert!(!none.missing.is_empty());

        let some = evaluate(
            "C-junit",
            &junit_policy("      failed: 1"),
            SUBJECT,
            &[evidence],
        );
        assert_eq!(some.verdict, Verdict::Proven);
    }

    #[test]
    fn junit_fail_case_matches_even_when_last_case_is_skipped() {
        let evidence = junit_report();
        let result = evaluate(
            "C-junit",
            &junit_policy("      name: \"bad\"\n      status: \"fail\""),
            SUBJECT,
            &[evidence],
        );
        assert_eq!(result.verdict, Verdict::Proven);
    }

    #[test]
    fn junit_skipped_status_is_not_flattened_away() {
        let evidence = junit_report();
        let result = evaluate(
            "C-junit",
            &junit_policy("      status: \"skipped\""),
            SUBJECT,
            &[evidence],
        );
        assert_eq!(result.verdict, Verdict::Proven);
    }

    #[test]
    fn junit_match_predicates_must_hold_on_the_same_observation() {
        let evidence = junit_report();
        let result = evaluate(
            "C-junit",
            &junit_policy("      name: \"bad\"\n      status: \"pass\""),
            SUBJECT,
            &[evidence],
        );
        assert_eq!(result.verdict, Verdict::Unknown);
        assert!(result.supporting.is_empty());
        assert!(!result.missing.is_empty());
    }
}
