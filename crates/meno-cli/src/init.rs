use std::fs;
use std::path::Path;

use meno_adapters::collect_snapshot;
use meno_core::{subject_id_hex, ClaimDocument};
use meno_store::Store;

use crate::error::CliError;
use crate::project::{git_toplevel, require_git_work_tree, short_subject_id};

pub const DEFAULT_MENO_TOML: &str = r#"[meno]
version = 1
subject_identity_version = 1

[[adapters.command]]
name = "unit"
argv = ["true"]
can_invoke = true
side_effect_level = "none"
"#;

pub const EXAMPLE_CLAIM_YAML: &str = r#"id: C-example
statement: "The configured unit command exits 0."
state: frozen
origin:
  kind: human
policy:
  version: 1
  requires:
    - kind: command.result
      match:
        exit_code: 0
      min_count: 1
      subject_bound: true
  contradicted_by: []
  freshness:
    subject_match: exact
"#;

pub fn run() -> Result<(), CliError> {
    let cwd = std::env::current_dir()?;
    require_git_work_tree(&cwd)?;
    let root = git_toplevel(&cwd)?;

    let toml_path = root.join("meno.toml");
    if !toml_path.is_file() {
        fs::write(&toml_path, DEFAULT_MENO_TOML)?;
        println!("wrote {}", display_rel(&root, &toml_path));
    }

    let claims_dir = root.join("claims");
    fs::create_dir_all(&claims_dir)?;
    if !has_yaml_claim(&claims_dir) {
        let example = claims_dir.join("C-example.yaml");
        ClaimDocument::from_yaml(EXAMPLE_CLAIM_YAML)?;
        fs::write(&example, EXAMPLE_CLAIM_YAML)?;
        println!("wrote {}", display_rel(&root, &example));
    }

    ensure_gitignore(&root)?;

    let store = Store::open_project(&root.join(".meno"))?;
    store.sync_contracts(&root)?;
    store.append_audit_event("project.init", Some("meno"), Some("project"), None, None)?;

    let snapshot = collect_snapshot(&root)?;
    let subject_id = subject_id_hex(&snapshot);
    println!("initialized Meno in {}", root.display());
    println!("subject {}", short_subject_id(&subject_id));
    println!("next: edit claims/ or run `meno verify`");
    Ok(())
}

fn has_yaml_claim(claims_dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(claims_dir) else {
        return false;
    };
    entries.filter_map(|e| e.ok()).any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        name.ends_with(".yaml") || name.ends_with(".yml")
    })
}

fn ensure_gitignore(root: &Path) -> Result<(), CliError> {
    let path = root.join(".gitignore");
    if path.is_file() {
        let contents = fs::read_to_string(&path)?;
        let present = contents
            .lines()
            .any(|line| matches!(line.trim(), ".meno/" | ".meno" | "/.meno/" | "/.meno"));
        if !present {
            let mut contents = contents;
            if !contents.is_empty() && !contents.ends_with('\n') {
                contents.push('\n');
            }
            contents.push_str(".meno/\n");
            fs::write(&path, contents)?;
        }
    } else {
        fs::write(&path, ".meno/\n")?;
    }
    Ok(())
}

fn display_rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}
