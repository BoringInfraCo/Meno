//! Generic command adapter: run a process and normalize the exit into an envelope.
//!
//! Exit code is an observation. This adapter never writes a verdict.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use meno_core::envelope::{
    new_ulid, seal_envelope, ArtifactRef, Envelope, Observation, Provenance, Source, Trust,
    TrustBasis, TrustOrigin, TrustRelation,
};
use meno_core::redaction::RedactionAction;
use serde::Deserialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::contract::{
    Adapter, AdapterError, AdapterSafety, DetectContext, DetectResult, InputBundle, SideEffectLevel,
};

pub struct CommandSpec {
    pub name: String,
    pub argv: Vec<String>,
    pub can_invoke: bool,
    pub side_effect_level: SideEffectLevel,
}

impl CommandSpec {
    /// Auto-run only when can_invoke && side_effect_level == None
    pub fn may_auto_invoke(&self) -> bool {
        self.can_invoke && self.side_effect_level == SideEffectLevel::None
    }

    /// Hybrid invoke gate. Consequential commands never run.
    pub fn invoke_allowed(&self, confirm_invoke: bool) -> bool {
        if !self.can_invoke {
            false
        } else if self.side_effect_level == SideEffectLevel::None {
            true
        } else if self.side_effect_level == SideEffectLevel::Consequential {
            false
        } else {
            confirm_invoke
        }
    }
}

pub struct CommandOutcome {
    pub exit_code: i32,
    pub duration_ms: u64,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Run argv[0] with argv[1..] in cwd. Inherit PATH. Do NOT dump environment into the outcome.
/// Stdin null. Capture stdout/stderr. duration from Instant. exit_code from ExitStatus::code().unwrap_or(255).
pub fn run_command(spec: &CommandSpec, cwd: &Path) -> Result<CommandOutcome, AdapterError> {
    let (program, args) = spec
        .argv
        .split_first()
        .ok_or_else(|| AdapterError::message("command argv is empty"))?;

    let start = Instant::now();
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()?;
    let duration_ms = start.elapsed().as_millis() as u64;
    let exit_code = output.status.code().unwrap_or(255);

    Ok(CommandOutcome {
        exit_code,
        duration_ms,
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

/// Build kind = "command.result" envelope. Observation type "command.exit" with fields:
///   { "exit_code": <i32>, "duration_ms": <u64> }
/// Source.name = spec.name, source.argv = spec.argv.
/// Trust: origin=machine, reproducible=true, basis=observed, relation=direct.
/// Provenance.producer = "meno.command", cwd = cwd display.
/// Attach stdout_ref/stderr_ref if Some.
/// Call seal_envelope before return.
/// Exit code is an observation; this function MUST NOT decide Proven/Disproven.
pub fn normalize_command(
    spec: &CommandSpec,
    outcome: &CommandOutcome,
    subject_id: &str,
    cwd: &Path,
    stdout_ref: Option<ArtifactRef>,
    stderr_ref: Option<ArtifactRef>,
) -> Result<Envelope, AdapterError> {
    let mut artifact_refs = Vec::new();
    if let Some(stdout_ref) = stdout_ref {
        artifact_refs.push(stdout_ref);
    }
    if let Some(stderr_ref) = stderr_ref {
        artifact_refs.push(stderr_ref);
    }

    let captured_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|err| AdapterError::message(err.to_string()))?;

    let mut envelope = Envelope {
        id: new_ulid(),
        kind: "command.result".to_string(),
        subject_id: subject_id.to_string(),
        source: Source {
            name: spec.name.clone(),
            version: None,
            argv: Some(spec.argv.clone()),
            config_digest: None,
        },
        observations: vec![Observation {
            type_name: "command.exit".to_string(),
            fields: serde_json::json!({
                "exit_code": outcome.exit_code,
                "duration_ms": outcome.duration_ms,
            }),
        }],
        artifact_refs,
        captured_at,
        provenance: Provenance {
            actor: None,
            producer: "meno.command".to_string(),
            producer_version: None,
            host: None,
            cwd: Some(cwd.display().to_string()),
        },
        integrity: None,
        trust: Trust {
            origin: TrustOrigin::Machine,
            reproducible: true,
            basis: TrustBasis::Observed,
            relation: TrustRelation::Direct,
        },
        source_metadata: serde_json::json!({}),
    };
    seal_envelope(&mut envelope).map_err(|err| AdapterError::message(err.to_string()))?;
    Ok(envelope)
}

pub struct CommandAdapter {
    pub spec: CommandSpec,
}

impl Adapter for CommandAdapter {
    fn name(&self) -> &str {
        &self.spec.name
    }

    fn safety(&self) -> AdapterSafety {
        AdapterSafety {
            can_collect: true,
            can_invoke: self.spec.can_invoke,
            side_effect_level: self.spec.side_effect_level,
            requires_confirmation: self.spec.side_effect_level != SideEffectLevel::None,
        }
    }

    fn detect(&self, _ctx: &DetectContext) -> Result<DetectResult, AdapterError> {
        Ok(DetectResult {
            detected: !self.spec.argv.is_empty(),
            detail: self.spec.argv.first().cloned(),
        })
    }

    fn normalize(&self, input: &InputBundle) -> Result<Envelope, AdapterError> {
        if input.payload.is_empty() {
            return Err(AdapterError::message(
                "empty payload: use normalize_command after run_command",
            ));
        }

        let payload: CommandNormalizePayload = serde_json::from_slice(&input.payload)
            .map_err(|err| AdapterError::message(format!("invalid command payload: {err}")))?;

        let outcome = CommandOutcome {
            exit_code: payload.exit_code,
            duration_ms: payload.duration_ms,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        let cwd = input.path.as_deref().unwrap_or_else(|| Path::new("."));
        normalize_command(&self.spec, &outcome, &payload.subject_id, cwd, None, None)
    }

    fn validate(&self, envelope: &Envelope) -> Result<(), AdapterError> {
        meno_core::verify_envelope(envelope).map_err(|err| AdapterError::message(err.to_string()))
    }
}

#[derive(Debug, Deserialize)]
struct CommandNormalizePayload {
    subject_id: String,
    exit_code: i32,
    duration_ms: u64,
}

pub fn redact_output(bytes: &[u8]) -> Vec<u8> {
    match meno_core::redact_bytes(bytes) {
        RedactionAction::Clean => bytes.to_vec(),
        RedactionAction::Redacted(b) => b,
        RedactionAction::Reject(_) => b"***REDACTED***".to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meno_core::envelope::Envelope;
    use meno_core::verdict::Verdict;
    use serde_json::json;
    use tempfile::TempDir;

    fn spec(name: &str, argv: &[&str]) -> CommandSpec {
        CommandSpec {
            name: name.to_string(),
            argv: argv.iter().map(|s| s.to_string()).collect(),
            can_invoke: true,
            side_effect_level: SideEffectLevel::None,
        }
    }

    #[test]
    fn true_exits_zero() {
        let dir = TempDir::new().expect("tempdir");
        let spec = spec("true", &["true"]);
        let outcome = run_command(&spec, dir.path()).expect("run true");
        assert_eq!(outcome.exit_code, 0);
    }

    #[test]
    fn false_exits_nonzero() {
        let dir = TempDir::new().expect("tempdir");
        let spec = spec("false", &["false"]);
        let outcome = run_command(&spec, dir.path()).expect("run false");
        assert_ne!(outcome.exit_code, 0);
        assert_eq!(outcome.exit_code, 1);
    }

    #[test]
    fn normalize_command_result_is_observation_not_verdict() {
        let dir = TempDir::new().expect("tempdir");
        let spec = spec("true", &["true"]);
        let outcome = run_command(&spec, dir.path()).expect("run true");
        let subject_id = "0".repeat(64);
        let stdout_ref = ArtifactRef {
            sha256: "aa".repeat(32),
            media_type: Some("text/plain".into()),
            size: outcome.stdout.len() as u64,
            relative_path: "stdout".into(),
        };
        let envelope = normalize_command(
            &spec,
            &outcome,
            &subject_id,
            dir.path(),
            Some(stdout_ref.clone()),
            None,
        )
        .expect("normalize");

        let _: Envelope = envelope.clone();
        let _ = std::any::type_name::<Verdict>();

        assert_eq!(envelope.kind, "command.result");
        assert_eq!(envelope.observations.len(), 1);
        assert_eq!(envelope.observations[0].type_name, "command.exit");
        assert_eq!(
            envelope.observations[0].fields["exit_code"],
            json!(outcome.exit_code)
        );
        assert_eq!(
            envelope.observations[0].fields["duration_ms"],
            json!(outcome.duration_ms)
        );
        assert_eq!(envelope.source.name, spec.name);
        assert_eq!(envelope.source.argv.as_deref(), Some(spec.argv.as_slice()));
        assert_eq!(envelope.provenance.producer, "meno.command");
        let cwd = dir.path().display().to_string();
        assert_eq!(envelope.provenance.cwd.as_deref(), Some(cwd.as_str()));
        assert_eq!(envelope.trust.origin, TrustOrigin::Machine);
        assert!(envelope.trust.reproducible);
        assert_eq!(envelope.trust.basis, TrustBasis::Observed);
        assert_eq!(envelope.trust.relation, TrustRelation::Direct);
        assert_eq!(envelope.artifact_refs, vec![stdout_ref]);

        for obs in &envelope.observations {
            assert!(
                !obs.type_name.to_ascii_lowercase().contains("verdict"),
                "adapters must not emit Verdict types, got {}",
                obs.type_name
            );
            assert!(
                !obs.type_name.to_ascii_lowercase().contains("proven"),
                "exit code is an observation, got {}",
                obs.type_name
            );
        }
        let value = serde_json::to_value(&envelope).expect("json");
        assert!(value.get("verdict").is_none());
        meno_core::verify_envelope(&envelope).expect("sealed envelope verifies");
    }

    #[test]
    fn may_auto_invoke_true_only_for_none_and_can_invoke() {
        let mut command = spec("true", &["true"]);
        assert!(command.may_auto_invoke());

        command.can_invoke = false;
        assert!(!command.may_auto_invoke());

        command.can_invoke = true;
        command.side_effect_level = SideEffectLevel::Filesystem;
        assert!(!command.may_auto_invoke());
        assert!(!command.invoke_allowed(false));
        assert!(command.invoke_allowed(true));
        command.side_effect_level = SideEffectLevel::Network;
        assert!(!command.may_auto_invoke());
        assert!(command.invoke_allowed(true));
        command.side_effect_level = SideEffectLevel::Consequential;
        assert!(!command.may_auto_invoke());
        assert!(!command.invoke_allowed(false));
        assert!(!command.invoke_allowed(true));

        command.side_effect_level = SideEffectLevel::None;
        command.can_invoke = false;
        assert!(!command.may_auto_invoke());

        let adapter = CommandAdapter {
            spec: CommandSpec {
                name: "true".into(),
                argv: vec!["true".into()],
                can_invoke: true,
                side_effect_level: SideEffectLevel::Filesystem,
            },
        };
        let safety = adapter.safety();
        assert!(safety.can_collect);
        assert!(safety.can_invoke);
        assert_eq!(safety.side_effect_level, SideEffectLevel::Filesystem);
        assert!(safety.requires_confirmation);

        let auto = CommandAdapter {
            spec: spec("true", &["true"]),
        };
        let safety = auto.safety();
        assert!(safety.can_invoke);
        assert_eq!(safety.side_effect_level, SideEffectLevel::None);
        assert!(!safety.requires_confirmation);
        assert!(
            auto.detect(&DetectContext {
                project_root: Path::new(".").to_path_buf(),
            })
            .expect("detect")
            .detected
        );
    }

    #[test]
    fn redact_output_redacts_password_secret() {
        let out = redact_output(b"password=secret");
        assert_eq!(out, b"password=***REDACTED***");
        assert_eq!(redact_output(b"hello"), b"hello");
        assert_eq!(
            redact_output(b"-----BEGIN PRIVATE KEY-----\nxx"),
            b"***REDACTED***"
        );
    }

    #[test]
    fn adapter_normalize_requires_payload_json() {
        let adapter = CommandAdapter {
            spec: spec("true", &["true"]),
        };
        let err = adapter
            .normalize(&InputBundle {
                kind: "command.result".into(),
                payload: Vec::new(),
                path: None,
            })
            .expect_err("empty payload");
        assert!(err.to_string().contains("normalize_command"), "{err}");

        let dir = TempDir::new().expect("tempdir");
        let envelope = adapter
            .normalize(&InputBundle {
                kind: "command.result".into(),
                payload: serde_json::to_vec(&json!({
                    "subject_id": "0".repeat(64),
                    "exit_code": 0,
                    "duration_ms": 1,
                }))
                .expect("payload json"),
                path: Some(dir.path().to_path_buf()),
            })
            .expect("normalize from json");
        assert_eq!(envelope.kind, "command.result");
        assert_eq!(envelope.observations[0].fields["exit_code"], json!(0));
        adapter.validate(&envelope).expect("validate");
    }
}
