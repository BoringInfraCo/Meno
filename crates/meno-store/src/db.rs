use crate::error::Result;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

const MIGRATION_001: &str = include_str!("../migrations/001_initial.sql");
const MIGRATION_001_NAME: &str = "001_initial";

/// Current schema version applied by this crate.
pub const CURRENT_SCHEMA_VERSION: i64 = 1;

/// SQLite-backed Meno store.
///
/// Holds a single-threaded [`rusqlite::Connection`] and is therefore `Send` but
/// not `Sync`. Use one `Store` per thread.
pub struct Store {
    pub(crate) conn: Connection,
    pub(crate) artifacts_dir: Option<PathBuf>,
}

/// Open a SQLite database at `path`, applying pending migrations.
pub fn open(path: &Path) -> Result<Store> {
    Store::open(path)
}

impl Store {
    /// Open a SQLite database at `path`, applying pending migrations.
    ///
    /// Artifact `put`/`get`/`verify` require [`Store::open_project`].
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut conn = Connection::open(path)?;
        configure_connection(&conn)?;
        migrate(&mut conn)?;
        Ok(Self {
            conn,
            artifacts_dir: None,
        })
    }

    /// Open the project-local store under `meno_dir` (typically `.meno`).
    ///
    /// Uses `meno_dir/meno.db` and `meno_dir/artifacts/`.
    pub fn open_project(meno_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(meno_dir)?;
        let artifacts_dir = meno_dir.join("artifacts");
        std::fs::create_dir_all(&artifacts_dir)?;
        let mut store = Self::open(&meno_dir.join("meno.db"))?;
        store.artifacts_dir = Some(artifacts_dir);
        Ok(store)
    }

    /// Highest applied schema migration version, or `0` if none.
    pub fn schema_version(&self) -> Result<i64> {
        schema_version(&self.conn)
    }

    /// Append an audit event. There is no update or delete API.
    pub fn append_audit_event(
        &self,
        action: &str,
        actor: Option<&str>,
        entity_kind: Option<&str>,
        entity_id: Option<&str>,
        payload_json: Option<&str>,
    ) -> Result<()> {
        insert_audit_event(
            &self.conn,
            action,
            actor,
            entity_kind,
            entity_id,
            payload_json,
        )
    }
}

pub(crate) fn insert_audit_event(
    conn: &Connection,
    action: &str,
    actor: Option<&str>,
    entity_kind: Option<&str>,
    entity_id: Option<&str>,
    payload_json: Option<&str>,
) -> Result<()> {
    let at = now_iso8601()?;
    conn.execute(
        "INSERT INTO audit_events (at, actor, action, entity_kind, entity_id, payload_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![at, actor, action, entity_kind, entity_id, payload_json],
    )?;
    Ok(())
}

pub(crate) fn now_iso8601() -> Result<String> {
    Ok(OffsetDateTime::now_utc().format(&Rfc3339)?)
}

fn configure_connection(conn: &Connection) -> Result<()> {
    let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

fn schema_version(conn: &Connection) -> Result<i64> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(0);
    }
    let version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    Ok(version)
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;
    if schema_version(&tx)? < CURRENT_SCHEMA_VERSION {
        tx.execute_batch(MIGRATION_001)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
            params![CURRENT_SCHEMA_VERSION, MIGRATION_001_NAME, now_iso8601()?],
        )?;
    }
    tx.commit()?;
    Ok(())
}
