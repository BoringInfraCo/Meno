//! Gate D: identical verification state via CLI, MCP, and a JSON file.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use meno_mcp::McpEngine;
use serde_json::Value;
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

fn parse_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "json parse error: {err}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn identity(report: &Value) -> (String, String, String) {
    let subject_id = report["subject_id"]
        .as_str()
        .unwrap_or_else(|| panic!("subject_id missing: {report}"))
        .to_string();
    let claims = report["claims"]
        .as_array()
        .unwrap_or_else(|| panic!("claims missing: {report}"));
    assert!(
        !claims.is_empty(),
        "expected at least one claim in {report}"
    );
    let claim = &claims[0];
    let id = claim["id"]
        .as_str()
        .unwrap_or_else(|| panic!("claim id missing: {report}"))
        .to_string();
    let verdict = claim["verdict"]
        .as_str()
        .unwrap_or_else(|| panic!("verdict missing: {report}"))
        .to_string();
    (subject_id, id, verdict)
}

fn unwrap_verification_state(value: Value) -> Value {
    if value.get("subject_id").is_some() && value.get("claims").is_some() {
        return value;
    }
    if let Some(result) = value.get("result") {
        return unwrap_verification_state(result.clone());
    }
    if let Some(text) = value.pointer("/content/0/text").and_then(Value::as_str) {
        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
            return unwrap_verification_state(parsed);
        }
    }
    value
}

/// Planned in-process API: `McpEngine::open` + `call_tool("get_verification_state", ...)`.
fn mcp_via_engine(root: &Path) -> Value {
    let mut engine = McpEngine::open(root).expect("McpEngine::open");
    let raw = engine
        .call_tool("get_verification_state", serde_json::json!({}))
        .expect("call_tool get_verification_state");
    unwrap_verification_state(raw)
}

/// Wire fallback: `meno connect --adapter agent --stdio` with `MENO_MCP_NDJSON=1`.
fn mcp_via_stdio(root: &Path) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_meno"))
        .current_dir(root)
        .args(["connect", "--adapter", "agent", "--stdio"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("MENO_MCP_NDJSON", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn meno connect --stdio");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        stdin
            .write_all(
                br#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_verification_state","arguments":{}}}
"#,
            )
            .expect("write mcp request");
        stdin.flush().expect("flush mcp request");
    }
    let output = child.wait_with_output().expect("wait mcp stdio");
    assert!(
        output.status.success(),
        "mcp stdio failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_else(|| {
            panic!(
                "no MCP response\nstdout:\n{stdout}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stderr)
            )
        });
    let raw: Value = serde_json::from_str(line).unwrap_or_else(|err| {
        panic!(
            "mcp json: {err}\nline: {line}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    unwrap_verification_state(raw)
}

#[test]
fn gate_d_cli_mcp_and_file_report_the_same_verification_state() {
    let dir = init_repo();
    let root = dir.path();

    let init = meno(root, &["init"]);
    assert_ok(&init, "meno init");

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
            "--replace",
        ],
    );
    assert_ok(&connect, "meno connect command");

    let verify = meno(root, &["verify", "--json"]);
    assert_ok(&verify, "meno verify");
    let verified = parse_json(&verify);
    assert_eq!(
        identity(&verified).2,
        "proven",
        "verify should prove the frozen command claim: {verified}"
    );

    let status = meno(root, &["status", "--json"]);
    assert_ok(&status, "meno status --json");
    let cli = parse_json(&status);
    let (subject_id, claim_id, verdict) = identity(&cli);
    assert_eq!(verdict, "proven", "status --json: {cli}");

    // Dump outside the worktree so the file is not itself a subject input.
    let json_dir = tempfile::tempdir().expect("json dir");
    let json_path = json_dir.path().join("status.json");
    fs::write(&json_path, &status.stdout).expect("write status.json");
    let from_file: Value = serde_json::from_slice(&fs::read(&json_path).expect("read status.json"))
        .expect("parse status.json");

    let via_engine = mcp_via_engine(root);
    let via_stdio = mcp_via_stdio(root);

    for (label, report) in [
        ("cli", &cli),
        ("file", &from_file),
        ("mcp-engine", &via_engine),
        ("mcp-stdio", &via_stdio),
    ] {
        let (got_subject, got_id, got_verdict) = identity(report);
        assert_eq!(got_subject, subject_id, "{label} subject_id");
        assert_eq!(got_id, claim_id, "{label} claim id");
        assert_eq!(got_verdict, "proven", "{label} verdict");
    }
}
