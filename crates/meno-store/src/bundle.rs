use crate::db::{now_iso8601, Store};
use crate::error::{Result, StoreError};
use crate::evidence::{verdict_sql, SubjectRow, VerdictRow};
use meno_core::canonical::sha256_hex;
use meno_core::{
    verify_envelope, ClaimDocument, ClaimState, Envelope, OriginKind, PolicyDocument, Verdict,
    EVALUATION_VERSION, SUBJECT_IDENTITY_VERSION,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

/// Directory-bundle format version (`meno_bundle_version`).
pub const BUNDLE_VERSION: u32 = 1;

const BUNDLE_JSON: &str = "meno-bundle.json";

/// Counts from export or additive import. `skipped` is missing artifacts,
/// unverifiable envelopes, and envelopes whose artifacts are unavailable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleReport {
    pub claims: usize,
    pub envelopes: usize,
    pub artifacts: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleDocument {
    meno_bundle_version: u32,
    exported_at: String,
    subject_identity_version: u32,
    evaluation_version: u32,
    subjects: Vec<BundleSubject>,
    claims: Vec<ClaimDocument>,
    policies: Vec<PolicyDocument>,
    envelopes: Vec<Envelope>,
    verdicts: Vec<BundleVerdict>,
    artifacts: Vec<BundleArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleSubject {
    id: String,
    identity_version: u32,
    origin: Option<String>,
    head: Option<String>,
}

impl From<SubjectRow> for BundleSubject {
    fn from(row: SubjectRow) -> Self {
        Self {
            id: row.id,
            identity_version: row.identity_version,
            origin: row.origin,
            head: row.head,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleVerdict {
    claim_id: String,
    subject_id: String,
    verdict: Verdict,
    evaluation_version: u32,
    subject_identity_version: u32,
    explanation_json: String,
}

impl From<VerdictRow> for BundleVerdict {
    fn from(row: VerdictRow) -> Self {
        Self {
            claim_id: row.claim_id,
            subject_id: row.subject_id,
            verdict: row.verdict,
            evaluation_version: row.evaluation_version,
            subject_identity_version: row.subject_identity_version,
            explanation_json: row.explanation_json,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleArtifact {
    sha256: String,
    media_type: String,
    size: u64,
}

impl Store {
    /// Write a bundle directory. `claim_id` None = all claims/evidence;
    /// Some = that claim plus related evidence (envelopes linked via
    /// `claim_evidence`, plus those envelopes' subjects/artifacts).
    pub fn export_bundle(&self, dest: &Path, claim_id: Option<&str>) -> Result<BundleReport> {
        ensure_empty_dest(dest)?;

        if let Some(id) = claim_id {
            if !self.claim_exists(id)? {
                return Err(StoreError::NotFound(format!("claim {id}")));
            }
        }

        let claims = selected_claims(self, claim_id)?;
        let policies = selected_policies(self, claim_id)?;
        let envelopes = selected_envelopes(self, claim_id)?;
        let mut verdicts: Vec<BundleVerdict> = self
            .load_latest_verdicts()?
            .into_iter()
            .map(BundleVerdict::from)
            .collect();
        if let Some(id) = claim_id {
            verdicts.retain(|v| v.claim_id == id);
        }

        let mut needed_subjects: BTreeSet<String> = BTreeSet::new();
        for env in &envelopes {
            needed_subjects.insert(env.subject_id.clone());
        }
        for v in &verdicts {
            needed_subjects.insert(v.subject_id.clone());
        }

        let mut subjects: Vec<BundleSubject> = self
            .list_subjects()?
            .into_iter()
            .map(BundleSubject::from)
            .collect();
        if claim_id.is_some() {
            subjects.retain(|s| needed_subjects.contains(&s.id));
        }
        let present: BTreeSet<String> = subjects.iter().map(|s| s.id.clone()).collect();
        for id in &needed_subjects {
            if !present.contains(id) {
                subjects.push(BundleSubject {
                    id: id.clone(),
                    identity_version: SUBJECT_IDENTITY_VERSION,
                    origin: None,
                    head: None,
                });
            }
        }
        subjects.sort_by(|a, b| a.id.cmp(&b.id));

        let artifacts = selected_artifacts(self, claim_id.is_none(), &envelopes)?;

        let mut report = BundleReport {
            claims: claims.len(),
            envelopes: envelopes.len(),
            artifacts: artifacts.len(),
            skipped: 0,
        };

        let doc = BundleDocument {
            meno_bundle_version: BUNDLE_VERSION,
            exported_at: now_iso8601()?,
            subject_identity_version: SUBJECT_IDENTITY_VERSION,
            evaluation_version: EVALUATION_VERSION,
            subjects,
            claims,
            policies,
            envelopes,
            verdicts,
            artifacts: artifacts.clone(),
        };

        let mut json = serde_json::to_string_pretty(&doc)?;
        json.push('\n');
        std::fs::write(dest.join(BUNDLE_JSON), json)?;

        let artifacts_out = dest.join("artifacts");
        for art in &artifacts {
            match self.get(&art.sha256) {
                Ok(bytes) => {
                    std::fs::create_dir_all(&artifacts_out)?;
                    std::fs::write(artifacts_out.join(&art.sha256), bytes)?;
                }
                Err(StoreError::ArtifactNotFound(_))
                | Err(StoreError::NoArtifactsDir)
                | Err(StoreError::IntegrityMismatch { .. }) => {
                    report.skipped += 1;
                }
                Err(err) => return Err(err),
            }
        }

        Ok(report)
    }

    /// Additive import from a bundle directory containing `meno-bundle.json`.
    /// Never overwrites existing claim rows. Does not rewrite envelope trust.
    pub fn import_bundle(&self, src: &Path) -> Result<BundleReport> {
        let json_path = src.join(BUNDLE_JSON);
        if !json_path.is_file() {
            return Err(StoreError::NotFound(format!(
                "{BUNDLE_JSON} in {}",
                src.display()
            )));
        }
        let text = std::fs::read_to_string(&json_path)?;
        let doc: BundleDocument = serde_json::from_str(&text)?;
        if doc.meno_bundle_version != BUNDLE_VERSION {
            return Err(StoreError::Contract(format!(
                "unsupported meno_bundle_version {}",
                doc.meno_bundle_version
            )));
        }

        let mut report = BundleReport::default();

        for subject in &doc.subjects {
            self.upsert_subject(
                &subject.id,
                subject.identity_version,
                subject.origin.as_deref(),
                subject.head.as_deref(),
            )?;
        }

        for claim in &doc.claims {
            if self.insert_claim_if_missing(claim)? {
                report.claims += 1;
            }
        }

        for policy in &doc.policies {
            match self.insert_policy_if_missing(policy)? {
                PolicyInsert::Inserted => {}
                PolicyInsert::Exists => {}
                PolicyInsert::Skipped => report.skipped += 1,
            }
        }

        let artifacts_dir = src.join("artifacts");
        for art in &doc.artifacts {
            let file = artifacts_dir.join(&art.sha256);
            if !file.is_file() {
                report.skipped += 1;
                continue;
            }
            if self.artifacts_dir.is_none() {
                return Err(StoreError::NoArtifactsDir);
            }
            if self.artifact_row_exists(&art.sha256)? {
                continue;
            }
            let bytes = std::fs::read(&file)?;
            if sha256_hex(&bytes) != art.sha256 {
                report.skipped += 1;
                continue;
            }
            match self.put(&bytes, &art.media_type) {
                Ok(_) => report.artifacts += 1,
                Err(StoreError::RedactionRejected) => report.skipped += 1,
                Err(err) => return Err(err),
            }
        }

        for env in &doc.envelopes {
            match self.import_envelope(env)? {
                EnvelopeImport::Inserted => report.envelopes += 1,
                EnvelopeImport::Exists => {}
                EnvelopeImport::Skipped => report.skipped += 1,
            }
        }

        for verdict in &doc.verdicts {
            self.import_verdict(verdict, &mut report)?;
        }

        Ok(report)
    }

    fn insert_claim_if_missing(&self, claim: &ClaimDocument) -> Result<bool> {
        if self.claim_exists(&claim.id)? {
            return Ok(false);
        }
        let now = now_iso8601()?;
        let policy_id = claim_policy_id(claim);
        self.conn.execute(
            "INSERT INTO claims (
                id, statement, state, origin_kind, origin_actor, origin_source,
                policy_id, current_revision, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?8)",
            params![
                claim.id,
                claim.statement,
                claim_state_sql(claim.state),
                origin_kind_sql(claim.origin.kind),
                claim.origin.actor.as_deref(),
                claim.origin.source.as_deref(),
                policy_id,
                now
            ],
        )?;
        Ok(true)
    }

    fn insert_policy_if_missing(&self, policy: &PolicyDocument) -> Result<PolicyInsert> {
        if self.policy_exists(&policy.id)? {
            return Ok(PolicyInsert::Exists);
        }
        if !self.claim_exists(&policy.claim)? {
            return Ok(PolicyInsert::Skipped);
        }
        let now = now_iso8601()?;
        let body_json = serde_json::to_string(policy)?;
        self.conn.execute(
            "INSERT INTO policies (
                id, claim_id, version, body_json, current_revision, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
            params![policy.id, policy.claim, policy.version, body_json, now],
        )?;
        Ok(PolicyInsert::Inserted)
    }

    fn import_envelope(&self, env: &Envelope) -> Result<EnvelopeImport> {
        if self.evidence_exists(&env.id)? {
            return Ok(EnvelopeImport::Exists);
        }
        if verify_envelope(env).is_err() {
            return Ok(EnvelopeImport::Skipped);
        }

        let mut artifact_ids = BTreeSet::new();
        for artifact in &env.artifact_refs {
            artifact_ids.insert(artifact.sha256.clone());
        }
        for sha256 in &artifact_ids {
            if !self.artifact_row_exists(sha256)? {
                return Ok(EnvelopeImport::Skipped);
            }
        }

        if !self.subject_exists(&env.subject_id)? {
            self.upsert_subject(&env.subject_id, SUBJECT_IDENTITY_VERSION, None, None)?;
        }

        match self.insert_evidence(env) {
            Ok(()) => Ok(EnvelopeImport::Inserted),
            Err(StoreError::NotFound(_)) | Err(StoreError::Core(_)) => Ok(EnvelopeImport::Skipped),
            Err(err) => Err(err),
        }
    }

    fn import_verdict(&self, row: &BundleVerdict, report: &mut BundleReport) -> Result<()> {
        if !self.claim_exists(&row.claim_id)? || !self.subject_exists(&row.subject_id)? {
            report.skipped += 1;
            return Ok(());
        }
        if let Some((existing_verdict, existing_expl)) =
            self.latest_verdict(&row.claim_id, &row.subject_id)?
        {
            if existing_expl == row.explanation_json && existing_verdict == verdict_sql(row.verdict)
            {
                return Ok(());
            }
        }
        self.insert_verdict(
            &row.claim_id,
            &row.subject_id,
            row.verdict,
            row.evaluation_version,
            row.subject_identity_version,
            &row.explanation_json,
        )
    }
}

enum PolicyInsert {
    Inserted,
    Exists,
    Skipped,
}

enum EnvelopeImport {
    Inserted,
    Exists,
    Skipped,
}

fn ensure_empty_dest(dest: &Path) -> Result<()> {
    if dest.exists() {
        if dest.is_file() {
            return Err(StoreError::Contract(format!(
                "bundle dest {} is a file",
                dest.display()
            )));
        }
        let empty = std::fs::read_dir(dest)?.next().is_none();
        if !empty {
            return Err(StoreError::Contract(format!(
                "bundle dest {} exists and is not empty",
                dest.display()
            )));
        }
    }
    std::fs::create_dir_all(dest)?;
    Ok(())
}

fn selected_claims(store: &Store, claim_id: Option<&str>) -> Result<Vec<ClaimDocument>> {
    let mut claims: Vec<ClaimDocument> = store
        .load_claims()?
        .into_iter()
        .map(|r| r.document)
        .collect();
    if let Some(id) = claim_id {
        claims.retain(|c| c.id == id);
    }
    Ok(claims)
}

fn selected_policies(store: &Store, claim_id: Option<&str>) -> Result<Vec<PolicyDocument>> {
    let mut policies = store.load_policies()?;
    if let Some(id) = claim_id {
        policies.retain(|p| p.claim == id);
    }
    Ok(policies)
}

fn selected_envelopes(store: &Store, claim_id: Option<&str>) -> Result<Vec<Envelope>> {
    match claim_id {
        None => store.load_evidence(),
        Some(id) => store.load_evidence_for_claim(id),
    }
}

fn selected_artifacts(
    store: &Store,
    all: bool,
    envelopes: &[Envelope],
) -> Result<Vec<BundleArtifact>> {
    if all {
        return Ok(store
            .list_artifact_meta()?
            .into_iter()
            .map(|(sha256, media_type, size)| BundleArtifact {
                sha256,
                media_type,
                size: size.max(0) as u64,
            })
            .collect());
    }

    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for env in envelopes {
        for refer in &env.artifact_refs {
            if !seen.insert(refer.sha256.clone()) {
                continue;
            }
            if let Some((media_type, size)) = store.artifact_meta(&refer.sha256)? {
                out.push(BundleArtifact {
                    sha256: refer.sha256.clone(),
                    media_type,
                    size: size.max(0) as u64,
                });
            } else {
                out.push(BundleArtifact {
                    sha256: refer.sha256.clone(),
                    media_type: refer
                        .media_type
                        .clone()
                        .unwrap_or_else(|| "application/octet-stream".into()),
                    size: refer.size,
                });
            }
        }
    }
    out.sort_by(|a, b| a.sha256.cmp(&b.sha256));
    Ok(out)
}

fn claim_policy_id(claim: &ClaimDocument) -> Option<String> {
    if let Some(pref) = &claim.policy_ref {
        return Some(pref.clone());
    }
    if let Some(body) = &claim.policy {
        if let Some(id) = &body.id {
            return Some(id.clone());
        }
        return Some(format!("P-{}", claim.id));
    }
    None
}

fn claim_state_sql(state: ClaimState) -> &'static str {
    match state {
        ClaimState::Draft => "draft",
        ClaimState::Frozen => "frozen",
        ClaimState::Retired => "retired",
    }
}

fn origin_kind_sql(kind: OriginKind) -> &'static str {
    match kind {
        OriginKind::Human => "human",
        OriginKind::Imported => "imported",
        OriginKind::Agent => "agent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meno_core::{
        seal_envelope, Observation, Provenance, Source, Trust, TrustBasis, TrustOrigin,
        TrustRelation,
    };

    fn write_claim(root: &Path, id: &str, statement: &str, state: &str) {
        let dir = root.join("claims");
        std::fs::create_dir_all(&dir).unwrap();
        let yaml = format!(
            "id: {id}\nstatement: \"{statement}\"\nstate: {state}\norigin:\n  kind: human\npolicy:\n  version: 1\n  requires: []\n  contradicted_by: []\n"
        );
        std::fs::write(dir.join(format!("{id}.yaml")), yaml).unwrap();
    }

    fn envelope_with_artifacts(
        subject_id: &str,
        artifact_refs: Vec<meno_core::ArtifactRef>,
        trust: Trust,
    ) -> Envelope {
        let mut env = Envelope {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            kind: "command.result".into(),
            subject_id: subject_id.into(),
            source: Source {
                name: "cmd".into(),
                version: None,
                argv: None,
                config_digest: None,
            },
            observations: vec![Observation {
                type_name: "command.exit".into(),
                fields: serde_json::json!({"exit_code": 0}),
            }],
            artifact_refs,
            captured_at: "2024-01-02T03:04:05Z".into(),
            provenance: Provenance {
                actor: None,
                producer: "meno-test".into(),
                producer_version: None,
                host: None,
                cwd: None,
            },
            integrity: None,
            trust,
            source_metadata: serde_json::json!({}),
        };
        seal_envelope(&mut env).unwrap();
        env
    }

    fn machine_trust() -> Trust {
        Trust {
            origin: TrustOrigin::Machine,
            reproducible: true,
            basis: TrustBasis::Observed,
            relation: TrustRelation::Direct,
        }
    }

    fn seeded_store(root: &Path, trust: Trust) -> (Store, Envelope, String) {
        write_claim(root, "C1", "original statement", "frozen");
        let store = Store::open_project(&root.join(".meno")).unwrap();
        store.sync_contracts(root).unwrap();

        let subject_id = "ab".repeat(32);
        store.upsert_subject(&subject_id, 1, None, None).unwrap();
        let refer = store.put(b"meno bundle payload", "text/plain").unwrap();
        let env = envelope_with_artifacts(&subject_id, vec![refer.clone()], trust);
        store.insert_evidence(&env).unwrap();
        store
            .insert_claim_evidence("C1", &env.id, "support")
            .unwrap();
        store
            .insert_verdict("C1", &subject_id, Verdict::Unknown, 1, 1, "{}")
            .unwrap();
        (store, env, refer.sha256)
    }

    #[test]
    fn export_bundle_writes_directory_and_version() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        let (store, env, sha256) = seeded_store(&root, Trust::human_confirmation());
        let dest = tmp.path().join("bundle");
        let report = store.export_bundle(&dest, None).unwrap();

        assert_eq!(report.claims, 1);
        assert_eq!(report.envelopes, 1);
        assert_eq!(report.artifacts, 1);
        assert!(dest.join(BUNDLE_JSON).is_file());
        assert!(dest.join("artifacts").join(&sha256).is_file());
        assert_eq!(
            std::fs::read(dest.join("artifacts").join(&sha256)).unwrap(),
            b"meno bundle payload"
        );

        let text = std::fs::read_to_string(dest.join(BUNDLE_JSON)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["meno_bundle_version"], 1);
        assert_eq!(value["envelopes"][0]["id"], env.id);
        assert_eq!(value["envelopes"][0]["trust"]["origin"], "human");
    }

    #[test]
    fn import_bundle_round_trip_envelope_and_artifact() {
        let tmp = tempfile::tempdir().unwrap();
        let src_root = tmp.path().join("src");
        let (store, env, sha256) = seeded_store(&src_root, Trust::human_confirmation());
        let dest = tmp.path().join("bundle");
        store.export_bundle(&dest, None).unwrap();

        let dest_root = tmp.path().join("dst");
        let imported = Store::open_project(&dest_root.join(".meno")).unwrap();
        let report = imported.import_bundle(&dest).unwrap();
        assert_eq!(report.envelopes, 1);
        assert_eq!(report.artifacts, 1);

        let loaded = imported.load_evidence().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, env.id);
        assert_eq!(loaded[0].trust.origin, env.trust.origin);
        assert_eq!(loaded[0].trust.origin, TrustOrigin::Human);
        assert_eq!(imported.get(&sha256).unwrap(), b"meno bundle payload");
    }

    #[test]
    fn import_bundle_is_additive_and_does_not_weaken_frozen_claim() {
        let tmp = tempfile::tempdir().unwrap();
        let src_root = tmp.path().join("src");
        let (store, _, _) = seeded_store(&src_root, Trust::human_confirmation());
        let dest = tmp.path().join("bundle");
        store.export_bundle(&dest, None).unwrap();

        let dest_root = tmp.path().join("dst");
        let imported = Store::open_project(&dest_root.join(".meno")).unwrap();
        imported.import_bundle(&dest).unwrap();
        imported.import_bundle(&dest).unwrap();

        let claims = imported.load_claims().unwrap();
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].document.statement, "original statement");
        assert_eq!(claims[0].document.state, ClaimState::Frozen);

        let tampered_dir = tmp.path().join("tampered");
        copy_dir(&dest, &tampered_dir);
        let json_path = tampered_dir.join(BUNDLE_JSON);
        let mut value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
        value["claims"][0]["statement"] = serde_json::json!("weakened statement");
        std::fs::write(&json_path, serde_json::to_string_pretty(&value).unwrap()).unwrap();

        imported.import_bundle(&tampered_dir).unwrap();
        let claims = imported.load_claims().unwrap();
        assert_eq!(claims[0].document.statement, "original statement");
        assert_eq!(claims[0].document.state, ClaimState::Frozen);
    }

    #[test]
    fn bundle_json_has_no_in_toto_or_slsa() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        let (store, _, _) = seeded_store(&root, Trust::human_confirmation());
        let dest = tmp.path().join("bundle");
        store.export_bundle(&dest, None).unwrap();
        let text = std::fs::read_to_string(dest.join(BUNDLE_JSON)).unwrap();
        let lower = text.to_ascii_lowercase();
        assert!(
            !lower.contains("in-toto") && !lower.contains("intoto"),
            "{text}"
        );
        assert!(!lower.contains("slsa"), "{text}");
    }

    #[test]
    fn import_skips_tampered_trust_without_resealing() {
        let tmp = tempfile::tempdir().unwrap();
        let src_root = tmp.path().join("src");
        let (store, env, _) = seeded_store(&src_root, machine_trust());
        assert_eq!(env.trust.origin, TrustOrigin::Machine);
        let dest = tmp.path().join("bundle");
        store.export_bundle(&dest, None).unwrap();

        let json_path = dest.join(BUNDLE_JSON);
        let mut value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
        value["envelopes"][0]["trust"]["origin"] = serde_json::json!("human");
        std::fs::write(&json_path, serde_json::to_string_pretty(&value).unwrap()).unwrap();

        let dest_root = tmp.path().join("dst");
        let imported = Store::open_project(&dest_root.join(".meno")).unwrap();
        let result = imported.import_bundle(&dest);
        match result {
            Ok(report) => {
                assert!(
                    report.skipped >= 1,
                    "expected skipped envelope, got {report:?}"
                );
                let loaded = imported.load_evidence().unwrap();
                assert!(
                    loaded.iter().all(|e| e.id != env.id),
                    "tampered envelope must not be stored: {loaded:?}"
                );
                assert!(loaded.iter().all(|e| e.trust.origin != TrustOrigin::Human));
            }
            Err(err) => {
                let msg = err.to_string().to_ascii_lowercase();
                assert!(
                    msg.contains("integrity") || msg.contains("mismatch") || msg.contains("trust"),
                    "{err}"
                );
                assert!(imported.load_evidence().unwrap().is_empty());
            }
        }
    }

    fn copy_dir(src: &Path, dest: &Path) {
        std::fs::create_dir_all(dest).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let to = dest.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &to);
            } else {
                std::fs::copy(entry.path(), to).unwrap();
            }
        }
    }

    #[test]
    fn export_refuses_non_empty_dest() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        write_claim(&root, "C1", "stmt", "draft");
        let store = Store::open_project(&root.join(".meno")).unwrap();
        store.sync_contracts(&root).unwrap();
        let dest = tmp.path().join("bundle");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("already"), b"nope").unwrap();
        let err = store.export_bundle(&dest, None).unwrap_err();
        assert!(err.to_string().contains("not empty"), "{err}");
    }

    #[test]
    fn export_single_claim_without_evidence_still_writes_claim() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        write_claim(&root, "C1", "stmt", "draft");
        let store = Store::open_project(&root.join(".meno")).unwrap();
        store.sync_contracts(&root).unwrap();
        let dest = tmp.path().join("bundle");
        let report = store.export_bundle(&dest, Some("C1")).unwrap();
        assert_eq!(report.claims, 1);
        assert_eq!(report.envelopes, 0);
        let text = std::fs::read_to_string(dest.join(BUNDLE_JSON)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["claims"][0]["id"], "C1");
        assert_eq!(value["envelopes"].as_array().unwrap().len(), 0);
        assert!(!value["policies"].as_array().unwrap().is_empty());
    }
}
