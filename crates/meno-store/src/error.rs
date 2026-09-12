use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("timestamp format: {0}")]
    Timestamp(#[from] time::error::Format),
    #[error("artifact integrity mismatch for {expected}: got {actual}")]
    IntegrityMismatch { expected: String, actual: String },
    #[error("redaction rejected artifact")]
    RedactionRejected,
    #[error("connection config must not contain credential-shaped values")]
    CredentialInConfig,
    #[error("artifacts directory is not configured; use Store::open_project")]
    NoArtifactsDir,
    #[error("artifact not found: {0}")]
    ArtifactNotFound(String),
    #[error("invalid artifact digest: {0}")]
    InvalidDigest(String),
    #[error("contract: {0}")]
    Contract(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Core(#[from] meno_core::MenoError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, StoreError>;
