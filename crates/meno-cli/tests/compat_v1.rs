//! v1 compatibility suite. Third parties can run `cargo test -p meno --test compat_v1`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn meno_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_meno"))
}

fn meno_output(args: &[&str]) -> Output {
    meno_bin().args(args).output().expect("spawn meno")
}

fn combined(output: &Output) -> String {
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

fn command_names(help: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_commands = false;
    for line in help.lines() {
        if line.trim_start().starts_with("Commands:") {
            in_commands = true;
            continue;
        }
        if in_commands {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(name) = trimmed.split_whitespace().next() {
                if name != "help" {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

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

fn meno_in(dir: &Path, args: &[&str]) -> Output {
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

fn example_evidence() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/generic-envelope/evidence.json")
}

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
fn v1_cli_has_five_subcommands_and_no_export() {
    let help = meno_output(&["--help"]);
    assert!(help.status.success(), "meno --help should succeed");
    let text = combined(&help);
    let names = command_names(&text);
    let expected = ["init", "connect", "verify", "status", "inspect"];
    for name in expected {
        assert!(
            names.iter().any(|n| n == name) || text.contains(name),
            "missing subcommand {name} in --help:\n{text}"
        );
    }
    assert_eq!(
        names, expected,
        "v1 CLI subcommands must be exactly init, connect, verify, status, inspect:\n{text}"
    );
    assert!(
        !names.iter().any(|n| n == "export"),
        "`meno export` must not be a command:\n{text}"
    );

    let export = meno_output(&["export"]);
    assert!(
        !export.status.success(),
        "`meno export` must not be a command"
    );
    let export_text = combined(&export).to_ascii_lowercase();
    assert!(
        export_text.contains("unrecognized")
            || export_text.contains("unexpected")
            || export_text.contains("invalid")
            || !command_names(&combined(&meno_output(&["--help"]))).contains(&"export".to_string()),
        "expected clap to reject `meno export`:\n{export_text}"
    );
}

#[test]
fn v1_cli_version_contains_crate_version() {
    let output = meno_output(&["--version"]);
    assert!(output.status.success(), "meno --version should succeed");
    let text = combined(&output);
    let version = env!("CARGO_PKG_VERSION");
    assert!(
        text.contains(version),
        "meno --version must contain {version}: {text:?}"
    );
}

#[test]
fn generic_envelope_example_is_visible_in_inspect() {
    let dir = init_repo();
    let root = dir.path();
    assert_ok(&meno_in(root, &["init"]), "meno init");

    fs::write(root.join("claims/C-generic.yaml"), GENERIC_CLAIM).unwrap();
    fs::copy(example_evidence(), root.join("evidence.json")).unwrap();

    let verify = meno_in(root, &["verify", "--from", "evidence.json", "--json"]);
    assert_ok(&verify, "meno verify --from evidence.json");

    let inspect = meno_in(root, &["inspect", "C-generic", "--json"]);
    assert_ok(&inspect, "meno inspect C-generic");
    let report: serde_json::Value = serde_json::from_slice(&inspect.stdout).expect("inspect json");
    let evidence = report["evidence"]
        .as_array()
        .expect("inspect json evidence");
    assert!(
        evidence
            .iter()
            .any(|item| item["kind"] == "generic.envelope"),
        "expected generic.envelope in inspect: {report}"
    );
}
