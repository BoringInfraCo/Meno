use crate::db::{now_iso8601, Store};
use crate::error::{Result, StoreError};
use meno_core::{verify_envelope, Envelope, Evaluation, Verdict};
use rusqlite::{params, OptionalExtension};
use std::collections::BTreeSet;

impl Store {
    /// INSERT OR IGNORE a content-addressed subject.
    pub fn upsert_subject(
        &self,
        id: &str,
        identity_version: u32,
        origin: Option<&str>,
        head: Option<&str>,
    ) -> Result<()> {
        let id = normalize_subject_id(id)?;
        let now = now_iso8601()?;
        self.conn.execute(
            "INSERT OR IGNORE INTO subjects (
                id, identity_version, origin, head, captured_at, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![id, identity_version, origin, head, now],
        )?;
        Ok(())
    }

    /// Verify then insert evidence, observations, and evidence_artifacts.
    /// There is no update API (a trigger already blocks updates).
    pub fn insert_evidence(&self, env: &Envelope) -> Result<()> {
        verify_envelope(env)?;
        let integrity = env
            .integrity
            .as_ref()
            .ok_or_else(|| StoreError::Contract("envelope is missing integrity".into()))?;

        let subject_exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM subjects WHERE id = ?1)",
            params![env.subject_id],
            |row| row.get(0),
        )?;
        if !subject_exists {
            return Err(StoreError::NotFound(format!("subject {}", env.subject_id)));
        }

        let mut artifact_ids = BTreeSet::new();
        for artifact in &env.artifact_refs {
            artifact_ids.insert(artifact.sha256.clone());
        }
        for sha256 in &artifact_ids {
            let exists: bool = self.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM artifacts WHERE sha256 = ?1)",
                params![sha256],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(StoreError::NotFound(format!("artifact {sha256}")));
            }
        }

        let envelope_json = serde_json::to_string(env)?;
        let now = now_iso8601()?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO evidence (
                id, kind, subject_id, envelope_json, integrity_alg, integrity_digest,
                captured_at, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                env.id,
                env.kind,
                env.subject_id,
                envelope_json,
                integrity.alg,
                integrity.digest,
                env.captured_at,
                now
            ],
        )?;

        for (ordinal, obs) in env.observations.iter().enumerate() {
            let fields_json = serde_json::to_string(&obs.fields)?;
            tx.execute(
                "INSERT INTO observations (evidence_id, ordinal, \"type\", fields_json)
                 VALUES (?1, ?2, ?3, ?4)",
                params![env.id, ordinal as i64, obs.type_name, fields_json],
            )?;
        }

        for sha256 in &artifact_ids {
            tx.execute(
                "INSERT INTO evidence_artifacts (evidence_id, sha256) VALUES (?1, ?2)",
                params![env.id, sha256],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Parse stored `envelope_json` rows.
    pub fn load_evidence(&self) -> Result<Vec<Envelope>> {
        let mut stmt = self
            .conn
            .prepare("SELECT envelope_json FROM evidence ORDER BY created_at, id")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    pub fn insert_claim_evidence(
        &self,
        claim_id: &str,
        evidence_id: &str,
        relation: &str,
    ) -> Result<()> {
        match relation {
            "support" | "contradict" | "related" => {}
            other => {
                return Err(StoreError::Contract(format!(
                    "claim_evidence relation must be support|contradict|related, got {other:?}"
                )));
            }
        }
        let now = now_iso8601()?;
        self.conn.execute(
            "INSERT INTO claim_evidence (claim_id, evidence_id, relation, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![claim_id, evidence_id, relation, now],
        )?;
        Ok(())
    }

    pub fn insert_verdict(
        &self,
        claim_id: &str,
        subject_id: &str,
        verdict: Verdict,
        evaluation_version: u32,
        subject_identity_version: u32,
        explanation_json: &str,
    ) -> Result<()> {
        let now = now_iso8601()?;
        self.conn.execute(
            "INSERT INTO verdicts (
                claim_id, subject_id, verdict, evaluation_version,
                subject_identity_version, explanation_json, evaluated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                claim_id,
                subject_id,
                verdict_sql(verdict),
                evaluation_version,
                subject_identity_version,
                explanation_json,
                now
            ],
        )?;
        Ok(())
    }

    /// Persist an evaluation as a verdict row, serializing explanation JSON.
    pub fn insert_evaluation(&self, eval: &Evaluation) -> Result<()> {
        let missing: Vec<serde_json::Value> = eval
            .missing
            .iter()
            .map(|m| {
                serde_json::json!({
                    "kind": m.kind,
                    "detail": m.detail,
                })
            })
            .collect();
        let explanation_json = serde_json::json!({
            "supporting": eval.supporting,
            "contradicting": eval.contradicting,
            "stale": eval.stale,
            "missing": missing,
            "conflict": eval.conflict,
        })
        .to_string();
        self.insert_verdict(
            &eval.claim_id,
            &eval.subject_id,
            eval.verdict,
            eval.evaluation_version,
            eval.subject_identity_version,
            &explanation_json,
        )
    }

    pub fn latest_verdict(
        &self,
        claim_id: &str,
        subject_id: &str,
    ) -> Result<Option<(String, String)>> {
        self.conn
            .query_row(
                "SELECT verdict, explanation_json FROM verdicts
                 WHERE claim_id = ?1 AND subject_id = ?2
                 ORDER BY id DESC
                 LIMIT 1",
                params![claim_id, subject_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(StoreError::from)
    }

    /// All subjects: id, identity_version, origin, head.
    pub fn list_subjects(&self) -> Result<Vec<SubjectRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, identity_version, origin, head FROM subjects ORDER BY id")?;
        let rows = stmt.query_map([], |row| {
            Ok(SubjectRow {
                id: row.get(0)?,
                identity_version: row.get(1)?,
                origin: row.get(2)?,
                head: row.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Latest verdict row per (claim_id, subject_id).
    pub fn load_latest_verdicts(&self) -> Result<Vec<VerdictRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT v.claim_id, v.subject_id, v.verdict, v.evaluation_version,
                    v.subject_identity_version, v.explanation_json
             FROM verdicts v
             INNER JOIN (
                SELECT claim_id, subject_id, MAX(id) AS max_id
                FROM verdicts
                GROUP BY claim_id, subject_id
             ) latest ON latest.max_id = v.id
             ORDER BY v.claim_id, v.subject_id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u32>(3)?,
                row.get::<_, u32>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (
                claim_id,
                subject_id,
                verdict,
                evaluation_version,
                subject_identity_version,
                explanation_json,
            ) = row?;
            out.push(VerdictRow {
                claim_id,
                subject_id,
                verdict: parse_verdict_sql(&verdict)?,
                evaluation_version,
                subject_identity_version,
                explanation_json,
            });
        }
        Ok(out)
    }

    /// Envelopes linked to `claim_id` via `claim_evidence`.
    pub fn load_evidence_for_claim(&self, claim_id: &str) -> Result<Vec<Envelope>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.envelope_json
             FROM claim_evidence ce
             JOIN evidence e ON e.id = ce.evidence_id
             WHERE ce.claim_id = ?1
             ORDER BY e.created_at, e.id",
        )?;
        let rows = stmt.query_map(params![claim_id], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    pub(crate) fn evidence_exists(&self, id: &str) -> Result<bool> {
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM evidence WHERE id = ?1)",
            params![id],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    pub(crate) fn artifact_row_exists(&self, sha256: &str) -> Result<bool> {
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM artifacts WHERE sha256 = ?1)",
            params![sha256],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    pub(crate) fn claim_exists(&self, id: &str) -> Result<bool> {
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM claims WHERE id = ?1)",
            params![id],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    pub(crate) fn subject_exists(&self, id: &str) -> Result<bool> {
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM subjects WHERE id = ?1)",
            params![id],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    pub(crate) fn policy_exists(&self, id: &str) -> Result<bool> {
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM policies WHERE id = ?1)",
            params![id],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    pub(crate) fn list_artifact_meta(&self) -> Result<Vec<(String, String, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT sha256, media_type, size FROM artifacts ORDER BY sha256")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub(crate) fn artifact_meta(&self, sha256: &str) -> Result<Option<(String, i64)>> {
        self.conn
            .query_row(
                "SELECT media_type, size FROM artifacts WHERE sha256 = ?1",
                params![sha256],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(StoreError::from)
    }
}

/// Subject row used by bundle export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectRow {
    pub id: String,
    pub identity_version: u32,
    pub origin: Option<String>,
    pub head: Option<String>,
}

/// Latest verdict row used by bundle export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerdictRow {
    pub claim_id: String,
    pub subject_id: String,
    pub verdict: Verdict,
    pub evaluation_version: u32,
    pub subject_identity_version: u32,
    pub explanation_json: String,
}

fn normalize_subject_id(id: &str) -> Result<String> {
    let s = id.trim().to_ascii_lowercase();
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(StoreError::Contract(
            "subject id must be 64 hex characters".into(),
        ));
    }
    Ok(s)
}

pub(crate) fn verdict_sql(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Proven => "proven",
        Verdict::Disproven => "disproven",
        Verdict::Unknown => "unknown",
    }
}

fn parse_verdict_sql(s: &str) -> Result<Verdict> {
    match s {
        "proven" => Ok(Verdict::Proven),
        "disproven" => Ok(Verdict::Disproven),
        "unknown" => Ok(Verdict::Unknown),
        other => Err(StoreError::Contract(format!("invalid verdict {other:?}"))),
    }
}
