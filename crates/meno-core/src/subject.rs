use crate::canonical::{put_len_bytes, put_tagged_bytes, put_u32, sha256_bytes};
use crate::error::{MenoError, Result};
use serde_json::{json, Map, Value};

pub const SUBJECT_MAGIC: &[u8] = b"meno-subject-v1\n";
pub const SUBJECT_IDENTITY_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub origin: Vec<u8>,
    pub head: Vec<u8>,
    pub index: Vec<IndexEntry>,
    pub worktree: Vec<WorktreeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub mode: u32,
    pub stage: u32,
    pub digest: [u8; 32],
    pub path: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub status: WorktreeStatus,
    pub mode: u32,
    pub digest: [u8; 32],
    pub path: Vec<u8>,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeStatus {
    Modified = 1,
    Added = 2,
    Deleted = 3,
}

impl WorktreeStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Modified => "modified",
            Self::Added => "added",
            Self::Deleted => "deleted",
        }
    }

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "modified" => Ok(Self::Modified),
            "added" => Ok(Self::Added),
            "deleted" => Ok(Self::Deleted),
            other => Err(MenoError::Parse(format!(
                "unknown worktree status {other:?}"
            ))),
        }
    }
}

pub fn encode_snapshot(s: &Snapshot) -> Vec<u8> {
    let mut buf = Vec::from(SUBJECT_MAGIC);
    put_tagged_bytes(&mut buf, 0x01, &s.origin);
    put_tagged_bytes(&mut buf, 0x02, &s.head);

    let mut index: Vec<&IndexEntry> = s.index.iter().collect();
    index.sort_by(|a, b| a.path.as_slice().cmp(b.path.as_slice()));
    for entry in index {
        buf.push(0x03);
        put_u32(&mut buf, entry.mode);
        put_u32(&mut buf, entry.stage);
        buf.extend_from_slice(&entry.digest);
        put_len_bytes(&mut buf, &entry.path);
    }

    let mut worktree: Vec<&WorktreeEntry> = s.worktree.iter().collect();
    worktree.sort_by(|a, b| a.path.as_slice().cmp(b.path.as_slice()));
    for entry in worktree {
        buf.push(0x04);
        buf.push(entry.status as u8);
        put_u32(&mut buf, entry.mode);
        buf.extend_from_slice(&entry.digest);
        put_len_bytes(&mut buf, &entry.path);
    }
    buf
}

pub fn hash_snapshot(s: &Snapshot) -> [u8; 32] {
    sha256_bytes(&encode_snapshot(s))
}

pub fn subject_id_hex(s: &Snapshot) -> String {
    hex::encode(hash_snapshot(s))
}

/// Normalize a git remote URL. Empty input stays empty; local filesystem paths
/// are never invented as a fallback.
pub fn normalize_origin(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }

    let mut s = scp_to_ssh(raw);

    if let Some(i) = s.find(['?', '#']) {
        s.truncate(i);
    }

    s = lowercase_scheme_netloc(&s);

    while s.ends_with('/') {
        s.pop();
    }
    if let Some(stripped) = s.strip_suffix(".git") {
        s = stripped.to_string();
    }
    while s.ends_with('/') {
        s.pop();
    }
    s
}

fn scp_to_ssh(raw: &str) -> String {
    if raw.contains("://") {
        return raw.to_string();
    }
    let Some(at) = raw.find('@') else {
        return raw.to_string();
    };
    let after_at = &raw[at + 1..];
    let Some(colon) = after_at.find(':') else {
        return raw.to_string();
    };
    let colon = at + 1 + colon;
    let user_host = &raw[..colon];
    if user_host.contains('/') {
        return raw.to_string();
    }
    let path = raw[colon + 1..].trim_start_matches('/');
    format!("ssh://{user_host}/{path}")
}

fn lowercase_scheme_netloc(s: &str) -> String {
    let Some(scheme_end) = s.find("://") else {
        return s.to_string();
    };
    let scheme = s[..scheme_end].to_ascii_lowercase();
    let rest = &s[scheme_end + 3..];
    let (netloc, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    format!("{scheme}://{}{path}", netloc.to_ascii_lowercase())
}

pub fn snapshot_to_json(s: &Snapshot) -> Value {
    json!({
        "origin": bytes_to_string(&s.origin),
        "head": bytes_to_string(&s.head),
        "index": s.index.iter().map(|e| json!({
            "mode": e.mode,
            "stage": e.stage,
            "sha256": hex::encode(e.digest),
            "path": bytes_to_string(&e.path),
        })).collect::<Vec<_>>(),
        "worktree": s.worktree.iter().map(|e| json!({
            "status": e.status.as_str(),
            "mode": e.mode,
            "sha256": hex::encode(e.digest),
            "path": bytes_to_string(&e.path),
        })).collect::<Vec<_>>(),
    })
}

pub fn snapshot_from_json(value: &Value) -> Result<Snapshot> {
    let obj = value
        .as_object()
        .ok_or_else(|| MenoError::Parse("snapshot must be an object".into()))?;
    let origin = optional_string_bytes(obj, "origin")?;
    let head = optional_string_bytes(obj, "head")?;
    let index = match obj.get("index") {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(index_entry_from_json)
            .collect::<Result<_>>()?,
        Some(_) => return Err(MenoError::Parse("index must be an array".into())),
    };
    let worktree = match obj.get("worktree") {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(worktree_entry_from_json)
            .collect::<Result<_>>()?,
        Some(_) => return Err(MenoError::Parse("worktree must be an array".into())),
    };
    Ok(Snapshot {
        origin,
        head,
        index,
        worktree,
    })
}

fn bytes_to_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn optional_string_bytes(obj: &Map<String, Value>, key: &str) -> Result<Vec<u8>> {
    match obj.get(key) {
        None => Ok(Vec::new()),
        Some(Value::String(s)) => Ok(s.as_bytes().to_vec()),
        Some(_) => Err(MenoError::Parse(format!("{key} must be a string"))),
    }
}

fn index_entry_from_json(value: &Value) -> Result<IndexEntry> {
    let obj = value
        .as_object()
        .ok_or_else(|| MenoError::Parse("index entry must be an object".into()))?;
    Ok(IndexEntry {
        mode: json_u32(obj, "mode")?,
        stage: json_u32(obj, "stage")?,
        digest: json_sha256(obj)?,
        path: json_path(obj)?,
    })
}

fn worktree_entry_from_json(value: &Value) -> Result<WorktreeEntry> {
    let obj = value
        .as_object()
        .ok_or_else(|| MenoError::Parse("worktree entry must be an object".into()))?;
    let status = obj
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MenoError::Parse("worktree status must be a string".into()))?;
    Ok(WorktreeEntry {
        status: WorktreeStatus::from_str(status)?,
        mode: json_u32(obj, "mode")?,
        digest: json_sha256(obj)?,
        path: json_path(obj)?,
    })
}

fn json_u32(obj: &Map<String, Value>, key: &str) -> Result<u32> {
    let n = obj
        .get(key)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| MenoError::Parse(format!("{key} must be a u32")))?;
    u32::try_from(n).map_err(|_| MenoError::Parse(format!("{key} out of range")))
}

fn json_sha256(obj: &Map<String, Value>) -> Result<[u8; 32]> {
    let s = obj
        .get("sha256")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MenoError::Parse("sha256 must be a hex string".into()))?;
    let bytes = hex::decode(s).map_err(|e| MenoError::Parse(format!("invalid sha256 hex: {e}")))?;
    <[u8; 32]>::try_from(bytes).map_err(|_| MenoError::Parse("sha256 must be 32 bytes".into()))
}

fn json_path(obj: &Map<String, Value>) -> Result<Vec<u8>> {
    obj.get("path")
        .and_then(|v| v.as_str())
        .map(|s| s.as_bytes().to_vec())
        .ok_or_else(|| MenoError::Parse("path must be a string".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn fixture_snapshot() -> Snapshot {
        Snapshot {
            origin: b"https://example.com/acme/meno".to_vec(),
            head: b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_vec(),
            index: vec![IndexEntry {
                mode: 33188,
                stage: 0,
                digest: [0x11; 32],
                path: b"src/lib.rs".to_vec(),
            }],
            worktree: vec![WorktreeEntry {
                status: WorktreeStatus::Modified,
                mode: 33188,
                digest: [0x22; 32],
                path: b"src/lib.rs".to_vec(),
            }],
        }
    }

    #[test]
    fn encode_and_hash_are_frozen_and_path_order_independent() {
        let snap = fixture_snapshot();
        let encoded = encode_snapshot(&snap);
        assert!(encoded.starts_with(SUBJECT_MAGIC));
        assert_eq!(
            subject_id_hex(&snap),
            "70c2e9c86aa569c1261b923dd0d6fc49af63edddb7060f3b7897c214b0357ebf"
        );

        let shuffled = Snapshot {
            origin: snap.origin.clone(),
            head: snap.head.clone(),
            index: vec![
                IndexEntry {
                    mode: 33188,
                    stage: 0,
                    digest: [0xbb; 32],
                    path: b"z.rs".to_vec(),
                },
                IndexEntry {
                    mode: 33188,
                    stage: 0,
                    digest: [0xaa; 32],
                    path: b"a.rs".to_vec(),
                },
            ],
            worktree: vec![
                WorktreeEntry {
                    status: WorktreeStatus::Added,
                    mode: 33188,
                    digest: [0xdd; 32],
                    path: b"z.rs".to_vec(),
                },
                WorktreeEntry {
                    status: WorktreeStatus::Deleted,
                    mode: 0,
                    digest: [0; 32],
                    path: b"a.rs".to_vec(),
                },
            ],
        };
        let sorted = Snapshot {
            origin: shuffled.origin.clone(),
            head: shuffled.head.clone(),
            index: vec![shuffled.index[1].clone(), shuffled.index[0].clone()],
            worktree: vec![shuffled.worktree[1].clone(), shuffled.worktree[0].clone()],
        };
        assert_eq!(encode_snapshot(&shuffled), encode_snapshot(&sorted));
        assert_eq!(hash_snapshot(&shuffled), hash_snapshot(&sorted));
    }

    #[test]
    fn timestamps_are_not_part_of_identity() {
        // Snapshot has no mtime fields; identical bytes hash equal even if the
        // files were "touched" at different times before collection.
        let a = fixture_snapshot();
        let b = fixture_snapshot();
        assert_eq!(encode_snapshot(&a), encode_snapshot(&b));
        assert_eq!(subject_id_hex(&a), subject_id_hex(&b));
    }

    #[test]
    fn normalize_origin_cases() {
        assert_eq!(
            normalize_origin("git@github.com:Acme/meno.git"),
            "ssh://git@github.com/Acme/meno"
        );
        assert_eq!(
            normalize_origin("https://GitHub.COM/Acme/meno.git"),
            "https://github.com/Acme/meno"
        );
        assert_eq!(
            normalize_origin("https://github.com/Acme/meno.git?foo=1#bar"),
            "https://github.com/Acme/meno"
        );
        assert_eq!(normalize_origin(""), "");
        assert_eq!(normalize_origin("   "), "");
        assert_eq!(
            normalize_origin("https://example.com/acme/meno/"),
            "https://example.com/acme/meno"
        );
    }

    #[test]
    fn json_roundtrip() {
        let snap = fixture_snapshot();
        let value = snapshot_to_json(&snap);
        let parsed = snapshot_from_json(&value).unwrap();
        assert_eq!(snap, parsed);
        assert_eq!(value["worktree"][0]["status"].as_str(), Some("modified"));
        assert_eq!(value["index"][0]["mode"].as_u64(), Some(33188));
    }

    #[test]
    fn matches_python_reference_if_present() {
        let script =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/reference/subject_hash.py");
        if !script.is_file() {
            return;
        }
        let snap = fixture_snapshot();
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("snapshot.json");
        std::fs::write(
            &json_path,
            serde_json::to_vec(&snapshot_to_json(&snap)).unwrap(),
        )
        .unwrap();
        let output = match Command::new("python3")
            .arg(&script)
            .arg("--snapshot")
            .arg(&json_path)
            .output()
        {
            Ok(output) => output,
            Err(_) => return,
        };
        assert!(
            output.status.success(),
            "python reference failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let hex = String::from_utf8_lossy(&output.stdout);
        assert_eq!(hex.trim(), subject_id_hex(&snap));

        let self_test = match Command::new("python3")
            .arg(&script)
            .arg("--self-test")
            .output()
        {
            Ok(output) => output,
            Err(_) => return,
        };
        if self_test.status.success() {
            let obj = json!({
                "origin": "https://example.com/acme/meno",
                "head": "0123456789abcdef0123456789abcdef01234567",
                "index": [{
                    "mode": 33188,
                    "stage": 0,
                    "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                    "path": "a.txt"
                }],
                "worktree": [{
                    "status": "modified",
                    "mode": 33188,
                    "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "path": "a.txt"
                }]
            });
            let parsed = snapshot_from_json(&obj).unwrap();
            assert_eq!(
                subject_id_hex(&parsed),
                String::from_utf8_lossy(&self_test.stdout).trim()
            );
        }
    }
}
