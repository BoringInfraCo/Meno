# 01 — Canonical Subject identity

**Version:** 1 (`meno-subject-v1`)  
**Status:** Frozen for v1  
**Identity version field:** `subject_identity_version = 1`

A Subject identifies the exact software state evidence applies to. Branch names are never identity. Timestamps never affect identity.

```text
subject_id = lowercase hex(SHA-256(canonical_encoding))
```

Two implementations MUST produce the same `subject_id` from identical repository bytes and git metadata. The Rust hasher in `meno-core` and `spec/reference/subject_hash.py` are the v0 pair.

## Inputs

```text
repository origin (normalized, or empty)
+ HEAD commit hex (or empty)
+ full index (staged) entries
+ worktree overlay (dirty tracked + non-ignored untracked + deletions)
```

Clean tracked files appear in the index section only. They are not repeated in the worktree overlay.

## Origin

Prefer `remote.origin.url`. Normalize:

1. Strip surrounding whitespace.
2. SCP-style `git@host:path` becomes `ssh://git@host/path`.
3. Lowercase scheme and netloc.
4. Strip a trailing `/`.
5. Strip a trailing `.git`.
6. Drop query and fragment.

If origin is missing, the origin field is **empty**. Do not fall back to a filesystem path (that would make clones without remotes disagree and leak local paths).

## HEAD

`git rev-parse HEAD` as lowercase hex. Empty repository / missing HEAD → empty field.

## Index

Every `git ls-files -s` entry:

- `mode`: Git mode integer (`0o100644`, `0o100755`, `0o120000`, `0o160000`)
- `stage`: 0 normally; conflict stages 1–3 are included and change identity
- `content_sha256`: SHA-256 of the **blob bytes** (`git cat-file blob <git-sha>`), not Git SHA-1
- `path`: worktree-relative, `/` separators, no leading `./`

Gitlink (`160000`): hash SHA-256 of the 40-character lowercase commit hex ASCII (the gitlink target), not the submodule tree.

Sort index records by **raw path bytes** (memcmp), not locale collation.

## Worktree overlay

Compare the worktree to the index. Record only differences:

| status | code | when |
|---|---|---|
| modified | 1 | tracked path exists; content or git-mode differs from index |
| added | 2 | non-ignored untracked file (`git ls-files --others --exclude-standard`) |
| deleted | 3 | index path missing on disk |

Mode on disk (non-gitlink):

- symlink → `0o120000`; hash **target bytes**, not the referent
- owner-executable regular file → `0o100755`
- other regular file → `0o100644`

Deleted: `mode = 0`, `content_sha256 = 32 zero bytes`.

Always exclude `.git/` and `.meno/` even if not gitignored. Honor `.gitignore`, `.git/info/exclude`, and exclude magic via Git itself.

Empty directories do not exist. Non-UTF8 paths are included as raw bytes. Large files are hashed in full (streaming). Line endings are on-disk bytes; `core.autocrlf` can change subjects — this is documented, not compensated.

Submodules: do not recurse. If the index records a gitlink and the checked-out submodule HEAD differs, emit `modified` with SHA-256 of the new 40-hex ASCII.

Sort worktree records by raw path bytes.

## Canonical encoding

Binary, big-endian, no JSON.

```text
MAGIC = b"meno-subject-v1\n"

records in order:
  ORIGIN (once)
  HEAD   (once)
  INDEX  (zero or more, path-sorted)
  WORKTREE (zero or more, path-sorted)

tag u8:
  0x01 ORIGIN
  0x02 HEAD
  0x03 INDEX
  0x04 WORKTREE

ORIGIN:  u32be len | bytes
HEAD:    u32be len | bytes
INDEX:   u32be mode | u32be stage | 32-byte sha256 | u32be path_len | path
WORKTREE: u8 status | u32be mode | 32-byte sha256 | u32be path_len | path
```

`subject_id` is SHA-256 of this byte string, lowercase hex (64 chars).

## Non-goals

- Dependency-aware reuse
- Environment identity (OS, CPU) in v0 subject
- `captured_at` as part of identity
