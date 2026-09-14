# Meno

**Bind evidence to the exact software state it observed.**

Meno is a local, harness-independent verification state layer. Existing tools produce evidence — tests, commands, artifacts; Meno binds that evidence to a subject hash, preserves provenance, and evaluates each claim as `proven` / `disproven` / `unknown`. It owns state, not execution.

- **Bind evidence** — every record is tied to the current Git work-tree subject hash
- **Evaluate claims** — policy files declare requires / contradicted-by / freshness per claim
- **Inspect fast** — `status` summarizes without launching tools; `inspect` explains why
- **Stay local** — SQLite store, no network calls; bundles export state only when you choose to share

All commands accept `--json` for machine-readable output, built for humans and agents alike.

---

## Quick start

```bash
curl -fsSL https://boringinfra.company/meno/install.sh | sh
meno init
meno verify
meno status
```

`status` never launches tools; `inspect` explains a single claim or exports a portable bundle. See [docs/specs/](docs/specs/) for the frozen v1 contracts.

---

## Commands

| Command | Description |
| --- | --- |
| `meno init` | Initialize Meno in a Git work tree |
| `meno connect` | Discover and configure adapters (`--adapter agent --stdio` for MCP) |
| `meno verify` | Collect evidence and evaluate claims against the current subject |
| `meno status` | Fast summary; never launches tools |
| `meno inspect` | Explain a claim or export a portable bundle |

Five commands. No sixth — the agent connects over MCP, not a new subcommand.

---

## Installation

Requires Rust **1.80 or newer** (`rust-version` in `Cargo.toml`).

```bash
curl -fsSL https://boringinfra.company/meno/install.sh | sh
```

Or build from source:

```bash
cargo build --release
cargo test --workspace
```

---

## Adapters

| Adapter | Collects | Status |
| --- | --- | --- |
| git | Work-tree subject hash | ✅ Supported |
| command | Exit codes bound to the subject | ✅ Supported |
| junit | Test-suite results (XML) | ✅ Supported |
| playwright | Browser evidence | ✅ Supported |
| human | Signed attestations | ✅ Supported |
| agent | MCP stdio bridge | ✅ Supported |

Run `meno connect` to discover and configure adapters. Adapter authors build against [`docs/specs/`](docs/specs/), not CLI internals.

---

## Limitations

- **State, not execution.** Meno never runs your tools on `status`; `verify` only collects via configured adapters.
- **Subject-bound.** Evidence counts only for the subject hash it observed; move the tree, re-verify.
- **Three verdicts.** Claims evaluate to `proven` / `disproven` / `unknown` — unknown means no evidence, not failure.
- **Local first.** SQLite store under `.meno/`; sharing happens only through explicit bundle export.
- **Frozen v1.** Adapter, envelope, and JSON contracts are frozen; breaking changes require a new spec version.

---

## Docs

- [docs/specs/](docs/specs/) — frozen v1 contracts (claims, subjects, envelopes, schema, MCP)
- [skills/meno/SKILL.md](skills/meno/SKILL.md) — behavioral contract for agents
- [examples/](examples/) — generic envelope examples
- [claims/](claims/) — policy files evaluated against the current subject

---

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## License

Apache-2.0 — see [LICENSE](LICENSE).
