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

const EXAMPLE_CLAIM: &str = r#"id: C-example
statement: "The configured unit command exits 0."
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
  contradicted_by: []
  freshness:
    subject_match: exact
"#;

const DISPROVEN_CLAIM: &str = r#"id: C-example
statement: "The configured unit command exits 0."
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
  contradicted_by:
    - kind: command.result
      match:
        exit_code:
          neq: 0
      min_count: 1
      subject_bound: true
  freshness:
    subject_match: exact
"#;

const MISSING_CLAIM: &str = r#"id: C-example
statement: "The configured unit command exits 0."
state: frozen
origin:
  kind: human
policy:
  version: 1
  requires:
    - kind: command.result
      match:
        exit_code: 1
      min_count: 1
      subject_bound: true
  contradicted_by: []
  freshness:
    subject_match: exact
"#;

fn write_toml(dir: &Path, argv: &str) {
    let toml = format!(
        r#"[meno]
version = 1
subject_identity_version = 1

[[adapters.command]]
name = "unit"
argv = {argv}
can_invoke = true
side_effect_level = "none"
"#
    );
    fs::write(dir.join("meno.toml"), toml).unwrap();
}

#[test]
fn canonical_verification_loop() {
    let dir = init_repo();
    let root = dir.path();

    let init = meno(root, &["init"]);
    assert_ok(&init, "meno init");
    let init_out = stdout(&init);
    assert!(
        init_out.contains("subject") || init_out.contains("initialized"),
        "{init_out}"
    );
    assert!(root.join("meno.toml").is_file());
    assert!(root.join("claims/C-example.yaml").is_file());
    assert!(root.join(".meno").is_dir());

    fs::write(root.join("claims/C-example.yaml"), EXAMPLE_CLAIM).unwrap();

    let verify = meno(root, &["verify", "--json"]);
    assert_ok(&verify, "meno verify");
    let report = parse_json(&verify);
    assert_eq!(report["meno_cli_json_version"], 1);
    assert_eq!(report["claims"][0]["id"], "C-example");
    assert_eq!(report["claims"][0]["verdict"], "proven");
    let evidence_id = report["claims"][0]["supporting"][0]
        .as_str()
        .expect("supporting evidence id")
        .to_string();
    assert!(!evidence_id.is_empty());

    let inspect = meno(root, &["inspect", "C-example"]);
    assert_ok(&inspect, "meno inspect");
    let inspect_out = stdout(&inspect);
    assert!(
        inspect_out.contains("Proven") || inspect_out.contains("proven"),
        "{inspect_out}"
    );
    assert!(
        inspect_out.contains(&evidence_id),
        "inspect missing evidence id {evidence_id}: {inspect_out}"
    );

    fs::write(root.join("README.md"), "hello world\n").unwrap();
    fs::write(
        root.join("write-sentinel.sh"),
        "#!/bin/sh\ntouch SENTINEL\nexit 0\n",
    )
    .unwrap();
    write_toml(root, r#"["sh", "write-sentinel.sh"]"#);

    let status = meno(root, &["status", "--json"]);
    assert_ok(&status, "meno status");
    assert!(
        !root.join("SENTINEL").exists(),
        "status must not invoke the configured command"
    );
    let status_report = parse_json(&status);
    assert_eq!(status_report["meno_cli_json_version"], 1);
    assert_eq!(status_report["claims"][0]["verdict"], "unknown");
    let stale = status_report["claims"][0]["stale"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let supporting = status_report["claims"][0]["supporting"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !stale.is_empty() || supporting.is_empty(),
        "expected stale evidence or no fresh supporting, got {status_report}"
    );

    let verify2 = meno(root, &["verify", "--json"]);
    assert_ok(&verify2, "meno verify after source change");
    assert!(
        root.join("SENTINEL").exists(),
        "verify should invoke the sentinel command"
    );
    let report2 = parse_json(&verify2);
    assert_eq!(report2["claims"][0]["verdict"], "proven");

    fs::write(root.join("claims/C-example.yaml"), MISSING_CLAIM).unwrap();
    write_toml(root, r#"["true"]"#);
    let missing = meno(root, &["verify", "--json"]);
    assert_ok(&missing, "meno verify missing require");
    let missing_report = parse_json(&missing);
    assert_eq!(
        missing_report["claims"][0]["verdict"], "unknown",
        "{missing_report}"
    );

    fs::write(root.join("README.md"), "hello again\n").unwrap();
    fs::write(root.join("claims/C-example.yaml"), DISPROVEN_CLAIM).unwrap();
    write_toml(root, r#"["false"]"#);
    let disproven = meno(root, &["verify", "--json"]);
    assert_ok(&disproven, "meno verify disproven");
    let disproven_report = parse_json(&disproven);
    assert_eq!(
        disproven_report["claims"][0]["verdict"], "disproven",
        "{disproven_report}"
    );
}
