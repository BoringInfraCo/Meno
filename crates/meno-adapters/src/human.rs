//! Human confirmation evidence. AI visual judgment is never stored as human.

use meno_core::envelope::{
    new_ulid, seal_envelope, ArtifactRef, Envelope, Observation, Provenance, Source, Trust,
};
use serde_json::{json, Map, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::contract::AdapterError;

pub struct HumanConfirmation {
    pub statement: String,
    pub actor: Option<String>,
    pub artifact: Option<ArtifactRef>,
}

/// kind = "human.confirmation"
/// trust = Trust::human_confirmation()  // origin=human, reproducible=false, basis=observed
/// producer = "meno.human"
/// observation type "human.statement" fields: { statement, actor? }
/// seal_envelope. Never writes a Verdict.
pub fn normalize_human_confirmation(
    input: &HumanConfirmation,
    subject_id: &str,
) -> Result<Envelope, AdapterError> {
    let statement = require_statement(&input.statement)?;
    let captured_at = now_rfc3339()?;

    let mut fields = Map::new();
    fields.insert("statement".to_string(), Value::String(statement));
    if let Some(actor) = &input.actor {
        fields.insert("actor".to_string(), Value::String(actor.clone()));
    }

    let mut artifact_refs = Vec::new();
    if let Some(artifact) = input.artifact.clone() {
        artifact_refs.push(artifact);
    }

    let mut envelope = Envelope {
        id: new_ulid(),
        kind: "human.confirmation".to_string(),
        subject_id: subject_id.to_string(),
        source: Source {
            name: "human".to_string(),
            version: None,
            argv: None,
            config_digest: None,
        },
        observations: vec![Observation {
            type_name: "human.statement".to_string(),
            fields: Value::Object(fields),
        }],
        artifact_refs,
        captured_at,
        provenance: Provenance {
            actor: input.actor.clone(),
            producer: "meno.human".to_string(),
            producer_version: None,
            host: None,
            cwd: None,
        },
        integrity: None,
        trust: Trust::human_confirmation(),
        source_metadata: json!({}),
    };
    seal_envelope(&mut envelope).map_err(|err| AdapterError::message(err.to_string()))?;
    Ok(envelope)
}

/// Explicit machine-inferred visual (NOT human). kind can be "visual.inference"
/// trust = Trust::inferred_machine()  // origin=machine, basis=inferred
/// Include this so tests prove it is distinct. CLI must not call this from --confirm.
pub fn normalize_inferred_visual(
    statement: &str,
    subject_id: &str,
) -> Result<Envelope, AdapterError> {
    let statement = require_statement(statement)?;
    let captured_at = now_rfc3339()?;

    let mut envelope = Envelope {
        id: new_ulid(),
        kind: "visual.inference".to_string(),
        subject_id: subject_id.to_string(),
        source: Source {
            name: "visual".to_string(),
            version: None,
            argv: None,
            config_digest: None,
        },
        observations: vec![Observation {
            type_name: "visual.statement".to_string(),
            fields: json!({ "statement": statement }),
        }],
        artifact_refs: Vec::new(),
        captured_at,
        provenance: Provenance {
            actor: None,
            producer: "meno.visual".to_string(),
            producer_version: None,
            host: None,
            cwd: None,
        },
        integrity: None,
        trust: Trust::inferred_machine(),
        source_metadata: json!({}),
    };
    seal_envelope(&mut envelope).map_err(|err| AdapterError::message(err.to_string()))?;
    Ok(envelope)
}

fn require_statement(statement: &str) -> Result<String, AdapterError> {
    let statement = statement.trim();
    if statement.is_empty() {
        return Err(AdapterError::message("empty statement"));
    }
    Ok(statement.to_string())
}

fn now_rfc3339() -> Result<String, AdapterError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|err| AdapterError::message(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use meno_core::envelope::{TrustBasis, TrustOrigin};
    use meno_core::verdict::Verdict;

    fn subject() -> String {
        "0".repeat(64)
    }

    #[test]
    fn normalize_human_confirmation_trust_is_human_observed() {
        let input = HumanConfirmation {
            statement: "looks correct".to_string(),
            actor: Some("alice".to_string()),
            artifact: Some(ArtifactRef {
                sha256: "aa".repeat(32),
                media_type: Some("image/png".into()),
                size: 12,
                relative_path: "shot.png".into(),
            }),
        };
        let envelope = normalize_human_confirmation(&input, &subject()).expect("normalize");

        assert_eq!(envelope.kind, "human.confirmation");
        assert_eq!(envelope.trust.origin, TrustOrigin::Human);
        assert!(!envelope.trust.reproducible);
        assert_eq!(envelope.trust.basis, TrustBasis::Observed);
        assert_eq!(envelope.trust, Trust::human_confirmation());
        assert_eq!(envelope.provenance.producer, "meno.human");
        assert_eq!(envelope.provenance.actor.as_deref(), Some("alice"));
        assert_eq!(envelope.source.name, "human");
        assert_eq!(envelope.observations.len(), 1);
        assert_eq!(envelope.observations[0].type_name, "human.statement");
        assert_eq!(
            envelope.observations[0].fields["statement"],
            json!("looks correct")
        );
        assert_eq!(envelope.observations[0].fields["actor"], json!("alice"));
        assert_eq!(envelope.artifact_refs.len(), 1);

        let value = serde_json::to_value(&envelope).expect("json");
        assert!(value.get("verdict").is_none());
        let _ = std::any::type_name::<Verdict>();
        meno_core::verify_envelope(&envelope).expect("sealed envelope verifies");
    }

    #[test]
    fn normalize_inferred_visual_is_machine_not_human_confirmation() {
        let envelope = normalize_inferred_visual("button is red", &subject()).expect("normalize");

        assert_eq!(envelope.trust.origin, TrustOrigin::Machine);
        assert_eq!(envelope.trust.basis, TrustBasis::Inferred);
        assert_eq!(envelope.trust, Trust::inferred_machine());
        assert_ne!(envelope.kind, "human.confirmation");
        assert_eq!(envelope.kind, "visual.inference");
        assert_ne!(envelope.trust.origin, TrustOrigin::Human);
        assert_ne!(envelope.trust, Trust::human_confirmation());

        let value = serde_json::to_value(&envelope).expect("json");
        assert!(value.get("verdict").is_none());
        meno_core::verify_envelope(&envelope).expect("sealed envelope verifies");
    }

    #[test]
    fn empty_statement_errors() {
        let input = HumanConfirmation {
            statement: String::new(),
            actor: None,
            artifact: None,
        };
        let err = normalize_human_confirmation(&input, &subject()).expect_err("empty");
        assert!(err.to_string().contains("empty statement"), "{err}");

        let err = normalize_human_confirmation(
            &HumanConfirmation {
                statement: "   ".to_string(),
                actor: None,
                artifact: None,
            },
            &subject(),
        )
        .expect_err("whitespace");
        assert!(err.to_string().contains("empty statement"), "{err}");

        let err = normalize_inferred_visual("", &subject()).expect_err("empty visual");
        assert!(err.to_string().contains("empty statement"), "{err}");
    }
}
