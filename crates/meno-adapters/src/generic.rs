//! Generic evidence envelope import (frozen schema).

use meno_core::envelope::{seal_envelope, verify_envelope, Envelope};

use crate::contract::AdapterError;

/// Parse a JSON evidence envelope (frozen schema). If integrity is missing, seal_envelope.
/// If subject_id is empty, set `subject_id` argument.
/// Kind may be `generic.envelope` or whatever is in the JSON (keep the file's kind if set).
/// verify_envelope before return.
pub fn ingest_generic_envelope(json: &[u8], subject_id: &str) -> Result<Envelope, AdapterError> {
    let mut env: Envelope = serde_json::from_slice(json)
        .map_err(|err| AdapterError::message(format!("invalid envelope json: {err}")))?;

    let mut mutated = false;
    if env.subject_id.is_empty() {
        env.subject_id = subject_id.to_string();
        mutated = true;
    }
    if env.kind.trim().is_empty() {
        env.kind = "generic.envelope".to_string();
        mutated = true;
    }

    if env.integrity.is_none() || mutated {
        env.integrity = None;
        seal_envelope(&mut env).map_err(|err| AdapterError::message(err.to_string()))?;
    }

    verify_envelope(&env).map_err(|err| AdapterError::message(err.to_string()))?;
    Ok(env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use meno_core::envelope::{
        new_ulid, seal_envelope, Observation, Provenance, Source, Trust, TrustBasis, TrustOrigin,
        TrustRelation,
    };
    use serde_json::json;

    fn minimal_envelope() -> Envelope {
        Envelope {
            id: new_ulid(),
            kind: "generic.envelope".to_string(),
            subject_id: "0".repeat(64),
            source: Source {
                name: "custom".to_string(),
                version: None,
                argv: None,
                config_digest: None,
            },
            observations: vec![Observation {
                type_name: "generic.note".to_string(),
                fields: json!({"ok": true}),
            }],
            artifact_refs: Vec::new(),
            captured_at: "2024-01-02T03:04:05Z".to_string(),
            provenance: Provenance {
                actor: None,
                producer: "external".to_string(),
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
        }
    }

    #[test]
    fn ingest_generic_round_trip_sealed_envelope() {
        let mut original = minimal_envelope();
        seal_envelope(&mut original).expect("seal");
        let json = serde_json::to_vec(&original).expect("json");

        let ingested = ingest_generic_envelope(&json, &original.subject_id).expect("ingest");
        verify_envelope(&ingested).expect("verify");
        assert_eq!(ingested, original);
        assert_eq!(ingested.kind, "generic.envelope");
    }

    #[test]
    fn ingest_generic_seals_missing_integrity_and_fills_subject() {
        let mut original = minimal_envelope();
        original.subject_id.clear();
        original.kind = "command.result".to_string();
        let json = serde_json::to_vec(&original).expect("json");
        let subject = "ab".repeat(32);

        let ingested = ingest_generic_envelope(&json, &subject).expect("ingest");
        verify_envelope(&ingested).expect("verify");
        assert_eq!(ingested.subject_id, subject);
        assert_eq!(ingested.kind, "command.result");
        assert!(ingested.integrity.is_some());
    }
}
