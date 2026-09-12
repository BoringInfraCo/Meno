use crate::error::{MenoError, Result};
use crate::ids::require_stable_id;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const POLICY_GRAMMAR_VERSION: u32 = 1;
pub const EVALUATION_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyDocument {
    pub id: String,
    #[serde(default = "one")]
    pub version: u32,
    pub claim: String,
    #[serde(default)]
    pub requires: Vec<Requirement>,
    #[serde(default)]
    pub contradicted_by: Vec<Requirement>,
    #[serde(default)]
    pub freshness: Freshness,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyBody {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub requires: Vec<Requirement>,
    #[serde(default)]
    pub contradicted_by: Vec<Requirement>,
    #[serde(default)]
    pub freshness: Freshness,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub kind: String,
    #[serde(default, rename = "match")]
    pub match_fields: BTreeMap<String, MatchAtom>,
    #[serde(default = "one")]
    pub min_count: u32,
    #[serde(default = "default_true")]
    pub subject_bound: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MatchAtom {
    Pred(MatchPred),
    Exact(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchPred {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eq: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neq: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Freshness {
    #[serde(default)]
    pub subject_match: SubjectMatch,
}

impl Default for Freshness {
    fn default() -> Self {
        Self {
            subject_match: SubjectMatch::Exact,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubjectMatch {
    #[default]
    Exact,
}

impl PolicyDocument {
    pub fn from_yaml(text: &str) -> Result<Self> {
        let doc: Self = serde_yaml::from_str(text)?;
        doc.validate()?;
        Ok(doc)
    }

    pub fn validate(&self) -> Result<()> {
        require_stable_id(&self.id, "policy").map_err(MenoError::InvalidPolicy)?;
        require_stable_id(&self.claim, "claim").map_err(MenoError::InvalidPolicy)?;
        if self.version == 0 {
            return Err(MenoError::InvalidPolicy(
                "policy version must be >= 1".into(),
            ));
        }
        validate_requirements(&self.requires)?;
        validate_requirements(&self.contradicted_by)?;
        Ok(())
    }

    pub fn from_body(claim_id: &str, body: PolicyBody) -> Result<Self> {
        let id = match body.id {
            Some(id) => id,
            None => format!("P-{claim_id}"),
        };
        let doc = Self {
            id,
            version: body.version,
            claim: claim_id.to_string(),
            requires: body.requires,
            contradicted_by: body.contradicted_by,
            freshness: body.freshness,
        };
        doc.validate()?;
        Ok(doc)
    }
}

fn validate_requirements(reqs: &[Requirement]) -> Result<()> {
    for req in reqs {
        if req.kind.trim().is_empty() {
            return Err(MenoError::InvalidPolicy(
                "requirement kind must be non-empty".into(),
            ));
        }
        if req.min_count == 0 {
            return Err(MenoError::InvalidPolicy(
                "requirement min_count must be >= 1".into(),
            ));
        }
        for pred in req.match_fields.values() {
            if let MatchAtom::Pred(p) = pred {
                if p.eq.is_none() && p.neq.is_none() {
                    return Err(MenoError::InvalidPolicy(
                        "match predicate must set eq or neq".into(),
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_neq_predicate() {
        let yaml = r#"
id: P17
version: 1
claim: C17
requires: []
contradicted_by:
  - kind: command.result
    match:
      exit_code:
        neq: 0
freshness:
  subject_match: exact
"#;
        let doc = PolicyDocument::from_yaml(yaml).unwrap();
        let atom = doc.contradicted_by[0]
            .match_fields
            .get("exit_code")
            .unwrap();
        match atom {
            MatchAtom::Pred(p) => assert_eq!(p.neq, Some(Value::from(0))),
            other => panic!("expected pred, got {other:?}"),
        }
    }
}
