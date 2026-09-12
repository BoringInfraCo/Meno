use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
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

const MINIMAL_TOML: &str = r#"[meno]
version = 1
subject_identity_version = 1
"#;

fn write_minimal_project(root: &Path) {
    fs::write(root.join("meno.toml"), MINIMAL_TOML).unwrap();
    fs::create_dir_all(root.join("claims")).unwrap();
    fs::write(root.join("claims/C-example.yaml"), EXAMPLE_CLAIM).unwrap();
}

fn write_sentinel_script(root: &Path) {
    fs::write(
        root.join("write-sentinel.sh"),
        "#!/bin/sh\ntouch SENTINEL\nexit 0\n",
    )
    .unwrap();
}

fn command_table_count(toml: &str) -> usize {
    toml.matches("[[adapters.command]]").count()
}

#[test]
fn connect_catalog_does_not_mutate_toml() {
    let dir = init_repo();
    let root = dir.path();
    let init = meno(root, &["init"]);
    assert_ok(&init, "meno init");
    let before = fs::read_to_string(root.join("meno.toml")).unwrap();

    let catalog = meno(root, &["connect"]);
    assert_ok(&catalog, "meno connect");
    let out = stdout(&catalog).to_ascii_lowercase();
    assert!(
        out.contains("playwright") || out.contains("junit") || out.contains("agent"),
        "catalog should mention playwright, junit, or agent: {out}"
    );
    let after = fs::read_to_string(root.join("meno.toml")).unwrap();
    assert_eq!(before, after, "bare connect must not mutate meno.toml");
}

#[test]
fn connect_command_then_verify_proven() {
    let dir = init_repo();
    let root = dir.path();
    write_minimal_project(root);

    let connect = meno(
        root,
        &[
            "connect",
            "--adapter",
            "command",
            "--name",
            "unit",
            "--argv",
            "true",
            "--can-invoke",
            "--side-effect-level",
            "none",
        ],
    );
    assert_ok(&connect, "meno connect command");
    let toml = fs::read_to_string(root.join("meno.toml")).unwrap();
    assert!(toml.contains("[[adapters.command]]"), "{toml}");
    assert!(toml.contains("name = \"unit\""), "{toml}");
    assert!(toml.contains("can_invoke = true"), "{toml}");
    assert!(toml.contains("side_effect_level = \"none\""), "{toml}");

    let verify = meno(root, &["verify", "--json"]);
    assert_ok(&verify, "meno verify after connect");
    let report = parse_json(&verify);
    assert_eq!(report["claims"][0]["id"], "C-example");
    assert_eq!(report["claims"][0]["verdict"], "proven");
}

#[test]
fn duplicate_name_without_replace_fails() {
    let dir = init_repo();
    let root = dir.path();
    let init = meno(root, &["init"]);
    assert_ok(&init, "meno init");
    let before = fs::read_to_string(root.join("meno.toml")).unwrap();
    assert_eq!(command_table_count(&before), 1, "{before}");

    let dup = meno(
        root,
        &[
            "connect",
            "--adapter",
            "command",
            "--name",
            "unit",
            "--argv",
            "true",
            "--can-invoke",
            "--side-effect-level",
            "none",
        ],
    );
    assert!(
        !dup.status.success(),
        "duplicate name must fail\nstdout:\n{}\nstderr:\n{}",
        stdout(&dup),
        stderr(&dup)
    );
    let err = format!("{}{}", stdout(&dup), stderr(&dup)).to_ascii_lowercase();
    assert!(
        err.contains("already exists") || err.contains("replace"),
        "duplicate error should mention exists/replace: {err}"
    );
    let after = fs::read_to_string(root.join("meno.toml")).unwrap();
    assert_eq!(before, after, "failed connect must not mutate meno.toml");
    assert_eq!(command_table_count(&after), 1, "{after}");
}

#[test]
fn gate_c_consequential_never_invokes() {
    let dir = init_repo();
    let root = dir.path();
    write_minimal_project(root);
    write_sentinel_script(root);

    let connect = meno(
        root,
        &[
            "connect",
            "--adapter",
            "command",
            "--name",
            "deploy",
            "--argv",
            "sh",
            "write-sentinel.sh",
            "--can-invoke",
            "--side-effect-level",
            "consequential",
        ],
    );
    assert_ok(&connect, "meno connect consequential");

    let verify = meno(root, &["verify"]);
    assert_ok(&verify, "meno verify consequential");
    assert!(
        !root.join("SENTINEL").exists(),
        "verify must not invoke consequential commands"
    );
    assert!(
        stderr(&verify).contains("skipping consequential command `deploy`"),
        "expected consequential skip message, stderr:\n{}",
        stderr(&verify)
    );

    let confirmed = meno(root, &["verify", "--confirm-invoke"]);
    assert_ok(&confirmed, "meno verify --confirm-invoke consequential");
    assert!(
        !root.join("SENTINEL").exists(),
        "verify --confirm-invoke must not invoke consequential commands"
    );
}

#[test]
fn filesystem_requires_confirm_invoke() {
    let dir = init_repo();
    let root = dir.path();
    write_minimal_project(root);
    write_sentinel_script(root);

    let connect = meno(
        root,
        &[
            "connect",
            "--adapter",
            "command",
            "--name",
            "deploy",
            "--argv",
            "sh",
            "write-sentinel.sh",
            "--can-invoke",
            "--side-effect-level",
            "filesystem",
        ],
    );
    assert_ok(&connect, "meno connect filesystem");

    let verify = meno(root, &["verify"]);
    assert_ok(&verify, "meno verify filesystem without confirm");
    assert!(
        !root.join("SENTINEL").exists(),
        "verify without --confirm-invoke must not invoke filesystem commands"
    );

    let confirmed = meno(root, &["verify", "--confirm-invoke"]);
    assert_ok(&confirmed, "meno verify --confirm-invoke filesystem");
    assert!(
        root.join("SENTINEL").exists(),
        "verify --confirm-invoke should invoke filesystem commands\nstdout:\n{}\nstderr:\n{}",
        stdout(&confirmed),
        stderr(&confirmed)
    );
    assert!(
        stderr(&confirmed).contains("invoking deploy with side_effect_level=filesystem"),
        "expected invoke warning, stderr:\n{}",
        stderr(&confirmed)
    );
}

#[test]
fn status_never_invokes_filesystem_adapter() {
    let dir = init_repo();
    let root = dir.path();
    write_minimal_project(root);
    write_sentinel_script(root);

    let connect = meno(
        root,
        &[
            "connect",
            "--adapter",
            "command",
            "--name",
            "deploy",
            "--argv",
            "sh",
            "write-sentinel.sh",
            "--can-invoke",
            "--side-effect-level",
            "filesystem",
        ],
    );
    assert_ok(&connect, "meno connect filesystem");

    let status = meno(root, &["status"]);
    assert_ok(&status, "meno status filesystem");
    assert!(
        !root.join("SENTINEL").exists(),
        "status must not invoke filesystem commands"
    );
}

#[test]
fn connect_junit_and_playwright_write_paths_without_requiring_files() {
    let dir = init_repo();
    let root = dir.path();
    write_minimal_project(root);
    let before = fs::read_to_string(root.join("meno.toml")).unwrap();

    let junit = meno(
        root,
        &[
            "connect",
            "--adapter",
            "junit",
            "--name",
            "unit",
            "--path",
            "junit.xml",
        ],
    );
    assert_ok(&junit, "meno connect junit");
    assert!(!root.join("junit.xml").exists());

    let pw = meno(
        root,
        &[
            "connect",
            "--adapter",
            "playwright",
            "--name",
            "e2e",
            "--path",
            "playwright.json",
        ],
    );
    assert_ok(&pw, "meno connect playwright");
    assert!(!root.join("playwright.json").exists());

    let toml = fs::read_to_string(root.join("meno.toml")).unwrap();
    assert!(toml.contains("[meno]"), "must preserve [meno]: {toml}");
    assert!(before.contains("[meno]"));
    assert!(toml.contains("[[adapters.junit]]"), "{toml}");
    assert!(toml.contains("path = \"junit.xml\""), "{toml}");
    assert!(toml.contains("[[adapters.playwright]]"), "{toml}");
    assert!(toml.contains("name = \"e2e\""), "{toml}");
    assert!(toml.contains("path = \"playwright.json\""), "{toml}");
}

fn project_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path.strip_prefix(root).expect("prefix").to_path_buf();
            if rel
                .components()
                .next()
                .is_some_and(|c| c.as_os_str() == ".git")
            {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(rel, fs::read(&path).unwrap_or_default());
            }
        }
    }
    walk(root, root, &mut out);
    out
}

#[test]
fn connect_agent_is_v05() {
    let dir = init_repo();
    let root = dir.path();
    write_minimal_project(root);
    let before_toml = fs::read_to_string(root.join("meno.toml")).unwrap();

    let catalog = meno(root, &["connect"]);
    assert_ok(&catalog, "meno connect");
    let catalog_out = format!("{}{}", stdout(&catalog), stderr(&catalog)).to_ascii_lowercase();
    assert!(
        catalog_out.contains("mcp") || catalog_out.contains("skill"),
        "catalog should mention MCP or Skill: {catalog_out}"
    );

    let before_files = project_files(root);
    let agent = meno(root, &["connect", "--adapter", "agent"]);
    assert_ok(&agent, "meno connect --adapter agent");
    let agent_out = format!("{}{}", stdout(&agent), stderr(&agent)).to_ascii_lowercase();
    assert!(
        agent_out.contains("--stdio") && agent_out.contains("--write"),
        "agent usage should mention --stdio and --write: {agent_out}"
    );
    assert_eq!(
        before_files,
        project_files(root),
        "agent without --write must not write files"
    );
    let after_toml = fs::read_to_string(root.join("meno.toml")).unwrap();
    assert_eq!(
        before_toml, after_toml,
        "agent connect must not mutate meno.toml"
    );

    let write = meno(root, &["connect", "--adapter", "agent", "--write"]);
    assert_ok(&write, "meno connect --adapter agent --write");
    let mcp_path = root.join(".mcp.json");
    let mcp = fs::read_to_string(&mcp_path).unwrap();
    assert!(
        mcp.contains("--stdio"),
        ".mcp.json should mention --stdio: {mcp}"
    );
    let parsed: serde_json::Value = serde_json::from_str(&mcp).unwrap();
    let args = parsed["mcpServers"]["meno"]["args"]
        .as_array()
        .expect("mcpServers.meno.args");
    assert!(
        args.iter().any(|v| v.as_str() == Some("--stdio")),
        "args should contain --stdio: {parsed}"
    );
    assert_eq!(parsed["mcpServers"]["meno"]["command"], "meno");
    let after_write_toml = fs::read_to_string(root.join("meno.toml")).unwrap();
    assert_eq!(
        before_toml, after_write_toml,
        "agent --write must not mutate meno.toml"
    );

    let first_mcp = fs::read(&mcp_path).unwrap();
    let again = meno(root, &["connect", "--adapter", "agent", "--write"]);
    assert!(
        !again.status.success(),
        "second write without --replace must fail\nstdout:\n{}\nstderr:\n{}",
        stdout(&again),
        stderr(&again)
    );
    let err = format!("{}{}", stdout(&again), stderr(&again)).to_ascii_lowercase();
    assert!(
        err.contains("already exists") || err.contains("replace"),
        "duplicate write error should mention exists/replace: {err}"
    );
    assert_eq!(
        first_mcp,
        fs::read(&mcp_path).unwrap(),
        "failed second write must not mutate .mcp.json"
    );
}
