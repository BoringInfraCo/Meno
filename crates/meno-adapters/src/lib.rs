//! Adapter contract and source collectors.
//!
//! Adapters translate tool output into evidence envelopes. They never write
//! verdicts. `configure` / `collect` / `invoke` execution is v0.4.

mod command;
mod contract;
mod discover;
mod fake;
mod generic;
mod git;
mod human;
mod junit;
mod playwright;

pub use command::{
    normalize_command, redact_output, run_command, CommandAdapter, CommandOutcome, CommandSpec,
};
pub use contract::{
    Adapter, AdapterError, AdapterSafety, DetectContext, DetectResult, InputBundle, SideEffectLevel,
};
pub use discover::{discover_integrations, Discovered};
pub use fake::FakeAdapter;
pub use generic::ingest_generic_envelope;
pub use git::collect_snapshot;
pub use human::{normalize_human_confirmation, normalize_inferred_visual, HumanConfirmation};
pub use junit::{
    normalize_junit, parse_junit_xml, JunitAdapter, JunitCase, JunitReport, JunitStatus, JunitSuite,
};
pub use playwright::{
    looks_like_playwright_json, normalize_playwright, parse_playwright_json, PlaywrightAdapter,
    PlaywrightAttachment, PlaywrightReport, PlaywrightTest,
};
