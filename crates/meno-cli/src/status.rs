use crate::error::CliError;
use crate::project::{print_claim_list, print_json, short_subject_id, ProjectContext};

pub fn run(json: bool) -> Result<(), CliError> {
    let ctx = ProjectContext::open()?;
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
