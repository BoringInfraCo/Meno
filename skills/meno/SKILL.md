---
name: meno
description: Verification state — inspect Meno before declaring work complete
---

# Meno

Behavioral guidance, not a source of truth. Verdicts come from Meno evaluation, never from this document or from agent prose.

Meno is a verification state layer, not a test runner. Project tools produce evidence; Meno binds it to the current subject and evaluates it.

## Rules

1. Before declaring relevant work complete, inspect Meno (`meno status --json` or MCP `get_verification_state`).
2. Identify UNKNOWN claims.
3. Gather evidence with **project** tools (tests, Playwright, commands). Do not invent verdicts.
4. Submit/ingest evidence (`meno verify --from`, or MCP `submit_evidence`) then re-evaluate (`meno verify` or MCP `request_evaluation`).
5. Never describe UNKNOWN as PROVEN.
6. Never weaken, rewrite, freeze-bypass, or delete frozen claims/policies to obtain PROVEN.
7. Human confirmation is `meno inspect --confirm`, not MCP.
8. CLI works without MCP; MCP is one interface to the same core.
