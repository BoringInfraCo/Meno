use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Proven,
    Disproven,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&Verdict::Proven).unwrap(),
            "\"proven\""
        );
        assert_eq!(
            serde_json::to_string(&Verdict::Disproven).unwrap(),
            "\"disproven\""
        );
        assert_eq!(
            serde_json::to_string(&Verdict::Unknown).unwrap(),
            "\"unknown\""
        );
        assert_eq!(
            serde_json::from_str::<Verdict>("\"unknown\"").unwrap(),
            Verdict::Unknown
        );
    }
}
