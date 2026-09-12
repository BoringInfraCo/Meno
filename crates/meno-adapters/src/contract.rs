use std::path::PathBuf;

use meno_core::envelope::Envelope;
use serde::{Deserialize, Serialize};

/// Adapter failure (detect, normalize, validate, or collection I/O).
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("git: {0}")]
    Git(String),
}

impl AdapterError {
    pub fn message(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }

    pub fn git(msg: impl Into<String>) -> Self {
        Self::Git(msg.into())
    }
}

/// Safety metadata frozen in v0 so later adapters cannot invent defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterSafety {
    pub can_collect: bool,
    pub can_invoke: bool,
    pub side_effect_level: SideEffectLevel,
    pub requires_confirmation: bool,
}

/// Unknown / unspecified adapters use the conservative default.
impl Default for AdapterSafety {
    fn default() -> Self {
        Self {
            can_collect: true,
            can_invoke: false,
            side_effect_level: SideEffectLevel::Consequential,
            requires_confirmation: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectLevel {
    None,
    Filesystem,
    Network,
    Consequential,
}

#[derive(Debug, Clone)]
pub struct DetectContext {
    pub project_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectResult {
    pub detected: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InputBundle {
    pub kind: String,
    pub payload: Vec<u8>,
    pub path: Option<PathBuf>,
}

/// Thin translator from tool output into an evidence envelope.
///
/// `configure` / `collect` / `invoke` are v0.4 and must not execute here.
pub trait Adapter: Send + Sync {
    fn name(&self) -> &str;
    fn safety(&self) -> AdapterSafety;
    fn detect(&self, ctx: &DetectContext) -> Result<DetectResult, AdapterError>;
    fn normalize(&self, input: &InputBundle) -> Result<Envelope, AdapterError>;
    fn validate(&self, envelope: &Envelope) -> Result<(), AdapterError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_adapter_defaults_are_conservative() {
        let safety = AdapterSafety::default();
        assert!(safety.can_collect);
        assert!(!safety.can_invoke);
        assert_eq!(safety.side_effect_level, SideEffectLevel::Consequential);
        assert!(safety.requires_confirmation);
    }
}
