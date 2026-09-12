use crate::db::{insert_audit_event, now_iso8601, Store};
use crate::error::{Result, StoreError};
use meno_core::{ClaimDocument, ClaimState, Origin, OriginKind, PolicyDocument};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Result of syncing claim/policy YAML files into SQLite.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub claims_upserted: usize,
    pub policies_upserted: usize,
    pub revisions_recorded: usize,
}

/// A claim loaded from the store, with `policy_ref` set from `claims.policy_id`.
#[derive(Debug, Clone)]
pub struct ClaimRecord {
    pub document: ClaimDocument,
    pub revision: i64,
}

struct PreparedSync {
    claims: Vec<(ClaimDocument, String)>,
    policies: Vec<PolicyDocument>,
}

impl Store {
    /// Load `project_root/claims/*.yaml` and `project_root/policies/*.yaml`.
    /// Upsert claims then policies. File edits of frozen claims are allowed
    /// (honest local trust) but MUST insert a new claim_revisions / policy_revisions
    /// row and append_audit_event when statement/state/policy body changes.
    /// Audit actions: "claim_proposed", "claim_changed", "claim_frozen", "claim_retired", "policy_changed"
    pub fn sync_contracts(&self, project_root: &Path) -> Result<SyncReport> {
        let prepared = prepare_sync(project_root)?;
        let tx = self.conn.unchecked_transaction()?;
        let mut report = SyncReport::default();
        for (claim, policy_id) in &prepared.claims {
            upsert_claim(&tx, claim, policy_id, &mut report)?;
        }
        for policy in &prepared.policies {
            upsert_policy(&tx, policy, &mut report)?;
        }
        tx.commit()?;
        Ok(report)
    }

    pub fn load_claims(&self) -> Result<Vec<ClaimRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, statement, state, origin_kind, origin_actor, origin_source,
                    policy_id, current_revision
             FROM claims
             ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (
                id,
                statement,
                state,
                origin_kind,
                origin_actor,
                origin_source,
                policy_id,
                revision,
            ) = row?;
            out.push(ClaimRecord {
                document: ClaimDocument {
                    id,
                    statement,
                    state: parse_claim_state(&state)?,
                    origin: Origin {
                        kind: parse_origin_kind(&origin_kind)?,
                        actor: origin_actor,
                        source: origin_source,
                    },
                    policy: None,
                    policy_ref: policy_id,
                },
                revision,
            });
        }
        Ok(out)
    }

    pub fn load_policy(&self, policy_id: &str) -> Result<Option<PolicyDocument>> {
        load_policy_conn(&self.conn, policy_id)
    }

    pub fn load_policies(&self) -> Result<Vec<PolicyDocument>> {
        let mut stmt = self
            .conn
            .prepare("SELECT body_json FROM policies ORDER BY id")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    pub fn load_policy_for_claim(&self, claim_id: &str) -> Result<Option<PolicyDocument>> {
        let policy_id: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT policy_id FROM claims WHERE id = ?1",
                params![claim_id],
                |row| row.get(0),
            )
            .optional()?;
        match policy_id {
            Some(Some(id)) => load_policy_conn(&self.conn, &id),
            Some(None) | None => Ok(None),
        }
    }
}

fn load_policy_conn(conn: &Connection, policy_id: &str) -> Result<Option<PolicyDocument>> {
    let body_json: Option<String> = conn
        .query_row(
            "SELECT body_json FROM policies WHERE id = ?1",
            params![policy_id],
            |row| row.get(0),
        )
        .optional()?;
    match body_json {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

fn prepare_sync(project_root: &Path) -> Result<PreparedSync> {
    let claim_files = list_yaml_files(&project_root.join("claims"))?;
    let policy_files = list_yaml_files(&project_root.join("policies"))?;

    let mut policies_by_id: BTreeMap<String, PolicyDocument> = BTreeMap::new();
    let mut policies_by_stem: BTreeMap<String, String> = BTreeMap::new();
    for path in &policy_files {
        let text = std::fs::read_to_string(path)?;
        let doc = PolicyDocument::from_yaml(&text).map_err(|e| {
            StoreError::Contract(format!("failed to parse {}: {e}", path.display()))
        })?;
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            policies_by_stem.insert(stem.to_string(), doc.id.clone());
        }
        if policies_by_id.insert(doc.id.clone(), doc.clone()).is_some() {
            return Err(StoreError::Contract(format!(
                "duplicate policy id {}",
                doc.id
            )));
        }
    }

    let mut seen_claims = BTreeSet::new();
    let mut claims = Vec::new();
    for path in &claim_files {
        let text = std::fs::read_to_string(path)?;
        let claim = ClaimDocument::from_yaml(&text).map_err(|e| {
            StoreError::Contract(format!("failed to parse {}: {e}", path.display()))
        })?;
        if !seen_claims.insert(claim.id.clone()) {
            return Err(StoreError::Contract(format!(
                "duplicate claim id {}",
                claim.id
            )));
        }

        let policy_id = if let Some(body) = &claim.policy {
            let pol = PolicyDocument::from_body(&claim.id, body.clone())?;
            let id = pol.id.clone();
            policies_by_id.insert(id.clone(), pol);
            id
        } else if let Some(pref) = &claim.policy_ref {
            if policies_by_id.contains_key(pref) {
                pref.clone()
            } else if let Some(id) = policies_by_stem.get(pref) {
                id.clone()
            } else {
                return Err(StoreError::NotFound(format!(
                    "policy {pref} referenced by claim {} not found",
                    claim.id
                )));
            }
        } else {
            return Err(StoreError::Contract(format!(
                "claim {} must set policy or policy_ref",
                claim.id
            )));
        };
        claims.push((claim, policy_id));
    }

    let policies = policies_by_id.into_values().collect();
    Ok(PreparedSync { claims, policies })
}

fn list_yaml_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err.into()),
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        if is_yaml_path(&path) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn is_yaml_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase()),
        Some(ext) if ext == "yaml" || ext == "yml"
    )
}

struct ExistingClaim {
    statement: String,
    state: String,
    origin_kind: String,
    origin_actor: Option<String>,
    origin_source: Option<String>,
    policy_id: Option<String>,
    current_revision: i64,
}

fn upsert_claim(
    conn: &Connection,
    claim: &ClaimDocument,
    policy_id: &str,
    report: &mut SyncReport,
) -> Result<()> {
    let now = now_iso8601()?;
    let state = claim_state_sql(claim.state);
    let origin_kind = origin_kind_sql(claim.origin.kind);
    let origin_actor = claim.origin.actor.as_deref();
    let origin_source = claim.origin.source.as_deref();

    let existing: Option<ExistingClaim> = conn
        .query_row(
            "SELECT statement, state, origin_kind, origin_actor, origin_source, policy_id, current_revision
             FROM claims WHERE id = ?1",
            params![claim.id],
            |row| {
                Ok(ExistingClaim {
                    statement: row.get(0)?,
                    state: row.get(1)?,
                    origin_kind: row.get(2)?,
                    origin_actor: row.get(3)?,
                    origin_source: row.get(4)?,
                    policy_id: row.get(5)?,
                    current_revision: row.get(6)?,
                })
            },
        )
        .optional()?;

    match existing {
        None => {
            conn.execute(
                "INSERT INTO claims (
                    id, statement, state, origin_kind, origin_actor, origin_source,
                    policy_id, current_revision, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?8)",
                params![
                    claim.id,
                    claim.statement,
                    state,
                    origin_kind,
                    origin_actor,
                    origin_source,
                    policy_id,
                    now
                ],
            )?;
            insert_claim_revision(conn, claim, policy_id, 1, &now)?;
            report.revisions_recorded += 1;
            let payload = revision_payload(1)?;
            insert_audit_event(
                conn,
                "claim_proposed",
                None,
                Some("claim"),
                Some(&claim.id),
                Some(&payload),
            )?;
            if claim.state == ClaimState::Frozen {
                insert_audit_event(
                    conn,
                    "claim_frozen",
                    None,
                    Some("claim"),
                    Some(&claim.id),
                    Some(&payload),
                )?;
            }
        }
        Some(old) => {
            let changed = old.statement != claim.statement
                || old.state != state
                || old.origin_kind != origin_kind
                || old.origin_actor.as_deref() != origin_actor
                || old.origin_source.as_deref() != origin_source
                || old.policy_id.as_deref() != Some(policy_id);
            if changed {
                let revision = old.current_revision + 1;
                conn.execute(
                    "UPDATE claims SET
                        statement = ?1,
                        state = ?2,
                        origin_kind = ?3,
                        origin_actor = ?4,
                        origin_source = ?5,
                        policy_id = ?6,
                        current_revision = ?7,
                        updated_at = ?8
                     WHERE id = ?9",
                    params![
                        claim.statement,
                        state,
                        origin_kind,
                        origin_actor,
                        origin_source,
                        policy_id,
                        revision,
                        now,
                        claim.id
                    ],
                )?;
                insert_claim_revision(conn, claim, policy_id, revision, &now)?;
                report.revisions_recorded += 1;
                let payload = revision_payload(revision)?;
                let action = claim_change_action(&old.state, claim.state);
                insert_audit_event(
                    conn,
                    action,
                    None,
                    Some("claim"),
                    Some(&claim.id),
                    Some(&payload),
                )?;
            }
        }
    }

    report.claims_upserted += 1;
    Ok(())
}

fn insert_claim_revision(
    conn: &Connection,
    claim: &ClaimDocument,
    policy_id: &str,
    revision: i64,
    now: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO claim_revisions (
            claim_id, revision, statement, state, origin_kind, origin_actor,
            origin_source, policy_id, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            claim.id,
            revision,
            claim.statement,
            claim_state_sql(claim.state),
            origin_kind_sql(claim.origin.kind),
            claim.origin.actor.as_deref(),
            claim.origin.source.as_deref(),
            policy_id,
            now
        ],
    )?;
    Ok(())
}

fn upsert_policy(
    conn: &Connection,
    policy: &PolicyDocument,
    report: &mut SyncReport,
) -> Result<()> {
    let claim_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM claims WHERE id = ?1)",
        params![policy.claim],
        |row| row.get(0),
    )?;
    if !claim_exists {
        return Err(StoreError::NotFound(format!(
            "claim {} for policy {} not found",
            policy.claim, policy.id
        )));
    }

    let now = now_iso8601()?;
    let body_json = serde_json::to_string(policy)?;
    let existing: Option<(i64, String, i64)> = conn
        .query_row(
            "SELECT version, body_json, current_revision FROM policies WHERE id = ?1",
            params![policy.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;

    match existing {
        None => {
            conn.execute(
                "INSERT INTO policies (
                    id, claim_id, version, body_json, current_revision, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
                params![policy.id, policy.claim, policy.version, body_json, now],
            )?;
            insert_policy_revision(conn, policy, 1, &body_json, &now)?;
            report.revisions_recorded += 1;
        }
        Some((_old_version, old_body, old_revision)) => {
            if !json_same(&old_body, &body_json) {
                let revision = old_revision + 1;
                conn.execute(
                    "UPDATE policies SET
                        claim_id = ?1,
                        version = ?2,
                        body_json = ?3,
                        current_revision = ?4,
                        updated_at = ?5
                     WHERE id = ?6",
                    params![
                        policy.claim,
                        policy.version,
                        body_json,
                        revision,
                        now,
                        policy.id
                    ],
                )?;
                insert_policy_revision(conn, policy, revision, &body_json, &now)?;
                report.revisions_recorded += 1;
                let payload = revision_payload(revision)?;
                insert_audit_event(
                    conn,
                    "policy_changed",
                    None,
                    Some("policy"),
                    Some(&policy.id),
                    Some(&payload),
                )?;
            }
        }
    }

    report.policies_upserted += 1;
    Ok(())
}

fn insert_policy_revision(
    conn: &Connection,
    policy: &PolicyDocument,
    revision: i64,
    body_json: &str,
    now: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO policy_revisions (policy_id, revision, version, body_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![policy.id, revision, policy.version, body_json, now],
    )?;
    Ok(())
}

fn claim_change_action(old_state: &str, new_state: ClaimState) -> &'static str {
    match new_state {
        ClaimState::Frozen if old_state != "frozen" => "claim_frozen",
        ClaimState::Retired if old_state != "retired" => "claim_retired",
        _ => "claim_changed",
    }
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

fn parse_claim_state(s: &str) -> Result<ClaimState> {
    match s {
        "draft" => Ok(ClaimState::Draft),
        "frozen" => Ok(ClaimState::Frozen),
        "retired" => Ok(ClaimState::Retired),
        other => Err(StoreError::Contract(format!(
            "invalid claim state {other:?}"
        ))),
    }
}

fn parse_origin_kind(s: &str) -> Result<OriginKind> {
    match s {
        "human" => Ok(OriginKind::Human),
        "imported" => Ok(OriginKind::Imported),
        "agent" => Ok(OriginKind::Agent),
        other => Err(StoreError::Contract(format!(
            "invalid origin kind {other:?}"
        ))),
    }
}

fn revision_payload(revision: i64) -> Result<String> {
    Ok(serde_json::to_string(
        &serde_json::json!({ "revision": revision }),
    )?)
}

fn json_same(a: &str, b: &str) -> bool {
    match (
        serde_json::from_str::<serde_json::Value>(a),
        serde_json::from_str::<serde_json::Value>(b),
    ) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}
