use regex::Regex;
use std::sync::OnceLock;

static ID_RE: OnceLock<Regex> = OnceLock::new();

/// Claim, policy, and similar stable identifiers.
pub fn is_stable_id(id: &str) -> bool {
    let re =
        ID_RE.get_or_init(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$").expect("id regex"));
    re.is_match(id)
}

pub fn require_stable_id(id: &str, what: &str) -> Result<(), String> {
    if is_stable_id(id) {
        Ok(())
    } else {
        Err(format!("{what} id {id:?} is not a stable id"))
    }
}
