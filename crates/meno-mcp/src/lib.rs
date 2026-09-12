//! MCP tool handlers over Meno core.
//!
//! Default authority allows reads, draft claim proposals, evidence submission,
//! and evaluation requests. It does not freeze claims, mutate trusted policy,
//! delete evidence, or rewrite provenance. Not a sixth CLI command.

mod engine;
mod error;
mod stdio;

pub use engine::{McpEngine, ToolSpec};
pub use error::McpError;
pub use stdio::serve_stdio;

pub const MENO_MCP_VERSION: u32 = 1;

#[cfg(test)]
mod tests;
