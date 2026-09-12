//! v1 compatibility suite. Third parties can run `cargo test -p meno-mcp --test compat_v1`.

use meno_mcp::{McpEngine, MENO_MCP_VERSION};

const V1_TOOLS: &[&str] = &[
    "get_verification_state",
    "list_claims",
    "get_claim",
    "get_evidence",
    "inspect_verdict",
    "propose_claim",
    "submit_evidence",
    "request_evaluation",
];

#[test]
fn v1_mcp_version_and_tools() {
    assert_eq!(MENO_MCP_VERSION, 1);

    let mut names: Vec<String> = McpEngine::list_tools()
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    names.sort();
    let mut expected: Vec<String> = V1_TOOLS.iter().map(|s| (*s).to_string()).collect();
    expected.sort();
    assert_eq!(names, expected);
    assert_eq!(names.len(), 8);
}
