use meno_core::envelope::{
    new_ulid, seal_envelope, Envelope, Observation, Provenance, Source, Trust, TrustBasis,
    TrustOrigin, TrustRelation,
};

use crate::contract::{
    Adapter, AdapterError, AdapterSafety, DetectContext, DetectResult, InputBundle, SideEffectLevel,
};

/// Test double used to lock the type-system invariant: adapters return envelopes.
#[derive(Debug, Default, Clone, Copy)]
pub struct FakeAdapter;

impl Adapter for FakeAdapter {
    fn name(&self) -> &str {
        "fake"
    }

    fn safety(&self) -> AdapterSafety {
        AdapterSafety {
            can_collect: true,
            can_invoke: false,
            side_effect_level: SideEffectLevel::None,
            requires_confirmation: false,
        }
    }

    fn detect(&self, _ctx: &DetectContext) -> Result<DetectResult, AdapterError> {
        Ok(DetectResult {
            detected: true,
            detail: Some("fake".to_string()),
        })
    }

    fn normalize(&self, input: &InputBundle) -> Result<Envelope, AdapterError> {
        let mut envelope = Envelope {
            id: new_ulid(),
            kind: "generic.envelope".to_string(),
            subject_id: "0".repeat(64),
            source: Source {
                name: self.name().to_string(),
                version: None,
                argv: None,
                config_digest: None,
            },
            observations: vec![Observation {
                type_name: "generic.payload".to_string(),
                fields: serde_json::json!({
                    "kind": input.kind,
                    "payload": input.payload,
                }),
            }],
            artifact_refs: Vec::new(),
            captured_at: "1970-01-01T00:00:00Z".to_string(),
            provenance: Provenance {
                actor: None,
                producer: "meno-adapters".to_string(),
                producer_version: Some(env!("CARGO_PKG_VERSION").to_string()),
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
            source_metadata: serde_json::json!({}),
        };
        seal_envelope(&mut envelope).map_err(|err| AdapterError::message(err.to_string()))?;
        Ok(envelope)
    }

    fn validate(&self, envelope: &Envelope) -> Result<(), AdapterError> {
        if envelope.kind != "generic.envelope" {
            return Err(AdapterError::message(format!(
                "expected kind generic.envelope, got {}",
                envelope.kind
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meno_core::envelope::Envelope;
    #[allow(unused_imports)]
    use meno_core::verdict::Verdict;

    #[test]
    fn adapter_returns_envelope_not_verdict() {
        // Only core writes Verdict. Adapter::normalize's return type is Envelope;
        // the trait has no persist_verdict.
        // v1 kinds: command.result, junit.report, playwright.result,
        // human.confirmation, generic.envelope.
        let adapter = FakeAdapter;
        let envelope = adapter
            .normalize(&InputBundle {
                kind: "bytes".to_string(),
                payload: b"hello".to_vec(),
                path: None,
            })
            .expect("fake normalize");
        let _: Envelope = envelope;
        fn _normalize_is_envelope<A: Adapter>(
            adapter: &A,
            input: &InputBundle,
        ) -> Result<Envelope, AdapterError> {
            adapter.normalize(input)
        }
        let _ = _normalize_is_envelope::<FakeAdapter>;
        let _ = std::any::type_name::<Verdict>();
        assert_eq!(adapter.name(), "fake");
        assert_eq!(
            std::any::type_name_of_val(&FakeAdapter::normalize),
            std::any::type_name_of_val(&<FakeAdapter as Adapter>::normalize)
        );
    }
}
