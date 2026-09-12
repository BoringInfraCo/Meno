use std::fs;
use std::path::{Path, PathBuf};

use meno_adapters::{CommandSpec, SideEffectLevel};
use serde::Deserialize;

use crate::error::CliError;

#[derive(Debug, Deserialize)]
pub struct MenoConfig {
    pub meno: MenoSection,
    #[serde(default)]
    pub adapters: AdaptersSection,
}

#[derive(Debug, Deserialize)]
pub struct MenoSection {
    pub version: u32,
    #[serde(default = "default_subject_identity_version")]
    pub subject_identity_version: u32,
}

fn default_subject_identity_version() -> u32 {
    1
}

#[derive(Debug, Default, Deserialize)]
pub struct AdaptersSection {
    #[serde(default)]
    pub command: Vec<CommandAdapterConfig>,
    #[serde(default)]
    pub junit: Vec<JunitAdapterConfig>,
    #[serde(default)]
    pub playwright: Vec<PlaywrightAdapterConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommandAdapterConfig {
    pub name: String,
    pub argv: Vec<String>,
    #[serde(default)]
    pub can_invoke: bool,
    #[serde(default = "default_side_effect")]
    pub side_effect_level: SideEffectLevel,
}

fn default_side_effect() -> SideEffectLevel {
    SideEffectLevel::Consequential
}

pub fn side_effect_slug(level: SideEffectLevel) -> &'static str {
    match level {
        SideEffectLevel::None => "none",
        SideEffectLevel::Filesystem => "filesystem",
        SideEffectLevel::Network => "network",
        SideEffectLevel::Consequential => "consequential",
    }
}

pub fn parse_side_effect_level(raw: &str) -> Result<SideEffectLevel, CliError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" => Ok(SideEffectLevel::None),
        "filesystem" => Ok(SideEffectLevel::Filesystem),
        "network" => Ok(SideEffectLevel::Network),
        "consequential" => Ok(SideEffectLevel::Consequential),
        other => Err(CliError::msg(format!(
            "invalid --side-effect-level `{other}`; expected none, filesystem, network, or consequential"
        ))),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JunitAdapterConfig {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlaywrightAdapterConfig {
    pub name: String,
    pub path: PathBuf,
}

impl MenoConfig {
    pub fn load(path: &Path) -> Result<Self, CliError> {
        let text = fs::read_to_string(path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                CliError::msg(format!(
                    "no meno.toml at {}; run `meno init`",
                    path.display()
                ))
            } else {
                err.into()
            }
        })?;
        let cfg: Self = toml::from_str(&text)?;
        Ok(cfg)
    }

    pub fn command_specs(&self) -> Vec<CommandSpec> {
        self.adapters
            .command
            .iter()
            .map(CommandAdapterConfig::to_spec)
            .collect()
    }
}

impl CommandAdapterConfig {
    pub fn to_spec(&self) -> CommandSpec {
        CommandSpec {
            name: self.name.clone(),
            argv: self.argv.clone(),
            can_invoke: self.can_invoke,
            side_effect_level: self.side_effect_level,
        }
    }
}
