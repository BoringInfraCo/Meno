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

const JUNIT_CLAIM: &str = r#"id: C-junit
statement: "Unit tests report zero failures and errors."
state: frozen
origin:
  kind: human
policy:
  version: 1
  requires:
    - kind: junit.report
      match:
        failed: 0
        errors: 0
      min_count: 1
      subject_bound: true
  contradicted_by:
    - kind: junit.report
      match:
        failed:
          neq: 0
      min_count: 1
      subject_bound: true
  freshness:
    subject_match: exact
"#;

const GENERIC_CLAIM: &str = r#"id: C-generic
statement: "A generic envelope observation is ok."
state: frozen
origin:
  kind: human
policy:
  version: 1
  requires:
    - kind: generic.envelope
      match:
        ok: true
      min_count: 1
      subject_bound: true
  contradicted_by: []
  freshness:
    subject_match: exact
"#;

#[test]
fn junit_verify_from_and_inspect() {
    let dir = init_repo();
    let root = dir.path();

    let init = meno(root, &["init"]);
    assert_ok(&init, "meno init");

    fs::remove_file(root.join("claims/C-example.yaml")).unwrap();
    fs::write(root.join("claims/C-junit.yaml"), JUNIT_CLAIM).unwrap();
    fs::write(root.join("junit.xml"), fixture("junit-mixed.xml")).unwrap();

    let mixed = meno(root, &["verify", "--from", "junit.xml", "--json"]);
    assert_ok(&mixed, "meno verify mixed junit");
    let mixed_report = parse_json(&mixed);
    assert_eq!(mixed_report["meno_cli_json_version"], 1);
    let mixed_claim = claim_by_id(&mixed_report, "C-junit");
    assert_ne!(
        mixed_claim["verdict"], "proven",
        "mixed junit must not be proven: {mixed_claim}"
    );
    assert_eq!(
        mixed_claim["verdict"], "disproven",
        "mixed junit with contradicted_by failed neq 0: {mixed_claim}"
    );

    let inspect_mixed = meno(root, &["inspect", "C-junit"]);
    assert_ok(&inspect_mixed, "meno inspect mixed");
    let inspect_mixed_out = stdout(&inspect_mixed);
    assert!(
        inspect_mixed_out.contains("junit.case") || inspect_mixed_out.contains("status"),
        "inspect should mention junit.case or status: {inspect_mixed_out}"
    );
    assert!(
        inspect_mixed_out.contains("meno.junit"),
        "inspect should mention provenance meno.junit: {inspect_mixed_out}"
    );
    assert!(
        inspect_mixed_out.contains("status=fail"),
        "Gate B: fail should be visible: {inspect_mixed_out}"
    );
    assert!(
        inspect_mixed_out.contains("status=error"),
        "Gate B: error should be visible: {inspect_mixed_out}"
    );
    assert!(
        inspect_mixed_out.contains("status=skipped") || inspect_mixed_out.contains("status=skip"),
        "Gate B: skip should be visible: {inspect_mixed_out}"
    );

    let inspect_json = meno(root, &["inspect", "C-junit", "--json"]);
    assert_ok(&inspect_json, "meno inspect json");
    let inspect_json_report = parse_json(&inspect_json);
    assert_eq!(inspect_json_report["meno_cli_json_version"], 1);
    assert_eq!(inspect_json_report["claims"][0]["id"], "C-junit");
    let evidence = inspect_json_report["evidence"]
        .as_array()
        .expect("single-claim inspect json includes evidence");
    assert!(
        evidence.iter().any(|item| item["kind"] == "junit.report"),
        "expected junit.report evidence: {inspect_json_report}"
    );
    assert!(
        evidence.iter().any(|item| {
            item["observations"].as_array().is_some_and(|obs| {
                obs.iter()
                    .any(|o| o["type"] == "junit.case" || o.get("status").is_some())
            })
        }),
        "expected case observations in inspect json: {inspect_json_report}"
    );

    fs::write(root.join("junit.xml"), fixture("junit-pass.xml")).unwrap();
    let pass = meno(root, &["verify", "--from", "junit.xml", "--json"]);
    assert_ok(&pass, "meno verify passing junit");
    let pass_report = parse_json(&pass);
    let pass_claim = claim_by_id(&pass_report, "C-junit");
    assert_eq!(
        pass_claim["verdict"], "proven",
        "all-pass junit should prove the claim: {pass_claim}"
    );

    let inspect_pass = meno(root, &["inspect", "C-junit"]);
    assert_ok(&inspect_pass, "meno inspect after pass");
    let inspect_pass_out = stdout(&inspect_pass);
    assert!(
        inspect_pass_out.contains("junit.case") || inspect_pass_out.contains("status"),
        "{inspect_pass_out}"
    );
    assert!(
        inspect_pass_out.contains("meno.junit"),
        "{inspect_pass_out}"
    );

    let xml_before = fs::read(root.join("junit.xml")).unwrap();
    fs::write(root.join("README.md"), "hello dirty\n").unwrap();
    let status = meno(root, &["status", "--json"]);
    assert_ok(&status, "meno status after dirty tree");
    let xml_after = fs::read(root.join("junit.xml")).unwrap();
    assert_eq!(
        xml_before, xml_after,
        "status must not ingest or rewrite the junit xml"
    );
    let status_report = parse_json(&status);
    assert_eq!(status_report["meno_cli_json_version"], 1);
    let status_claim = claim_by_id(&status_report, "C-junit");
    assert_eq!(
        status_claim["verdict"], "unknown",
        "dirty tree should stale junit evidence: {status_claim}"
    );
    let stale = status_claim["stale"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !stale.is_empty(),
        "expected stale evidence after dirty tree: {status_claim}"
    );

    fs::write(root.join("claims/C-generic.yaml"), GENERIC_CLAIM).unwrap();
    fs::write(
        root.join("generic-envelope.json"),
        fixture("generic-envelope.json"),
    )
    .unwrap();
    let generic = meno(
        root,
        &["verify", "--from", "generic-envelope.json", "--json"],
    );
    assert_ok(&generic, "meno verify generic envelope");
    let generic_report = parse_json(&generic);
    let generic_claim = claim_by_id(&generic_report, "C-generic");
    assert_eq!(
        generic_claim["verdict"], "proven",
        "generic envelope should prove C-generic: {generic_claim}"
    );

    let inspect_generic = meno(root, &["inspect", "C-generic"]);
    assert_ok(&inspect_generic, "meno inspect generic");
    let inspect_generic_out = stdout(&inspect_generic);
    assert!(
        inspect_generic_out.contains("generic.envelope"),
        "inspect should mention generic.envelope: {inspect_generic_out}"
    );

    let inspect_generic_json = meno(root, &["inspect", "C-generic", "--json"]);
    assert_ok(&inspect_generic_json, "meno inspect generic json");
    let generic_inspect = parse_json(&inspect_generic_json);
    let generic_evidence = generic_inspect["evidence"]
        .as_array()
        .expect("generic inspect json evidence");
    assert!(
        generic_evidence
            .iter()
            .any(|item| item["kind"] == "generic.envelope"),
        "expected generic.envelope in inspect json: {generic_inspect}"
    );
}

#[test]
fn verify_from_missing_file_warns_and_does_not_fail() {
    let dir = init_repo();
    let root = dir.path();
    assert_ok(&meno(root, &["init"]), "meno init");
    fs::remove_file(root.join("claims/C-example.yaml")).unwrap();
    fs::write(root.join("claims/C-junit.yaml"), JUNIT_CLAIM).unwrap();

    let missing = meno(root, &["verify", "--from", "no-such-report.xml", "--json"]);
    assert_ok(&missing, "missing --from must not fail verify");
    let stderr = String::from_utf8_lossy(&missing.stderr);
    assert!(
        stderr.contains("no-such-report.xml"),
        "missing --from should warn: {stderr}"
    );
    let report = parse_json(&missing);
    let claim = claim_by_id(&report, "C-junit");
    assert_eq!(
        claim["verdict"], "unknown",
        "missing file must not disprove: {claim}"
    );
}
