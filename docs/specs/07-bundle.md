# 07 — Portable bundle (directory export)

**Version:** 1  
**Status:** Frozen for v1

Gate E: no in-toto, no SLSA, no signatures. A bundle is a directory, not a zip. It is not a SQLite file and MUST NOT contain connections or secrets.

## Layout

```text
<path>/
  meno-bundle.json
  artifacts/<sha256>
```

`artifacts/<sha256>` is the raw bytes of a content-addressed artifact when available. Missing files are allowed; importers skip those artifacts (and any envelope that needs them and cannot find them in the destination store).

Export creates `<path>/`. If the destination exists and is not empty, export fails — it does not clobber.

## `meno-bundle.json`

```json
{
  "meno_bundle_version": 1,
  "exported_at": "<RFC3339>",
  "subject_identity_version": 1,
  "evaluation_version": 1,
  "subjects": [
    { "id": "<64hex>", "identity_version": 1, "origin": null, "head": null }
  ],
  "claims": [],
  "policies": [],
  "envelopes": [],
  "verdicts": [
    {
      "claim_id": "",
      "subject_id": "",
      "verdict": "proven|disproven|unknown",
      "evaluation_version": 1,
      "subject_identity_version": 1,
      "explanation_json": "{}"
    }
  ],
  "artifacts": [
    { "sha256": "", "media_type": "application/octet-stream", "size": 0 }
  ]
}
```

- `claims` are `ClaimDocument` serde.
- `policies` are `PolicyDocument` serde.
- `envelopes` are `Envelope` serde, including `trust` and `integrity`. Trust is sealed into the envelope digest; importers MUST NOT rewrite `origin` or `basis`.
- `verdicts` are latest-per-`(claim_id, subject_id)` on export; import inserts history.
- `explanation_json` is a JSON string (the store column), not a nested object.

Not included: `connections`, secrets, the SQLite file, `claim_evidence`, `slsa`, `in-toto`.

## Selection

- No `claim_id`: all claims, policies, subjects, evidence, latest verdicts, and artifact metadata.
- With `claim_id`: that claim, its policy, envelopes linked via `claim_evidence` (if any), those envelopes' subjects and artifacts, and latest verdicts for that claim. If the claim has no `claim_evidence` rows, still export the claim and its policy.

## Import

Additive. Never overwrite existing claim rows or frozen statements.

| Item | Rule |
|---|---|
| subjects | `INSERT OR IGNORE` (`upsert_subject`) |
| artifacts | `put` bytes when the file is present; skip missing (count `skipped`) |
| envelopes | `verify_envelope`; skip if `id` already exists; do not modify trust fields |
| envelopes with missing artifacts | skip that envelope (`skipped`); do not fail the whole import |
| verdicts | insert history; skip when the latest row is the same claim+subject+explanation |
| claims / policies | insert if `id` is missing; do not update existing rows |

Artifact bytes are stored before evidence (insert_evidence requires artifact rows). Tampering `trust` without resealing fails integrity verify and MUST NOT be accepted as a trust upgrade.
