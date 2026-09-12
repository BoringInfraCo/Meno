use std::fs;
use std::path::{Path, PathBuf};

use meno_adapters::collect_snapshot;
use meno_core::{
    evaluate, seal_envelope, subject_id_hex, ClaimDocument, ClaimState, Envelope, Evaluation,
    MissingEvidence, Origin, OriginKind, PolicyBody, PolicyDocument, TrustOrigin, Verdict,
    SUBJECT_IDENTITY_VERSION,
};
use meno_store::Store;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{McpError, Result};
use crate::MENO_MCP_VERSION;

/// Same schema version as CLI `JsonReport` (`meno status --json`).
const JSON_VERSION: u32 = 1;

const READ_TOOLS: &[(&str, &str)] = &[
    (
        "get_verification_state",
        "Current verification state for the active subject (same as `meno status --json`)",
    ),
    ("list_claims", "List claims and current verdicts"),
    ("get_claim", "Get one claim by id"),
    ("get_evidence", "List evidence or fetch one envelope by id"),
    ("inspect_verdict", "Explain the current verdict for a claim"),
];

const WRITE_TOOLS: &[(&str, &str)] = &[
    (
        "propose_claim",
        "Propose a draft claim (cannot freeze or overwrite frozen claims)",
    ),
    ("submit_evidence", "Submit a machine evidence envelope"),
    (
        "request_evaluation",
        "Re-evaluate claims without invoking adapters",
    ),
];

const FORBIDDEN_TOOLS: &[&str] = &[
    "freeze_claim",
    "retire_claim",
    "delete_evidence",
    "update_policy",
    "set_claim_state",
    "rewrite_provenance",
];

pub struct ToolSpec {
    pub name: String,
    pub description: String,
}

pub struct McpEngine {
    root: PathBuf,
    subject_id: String,
    store: Store,
}

impl McpEngine {
    pub fn open(start: &Path) -> Result<Self> {
        let root = find_project_root(start)?;
        let cfg_path = root.join("meno.toml");
        if !cfg_path.is_file() {
            return Err(McpError::msg(format!(
                "no meno.toml in {}; run `meno init`",
                root.display()
            )));
        }
        let config: FileConfig = toml::from_str(&fs::read_to_string(&cfg_path)?)
            .map_err(|err| McpError::msg(format!("toml: {err}")))?;
        if config.meno.version != 1 {
            return Err(McpError::msg(format!(
                "unsupported meno.toml version {}",
                config.meno.version
            )));
        }
        let store = Store::open_project(&root.join(".meno"))?;
        store.sync_contracts(&root)?;
        let snapshot = collect_snapshot(&root)?;
        let subject_id = subject_id_hex(&snapshot);
        let origin = String::from_utf8_lossy(&snapshot.origin);
        let head = String::from_utf8_lossy(&snapshot.head);
        store.upsert_subject(
            &subject_id,
            SUBJECT_IDENTITY_VERSION,
            nonempty(&origin),
            nonempty(&head),
        )?;
        Ok(Self {
            root,
            subject_id,
            store,
        })
    }

    pub fn open_from(start: &Path) -> Result<Self> {
        Self::open(start)
    }

    pub fn list_tools() -> Vec<ToolSpec> {
        READ_TOOLS
            .iter()
            .chain(WRITE_TOOLS)
            .map(|(name, description)| ToolSpec {
                name: (*name).to_string(),
                description: (*description).to_string(),
            })
            .collect()
    }

    pub fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        if FORBIDDEN_TOOLS.contains(&name) {
            return Err(McpError::authority(name));
        }
        match name {
            "get_verification_state" | "request_evaluation" => self.verification_state(),
            "list_claims" => Ok(json!({ "claims": self.evaluate_claims()? })),
            "get_claim" => self.get_claim(&arguments),
            "get_evidence" => self.get_evidence(&arguments),
            "inspect_verdict" => self.inspect_verdict(&arguments),
            "propose_claim" => self.propose_claim(&arguments),
            "submit_evidence" => self.submit_evidence(&arguments),
            other => Err(McpError::msg(format!("unknown tool `{other}`"))),
        }
    }

    fn verification_state(&self) -> Result<Value> {
        let claims = self.evaluate_claims()?;
        Ok(json!({
            "meno_cli_json_version": JSON_VERSION,
            "meno_mcp_version": MENO_MCP_VERSION,
            "subject_id": self.subject_id,
            "claims": claims,
        }))
    }

    fn evaluate_claims(&self) -> Result<Vec<Value>> {
        Ok(self
            .evaluations()?
            .into_iter()
            .map(|(doc, eval)| json_claim(&doc, &eval))
            .collect())
    }

    fn evaluations(&self) -> Result<Vec<(ClaimDocument, Evaluation)>> {
        let evidence = self.store.load_evidence()?;
        let records = self.store.load_claims()?;
        let mut out = Vec::new();
        for record in records {
            if record.document.state == ClaimState::Retired {
                continue;
            }
            let policy = policy_for(&self.store, &record.document)?;
            let evaluation = evaluate(&record.document.id, &policy, &self.subject_id, &evidence);
            out.push((record.document, evaluation));
        }
        Ok(out)
    }

    fn get_claim(&self, arguments: &Value) -> Result<Value> {
        let id = required_str(arguments, "id")?;
        self.evaluations()?
            .into_iter()
            .find(|(doc, _)| doc.id == id)
            .map(|(doc, eval)| json_claim(&doc, &eval))
            .ok_or_else(|| McpError::msg(format!("unknown claim {id}")))
    }

    fn get_evidence(&self, arguments: &Value) -> Result<Value> {
        let envelopes = self.store.load_evidence()?;
        if let Some(id) = arguments.get("id").and_then(Value::as_str) {
            return envelopes
                .into_iter()
                .find(|env| env.id == id)
                .map(|env| serde_json::to_value(env).map_err(McpError::from))
                .transpose()?
                .ok_or_else(|| McpError::msg(format!("unknown evidence {id}")));
        }
        Ok(json!({ "evidence": envelopes }))
    }

    fn inspect_verdict(&self, arguments: &Value) -> Result<Value> {
        let id = arguments
            .get("claim_id")
            .or_else(|| arguments.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| McpError::msg("claim_id is required"))?;
        let (doc, eval) = self
            .evaluations()?
            .into_iter()
            .find(|(doc, _)| doc.id == id)
            .ok_or_else(|| McpError::msg(format!("unknown claim {id}")))?;
        Ok(json!({
            "claim_id": doc.id,
            "verdict": eval.verdict,
            "why": why_verdict(&eval),
            "conflict": eval.conflict,
        }))
    }

    fn propose_claim(&mut self, arguments: &Value) -> Result<Value> {
        let id = required_str(arguments, "id")?.to_string();
        let statement = required_str(arguments, "statement")?.to_string();
        let claims_dir = self.root.join("claims");
        fs::create_dir_all(&claims_dir)?;
        let path = claims_dir.join(format!("{id}.yaml"));
        if path.is_file() {
            let existing = ClaimDocument::from_yaml(&fs::read_to_string(&path)?)?;
            if existing.state == ClaimState::Frozen {
                return Err(McpError::msg(format!("cannot overwrite frozen claim {id}")));
            }
        }
        let policy = match arguments.get("policy") {
            Some(value) if !value.is_null() => serde_json::from_value::<PolicyBody>(value.clone())?,
            _ => {
                return Err(McpError::msg(
                    "propose_claim requires a policy body; freezing is not permitted over MCP",
                ));
            }
        };
        let doc = ClaimDocument {
            id: id.clone(),
            statement,
            state: ClaimState::Draft,
            origin: Origin {
                kind: OriginKind::Agent,
                actor: None,
                source: None,
            },
            policy: Some(policy),
            policy_ref: None,
        };
        doc.validate()?;
        fs::write(&path, serde_yaml::to_string(&doc)?)?;
        self.store.sync_contracts(&self.root)?;
        Ok(serde_json::to_value(&doc)?)
    }

    fn submit_evidence(&mut self, arguments: &Value) -> Result<Value> {
        let raw = arguments
            .get("envelope")
            .cloned()
            .ok_or_else(|| McpError::msg("submit_evidence requires envelope"))?;
        let mut env: Envelope = serde_json::from_value(raw)?;
        if env.kind == "human.confirmation" || env.trust.origin == TrustOrigin::Human {
            return Err(McpError::msg(
                "human confirmation is not permitted over MCP; use `meno inspect --confirm`",
            ));
        }
        if env.subject_id.is_empty() {
            env.subject_id = self.subject_id.clone();
            env.integrity = None;
        }
        if env.integrity.is_none() {
            seal_envelope(&mut env)?;
        }
        self.store.insert_evidence(&env)?;
        Ok(serde_json::to_value(&env)?)
    }
}

fn json_claim(claim: &ClaimDocument, eval: &Evaluation) -> Value {
    json!({
        "id": claim.id,
        "statement": claim.statement,
        "state": claim.state,
        "verdict": eval.verdict,
        "supporting": eval.supporting,
        "contradicting": eval.contradicting,
        "stale": eval.stale,
        "missing": missing_json(&eval.missing),
        "conflict": eval.conflict,
    })
}

fn missing_json(missing: &[MissingEvidence]) -> Value {
    Value::Array(
        missing
            .iter()
            .map(|item| {
                json!({
                    "kind": item.kind,
                    "detail": item.detail,
                })
            })
            .collect(),
    )
}

fn why_verdict(eval: &Evaluation) -> String {
    match eval.verdict {
        Verdict::Proven => {
            "policy requires are satisfied by fresh supporting evidence with no contradiction"
                .into()
        }
        Verdict::Disproven => {
            "fresh contradicting evidence applies and requires are not satisfied".into()
        }
        Verdict::Unknown if eval.conflict => {
            "supporting and contradicting evidence both apply".into()
        }
        Verdict::Unknown => "insufficient fresh evidence".into(),
    }
}

fn policy_for(store: &Store, claim: &ClaimDocument) -> Result<PolicyDocument> {
    if let Some(policy) = store.load_policy_for_claim(&claim.id)? {
        return Ok(policy);
    }
    match claim.resolved_policy()? {
        Some(policy) => Ok(policy),
        None => Err(McpError::msg(format!("no policy for claim {}", claim.id))),
    }
}

fn required_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| McpError::msg(format!("{key} is required")))
}

fn find_project_root(start: &Path) -> Result<PathBuf> {
    let mut dir = fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
    loop {
        if dir.join("meno.toml").is_file() {
            return Ok(dir);
        }
        let git = dir.join(".git");
        if git.is_dir() || git.is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(McpError::msg(
                "could not find meno.toml or a git repository; run `meno init` inside a git work tree",
            ));
        }
    }
}

fn nonempty(s: &str) -> Option<&str> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[derive(Deserialize)]
struct FileConfig {
    meno: MenoSection,
}

#[derive(Deserialize)]
struct MenoSection {
    version: u32,
}
