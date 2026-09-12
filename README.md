# Meno

Meno is a local, harness-independent **verification state layer**. Existing tools produce evidence; Meno binds that evidence to the exact software state it observed, preserves provenance, and evaluates deterministic `proven` / `disproven` / `unknown` verdicts. It owns verification **state**, not execution: it does not run your test suite, drive a browser, or replace CI.

> This claim was established by this evidence against this exact software state. The state changed, so the old evidence no longer proves it.

## Status

**v1.0 local freeze** (crate version `1.0.0`). Frozen contracts live in [`docs/specs/`](docs/specs/). Adapter authors should implement against those specs, not CLI internals.

## Commands

Five top-level commands. There is no sixth.

| Command | Role |
|---|---|
| `meno init` | Initialize Meno in a Git work tree |
| `meno connect` | Discover and configure adapters, including agents |
| `meno verify` | Collect safe evidence and evaluate claims |
| `meno status` | Fast verification summary; never launches tools |
| `meno inspect` | Explain a claim, record human confirmation, or export a bundle |

## Quick start

```bash
cargo install --path crates/meno-cli
meno init
```

Or without installing:

```bash
cargo run -p meno -- init
```

Machine-readable state:

```bash
meno status --json
meno inspect --export ./bundle
```

Agent (MCP + Skill; not a sixth command):

```bash
meno connect --adapter agent --write
meno connect --adapter agent --stdio
```

## Layout

```text
crates/meno-core        # pure domain: subject, policy, envelopes, verdicts
crates/meno-store       # SQLite + content-addressed artifacts + bundle
crates/meno-adapters    # adapter contract; git, command, junit, playwright, human
crates/meno-cli         # `meno` binary (five-command ceiling)
crates/meno-mcp         # MCP tools; served via `meno connect --adapter agent --stdio`
docs/specs/             # frozen implementation contracts
skills/meno/SKILL.md    # Skill behavioral contract
spec/reference/         # independent Python subject hasher
```

## Specs

Adapter-author index: [`docs/specs/`](docs/specs/).

## Non-goals

Meno is **not a test runner**, **not a browser**, and **does not issue LLM verdicts**. Tools observe; humans confirm; Meno evaluates.
