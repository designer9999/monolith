//! Storage layer: a plain bundled SQLite database (Stage 1).
//!
//! Whole-database encryption (SQLCipher) is a planned Stage 2; today every
//! *secret value* and *TOTP seed* is encrypted at the field level with the vault
//! key, so the most sensitive data is protected at rest even though metadata
//! (project names, labels) is stored in clear text for a fast, searchable UI.

pub mod repo;

use std::path::Path;

use rusqlite::Connection;
use uuid::Uuid;

use crate::error::AppResult;

/// Current schema version, stored in `vault_meta` and used by migrations.
pub const SCHEMA_VERSION: i64 = 2;

/// Open (creating if missing) the SQLite database at `path` and ensure the
/// schema is present.
pub fn open(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Open an in-memory database (used by tests).
#[cfg(test)]
pub fn open_in_memory() -> AppResult<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Create all tables if they don't exist. Single migration for schema v1.
fn migrate(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(SCHEMA_V1)?;
    ensure_column(conn, "vault_meta", "vault_id", "TEXT")?;
    ensure_column(conn, "services", "expires_at", "TEXT")?;
    for table in ["projects", "services", "secret_fields", "password_history"] {
        ensure_column(conn, table, "revision", "INTEGER NOT NULL DEFAULT 0")?;
        ensure_column(conn, table, "deleted_at", "TEXT")?;
        ensure_column(conn, table, "last_modified_device_id", "TEXT")?;
    }
    conn.execute_batch(SCHEMA_V2)?;
    ensure_vault_id(conn)?;
    Ok(())
}

fn ensure_vault_id(conn: &Connection) -> AppResult<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM vault_meta", [], |r| r.get(0))?;
    if count == 0 {
        return Ok(());
    }
    let existing: Option<String> =
        conn.query_row("SELECT vault_id FROM vault_meta WHERE id = 1", [], |r| {
            r.get(0)
        })?;
    if existing.as_deref().unwrap_or("").trim().is_empty() {
        conn.execute(
            "UPDATE vault_meta SET vault_id = ?1, schema_version = ?2 WHERE id = 1",
            (format!("v_{}", Uuid::new_v4()), SCHEMA_VERSION),
        )?;
    }
    Ok(())
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> AppResult<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(());
        }
    }
    conn.execute_batch(&format!(
        "ALTER TABLE {table} ADD COLUMN {column} {definition}"
    ))?;
    Ok(())
}

/// Whether a vault has been initialized (a header row exists).
pub fn is_initialized(conn: &Connection) -> AppResult<bool> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM vault_meta", [], |r| r.get(0))?;
    Ok(n > 0)
}

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS vault_meta (
  id                  INTEGER PRIMARY KEY CHECK (id = 1),
  schema_version      INTEGER NOT NULL,
  vault_id            TEXT,
  kdf_params          TEXT    NOT NULL,
  kdf_salt            BLOB    NOT NULL,
  vault_key_nonce     BLOB    NOT NULL,
  encrypted_vault_key BLOB    NOT NULL,
  created_at          TEXT    NOT NULL,
  updated_at          TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS projects (
  id          TEXT PRIMARY KEY,
  name        TEXT    NOT NULL,
  sub         TEXT    NOT NULL DEFAULT '',
  mono        TEXT    NOT NULL,
  color       TEXT    NOT NULL,
  icon_json   TEXT,
  sort_index  INTEGER NOT NULL,
  created_at  TEXT    NOT NULL,
  updated_at  TEXT    NOT NULL,
  revision    INTEGER NOT NULL DEFAULT 0,
  deleted_at  TEXT,
  last_modified_device_id TEXT
);

CREATE TABLE IF NOT EXISTS services (
  id           TEXT PRIMARY KEY,
  project_id   TEXT    NOT NULL,
  template_id  TEXT    NOT NULL,
  label        TEXT    NOT NULL DEFAULT '',
  env          TEXT    NOT NULL DEFAULT 'all',
  expires_at   TEXT,
  sort_index   INTEGER NOT NULL,
  fav          INTEGER NOT NULL DEFAULT 0,
  reused       INTEGER NOT NULL DEFAULT 0,
  exposed      INTEGER NOT NULL DEFAULT 0,
  -- min password-like strength (0-100), computed at write time so list/view
  -- rendering never has to decrypt secret values. NULL when no scorable field.
  strength     INTEGER,
  totp_nonce   BLOB,
  totp_cipher  BLOB,
  created_at   TEXT    NOT NULL,
  updated_at   TEXT    NOT NULL,
  revision     INTEGER NOT NULL DEFAULT 0,
  deleted_at   TEXT,
  last_modified_device_id TEXT,
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS secret_fields (
  id          TEXT PRIMARY KEY,
  service_id  TEXT    NOT NULL,
  label       TEXT    NOT NULL,
  field_type  TEXT    NOT NULL DEFAULT 'text',
  is_secret   INTEGER NOT NULL DEFAULT 1,
  is_danger   INTEGER NOT NULL DEFAULT 0,
  is_area     INTEGER NOT NULL DEFAULT 0,
  sort_index  INTEGER NOT NULL,
  -- non-secret values are stored in clear text (the UI shows them anyway);
  -- secret values are stored encrypted in (nonce, cipher) with plaintext NULL.
  plain_value TEXT,
  nonce       BLOB,
  cipher      BLOB,
  updated_at  TEXT    NOT NULL,
  revision    INTEGER NOT NULL DEFAULT 0,
  deleted_at  TEXT,
  last_modified_device_id TEXT,
  FOREIGN KEY (service_id) REFERENCES services(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS attachments (
  id          TEXT PRIMARY KEY,
  project_id  TEXT    NOT NULL,
  name        TEXT    NOT NULL,
  size        TEXT    NOT NULL,
  created_at  TEXT    NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS activity (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  at          TEXT NOT NULL,
  action      TEXT NOT NULL,
  target      TEXT NOT NULL,
  kind        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS password_history (
  id          TEXT PRIMARY KEY,
  field_id    TEXT    NOT NULL,
  service_id  TEXT    NOT NULL,
  label       TEXT    NOT NULL,
  nonce       BLOB    NOT NULL,
  cipher      BLOB    NOT NULL,
  created_at  TEXT    NOT NULL,
  revision    INTEGER NOT NULL DEFAULT 0,
  deleted_at  TEXT,
  last_modified_device_id TEXT,
  FOREIGN KEY (field_id) REFERENCES secret_fields(id) ON DELETE CASCADE,
  FOREIGN KEY (service_id) REFERENCES services(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_services_project ON services(project_id);
CREATE INDEX IF NOT EXISTS idx_fields_service ON secret_fields(service_id);
CREATE INDEX IF NOT EXISTS idx_attachments_project ON attachments(project_id);
CREATE INDEX IF NOT EXISTS idx_password_history_service ON password_history(service_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_password_history_field ON password_history(field_id, created_at DESC);
"#;

const SCHEMA_V2: &str = r#"
CREATE TABLE IF NOT EXISTS app_settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS devices (
  id           TEXT PRIMARY KEY,
  name         TEXT    NOT NULL,
  platform     TEXT    NOT NULL,
  public_key   TEXT    NOT NULL,
  trusted      INTEGER NOT NULL DEFAULT 1,
  revoked_at   TEXT,
  created_at   TEXT    NOT NULL,
  last_seen_at TEXT
);

CREATE TABLE IF NOT EXISTS device_unlocks (
  id                    TEXT PRIMARY KEY CHECK (id = 'local'),
  device_id             TEXT    NOT NULL,
  device_key_nonce      BLOB    NOT NULL,
  encrypted_vault_key   BLOB    NOT NULL,
  created_at            TEXT    NOT NULL,
  updated_at            TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_devices_trusted ON devices(trusted, revoked_at);
"#;
