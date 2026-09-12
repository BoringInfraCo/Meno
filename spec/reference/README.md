# Meno subject hasher (reference)
Independent Python 3.11 cross-check of canonical `meno-subject-v1` subject identity.
This is not a product surface; it exists only to verify the Rust hasher byte-for-byte.
Hash a snapshot JSON file (prints 64 lowercase hex to stdout):
  python3 spec/reference/subject_hash.py --snapshot snapshot.json
Hash a git worktree the same way (never uses the filesystem path as origin):
  python3 spec/reference/subject_hash.py /path/to/git/repo
Optional encoder self-check:
  python3 spec/reference/subject_hash.py --self-test
Snapshot JSON: origin, head, index[{mode,stage,sha256,path}], worktree[{status,mode,sha256,path}].
