use crate::db::{now_iso8601, Store};
use crate::error::{Result, StoreError};
use meno_core::redaction::contains_credential_shaped;
use rusqlite::params;

/// Persisted adapter connection. `config_json` must not be credential-shaped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionRecord {
    pub id: String,
    pub adapter: String,
    pub name: String,
    pub config_json: String,
    pub can_collect: bool,
    pub can_invoke: bool,
    /// `none` | `filesystem` | `network` | `consequential`
    pub side_effect_level: String,
    pub requires_confirmation: bool,
}

impl Store {
    /// Insert or replace by id. Refuses credential-shaped config_json (existing check).
    pub fn upsert_connection(&self, rec: &ConnectionRecord) -> Result<()> {
        if contains_credential_shaped(rec.config_json.as_bytes()) {
            return Err(StoreError::CredentialInConfig);
        }
        let now = now_iso8601()?;
        self.conn.execute(
            "INSERT INTO connections (
                id, adapter, name, config_json,
                can_collect, can_invoke, side_effect_level, requires_confirmation,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
             ON CONFLICT(id) DO UPDATE SET
                adapter = excluded.adapter,
                name = excluded.name,
                config_json = excluded.config_json,
                can_collect = excluded.can_collect,
                can_invoke = excluded.can_invoke,
                side_effect_level = excluded.side_effect_level,
                requires_confirmation = excluded.requires_confirmation,
                updated_at = excluded.updated_at",
            params![
                rec.id,
                rec.adapter,
                rec.name,
                rec.config_json,
                rec.can_collect,
                rec.can_invoke,
                rec.side_effect_level,
                rec.requires_confirmation,
                now,
            ],
        )?;
        Ok(())
    }

    /// Conservative insert: `can_invoke = false`, `side_effect_level = consequential`,
    /// `requires_confirmation = true`. Name is `{adapter}` unless `id` is `{adapter}:{name}`.
    pub fn insert_connection(&self, id: &str, adapter: &str, config_json: &str) -> Result<()> {
        let name = match id.split_once(':') {
            Some((prefix, rest)) if prefix == adapter && !rest.is_empty() => rest,
            _ => adapter,
        };
        self.upsert_connection(&ConnectionRecord {
            id: id.to_owned(),
            adapter: adapter.to_owned(),
            name: name.to_owned(),
            config_json: config_json.to_owned(),
            can_collect: true,
            can_invoke: false,
            side_effect_level: "consequential".to_owned(),
            requires_confirmation: true,
        })
    }

    pub fn list_connections(&self) -> Result<Vec<ConnectionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, adapter, name, config_json, can_collect, can_invoke,
                    side_effect_level, requires_confirmation
             FROM connections
             ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ConnectionRecord {
                id: row.get(0)?,
                adapter: row.get(1)?,
                name: row.get(2)?,
                config_json: row.get(3)?,
                can_collect: row.get(4)?,
                can_invoke: row.get(5)?,
                side_effect_level: row.get(6)?,
                requires_confirmation: row.get(7)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(StoreError::from)
    }
}
