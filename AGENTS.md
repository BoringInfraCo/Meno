# AGENTS.md — Meno

Meno is a local, harness-independent verification state layer. Existing tools
produce evidence — tests, commands, artifacts; Meno binds that evidence to the
exact Git work-tree subject hash, preserves provenance, and evaluates each
claim as `proven` / `disproven` / `unknown`. It owns state, not execution.

Meno is at **v1.0 local freeze (2026-09-12)**. Workspace / crate version is
`1.0.0`. Frozen v1 contracts live in `docs/specs/` (specs 00–10): claim/policy
text format, canonical subject identity (`meno-subject-v1`), evidence envelope
(`meno-envelope-v1`), AND-only policy grammar (`evaluation_version = 1`),
SQLite schema (`001_initial.sql`, forward-only migrations), artifact
integrity/redaction, adapter contract with conservative safety defaults, portable
bundle directory export, CLI JSON (`meno_cli_json_version = 1`), MCP
(`meno_mcp_version = 1`), and the stability surface. Changing any frozen integer
or behavior is a breaking change requiring a new spec version, not a silent
reinterpretation.

Five CLI commands, no sixth — the agent connects over MCP, not a new
subcommand:

```text
meno init
  → meno connect (adapters, agent MCP/skill setup)
  → meno verify (collect evidence, evaluate against current subject)
  → meno status (fast summary; never launches tools)
  → meno inspect (explain a claim, confirm human evidence, export bundle)
```

Default MCP authority allows reads, evidence submission, evaluation requests,
and draft claim proposals. It does **not** permit freezing claims, weakening
frozen claims, trusted policy mutation, adverse-evidence deletion, provenance
rewriting, or `human.confirmation` submission. Human confirmation is
`meno inspect --confirm`, not MCP. `status` never launches tools.
`request_evaluation` never invokes adapters. Conflict (fresh support **and**
fresh contradiction) is `unknown` with explanation, not a fourth verdict.
Exact-subject freshness: evidence bound to another subject is stale, not
deleted. `UNKNOWN` is first-class — never describe it as `PROVEN`, and never
weaken frozen claims or policies to obtain `PROVEN`.

## Mandatory reading order before any substantive change

Per `skills/build-meno/SKILL.md` (the canonical Engineering Constitution), read in this order:

1. `skills/build-meno/SKILL.md`
2. `docs/internal/PRD.md`
3. `docs/internal/ARCHITECTURE.md`
4. `docs/internal/ROADMAP.md`
5. `docs/specs/10-stability.md`, then the spec(s) governing the task
6. `skills/meno/SKILL.md` when the task touches agent behavior
7. Active task
8. Relevant code and tests under `crates/` and `spec/`

## Source-of-truth hierarchy

`PRD.md` → `ARCHITECTURE.md` → `ROADMAP.md` → `docs/specs/` (frozen v1) → Active task → Code.

- If implementation conflicts with the Canon or a frozen spec, **report the conflict — do not silently change the Canon, the spec, or the code to mask it.**
- If a spec disagrees with the PRD, the product docs win and the spec must change — not the other way around.
- Do not create new permanent documents unless explicitly requested. Canon = PRD, ARCHITECTURE, ROADMAP, specs, SKILLs.
- Update only the canonical doc whose *material* content changed; frozen specs change only through explicit versioned contract review. Otherwise leave docs untouched.

## Current baseline: v1.0 local freeze

Proven end-to-end loop: `init` → define/freeze claim (`claims/*.yaml`) →
current subject digest → evidence via adapters (git/subject, generic command,
JUnit, Playwright, human confirmation, generic envelope) → deterministic
policy evaluation → `proven` / `disproven` / `unknown` → `inspect` explains
why → source change → old evidence stale → `unknown` until fresh evidence.

- Human-readable version-controlled contract (`meno.toml`, `claims/*.yaml`,
  `policies/*.yaml`) is authoritative for claims/policies; SQLite under
  `.meno/` stores normalized runtime state, evidence, artifacts, verdicts,
  connections, and audit history.
- `spec/reference/subject_hash.py` must agree with `meno_core::subject` on
  golden snapshots and git repositories.
- Explicitly out of scope until a later phase authorizes a change:
  dependency-aware freshness, LLM verdict judges, cloud sync / hosted control
  plane, browser runtime, replacement test framework or CI, observability
  platform, autonomous fix-until-green, general agent authorization, giant
  policy DSL (OR/scores), dozens of framework-specific adapters, sixth
  top-level command, MCP authority expansion.

## Repository layout

```text
crates/
  meno-core/       domain, subjects, claims, evidence, policy, verdict, authority
  meno-store/      sqlite, migrations (001_initial.sql, forward-only), artifacts, bundles
  meno-adapters/   git, command, junit, playwright, human, generic, contract, discovery
  meno-cli/        init, connect, verify, status, inspect
  meno-mcp/        tools, permissions, stdio (no types leak into meno-core)
spec/reference/    subject_hash.py cross-implementation check
claims/            version-controlled human-readable contract (*.yaml)
docs/specs/        frozen v1 contracts (00–10)
docs/internal/     PRD, ARCHITECTURE, ROADMAP
skills/meno/       behavioral contract for agents (guidance, never source of truth)
skills/build-meno/ canonical Engineering Constitution for building Meno
```

## Conventions

- Stack: Rust workspace (`rust-version = "1.80"`, edition 2021).
- TDD: Red → Green → Refactor; smallest implementation that satisfies the
  task; adapter-specific logic stays inside the adapter; adapters return
  `Envelope`, only core evaluation produces `Verdict`.
- Frozen wire contracts (subject bytes, envelope, policy evaluation, CLI JSON
  shapes, MCP tool names/order/authority, bundle format) are never weakened to
  make code pass — version them explicitly or fix the code.
- Test suite must run **without network or live credentials**
  (fixtures/mocks; versioned real-world tool outputs, no secrets or private
  data in fixtures).
- Authority: evidence submission ≠ claim/policy mutation. Frozen claims are
  the verification contract; sensitive mutations generate audit events.
- Secrets: never persist credentials in verification state, config,
  `.mcp.json`, Skill files, fixtures, or commits. No filesystem /
  shell-history / `.env` scanning for secrets.
- Errors must say what the user can do next, distinguish `UNKNOWN` vs
  `DISPROVEN` vs run error (tool failure leaves the claim `UNKNOWN`, never
  auto-`DISPROVEN`), and preserve context without leaking secrets.
- No sixth top-level command without explicit architecture review. New
  functionality belongs in flags, JSON modes, `connect` discovery, or
  `inspect`.
- Commits only when authorized; conventional style (`feat(adapter): …`,
  `fix(subject): …`).
- When a task completes, record Implemented / Deviations / Validation /
  Learnings / Canon Changes notes in the task doc; never start the next task.
- Never request credentials, authorization headers, unredacted state
  databases, or private resource names.

## Commands

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 spec/reference/subject_hash.py --self-test
cargo run -p meno-cli -- help
```
