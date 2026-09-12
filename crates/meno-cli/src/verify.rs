use std::path::PathBuf;

use crate::error::CliError;
use crate::project::{print_claim_list, print_json, short_subject_id, ProjectContext};

pub fn run(json: bool, from: Vec<PathBuf>, confirm_invoke: bool) -> Result<(), CliError> {
    let ctx = ProjectContext::open()?;
    ctx.invoke_commands(confirm_invoke)?;
    ctx.ingest_from_paths(&from)?;
    ctx.ingest_configured_junit()?;
    ctx.ingest_configured_playwright()?;
    let claims = ctx.evaluate_claims()?;
    if json {
        print_json(&ctx.subject_id, &claims)?;
    } else {
        println!("subject {}", short_subject_id(&ctx.subject_id));
        println!();
        print_claim_list(&claims);
    }
    Ok(())
}
