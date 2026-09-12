use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

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
    fs::write(dir.path().join("README.md"), "hello\n").unwrap();
    git(dir.path(), &["add", "README.md"]);
    git(dir.path(), &["commit", "-m", "init", "--no-gpg-sign"]);
    dir
}

fn meno(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_meno"))
        .current_dir(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("spawn meno")
}

fn assert_ok(output: &Output, ctx: &str) {
    assert!(
        output.status.success(),
        "{ctx} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn parse_json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "json parse error: {err}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn claim_by_id<'a>(report: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    report["claims"]
        .as_array()
        .expect("claims array")
        .iter()
        .find(|claim| claim["id"] == id)
        .unwrap_or_else(|| panic!("missing claim {id} in {report}"))
}

fn missing_kinds(claim: &serde_json::Value) -> Vec<String> {
    claim["missing"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|item| item["kind"].as_str().map(str::to_string))
        .collect()
}

const MIXED_CLAIM: &str = r#"id: C-mixed
statement: "The unit command, browser checks, and a human review all pass."
state: frozen
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
    - kind: playwright.result
      match:
        unexpected: 0
      min_count: 1
      subject_bound: true
    - kind: human.confirmation
      min_count: 1
      subject_bound: true
  contradicted_by: []
  freshness:
    subject_match: exact
"#;

const PW_ONLY_CLAIM: &str = r#"id: C-pw-only
statement: "Playwright reports no unexpected results."
state: frozen
origin:
  kind: human
policy:
  version: 1
  requires:
    - kind: playwright.result
      match:
        unexpected: 0
      min_count: 1
      subject_bound: true
  contradicted_by:
    - kind: playwright.result
      match:
        unexpected:
          neq: 0
      min_count: 1
      subject_bound: true
  freshness:
    subject_match: exact
"#;

const COMMAND_TOML: &str = r#"[meno]
version = 1
subject_identity_version = 1

[[adapters.command]]
name = "unit"
argv = ["true"]
can_invoke = true
side_effect_level = "none"
"#;

const PLAYWRIGHT_STATUS_TOML: &str = r#"[meno]
version = 1
subject_identity_version = 1

[[adapters.playwright]]
name = "e2e"
path = "pw.json"
"#;

#[test]
fn mixed_evidence_command_playwright_human() {
    let dir = init_repo();
    let root = dir.path();

    let init = meno(root, &["init"]);
    assert_ok(&init, "meno init");

    fs::remove_file(root.join("claims/C-example.yaml")).unwrap();
    fs::write(root.join("claims/C-mixed.yaml"), MIXED_CLAIM).unwrap();
    fs::write(root.join("meno.toml"), COMMAND_TOML).unwrap();
    fs::write(
        root.join("playwright-pass.json"),
        fixture("playwright-pass.json"),
    )
    .unwrap();

    let verify = meno(
        root,
        &["verify", "--from", "playwright-pass.json", "--json"],
    );
    assert_ok(&verify, "meno verify mixed without confirm");
    let report = parse_json(&verify);
    let claim = claim_by_id(&report, "C-mixed");
    assert_eq!(
        claim["verdict"], "unknown",
        "mixed claim without human confirmation must be unknown: {claim}"
    );
    let missing = missing_kinds(claim);
    assert!(
        missing.iter().any(|kind| kind == "human.confirmation"),
        "missing should include human.confirmation: {claim}"
    );

    let confirm = meno(
        root,
        &[
            "inspect",
            "C-mixed",
            "--confirm",
            "--statement",
            "Looks correct on mobile",
            "--actor",
            "tester",
        ],
    );
    assert_ok(&confirm, "meno inspect --confirm");

    let status = meno(root, &["status", "--json"]);
    assert_ok(&status, "meno status after confirm");
    let status_report = parse_json(&status);
    let status_claim = claim_by_id(&status_report, "C-mixed");
    assert_eq!(
        status_claim["verdict"], "proven",
        "command + playwright + human should prove C-mixed: {status_claim}"
    );

    let inspect = meno(root, &["inspect", "C-mixed"]);
    assert_ok(&inspect, "meno inspect mixed");
    let inspect_out = stdout(&inspect);
    assert!(
        inspect_out.contains("meno.playwright"),
        "inspect should mention meno.playwright: {inspect_out}"
    );
    assert!(
        inspect_out.contains("meno.human"),
        "inspect should mention meno.human: {inspect_out}"
    );
    assert!(
        !inspect_out.to_ascii_lowercase().contains("inferred"),
        "confirm must not use inferred visual as human: {inspect_out}"
    );
    assert!(
        !inspect_out.contains("meno.visual"),
        "confirm must not record inferred visual: {inspect_out}"
    );

    fs::write(root.join("README.md"), "hello dirty\n").unwrap();
    let dirty = meno(root, &["status", "--json"]);
    assert_ok(&dirty, "meno status after dirty tree");
    let dirty_report = parse_json(&dirty);
    let dirty_claim = claim_by_id(&dirty_report, "C-mixed");
    assert_eq!(
        dirty_claim["verdict"], "unknown",
        "dirty tree should stale mixed evidence: {dirty_claim}"
    );
    let stale = dirty_claim["stale"].as_array().cloned().unwrap_or_default();
    assert!(
        !stale.is_empty(),
        "expected stale evidence after dirty tree: {dirty_claim}"
    );
}

#[test]
fn playwright_fail_is_not_proven() {
    let dir = init_repo();
    let root = dir.path();
    assert_ok(&meno(root, &["init"]), "meno init");

    fs::remove_file(root.join("claims/C-example.yaml")).unwrap();
    fs::write(root.join("claims/C-pw-only.yaml"), PW_ONLY_CLAIM).unwrap();
    fs::write(
        root.join("playwright-fail.json"),
        fixture("playwright-fail.json"),
    )
    .unwrap();

    let fail = meno(
        root,
        &["verify", "--from", "playwright-fail.json", "--json"],
    );
    assert_ok(&fail, "meno verify failing playwright");
    let report = parse_json(&fail);
    let claim = claim_by_id(&report, "C-pw-only");
    assert_ne!(
        claim["verdict"], "proven",
        "fail fixture must not prove unexpected=0: {claim}"
    );
    assert_eq!(
        claim["verdict"], "disproven",
        "unexpected neq 0 should disprove C-pw-only: {claim}"
    );
}

#[test]
fn status_does_not_ingest_configured_playwright() {
    let dir = init_repo();
    let root = dir.path();
    assert_ok(&meno(root, &["init"]), "meno init");

    fs::remove_file(root.join("claims/C-example.yaml")).unwrap();
    fs::write(root.join("claims/C-pw-only.yaml"), PW_ONLY_CLAIM).unwrap();
    fs::write(root.join("meno.toml"), PLAYWRIGHT_STATUS_TOML).unwrap();
    fs::write(root.join("pw.json"), fixture("playwright-pass.json")).unwrap();

    let status = meno(root, &["status", "--json"]);
    assert_ok(&status, "meno status before verify");
    let status_report = parse_json(&status);
    let status_claim = claim_by_id(&status_report, "C-pw-only");
    assert_ne!(
        status_claim["verdict"], "proven",
        "status must not ingest configured playwright: {status_claim}"
    );
    let missing = missing_kinds(status_claim);
    assert!(
        missing.iter().any(|kind| kind == "playwright.result"),
        "status JSON should be missing playwright evidence: {status_claim}"
    );

    let verify = meno(root, &["verify", "--json"]);
    assert_ok(&verify, "meno verify configured playwright");
    let verify_report = parse_json(&verify);
    let verify_claim = claim_by_id(&verify_report, "C-pw-only");
    assert_eq!(
        verify_claim["verdict"], "proven",
        "verify should ingest configured playwright.json: {verify_claim}"
    );
}
