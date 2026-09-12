use regex::bytes::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedactionAction {
    Clean,
    Redacted(Vec<u8>),
    Reject(String),
}

pub fn redact_bytes(input: &[u8]) -> RedactionAction {
    if pem_private_key().is_match(input) {
        return RedactionAction::Reject("PEM private key".into());
    }

    let mut out = input.to_vec();
    out = authorization()
        .replace_all(&out, b"Authorization: ***REDACTED***".as_ref())
        .into_owned();
    out = aws_key()
        .replace_all(&out, b"***REDACTED***".as_ref())
        .into_owned();
    out = github_pat()
        .replace_all(&out, b"***REDACTED***".as_ref())
        .into_owned();
    out = secret_assignment()
        .replace_all(&out, b"$1=***REDACTED***".as_ref())
        .into_owned();
    out = db_url()
        .replace_all(&out, b"$1://***REDACTED***@".as_ref())
        .into_owned();

    if out.as_slice() == input {
        RedactionAction::Clean
    } else {
        RedactionAction::Redacted(out)
    }
}

pub fn contains_credential_shaped(input: &[u8]) -> bool {
    !matches!(redact_bytes(input), RedactionAction::Clean)
}

fn pem_private_key() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"-----BEGIN[ A-Z0-9]*PRIVATE KEY-----").expect("pem regex"))
}

fn authorization() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)Authorization:[^\S\r\n]*[^\r\n]+").expect("authorization regex")
    })
}

fn aws_key() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"AKIA[0-9A-Z]{16}").expect("aws regex"))
}

fn github_pat() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"ghp_[A-Za-z0-9]{20,}").expect("github pat regex"))
}

fn secret_assignment() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)(password|secret)=[^\s&"']+"#).expect("assignment regex"))
}

fn db_url() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?i)(postgres|mysql|mongodb)://[^@\s"']+@"#).expect("db url regex")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redacted(input: &str) -> String {
        match redact_bytes(input.as_bytes()) {
            RedactionAction::Redacted(bytes) => String::from_utf8(bytes).unwrap(),
            other => panic!("expected Redacted, got {other:?}"),
        }
    }

    #[test]
    fn clean_text() {
        assert_eq!(redact_bytes(b"hello world"), RedactionAction::Clean);
        assert!(!contains_credential_shaped(b"hello world"));
    }

    #[test]
    fn rejects_pem_private_key() {
        let pem =
            b"-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n-----END RSA PRIVATE KEY-----";
        match redact_bytes(pem) {
            RedactionAction::Reject(reason) => {
                assert!(reason.contains("PRIVATE KEY") || reason.contains("PEM"))
            }
            other => panic!("expected Reject, got {other:?}"),
        }
        assert!(contains_credential_shaped(pem));
    }

    #[test]
    fn reject_takes_precedence() {
        let mixed =
            b"AKIAIOSFODNN7EXAMPLE\n-----BEGIN PRIVATE KEY-----\nxx\n-----END PRIVATE KEY-----";
        assert!(matches!(redact_bytes(mixed), RedactionAction::Reject(_)));
    }

    #[test]
    fn redacts_authorization_header() {
        let out = redacted("GET /\nAuthorization: Bearer super-secret\nHost: x");
        assert!(out.contains("Authorization: ***REDACTED***"));
        assert!(!out.contains("super-secret"));
        assert!(contains_credential_shaped(
            b"Authorization: Bearer super-secret"
        ));
    }

    #[test]
    fn redacts_aws_access_key() {
        let out = redacted("key=AKIAIOSFODNN7EXAMPLE done");
        assert_eq!(out, "key=***REDACTED*** done");
    }

    #[test]
    fn redacts_github_pat() {
        let pat = "ghp_0123456789abcdefghij";
        let out = redacted(&format!("token {pat}"));
        assert_eq!(out, "token ***REDACTED***");
    }

    #[test]
    fn redacts_password_and_secret_assignments() {
        assert_eq!(
            redacted("password=hunter2 extra"),
            "password=***REDACTED*** extra"
        );
        assert_eq!(redacted("secret=s3cret&x=1"), "secret=***REDACTED***&x=1");
    }

    #[test]
    fn redacts_db_urls_with_userinfo() {
        assert_eq!(
            redacted("postgres://user:pass@localhost/db"),
            "postgres://***REDACTED***@localhost/db"
        );
        assert_eq!(
            redacted("mysql://root:pw@127.0.0.1:3306/app"),
            "mysql://***REDACTED***@127.0.0.1:3306/app"
        );
        assert_eq!(
            redacted("mongodb://alice:s3@mongo.example/app"),
            "mongodb://***REDACTED***@mongo.example/app"
        );
        assert_eq!(
            redact_bytes(b"postgres://localhost/db"),
            RedactionAction::Clean
        );
    }
}
