use std::fs;
use std::path::{Path, PathBuf};

use meno_adapters::{discover_integrations, Discovered, SideEffectLevel};
use meno_core::redaction::contains_credential_shaped;
use meno_store::{ConnectionRecord, Store};
use serde_json::{json, Value};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

use crate::config::{parse_side_effect_level, side_effect_slug};
use crate::error::CliError;
use crate::project::find_project_root;

pub struct ConnectArgs {
    pub adapter: Option<String>,
    pub name: Option<String>,
    pub path: Option<PathBuf>,
    pub argv: Vec<String>,
    pub can_invoke: bool,
    pub side_effect_level: Option<String>,
    pub replace: bool,
    pub stdio: bool,
    pub write: bool,
    pub harness: Option<String>,
}

pub fn run(args: ConnectArgs) -> Result<(), CliError> {
    if args.stdio && args.write {
        return Err(CliError::msg("use either --stdio or --write, not both"));
    }

    match args
        .adapter
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => {
            if args.stdio || args.write || args.harness.is_some() {
                return Err(CliError::msg(
                    "--stdio, --write, and --harness require --adapter agent",
                ));
            }
            print_catalog()
        }
        Some(adapter) => {
            let kind = normalize_adapter(adapter)?;
            if kind == "agent" {
                connect_agent(
                    args.stdio,
                    args.write,
                    args.harness.as_deref(),
                    args.replace,
                )
            } else {
                if args.stdio || args.write || args.harness.is_some() {
                    return Err(CliError::msg(
                        "--stdio, --write, and --harness require --adapter agent",
                    ));
                }
                connect_adapter(
                    kind,
                    args.name,
                    args.path,
                    args.argv,
                    args.can_invoke,
                    args.side_effect_level,
                    args.replace,
                )
            }
        }
    }
}

fn print_catalog() -> Result<(), CliError> {
    let root = connect_root()?;
    let detected = discover_integrations(&root)?;

    println!("Verification tools");
    println!("  command     Generic command (argv)");
    println!("  junit       JUnit XML report");
    println!("  playwright  Playwright JSON reporter");
    println!();
    println!("Agents");
    println!("  agent       MCP stdio + Skill (meno connect --adapter agent)");
    println!();
    println!("Detected");
    let mcp = detect_mcp_configs(&root);
    let tools: Vec<_> = detected
        .iter()
        .filter(|item| item.adapter != "agent")
        .collect();
    if tools.is_empty() && mcp.is_empty() {
        println!("  (none)");
    } else {
        for item in tools {
            print_detected(&root, item);
        }
        for path in mcp {
            println!("  {:<11} {}", "agent", rel_display(&root, &path));
        }
    }
    println!();
    println!("Connect with:");
    println!(
        "  meno connect --adapter command --name <name> --argv <cmd...> [--can-invoke] [--side-effect-level none|filesystem|network|consequential]"
    );
    println!("  meno connect --adapter junit --name <name> --path <junit.xml>");
    println!("  meno connect --adapter playwright --name <name> --path <playwright.json>");
    println!("  meno connect --adapter agent [--stdio|--write] [--harness generic|claude]");
    println!("  side_effect_level defaults to consequential");
    Ok(())
}

fn print_detected(root: &Path, item: &Discovered) {
    println!("  {:<11} {}", item.adapter, item.detail);
    match item.adapter.as_str() {
        "agent" => {}
        "command" => {
            println!(
                "              suggested: --adapter command --name {}",
                item.suggested_name
            );
        }
        kind => {
            let path = item
                .suggested_path
                .as_ref()
                .map(|p| rel_display(root, p))
                .unwrap_or_else(|| "<path>".into());
            println!(
                "              suggested: --adapter {kind} --name {} --path {path}",
                item.suggested_name
            );
        }
    }
}

fn connect_agent(
    stdio: bool,
    write: bool,
    harness: Option<&str>,
    replace: bool,
) -> Result<(), CliError> {
    if stdio {
        return run_agent_stdio();
    }
    if write {
        return write_agent_integration(harness, replace);
    }
    parse_harness(harness)?;
    print_agent_usage()
}

fn print_agent_usage() -> Result<(), CliError> {
    let root = connect_root()?;
    println!("Detected");
    let detected = detect_mcp_configs(&root);
    if detected.is_empty() {
        println!("  (none)");
    } else {
        for path in detected {
            println!("  {}", rel_display(&root, &path));
        }
    }
    println!();
    println!("Connect agent with:");
    println!("  --stdio    serve MCP on stdin/stdout");
    println!("  --write    write project .mcp.json + install skill after explicit flag");
    println!("  --harness  generic|claude (default generic)");
    Ok(())
}

fn detect_mcp_configs(root: &Path) -> Vec<PathBuf> {
    [".mcp.json", ".claude/mcp.json", "mcp.json"]
        .into_iter()
        .map(|rel| root.join(rel))
        .filter(|path| path.is_file())
        .collect()
}

fn run_agent_stdio() -> Result<(), CliError> {
    let cwd = std::env::current_dir()?;
    let root = find_project_root(&cwd)?;
    serve_stdio(&root)
}

fn serve_stdio(root: &Path) -> Result<(), CliError> {
    meno_mcp::serve_stdio(root).map_err(|err| CliError::msg(err.to_string()))
}

fn write_agent_integration(harness: Option<&str>, replace: bool) -> Result<(), CliError> {
    parse_harness(harness)?;
    let root = connect_root()?;
    let mcp_path = root.join(".mcp.json");
    if mcp_path.exists() && !replace {
        return Err(CliError::msg(
            ".mcp.json already exists; pass --replace to overwrite",
        ));
    }

    let mut rendered = serde_json::to_string_pretty(&agent_mcp_config())?;
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    let skill = load_skill_bytes(&root)?;
    if contains_credential_shaped(rendered.as_bytes()) || contains_credential_shaped(&skill) {
        return Err(CliError::msg(
            "refusing credential-shaped value in connection config; secrets do not belong in .mcp.json or SKILL.md",
        ));
    }

    let mut written = vec![mcp_path.clone()];
    fs::write(&mcp_path, rendered)?;

    for dest in skill_destinations(&root) {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, &skill)?;
        written.push(dest);
    }

    for path in written {
        println!("wrote {}", rel_display(&root, &path));
    }
    Ok(())
}

fn parse_harness(raw: Option<&str>) -> Result<&'static str, CliError> {
    let owned = raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("generic")
        .to_ascii_lowercase();
    match owned.as_str() {
        "generic" => Ok("generic"),
        "claude" => Ok("claude"),
        other => Err(CliError::msg(format!(
            "unknown harness `{other}`; expected generic or claude"
        ))),
    }
}

fn agent_mcp_config() -> Value {
    json!({
        "mcpServers": {
            "meno": {
                "command": "meno",
                "args": ["connect", "--adapter", "agent", "--stdio"]
            }
        }
    })
}

fn skill_destinations(root: &Path) -> [PathBuf; 2] {
    [
        root.join(".agents/skills/meno/SKILL.md"),
        root.join("skills/meno/SKILL.md"),
    ]
}

const SKILL_PLACEHOLDER: &str = "\
# Meno

Inspect Meno before declaring work complete.
Never call UNKNOWN PROVEN.
Never weaken frozen claims.
";

fn load_skill_bytes(root: &Path) -> Result<Vec<u8>, CliError> {
    match skill_source(root) {
        Some(path) => Ok(fs::read(path)?),
        None => Ok(SKILL_PLACEHOLDER.as_bytes().to_vec()),
    }
}

fn skill_source(root: &Path) -> Option<PathBuf> {
    let rel = Path::new("skills").join("meno").join("SKILL.md");
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(Path::to_path_buf);
        while let Some(current) = dir {
            let candidate = current.join(&rel);
            if candidate.is_file() {
                return Some(candidate);
            }
            dir = current.parent().map(Path::to_path_buf);
        }
    }
    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        let candidate = Path::new(manifest).join("../../skills/meno/SKILL.md");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let in_root = root.join(&rel);
    in_root.is_file().then_some(in_root)
}

fn connect_adapter(
    kind: &str,
    name: Option<String>,
    path: Option<PathBuf>,
    argv: Vec<String>,
    can_invoke: bool,
    side_effect_level: Option<String>,
    replace: bool,
) -> Result<(), CliError> {
    let name = require_name(name)?;
    let side_effect = match side_effect_level {
        Some(raw) => parse_side_effect_level(&raw)?,
        None => SideEffectLevel::Consequential,
    };

    let entry = match kind {
        "command" => {
            if argv.is_empty() {
                return Err(CliError::msg(
                    "command adapter requires --argv <program> [args...]",
                ));
            }
            AdapterEntry::Command {
                name,
                argv,
                can_invoke,
                side_effect,
            }
        }
        "junit" => AdapterEntry::Junit {
            name,
            path: require_path(path, "junit")?,
        },
        "playwright" => AdapterEntry::Playwright {
            name,
            path: require_path(path, "playwright")?,
        },
        other => {
            return Err(CliError::msg(format!(
                "unknown adapter `{other}`; expected command, junit, playwright, or agent"
            )))
        }
    };

    refuse_secrets(&entry)?;

    let root = connect_root()?;
    let toml_path = root.join("meno.toml");
    if !toml_path.is_file() {
        return Err(CliError::msg(format!(
            "no meno.toml at {}; run `meno init`",
            toml_path.display()
        )));
    }

    let original = fs::read_to_string(&toml_path)?;
    let mut doc = original.parse::<DocumentMut>()?;
    let existed = adapter_name_exists(&doc, entry.kind(), entry.name());
    if existed && !replace {
        return Err(CliError::msg(format!(
            "{} adapter `{}` already exists; pass --replace to overwrite",
            entry.kind(),
            entry.name()
        )));
    }

    upsert_toml_entry(&mut doc, &entry, existed)?;
    let rendered = doc.to_string();
    if contains_credential_shaped(rendered.as_bytes()) {
        return Err(CliError::msg(
            "refusing credential-shaped value in connection config; secrets do not belong in meno.toml",
        ));
    }
    fs::write(&toml_path, rendered)?;

    persist_connection(&root, &entry)?;
    println!("connected {} adapter `{}`", entry.kind(), entry.name());
    Ok(())
}

enum AdapterEntry {
    Command {
        name: String,
        argv: Vec<String>,
        can_invoke: bool,
        side_effect: SideEffectLevel,
    },
    Junit {
        name: String,
        path: PathBuf,
    },
    Playwright {
        name: String,
        path: PathBuf,
    },
}

impl AdapterEntry {
    fn kind(&self) -> &'static str {
        match self {
            Self::Command { .. } => "command",
            Self::Junit { .. } => "junit",
            Self::Playwright { .. } => "playwright",
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Command { name, .. }
            | Self::Junit { name, .. }
            | Self::Playwright { name, .. } => name,
        }
    }

    fn config_json(&self) -> Result<String, CliError> {
        Ok(serde_json::to_string(&self.config_value())?)
    }

    fn config_value(&self) -> Value {
        match self {
            Self::Command {
                name,
                argv,
                can_invoke,
                side_effect,
            } => json!({
                "name": name,
                "argv": argv,
                "can_invoke": can_invoke,
                "side_effect_level": side_effect_slug(*side_effect),
            }),
            Self::Junit { name, path } => json!({
                "name": name,
                "path": path_string(path),
            }),
            Self::Playwright { name, path } => json!({
                "name": name,
                "path": path_string(path),
            }),
        }
    }

    fn can_invoke(&self) -> bool {
        match self {
            Self::Command { can_invoke, .. } => *can_invoke,
            Self::Junit { .. } | Self::Playwright { .. } => false,
        }
    }

    fn side_effect_level(&self) -> &'static str {
        match self {
            Self::Command { side_effect, .. } => side_effect_slug(*side_effect),
            Self::Junit { .. } | Self::Playwright { .. } => "none",
        }
    }

    fn requires_confirmation(&self) -> bool {
        match self {
            Self::Command { side_effect, .. } => *side_effect != SideEffectLevel::None,
            Self::Junit { .. } | Self::Playwright { .. } => false,
        }
    }
}

fn persist_connection(root: &Path, entry: &AdapterEntry) -> Result<(), CliError> {
    let store = Store::open_project(&root.join(".meno"))?;
    let config_json = entry.config_json()?;
    if contains_credential_shaped(config_json.as_bytes()) {
        return Err(CliError::msg(
            "refusing credential-shaped value in connection config; secrets do not belong in meno.toml",
        ));
    }
    let rec = ConnectionRecord {
        id: format!("{}:{}", entry.kind(), entry.name()),
        adapter: entry.kind().to_string(),
        name: entry.name().to_string(),
        config_json,
        can_collect: true,
        can_invoke: entry.can_invoke(),
        side_effect_level: entry.side_effect_level().to_string(),
        requires_confirmation: entry.requires_confirmation(),
    };
    store.upsert_connection(&rec)?;
    Ok(())
}

fn upsert_toml_entry(
    doc: &mut DocumentMut,
    entry: &AdapterEntry,
    existed: bool,
) -> Result<(), CliError> {
    let kind = entry.kind();
    let name = entry.name();
    let tables = adapter_tables(doc, kind)?;
    if existed {
        let mut updated = false;
        for table in tables.iter_mut() {
            if table_name(table) == Some(name) {
                write_entry(table, entry);
                updated = true;
            }
        }
        if !updated {
            tables.push(entry_table(entry));
        }
    } else {
        tables.push(entry_table(entry));
    }
    Ok(())
}

fn adapter_tables<'a>(
    doc: &'a mut DocumentMut,
    kind: &str,
) -> Result<&'a mut ArrayOfTables, CliError> {
    if doc.get("adapters").is_none() {
        let mut adapters = Table::new();
        adapters.set_implicit(true);
        doc["adapters"] = Item::Table(adapters);
    }
    let adapters = doc
        .get_mut("adapters")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| CliError::msg("meno.toml `adapters` must be a table"))?;
    adapters.set_implicit(true);
    if adapters.get(kind).is_none() {
        adapters.insert(kind, Item::ArrayOfTables(ArrayOfTables::new()));
    }
    adapters
        .get_mut(kind)
        .and_then(Item::as_array_of_tables_mut)
        .ok_or_else(|| {
            CliError::msg(format!(
                "meno.toml adapters.{kind} must be an array of tables"
            ))
        })
}

fn adapter_name_exists(doc: &DocumentMut, kind: &str, name: &str) -> bool {
    doc.get("adapters")
        .and_then(Item::as_table)
        .and_then(|adapters| adapters.get(kind))
        .and_then(Item::as_array_of_tables)
        .is_some_and(|tables| tables.iter().any(|table| table_name(table) == Some(name)))
}

fn table_name(table: &Table) -> Option<&str> {
    table.get("name").and_then(Item::as_str)
}

fn entry_table(entry: &AdapterEntry) -> Table {
    let mut table = Table::new();
    table.decor_mut().set_prefix("\n");
    write_entry(&mut table, entry);
    table
}

fn write_entry(table: &mut Table, entry: &AdapterEntry) {
    match entry {
        AdapterEntry::Command {
            name,
            argv,
            can_invoke,
            side_effect,
        } => {
            table["name"] = toml_edit::value(name.as_str());
            let mut arr = toml_edit::Array::new();
            for arg in argv {
                arr.push(arg.as_str());
            }
            table["argv"] = toml_edit::value(arr);
            table["can_invoke"] = toml_edit::value(*can_invoke);
            table["side_effect_level"] = toml_edit::value(side_effect_slug(*side_effect));
        }
        AdapterEntry::Junit { name, path } => {
            table["name"] = toml_edit::value(name.as_str());
            table["path"] = toml_edit::value(path_string(path));
        }
        AdapterEntry::Playwright { name, path } => {
            table["name"] = toml_edit::value(name.as_str());
            table["path"] = toml_edit::value(path_string(path));
        }
    }
}

fn refuse_secrets(entry: &AdapterEntry) -> Result<(), CliError> {
    let mut parts: Vec<String> = vec![entry.name().to_string()];
    match entry {
        AdapterEntry::Command { argv, .. } => parts.extend(argv.iter().cloned()),
        AdapterEntry::Junit { path, .. } | AdapterEntry::Playwright { path, .. } => {
            parts.push(path_string(path));
        }
    }
    parts.push(entry.config_json()?);
    for part in &parts {
        if contains_credential_shaped(part.as_bytes()) {
            return Err(CliError::msg(
                "refusing credential-shaped value in connection config; secrets do not belong in meno.toml",
            ));
        }
    }
    Ok(())
}

fn normalize_adapter(adapter: &str) -> Result<&'static str, CliError> {
    match adapter.trim().to_ascii_lowercase().as_str() {
        "command" => Ok("command"),
        "junit" => Ok("junit"),
        "playwright" => Ok("playwright"),
        "agent" => Ok("agent"),
        other => Err(CliError::msg(format!(
            "unknown adapter `{other}`; expected command, junit, playwright, or agent"
        ))),
    }
}

fn require_name(name: Option<String>) -> Result<String, CliError> {
    let name = name.unwrap_or_default();
    let name = name.trim();
    if name.is_empty() {
        return Err(CliError::msg("--name is required"));
    }
    Ok(name.to_string())
}

fn require_path(path: Option<PathBuf>, adapter: &str) -> Result<PathBuf, CliError> {
    path.filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| CliError::msg(format!("{adapter} adapter requires --path <file>")))
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn connect_root() -> Result<PathBuf, CliError> {
    let cwd = std::env::current_dir()?;
    Ok(find_project_root(&cwd).unwrap_or(cwd))
}

fn rel_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}
