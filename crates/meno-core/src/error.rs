use thiserror::Error;

#[derive(Debug, Error)]
pub enum MenoError {
    #[error("invalid claim: {0}")]
    InvalidClaim(String),
    #[error("invalid policy: {0}")]
    InvalidPolicy(String),
    #[error("invalid envelope: {0}")]
    InvalidEnvelope(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("integrity error: {0}")]
    Integrity(String),
    #[error("redaction rejected artifact: {0}")]
    RedactionRejected(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, MenoError>;
