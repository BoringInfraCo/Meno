use std::fs;
use std::path::Path;
use std::process::Command;

use meno_core::{
    evaluate, new_ulid, ClaimDocument, ClaimState, Envelope, Observation, OriginKind, Provenance,
    Source, Trust, TrustBasis, TrustOrigin, TrustRelation, Verdict,
};
use meno_store::Store;
use serde_json::{json, Value};
use tempfile::TempDir;

use crate::engine::McpEngine;
use crate::stdio::handle_request;
use crate::{McpError, MENO_MCP_VERSION};

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "Meno Test"]);
    git(dir.path(), &["config", "user.email", "meno@example.com"]);
    git(dir.path(), &["config", "commit.gpgsign", "false"]);
    git(dir.path(), &["config", "core.autocrlf", "false"]);
    fs::write(dir.path().join("README.md"), "hello\n").expect("readme");
    git(dir.path(), &["add", "README.md"]);
    git(dir.path(), &["commit", "-m", "init", "--no-gpg-sign"]);
    dir
}

fn write_toml(root: &Path) {
    fs::write(
        root.join("meno.toml"),
        r#"[meno]
version = 1
subject_identity_version = 1

[[adapters.command]]
name = "unit"
argv = ["sh", "-c", "touch SENTINEL"]
can_invoke = true
side_effect_level = "none"
"#,
    )
    .expect("meno.toml");
}

fn claim_yaml(id: &str, statement: &str, state: &str) -> String {
    format!(
        r#"id: {id}
statement: "{statement}"
state: {state}
origin:
  kind: human
policy:
  version: 1
  requires:
    - kind: command.result
      match:
        exit_code: 0
      min_count: 1
      subject_bound: true
  contradicted_by: []
  freshness:
    subject_match: exact
"#
    )
}

fn write_claim(root: &Path, id: &str, statement: &str, state: &str) {
    let dir = root.join("claims");
    fs::create_dir_all(&dir).expect("claims dir");
    fs::write(
        dir.join(format!("{id}.yaml")),
        claim_yaml(id, statement, state),
    )
    .expect("claim");
}

fn machine_trust() -> Trust {
    Trust {
        origin: TrustOrigin::Machine,
        reproducible: true,
        basis: TrustBasis::Observed,
        relation: TrustRelation::Direct,
    }
}

fn command_result_envelope(subject_id: &str, exit_code: i64) -> Envelope {
    Envelope {
        id: new_ulid(),
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
            fields: json!({ "exit_code": exit_code }),
        }],
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
        trust: machine_trust(),
        source_metadata: json!({}),
    }
}

fn open_project(root: &Path) -> McpEngine {
    write_toml(root);
    McpEngine::open(root).expect("open mcp engine")
}

fn call(engine: &mut McpEngine, name: &str, arguments: Value) -> Value {
    engine
        .call_tool(name, arguments)
        .unwrap_or_else(|err| panic!("{name} failed: {err}"))
}

fn call_err(engine: &mut McpEngine, name: &str, arguments: Value) -> McpError {
    engine
        .call_tool(name, arguments)
        .expect_err("expected tool error")
}

#[test]
fn get_verification_state_returns_version_and_matches_evaluate() {
    let dir = init_repo();
    let root = dir.path();
    write_claim(root, "C17", "The unit command exits 0.", "frozen");
    let mut engine = open_project(root);

    let state = call(&mut engine, "get_verification_state", json!({}));
    assert_eq!(state["meno_mcp_version"], MENO_MCP_VERSION);
    assert_eq!(state["meno_mcp_version"], 1);
    assert_eq!(state["claims"][0]["id"], "C17");
    assert_eq!(state["claims"][0]["verdict"], "unknown");

    let subject_id = state["subject_id"]
        .as_str()
        .expect("subject_id")
        .to_string();
    let store = Store::open_project(&root.join(".meno")).expect("store");
    let policy = store
        .load_policy_for_claim("C17")
        .expect("policy")
        .expect("policy present");
    let empty = evaluate("C17", &policy, &subject_id, &[]);
    assert_eq!(empty.verdict, Verdict::Unknown);

    let mut sealed = command_result_envelope(&subject_id, 0);
    meno_core::seal_envelope(&mut sealed).expect("seal");
    store.insert_evidence(&sealed).expect("insert evidence");

    let proven = evaluate("C17", &policy, &subject_id, std::slice::from_ref(&sealed));
    assert_eq!(proven.verdict, Verdict::Proven);

    let state = call(&mut engine, "get_verification_state", json!({}));
    assert_eq!(state["meno_mcp_version"], 1);
    assert_eq!(state["claims"][0]["verdict"], "proven");
    assert_eq!(state["claims"][0]["supporting"][0], sealed.id, "{state}");
}

#[test]
fn propose_claim_creates_draft_and_rejects_frozen() {
    let dir = init_repo();
    let root = dir.path();
    write_claim(root, "C17", "Frozen contract.", "frozen");
    let mut engine = open_project(root);

    let created = call(
        &mut engine,
        "propose_claim",
        json!({
            "id": "C-agent",
            "statement": "Agent proposed this.",
            "policy": {
                "requires": [{
                    "kind": "command.result",
                    "match": { "exit_code": 0 },
                    "min_count": 1,
                    "subject_bound": true
                }],
                "contradicted_by": []
            }
        }),
    );
    assert_eq!(created["id"], "C-agent");
    assert_eq!(created["state"], "draft");
    assert_eq!(created["origin"]["kind"], "agent");

    let path = root.join("claims/C-agent.yaml");
    let yaml = fs::read_to_string(&path).expect("draft yaml");
    let doc = ClaimDocument::from_yaml(&yaml).expect("parse draft");
    assert_eq!(doc.id, "C-agent");
    assert_eq!(doc.state, ClaimState::Draft);
    assert_eq!(doc.origin.kind, OriginKind::Agent);
    assert!(yaml.contains("state: draft"));
    assert!(!yaml.contains("state: frozen"));

    let frozen_path = root.join("claims/C17.yaml");
    let before = fs::read_to_string(&frozen_path).expect("frozen yaml");
    let err = call_err(
        &mut engine,
        "propose_claim",
        json!({
            "id": "C17",
            "statement": "should not overwrite frozen",
        }),
    );
    let err_text = err.to_string();
    assert!(
        err_text.contains("frozen") || err_text.contains("authority"),
        "{err_text}"
    );
    let after = fs::read_to_string(&frozen_path).expect("frozen yaml after");
    assert_eq!(before, after);
    let frozen = ClaimDocument::from_yaml(&after).expect("parse frozen");
    assert_eq!(frozen.state, ClaimState::Frozen);
    assert_eq!(frozen.statement, "Frozen contract.");
}

#[test]
fn submit_evidence_rejects_human_confirmation() {
    let dir = init_repo();
    let root = dir.path();
    write_claim(root, "C17", "The unit command exits 0.", "draft");
    let mut engine = open_project(root);
    let state = call(&mut engine, "get_verification_state", json!({}));
    let subject_id = state["subject_id"].as_str().expect("subject_id");

    let mut human = command_result_envelope(subject_id, 0);
    human.kind = "human.confirmation".into();
    human.trust = Trust::human_confirmation();
    let err = call_err(&mut engine, "submit_evidence", json!({ "envelope": human }));
    let err_text = err.to_string();
    assert!(
        err_text.contains("human") || err_text.to_ascii_lowercase().contains("mcp"),
        "{err_text}"
    );

    let mut human_trust = command_result_envelope("", 0);
    human_trust.kind = "command.result".into();
    human_trust.trust.origin = TrustOrigin::Human;
    let err = call_err(
        &mut engine,
        "submit_evidence",
        json!({ "envelope": human_trust }),
    );
    assert!(err.to_string().contains("human"), "{}", err);

    let listed = call(&mut engine, "get_evidence", json!({}));
    let evidence = listed["evidence"].as_array().expect("evidence array");
    assert!(evidence.is_empty(), "{listed}");
}

#[test]
fn submit_evidence_accepts_command_result_envelope() {
    let dir = init_repo();
    let root = dir.path();
    write_claim(root, "C17", "The unit command exits 0.", "frozen");
    let mut engine = open_project(root);

    let mut env = command_result_envelope("", 0);
    env.kind = "command.result".into();
    let submitted = call(&mut engine, "submit_evidence", json!({ "envelope": env }));
    assert_eq!(submitted["kind"], "command.result");
    assert!(!submitted["id"].as_str().unwrap_or("").is_empty());
    assert_eq!(submitted["subject_id"].as_str().map(str::len), Some(64));

    let listed = call(&mut engine, "get_evidence", json!({}));
    assert_eq!(listed["evidence"][0]["kind"], "command.result");
    assert_eq!(listed["evidence"][0]["id"], submitted["id"]);

    let by_id = call(
        &mut engine,
        "get_evidence",
        json!({ "id": submitted["id"] }),
    );
    assert_eq!(by_id["kind"], "command.result");
    assert_eq!(by_id["id"], submitted["id"]);
}

#[test]
fn freeze_claim_is_authority_error() {
    let dir = init_repo();
    let root = dir.path();
    write_claim(root, "C17", "The unit command exits 0.", "draft");
    let mut engine = open_project(root);

    for name in [
        "freeze_claim",
        "retire_claim",
        "delete_evidence",
        "update_policy",
        "set_claim_state",
        "rewrite_provenance",
    ] {
        let err = call_err(&mut engine, name, json!({ "id": "C17" }));
        let text = err.to_string();
        assert!(
            text.contains("authority"),
            "expected authority error for {name}, got {text}"
        );
        assert!(
            matches!(err, McpError::Authority(ref tool) if tool == name),
            "{name}: {err:?}"
        );
    }

    let yaml = fs::read_to_string(root.join("claims/C17.yaml")).expect("claim");
    assert!(yaml.contains("state: draft"));
}

#[test]
fn request_evaluation_does_not_need_network() {
    let dir = init_repo();
    let root = dir.path();
    write_claim(root, "C17", "The unit command exits 0.", "frozen");
    let mut engine = open_project(root);

    let state = call(&mut engine, "request_evaluation", json!({}));
    assert_eq!(state["meno_mcp_version"], 1);
    assert_eq!(state["claims"][0]["verdict"], "unknown");
    assert!(
        !root.join("SENTINEL").exists(),
        "request_evaluation must not invoke adapters"
    );

    let mut env = command_result_envelope("", 0);
    env.kind = "command.result".into();
    call(&mut engine, "submit_evidence", json!({ "envelope": env }));

    let state = call(&mut engine, "request_evaluation", json!({}));
    assert_eq!(state["claims"][0]["verdict"], "proven");
    assert!(
        !root.join("SENTINEL").exists(),
        "request_evaluation must not invoke adapters after evidence submit"
    );

    let inspect = call(&mut engine, "inspect_verdict", json!({ "claim_id": "C17" }));
    assert_eq!(inspect["claim_id"], "C17");
    assert_eq!(inspect["verdict"], "proven");
    assert!(inspect["why"].as_str().is_some_and(|why| !why.is_empty()));
    assert_eq!(inspect["conflict"], false);
}

#[test]
fn json_rpc_initialize_and_forbidden_tool_is_error() {
    let dir = init_repo();
    let root = dir.path();
    write_claim(root, "C17", "The unit command exits 0.", "draft");
    let mut engine = open_project(root);

    let init = handle_request(
        &mut engine,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": "2024-11-05" }
        }),
    )
    .expect("initialize response");
    assert_eq!(init["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(init["result"]["serverInfo"]["name"], "meno");
    assert!(init["result"]["capabilities"]["tools"].is_object());

    assert!(handle_request(
        &mut engine,
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
    )
    .is_none());

    let listed = handle_request(
        &mut engine,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    )
    .expect("tools/list");
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"get_verification_state"));
    assert!(names.contains(&"propose_claim"));
    assert!(!names.contains(&"freeze_claim"));

    let freeze = handle_request(
        &mut engine,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": "freeze_claim", "arguments": {} }
        }),
    )
    .expect("tools/call freeze");
    assert_eq!(freeze["result"]["isError"], true);
    let text = freeze["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(text.contains("authority"), "{text}");
}

#[test]
fn list_tools_names_are_exact() {
    let names: Vec<String> = McpEngine::list_tools()
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    assert_eq!(
        names,
        vec![
            "get_verification_state",
            "list_claims",
            "get_claim",
            "get_evidence",
            "inspect_verdict",
            "propose_claim",
            "submit_evidence",
            "request_evaluation",
        ]
    );
}
