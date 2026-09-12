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

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
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

fn read_json(path: &Path) -> serde_json::Value {
    let text = fs::read_to_string(path).unwrap_or_else(|err| {
        panic!("read {}: {err}", path.display());
    });
    serde_json::from_str(&text).unwrap_or_else(|err| {
        panic!("json parse error for {}: {err}\n{text}", path.display());
    })
}

fn copy_dir(src: &Path, dest: &Path) {
    fs::create_dir_all(dest).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let dest_path = dest.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dest_path);
        } else {
            fs::copy(entry.path(), dest_path).unwrap();
        }
    }
}

fn claim_ids(bundle: &serde_json::Value) -> Vec<String> {
    bundle["claims"]
        .as_array()
        .unwrap_or_else(|| panic!("claims array: {bundle}"))
        .iter()
        .map(|claim| {
            claim["id"]
                .as_str()
                .unwrap_or_else(|| panic!("claim id: {claim}"))
                .to_string()
        })
        .collect()
}

const OTHER_CLAIM: &str = r#"id: C-other
statement: "A second command claim."
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

#[test]
fn export_import_round_trip_keeps_stale_provenance() {
    let src = init_repo();
    let root = src.path();

    let init = meno(root, &["init"]);
    assert_ok(&init, "meno init");

    let verify = meno(root, &["verify", "--json"]);
    assert_ok(&verify, "meno verify");
    let report = parse_json(&verify);
    assert_eq!(report["claims"][0]["id"], "C-example");
    assert_eq!(report["claims"][0]["verdict"], "proven");

    let export = meno(root, &["inspect", "--export", "bundle"]);
    assert_ok(&export, "meno inspect --export bundle");
    let manifest = root.join("bundle/meno-bundle.json");
    assert!(
        manifest.is_file(),
        "expected bundle/meno-bundle.json after export"
    );
    let bundle = read_json(&manifest);
    assert_eq!(bundle["meno_bundle_version"], 1);
    let envelopes = bundle["envelopes"]
        .as_array()
        .unwrap_or_else(|| panic!("envelopes array: {bundle}"));
    assert!(
        !envelopes.is_empty(),
        "exported bundle must include envelopes: {bundle}"
    );
    assert!(
        envelopes.iter().any(|env| env["kind"] == "command.result"),
        "expected command.result envelope: {bundle}"
    );

    let dest = init_repo();
    let dest_root = dest.path();
    let dest_init = meno(dest_root, &["init"]);
    assert_ok(&dest_init, "meno init dest");
    copy_dir(&root.join("bundle"), &dest_root.join("bundle"));

    let import = meno(dest_root, &["inspect", "--import", "bundle"]);
    assert_ok(&import, "meno inspect --import bundle");
    let import_out = stdout(&import);
    assert!(
        import_out.contains("envelope") || import_out.contains("imported"),
        "import should print counts: {import_out}"
    );

    let status = meno(dest_root, &["status", "--json"]);
    assert_ok(&status, "meno status after import");
    let status_report = parse_json(&status);
    assert_eq!(status_report["claims"][0]["verdict"], "unknown");
    let stale = status_report["claims"][0]["stale"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !stale.is_empty(),
        "imported evidence on a different subject should be stale: {status_report}"
    );

    let inspect_json = meno(dest_root, &["inspect", "C-example", "--json"]);
    assert_ok(&inspect_json, "meno inspect json after import");
    let inspect_report = parse_json(&inspect_json);
    assert_eq!(inspect_report["claims"][0]["verdict"], "unknown");
    let inspect_stale = inspect_report["claims"][0]["stale"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !inspect_stale.is_empty(),
        "inspect json should list stale evidence: {inspect_report}"
    );
    let evidence = inspect_report["evidence"]
        .as_array()
        .expect("inspect json includes evidence");
    assert!(
        evidence.iter().any(|item| {
            item["provenance"]["producer"] == "meno.command" && item["trust"]["origin"] == "machine"
        }),
        "inspect should keep original producer/trust: {inspect_report}"
    );

    let inspect = meno(dest_root, &["inspect", "C-example"]);
    assert_ok(&inspect, "meno inspect after import");
    let inspect_out = stdout(&inspect);
    assert!(
        inspect_out.contains("meno.command"),
        "inspect should mention original producer: {inspect_out}"
    );
    assert!(
        inspect_out.contains("origin=machine") || inspect_out.contains("machine"),
        "inspect should mention original trust: {inspect_out}"
    );
}

#[test]
fn scoped_export_only_includes_that_claim() {
    let dir = init_repo();
    let root = dir.path();
    assert_ok(&meno(root, &["init"]), "meno init");
    fs::write(root.join("claims/C-other.yaml"), OTHER_CLAIM).unwrap();

    let verify = meno(root, &["verify", "--json"]);
    assert_ok(&verify, "meno verify two claims");
    let report = parse_json(&verify);
    let ids: Vec<&str> = report["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .filter_map(|claim| claim["id"].as_str())
        .collect();
    assert!(ids.contains(&"C-example"), "{report}");
    assert!(ids.contains(&"C-other"), "{report}");

    let full = meno(root, &["inspect", "--export", "full-bundle"]);
    assert_ok(&full, "unscoped export");
    let full_bundle = read_json(&root.join("full-bundle/meno-bundle.json"));
    let full_ids = claim_ids(&full_bundle);
    assert!(full_ids.contains(&"C-example".to_string()), "{full_ids:?}");
    assert!(full_ids.contains(&"C-other".to_string()), "{full_ids:?}");
    assert!(full_ids.len() >= 2, "{full_ids:?}");

    let scoped = meno(root, &["inspect", "C-example", "--export", "c17-bundle"]);
    assert_ok(&scoped, "scoped export");
    let scoped_bundle = read_json(&root.join("c17-bundle/meno-bundle.json"));
    assert_eq!(scoped_bundle["meno_bundle_version"], 1);
    let scoped_ids = claim_ids(&scoped_bundle);
    assert_eq!(scoped_ids, vec!["C-example".to_string()], "{scoped_ids:?}");
    assert!(
        !scoped_ids.contains(&"C-other".to_string()),
        "scoped export must not include C-other: {scoped_ids:?}"
    );
}

#[test]
fn export_fails_on_nonempty_dest() {
    let dir = init_repo();
    let root = dir.path();
    assert_ok(&meno(root, &["init"]), "meno init");
    assert_ok(&meno(root, &["verify"]), "meno verify");

    let first = meno(root, &["inspect", "--export", "bundle"]);
    assert_ok(&first, "first export");
    let again = meno(root, &["inspect", "--export", "bundle"]);
    assert!(
        !again.status.success(),
        "export onto non-empty dest must fail\nstdout:\n{}\nstderr:\n{}",
        stdout(&again),
        stderr(&again)
    );
    let err = format!("{}{}", stdout(&again), stderr(&again)).to_ascii_lowercase();
    assert!(
        err.contains("not empty") || err.contains("export"),
        "expected non-empty destination error, got {err}"
    );
}

#[test]
fn export_is_not_a_top_level_command() {
    let dir = init_repo();
    let root = dir.path();
    let help = meno(root, &["--help"]);
    assert_ok(&help, "meno --help");
    let help_out = stdout(&help);
    assert!(
        !help_out.lines().any(|line| {
            let trimmed = line.trim_start();
            trimmed == "export" || trimmed.starts_with("export ")
        }),
        "clap help must not list export as a top-level command:\n{help_out}"
    );

    let export = meno(root, &["export"]);
    assert!(
        !export.status.success(),
        "meno export must not be a subcommand\nstdout:\n{}\nstderr:\n{}",
        stdout(&export),
        stderr(&export)
    );
}

#[test]
fn confirm_cannot_combine_with_export() {
    let dir = init_repo();
    let root = dir.path();
    let out = meno(
        root,
        &[
            "inspect",
            "C-example",
            "--confirm",
            "--statement",
            "ok",
            "--export",
            "bundle",
        ],
    );
    assert!(
        !out.status.success(),
        "--confirm + --export must fail\nstdout:\n{}\nstderr:\n{}",
        stdout(&out),
        stderr(&out)
    );
}
