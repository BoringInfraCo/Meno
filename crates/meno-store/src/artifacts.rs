use crate::db::Store;
use crate::error::{Result, StoreError};
use meno_core::canonical::{sha256_bytes, sha256_hex};
use meno_core::envelope::ArtifactRef;
use meno_core::redaction::{redact_bytes, RedactionAction};
use rusqlite::params;
use std::path::Path;

impl Store {
    /// Store `bytes` content-addressed as `artifacts/<sha256>`.
    ///
    /// Existing files are not rewritten. Rejects payloads that redaction refuses
    /// (for example PEM private keys).
    pub fn put(&self, bytes: &[u8], media_type: &str) -> Result<ArtifactRef> {
        match redact_bytes(bytes) {
            RedactionAction::Reject(_) => return Err(StoreError::RedactionRejected),
            RedactionAction::Clean | RedactionAction::Redacted(_) => {}
        }

        let artifacts_dir = self.artifacts_dir()?;
        let sha256 = sha256_hex(bytes);
        let relative_path = format!("artifacts/{sha256}");
        let dest = artifacts_dir.join(&sha256);

        if !dest.exists() {
            let tmp = artifacts_dir.join(format!(".{sha256}.tmp"));
            std::fs::write(&tmp, bytes)?;
            match std::fs::rename(&tmp, &dest) {
                Ok(()) => {}
                Err(err) => {
                    let _ = std::fs::remove_file(&tmp);
                    if !dest.exists() {
                        return Err(err.into());
                    }
                }
            }
        }

        let now = crate::db::now_iso8601()?;
        self.conn.execute(
            "INSERT OR IGNORE INTO artifacts (sha256, media_type, size, relative_path, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![sha256, media_type, bytes.len() as i64, relative_path, now],
        )?;

        Ok(ArtifactRef {
            sha256,
            media_type: Some(media_type.to_owned()),
            size: bytes.len() as u64,
            relative_path,
        })
    }

    /// Read artifact bytes and re-hash them. Mismatch is an integrity error.
    pub fn get(&self, sha256: &str) -> Result<Vec<u8>> {
        let sha256 = normalize_sha256(sha256)?;
        let bytes = self.read_file(&sha256)?;
        check_hash(&sha256, &bytes)?;
        Ok(bytes)
    }

    /// Re-hash the on-disk artifact and compare it to `sha256`.
    pub fn verify(&self, sha256: &str) -> Result<()> {
        let sha256 = normalize_sha256(sha256)?;
        let bytes = self.read_file(&sha256)?;
        check_hash(&sha256, &bytes)
    }

    fn artifacts_dir(&self) -> Result<&Path> {
        self.artifacts_dir
            .as_deref()
            .ok_or(StoreError::NoArtifactsDir)
    }

    fn read_file(&self, sha256: &str) -> Result<Vec<u8>> {
        let path = self.artifacts_dir()?.join(sha256);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(bytes),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Err(StoreError::ArtifactNotFound(sha256.to_owned()))
            }
            Err(err) => Err(err.into()),
        }
    }
}

fn normalize_sha256(sha256: &str) -> Result<String> {
    let s = sha256.trim().to_ascii_lowercase();
    if s.len() != 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(StoreError::InvalidDigest(sha256.to_owned()));
    }
    Ok(s)
}

fn check_hash(expected: &str, bytes: &[u8]) -> Result<()> {
    let actual = hex::encode(sha256_bytes(bytes));
    if actual != expected {
        return Err(StoreError::IntegrityMismatch {
            expected: expected.to_owned(),
            actual,
        });
    }
    Ok(())
}
