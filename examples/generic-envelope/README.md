# Generic evidence envelope

Third-party tools can emit a frozen v1 envelope and ingest it with `meno verify --from`. No custom adapter is required.

`evidence.json` is kind `generic.envelope`, trust `machine` / `observed`, and an observation with `{ "ok": true }`. `subject_id` is 64 hex zeros so ingest binds the envelope to the current subject.

## Run

In a Git repository:

```sh
meno init
meno verify --from evidence.json
meno inspect
```

Copy this directory's `evidence.json` into the repo first, or pass its path:

```sh
meno verify --from path/to/examples/generic-envelope/evidence.json
```

To require this envelope in a claim, match `kind: generic.envelope` and `ok: true`.
