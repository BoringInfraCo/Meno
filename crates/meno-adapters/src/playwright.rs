//! Playwright JSON reporter ingestion. Observations only; this adapter never writes a verdict.
//!
//! Meno does not ship Chromium or run Playwright. Callers pass JSON produced by
//! Playwright's JSON reporter. When a test has multiple `results` (retries), the
//! last result is used.
//!
//! CLI should call [`normalize_playwright`] with the real subject id. The [`Adapter`]
//! impl uses an all-zero subject placeholder because [`InputBundle`] has none.

use std::path::Path;

use meno_core::envelope::{
    new_ulid, seal_envelope, ArtifactRef, Envelope, Observation, Provenance, Source, Trust,
    TrustBasis, TrustOrigin, TrustRelation,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::contract::{
    Adapter, AdapterError, AdapterSafety, DetectContext, DetectResult, InputBundle, SideEffectLevel,
};

/// All-zero subject used only by [`PlaywrightAdapter::normalize`]. Prefer [`normalize_playwright`].
const PLACEHOLDER_SUBJECT: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaywrightReport {
    pub tests: Vec<PlaywrightTest>,
    /// Count of tests whose last result is `passed`.
    pub expected: u64,
    /// Count of tests whose last result is `failed`, `timedOut`, or `interrupted`.
    pub unexpected: u64,
    pub skipped: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaywrightTest {
    pub title: String,
    pub file: Option<String>,
    pub project: Option<String>,
    /// `passed` | `failed` | `timedOut` | `skipped` | `interrupted`
    pub status: String,
    pub duration_ms: Option<u64>,
    pub url: Option<String>,
    pub route: Option<String>,
    pub attachments: Vec<PlaywrightAttachment>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaywrightAttachment {
    /// screenshot, trace, video, ...
    pub name: String,
    pub content_type: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PlaywrightAdapter;

/// True for Playwright JSON reporter output (or a flat fixture with the same shape).
///
/// An object is Playwright-like if it has a `suites` array, a `config` object plus
/// `suites`, or a `tests` array whose items carry `projectName` / `results`.
/// Meno envelopes (`kind` + `observations` + `trust`) are never treated as Playwright.
pub fn looks_like_playwright_json(value: &Value) -> bool {
    let Some(obj) = value.as_object() else {
        return false;
    };
    if obj.contains_key("kind") && obj.contains_key("observations") && obj.contains_key("trust") {
        return false;
    }
    if obj.get("suites").is_some_and(Value::is_array) {
        return true;
    }
    if obj.get("config").is_some_and(Value::is_object) && obj.contains_key("suites") {
        return true;
    }
    tests_have_playwright_shape(obj.get("tests"))
}

pub fn parse_playwright_json(json: &[u8]) -> Result<PlaywrightReport, AdapterError> {
    let json = strip_bom(json);
    let value: Value = serde_json::from_slice(json)
        .map_err(|err| AdapterError::message(format!("playwright json: {err}")))?;
    let Some(root) = value.as_object() else {
        return Err(AdapterError::message("playwright json must be an object"));
    };

    let mut tests = Vec::new();
    if let Some(suites) = root.get("suites").and_then(Value::as_array) {
        collect_from_suites(suites, &[], None, &mut tests);
    } else if let Some(top_tests) = root.get("tests").and_then(Value::as_array) {
        for test in top_tests {
            tests.push(parse_test(test, "", None, None));
        }
    }

    Ok(report_from_tests(tests))
}

/// Build a `playwright.result` envelope. Callers (CLI) pass the real `subject_id`.
pub fn normalize_playwright(
    report: &PlaywrightReport,
    subject_id: &str,
    source_name: &str,
    report_ref: Option<ArtifactRef>,
) -> Result<Envelope, AdapterError> {
    let attachment_count: usize = report.tests.iter().map(|t| t.attachments.len()).sum();
    let mut observations = Vec::with_capacity(1 + report.tests.len() + attachment_count);
    observations.push(Observation {
        type_name: "playwright.summary".to_string(),
        fields: json!({
            "expected": report.expected,
            "unexpected": report.unexpected,
            "skipped": report.skipped,
        }),
    });

    for test in &report.tests {
        observations.push(Observation {
            type_name: "playwright.test".to_string(),
            fields: test_fields(test),
        });
        for attachment in &test.attachments {
            observations.push(Observation {
                type_name: "playwright.attachment".to_string(),
                fields: attachment_fields(attachment),
            });
        }
    }

    let mut artifact_refs = Vec::new();
    if let Some(report_ref) = report_ref {
        artifact_refs.push(report_ref);
    }

    let captured_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|err| AdapterError::message(err.to_string()))?;

    let mut envelope = Envelope {
        id: new_ulid(),
        kind: "playwright.result".to_string(),
        subject_id: subject_id.to_string(),
        source: Source {
            name: source_name.to_string(),
            version: None,
            argv: None,
            config_digest: None,
        },
        observations,
        artifact_refs,
        captured_at,
        provenance: Provenance {
            actor: None,
            producer: "meno.playwright".to_string(),
            producer_version: None,
            host: None,
            cwd: None,
        },
        integrity: None,
        trust: Trust {
            origin: TrustOrigin::Machine,
            reproducible: false,
            basis: TrustBasis::Observed,
            relation: TrustRelation::Direct,
        },
        source_metadata: json!({}),
    };
    seal_envelope(&mut envelope).map_err(|err| AdapterError::message(err.to_string()))?;
    Ok(envelope)
}

impl Adapter for PlaywrightAdapter {
    fn name(&self) -> &str {
        "playwright"
    }

    fn safety(&self) -> AdapterSafety {
        AdapterSafety {
            can_collect: true,
            can_invoke: false,
            side_effect_level: SideEffectLevel::None,
            requires_confirmation: false,
        }
    }

    fn detect(&self, ctx: &DetectContext) -> Result<DetectResult, AdapterError> {
        let path = &ctx.project_root;
        if looks_like_playwright_file(path)? {
            return Ok(DetectResult {
                detected: true,
                detail: Some(path.display().to_string()),
            });
        }
        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                if looks_like_playwright_file(&entry.path())? {
                    return Ok(DetectResult {
                        detected: true,
                        detail: Some(entry.path().display().to_string()),
                    });
                }
            }
            if let Some(found) = crate::discover::discover_playwright(path)? {
                return Ok(DetectResult {
                    detected: true,
                    detail: Some(found.detail),
                });
            }
        }
        Ok(DetectResult {
            detected: false,
            detail: None,
        })
    }

    fn normalize(&self, input: &InputBundle) -> Result<Envelope, AdapterError> {
        // Payload is Playwright JSON. InputBundle has no subject_id; CLI should
        // call normalize_playwright with the real subject instead of this trait method.
        let report = parse_playwright_json(&input.payload)?;
        let source_name = input
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("playwright");
        normalize_playwright(&report, PLACEHOLDER_SUBJECT, source_name, None)
    }

    fn validate(&self, envelope: &Envelope) -> Result<(), AdapterError> {
        meno_core::verify_envelope(envelope)
            .map_err(|err| AdapterError::message(err.to_string()))?;
        if envelope.kind != "playwright.result" {
            return Err(AdapterError::message(format!(
                "expected kind playwright.result, got {}",
                envelope.kind
            )));
        }
        Ok(())
    }
}

fn collect_from_suites(
    suites: &[Value],
    parent_titles: &[String],
    parent_file: Option<&str>,
    tests: &mut Vec<PlaywrightTest>,
) {
    for suite in suites {
        let suite_title = string_field(suite, "title");
        let file = string_field(suite, "file");
        let file_ref = file.as_deref().or(parent_file);
        let mut titles = parent_titles.to_vec();
        if let Some(title) = suite_title {
            titles.push(title);
        }
        if let Some(specs) = suite.get("specs").and_then(Value::as_array) {
            for spec in specs {
                collect_from_spec(spec, &titles, file_ref, tests);
            }
        }
        if let Some(nested) = suite.get("suites").and_then(Value::as_array) {
            collect_from_suites(nested, &titles, file_ref, tests);
        }
    }
}

fn collect_from_spec(
    spec: &Value,
    parent_titles: &[String],
    parent_file: Option<&str>,
    tests: &mut Vec<PlaywrightTest>,
) {
    let spec_title = string_field(spec, "title");
    let file = string_field(spec, "file").or_else(|| parent_file.map(str::to_string));
    let mut titles = parent_titles.to_vec();
    if let Some(title) = spec_title {
        titles.push(title);
    }
    let title = titles.join(" › ");
    let spec_ok = spec.get("ok").and_then(Value::as_bool);
    if let Some(spec_tests) = spec.get("tests").and_then(Value::as_array) {
        for test in spec_tests {
            tests.push(parse_test(test, &title, file.clone(), spec_ok));
        }
    }
}

fn parse_test(
    test: &Value,
    title: &str,
    file: Option<String>,
    spec_ok: Option<bool>,
) -> PlaywrightTest {
    let project = string_field(test, "projectName");
    // Last result wins when Playwright retried the test.
    let last_result = test
        .get("results")
        .and_then(Value::as_array)
        .and_then(|results| results.last());
    let status = result_status(last_result, test, spec_ok);
    let duration_ms = last_result.and_then(|result| json_u64(result.get("duration")));
    let attachments = last_result.map(extract_attachments).unwrap_or_default();
    let error = last_result.and_then(extract_error);
    let (url, route) = extract_url_route(test, last_result);

    PlaywrightTest {
        title: title.to_string(),
        file,
        project,
        status,
        duration_ms,
        url,
        route,
        attachments,
        error,
    }
}

fn result_status(result: Option<&Value>, test: &Value, spec_ok: Option<bool>) -> String {
    if let Some(status) = result.and_then(|r| string_field(r, "status")) {
        return status;
    }
    let ok = result
        .and_then(|r| r.get("ok"))
        .and_then(Value::as_bool)
        .or_else(|| test.get("ok").and_then(Value::as_bool))
        .or(spec_ok)
        .unwrap_or(true);
    if ok {
        "passed".to_string()
    } else {
        "failed".to_string()
    }
}

fn extract_attachments(result: &Value) -> Vec<PlaywrightAttachment> {
    let Some(items) = result.get("attachments").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let name = string_field(item, "name")?;
            Some(PlaywrightAttachment {
                name,
                content_type: string_field(item, "contentType")
                    .or_else(|| string_field(item, "content_type")),
                path: string_field(item, "path"),
            })
        })
        .collect()
}

fn extract_error(result: &Value) -> Option<String> {
    if let Some(message) = result
        .get("error")
        .and_then(|error| string_field(error, "message"))
    {
        return Some(message);
    }
    result
        .get("errors")
        .and_then(Value::as_array)
        .and_then(|errors| {
            errors
                .iter()
                .find_map(|error| string_field(error, "message"))
        })
}

fn extract_url_route(test: &Value, result: Option<&Value>) -> (Option<String>, Option<String>) {
    let mut url = result.and_then(|r| string_field(r, "url"));
    let mut route = result.and_then(|r| string_field(r, "route"));
    if let Some(result) = result {
        take_from_annotations(result.get("annotations"), &mut url, &mut route);
    }
    if url.is_none() {
        url = string_field(test, "url");
    }
    if route.is_none() {
        route = string_field(test, "route");
    }
    take_from_annotations(test.get("annotations"), &mut url, &mut route);
    (url, route)
}

fn take_from_annotations(
    annotations: Option<&Value>,
    url: &mut Option<String>,
    route: &mut Option<String>,
) {
    let Some(items) = annotations.and_then(Value::as_array) else {
        return;
    };
    for item in items {
        if url.is_none() {
            *url = string_field(item, "url");
        }
        if route.is_none() {
            *route = string_field(item, "route");
        }
        let Some(kind) = item.get("type").and_then(Value::as_str) else {
            continue;
        };
        let description = string_field(item, "description");
        if url.is_none() && kind.eq_ignore_ascii_case("url") {
            *url = description.clone();
        }
        if route.is_none() && kind.eq_ignore_ascii_case("route") {
            *route = description;
        }
    }
}

fn report_from_tests(tests: Vec<PlaywrightTest>) -> PlaywrightReport {
    let mut expected = 0;
    let mut unexpected = 0;
    let mut skipped = 0;
    for test in &tests {
        match test.status.as_str() {
            "passed" => expected += 1,
            "skipped" => skipped += 1,
            _ => unexpected += 1,
        }
    }
    PlaywrightReport {
        tests,
        expected,
        unexpected,
        skipped,
    }
}

fn test_fields(test: &PlaywrightTest) -> Value {
    let mut fields = Map::new();
    fields.insert("title".into(), json!(test.title));
    fields.insert("status".into(), json!(test.status));
    if let Some(file) = &test.file {
        fields.insert("file".into(), json!(file));
    }
    if let Some(project) = &test.project {
        fields.insert("project".into(), json!(project));
    }
    if let Some(duration_ms) = test.duration_ms {
        fields.insert("duration_ms".into(), json!(duration_ms));
    }
    if let Some(url) = &test.url {
        fields.insert("url".into(), json!(url));
    }
    if let Some(route) = &test.route {
        fields.insert("route".into(), json!(route));
    }
    Value::Object(fields)
}

fn attachment_fields(attachment: &PlaywrightAttachment) -> Value {
    let mut fields = Map::new();
    fields.insert("name".into(), json!(attachment.name));
    if let Some(content_type) = &attachment.content_type {
        fields.insert("content_type".into(), json!(content_type));
    }
    if let Some(path) = &attachment.path {
        fields.insert("path".into(), json!(path));
    }
    Value::Object(fields)
}

fn tests_have_playwright_shape(tests: Option<&Value>) -> bool {
    tests.and_then(Value::as_array).is_some_and(|arr| {
        arr.iter()
            .any(|test| test.get("projectName").is_some() || test.get("results").is_some())
    })
}

fn looks_like_playwright_file(path: &Path) -> Result<bool, AdapterError> {
    if !path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        return Ok(false);
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err.into()),
    };
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) => Ok(looks_like_playwright_json(&value)),
        Err(_) => Ok(false),
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn json_u64(value: Option<&Value>) -> Option<u64> {
    let value = value?;
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
        .or_else(|| {
            value.as_f64().and_then(|n| {
                if n.is_finite() && n >= 0.0 {
                    Some(n as u64)
                } else {
                    None
                }
            })
        })
}

fn strip_bom(json: &[u8]) -> &[u8] {
    if json.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &json[3..]
    } else {
        json
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meno_core::envelope::Envelope;
    use serde_json::json;
    use tempfile::TempDir;

    const PASS: &[u8] = include_bytes!("../tests/fixtures/playwright-pass.json");
    const FAIL: &[u8] = include_bytes!("../tests/fixtures/playwright-fail.json");

    fn subject() -> String {
        "0".repeat(64)
    }

    fn envelope_shaped() -> Value {
        json!({
            "kind": "generic.envelope",
            "observations": [],
            "trust": {
                "origin": "machine",
                "reproducible": true,
                "basis": "observed",
                "relation": "direct"
            }
        })
    }

    #[test]
    fn parse_pass_fixture_counts_and_url() {
        let report = parse_playwright_json(PASS).expect("parse pass");
        assert_eq!(report.unexpected, 0);
        assert!(report.expected >= 1);
        assert!(
            report.tests.iter().any(|t| t.url.is_some()),
            "expected a test url in the pass fixture"
        );
    }

    #[test]
    fn parse_fail_fixture_unexpected_and_error() {
        let report = parse_playwright_json(FAIL).expect("parse fail");
        assert!(report.unexpected >= 1);
        assert!(report.tests.iter().any(|t| t.status == "failed"));
        assert!(
            report
                .tests
                .iter()
                .any(|t| t.status == "failed" && t.error.is_some()),
            "failed test should carry an error message"
        );
    }

    #[test]
    fn normalize_playwright_kind_observations_and_machine_trust() {
        let report = parse_playwright_json(PASS).expect("parse pass");
        let report_ref = ArtifactRef {
            sha256: "aa".repeat(32),
            media_type: Some("application/json".into()),
            size: PASS.len() as u64,
            relative_path: "playwright-pass.json".into(),
        };
        let envelope =
            normalize_playwright(&report, &subject(), "playwright", Some(report_ref.clone()))
                .expect("normalize");

        let _: Envelope = envelope.clone();
        assert_eq!(envelope.kind, "playwright.result");
        assert_eq!(envelope.provenance.producer, "meno.playwright");
        assert_eq!(envelope.trust.origin, TrustOrigin::Machine);
        assert_ne!(envelope.trust.origin, TrustOrigin::Human);
        assert!(!envelope.trust.reproducible);
        assert_eq!(envelope.trust.basis, TrustBasis::Observed);
        assert_eq!(envelope.trust.relation, TrustRelation::Direct);
        assert_eq!(envelope.artifact_refs, vec![report_ref]);

        let types: Vec<_> = envelope
            .observations
            .iter()
            .map(|o| o.type_name.as_str())
            .collect();
        assert!(types.contains(&"playwright.summary"));
        assert!(types.contains(&"playwright.test"));

        let summary = envelope
            .observations
            .iter()
            .find(|o| o.type_name == "playwright.summary")
            .expect("summary");
        assert_eq!(summary.fields["expected"], json!(report.expected));
        assert_eq!(summary.fields["unexpected"], json!(report.unexpected));
        assert_eq!(summary.fields["skipped"], json!(report.skipped));

        let value = serde_json::to_value(&envelope).expect("json");
        assert!(value.get("verdict").is_none());
        for obs in &envelope.observations {
            assert!(
                !obs.type_name.to_ascii_lowercase().contains("verdict"),
                "adapters must not emit Verdict types, got {}",
                obs.type_name
            );
        }
        meno_core::verify_envelope(&envelope).expect("sealed envelope verifies");
    }

    #[test]
    fn looks_like_playwright_json_distinguishes_fixture_from_envelope() {
        let pass: Value = serde_json::from_slice(PASS).expect("pass json");
        assert!(looks_like_playwright_json(&pass));
        assert!(!looks_like_playwright_json(&envelope_shaped()));
    }

    #[test]
    fn nested_suites_are_flattened_and_last_result_wins() {
        let json = json!({
            "config": { "projects": [] },
            "suites": [{
                "title": "outer.spec.ts",
                "file": "outer.spec.ts",
                "suites": [{
                    "title": "account",
                    "file": "outer.spec.ts",
                    "specs": [{
                        "title": "retries then passes",
                        "file": "outer.spec.ts",
                        "ok": true,
                        "tests": [{
                            "projectName": "chromium",
                            "results": [
                                { "status": "failed", "duration": 10 },
                                {
                                    "status": "passed",
                                    "duration": 20,
                                    "route": "/welcome",
                                    "attachments": [{ "name": "trace", "path": "trace.zip" }]
                                }
                            ]
                        }]
                    }]
                }]
            }]
        });
        let report = parse_playwright_json(serde_json::to_vec(&json).unwrap().as_slice())
            .expect("parse nested");
        assert_eq!(report.tests.len(), 1);
        assert_eq!(report.expected, 1);
        assert_eq!(report.unexpected, 0);
        assert_eq!(
            report.tests[0].title,
            "outer.spec.ts › account › retries then passes"
        );
        assert_eq!(report.tests[0].status, "passed");
        assert_eq!(report.tests[0].duration_ms, Some(20));
        assert_eq!(report.tests[0].route.as_deref(), Some("/welcome"));
        assert_eq!(report.tests[0].attachments.len(), 1);
        assert!(looks_like_playwright_json(&json));
    }

    #[test]
    fn adapter_detects_json_path_and_normalizes_payload() {
        let dir = TempDir::new().expect("tempdir");
        let json_path = dir.path().join("report.json");
        std::fs::write(&json_path, PASS).unwrap();

        let adapter = PlaywrightAdapter;
        let detected = adapter
            .detect(&DetectContext {
                project_root: json_path.clone(),
            })
            .expect("detect file");
        assert!(detected.detected);

        let dir_detected = adapter
            .detect(&DetectContext {
                project_root: dir.path().to_path_buf(),
            })
            .expect("detect dir");
        assert!(dir_detected.detected);

        let envelope = adapter
            .normalize(&InputBundle {
                kind: "playwright.result".into(),
                payload: PASS.to_vec(),
                path: Some(json_path),
            })
            .expect("normalize json payload");
        assert_eq!(envelope.kind, "playwright.result");
        assert_eq!(envelope.subject_id, PLACEHOLDER_SUBJECT);
        assert_eq!(envelope.source.name, "report.json");
        adapter.validate(&envelope).expect("validate");
    }
}
