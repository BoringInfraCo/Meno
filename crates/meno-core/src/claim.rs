use crate::error::{MenoError, Result};
use crate::ids::require_stable_id;
use crate::policy::{PolicyBody, PolicyDocument};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimState {
    Draft,
    Frozen,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginKind {
    Human,
    Imported,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub kind: OriginKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimDocument {
    pub id: String,
    pub statement: String,
    pub state: ClaimState,
    pub origin: Origin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyBody>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_ref: Option<String>,
}

impl ClaimDocument {
    pub fn from_yaml(text: &str) -> Result<Self> {
        reject_runtime_fields(text)?;
        let doc: Self = serde_yaml::from_str(text)?;
        doc.validate()?;
        Ok(doc)
    }

    pub fn validate(&self) -> Result<()> {
        require_stable_id(&self.id, "claim").map_err(MenoError::InvalidClaim)?;
        if self.statement.trim().is_empty() {
            return Err(MenoError::InvalidClaim(
                "statement must be non-empty".into(),
            ));
        }
        match (&self.policy, &self.policy_ref) {
            (Some(_), Some(_)) => {
                return Err(MenoError::InvalidClaim(
                    "claim must not set both policy and policy_ref".into(),
                ));
            }
            (None, None) => {
                return Err(MenoError::InvalidClaim(
                    "claim must set policy or policy_ref".into(),
                ));
            }
            (None, Some(pref)) => {
                require_stable_id(pref, "policy").map_err(MenoError::InvalidClaim)?;
            }
            (Some(body), None) => {
                PolicyDocument::from_body(&self.id, body.clone())?;
            }
        }
        Ok(())
    }

    pub fn resolved_policy(&self) -> Result<Option<PolicyDocument>> {
        match &self.policy {
            Some(body) => Ok(Some(PolicyDocument::from_body(&self.id, body.clone())?)),
            None => Ok(None),
        }
    }
}

fn reject_runtime_fields(text: &str) -> Result<()> {
    let value: serde_yaml::Value = serde_yaml::from_str(text)?;
    let Some(map) = value.as_mapping() else {
        return Err(MenoError::InvalidClaim("claim must be a mapping".into()));
    };
    for forbidden in ["evidence", "verdict", "subject", "observations"] {
        if map.contains_key(serde_yaml::Value::String(forbidden.into())) {
            return Err(MenoError::InvalidClaim(format!(
                "runtime field {forbidden:?} is not allowed in claim files"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_evidence_field() {
        let yaml = r#"
id: C17
statement: "x"
state: draft
origin:
  kind: human
policy_ref: P17
evidence: []
"#;
        let err = ClaimDocument::from_yaml(yaml).unwrap_err().to_string();
        assert!(err.contains("evidence"), "{err}");
    }
}
