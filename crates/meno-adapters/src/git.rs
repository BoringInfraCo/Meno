//! Git worktree snapshot collection (I/O). Canonical encoding lives in `meno-core`.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use meno_core::subject::{normalize_origin, IndexEntry, Snapshot, WorktreeEntry, WorktreeStatus};
use sha2::{Digest, Sha256};

use crate::contract::AdapterError;

const MODE_SYMLINK: u32 = 0o120000;
const MODE_GITLINK: u32 = 0o160000;
const MODE_EXEC: u32 = 0o100755;
const MODE_FILE: u32 = 0o100644;

/// Collect origin, HEAD, full index, and worktree overlay for `repo`.
///
/// Origin is `remote.origin.url` after `normalize_origin`, or empty — never a
/// filesystem path fallback.
pub fn collect_snapshot(repo: &Path) -> Result<Snapshot, AdapterError> {
    let root = git_toplevel(repo)?;
    let origin = match git_origin(&root)? {
        Some(raw) => normalize_origin(&raw),
        None => String::new(),
    };
    let head = git_head(&root)?;

    let collected = collect_index(&root)?;
    let mut index: Vec<IndexEntry> = collected
        .iter()
        .map(|rec| IndexEntry {
            mode: rec.mode,
            stage: rec.stage,
            digest: rec.digest,
            path: rec.path.clone(),
        })
        .collect();
    index.sort_by(|a, b| a.path.cmp(&b.path).then(a.stage.cmp(&b.stage)));

    let mut worktree = collect_worktree(&root, &collected)?;
    worktree.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(Snapshot {
        origin: origin.into_bytes(),
        head: head.into_bytes(),
        index,
        worktree,
    })
}

fn git_toplevel(repo: &Path) -> Result<PathBuf, AdapterError> {
    let inside = git_stdout(repo, &["rev-parse", "--is-inside-work-tree"]);
    match inside {
        Ok(out) if out.trim_ascii() == b"true" => {}
        Ok(_) => return Err(AdapterError::git("not a git work tree")),
        Err(err) => return Err(err),
    }
    let top = git_stdout(repo, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(OsStr::from_bytes(top.trim_ascii())))
}

fn git_origin(repo: &Path) -> Result<Option<String>, AdapterError> {
    let output = git_output(repo, &["config", "--get", "remote.origin.url"])?;
    if !output.status.success() {
        return Ok(None);
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        Ok(None)
    } else {
        Ok(Some(raw))
    }
}

fn git_head(repo: &Path) -> Result<String, AdapterError> {
    let output = git_output(repo, &["rev-parse", "--verify", "--quiet", "HEAD"])?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_ascii_lowercase())
}

struct IndexRec {
    mode: u32,
    stage: u32,
    digest: [u8; 32],
    path: Vec<u8>,
    git_sha: String,
}

fn collect_index(repo: &Path) -> Result<Vec<IndexRec>, AdapterError> {
    let data = git_stdout(repo, &["ls-files", "-s", "-z"])?;
    let mut entries = Vec::new();
    let mut blob_cache: HashMap<String, [u8; 32]> = HashMap::new();
    for rec in split_z(&data) {
        let (mode, object, stage, path) = parse_ls_files_s(rec)?;
        let digest = if mode == MODE_GITLINK {
            sha256_bytes(object.as_bytes())
        } else if let Some(hit) = blob_cache.get(&object) {
            *hit
        } else {
            let bytes = git_stdout(repo, &["cat-file", "blob", &object])?;
            let digest = sha256_bytes(&bytes);
            blob_cache.insert(object.clone(), digest);
            digest
        };
        entries.push(IndexRec {
            mode,
            stage,
            digest,
            path,
            git_sha: object,
        });
    }
    Ok(entries)
}

fn collect_worktree(repo: &Path, index: &[IndexRec]) -> Result<Vec<WorktreeEntry>, AdapterError> {
    // Lowest stage wins for overlay comparison (matches spec/reference/subject_hash.py).
    let mut by_path: HashMap<Vec<u8>, &IndexRec> = HashMap::new();
    for rec in index {
        let replace = match by_path.get(&rec.path) {
            Some(prev) => rec.stage < prev.stage,
            None => true,
        };
        if replace {
            by_path.insert(rec.path.clone(), rec);
        }
    }

    let mut overlay = Vec::new();
    for (path, rec) in &by_path {
        if excluded_from_overlay(path) {
            continue;
        }
        let full = join_repo(repo, path);
        if !path_lexists(&full) {
            overlay.push(deleted_entry(path));
            continue;
        }
        if rec.mode == MODE_GITLINK {
            let head = submodule_head(&full)?;
            if head != rec.git_sha {
                overlay.push(WorktreeEntry {
                    status: WorktreeStatus::Modified,
                    mode: MODE_GITLINK,
                    digest: sha256_bytes(head.as_bytes()),
                    path: path.clone(),
                });
            }
            continue;
        }
        match disk_mode_and_hash(&full)? {
            Some((mode, digest)) if mode == rec.mode && digest == rec.digest => {}
            Some((mode, digest)) => overlay.push(WorktreeEntry {
                status: WorktreeStatus::Modified,
                mode,
                digest,
                path: path.clone(),
            }),
            None => overlay.push(deleted_entry(path)),
        }
    }

    let others = git_stdout(repo, &["ls-files", "-z", "--others", "--exclude-standard"])?;
    for path in split_z(&others) {
        let path = path.to_vec();
        if excluded_from_overlay(&path) {
            continue;
        }
        let full = join_repo(repo, &path);
        if let Some((mode, digest)) = disk_mode_and_hash(&full)? {
            overlay.push(WorktreeEntry {
                status: WorktreeStatus::Added,
                mode,
                digest,
                path,
            });
        }
    }
    Ok(overlay)
}

fn deleted_entry(path: &[u8]) -> WorktreeEntry {
    WorktreeEntry {
        status: WorktreeStatus::Deleted,
        mode: 0,
        digest: [0u8; 32],
        path: path.to_vec(),
    }
}

fn path_lexists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn submodule_head(path: &Path) -> Result<String, AdapterError> {
    let output = git_output(path, &["rev-parse", "HEAD"])?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_ascii_lowercase())
}

fn disk_mode_and_hash(path: &Path) -> Result<Option<(u32, [u8; 32])>, AdapterError> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let ft = meta.file_type();
    if ft.is_symlink() {
        let target = fs::read_link(path)?;
        return Ok(Some((
            MODE_SYMLINK,
            sha256_bytes(target.as_os_str().as_bytes()),
        )));
    }
    if !ft.is_file() {
        return Ok(None);
    }
    let mode = if meta.permissions().mode() & 0o100 != 0 {
        MODE_EXEC
    } else {
        MODE_FILE
    };
    Ok(Some((mode, sha256_file(path)?)))
}

fn excluded_from_overlay(path: &[u8]) -> bool {
    path == b".git" || path == b".meno" || path.starts_with(b".git/") || path.starts_with(b".meno/")
}

fn join_repo(repo: &Path, rel: &[u8]) -> PathBuf {
    repo.join(OsStr::from_bytes(rel))
}

fn parse_ls_files_s(rec: &[u8]) -> Result<(u32, String, u32, Vec<u8>), AdapterError> {
    let sp1 = rec
        .iter()
        .position(|&b| b == b' ')
        .ok_or_else(|| AdapterError::git("malformed ls-files -s record"))?;
    let sp2 = rec[sp1 + 1..]
        .iter()
        .position(|&b| b == b' ')
        .map(|i| i + sp1 + 1)
        .ok_or_else(|| AdapterError::git("malformed ls-files -s record"))?;
    let tab = rec[sp2 + 1..]
        .iter()
        .position(|&b| b == b'\t')
        .map(|i| i + sp2 + 1)
        .ok_or_else(|| AdapterError::git("malformed ls-files -s record"))?;
    let mode = parse_octal(&rec[..sp1])?;
    let object = std::str::from_utf8(&rec[sp1 + 1..sp2])
        .map_err(|_| AdapterError::git("non-utf8 git object name"))?
        .to_ascii_lowercase();
    let stage: u32 = std::str::from_utf8(&rec[sp2 + 1..tab])
        .map_err(|_| AdapterError::git("non-utf8 git stage"))?
        .parse()
        .map_err(|_| AdapterError::git("invalid git stage"))?;
    let path = rec[tab + 1..].to_vec();
    Ok((mode, object, stage, path))
}

fn parse_octal(bytes: &[u8]) -> Result<u32, AdapterError> {
    let s = std::str::from_utf8(bytes).map_err(|_| AdapterError::git("non-utf8 git mode"))?;
    u32::from_str_radix(s, 8).map_err(|_| AdapterError::git(format!("invalid git mode {s:?}")))
}

fn split_z(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.split(|b| *b == 0).filter(|s| !s.is_empty())
}

fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

fn sha256_file(path: &Path) -> Result<[u8; 32], AdapterError> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().into())
}

fn git_output(repo: &Path, args: &[&str]) -> Result<Output, AdapterError> {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(AdapterError::from)
}

fn git_stdout(repo: &Path, args: &[&str]) -> Result<Vec<u8>, AdapterError> {
    let output = git_output(repo, args)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AdapterError::git(format!(
            "git -C {} {} failed: {stderr}",
            repo.display(),
            args.join(" ")
        )));
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use meno_core::subject::subject_id_hex;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn git_ok(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .stdin(Stdio::null())
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_repo() -> TempDir {
        let dir = TempDir::new().expect("tempdir");
        git_ok(dir.path(), &["init", "-q", "-b", "main"]);
        git_ok(dir.path(), &["config", "user.email", "meno@example.com"]);
        git_ok(dir.path(), &["config", "user.name", "Meno Test"]);
        git_ok(dir.path(), &["config", "commit.gpgsign", "false"]);
        git_ok(dir.path(), &["config", "core.autocrlf", "false"]);
        dir
    }

    fn commit_file(repo: &Path, rel: &str, contents: &[u8]) {
        let path = repo.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, contents).unwrap();
        git_ok(repo, &["add", rel]);
        git_ok(repo, &["commit", "-m", "init", "--no-gpg-sign"]);
    }

    fn subject_id(repo: &Path) -> String {
        let snap = collect_snapshot(repo).unwrap_or_else(|e| panic!("{e}"));
        subject_id_hex(&snap)
    }

    #[test]
    fn clean_commit_is_stable() {
        let dir = init_repo();
        commit_file(dir.path(), "README.md", b"hello\n");
        let a = subject_id(dir.path());
        let b = subject_id(dir.path());
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn modify_tracked_file_changes_hash() {
        let dir = init_repo();
        commit_file(dir.path(), "README.md", b"hello\n");
        let clean = subject_id(dir.path());
        fs::write(dir.path().join("README.md"), b"hello world\n").unwrap();
        let dirty = subject_id(dir.path());
        assert_ne!(clean, dirty);
    }

    #[test]
    fn untracked_changes_hash_ignored_does_not() {
        let dir = init_repo();
        commit_file(dir.path(), "README.md", b"hello\n");
        let clean = subject_id(dir.path());

        fs::write(dir.path().join("untracked.txt"), b"visible\n").unwrap();
        assert_ne!(subject_id(dir.path()), clean);
        fs::remove_file(dir.path().join("untracked.txt")).unwrap();
        assert_eq!(subject_id(dir.path()), clean);

        fs::write(dir.path().join(".gitignore"), b"ignored.txt\n").unwrap();
        git_ok(dir.path(), &["add", ".gitignore"]);
        git_ok(dir.path(), &["commit", "-m", "ignore", "--no-gpg-sign"]);
        let with_gitignore = subject_id(dir.path());

        fs::write(dir.path().join("ignored.txt"), b"nope\n").unwrap();
        assert_eq!(subject_id(dir.path()), with_gitignore);

        fs::write(dir.path().join("visible.txt"), b"yes\n").unwrap();
        assert_ne!(subject_id(dir.path()), with_gitignore);
    }

    #[test]
    fn touch_mtime_does_not_change_hash() {
        let dir = init_repo();
        commit_file(dir.path(), "README.md", b"hello\n");
        let before = subject_id(dir.path());
        let status = Command::new("touch")
            .arg(dir.path().join("README.md"))
            .status()
            .expect("touch");
        assert!(status.success());
        let after = subject_id(dir.path());
        assert_eq!(before, after);
    }

    #[test]
    fn matches_python_reference_hasher() {
        let script =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/reference/subject_hash.py");
        if !script.is_file() {
            return;
        }
        let dir = init_repo();
        commit_file(dir.path(), "README.md", b"hello\n");
        git_ok(
            dir.path(),
            &[
                "remote",
                "add",
                "origin",
                "git@github.com:BoringInfraCo/Meno.git",
            ],
        );
        fs::write(dir.path().join("dirty.txt"), b"unstaged\n").unwrap();

        let rust_id = subject_id(dir.path());
        let output = Command::new("python3")
            .arg(&script)
            .arg(dir.path())
            .output()
            .expect("python3 spec/reference/subject_hash.py");
        assert!(
            output.status.success(),
            "python hasher failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let py_id = String::from_utf8_lossy(&output.stdout);
        let py_id = py_id
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .expect("python hasher printed a subject id");
        assert_eq!(rust_id, py_id);
    }
}
