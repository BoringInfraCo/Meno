# Meno
**Bind evidence to the exact software state it observed.**

Meno is a local, harness-independent verification state layer. Existing tools produce evidence; Meno binds that evidence to a subject hash, preserves provenance, and evaluates `proven` / `disproven` / `unknown`. It owns state, not execution.

- All commands accept `--json` for humans and agents alike.

[Specs](docs/specs/) · [Skill](skills/meno/SKILL.md) · [Examples](examples/)

---
## Quick start

```bash
curl -fsSL https://boringinfra.company/meno/install.sh | sh
meno init
meno verify
meno status
```

```text
claim  C-subject-identity  proven    subject 9f3a…c1
claim  C17                 unknown   no evidence for current subject
```

---
## Commands

Five commands. No sixth.

| Command | Description |
|---|---|
| `meno init` | Initialize Meno in a Git work tree |
| `meno connect` | Discover and configure adapters |
| `meno verify` | Collect evidence and evaluate claims |
| `meno status` | Fast summary; never launches tools |
| `meno inspect` | Explain a claim or export a bundle |

Agent over MCP (not a sixth command):

```bash
meno connect --adapter agent --write
meno connect --adapter agent --stdio
```

---
## How it works

1. Adapters collect evidence (`git`, `command`, `junit`, `playwright`, `human`).
2. Meno binds evidence to the current subject hash.
3. Claims evaluate to `proven` / `disproven` / `unknown`.
4. `status` summarizes, `inspect` explains.

---
## Status

**v1.0 local freeze.** Frozen contracts live in [`docs/specs/`](docs/specs/). Adapter authors should build against those specs, not CLI internals.

---
## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## License

Apache-2.0
