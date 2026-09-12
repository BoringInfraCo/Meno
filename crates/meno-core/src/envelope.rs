use crate::canonical::{canonical_json, put_len_bytes, put_tagged_bytes, sha256_hex};
use crate::error::{MenoError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub const ENVELOPE_MAGIC: &[u8] = b"meno-envelope-v1\n";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub id: String,
    pub kind: String,
    pub subject_id: String,
    pub source: Source,
    pub observations: Vec<Observation>,
    pub artifact_refs: Vec<ArtifactRef>,
    pub captured_at: String,
    pub provenance: Provenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<Integrity>,
    pub trust: Trust,
    /// Opaque adapter metadata. Included in integrity; verdicts must ignore it.
    #[serde(default = "empty_object")]
    pub source_metadata: Value,
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argv: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default = "empty_object")]
    pub fields: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    pub size: u64,
    pub relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    pub producer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Integrity {
    pub alg: String,
    pub digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trust {
    pub origin: TrustOrigin,
    pub reproducible: bool,
    pub basis: TrustBasis,
    pub relation: TrustRelation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustOrigin {
    Machine,
    Human,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustBasis {
    Observed,
    Inferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustRelation {
    Direct,
    Indirect,
}

impl Trust {
    pub fn human_confirmation() -> Self {
        Self {
            origin: TrustOrigin::Human,
            reproducible: false,
            basis: TrustBasis::Observed,
            relation: TrustRelation::Direct,
        }
    }

    pub fn inferred_machine() -> Self {
        Self {
            origin: TrustOrigin::Machine,
            reproducible: false,
            basis: TrustBasis::Inferred,
            relation: TrustRelation::Direct,
        }
    }
}

pub fn new_ulid() -> String {
    ulid::Ulid::new().to_string()
}

pub fn canonical_envelope_bytes(env: &Envelope) -> Result<Vec<u8>> {
    let mut buf = Vec::from(ENVELOPE_MAGIC);
    put_tagged_bytes(&mut buf, 0x01, env.id.as_bytes());
    put_tagged_bytes(&mut buf, 0x02, env.kind.as_bytes());
    put_tagged_bytes(&mut buf, 0x03, env.subject_id.as_bytes());
    put_tagged_bytes(&mut buf, 0x04, &serialize_canonical(&env.source)?);

    for obs in &env.observations {
        buf.push(0x05);
        put_len_bytes(&mut buf, obs.type_name.as_bytes());
        put_len_bytes(&mut buf, &canonical_json(&obs.fields)?);
    }

    let mut artifacts: Vec<String> = env
        .artifact_refs
        .iter()
        .map(|a| a.sha256.to_ascii_lowercase())
        .collect();
    artifacts.sort();
    for sha in artifacts {
        put_tagged_bytes(&mut buf, 0x06, sha.as_bytes());
    }

    put_tagged_bytes(&mut buf, 0x07, env.captured_at.as_bytes());
    put_tagged_bytes(&mut buf, 0x08, &serialize_canonical(&env.provenance)?);
    put_tagged_bytes(&mut buf, 0x09, &serialize_canonical(&env.trust)?);
    put_tagged_bytes(&mut buf, 0x0A, &canonical_json(&env.source_metadata)?);
    Ok(buf)
}

pub fn seal_envelope(env: &mut Envelope) -> Result<()> {
    validate_fields(env)?;
    let digest = sha256_hex(&canonical_envelope_bytes(env)?);
    env.integrity = Some(Integrity {
        alg: "sha256".into(),
        digest,
    });
    Ok(())
}

pub fn verify_envelope(env: &Envelope) -> Result<()> {
    validate_fields(env)?;
    let Some(integrity) = &env.integrity else {
        return Err(MenoError::Integrity("envelope is missing integrity".into()));
    };
    if integrity.alg != "sha256" {
        return Err(MenoError::Integrity(format!(
            "unsupported integrity alg {}",
            integrity.alg
        )));
    }
    let expected = sha256_hex(&canonical_envelope_bytes(env)?);
    if integrity.digest != expected {
        return Err(MenoError::Integrity(
            "envelope integrity digest mismatch".into(),
        ));
    }
    Ok(())
}

fn serialize_canonical<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let json = serde_json::to_value(value)?;
    Ok(canonical_json(&json)?)
}

fn validate_fields(env: &Envelope) -> Result<()> {
    env.id
        .parse::<ulid::Ulid>()
        .map_err(|e| MenoError::InvalidEnvelope(format!("id is not a ULID: {e}")))?;
    if env.kind.trim().is_empty() {
        return Err(MenoError::InvalidEnvelope("kind must be non-empty".into()));
    }
    if !is_lowercase_sha256(&env.subject_id) {
        return Err(MenoError::InvalidEnvelope(
            "subject_id must be 64 lowercase hex characters".into(),
        ));
    }
    OffsetDateTime::parse(&env.captured_at, &Rfc3339)
        .map_err(|e| MenoError::InvalidEnvelope(format!("captured_at is not RFC3339: {e}")))?;
    for artifact in &env.artifact_refs {
        if !is_lowercase_sha256(&artifact.sha256) {
            return Err(MenoError::InvalidEnvelope(
                "artifact sha256 must be 64 lowercase hex characters".into(),
            ));
        }
    }
    Ok(())
}

fn is_lowercase_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_envelope() -> Envelope {
        Envelope {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            kind: "command.result".into(),
            subject_id: "0".repeat(64),
            source: Source {
                name: "cmd".into(),
                version: Some("1.0".into()),
                argv: Some(vec!["echo".into()]),
                config_digest: None,
            },
            observations: vec![Observation {
                type_name: "command.exit".into(),
                fields: json!({"exit_code": 0}),
            }],
            artifact_refs: vec![
                ArtifactRef {
                    sha256: "bb".repeat(32),
                    media_type: Some("text/plain".into()),
                    size: 3,
                    relative_path: "artifacts/bb".into(),
                },
                ArtifactRef {
                    sha256: "aa".repeat(32),
                    media_type: None,
                    size: 1,
                    relative_path: "artifacts/aa".into(),
                },
            ],
            captured_at: "2024-01-02T03:04:05Z".into(),
            provenance: Provenance {
                actor: None,
                producer: "meno-test".into(),
                producer_version: Some("0.0.0".into()),
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
            source_metadata: json!({"region": "us"}),
        }
    }

    #[test]
    fn new_ulid_is_parseable() {
        let id = new_ulid();
        assert_eq!(id.len(), 26);
        id.parse::<ulid::Ulid>().unwrap();
    }

    #[test]
    fn seal_is_stable_and_omits_integrity() {
        let mut env = sample_envelope();
        let before = canonical_envelope_bytes(&env).unwrap();
        assert!(before.starts_with(ENVELOPE_MAGIC));
        seal_envelope(&mut env).unwrap();
        let after = canonical_envelope_bytes(&env).unwrap();
        assert_eq!(before, after);
        verify_envelope(&env).unwrap();

        let mut again = env.clone();
        again.integrity = None;
        seal_envelope(&mut again).unwrap();
        assert_eq!(env.integrity, again.integrity);
    }

    #[test]
    fn source_metadata_changes_digest() {
        let mut a = sample_envelope();
        a.source_metadata = json!({"k": 1});
        seal_envelope(&mut a).unwrap();

        let mut b = a.clone();
        b.source_metadata = json!({"k": 2});
        b.integrity = None;
        seal_envelope(&mut b).unwrap();
        assert_ne!(
            a.integrity.as_ref().unwrap().digest,
            b.integrity.as_ref().unwrap().digest
        );

        let mut c = sample_envelope();
        c.source_metadata = json!({"k": 1});
        seal_envelope(&mut c).unwrap();
        assert_eq!(a.integrity, c.integrity);
    }

    #[test]
    fn tampering_fails_verify() {
        let mut env = sample_envelope();
        seal_envelope(&mut env).unwrap();
        env.kind = "junit.report".into();
        let err = verify_envelope(&env).unwrap_err().to_string();
        assert!(err.contains("mismatch"), "{err}");
    }

    #[test]
    fn human_confirmation_trust() {
        let t = Trust::human_confirmation();
        assert_eq!(t.origin, TrustOrigin::Human);
        assert!(!t.reproducible);
        assert_eq!(
            serde_json::to_value(t).unwrap(),
            json!({
                "origin": "human",
                "reproducible": false,
                "basis": "observed",
                "relation": "direct"
            })
        );
    }

    #[test]
    fn inferred_machine_visual_trust() {
        let t = Trust::inferred_machine();
        assert_eq!(t.origin, TrustOrigin::Machine);
        assert_eq!(t.basis, TrustBasis::Inferred);
        assert_eq!(serde_json::to_value(t).unwrap()["origin"], json!("machine"));
        assert_eq!(serde_json::to_value(t).unwrap()["basis"], json!("inferred"));
    }
}
