---
name: build-meno
description: The canonical engineering playbook for building Meno. Use for every substantive implementation, architectural review, sprint execution, refactor, documentation update, adapter integration, and engineering decision.
---

# SKILL.md

# Build Meno

> Engineering Constitution

This document defines how Meno is engineered.

It is the canonical engineering playbook for both human engineers and AI coding agents.

Its purpose is not to maximize code generation.

Its purpose is to maximize shipping velocity **without sacrificing verification trust**.

Meno should move fast because the product stays focused.

Every meaningful implementation should follow this document.

---

# Why This Document Exists

Software projects become complicated gradually.

Not because engineers intentionally overbuild them.

Because small local decisions accumulate:

- abstractions are introduced too early
- future roadmap concepts leak into current implementation
- adapter-specific logic spreads into the core
- documentation grows faster than the product
- AI agents optimize for completeness instead of scope
- implementation begins to redefine frozen semantics

Meno intentionally avoids this.

The product should remain understandable.

The architecture should remain small.

The active task should remain narrow.

The code should reflect what Meno needs **today**, not everything it may need someday.

For Meno the cost of drift is higher than usual: silent semantic change destroys trust in verdicts.

---

# The Engineering North Star

Every implementation should make Meno better at one or more of these things:

```text
Claim
Evidence
Subject
Provenance
Policy
Verdict
```

But only the capability required by the current roadmap phase should be implemented.

Whenever multiple implementation choices exist:

> Choose the smallest design that satisfies the active task while preserving Meno's frozen semantics and architectural boundaries.

---

# What Meno Is

Meno is a local, harness-independent verification state layer that turns software-agent claims into evidence-backed facts.

Meno binds evidence produced by existing tools to the exact software state it observed, preserves provenance, and deterministically evaluates each claim as `proven` / `disproven` / `unknown`.

Meno normalizes evidence from existing systems such as:

- Git (work-tree subject identity)
- generic commands (exit codes, builds, lint, typecheck)
- JUnit-compatible test results
- Playwright (browser evidence)
- human confirmation
- generic structured envelopes from custom tools

Over time it may normalize evidence from larger systems such as:

- CI providers
- OpenTelemetry
- CDP / browser observability
- databases and deployment platforms
- accessibility / performance / security scanners

These producers remain evidence sources.

They never define Meno.

---

# What Meno Is Not

Meno is not:

- a browser
- a hosted browser service
- a Playwright replacement
- a test framework
- a test generator
- a CI platform
- an observability platform
- a QA agent
- a coding agent
- an agent harness
- an autonomous fix-until-green loop
- a generic proof-of-done workflow engine
- a formal-verification system
- an LLM confidence scorer
- a mandatory workflow engine
- a replacement for product requirements or human judgment

Meno may consume output from these systems.

It may expose itself through MCP.

It may use AI models to propose claims or policies.

None of those capabilities define the product.

Meno also does not promise that a set of claims is complete. It establishes the verification state of declared claims; it cannot prove arbitrary human intent was perfectly decomposed.

---

# Product Philosophy

Meno begins with one belief:

> Agents can claim anything. Evidence determines what is true.

Meno exists to answer a stricter question than "did the tools pass":

> What has actually been established as true about this work, by what evidence, against which exact software state, and is that evidence still valid?

The product should remain valuable without AI.

Claims, subjects, evidence, provenance, policies, and verdicts should be deterministic wherever possible.

AI may propose claims and policies.

Tools observe.

Meno evaluates.

AI should never sit in the final deterministic verdict path.

---

# Engineering Philosophy

Meno should be built through short feedback loops.

Prefer shipping over predicting.

Prefer vertical slices over platform building.

Prefer real tool behavior over theoretical adapters.

Prefer explicit properties over hidden confidence scores.

Prefer boring infrastructure over unnecessary cleverness.

Prefer changing code over maintaining speculative documentation.

Prefer conservative invalidation over clever evidence reuse.

The fastest path is not the one with the most code.

The fastest path is the one with the fewest wrong assumptions about what counts as proof.

---

# The Meno Canon

Meno deliberately keeps its permanent documentation small.

The canonical product documents are:

1. `docs/internal/PRD.md`
2. `docs/internal/ARCHITECTURE.md`
3. `docs/internal/ROADMAP.md`
4. `docs/specs/` (frozen v1 contracts 00–10)
5. this `SKILL.md`
6. `skills/meno/SKILL.md` (behavioral contract for agents using Meno — guidance, never a source of truth)

These are the guiding lights.

Everything else should exist only when it earns the right to exist.

---

# Product Canon

Read first:

- `docs/internal/PRD.md`

This answers:

> What is Meno?

> Why does it exist?

> What does it deliberately not become?

> What principles must remain true as the implementation evolves?

---

# Architecture Canon

Read second:

- `docs/internal/ARCHITECTURE.md`

This answers:

> How is Meno structured?

> What belongs in Meno Core?

> What belongs in adapters?

> What does the Claim / Evidence / Subject / Policy / Verdict model own?

> How do subjects, provenance, freshness, authority, and evaluation fit together?

> What is the trusted computing base?

---

# Roadmap Canon

Read third:

- `docs/internal/ROADMAP.md`

This answers:

> What are we proving now?

> What comes later?

The Roadmap describes direction.

It is **not permission to implement future phases early**.

Future roadmap concepts must not leak into the active task unless explicitly required.

Meno is at **v1.0 local freeze (2026-09-12)**. Workspace version is `1.0.0`. Earlier v0.x labels were sequencing markers. The freeze is real: breaking semantic changes require a new spec version, not a silent reinterpretation.

---

# Spec Canon

Read fourth:

- `docs/specs/` (00 claim/policy format through 10 stability surface)
- `docs/specs/10-stability.md` first for the frozen integer and behavior table
- the specific spec governing your task (01 subject identity, 02 envelope, 03 policy grammar, 04 schema, 05 artifacts, 06 adapter contract, 07 bundle, 08 CLI JSON, 09 MCP)

Specs freeze implementation contracts so third-party adapters can build against `docs/specs/` without reading CLI internals.

If a spec disagrees with the PRD, the product docs win and the spec must change — not the other way around.

Never weaken a frozen spec to make an implementation pass.

---

# Execution Canon

Read fifth:

- Active task / sprint / issue

This answers:

> What exactly are we building now?

The active task defines today's implementation boundary.

The active task never overrides the PRD, Architecture, Roadmap, or frozen specs.

---

# Source of Truth Order

When sources disagree, use this order:

```text
PRD.md
   ↓
ARCHITECTURE.md
   ↓
ROADMAP.md
   ↓
docs/specs/ (frozen v1 contracts)
   ↓
Active task
   ↓
Current Code
```

The code reflects implementation reality.

It does not automatically redefine product direction or frozen semantics.

If code conflicts with the Canon:

report the conflict.

Do not silently change the Canon to match the implementation.

Do not silently change frozen version integers (`subject_identity_version`, policy `evaluation_version`, `meno_cli_json_version`, `meno_mcp_version`, `meno_bundle_version`, schema migration version, envelope/subject magic) to match code. Changing any frozen integer is a breaking change requiring a new version.

---

# Core Product Progression

Meno evolves in this order:

```text
SEMANTICS
   ↓
STATE
   ↓
EVIDENCE
   ↓
TRUTH
   ↓
INTEROPERABILITY
   ↓
ORCHESTRATION
   ↓
AGENTS
   ↓
TRUST PORTABILITY
```

This order is intentional. It mirrors `ROADMAP.md` v0.0 → v1.0.

Do not skip ahead. MCP and Skills deliberately come after the core primitive works. Attestation and portability come after local verification state proves valuable.

---

# The v0 Proof Loop

Every Meno slice should serve this loop:

```text
Claim
  ↓
current Subject
  ↓
existing tool/human produces Evidence
  ↓
Meno normalizes Evidence + Provenance
  ↓
Policy evaluates sufficiency
  ↓
PROVEN / DISPROVEN / UNKNOWN
  ↓
relevant software changes
  ↓
old evidence becomes STALE
  ↓
claim returns UNKNOWN unless fresh evidence establishes it
```

If a change does not strengthen this loop, question whether it belongs now.

---

# Architectural Invariants

These rules must remain true. They derive from `ARCHITECTURE.md` §28 and the v1 freeze in spec 10.

## Invariant 1 — Agent Assertion Is Not Evidence

Agent prose and confidence scores are never equivalent to verification. `UNKNOWN` must never be described as `PROVEN`. No `confidence = 0.87` semantics for infrastructure truth.

---

## Invariant 2 — Evidence Does Not Apply Beyond Its Subject Without Explicit Policy

Default freshness is `subject_match: exact`. Evidence bound to another subject is stale — historical, not applicable, never deleted to hide the fact. Branch names are never identity. Timestamps never affect identity.

---

## Invariant 3 — Stale Evidence Cannot Silently Establish Current Truth

`UNKNOWN` is first-class. Absence of proof is not proof of failure. Adapter or execution failure is a run error; the claim stays `UNKNOWN`, never auto-`DISPROVEN`.

---

## Invariant 4 — Adapters Produce Evidence, Never Verdicts

Git, command, JUnit, Playwright, human, and future adapters are thin translators. They return `Envelope`. Only core evaluation produces `Verdict`. A fake adapter in tests documents this boundary. Type-system enforcement (`meno-core` produces verdicts; adapters cannot persist them) must not be weakened.

---

## Invariant 5 — Verdicts Derive Deterministically From Claims, Policies, Evidence, and Subjects

Same inputs → same verdict. LLMs must not sit in the verdict path. Conflict (fresh support **and** fresh contradiction) is `UNKNOWN` with explanation — not a fourth verdict.

---

## Invariant 6 — Frozen Claims Cannot Be Silently Weakened by the Actor Being Evaluated

Lifecycle is `draft` → `frozen` → `retired`. Frozen claims are the verification contract. Claim proposal authority, evidence submission authority, and freeze/policy-mutation authority are distinct. The actor doing the work may collect proof without controlling the contract judging it. Sensitive mutations generate audit events.

---

## Invariant 7 — CLI, MCP, and Skills Expose One Core Model

MCP is an interface to the same core, served via `meno connect --adapter agent --stdio`. There is no `meno mcp` command. The Skill (`skills/meno/SKILL.md`) is behavioral guidance, not a source of truth. `meno-core` must not mention MCP — harness types stay in `meno-mcp`.

---

## Invariant 8 — Meno Works Without MCP or Skills

A shell-only custom agent must be able to use Meno through the CLI alone. Every MCP/Skill workflow must have a CLI path. `status` never launches tools.

---

## Invariant 9 — Adding an Integration Must Not Require a New Top-Level Command

Five commands are a ceiling, not a target:

```text
meno init
meno connect
meno verify
meno status
meno inspect
```

New functionality belongs in flags, JSON modes, `connect` discovery, or `inspect`. A sixth command requires explicit architecture review. No `meno setup-opencode`, `meno connect-playwright`, `meno agent setup`, or `meno mcp`.

---

## Invariant 10 — Meno Owns Verification State, Not Verification Execution

`meno verify` resolves the subject, loads claims/policies, finds applicable fresh evidence, optionally invokes only safe deterministic connected tools, normalizes evidence, evaluates policy, and persists/displays verdicts. The connected tool still owns execution. Meno never ships its own browser or test engine.

---

## Invariant 11 — Existing Standards Are Preferred Over Proprietary Equivalents

Consume in-toto, SLSA provenance concepts, OpenTelemetry conventions, JUnit formats, and Git identity honestly. Do not claim standard compliance the implementation does not meet. Do not force Meno semantics into standards where mappings are dishonest.

---

## Invariant 12 — Default MCP Authority Is Conservative

Default MCP authority allows reads, evidence submission, evaluation requests, and draft claim proposals. It does **not** permit freezing claims, weakening frozen claims, trusted policy mutation, adverse-evidence deletion, provenance rewriting, or `human.confirmation` submission. Human confirmation is `meno inspect --confirm`, not MCP. Fail-closed stub tools (`freeze_claim`, `retire_claim`, `delete_evidence`, `update_policy`, `set_claim_state`, `rewrite_provenance`) exist only to deny.

---

# The Anti-Speculation Rule

Before creating any new abstraction, ask:

> Does the active task require this abstraction today?

If the answer is no:

Do not create it.

Examples of premature work in Meno include:

```text
DependencyAwareFreshness
LlmVerdictJudge
CloudSync
HostedControlPlane
BrowserRuntime
TestFramework
ObservabilityBackend
FixUntilGreenLoop
GenericWorkflowEngine
PolicyDSL with OR/scores
Dozens of framework-specific adapters
CryptographicHumanIdentity
UniversalFormalVerification
```

These concepts may be correct later.

That does not make them correct now.

The deferred list in `ROADMAP.md` §14 is explicit. Do not pull items from it without evidence from real Meno use.

Policy grammar is AND-only in v1. No OR. No scores. Add fields only when a real adapter demonstrates need.

---

# Adapter Development Philosophy

Adapter contracts should evolve from real tool output.

Do not design a universal adapter framework from imagination.

If the task needs JUnit:

ingest real JUnit fixtures preserving suites, cases, pass/fail/error/skipped, timing, producer metadata, subject, report hash, and test identity.

When Playwright arrives:

test whether the envelope and policy grammar survive rich browser evidence without core changes.

When human confirmation arrives:

record statement, subject, timestamp, actor, and optional artifact with `source = human` and `reproducible = false`. Never let AI visual judgment silently become human confirmation.

Rules:

- Adapters are thin translators, not reimplementations of the tool.
- Adapter output remains untrusted until Meno Core validates it.
- Unknown adapters default to `can_collect=true`, `can_invoke=false`, `side_effect_level=consequential`, `requires_confirmation=true`.
- Generic Command may auto-run only when `side_effect_level = none` and `can_invoke = true`.
- Source-specific metadata may be retained without polluting core semantics.
- Missing metadata stays missing. Never guess provenance, subject binding, or required evidence.

---

# Vertical Slice Discipline

Meno is built through complete vertical slices.

Every task should leave the repository in a working and demonstrable state.

Prefer:

```text
One claim
↓
Current subject
↓
Real evidence (command/JUnit/Playwright/human)
↓
Deterministic evaluation
↓
status + inspect explanation
```

over:

```text
Five unfinished adapters
+
future MCP tools
+
partial policy DSL
+
placeholder attestation system
```

Depth wins over breadth.

Canonical acceptance shape (from `ROADMAP.md`):

```text
init
→ define/freeze claim
→ Subject S1
→ evidence
→ PROVEN
→ inspect why
→ source changes
→ Subject S2
→ S1 evidence becomes stale
→ UNKNOWN
→ fresh S2 evidence
→ PROVEN
```

Also prove `DISPROVEN` and conflict → `UNKNOWN`.

---

# Documentation Discipline

Permanent documentation should remain intentionally small.

Canonical docs are PRD, ARCHITECTURE, ROADMAP, `docs/specs/`, and SKILLs.

Do not create new permanent architectural documents unless explicitly requested or clearly justified by complexity.

Examples of documents that should **not** be created speculatively:

- `VERDICT_MODEL.md`
- `SUBJECT_MODEL.md`
- `ADAPTER_MODEL.md`
- `MCP_SPEC.md` (spec 09 already freezes MCP)
- `LEARNING_MODEL.md`
- `EXECUTION_MODEL.md`

If a subsystem eventually becomes complicated enough that contributors cannot understand it from:

- `ARCHITECTURE.md`
- the relevant frozen spec
- code
- tests

then a dedicated document may be warranted.

Documentation must be earned by complexity.

Frozen specs (00–10) change only through explicit versioned contract change, never as drive-by edits to make code pass.

---

# Documentation Update Rules

During implementation:

### If product direction changed materially

Update:

- `docs/internal/PRD.md`

### If architectural boundaries changed materially

Update:

- `docs/internal/ARCHITECTURE.md`

### If release sequencing changed materially

Update:

- `docs/internal/ROADMAP.md`

### If a frozen contract changed (breaking)

This requires a new spec version and explicit review. Never silently reinterpret a frozen integer or behavior. Update the affected spec plus `10-stability.md`, and record the version bump.

### If implementation details changed

Prefer:

- code
- tests
- fixtures
- inline documentation

### If nothing canonical changed

Do not touch the canonical docs.

Avoid documentation churn.

---

# Repository Reading Order

Before implementing any meaningful change:

1. Read this `SKILL.md`
2. Read `docs/internal/PRD.md`
3. Read `docs/internal/ARCHITECTURE.md`
4. Read `docs/internal/ROADMAP.md`
5. Read `docs/specs/10-stability.md`
6. Read the spec(s) governing the task (01 subject, 02 envelope, 03 policy, 04 schema, 05 artifacts, 06 adapters, 07 bundle, 08 JSON, 09 MCP)
7. Read `skills/meno/SKILL.md` when the task touches agent behavior
8. Read the Active task
9. Inspect relevant source code and tests

Key layout:

```text
crates/meno-core/      domain, subjects, claims, evidence, policy, verdict, authority
crates/meno-store/     sqlite, migrations (001_initial.sql, forward-only), artifacts, bundles
crates/meno-adapters/  git, command, junit, playwright, human, generic
crates/meno-cli/       init, connect, verify, status, inspect
crates/meno-mcp/       tools, permissions, stdio (no types leak into meno-core)
spec/reference/        subject_hash.py cross-implementation check
claims/                version-controlled human-readable contract (*.yaml)
docs/specs/            frozen v1 contracts
```

Do not let old plans override current architecture or frozen specs.

---

# Task Execution Protocol

When the user requests a substantive implementation, the coding agent MUST:

1. Read this `SKILL.md`
2. Read `docs/internal/PRD.md`
3. Read `docs/internal/ARCHITECTURE.md`
4. Read `docs/internal/ROADMAP.md`
5. Read `docs/specs/10-stability.md` plus governing specs
6. Read the Active task
7. Inspect the repository
8. Produce the Repository Understanding Report
9. Validate architecture, frozen-spec, and task scope
10. Produce an implementation plan
11. Implement using Red → Green → Refactor
12. Perform architecture review
13. Perform engineering review
14. Run focused validation
15. Run the full relevant test suite (`cargo test`, `clippy`, `fmt`, subject cross-check)
16. Perform manual verification (`init` / `verify` / `status` / `inspect`)
17. Review the complete diff
18. Update documentation only when required
19. Commit when authorized
20. Verify repository state
21. Stop after the active task

Do not continue into future roadmap phases.

---

# Phase 1 — Understand Meno

Before changing code, understand the current product boundary.

Answer:

- What verification question is this task answering?
- Which roadmap phase does it belong to?
- Which Canon documents and frozen specs govern this work?
- Which architectural invariants apply?
- Which MCP authority boundaries apply?
- What is explicitly out of scope?

If the task conflicts with the Canon or a frozen spec:

STOP.

Explain the conflict.

Do not silently choose one interpretation. Never weaken a frozen claim, policy, or spec to obtain `PROVEN` or a green build.

---

# Phase 2 — Repository Understanding

Inspect the current repository.

Produce a concise Repository Understanding Report.

Include:

## Repository Summary

- workspace and crate structure
- important modules (core / store / adapters / cli / mcp)
- current implementation maturity
- relevant tests and fixtures
- relevant commands (`cargo test --workspace`, `cargo clippy`, `cargo fmt`, `spec/reference/subject_hash.py --self-test`)

## Task Readiness

- what already exists
- what can be reused
- what must be added
- what should remain untouched (especially frozen semantics)

## Canon Alignment

- whether current implementation matches PRD and Architecture
- whether frozen specs still hold byte-for-byte (subject hashing, envelope, policy evaluation, JSON shapes, MCP tool names/order)
- any architectural drift relevant to the task

## Risks

Identify concrete implementation risks, especially:

- subject-identity divergence between Rust and `subject_hash.py`
- stale-evidence mishandling
- adapter writing verdicts or bypassing validation
- authority-boundary weakening
- secret persistence in state or artifacts

Do not invent theoretical risks unrelated to the active work.

---

# Phase 3 — Scope Validation

Confirm:

- the task belongs to the current Roadmap phase
- the task is a complete vertical slice through the proof loop
- no future phase work is being introduced
- no unnecessary adapter abstraction is being introduced
- no LLM judge is being added to the verdict path
- no execution engine (browser/test/CI) is being absorbed into Meno Core
- no sixth top-level command is being added without architecture review
- no MCP authority expansion is being smuggled in
- no frozen spec is being reinterpreted
- canonical documentation does not need speculative expansion

If the proposed implementation exceeds task scope:

reduce it.

---

# Phase 4 — Implementation Plan

Before modifying code, produce a concise implementation plan.

Include:

- files or modules to modify (`meno-core`, `meno-store`, `meno-adapters`, `meno-cli`, `meno-mcp`)
- new modules if required
- responsibilities
- public interfaces (and whether they touch frozen wire contracts)
- fixtures to add (JUnit, Playwright, git, OTel outputs — versioned, no secrets)
- tests to add (unit, invariant/property, integration, adversarial)
- migration considerations (forward-only; no silent destructive migration)
- meaningful risks

Avoid long implementation essays.

The plan should make the change understandable.

Then implement.

---

# Phase 5 — Red → Green → Refactor

Use test-driven development where practical.

## Red

Add or update tests that express the required behavior.

Confirm the required behavior is not already satisfied.

Cover the Meno invariants as applicable:

```text
same inputs → same verdict
stale evidence cannot satisfy exact-subject policy
UNKNOWN cannot become PROVEN without applicable evidence/policy change
adapter cannot directly persist verdict
frozen claim mutation requires authority
artifact hash mismatch invalidates integrity
```

## Green

Implement the smallest amount of code required to satisfy the task.

Do not build future capability.

## Refactor

Improve:

- naming
- organization
- clarity
- error handling
- duplication

without increasing scope or weakening frozen semantics.

---

# Phase 6 — Architecture Review

After implementation, review the system architecture.

Ask:

- Did adapter-specific concepts leak into Meno Core?
- Does the Claim / Evidence / Subject / Policy / Verdict model remain intact?
- Did we introduce model reasoning where deterministic evaluation was enough?
- Did we accidentally absorb execution (browser/test/CI) into Meno?
- Did we introduce future orchestration, attestation, or cloud-sync capability?
- Did we add speculative abstractions or a policy-DSL extension without adapter evidence?
- Did we weaken claim/policy authority boundaries?
- Did MCP types leak into `meno-core`?
- Did we make MCP, Skills, or a harness a core dependency?
- Did we reinterpret a frozen spec instead of versioning it?
- Did we stay within the active task?

Resolve architectural drift before declaring completion.

---

# Phase 7 — Engineering Review

Review implementation quality.

Evaluate:

- readability
- naming
- module boundaries (core / store / adapters / cli / mcp)
- error handling (diagnostic, actionable, no secret leakage)
- test quality (deterministic, no live-network dependence)
- fixture quality (versioned real-world outputs, no private data)
- dead code
- duplication
- complexity
- unnecessary dependencies
- hidden state
- performance concerns relevant to the task (indexed SQLite, content hashes, lazy artifact loading, bounded adapter work)

Prefer simplification whenever possible.

Rust gates:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

No warnings. No unformatted code.

---

# Phase 8 — Testing

Run focused validation first.

Examples:

- subject-identity tests
- freshness / staleness tests
- policy evaluation tests
- envelope validation tests
- adapter normalization tests (command, JUnit, Playwright, human)
- authority tests (frozen-claim protection, MCP denials)
- CLI tests (`verify` / `status` / `inspect`, `--json` shapes)
- MCP tests (tool names, order, argument shapes)

Then run the full relevant suite:

```bash
cargo test --workspace
python3 spec/reference/subject_hash.py --self-test
```

All required tests must pass.

`cargo test --workspace` must run without live network or provider credentials (fixtures/mocks only).

Do not hide failing tests.

Do not disable tests merely to make the build green.

Do not weaken frozen assertions to pass.

---

# Phase 9 — Manual Verification

Perform representative manual verification whenever the task produces user-visible behavior.

Examples:

```text
meno init
meno verify
meno status
meno status --json
meno inspect <claim-id>
meno inspect <claim-id> --json
meno connect
meno connect --adapter agent --stdio
```

Verify:

- expected workflow succeeds
- verdicts are explainable back to policy and evidence
- stale transitions behave (change source → old evidence stale → `UNKNOWN` until fresh evidence)
- errors are understandable and actionable
- `status` never launches tools
- existing behavior remains unchanged
- the task is demonstrable

Automated tests do not replace representative manual verification.

---

# Phase 10 — Documentation Review

Ask:

Did the product change?

Did architecture change?

Did roadmap sequencing change?

Did a frozen contract change (requiring a new spec version)?

If no:

leave canonical docs unchanged.

If yes:

update only the affected canonical document (and `10-stability.md` when a frozen integer or behavior changes).

Do not create new permanent documentation by default.

Never edit a frozen spec casually to match code. Version it or fix the code.

---

# Phase 11 — Review the Complete Diff

Review every modified file.

Remove:

- temporary debugging
- abandoned experiments
- unnecessary TODOs
- commented-out code
- unused imports
- unused abstractions
- future-facing placeholders
- dead configuration
- secrets, tokens, private paths, unredacted artifacts

The final diff should read like one coherent implementation.

Check: no credentials in `meno.toml`, `.mcp.json`, Skill files, fixtures, or commits. Secrets never belong in verification state.

---

# Phase 12 — Commit

Commit only when authorized by repository policy or the user.

Commits should represent meaningful engineering steps.

Prefer messages such as:

```text
feat(adapter): add JUnit skipped-state normalization
feat(cli): explain stale evidence on inspect
fix(subject): normalize scp-style origin before hashing
fix(policy): conflict yields unknown with explanation
```

Avoid generic messages such as:

```text
update
fix
changes
stuff
```

---

# Phase 13 — Push

Push only when:

- repository policy explicitly allows it

or

- the user explicitly requests it

Never assume pushing is desired.

---

# Phase 14 — Verification

After committing or pushing, verify:

- correct repository
- correct branch
- expected commit
- clean working tree
- tests remain passing (`cargo test --workspace`, cross-implementation subject check)
- only intended files changed
- no frozen spec was silently reinterpreted

Report exactly what was completed.

---

# Phase 15 — Task Completion

A completed task should record enough implementation context for the next task to begin safely.

Update the active task with concise completion notes if that is the repository practice.

Do not rewrite historical task intent.

Do not begin the next task.

---

# Phase 16 — Stop

Stop after the active task.

Never:

- partially implement the next roadmap phase
- create scaffolding for attestation, cloud sync, or broad adapters "while already here"
- build generic systems for hypothetical needs
- add OR/scores/LLM-judge to the policy grammar early
- introduce a sixth command speculatively
- expand MCP authority speculatively
- silently redesign Meno

One finished vertical slice through the proof loop is better than several partial ones.

---

# Definition of Done

A task is complete only when:

✓ Task requirements are satisfied.

✓ Canon and frozen specs remain intact (or were versioned explicitly with review).

✓ Scope remains narrow.

✓ Core model remains adapter-independent.

✓ No speculative future work was introduced.

✓ `cargo test --workspace` passes without network or credentials.

✓ `cargo clippy --workspace --all-targets -- -D warnings` passes.

✓ `cargo fmt --all -- --check` passes.

✓ `spec/reference/subject_hash.py --self-test` passes when subject code changed.

✓ Manual verification succeeds where applicable (`status` reads state, `verify` evaluates, `inspect` explains).

✓ Documentation is accurate; frozen specs untouched unless versioned.

✓ The full diff was reviewed; no secrets or private data leaked.

✓ Repository state is clean.

Anything less is incomplete.

---

# Engineering Decision Rules

Before implementing anything substantial, ask:

### Product

Does this strengthen Meno's ability to bind evidence to subjects, preserve provenance, evaluate policy deterministically, or expose verification state?

Is that capability part of the current roadmap phase?

### Architecture

Does this belong in:

- interface (CLI flag, JSON mode, `connect`, `inspect`)
- Meno Core (claims, subjects, evidence, policy, verdict, authority, freshness)
- adapter (thin translator for one tool format)
- persistence (SQLite, migrations, artifacts, bundles)
- MCP (interface only, never core semantics)

Is ownership clear? Does `meno-core` stay free of MCP/harness types?

### Scope

Does the active task require it?

If not:

do not build it.

### Adapters

Is this normalization based on real tool output and fixtures?

Or are we guessing about future producers?

### AI

Do we need model reasoning to propose something?

Or can deterministic evaluation decide it? (If it decides truth, it must be deterministic — no LLM judge.)

### Evidence

Do we need the raw artifact stored?

Or only content-addressed storage with metadata in SQLite, plus redaction?

### Authority

Does this let the evaluated actor weaken its own contract?

If yes: deny by default, require explicit trusted authority and audit events.

### Documentation

Did something canonical actually change?

Or can the code, tests, and fixtures remain the source of truth?

---

# Working With AI Coding Agents

AI coding agents are engineering partners.

They are not autonomous product designers or verification judges.

Agents should:

- understand before implementing
- inspect verification state before declaring work complete
- plan before modifying
- test before claiming completion
- explain architectural or frozen-spec uncertainty
- stay inside task scope
- never describe `UNKNOWN` as `PROVEN`
- never weaken frozen claims or policies to obtain `PROVEN`
- stop when the task is done

Agents should never:

- invent product requirements
- silently change architecture or frozen semantics
- implement future roadmap phases
- create speculative abstractions or policy-DSL extensions
- submit `human.confirmation` via MCP
- delete adverse evidence or rewrite provenance
- bypass authority or confirmation boundaries
- persist secrets in state, artifacts, or config
- expand documentation unnecessarily

When genuinely uncertain about a canonical or frozen-spec conflict:

report it.

Do not guess.

The Meno Skill loop governs agents using Meno:

```text
inspect Meno (status --json / get_verification_state)
→ identify UNKNOWN claims
→ gather evidence with project tools
→ submit/ingest evidence (verify --from / submit_evidence)
→ re-evaluate (verify / request_evaluation)
→ never call UNKNOWN PROVEN
→ never weaken frozen claims to pass
```

Verdicts come from Meno evaluation, never from agent prose or this document.

---

# Coding Philosophy

Prefer explicit code over clever code.

Prefer small interfaces over broad frameworks.

Prefer composition within clear crate boundaries.

Prefer predictable data flow (tool → adapter → envelope → core → verdict).

Prefer typed domain concepts where they remove ambiguity (Claim, Subject, Envelope, Policy, Verdict).

Prefer content-addressed artifacts with SQLite metadata over blobs in the database.

Avoid:

- speculative abstractions
- premature optimization
- unnecessary dependencies
- adapter-specific core logic
- hidden mutable state
- framework-building without evidence
- MCP/harness types in `meno-core`
- proprietary formats where a standard suffices

Code should remain understandable months later. Humans must be able to inspect `claims/*.yaml`, `policies/*.yaml`, and `meno.toml` and understand the verification contract without querying SQLite.

---

# Dependency Philosophy

New dependencies must justify their cost.

Before adding one, ask:

- Does the standard library already solve this adequately?
- Does the current workspace already contain an appropriate dependency?
- Is this dependency maintained and compatible with `rust-version = "1.80"`?
- Does it materially simplify the active task?
- Does it introduce unnecessary architectural commitment or expand the trusted computing base?

Do not add frameworks because they may be useful later.

Keep the trusted core (`Subject Resolver`, `Evidence Validator`, `Artifact Integrity`, `Claim/Policy Store`, `Authority Checks`, `Freshness Engine`, `Verdict Engine`, `Persistence Transactions`) small.

---

# Error Philosophy

Errors should help humans understand what failed and what to do next.

Evidence rejection should say why (schema, subject, provenance, integrity) and preserve an audit record.

Prefer:

```text
evidence rejected: JUnit report missing testcase identity (suite=auth, file=report.xml)
```

over:

```text
invalid input
```

Distinguish clearly:

```text
UNKNOWN (no applicable fresh evidence)
DISPROVEN (fresh contradiction under policy)
run error (tool could not execute — claim stays UNKNOWN, never auto-DISPROVEN)
```

But avoid exposing:

- raw credentials
- secrets and tokens
- sensitive headers or environment dumps
- unredacted HAR/log/trace content

---

# Security Philosophy

Verification state is trust-sensitive. Adapters handle secret-bearing artifacts (HAR, logs, headers, traces, screenshots).

Security is not future cleanup.

Always:

- minimize secret exposure; support sanitization/redaction
- never store credentials in verification state, config, `.mcp.json`, or Skill files
- keep credentials file handling separate from the domain store where applicable
- prefer scoped, explicit authorization for consequential verification
- require confirmation for side-effecting or ambiguous invocation
- validate external input; reject malformed envelopes with diagnostics
- distinguish evaluating evidence from authority to perform side-effecting verification
- maintain audit events for trust-sensitive transitions (claim proposed/changed/frozen/retired, policy changed, evidence ingested/rejected, subject changed, verdict evaluated, connection configured)

Convenience must not override trust. Meno must accurately represent trust limitations rather than overclaim cryptographic certainty in v1.

---

# Performance Philosophy

Do not prematurely optimize.

But do not ignore obvious boundaries: subject hashing, SQLite indexing, artifact storage, and `status` latency matter.

During v1, prioritize:

- correctness of subject identity and verdicts
- clarity of evaluation
- predictable `status` (reads state, never launches tools)
- understandable persistence (SQLite + content-addressed artifacts)
- deterministic behavior across implementations

Favor indexed SQLite, content hashes, safe incremental subject computation, lazy artifact loading, bounded adapter work, and no mandatory network round trip.

Optimize only after measurement or real user need.

---

# Open Source Philosophy

Meno should remain understandable and useful when self-hosted and offline after dependencies are installed.

Avoid architecture that requires a hosted control plane for basic functionality.

Third-party adapters must be writable against `docs/specs/` alone.

The cross-implementation check (`spec/reference/subject_hash.py` agreeing with `meno_core::subject`) is a compatibility promise — keep it green.

---

# The Meno Flywheel

The long-term system is:

```text
Claim
   ↓
current Subject
   ↓
Evidence (tool + human)
   ↓
Provenance + Subject binding
   ↓
Policy evaluation
   ↓
PROVEN / DISPROVEN / UNKNOWN
   ↓
inspect why
   ↓
software changes
   ↓
STALE → UNKNOWN
   ↓
fresh Evidence
   │
   └──────────────↺
```

Do not attempt to build the whole flywheel plus attestation, cloud sync, and autonomous planning at once.

Each roadmap phase earns the next capability.

---

# Current Build Strategy

Meno is at v1.0 local freeze. The value must exist before integrations multiply.

Default posture:

```text
Preserve frozen v1 semantics
   ↓
Vertical slices through the proof loop
   ↓
Real fixtures from real tools
   ↓
Deterministic evaluation + explainable inspect
   ↓
CLI first, MCP/Skill as interfaces
```

For any change touching subject hashing, envelope shape, policy evaluation, JSON shapes, MCP tool names/order/authority, bundle format, migration behavior, or version integers: treat it as a breaking contract change. Version it explicitly or do not ship it.

Adapter work stays evidence-driven and capability-driven, not logo-driven. Framework-specific adapters land only where they preserve meaning unavailable through generic/JUnit normalization.

---

# Success Criteria

Every task should improve one of Meno's current roadmap capabilities while preserving the trust foundation.

A good task:

- answers one clear verification question
- is demonstrable through `verify` / `status` / `inspect`
- has a narrow diff
- strengthens real verification behavior (not prose about verification)
- teaches us something about the evidence model
- avoids future work
- leaves frozen semantics untouched (or versioned explicitly)
- leaves the repository easier to understand

The goal is not to produce the largest change.

The goal is to produce the smallest trustworthy learning loop:

> This claim was established by this evidence against this exact software state. The state changed, so the old evidence no longer proves it.

---

# Final Principle

Meno should move fast because it stays small and its semantics stay frozen.

The PRD defines **why Meno exists**.

The Architecture defines **what must remain true**.

The Roadmap defines **what we prove next**.

The frozen specs define **what third parties may rely on byte-for-byte**.

The task defines **what we build today**.

The code brings that one slice to life.

Do not predict complexity.

Let real tools, real evidence, and real implementation pressure reveal it.

> **Agents can claim anything. Evidence determines what is true. Bind evidence to the exact software state it observed — and earn everything after that.**
