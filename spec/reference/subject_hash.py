#!/usr/bin/env python3
"""Independent meno-subject-v1 hasher. Cross-check only; not a product."""

import hashlib
import json
import os
import stat
import struct
import subprocess
import sys
import urllib.parse
from pathlib import Path

MAGIC = b"meno-subject-v1\n"
TAG_ORIGIN = 0x01
TAG_HEAD = 0x02
TAG_INDEX = 0x03
TAG_WORKTREE = 0x04

ST_MODIFIED = 1
ST_ADDED = 2
ST_DELETED = 3
STATUS_NAME = {"modified": ST_MODIFIED, "added": ST_ADDED, "deleted": ST_DELETED}

GITLINK = 0o160000
MODE_SYMLINK = 0o120000
MODE_EXEC = 0o100755
MODE_FILE = 0o100644
ZERO_DIGEST = b"\x00" * 32
CHUNK = 1024 * 1024

SELF_TEST = {
    "origin": "https://example.com/acme/meno",
    "head": "0123456789abcdef0123456789abcdef01234567",
    "index": [
        {
            "mode": 33188,
            "stage": 0,
            "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "path": "a.txt",
        }
    ],
    "worktree": [
        {
            "status": "modified",
            "mode": 33188,
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "path": "a.txt",
        }
    ],
}


class IndexRec:
    __slots__ = ("mode", "stage", "digest", "path", "git_sha")

    def __init__(self, mode, stage, digest, path, git_sha=""):
        self.mode = mode
        self.stage = stage
        self.digest = digest
        self.path = path
        self.git_sha = git_sha


class WorkRec:
    __slots__ = ("status", "mode", "digest", "path")

    def __init__(self, status, mode, digest, path):
        self.status = status
        self.mode = mode
        self.digest = digest
        self.path = path


def _put_u32(buf, value):
    buf.extend(struct.pack(">I", value))


def encode(origin, head, index, worktree):
    buf = bytearray(MAGIC)
    origin_b = origin.encode("utf-8")
    buf.append(TAG_ORIGIN)
    _put_u32(buf, len(origin_b))
    buf.extend(origin_b)
    head_b = head.encode("utf-8")
    buf.append(TAG_HEAD)
    _put_u32(buf, len(head_b))
    buf.extend(head_b)
    for rec in sorted(index, key=lambda r: r.path):
        if len(rec.digest) != 32:
            raise ValueError("index sha256 must be 32 bytes")
        buf.append(TAG_INDEX)
        _put_u32(buf, rec.mode)
        _put_u32(buf, rec.stage)
        buf.extend(rec.digest)
        _put_u32(buf, len(rec.path))
        buf.extend(rec.path)
    for rec in sorted(worktree, key=lambda r: r.path):
        if len(rec.digest) != 32:
            raise ValueError("worktree sha256 must be 32 bytes")
        buf.append(TAG_WORKTREE)
        buf.append(rec.status)
        _put_u32(buf, rec.mode)
        buf.extend(rec.digest)
        _put_u32(buf, len(rec.path))
        buf.extend(rec.path)
    return bytes(buf)


def subject_id(origin, head, index, worktree):
    return hashlib.sha256(encode(origin, head, index, worktree)).hexdigest()


def _digest32(hexstr):
    digest = bytes.fromhex(hexstr)
    if len(digest) != 32:
        raise ValueError("sha256 must be 64 hex chars")
    return digest


def snapshot_from_obj(obj):
    origin = obj.get("origin") or ""
    head = obj.get("head") or ""
    if not isinstance(origin, str) or not isinstance(head, str):
        raise ValueError("origin and head must be strings")
    index = []
    for item in obj.get("index") or []:
        index.append(
            IndexRec(
                int(item["mode"]),
                int(item["stage"]),
                _digest32(item["sha256"]),
                item["path"].encode("utf-8"),
            )
        )
    worktree = []
    for item in obj.get("worktree") or []:
        status = STATUS_NAME.get(item["status"])
        if status is None:
            raise ValueError(f"unknown worktree status {item['status']!r}")
        worktree.append(
            WorkRec(
                status,
                int(item["mode"]),
                _digest32(item["sha256"]),
                item["path"].encode("utf-8"),
            )
        )
    return origin, head, index, worktree


def subject_id_from_snapshot(obj):
    origin, head, index, worktree = snapshot_from_obj(obj)
    return subject_id(origin, head, index, worktree)


def load_snapshot_file(path):
    if path == "-":
        return json.load(sys.stdin)
    with open(path, "r", encoding="utf-8") as fh:
        return json.load(fh)


def normalize_origin(raw):
    url = raw.strip()
    if not url:
        return ""
    if "://" not in url:
        at = url.find("@")
        colon = url.find(":")
        if at != -1 and colon > at:
            path = url[colon + 1 :]
            url = "ssh://" + url[:colon] + "/" + path.lstrip("/")
    parts = urllib.parse.urlsplit(url)
    rebuilt = urllib.parse.urlunsplit(
        (parts.scheme.lower(), parts.netloc.lower(), parts.path, "", "")
    )
    rebuilt = rebuilt.rstrip("/")
    if rebuilt.endswith(".git"):
        rebuilt = rebuilt[: -len(".git")]
    return rebuilt


def is_excluded(path):
    return (
        path == b".meno"
        or path.startswith(b".meno/")
        or path == b".git"
        or path.startswith(b".git/")
    )


def git_c(repo, args, check=True):
    proc = subprocess.run(
        ["git", "-C", repo, *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if check and proc.returncode != 0:
        err = proc.stderr.decode("utf-8", "replace").strip() or f"exit {proc.returncode}"
        raise RuntimeError(f"git {' '.join(args)} failed: {err}")
    return proc


def sha256_stream(fh):
    h = hashlib.sha256()
    while True:
        chunk = fh.read(CHUNK)
        if not chunk:
            break
        h.update(chunk)
    return h.digest()


def sha256_file(path_b):
    with open(path_b, "rb") as fh:
        return sha256_stream(fh)


def sha256_git_blob(repo, git_sha):
    proc = subprocess.Popen(
        ["git", "-C", repo, "cat-file", "blob", git_sha],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    try:
        digest = sha256_stream(proc.stdout)
        stderr = proc.stderr.read()
        rc = proc.wait()
    finally:
        if proc.stdout:
            proc.stdout.close()
        if proc.stderr:
            proc.stderr.close()
    if rc != 0:
        err = stderr.decode("utf-8", "replace").strip() or f"exit {rc}"
        raise RuntimeError(f"git cat-file blob {git_sha} failed: {err}")
    return digest


def disk_mode_and_hash(full):
    st = os.lstat(full)
    if stat.S_ISLNK(st.st_mode):
        target = os.readlink(full)
        if isinstance(target, str):
            target = os.fsencode(target)
        return MODE_SYMLINK, hashlib.sha256(target).digest()
    if not stat.S_ISREG(st.st_mode):
        raise IsADirectoryError(full)
    mode = MODE_EXEC if (st.st_mode & stat.S_IXUSR) else MODE_FILE
    return mode, sha256_file(full)


def submodule_head(full):
    proc = git_c(os.fsdecode(full), ["rev-parse", "HEAD"], check=False)
    if proc.returncode != 0:
        return ""
    return proc.stdout.decode("ascii", "replace").strip().lower()


def collect_index(repo):
    raw = git_c(repo, ["ls-files", "-s", "-z"]).stdout
    recs = []
    for rec in raw.split(b"\0"):
        if not rec:
            continue
        meta, path = rec.split(b"\t", 1)
        mode_s, git_sha_b, stage_s = meta.split(b" ")
        mode = int(mode_s, 8)
        git_sha = git_sha_b.decode("ascii").lower()
        stage = int(stage_s, 10)
        if mode == GITLINK:
            digest = hashlib.sha256(git_sha.encode("ascii")).digest()
        else:
            digest = sha256_git_blob(repo, git_sha)
        recs.append(IndexRec(mode, stage, digest, path, git_sha))
    return recs


def collect_worktree(repo, index):
    by_path = {}
    for rec in index:
        prev = by_path.get(rec.path)
        if prev is None or rec.stage < prev.stage:
            by_path[rec.path] = rec
    overlay = []
    for path, rec in by_path.items():
        if is_excluded(path):
            continue
        full = os.path.join(os.fsencode(repo), path)
        if not os.path.lexists(full):
            overlay.append(WorkRec(ST_DELETED, 0, ZERO_DIGEST, path))
            continue
        if rec.mode == GITLINK:
            head = submodule_head(full)
            if head != rec.git_sha:
                overlay.append(
                    WorkRec(
                        ST_MODIFIED,
                        GITLINK,
                        hashlib.sha256(head.encode("ascii")).digest(),
                        path,
                    )
                )
            continue
        try:
            mode, digest = disk_mode_and_hash(full)
        except (FileNotFoundError, IsADirectoryError):
            overlay.append(WorkRec(ST_DELETED, 0, ZERO_DIGEST, path))
            continue
        if mode != rec.mode or digest != rec.digest:
            overlay.append(WorkRec(ST_MODIFIED, mode, digest, path))
    others = git_c(repo, ["ls-files", "-z", "--others", "--exclude-standard"]).stdout
    for path in others.split(b"\0"):
        if not path or is_excluded(path):
            continue
        full = os.path.join(os.fsencode(repo), path)
        try:
            mode, digest = disk_mode_and_hash(full)
        except (FileNotFoundError, IsADirectoryError, OSError):
            continue
        overlay.append(WorkRec(ST_ADDED, mode, digest, path))
    return overlay


def collect_repo(repo):
    origin_proc = git_c(repo, ["config", "--get", "remote.origin.url"], check=False)
    if origin_proc.returncode != 0:
        origin = ""
    else:
        origin = normalize_origin(origin_proc.stdout.decode("utf-8", "replace"))
    head_proc = git_c(repo, ["rev-parse", "HEAD"], check=False)
    if head_proc.returncode != 0:
        head = ""
    else:
        head = head_proc.stdout.decode("ascii", "replace").strip().lower()
    index = collect_index(repo)
    worktree = collect_worktree(repo, index)
    return origin, head, index, worktree


def resolve_repo(path):
    repo = str(Path(path).resolve())
    proc = git_c(repo, ["rev-parse", "--show-toplevel"], check=False)
    if proc.returncode != 0:
        raise RuntimeError(f"not a git repository: {path}")
    return os.fsdecode(proc.stdout.strip())


def subject_id_from_repo(path):
    origin, head, index, worktree = collect_repo(resolve_repo(path))
    return subject_id(origin, head, index, worktree)


def usage():
    sys.stderr.write(
        "usage: subject_hash.py --snapshot FILE | --self-test | REPO\n"
    )
    return 2


def main(argv):
    if len(argv) == 2 and argv[1] == "--self-test":
        sys.stdout.write(subject_id_from_snapshot(SELF_TEST) + "\n")
        return 0
    if len(argv) == 3 and argv[1] == "--snapshot":
        sys.stdout.write(subject_id_from_snapshot(load_snapshot_file(argv[2])) + "\n")
        return 0
    if len(argv) == 2 and not argv[1].startswith("-"):
        sys.stdout.write(subject_id_from_repo(argv[1]) + "\n")
        return 0
    return usage()


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv))
    except (OSError, ValueError, RuntimeError, json.JSONDecodeError, KeyError, TypeError) as exc:
        sys.stderr.write(f"error: {exc}\n")
        raise SystemExit(1)
