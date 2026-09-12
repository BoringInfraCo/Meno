use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use meno_adapters::{
    collect_snapshot, ingest_generic_envelope, looks_like_playwright_json, normalize_command,
    normalize_human_confirmation, normalize_junit, normalize_playwright, parse_junit_xml,
    parse_playwright_json, redact_output, run_command, CommandSpec, HumanConfirmation,
    PlaywrightReport, SideEffectLevel,
};
use meno_core::{
    evaluate, seal_envelope, subject_id_hex, verify_envelope, ClaimDocument, ClaimState, Envelope,
    Evaluation, MissingEvidence, Observation, PolicyDocument, Provenance, Source, Trust, Verdict,
    SUBJECT_IDENTITY_VERSION,
};
use meno_store::Store;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::config::{side_effect_slug, MenoConfig};
use crate::error::CliError;

pub const JSON_VERSION: u32 = 1;

pub struct ProjectContext {
    pub root: PathBuf,
    pub store: Store,
    pub subject_id: String,
    pub config: MenoConfig,
}

pub struct EvaluatedClaim {
    pub document: ClaimDocument,
    pub policy: PolicyDocument,
    pub evaluation: Evaluation,
}

#[derive(Serialize)]
pub struct JsonReport {
    pub meno_cli_json_version: u32,
    pub subject_id: String,
    pub claims: Vec<JsonClaim>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<JsonEvidence>,
}

#[derive(Serialize)]
pub struct JsonEvidence {
    pub id: String,
    pub kind: String,
    pub source: Source,
    pub provenance: Provenance,
    pub trust: Trust,
    pub captured_at: String,
    pub artifacts: Vec<String>,
    pub observations: Vec<JsonObservationSummary>,
}

#[derive(Serialize)]
pub struct JsonObservationSummary {
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(flatten)]
    pub fields: Map<String, Value>,
}

#[derive(Serialize)]
pub struct JsonClaim {
    pub id: String,
    pub statement: String,
    pub state: ClaimState,
    pub verdict: Verdict,
    pub supporting: Value,
    pub contradicting: Value,
    pub stale: Value,
    pub missing: Value,
    pub conflict: Value,
}

pub fn find_project_root(start: &Path) -> Result<PathBuf, CliError> {
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
            return Err(CliError::msg(
                "could not find meno.toml or a git repository; run `meno init` inside a git work tree",
            ));
        }
    }
}

pub fn require_git_work_tree(cwd: &Path) -> Result<(), CliError> {
    let output = git_output(cwd, &["rev-parse", "--is-inside-work-tree"])?;
    if !output.status.success() {
        return Err(CliError::msg(
            "meno init must be run inside a git work tree",
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim() != "true" {
        return Err(CliError::msg(
            "meno init must be run inside a git work tree",
        ));
    }
    Ok(())
}

pub fn git_toplevel(cwd: &Path) -> Result<PathBuf, CliError> {
    let output = git_output(cwd, &["rev-parse", "--show-toplevel"])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::msg(format!(
            "not a git work tree: {}",
            stderr.trim()
        )));
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return Err(CliError::msg("git toplevel was empty"));
    }
    Ok(PathBuf::from(path))
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<std::process::Output, CliError> {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|err| CliError::msg(format!("failed to run git: {err}")))
}

pub fn short_subject_id(subject_id: &str) -> &str {
    if subject_id.len() >= 12 {
        &subject_id[..12]
    } else {
        subject_id
    }
}

impl ProjectContext {
    pub fn open() -> Result<Self, CliError> {
        let cwd = std::env::current_dir()?;
        Self::open_from(&cwd)
    }

    pub fn open_from(start: &Path) -> Result<Self, CliError> {
        let root = find_project_root(start)?;
        let cfg_path = root.join("meno.toml");
        if !cfg_path.is_file() {
            return Err(CliError::msg(format!(
                "no meno.toml in {}; run `meno init`",
                root.display()
            )));
        }
        let config = MenoConfig::load(&cfg_path)?;
        if config.meno.version != 1 {
            return Err(CliError::msg(format!(
                "unsupported meno.toml version {}",
                config.meno.version
            )));
        }
        if config.meno.subject_identity_version != SUBJECT_IDENTITY_VERSION {
            eprintln!(
                "warning: meno.toml subject_identity_version {} != runtime {SUBJECT_IDENTITY_VERSION}",
                config.meno.subject_identity_version
            );
        }
        let store = Store::open_project(&root.join(".meno"))?;
        store.sync_contracts(&root)?;
        if let Ok(top) = git_toplevel(&root) {
            let top_c = fs::canonicalize(&top).unwrap_or(top.clone());
            let root_c = fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
            if top_c != root_c {
                eprintln!(
                    "warning: git worktree is {} but meno.toml is in {}. Subject identity includes the entire git worktree.",
                    top_c.display(),
                    root_c.display()
                );
            }
        }
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
            store,
            subject_id,
            config,
        })
    }

    pub fn invoke_commands(&self, confirm_invoke: bool) -> Result<(), CliError> {
        for spec in self.config.command_specs() {
            invoke_one(
                &self.store,
                &spec,
                &self.root,
                &self.subject_id,
                confirm_invoke,
            )?;
        }
        Ok(())
    }

    pub fn ingest_from_paths(&self, paths: &[PathBuf]) -> Result<(), CliError> {
        for path in paths {
            let resolved = resolve_from_path(&self.root, path);
            if !resolved.is_file() {
                eprintln!("warning: --from {} not found; skipping", path.display());
                continue;
            }
            if let Err(err) =
                ingest_file(&self.store, &self.root, &self.subject_id, &resolved, "from")
            {
                eprintln!("warning: --from {}: {err}", path.display());
            }
        }
        Ok(())
    }

    pub fn ingest_configured_junit(&self) -> Result<(), CliError> {
        for adapter in &self.config.adapters.junit {
            let path = if adapter.path.is_absolute() {
                adapter.path.clone()
            } else {
                self.root.join(&adapter.path)
            };
            if !path.is_file() {
                continue;
            }
            if let Err(err) = ingest_file(
                &self.store,
                &self.root,
                &self.subject_id,
                &path,
                &adapter.name,
            ) {
                eprintln!(
                    "warning: junit adapter `{}` at {}: {err}",
                    adapter.name,
                    adapter.path.display()
                );
            }
        }
        Ok(())
    }

    pub fn ingest_configured_playwright(&self) -> Result<(), CliError> {
        for adapter in &self.config.adapters.playwright {
            let path = if adapter.path.is_absolute() {
                adapter.path.clone()
            } else {
                self.root.join(&adapter.path)
            };
            if !path.is_file() {
                continue;
            }
            if let Err(err) = ingest_file(
                &self.store,
                &self.root,
                &self.subject_id,
                &path,
                &adapter.name,
            ) {
                eprintln!(
                    "warning: playwright adapter `{}` at {}: {err}",
                    adapter.name,
                    adapter.path.display()
                );
            }
        }
        Ok(())
    }

    pub fn insert_human_confirmation(
        &self,
        statement: &str,
        actor: Option<&str>,
        artifact_path: Option<&Path>,
    ) -> Result<(), CliError> {
        let artifact = match artifact_path {
            Some(path) => {
                let resolved = resolve_from_path(&self.root, path);
                if !resolved.is_file() {
                    return Err(CliError::msg(format!(
                        "artifact {} not found",
                        path.display()
                    )));
                }
                let bytes = fs::read(&resolved)?;
                Some(self.store.put(&bytes, media_type_for(&resolved))?)
            }
            None => None,
        };
        let input = HumanConfirmation {
            statement: statement.to_string(),
            actor: actor.map(str::to_string),
            artifact,
        };
        let envelope = normalize_human_confirmation(&input, &self.subject_id)?;
        persist_evidence(&self.store, &envelope)
    }

    pub fn evaluate_claims(&self) -> Result<Vec<EvaluatedClaim>, CliError> {
        let evidence = self.store.load_evidence()?;
        let records = self.store.load_claims()?;
        let mut out = Vec::new();
        for record in records {
            if record.document.state == ClaimState::Retired {
                continue;
            }
            let policy = policy_for(&self.store, &record.document)?;
            let evaluation = evaluate(&record.document.id, &policy, &self.subject_id, &evidence);
            persist_evaluation(
                &self.store,
                &record.document.id,
                &self.subject_id,
                &evaluation,
            )?;
            out.push(EvaluatedClaim {
                document: record.document,
                policy,
                evaluation,
            });
        }
        Ok(out)
    }
}

fn policy_for(store: &Store, claim: &ClaimDocument) -> Result<PolicyDocument, CliError> {
    if let Some(policy) = store.load_policy_for_claim(&claim.id)? {
        return Ok(policy);
    }
    match claim.resolved_policy()? {
        Some(policy) => Ok(policy),
        None => Err(CliError::msg(format!("no policy for claim {}", claim.id))),
    }
}

fn persist_evaluation(
    store: &Store,
    claim_id: &str,
    subject_id: &str,
    evaluation: &Evaluation,
) -> Result<(), CliError> {
    let explanation = json!({
        "supporting": evaluation.supporting,
        "contradicting": evaluation.contradicting,
        "stale": evaluation.stale,
        "missing": missing_json(&evaluation.missing),
        "conflict": evaluation.conflict,
    })
    .to_string();
    store.insert_verdict(
        claim_id,
        subject_id,
        evaluation.verdict,
        evaluation.evaluation_version,
        evaluation.subject_identity_version,
        &explanation,
    )?;
    for id in &evaluation.supporting {
        insert_relation(store, claim_id, id, "support")?;
    }
    for id in &evaluation.contradicting {
        insert_relation(store, claim_id, id, "contradict")?;
    }
    Ok(())
}

fn insert_relation(
    store: &Store,
    claim_id: &str,
    evidence_id: &str,
    relation: &str,
) -> Result<(), CliError> {
    match store.insert_claim_evidence(claim_id, evidence_id, relation) {
        Ok(()) => Ok(()),
        Err(err) => {
            let msg = err.to_string();
            if msg.to_ascii_lowercase().contains("unique")
                || msg.to_ascii_lowercase().contains("constraint")
                || msg.to_ascii_lowercase().contains("primary")
            {
                Ok(())
            } else {
                Err(err.into())
            }
        }
    }
}

fn invoke_one(
    store: &Store,
    spec: &CommandSpec,
    cwd: &Path,
    subject_id: &str,
    confirm_invoke: bool,
) -> Result<(), CliError> {
    if spec.side_effect_level == SideEffectLevel::Consequential {
        eprintln!(
            "skipping consequential command `{}`; collect evidence externally",
            spec.name
        );
        return Ok(());
    }
    if !spec.invoke_allowed(confirm_invoke) {
        return Ok(());
    }
    if spec.side_effect_level != SideEffectLevel::None {
        eprintln!(
            "invoking {} with side_effect_level={}",
            spec.name,
            side_effect_slug(spec.side_effect_level)
        );
    }
    if spec.argv.is_empty() {
        eprintln!(
            "command adapter `{}` has empty argv; skipping invoke",
            spec.name
        );
        return Ok(());
    }
    let outcome = match run_command(spec, cwd) {
        Ok(outcome) => outcome,
        Err(err) => {
            eprintln!("command adapter `{}` failed to run: {err}", spec.name);
            return Ok(());
        }
    };
    let stdout = redact_output(&outcome.stdout);
    let stderr = redact_output(&outcome.stderr);
    let stdout_ref = store.put(&stdout, "text/plain")?;
    let stderr_ref = store.put(&stderr, "text/plain")?;
    let envelope = normalize_command(
        spec,
        &outcome,
        subject_id,
        cwd,
        Some(stdout_ref),
        Some(stderr_ref),
    )?;
    persist_evidence(store, &envelope)
}

fn resolve_from_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() || path.is_file() {
        return path.to_path_buf();
    }
    let from_root = root.join(path);
    if from_root.is_file() {
        from_root
    } else {
        path.to_path_buf()
    }
}

fn ingest_file(
    store: &Store,
    root: &Path,
    subject_id: &str,
    path: &Path,
    source_name: &str,
) -> Result<(), CliError> {
    let bytes = fs::read(path)?;
    if looks_like_xml(path, &bytes) {
        ingest_junit(store, subject_id, &bytes, source_name)
    } else if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        let value: Value = serde_json::from_slice(&bytes)?;
        if looks_like_playwright_json(&value) {
            ingest_playwright(store, root, path, subject_id, &bytes, source_name)
        } else {
            ingest_generic(store, root, path, subject_id, &bytes)
        }
    } else {
        Err(CliError::msg(format!(
            "unsupported evidence format {}",
            path.display()
        )))
    }
}

fn looks_like_xml(path: &Path, bytes: &[u8]) -> bool {
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
    {
        return true;
    }
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    bytes[start..].starts_with(b"<")
}

fn ingest_junit(
    store: &Store,
    subject_id: &str,
    bytes: &[u8],
    source_name: &str,
) -> Result<(), CliError> {
    let report_ref = store.put(bytes, "application/xml")?;
    let report = parse_junit_xml(bytes)?;
    let envelope = normalize_junit(&report, subject_id, source_name, Some(report_ref))?;
    persist_evidence(store, &envelope)
}

fn ingest_playwright(
    store: &Store,
    root: &Path,
    json_path: &Path,
    subject_id: &str,
    bytes: &[u8],
    source_name: &str,
) -> Result<(), CliError> {
    let report_ref = store.put(bytes, "application/json")?;
    let report = parse_playwright_json(bytes)?;
    let mut envelope = normalize_playwright(&report, subject_id, source_name, Some(report_ref))?;
    put_playwright_attachments(store, root, json_path, &report, &mut envelope)?;
    persist_evidence(store, &envelope)
}

fn put_playwright_attachments(
    store: &Store,
    root: &Path,
    json_path: &Path,
    report: &PlaywrightReport,
    envelope: &mut Envelope,
) -> Result<(), CliError> {
    let json_dir = json_path.parent().unwrap_or(root);
    let mut changed = false;
    for test in &report.tests {
        for attachment in &test.attachments {
            let Some(rel) = attachment.path.as_deref().filter(|p| !p.is_empty()) else {
                continue;
            };
            let rel_path = Path::new(rel);
            let candidates = [
                root.join(rel_path),
                json_dir.join(rel_path),
                rel_path.to_path_buf(),
            ];
            let Some(path) = candidates.iter().find(|path| path.is_file()) else {
                continue;
            };
            let fallback = media_type_for(path);
            let media = attachment.content_type.as_deref().unwrap_or(fallback);
            let data = fs::read(path)?;
            let stored = store.put(&data, media)?;
            if envelope
                .artifact_refs
                .iter()
                .any(|artifact| artifact.sha256 == stored.sha256)
            {
                continue;
            }
            envelope.artifact_refs.push(stored);
            changed = true;
        }
    }
    if changed {
        envelope.integrity = None;
        seal_envelope(envelope)?;
    }
    Ok(())
}

fn media_type_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("json") => "application/json",
        Some("zip") => "application/zip",
        Some("webm") => "video/webm",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
}

fn ingest_generic(
    store: &Store,
    root: &Path,
    json_path: &Path,
    subject_id: &str,
    bytes: &[u8],
) -> Result<(), CliError> {
    let mut envelope = ingest_generic_envelope(bytes, subject_id)?;
    if envelope.subject_id != subject_id {
        envelope.subject_id = subject_id.to_string();
        envelope.integrity = None;
        seal_envelope(&mut envelope)?;
    }
    put_generic_artifacts(store, root, json_path, &mut envelope)?;
    persist_evidence(store, &envelope)
}

fn put_generic_artifacts(
    store: &Store,
    root: &Path,
    json_path: &Path,
    envelope: &mut Envelope,
) -> Result<(), CliError> {
    if envelope.artifact_refs.is_empty() {
        return Ok(());
    }
    let json_dir = json_path.parent().unwrap_or(root);
    let mut kept = Vec::new();
    let mut changed = false;
    for artifact in envelope.artifact_refs.drain(..) {
        if store.get(&artifact.sha256).is_ok() {
            kept.push(artifact);
            continue;
        }
        let media = artifact
            .media_type
            .as_deref()
            .unwrap_or("application/octet-stream");
        let candidates = [
            root.join(&artifact.relative_path),
            json_dir.join(&artifact.relative_path),
            PathBuf::from(&artifact.relative_path),
        ];
        match candidates.iter().find(|path| path.is_file()) {
            Some(path) => {
                let data = fs::read(path)?;
                let stored = store.put(&data, media)?;
                if stored.sha256 != artifact.sha256 {
                    changed = true;
                    kept.push(stored);
                } else {
                    kept.push(artifact);
                }
            }
            None => {
                changed = true;
            }
        }
    }
    envelope.artifact_refs = kept;
    if changed {
        envelope.integrity = None;
        seal_envelope(envelope)?;
    }
    Ok(())
}

fn persist_evidence(store: &Store, envelope: &Envelope) -> Result<(), CliError> {
    verify_envelope(envelope)?;
    store.insert_evidence(envelope)?;
    store.append_audit_event(
        "evidence.ingested",
        Some("meno"),
        Some("evidence"),
        Some(&envelope.id),
        None,
    )?;
    Ok(())
}

pub fn json_report(subject_id: &str, claims: &[EvaluatedClaim]) -> JsonReport {
    JsonReport {
        meno_cli_json_version: JSON_VERSION,
        subject_id: subject_id.to_string(),
        claims: claims.iter().map(json_claim).collect(),
        evidence: Vec::new(),
    }
}

pub fn json_inspect_report(
    subject_id: &str,
    claim: &EvaluatedClaim,
    envelopes: &[Envelope],
) -> JsonReport {
    let mut report = json_report(subject_id, std::slice::from_ref(claim));
    report.evidence = collect_claim_evidence(claim, envelopes);
    report
}

pub fn print_inspect_json(
    subject_id: &str,
    claim: &EvaluatedClaim,
    envelopes: &[Envelope],
) -> Result<(), CliError> {
    let report = json_inspect_report(subject_id, claim, envelopes);
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub fn collect_claim_evidence(claim: &EvaluatedClaim, envelopes: &[Envelope]) -> Vec<JsonEvidence> {
    let mut by_id = HashMap::new();
    for env in envelopes {
        by_id.insert(env.id.as_str(), env);
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for id in claim
        .evaluation
        .supporting
        .iter()
        .chain(&claim.evaluation.contradicting)
        .chain(&claim.evaluation.stale)
    {
        if !seen.insert(id.as_str()) {
            continue;
        }
        if let Some(env) = by_id.get(id.as_str()) {
            out.push(json_evidence_detail(env));
        }
    }
    out
}

pub fn json_evidence_detail(env: &Envelope) -> JsonEvidence {
    JsonEvidence {
        id: env.id.clone(),
        kind: env.kind.clone(),
        source: env.source.clone(),
        provenance: env.provenance.clone(),
        trust: env.trust,
        captured_at: env.captured_at.clone(),
        artifacts: env
            .artifact_refs
            .iter()
            .map(|artifact| artifact.sha256.clone())
            .collect(),
        observations: env.observations.iter().map(compact_observation).collect(),
    }
}

pub const COMPACT_OBS_KEYS: &[&str] = &[
    "status",
    "name",
    "classname",
    "failed",
    "errors",
    "skipped",
    "exit_code",
    "message",
    "url",
    "route",
    "title",
    "unexpected",
    "statement",
    "actor",
];

pub fn compact_observation(obs: &Observation) -> JsonObservationSummary {
    let mut fields = Map::new();
    if let Value::Object(map) = &obs.fields {
        for key in COMPACT_OBS_KEYS {
            if let Some(value) = map.get(*key) {
                fields.insert((*key).to_string(), compact_field_value(key, value));
            }
        }
    }
    JsonObservationSummary {
        type_name: obs.type_name.clone(),
        fields,
    }
}

fn compact_field_value(key: &str, value: &Value) -> Value {
    if key == "message" {
        if let Some(text) = value.as_str() {
            return Value::String(truncate_chars(text, 80));
        }
    }
    value.clone()
}

fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn json_claim(claim: &EvaluatedClaim) -> JsonClaim {
    let eval = &claim.evaluation;
    JsonClaim {
        id: claim.document.id.clone(),
        statement: claim.document.statement.clone(),
        state: claim.document.state,
        verdict: eval.verdict,
        supporting: value_or(&eval.supporting, json!([])),
        contradicting: value_or(&eval.contradicting, json!([])),
        stale: value_or(&eval.stale, json!([])),
        missing: missing_json(&eval.missing),
        conflict: value_or(&eval.conflict, json!(false)),
    }
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

fn value_or<T: Serialize>(v: &T, fallback: Value) -> Value {
    serde_json::to_value(v).unwrap_or(fallback)
}

pub fn print_json(subject_id: &str, claims: &[EvaluatedClaim]) -> Result<(), CliError> {
    let report = json_report(subject_id, claims);
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub fn verdict_label(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Proven => "Proven",
        Verdict::Disproven => "Disproven",
        Verdict::Unknown => "Unknown",
    }
}

pub fn print_claim_list(claims: &[EvaluatedClaim]) {
    let mut proven = 0u32;
    let mut disproven = 0u32;
    let mut unknown = 0u32;
    for claim in claims {
        match claim.evaluation.verdict {
            Verdict::Proven => proven += 1,
            Verdict::Disproven => disproven += 1,
            Verdict::Unknown => unknown += 1,
        }
    }
    println!("{proven} proven · {disproven} disproven · {unknown} unknown");
    println!();
    for claim in claims {
        println!(
            "{}  {}",
            claim.document.id,
            verdict_label(claim.evaluation.verdict)
        );
        println!("  {}", claim.document.statement);
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

pub fn conflict_set<T: Serialize>(conflict: &T) -> bool {
    match serde_json::to_value(conflict).ok() {
        Some(Value::Bool(b)) => b,
        Some(Value::Null) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        Some(_) => true,
        None => false,
    }
}

pub fn why_verdict(eval: &Evaluation) -> String {
    match eval.verdict {
        Verdict::Proven => {
            "policy requires are satisfied by fresh supporting evidence with no contradiction"
                .into()
        }
        Verdict::Disproven => {
            "fresh contradicting evidence applies and requires are not satisfied".into()
        }
        Verdict::Unknown if conflict_set(&eval.conflict) => {
            "supporting and contradicting evidence both apply".into()
        }
        Verdict::Unknown => "insufficient fresh evidence".into(),
    }
}
