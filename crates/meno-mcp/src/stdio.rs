use std::io::{self, BufRead, Write};
use std::path::Path;

use serde_json::{json, Value};

use crate::engine::McpEngine;
use crate::error::Result;

/// NDJSON JSON-RPC over stdin/stdout. Used by `meno connect --adapter agent --stdio`.
pub fn serve_stdio(root: &Path) -> Result<()> {
    let mut engine = McpEngine::open(root)?;
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(err) => {
                write_line(
                    &mut stdout,
                    &rpc_error(Value::Null, -32700, err.to_string()),
                )?;
                continue;
            }
        };
        if let Some(response) = handle_request(&mut engine, request) {
            write_line(&mut stdout, &response)?;
        }
    }
    Ok(())
}

pub(crate) fn handle_request(engine: &mut McpEngine, req: Value) -> Option<Value> {
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let id = req.get("id").cloned();
    let notification = id.is_none();

    if matches!(
        method,
        "notifications/initialized" | "initialized" | "notifications/cancelled"
    ) {
        return None;
    }

    if method == "initialize" {
        return Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "meno",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            }
        }));
    }

    if method == "tools/list" || method == "list_tools" {
        let tools: Vec<Value> = McpEngine::list_tools()
            .into_iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": { "type": "object", "properties": {} }
                })
            })
            .collect();
        return Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "tools": tools }
        }));
    }

    if method == "ping" || method == "shutdown" {
        return if notification {
            None
        } else {
            Some(json!({ "jsonrpc": "2.0", "id": id, "result": {} }))
        };
    }

    let Some(tool) = request_tool(&req) else {
        return if notification {
            None
        } else {
            Some(rpc_error(
                id.unwrap_or(Value::Null),
                -32601,
                format!("method not found: {method}"),
            ))
        };
    };
    let arguments = req
        .pointer("/params/arguments")
        .cloned()
        .or_else(|| req.get("arguments").cloned())
        .unwrap_or_else(|| json!({}));
    let result = match engine.call_tool(tool, arguments) {
        Ok(value) => json!({
            "content": [{ "type": "text", "text": value.to_string() }]
        }),
        Err(err) => json!({
            "isError": true,
            "content": [{ "type": "text", "text": err.to_string() }]
        }),
    };
    if notification {
        None
    } else {
        Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }))
    }
}

fn write_line(stdout: &mut io::Stdout, value: &Value) -> Result<()> {
    writeln!(stdout, "{}", serde_json::to_string(value)?)?;
    stdout.flush()?;
    Ok(())
}

fn request_tool(req: &Value) -> Option<&str> {
    if let Some(name) = req.get("tool").and_then(Value::as_str) {
        return Some(name);
    }
    let method = req.get("method").and_then(Value::as_str)?;
    match method {
        "tools/call" => req
            .get("params")
            .and_then(|params| params.get("name"))
            .and_then(Value::as_str),
        "get_verification_state"
        | "list_claims"
        | "get_claim"
        | "get_evidence"
        | "inspect_verdict"
        | "propose_claim"
        | "submit_evidence"
        | "request_evaluation" => Some(method),
        other if other.starts_with("tools/") => None,
        other => Some(other),
    }
}

fn rpc_error(id: Value, code: i64, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}
