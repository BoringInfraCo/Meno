use std::collections::HashMap;
use std::path::PathBuf;

use meno_core::policy::Requirement;
use meno_core::{Envelope, Trust, TrustBasis, TrustOrigin, TrustRelation};
use meno_store::BundleReport;
use serde_json::Value;

use crate::error::CliError;
use crate::project::{
    compact_observation, print_claim_list, print_inspect_json, print_json, short_subject_id,
    why_verdict, EvaluatedClaim, JsonObservationSummary, ProjectContext, COMPACT_OBS_KEYS,
};

pub struct InspectArgs {
    pub claim_id: Option<String>,
    pub json: bool,
    pub confirm: bool,
    pub statement: Option<String>,
    pub actor: Option<String>,
    pub artifact: Option<PathBuf>,
    pub export: Option<PathBuf>,
    pub import: Option<PathBuf>,
}

pub fn run(args: InspectArgs) -> Result<(), CliError> {
    if args.export.is_some() && args.import.is_some() {
        return Err(CliError::msg("use either --export or --import, not both"));
    }
    if args.export.is_some() && args.confirm {
        return Err(CliError::msg("cannot combine --confirm with --export"));
    }

    let ctx = ProjectContext::open()?;

    if let Some(path) = args.import {
        let report = ctx.store.import_bundle(&path)?;
        print_bundle_report(args.json, "imported", &report)?;
        return Ok(());
    }

    if let Some(path) = args.export {
        let report = ctx.store.export_bundle(&path, args.claim_id.as_deref())?;
        print_bundle_report(args.json, "exported", &report)?;
        return Ok(());
    }

    if args.confirm {
        let id = args
            .claim_id
            .as_deref()
            .ok_or_else(|| CliError::msg("claim id is required with --confirm"))?;
        let statement = args
            .statement
            .as_deref()
            .ok_or_else(|| CliError::msg("--statement is required with --confirm"))?;
        if statement.trim().is_empty() {
            return Err(CliError::msg("--statement is required with --confirm"));
        }
        let records = ctx.store.load_claims()?;
        if !records.iter().any(|record| record.document.id == id) {
            return Err(CliError::msg(format!("unknown claim {id}")));
        }
        ctx.insert_human_confirmation(statement, args.actor.as_deref(), args.artifact.as_deref())?;
    }
    let claims = ctx.evaluate_claims()?;
    match args.claim_id {
        None => {
            if args.json {
                print_json(&ctx.subject_id, &claims)?;
            } else {
                println!("subject {}", short_subject_id(&ctx.subject_id));
                println!();
                print_claim_list(&claims);
            }
        }
        Some(id) => {
            let claim = claims
                .iter()
                .find(|c| c.document.id == id)
                .ok_or_else(|| CliError::msg(format!("unknown claim {id}")))?;
            let envelopes = ctx.store.load_evidence()?;
            if args.json {
                print_inspect_json(&ctx.subject_id, claim, &envelopes)?;
            } else {
                print_detail(&ctx, claim, &envelopes)?;
            }
        }
    }
    Ok(())
}

fn print_bundle_report(json: bool, verb: &str, report: &BundleReport) -> Result<(), CliError> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!(
            "{verb} {}, {}, {}, {}",
            count_label(report.claims, "claim", "claims"),
            count_label(report.envelopes, "envelope", "envelopes"),
            count_label(report.artifacts, "artifact", "artifacts"),
            count_label(report.skipped, "skipped", "skipped"),
        );
    }
    Ok(())
}

fn count_label(n: usize, singular: &str, plural: &str) -> String {
    if n == 1 {
        format!("1 {singular}")
    } else {
        format!("{n} {plural}")
    }
}

fn print_detail(
    ctx: &ProjectContext,
    claim: &EvaluatedClaim,
    envelopes: &[Envelope],
) -> Result<(), CliError> {
    let eval = &claim.evaluation;
    let mut by_id = HashMap::new();
    for env in envelopes {
        by_id.insert(env.id.as_str(), env);
    }
    println!(
        "{} — {}",
        claim.document.id,
        crate::project::verdict_label(eval.verdict)
    );
    println!();
    println!("Claim");
    println!("  {}", claim.document.statement);
    println!("  state: {}", state_slug(claim.document.state));
    println!();
    println!("Subject");
    println!("  {}", ctx.subject_id);
    println!();
    println!("Policy requires");
    if claim.policy.requires.is_empty() {
        println!("  (none)");
    } else {
        for req in &claim.policy.requires {
            println!("  - {}", format_requirement(req));
        }
    }
    if !claim.policy.contradicted_by.is_empty() {
        println!("Contradicted by");
        for req in &claim.policy.contradicted_by {
            println!("  - {}", format_requirement(req));
        }
    }
    println!();
    println!("Evidence");
    println!("  supporting:");
    print_evidence_group(&eval.supporting, &by_id);
    println!("  contradicting:");
    print_evidence_group(&eval.contradicting, &by_id);
    println!("  stale:");
    print_evidence_group(&eval.stale, &by_id);
    println!("  missing:");
    if eval.missing.is_empty() {
        println!("    (none)");
    } else {
        for item in &eval.missing {
            println!("    {}: {}", item.kind, item.detail);
        }
    }
    println!();
    println!("Verdict");
    println!(
        "  {} — {}",
        crate::project::verdict_label(eval.verdict),
        why_verdict(eval)
    );
    Ok(())
}

fn print_evidence_group(ids: &[String], by_id: &HashMap<&str, &Envelope>) {
    if ids.is_empty() {
        println!("    (none)");
        return;
    }
    for id in ids {
        println!("    {id}");
        if let Some(env) = by_id.get(id.as_str()) {
            print_evidence_detail(env);
        }
    }
}

fn print_evidence_detail(env: &Envelope) {
    println!("      kind: {}", env.kind);
    println!("      source: {}", env.source.name);
    println!("      producer: {}", env.provenance.producer);
    println!("      trust: {}", format_trust(env.trust));
    println!("      captured_at: {}", env.captured_at);
    println!("      artifacts:");
    if env.artifact_refs.is_empty() {
        println!("        (none)");
    } else {
        for artifact in &env.artifact_refs {
            println!("        {}", artifact.sha256);
        }
    }
    println!("      observations:");
    if env.observations.is_empty() {
        println!("        (none)");
    } else {
        for obs in &env.observations {
            println!(
                "        - {}",
                format_observation(&compact_observation(obs))
            );
        }
    }
}

fn format_observation(obs: &JsonObservationSummary) -> String {
    let mut out = obs.type_name.clone();
    let fields = format_compact_fields(&obs.fields);
    if !fields.is_empty() {
        out.push(' ');
        out.push_str(&fields);
    }
    out
}

fn format_compact_fields(fields: &serde_json::Map<String, Value>) -> String {
    let mut bits = Vec::new();
    for key in COMPACT_OBS_KEYS {
        if let Some(value) = fields.get(*key) {
            bits.push(format!("{key}={}", format_compact_value(value)));
        }
    }
    bits.join(" ")
}

fn format_compact_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        other => other.to_string(),
    }
}

fn format_trust(trust: Trust) -> String {
    format!(
        "origin={} reproducible={} basis={} relation={}",
        match trust.origin {
            TrustOrigin::Machine => "machine",
            TrustOrigin::Human => "human",
        },
        trust.reproducible,
        match trust.basis {
            TrustBasis::Observed => "observed",
            TrustBasis::Inferred => "inferred",
        },
        match trust.relation {
            TrustRelation::Direct => "direct",
            TrustRelation::Indirect => "indirect",
        }
    )
}

fn format_requirement(req: &Requirement) -> String {
    let mut out = format!("kind={}", req.kind);
    if !req.match_fields.is_empty() {
        match serde_json::to_string(&req.match_fields) {
            Ok(m) => out.push_str(&format!(" match={m}")),
            Err(_) => out.push_str(&format!(" match={:?}", req.match_fields)),
        }
    }
    out.push_str(&format!(
        " min_count={} subject_bound={}",
        req.min_count, req.subject_bound
    ));
    out
}

fn state_slug(state: meno_core::ClaimState) -> &'static str {
    match state {
        meno_core::ClaimState::Draft => "draft",
        meno_core::ClaimState::Frozen => "frozen",
        meno_core::ClaimState::Retired => "retired",
    }
}
