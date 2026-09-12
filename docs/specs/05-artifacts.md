# 05 — Artifact integrity and redaction

**Version:** 1  
**Status:** Frozen for v1

## Storage

```text
.meno/artifacts/<sha256>
```

`sha256` is lowercase hex of SHA-256(file bytes). Put is content-addressed: identical bytes collapse. Metadata (MIME, size, digest) lives in SQLite `artifacts`.

Relative path recorded as `artifacts/<sha256>`.

## Integrity

On read, re-hash and compare. Mismatch → integrity failure; the referencing evidence cannot support a verdict.

## Redaction

Adapters MUST run redaction before `put`.

**Redact in place** (replace with `***REDACTED***`):

- `Authorization:` request headers
- AWS access keys `AKIA[0-9A-Z]{16}`
- GitHub PATs `ghp_[A-Za-z0-9]{20,}`
- `password=` / `secret=` query-like assignments
- `postgres://` / `mysql://` / `mongodb://` URLs with userinfo

**Reject** (do not store):

- PEM private keys (`-----BEGIN … PRIVATE KEY-----`)
- Connection-config JSON/TOML that still contains credential-shaped values after redaction

v0 does not provide cryptographic producer identity. Hashing is integrity, not attestation.
