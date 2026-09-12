use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("toml: {0}")]
    TomlEdit(#[from] toml_edit::TomlError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Core(#[from] meno_core::MenoError),
    #[error("{0}")]
    Store(#[from] meno_store::StoreError),
    #[error("{0}")]
    Adapter(#[from] meno_adapters::AdapterError),
}

impl CliError {
    pub fn msg(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }
}
