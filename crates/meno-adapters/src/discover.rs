//! Read-only project scan for connectable integrations. Never writes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::contract::AdapterError;

const PLAYWRIGHT_CONFIGS: &[&str] = &[
    "playwright.config.ts",
    "playwright.config.js",
    "playwright.config.mts",
    "playwright.config.mjs",
    "playwright.config.cjs",
];

/// A candidate integration found under a project root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovered {
    /// `"junit"` | `"playwright"` | `"command"` | `"agent"`
    pub adapter: String,
    /// Human one-liner describing why this was found.
    pub detail: String,
    pub suggested_name: String,
    pub suggested_path: Option<PathBuf>,
}

/// Scan `project_root`. Read-only. Never writes.
pub fn discover_integrations(project_root: &Path) -> Result<Vec<Discovered>, AdapterError> {
    let mut found = Vec::new();
    if let Some(item) = discover_playwright(project_root)? {
        found.push(item);
    }
    if let Some(item) = discover_junit(project_root)? {
        found.push(item);
    }
    // Command is not auto-detected: do not invent `cargo test` or Makefile invoke.
    found.push(agent_stub());
    Ok(found)
}

pub(crate) fn discover_playwright(project_root: &Path) -> Result<Option<Discovered>, AdapterError> {
    let detail = if let Some(name) = playwright_config_name(project_root) {
        format!("Playwright config {name}")
    } else if package_depends_on_playwright(project_root)? {
        "package.json depends on @playwright/test".to_string()
    } else {
        return Ok(None);
    };

    Ok(Some(Discovered {
        adapter: "playwright".into(),
        detail,
        suggested_name: "playwright".into(),
        suggested_path: Some(playwright_suggested_path(project_root)),
    }))
}

pub(crate) fn discover_junit(project_root: &Path) -> Result<Option<Discovered>, AdapterError> {
    let Some(path) = junit_report_path(project_root)? else {
        return Ok(None);
    };
    let rel = path.strip_prefix(project_root).unwrap_or(&path);
    Ok(Some(Discovered {
        adapter: "junit".into(),
        detail: format!("JUnit report {}", rel.display()),
        suggested_name: "junit".into(),
        suggested_path: Some(path),
    }))
}

fn agent_stub() -> Discovered {
    Discovered {
        adapter: "agent".into(),
        detail: "MCP/Skill configuration lands in v0.5".into(),
        suggested_name: "agent".into(),
        suggested_path: None,
    }
}

fn playwright_config_name(project_root: &Path) -> Option<&'static str> {
    PLAYWRIGHT_CONFIGS
        .iter()
        .copied()
        .find(|name| project_root.join(name).is_file())
}

fn package_depends_on_playwright(project_root: &Path) -> Result<bool, AdapterError> {
    let Some(bytes) = read_if_file(&project_root.join("package.json"))? else {
        return Ok(false);
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return Ok(false);
    };
    Ok(dep_has_playwright(&value, "dependencies") || dep_has_playwright(&value, "devDependencies"))
}

fn dep_has_playwright(pkg: &Value, key: &str) -> bool {
    pkg.get(key)
        .and_then(Value::as_object)
        .is_some_and(|deps| deps.contains_key("@playwright/test"))
}

fn playwright_suggested_path(project_root: &Path) -> PathBuf {
    let test_results = project_root.join("test-results.json");
    if test_results.is_file() {
        return test_results;
    }
    let report = project_root.join("playwright-report").join("results.json");
    if report.is_file() {
        return report;
    }
    project_root.join("playwright.json")
}

fn junit_report_path(project_root: &Path) -> Result<Option<PathBuf>, AdapterError> {
    let root_junit = project_root.join("junit.xml");
    if root_junit.is_file() {
        return Ok(Some(root_junit));
    }
    if let Some(path) = first_xml_matching(project_root, |name| {
        name.starts_with("TEST-") && name.ends_with(".xml")
    })? {
        return Ok(Some(path));
    }
    if let Some(path) = first_xml_matching(
        &project_root.join("target").join("surefire-reports"),
        |name| name.ends_with(".xml"),
    )? {
        return Ok(Some(path));
    }
    first_xml_matching(&project_root.join("test-results"), |name| {
        name.ends_with(".xml")
    })
}

fn first_xml_matching(
    dir: &Path,
    predicate: impl Fn(&str) -> bool,
) -> Result<Option<PathBuf>, AdapterError> {
    if !dir.is_dir() {
        return Ok(None);
    }
    let mut matches = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if predicate(name) && path.is_file() {
            matches.push(path);
        }
    }
    matches.sort();
    Ok(matches.into_iter().next())
}

fn read_if_file(path: &Path) -> Result<Option<Vec<u8>>, AdapterError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::SystemTime;

    use tempfile::TempDir;

    #[test]
    fn empty_dir_yields_only_agent_stub() {
        let dir = TempDir::new().expect("tempdir");
        let found = discover_integrations(dir.path()).expect("discover");
        assert_eq!(
            found.iter().map(|d| d.adapter.as_str()).collect::<Vec<_>>(),
            ["agent"]
        );
        assert_eq!(found[0].detail, "MCP/Skill configuration lands in v0.5");
        assert_eq!(found[0].suggested_name, "agent");
        assert_eq!(found[0].suggested_path, None);
        assert!(dir.path().read_dir().expect("read_dir").next().is_none());
    }

    #[test]
    fn playwright_config_is_discovered() {
        let dir = TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("playwright.config.ts"), "export default {}")
            .expect("write config");

        let found = discover_integrations(dir.path()).expect("discover");
        assert_eq!(
            found.iter().map(|d| d.adapter.as_str()).collect::<Vec<_>>(),
            ["playwright", "agent"]
        );
        assert_eq!(found[0].suggested_name, "playwright");
        assert_eq!(
            found[0].suggested_path.as_deref(),
            Some(dir.path().join("playwright.json").as_path())
        );
        assert!(found[0].detail.contains("playwright.config.ts"));
    }

    #[test]
    fn junit_xml_is_discovered() {
        let dir = TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("junit.xml"), "<testsuite/>").expect("write junit");

        let found = discover_integrations(dir.path()).expect("discover");
        assert_eq!(
            found.iter().map(|d| d.adapter.as_str()).collect::<Vec<_>>(),
            ["junit", "agent"]
        );
        assert_eq!(found[0].suggested_name, "junit");
        assert_eq!(
            found[0].suggested_path.as_deref(),
            Some(dir.path().join("junit.xml").as_path())
        );
        assert!(found[0].detail.contains("junit.xml"));
    }

    #[test]
    fn discover_does_not_create_or_modify_files() {
        let dir = TempDir::new().expect("tempdir");
        let config = dir.path().join("playwright.config.ts");
        let junit = dir.path().join("junit.xml");
        std::fs::write(&config, "export default {}").expect("write config");
        std::fs::write(&junit, "<testsuite/>").expect("write junit");

        let before = snapshot(dir.path());
        let found = discover_integrations(dir.path()).expect("discover");
        assert_eq!(
            found.iter().map(|d| d.adapter.as_str()).collect::<Vec<_>>(),
            ["playwright", "junit", "agent"]
        );
        assert_eq!(snapshot(dir.path()), before);
        assert!(!dir.path().join("playwright.json").exists());
    }

    #[test]
    fn package_json_playwright_dep_is_discovered() {
        let dir = TempDir::new().expect("tempdir");
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"@playwright/test":"^1.40.0"}}"#,
        )
        .expect("write package.json");

        let found = discover_integrations(dir.path()).expect("discover");
        assert_eq!(found[0].adapter, "playwright");
        assert!(found[0].detail.contains("@playwright/test"));
    }

    fn snapshot(root: &Path) -> BTreeMap<PathBuf, (Vec<u8>, Option<SystemTime>)> {
        let mut out = BTreeMap::new();
        snapshot_walk(root, root, &mut out);
        out
    }

    fn snapshot_walk(
        root: &Path,
        dir: &Path,
        out: &mut BTreeMap<PathBuf, (Vec<u8>, Option<SystemTime>)>,
    ) {
        for entry in fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            let rel = path.strip_prefix(root).expect("prefix").to_path_buf();
            let meta = fs::metadata(&path).expect("metadata");
            let mtime = meta.modified().ok();
            if meta.is_dir() {
                out.insert(rel, (Vec::new(), mtime));
                snapshot_walk(root, &path, out);
            } else {
                out.insert(rel, (fs::read(&path).expect("read"), mtime));
            }
        }
    }
}
