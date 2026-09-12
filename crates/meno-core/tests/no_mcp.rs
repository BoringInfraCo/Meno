use std::fs;
use std::path::{Path, PathBuf};

fn assert_no_mcp(label: &str, src: &str) {
    let lower = src.to_ascii_lowercase();
    assert!(
        !lower.contains("mcp"),
        "{label} must not mention mcp (Gate D: harness types stay out of meno-core)"
    );
}

const LIB: &str = include_str!("../src/lib.rs");
const CANONICAL: &str = include_str!("../src/canonical.rs");
const CLAIM: &str = include_str!("../src/claim.rs");
const ENVELOPE: &str = include_str!("../src/envelope.rs");
const ERROR: &str = include_str!("../src/error.rs");
const EVALUATE: &str = include_str!("../src/evaluate.rs");
const IDS: &str = include_str!("../src/ids.rs");
const POLICY: &str = include_str!("../src/policy.rs");
const REDACTION: &str = include_str!("../src/redaction.rs");
const SUBJECT: &str = include_str!("../src/subject.rs");
const VERDICT: &str = include_str!("../src/verdict.rs");

#[test]
fn meno_core_sources_do_not_mention_mcp() {
    let included = [
        ("lib.rs", LIB),
        ("canonical.rs", CANONICAL),
        ("claim.rs", CLAIM),
        ("envelope.rs", ENVELOPE),
        ("error.rs", ERROR),
        ("evaluate.rs", EVALUATE),
        ("ids.rs", IDS),
        ("policy.rs", POLICY),
        ("redaction.rs", REDACTION),
        ("subject.rs", SUBJECT),
        ("verdict.rs", VERDICT),
    ];
    for (name, src) in included {
        assert_no_mcp(name, src);
    }

    let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let on_disk = rust_files(&src_dir);
    assert_eq!(
        on_disk.len(),
        included.len(),
        "meno-core src/*.rs changed; update include_str list in no_mcp.rs (found {on_disk:?})"
    );
    for path in on_disk {
        let text = fs::read_to_string(&path).unwrap();
        assert_no_mcp(&path.display().to_string(), &text);
    }
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
    files.sort();
    files
}
