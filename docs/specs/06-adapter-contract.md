# 06 — Adapter contract

**Version:** 1  
**Status:** Frozen for v1

Adapters are thin translators from existing tool output into the evidence envelope. They never write verdicts. Core validates envelopes.

## Trait (conceptual)

```text
name() -> string
safety() -> AdapterSafety
detect(context) -> DetectResult
normalize(input) -> Envelope
validate(envelope) -> Result
```

`configure` and `collect`/`invoke` are v0.4. v0.0 freezes the types so later adapters cannot invent safety defaults.

## Safety metadata

```text
can_collect: bool
can_invoke: bool
side_effect_level: none | filesystem | network | consequential
requires_confirmation: bool
```

Unknown or unspecified adapters default to:

```text
can_collect = true
can_invoke = false
side_effect_level = consequential
requires_confirmation = true
```

v0.1 Generic Command may auto-run only when `side_effect_level = none` and `can_invoke = true`.

## Type-system invariant

Adapters return `Envelope`. Only core evaluation produces `Verdict`. A fake adapter in tests documents that adapters cannot persist verdicts.
