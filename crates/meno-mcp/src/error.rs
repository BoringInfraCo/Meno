use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum McpError {
    #[error("{0}")]
    Message(String),
    #[error("{0} is not permitted under default MCP authority")]
    Authority(String),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("{0}")]
    Core(#[from] meno_core::MenoError),
    #[error("{0}")]
    Store(#[from] meno_store::StoreError),
    #[error("{0}")]
    Adapter(#[from] meno_adapters::AdapterError),
}

pub type Result<T> = std::result::Result<T, McpError>;

impl McpError {
    pub fn msg(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }

    pub fn authority(tool: impl Into<String>) -> Self {
        Self::Authority(tool.into())
    }
}
