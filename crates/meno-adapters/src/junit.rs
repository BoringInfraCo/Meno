//! JUnit XML ingestion. Observations only; this adapter never writes a verdict.
//!
//! CLI should call [`normalize_junit`] with the real subject id. The [`Adapter`]
//! impl uses an all-zero subject placeholder because [`InputBundle`] has none.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;

use meno_core::envelope::{
    new_ulid, seal_envelope, ArtifactRef, Envelope, Observation, Provenance, Source, Trust,
    TrustBasis, TrustOrigin, TrustRelation,
};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::contract::{
    Adapter, AdapterError, AdapterSafety, DetectContext, DetectResult, InputBundle, SideEffectLevel,
};

/// All-zero subject used only by [`JunitAdapter::normalize`]. Prefer [`normalize_junit`].
const PLACEHOLDER_SUBJECT: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JunitReport {
    pub suites: Vec<JunitSuite>,
    pub tests: u64,
    pub passed: u64,
    pub failed: u64,
    pub errors: u64,
    pub skipped: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JunitSuite {
    pub name: String,
    pub cases: Vec<JunitCase>,
    pub tests: u64,
    pub passed: u64,
    pub failed: u64,
    pub errors: u64,
    pub skipped: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JunitCase {
    pub name: String,
    pub classname: Option<String>,
    pub status: JunitStatus,
    pub time: Option<f64>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JunitStatus {
    Pass,
    Fail,
    Error,
    Skipped,
}

impl JunitStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Error => "error",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct JunitAdapter;

pub fn parse_junit_xml(xml: &[u8]) -> Result<JunitReport, AdapterError> {
    let xml = strip_bom(xml);
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut suites = Vec::new();
    let mut saw_root = false;

    loop {
        let event = reader.read_event_into(&mut buf).map_err(xml_err)?;
        let (name, attrs, empty) = match event {
            Event::Start(e) => (local_name(&e), Some(parse_attrs(&e)?), false),
            Event::Empty(e) => (local_name(&e), Some(parse_attrs(&e)?), true),
            Event::Eof => break,
            _ => {
                buf.clear();
                continue;
            }
        };
        match name.as_slice() {
            b"testsuites" => {
                saw_root = true;
                if !empty {
                    parse_testsuites(&mut reader, &mut buf, &mut suites)?;
                }
            }
            b"testsuite" => {
                saw_root = true;
                suites.extend(parse_suite(
                    &mut reader,
                    attrs.expect("testsuite attrs"),
                    empty,
                    &mut buf,
                )?);
            }
            _ if !empty => skip_element(&mut reader, &mut buf)?,
            _ => {}
        }
        buf.clear();
    }

    if !saw_root {
        return Err(AdapterError::message(
            "junit xml must have a testsuites or testsuite root",
        ));
    }

    Ok(report_from_suites(suites))
}

/// Build a `junit.report` envelope. Callers (CLI) pass the real `subject_id`.
pub fn normalize_junit(
    report: &JunitReport,
    subject_id: &str,
    source_name: &str,
    report_ref: Option<ArtifactRef>,
) -> Result<Envelope, AdapterError> {
    let mut observations = Vec::with_capacity(1 + report.tests as usize);
    observations.push(Observation {
        type_name: "junit.summary".to_string(),
        fields: json!({
            "tests": report.tests,
            "passed": report.passed,
            "failed": report.failed,
            "errors": report.errors,
            "skipped": report.skipped,
        }),
    });

    for suite in &report.suites {
        for case in &suite.cases {
            observations.push(Observation {
                type_name: "junit.case".to_string(),
                fields: case_fields(case),
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
        kind: "junit.report".to_string(),
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
            producer: "meno.junit".to_string(),
            producer_version: None,
            host: None,
            cwd: None,
        },
        integrity: None,
        trust: Trust {
            origin: TrustOrigin::Machine,
            reproducible: true,
            basis: TrustBasis::Observed,
            relation: TrustRelation::Direct,
        },
        source_metadata: json!({}),
    };
    seal_envelope(&mut envelope).map_err(|err| AdapterError::message(err.to_string()))?;
    Ok(envelope)
}

impl Adapter for JunitAdapter {
    fn name(&self) -> &str {
        "junit"
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
        if looks_like_xml(path) {
            return Ok(DetectResult {
                detected: true,
                detail: Some(path.display().to_string()),
            });
        }
        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                if looks_like_xml(&entry.path()) {
                    return Ok(DetectResult {
                        detected: true,
                        detail: Some(entry.path().display().to_string()),
                    });
                }
            }
            if let Some(found) = crate::discover::discover_junit(path)? {
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
        // Payload is raw JUnit XML. InputBundle has no subject_id; CLI should
        // call normalize_junit with the real subject instead of this trait method.
        let report = parse_junit_xml(&input.payload)?;
        let source_name = input
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("junit");
        normalize_junit(&report, PLACEHOLDER_SUBJECT, source_name, None)
    }

    fn validate(&self, envelope: &Envelope) -> Result<(), AdapterError> {
        meno_core::verify_envelope(envelope)
            .map_err(|err| AdapterError::message(err.to_string()))?;
        if envelope.kind != "junit.report" {
            return Err(AdapterError::message(format!(
                "expected kind junit.report, got {}",
                envelope.kind
            )));
        }
        Ok(())
    }
}

fn case_fields(case: &JunitCase) -> Value {
    let mut fields = Map::new();
    fields.insert("name".into(), json!(case.name));
    if let Some(classname) = &case.classname {
        fields.insert("classname".into(), json!(classname));
    }
    fields.insert("status".into(), json!(case.status.as_str()));
    if let Some(time) = case.time {
        fields.insert("time".into(), json!(time));
    }
    if let Some(message) = &case.message {
        fields.insert("message".into(), json!(message));
    }
    Value::Object(fields)
}

fn looks_like_xml(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
}

fn strip_bom(xml: &[u8]) -> &[u8] {
    if xml.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &xml[3..]
    } else {
        xml
    }
}

fn xml_err(err: impl std::fmt::Display) -> AdapterError {
    AdapterError::message(format!("junit xml: {err}"))
}

fn local_name(e: &BytesStart<'_>) -> Vec<u8> {
    e.local_name().as_ref().to_vec()
}

fn parse_attrs(e: &BytesStart<'_>) -> Result<HashMap<String, String>, AdapterError> {
    let mut map = HashMap::new();
    for attr in e.attributes().with_checks(false) {
        let attr = attr.map_err(xml_err)?;
        let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).into_owned();
        let value = attr.unescape_value().map_err(xml_err)?.into_owned();
        map.insert(key, value);
    }
    Ok(map)
}

fn attr<'a>(attrs: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    attrs.get(key).map(String::as_str).filter(|s| !s.is_empty())
}

fn attr_u64(attrs: &HashMap<String, String>, key: &str) -> Option<u64> {
    attr(attrs, key)?.parse().ok()
}

fn attr_f64(attrs: &HashMap<String, String>, key: &str) -> Option<f64> {
    attr(attrs, key)?.parse().ok()
}

fn parse_testsuites<R: BufRead>(
    reader: &mut Reader<R>,
    buf: &mut Vec<u8>,
    suites: &mut Vec<JunitSuite>,
) -> Result<(), AdapterError> {
    loop {
        let event = reader.read_event_into(buf).map_err(xml_err)?;
        let (name, attrs, empty) = match event {
            Event::Start(e) => (local_name(&e), Some(parse_attrs(&e)?), false),
            Event::Empty(e) => (local_name(&e), Some(parse_attrs(&e)?), true),
            Event::End(e) if e.local_name().as_ref() == b"testsuites" => return Ok(()),
            Event::Eof => {
                return Err(AdapterError::message("unterminated testsuites element"));
            }
            _ => {
                buf.clear();
                continue;
            }
        };
        match name.as_slice() {
            b"testsuite" => suites.extend(parse_suite(
                reader,
                attrs.expect("testsuite attrs"),
                empty,
                buf,
            )?),
            b"testsuites" if !empty => parse_testsuites(reader, buf, suites)?,
            _ if !empty => skip_element(reader, buf)?,
            _ => {}
        }
        buf.clear();
    }
}

fn parse_suite<R: BufRead>(
    reader: &mut Reader<R>,
    attrs: HashMap<String, String>,
    empty: bool,
    buf: &mut Vec<u8>,
) -> Result<Vec<JunitSuite>, AdapterError> {
    let name = attr(&attrs, "name").unwrap_or("").to_string();
    let time = attr_f64(&attrs, "time");
    let mut cases = Vec::new();
    let mut nested = Vec::new();

    if !empty {
        loop {
            let event = reader.read_event_into(buf).map_err(xml_err)?;
            let (local, child_attrs, child_empty) = match event {
                Event::Start(e) => (local_name(&e), Some(parse_attrs(&e)?), false),
                Event::Empty(e) => (local_name(&e), Some(parse_attrs(&e)?), true),
                Event::End(e) if e.local_name().as_ref() == b"testsuite" => break,
                Event::Eof => {
                    return Err(AdapterError::message("unterminated testsuite element"));
                }
                _ => {
                    buf.clear();
                    continue;
                }
            };
            match local.as_slice() {
                b"testcase" => {
                    cases.push(parse_case(
                        reader,
                        child_attrs.expect("testcase attrs"),
                        child_empty,
                        buf,
                    )?);
                }
                b"testsuite" => {
                    nested.extend(parse_suite(
                        reader,
                        child_attrs.expect("testsuite attrs"),
                        child_empty,
                        buf,
                    )?);
                }
                _ if !child_empty => skip_element(reader, buf)?,
                _ => {}
            }
            buf.clear();
        }
    }

    let computed = tally(&cases);
    let tests = attr_u64(&attrs, "tests").unwrap_or(computed.tests);
    let failed = attr_u64(&attrs, "failures").unwrap_or(computed.failed);
    let errors = attr_u64(&attrs, "errors").unwrap_or(computed.errors);
    let skipped = attr_u64(&attrs, "skipped").unwrap_or(computed.skipped);
    let passed = tests
        .saturating_sub(failed)
        .saturating_sub(errors)
        .saturating_sub(skipped);

    let mut out = vec![JunitSuite {
        name,
        cases,
        tests,
        passed,
        failed,
        errors,
        skipped,
        time,
    }];
    out.extend(nested);
    Ok(out)
}

fn parse_case<R: BufRead>(
    reader: &mut Reader<R>,
    attrs: HashMap<String, String>,
    empty: bool,
    buf: &mut Vec<u8>,
) -> Result<JunitCase, AdapterError> {
    let name = attr(&attrs, "name").unwrap_or("").to_string();
    let classname = attr(&attrs, "classname").map(str::to_string);
    let time = attr_f64(&attrs, "time");
    let mut status = JunitStatus::Pass;
    let mut message = None;

    if !empty {
        loop {
            let event = reader.read_event_into(buf).map_err(xml_err)?;
            let (local, child_message, need_skip) = match event {
                Event::Start(e) => {
                    let local = local_name(&e);
                    let child_attrs = parse_attrs(&e)?;
                    (
                        local,
                        attr(&child_attrs, "message").map(str::to_string),
                        true,
                    )
                }
                Event::Empty(e) => {
                    let local = local_name(&e);
                    let child_attrs = parse_attrs(&e)?;
                    (
                        local,
                        attr(&child_attrs, "message").map(str::to_string),
                        false,
                    )
                }
                Event::End(e) if e.local_name().as_ref() == b"testcase" => break,
                Event::Eof => {
                    return Err(AdapterError::message("unterminated testcase element"));
                }
                _ => {
                    buf.clear();
                    continue;
                }
            };
            if need_skip {
                skip_element(reader, buf)?;
            }
            apply_status(&mut status, &mut message, &local, child_message);
            buf.clear();
        }
    }

    Ok(JunitCase {
        name,
        classname,
        status,
        time,
        message,
    })
}

fn apply_status(
    status: &mut JunitStatus,
    message: &mut Option<String>,
    local: &[u8],
    child_message: Option<String>,
) {
    // error > failure > skipped > pass
    match local {
        b"error" => {
            *status = JunitStatus::Error;
            *message = child_message;
        }
        b"failure" if *status != JunitStatus::Error => {
            *status = JunitStatus::Fail;
            *message = child_message;
        }
        b"skipped" if *status == JunitStatus::Pass => {
            *status = JunitStatus::Skipped;
            *message = child_message;
        }
        _ => {}
    }
}

struct Counts {
    tests: u64,
    passed: u64,
    failed: u64,
    errors: u64,
    skipped: u64,
}

fn tally(cases: &[JunitCase]) -> Counts {
    let mut counts = Counts {
        tests: cases.len() as u64,
        passed: 0,
        failed: 0,
        errors: 0,
        skipped: 0,
    };
    for case in cases {
        match case.status {
            JunitStatus::Pass => counts.passed += 1,
            JunitStatus::Fail => counts.failed += 1,
            JunitStatus::Error => counts.errors += 1,
            JunitStatus::Skipped => counts.skipped += 1,
        }
    }
    counts
}

fn report_from_suites(suites: Vec<JunitSuite>) -> JunitReport {
    let mut tests = 0;
    let mut passed = 0;
    let mut failed = 0;
    let mut errors = 0;
    let mut skipped = 0;
    for suite in &suites {
        let counts = tally(&suite.cases);
        tests += counts.tests;
        passed += counts.passed;
        failed += counts.failed;
        errors += counts.errors;
        skipped += counts.skipped;
    }
    JunitReport {
        suites,
        tests,
        passed,
        failed,
        errors,
        skipped,
    }
}

fn skip_element<R: BufRead>(reader: &mut Reader<R>, buf: &mut Vec<u8>) -> Result<(), AdapterError> {
    let mut depth = 1u32;
    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(_)) => {
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
            }
            Ok(Event::Eof) => return Err(AdapterError::message("unterminated xml element")),
            Err(err) => return Err(xml_err(err)),
            _ => {}
        }
        buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meno_core::envelope::Envelope;
    use serde_json::json;
    use std::collections::BTreeSet;
    use tempfile::TempDir;

    const ALL_PASS: &[u8] = include_bytes!("../tests/fixtures/junit-all-pass.xml");
    const MIXED: &[u8] = include_bytes!("../tests/fixtures/junit-mixed.xml");

    fn subject() -> String {
        "0".repeat(64)
    }

    #[test]
    fn all_pass_summary_counts() {
        let report = parse_junit_xml(ALL_PASS).expect("parse all-pass");
        assert_eq!(report.tests, 2);
        assert_eq!(report.passed, 2);
        assert_eq!(report.failed, 0);
        assert_eq!(report.errors, 0);
        assert_eq!(report.skipped, 0);
        assert_eq!(report.suites.len(), 1);
        assert_eq!(report.suites[0].cases.len(), 2);
        assert!(report
            .suites
            .iter()
            .flat_map(|s| &s.cases)
            .all(|c| c.status == JunitStatus::Pass));
    }

    #[test]
    fn mixed_counts_and_distinct_statuses() {
        let report = parse_junit_xml(MIXED).expect("parse mixed");
        assert_eq!(report.tests, 4);
        assert_eq!(report.passed, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.errors, 1);
        assert_eq!(report.skipped, 1);

        let statuses: BTreeSet<_> = report
            .suites
            .iter()
            .flat_map(|s| s.cases.iter().map(|c| c.status))
            .collect();
        assert_eq!(
            statuses,
            BTreeSet::from([
                JunitStatus::Pass,
                JunitStatus::Fail,
                JunitStatus::Error,
                JunitStatus::Skipped,
            ])
        );

        let by_name: HashMap<_, _> = report
            .suites
            .iter()
            .flat_map(|s| s.cases.iter())
            .map(|c| (c.name.as_str(), c))
            .collect();
        assert_eq!(by_name["passes"].status, JunitStatus::Pass);
        assert_eq!(by_name["fails"].status, JunitStatus::Fail);
        assert_eq!(by_name["errors"].status, JunitStatus::Error);
        assert_eq!(by_name["skipped"].status, JunitStatus::Skipped);
        assert_eq!(
            by_name["fails"].message.as_deref(),
            Some("assertion failed")
        );
        assert_eq!(by_name["errors"].message.as_deref(), Some("boom"));
    }

    #[test]
    fn normalize_junit_kind_and_observations() {
        let report = parse_junit_xml(MIXED).expect("parse mixed");
        let report_ref = ArtifactRef {
            sha256: "aa".repeat(32),
            media_type: Some("application/xml".into()),
            size: MIXED.len() as u64,
            relative_path: "junit-mixed.xml".into(),
        };
        let envelope = normalize_junit(&report, &subject(), "surefire", Some(report_ref.clone()))
            .expect("normalize");

        let _: Envelope = envelope.clone();
        assert_eq!(envelope.kind, "junit.report");
        assert_eq!(envelope.provenance.producer, "meno.junit");
        assert_eq!(envelope.source.name, "surefire");
        assert_eq!(envelope.trust.origin, TrustOrigin::Machine);
        assert!(envelope.trust.reproducible);
        assert_eq!(envelope.trust.basis, TrustBasis::Observed);
        assert_eq!(envelope.trust.relation, TrustRelation::Direct);
        assert_eq!(envelope.artifact_refs, vec![report_ref]);

        let types: Vec<_> = envelope
            .observations
            .iter()
            .map(|o| o.type_name.as_str())
            .collect();
        assert!(types.contains(&"junit.summary"));
        assert_eq!(types.iter().filter(|t| **t == "junit.case").count(), 4);

        let summary = envelope
            .observations
            .iter()
            .find(|o| o.type_name == "junit.summary")
            .expect("summary");
        assert_eq!(summary.fields["tests"], json!(4));
        assert_eq!(summary.fields["passed"], json!(1));
        assert_eq!(summary.fields["failed"], json!(1));
        assert_eq!(summary.fields["errors"], json!(1));
        assert_eq!(summary.fields["skipped"], json!(1));

        let case_statuses: BTreeSet<_> = envelope
            .observations
            .iter()
            .filter(|o| o.type_name == "junit.case")
            .map(|o| o.fields["status"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            case_statuses,
            BTreeSet::from([
                "pass".into(),
                "fail".into(),
                "error".into(),
                "skipped".into()
            ])
        );

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
    fn adapter_detects_xml_path_and_normalizes_payload() {
        let dir = TempDir::new().expect("tempdir");
        let xml_path = dir.path().join("report.xml");
        std::fs::write(&xml_path, ALL_PASS).unwrap();

        let adapter = JunitAdapter;
        let detected = adapter
            .detect(&DetectContext {
                project_root: xml_path.clone(),
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
                kind: "junit.report".into(),
                payload: ALL_PASS.to_vec(),
                path: Some(xml_path),
            })
            .expect("normalize xml payload");
        assert_eq!(envelope.kind, "junit.report");
        assert_eq!(envelope.subject_id, PLACEHOLDER_SUBJECT);
        assert_eq!(envelope.source.name, "report.xml");
        adapter.validate(&envelope).expect("validate");
    }
}
