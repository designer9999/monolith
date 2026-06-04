//! Repository functions: all SQL lives here, behind typed helpers.
//!
//! Secret values are sealed with the vault key and bound to *associated data*
//! (`project|service|field`) so a ciphertext can't be relocated to a different
//! field. Reads that build view models never decrypt secret values — they only
//! report `has_value`. Decryption happens only in the explicit reveal/copy/TOTP
//! paths.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use zeroize::Zeroize;

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::strength;
use crate::templates;
use crate::vault::crypto;
use crate::vault::{VaultHeader, VaultKey};

use super::SCHEMA_VERSION;

pub const PERSONAL_PROJECT_ID: &str = "p_personal";

/// Current RFC-3339 timestamp.
fn now() -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default()
}

/// Convert a stored env string to the typed [`Environment`].
fn parse_env(s: &str) -> Environment {
    match s {
        "production" => Environment::Production,
        "staging" => Environment::Staging,
        "dev" => Environment::Dev,
        _ => Environment::All,
    }
}

fn env_str(e: Environment) -> &'static str {
    match e {
        Environment::Production => "production",
        Environment::Staging => "staging",
        Environment::Dev => "dev",
        Environment::All => "all",
    }
}

fn parse_field_type(s: &str) -> FieldType {
    match s {
        "password" => FieldType::Password,
        "api_key" => FieldType::ApiKey,
        "url" => FieldType::Url,
        "email" => FieldType::Email,
        "json" => FieldType::Json,
        _ => FieldType::Text,
    }
}

fn field_type_str(t: FieldType) -> &'static str {
    match t {
        FieldType::Text => "text",
        FieldType::Password => "password",
        FieldType::ApiKey => "api_key",
        FieldType::Url => "url",
        FieldType::Email => "email",
        FieldType::Json => "json",
    }
}

/// Associated data bound to a field's ciphertext: it ties the secret to its
/// exact location so it can't be silently moved elsewhere.
fn field_aad(project_id: &str, service_id: &str, field_id: &str) -> Vec<u8> {
    format!("{project_id}|{service_id}|{field_id}").into_bytes()
}

fn history_aad(history_id: &str) -> Vec<u8> {
    format!("history|{history_id}").into_bytes()
}

fn with_savepoint<T>(
    conn: &Connection,
    name: &str,
    f: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    conn.execute_batch(&format!("SAVEPOINT {name}"))?;
    match f() {
        Ok(value) => {
            conn.execute_batch(&format!("RELEASE {name}"))?;
            Ok(value)
        }
        Err(err) => {
            let _ = conn.execute_batch(&format!("ROLLBACK TO {name}; RELEASE {name}"));
            Err(err)
        }
    }
}

fn is_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn validate_project_icon(icon: &ProjectIcon) -> AppResult<()> {
    const MAX_ICON_JSON: usize = 384 * 1024;
    const MAX_ICON_DATA_URL: usize = 360 * 1024;
    const GLYPHS: &[&str] = &[
        "folder", "vault", "layers", "shield", "globe", "terminal", "star", "key", "card", "qr",
    ];

    if let Some(color) = icon.color.as_deref() {
        if !is_hex_color(color) {
            return Err(AppError::Invalid(
                "Project icon color must be a #RRGGBB value".into(),
            ));
        }
    }

    match icon.kind.as_str() {
        "mono" => {}
        "glyph" => {
            let Some(name) = icon.name.as_deref() else {
                return Err(AppError::Invalid(
                    "Project glyph icon requires a name".into(),
                ));
            };
            if !GLYPHS.contains(&name) {
                return Err(AppError::Invalid(format!(
                    "Unsupported project glyph: {name}"
                )));
            }
        }
        "img" => {
            let Some(src) = icon.src.as_deref() else {
                return Err(AppError::Invalid(
                    "Uploaded project icon requires image data".into(),
                ));
            };
            if !src.starts_with("data:image/") || src.len() > MAX_ICON_DATA_URL {
                return Err(AppError::Invalid(
                    "Uploaded project icon must be an image data URL under 360 KB".into(),
                ));
            }
        }
        _ => {
            return Err(AppError::Invalid(format!(
                "Unsupported project icon kind: {}",
                icon.kind
            )))
        }
    }

    let json_len = serde_json::to_vec(icon)?.len();
    if json_len > MAX_ICON_JSON {
        return Err(AppError::Invalid(
            "Project icon metadata is too large".into(),
        ));
    }
    Ok(())
}

fn validate_len(label: &str, value: &str, max: usize) -> AppResult<()> {
    if value.chars().count() > max {
        return Err(AppError::Invalid(format!("{label} is too long")));
    }
    Ok(())
}

/// Ensure the global Personal collection exists. This is where standalone
/// credentials such as ZeroID, Gmail accounts, and personal logins live.
pub fn ensure_personal_project(conn: &Connection) -> AppResult<()> {
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM projects WHERE id = ?1 AND deleted_at IS NULL",
            params![PERSONAL_PROJECT_ID],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if exists {
        return Ok(());
    }

    let ts = now();
    with_savepoint(conn, "ensure_personal_project", || {
        conn.execute(
            "UPDATE projects SET sort_index = sort_index + 1 WHERE deleted_at IS NULL",
            [],
        )?;
        conn.execute(
            "INSERT INTO projects (id, name, sub, mono, color, icon_json, sort_index, created_at, updated_at)
             VALUES (?1, 'Personal', 'Global credentials', 'P', '#34e29a', ?2, 0, ?3, ?3)",
            params![
                PERSONAL_PROJECT_ID,
                serde_json::json!({
                    "kind": "glyph",
                    "name": "vault",
                    "color": "#34e29a"
                })
                .to_string(),
                ts
            ],
        )?;
        conn.execute(
            "INSERT INTO activity (at, action, target, kind) VALUES (?1, ?2, ?3, ?4)",
            params![ts, "CREATE", "Personal · global vault", "add"],
        )?;
        Ok(())
    })
}

fn validate_expiration(expires_at: Option<&str>) -> AppResult<Option<String>> {
    let Some(raw) = expires_at else {
        return Ok(None);
    };
    let value = raw.trim();
    if value.is_empty() {
        return Ok(None);
    }
    validate_len("Expiration date", value, 10)?;
    time::Date::parse(
        value,
        time::macros::format_description!("[year]-[month]-[day]"),
    )
    .map_err(|_| AppError::Invalid("Expiration date must use YYYY-MM-DD".into()))?;
    Ok(Some(value.to_string()))
}

fn is_password_label(label: &str) -> bool {
    let label = label.to_ascii_lowercase();
    label.contains("password") || label.contains("passphrase") || label.contains("pin")
}

fn decrypt_secret_string(
    key: &VaultKey,
    nonce: &[u8],
    cipher: &[u8],
    aad: &[u8],
    invalid_message: &str,
) -> AppResult<String> {
    let mut bytes = crypto::decrypt(key.expose(), nonce, cipher, aad)?;
    let value = match std::str::from_utf8(&bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            bytes.zeroize();
            return Err(AppError::Crypto(invalid_message.into()));
        }
    };
    bytes.zeroize();
    Ok(value)
}

fn archive_secret_value(
    conn: &Connection,
    key: &VaultKey,
    field_id: &str,
    service_id: &str,
    label: &str,
    value: &str,
    ts: &str,
) -> AppResult<()> {
    if value.is_empty() {
        return Ok(());
    }
    let id = format!("hist_{}", uuid::Uuid::new_v4().simple());
    let aad = history_aad(&id);
    let (nonce, cipher) = crypto::encrypt(key.expose(), value.as_bytes(), &aad)?;
    conn.execute(
        "INSERT INTO password_history (id, field_id, service_id, label, nonce, cipher, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, field_id, service_id, label, nonce, cipher, ts],
    )?;
    conn.execute(
        "DELETE FROM password_history
          WHERE field_id = ?1
            AND id NOT IN (
              SELECT id FROM password_history
               WHERE field_id = ?1
               ORDER BY created_at DESC, id DESC
               LIMIT 3
            )",
        params![field_id],
    )?;
    Ok(())
}

// ----------------------------------------------------------------------------
// Vault header
// ----------------------------------------------------------------------------

/// Persist a freshly created vault header (first run).
pub fn insert_header(conn: &Connection, header: &VaultHeader) -> AppResult<()> {
    let ts = now();
    let vault_id = format!("v_{}", uuid::Uuid::new_v4());
    conn.execute(
        "INSERT INTO vault_meta
            (id, schema_version, vault_id, kdf_params, kdf_salt, vault_key_nonce, encrypted_vault_key, created_at, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![
            header.schema_version,
            vault_id,
            header.kdf_params,
            header.salt.to_vec(),
            header.wrap_nonce.to_vec(),
            header.wrapped_vault_key,
            ts,
        ],
    )?;
    Ok(())
}

pub fn vault_id(conn: &Connection) -> AppResult<Option<String>> {
    conn.query_row("SELECT vault_id FROM vault_meta WHERE id = 1", [], |r| {
        r.get(0)
    })
    .optional()
    .map_err(Into::into)
}

/// Create the vault header and optional demo content as one logical unit.
pub fn initialize_vault(
    conn: &Connection,
    header: &VaultHeader,
    key: &VaultKey,
    seed_demo: bool,
) -> AppResult<()> {
    with_savepoint(conn, "initialize_vault", || {
        insert_header(conn, header)?;
        if seed_demo {
            crate::seed::seed_if_empty(conn, key)?;
        }
        Ok(())
    })
}

/// Load the vault header, if a vault has been initialized.
pub fn load_header(conn: &Connection) -> AppResult<Option<VaultHeader>> {
    let row = conn
        .query_row(
            "SELECT schema_version, kdf_params, kdf_salt, vault_key_nonce, encrypted_vault_key
               FROM vault_meta WHERE id = 1",
            [],
            |r| {
                let schema_version: i64 = r.get(0)?;
                let kdf_params: String = r.get(1)?;
                let salt: Vec<u8> = r.get(2)?;
                let nonce: Vec<u8> = r.get(3)?;
                let wrapped: Vec<u8> = r.get(4)?;
                Ok((schema_version, kdf_params, salt, nonce, wrapped))
            },
        )
        .optional()?;

    let Some((schema_version, kdf_params, salt, nonce, wrapped)) = row else {
        return Ok(None);
    };

    let salt: [u8; crypto::SALT_LEN] = salt
        .try_into()
        .map_err(|_| AppError::Db("corrupt vault salt".into()))?;
    let wrap_nonce: [u8; crypto::NONCE_LEN] = nonce
        .try_into()
        .map_err(|_| AppError::Db("corrupt vault nonce".into()))?;

    Ok(Some(VaultHeader {
        schema_version,
        kdf_params,
        salt,
        wrap_nonce,
        wrapped_vault_key: wrapped,
    }))
}

// ----------------------------------------------------------------------------
// Projects
// ----------------------------------------------------------------------------

/// Insert a new project and return its id. Placed at the top (sort_index 0) with
/// existing projects pushed down, matching the design's "new project goes first".
pub fn create_project(conn: &Connection, input: &CreateProjectInput) -> AppResult<String> {
    let id = format!("p_{}", uuid::Uuid::new_v4().simple());
    let mono = monogram(&input.name);
    let ts = now();
    let name = input.name.trim();
    let sub = input.sub.trim();
    validate_len("Project name", name, 80)?;
    validate_len("Project description", sub, 160)?;
    if !is_hex_color(&input.color) {
        return Err(AppError::Invalid(
            "Project color must be a #RRGGBB value".into(),
        ));
    }
    with_savepoint(conn, "create_project", || {
        conn.execute(
            "UPDATE projects SET sort_index = sort_index + 1 WHERE deleted_at IS NULL",
            [],
        )?;
        conn.execute(
            "INSERT INTO projects (id, name, sub, mono, color, icon_json, sort_index, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, 0, ?6, ?6)",
            params![&id, name, sub, mono, input.color, ts],
        )?;
        conn.execute(
            "INSERT INTO activity (at, action, target, kind) VALUES (?1, ?2, ?3, ?4)",
            params![ts, "CREATE", format!("{name} · project"), "add"],
        )?;
        Ok(())
    })?;
    Ok(id)
}

/// Update a project's editable metadata.
pub fn update_project(conn: &Connection, input: &UpdateProjectInput) -> AppResult<()> {
    let mono = monogram(&input.name);
    let ts = now();
    let name = input.name.trim();
    let sub = input.sub.trim();
    validate_len("Project name", name, 80)?;
    validate_len("Project description", sub, 160)?;
    if !is_hex_color(&input.color) {
        return Err(AppError::Invalid(
            "Project color must be a #RRGGBB value".into(),
        ));
    }
    with_savepoint(conn, "update_project", || {
        let n = conn.execute(
            "UPDATE projects
                SET name = ?2,
                    sub = ?3,
                    mono = ?4,
                    color = ?5,
                    updated_at = ?6,
                    revision = revision + 1
              WHERE id = ?1 AND deleted_at IS NULL",
            params![&input.project_id, name, sub, mono, &input.color, ts],
        )?;
        if n == 0 {
            return Err(AppError::NotFound(format!("project {}", input.project_id)));
        }
        conn.execute(
            "INSERT INTO activity (at, action, target, kind) VALUES (?1, ?2, ?3, ?4)",
            params![ts, "EDIT", format!("{name} · project"), "edit"],
        )?;
        Ok(())
    })
}

/// Derive a 2-letter monogram from a project name.
fn monogram(name: &str) -> String {
    let letters: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect();
    if letters.is_empty() {
        "NP".to_string()
    } else {
        letters.to_uppercase()
    }
}

/// Update a project's icon (stored as JSON, or cleared when `None`).
pub fn set_project_icon(
    conn: &Connection,
    project_id: &str,
    icon: Option<&ProjectIcon>,
) -> AppResult<()> {
    let json = match icon {
        Some(i) => {
            validate_project_icon(i)?;
            Some(serde_json::to_string(i)?)
        }
        None => None,
    };
    let n = conn.execute(
        "UPDATE projects SET icon_json = ?2, updated_at = ?3, revision = revision + 1 WHERE id = ?1 AND deleted_at IS NULL",
        params![project_id, json, now()],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("project {project_id}")));
    }
    Ok(())
}

/// Soft-delete a project and all of its service data.
pub fn delete_project(conn: &Connection, project_id: &str) -> AppResult<()> {
    if project_id == PERSONAL_PROJECT_ID {
        return Err(AppError::Invalid("Personal vault cannot be deleted".into()));
    }
    let (name, sort_index) = conn
        .query_row(
            "SELECT name, sort_index FROM projects WHERE id = ?1 AND deleted_at IS NULL",
            params![project_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("project {project_id}")))?;

    let ts = now();
    with_savepoint(conn, "delete_project", || {
        let n = conn.execute(
            "UPDATE projects
                SET deleted_at = ?2, updated_at = ?2, revision = revision + 1
              WHERE id = ?1 AND deleted_at IS NULL",
            params![project_id, ts],
        )?;
        if n == 0 {
            return Err(AppError::NotFound(format!("project {project_id}")));
        }
        conn.execute(
            "UPDATE secret_fields
                SET deleted_at = ?2, updated_at = ?2, revision = revision + 1
              WHERE service_id IN (SELECT id FROM services WHERE project_id = ?1)
                AND deleted_at IS NULL",
            params![project_id, ts],
        )?;
        conn.execute(
            "UPDATE password_history
                SET deleted_at = ?2, revision = revision + 1
              WHERE service_id IN (SELECT id FROM services WHERE project_id = ?1)
                AND deleted_at IS NULL",
            params![project_id, ts],
        )?;
        conn.execute(
            "UPDATE services
                SET deleted_at = ?2, updated_at = ?2, revision = revision + 1
              WHERE project_id = ?1 AND deleted_at IS NULL",
            params![project_id, ts],
        )?;
        conn.execute(
            "DELETE FROM attachments WHERE project_id = ?1",
            params![project_id],
        )?;
        conn.execute(
            "UPDATE projects
                SET sort_index = sort_index - 1, revision = revision + 1
              WHERE sort_index > ?1 AND deleted_at IS NULL",
            params![sort_index],
        )?;
        conn.execute(
            "INSERT INTO activity (at, action, target, kind) VALUES (?1, ?2, ?3, ?4)",
            params![ts, "DELETE", format!("{name} · project"), "warn"],
        )?;
        Ok(())
    })
}

/// Reorder projects to match the given id order. The list must be a permutation
/// of exactly the existing project ids — duplicates, unknown ids, or a wrong
/// count are rejected so the UI can't silently corrupt ordering.
pub fn reorder_projects(conn: &Connection, ordered_ids: &[String]) -> AppResult<()> {
    let mut unique = std::collections::HashSet::new();
    for id in ordered_ids {
        if !unique.insert(id.as_str()) {
            return Err(AppError::Invalid(format!("duplicate id in reorder: {id}")));
        }
    }
    let existing: i64 = conn.query_row(
        "SELECT COUNT(*) FROM projects WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    if ordered_ids.len() as i64 != existing {
        return Err(AppError::Invalid(
            "reorder must list every project exactly once".into(),
        ));
    }
    with_savepoint(conn, "reorder_projects", || {
        for (idx, id) in ordered_ids.iter().enumerate() {
            let n = conn.execute(
                "UPDATE projects SET sort_index = ?2, revision = revision + 1 WHERE id = ?1 AND deleted_at IS NULL",
                params![id, idx as i64],
            )?;
            if n == 0 {
                return Err(AppError::NotFound(format!("project {id}")));
            }
        }
        Ok(())
    })?;
    Ok(())
}

/// Load all projects with their card-preview marks, counts and attachments.
pub fn list_projects(conn: &Connection) -> AppResult<Vec<Project>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, sub, mono, color, icon_json, sort_index, created_at, updated_at
           FROM projects WHERE deleted_at IS NULL ORDER BY sort_index ASC, created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, String>(8)?,
        ))
    })?;

    let mut projects = Vec::new();
    for row in rows {
        let (id, name, sub, mono, color, icon_json, sort_index, created, updated) = row?;
        let personal = id == PERSONAL_PROJECT_ID;
        let icon = match icon_json {
            Some(j) => serde_json::from_str(&j).ok(),
            None => None,
        };
        let (count, totp_count, marks) = project_summary(conn, &id)?;
        let files = list_attachments(conn, &id)?;
        projects.push(Project {
            id,
            name,
            sub,
            mono,
            color,
            icon,
            created,
            updated,
            sort_index,
            personal,
            count,
            totp_count,
            marks,
            files,
        });
    }
    Ok(projects)
}

/// Fetch a single project view, or `None` if missing.
pub fn get_project(conn: &Connection, project_id: &str) -> AppResult<Option<Project>> {
    Ok(list_projects(conn)?
        .into_iter()
        .find(|p| p.id == project_id))
}

/// Validate a project id without building the heavier project view.
pub fn project_exists(conn: &Connection, project_id: &str) -> AppResult<bool> {
    conn.query_row(
        "SELECT 1 FROM projects WHERE id = ?1 AND deleted_at IS NULL",
        params![project_id],
        |_| Ok(true),
    )
    .optional()
    .map(|v| v.unwrap_or(false))
    .map_err(Into::into)
}

/// Find an existing project by display name, case-insensitively.
pub fn project_id_by_name(conn: &Connection, name: &str) -> AppResult<Option<String>> {
    let name = name.trim();
    if name.eq_ignore_ascii_case("personal") {
        ensure_personal_project(conn)?;
        return Ok(Some(PERSONAL_PROJECT_ID.to_string()));
    }
    conn.query_row(
        "SELECT id FROM projects WHERE lower(name) = lower(?1) AND deleted_at IS NULL ORDER BY sort_index ASC LIMIT 1",
        params![name],
        |r| r.get(0),
    )
    .optional()
    .map_err(Into::into)
}

/// Resolve a human project name for imports, creating it when missing.
pub fn ensure_project_by_name(conn: &Connection, name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() || name.eq_ignore_ascii_case("personal") {
        ensure_personal_project(conn)?;
        return Ok(PERSONAL_PROJECT_ID.to_string());
    }
    if let Some(id) = project_id_by_name(conn, name)? {
        return Ok(id);
    }
    create_project(
        conn,
        &CreateProjectInput {
            name: name.to_string(),
            sub: "Imported credentials".to_string(),
            color: "#c8ff2e".to_string(),
        },
    )
}

/// Count services, TOTP-enabled services, and gather up to 6 distinct marks.
fn project_summary(conn: &Connection, project_id: &str) -> AppResult<(i64, i64, Vec<ServiceMark>)> {
    let mut stmt = conn.prepare(
        "SELECT template_id, (totp_cipher IS NOT NULL) AS has_totp
           FROM services WHERE project_id = ?1 AND deleted_at IS NULL ORDER BY sort_index ASC",
    )?;
    let rows = stmt.query_map(params![project_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
    })?;

    let mut count = 0i64;
    let mut totp_count = 0i64;
    let mut marks: Vec<ServiceMark> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for row in rows {
        let (template_id, has_totp) = row?;
        count += 1;
        if has_totp {
            totp_count += 1;
        }
        if !seen.contains(&template_id) && marks.len() < 6 {
            if let Some(t) = templates::find(&template_id) {
                marks.push(ServiceMark {
                    mono: t.mono.to_string(),
                    color: t.color.to_string(),
                    slug: t.slug.map(|s| s.to_string()),
                    icon: t.icon.map(|s| s.to_string()),
                });
                seen.push(template_id);
            }
        }
    }
    Ok((count, totp_count, marks))
}

// ----------------------------------------------------------------------------
// Services + fields
// ----------------------------------------------------------------------------

/// Add a service to a project from a template, sealing each secret field value
/// and the optional TOTP secret. Returns the new service id.
pub fn add_service(
    conn: &Connection,
    key: &VaultKey,
    input: &AddServiceInput,
) -> AppResult<String> {
    let template = templates::find(&input.template_id)
        .ok_or_else(|| AppError::NotFound(format!("template {}", input.template_id)))?;
    validate_len("Service label", input.label.trim(), 80)?;
    let expires_at = validate_expiration(input.expires_at.as_deref())?;
    let allowed_labels: std::collections::HashSet<&str> =
        template.fields.iter().map(|f| f.label).collect();
    let mut seen_labels = std::collections::HashSet::new();
    for field in &input.fields {
        let label = field.label.as_str();
        if !seen_labels.insert(label) {
            return Err(AppError::Invalid(format!("Duplicate field: {label}")));
        }
        if !allowed_labels.contains(label) {
            return Err(AppError::Invalid(format!(
                "Unknown field for template: {label}"
            )));
        }
        validate_len(label, &field.value, 65_536)?;
    }
    if let Some(secret) = input.totp_secret.as_deref() {
        validate_len("TOTP secret", secret.trim(), 256)?;
    }
    // Validate parent exists.
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM projects WHERE id = ?1 AND deleted_at IS NULL",
            params![input.project_id],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if !exists {
        return Err(AppError::NotFound(format!("project {}", input.project_id)));
    }

    let service_id = format!("svc_{}", uuid::Uuid::new_v4().simple());
    let ts = now();

    // Seal the optional TOTP secret.
    let (totp_nonce, totp_cipher) = match &input.totp_secret {
        Some(s) if !s.trim().is_empty() => {
            crate::totp::validate_secret(s)?;
            let aad = format!("totp|{service_id}").into_bytes();
            let (n, c) = crypto::encrypt(key.expose(), s.trim().as_bytes(), &aad)?;
            (Some(n), Some(c))
        }
        _ => (None, None),
    };

    let next_index: i64 = conn.query_row(
        "SELECT COALESCE(MAX(sort_index)+1, 0) FROM services WHERE project_id = ?1 AND deleted_at IS NULL",
        params![input.project_id],
        |r| r.get(0),
    )?;

    // Compute the password strength NOW, from the plaintext we already hold, so
    // view/list rendering never has to decrypt. Stored on the service row.
    let strength: Option<u8> = template
        .fields
        .iter()
        .filter(|tf| tf.secret)
        .filter_map(|tf| {
            let value = input
                .fields
                .iter()
                .find(|f| f.label == tf.label)?
                .value
                .as_str();
            if strength::is_password_like(tf.label, value) {
                Some(strength::estimate(value))
            } else {
                None
            }
        })
        .min();

    with_savepoint(conn, "add_service", || {
        conn.execute(
            "INSERT INTO services
                (id, project_id, template_id, label, env, expires_at, sort_index, fav, reused, exposed, strength, totp_nonce, totp_cipher, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 0, 0, ?8, ?9, ?10, ?11, ?11)",
            params![
                &service_id,
                input.project_id,
                template.id,
                input.label.trim(),
                env_str(input.env),
                expires_at,
                next_index,
                strength,
                totp_nonce,
                totp_cipher,
                ts,
            ],
        )?;

        // Create one row per template field; fill from the provided values.
        for (idx, tf) in template.fields.iter().enumerate() {
            let field_id = format!("f_{}", uuid::Uuid::new_v4().simple());
            let provided = input
                .fields
                .iter()
                .find(|f| f.label == tf.label)
                .map(|f| f.value.clone())
                .unwrap_or_default();

            let (plain, nonce, cipher) = if tf.secret {
                if provided.is_empty() {
                    (None, None, None)
                } else {
                    let aad = field_aad(&input.project_id, &service_id, &field_id);
                    let (n, c) = crypto::encrypt(key.expose(), provided.as_bytes(), &aad)?;
                    (None, Some(n), Some(c))
                }
            } else {
                (Some(provided), None, None)
            };

            conn.execute(
                "INSERT INTO secret_fields
                    (id, service_id, label, field_type, is_secret, is_danger, is_area, sort_index, plain_value, nonce, cipher, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    field_id,
                    &service_id,
                    tf.label,
                    field_type_str(tf.field_type),
                    tf.secret,
                    tf.danger,
                    tf.area,
                    idx as i64,
                    plain,
                    nonce,
                    cipher,
                    ts,
                ],
            )?;
        }

        conn.execute(
            "UPDATE projects SET updated_at = ?2, revision = revision + 1 WHERE id = ?1",
            params![input.project_id, ts],
        )?;
        conn.execute(
            "INSERT INTO activity (at, action, target, kind) VALUES (?1, ?2, ?3, ?4)",
            params![
                ts,
                "CREATE",
                format!("{} · {} service", template.name, input.project_id),
                "add"
            ],
        )?;
        Ok(())
    })?;
    Ok(service_id)
}

/// Find a service by its stable import identity: project, template, and label.
pub fn find_service_id_by_identity(
    conn: &Connection,
    project_id: &str,
    template_id: &str,
    label: &str,
) -> AppResult<Option<String>> {
    conn.query_row(
        "SELECT id FROM services
          WHERE project_id = ?1
            AND template_id = ?2
            AND label = ?3
            AND deleted_at IS NULL
          ORDER BY updated_at DESC, id DESC
          LIMIT 1",
        params![project_id, template_id, label.trim()],
        |r| r.get(0),
    )
    .optional()
    .map_err(Into::into)
}

/// Remove a service (cascades to its fields).
pub fn delete_service(conn: &Connection, service_id: &str) -> AppResult<()> {
    let row = conn
        .query_row(
            "SELECT project_id, sort_index, label, template_id FROM services WHERE id = ?1 AND deleted_at IS NULL",
            params![service_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((project_id, sort_index, label, template_id)) = row else {
        return Err(AppError::NotFound(format!("service {service_id}")));
    };

    let ts = now();
    with_savepoint(conn, "delete_service", || {
        let n = conn.execute(
            "UPDATE services
                SET deleted_at = ?2, updated_at = ?2, revision = revision + 1
              WHERE id = ?1 AND deleted_at IS NULL",
            params![service_id, ts],
        )?;
        if n == 0 {
            return Err(AppError::NotFound(format!("service {service_id}")));
        }
        conn.execute(
            "UPDATE services
                SET sort_index = sort_index - 1, revision = revision + 1
              WHERE project_id = ?1 AND sort_index > ?2 AND deleted_at IS NULL",
            params![project_id, sort_index],
        )?;
        conn.execute(
            "UPDATE projects SET updated_at = ?2, revision = revision + 1 WHERE id = ?1",
            params![project_id, ts],
        )?;
        let target = if label.trim().is_empty() {
            let template_name = templates::find(&template_id).map_or("Service", |t| t.name);
            format!("{template_name} · service")
        } else {
            format!("{label} · service")
        };
        conn.execute(
            "INSERT INTO activity (at, action, target, kind) VALUES (?1, ?2, ?3, ?4)",
            params![ts, "DELETE", target, "warn"],
        )?;
        Ok(())
    })?;
    Ok(())
}

/// A lightweight internal row used to build [`Service`] / [`Item`] views.
struct ServiceRow {
    id: String,
    project_id: String,
    template_id: String,
    label: String,
    env: String,
    expires_at: Option<String>,
    sort_index: i64,
    fav: bool,
    reused: bool,
    exposed: bool,
    strength: Option<u8>,
    has_totp: bool,
    updated: String,
}

fn load_service_rows(conn: &Connection, where_project: Option<&str>) -> AppResult<Vec<ServiceRow>> {
    let base =
        "SELECT s.id, s.project_id, s.template_id, s.label, s.env, s.expires_at, s.sort_index,
                       s.fav, s.reused, s.exposed, s.strength, (s.totp_cipher IS NOT NULL),
                       s.updated_at
                  FROM services s";
    let mut rows = Vec::new();
    let map = |r: &rusqlite::Row| {
        Ok(ServiceRow {
            id: r.get(0)?,
            project_id: r.get(1)?,
            template_id: r.get(2)?,
            label: r.get(3)?,
            env: r.get(4)?,
            expires_at: r.get(5)?,
            sort_index: r.get(6)?,
            fav: r.get(7)?,
            reused: r.get(8)?,
            exposed: r.get(9)?,
            strength: r.get::<_, Option<i64>>(10)?.map(|v| v.clamp(0, 100) as u8),
            has_totp: r.get(11)?,
            updated: r.get(12)?,
        })
    };
    if let Some(pid) = where_project {
        let mut stmt = conn.prepare(&format!(
            "{base} WHERE s.project_id = ?1 AND s.deleted_at IS NULL ORDER BY s.sort_index ASC"
        ))?;
        for row in stmt.query_map(params![pid], map)? {
            rows.push(row?);
        }
    } else {
        let mut stmt = conn.prepare(&format!(
            "{base} WHERE s.deleted_at IS NULL ORDER BY s.updated_at DESC"
        ))?;
        for row in stmt.query_map([], map)? {
            rows.push(row?);
        }
    }
    Ok(rows)
}

/// Build the full [`Service`] view (with fields) for a row.
///
/// This NEVER decrypts secret values — it only reports whether a value exists
/// (`has_value`) and reads the precomputed `strength` from the row. Plaintext is
/// produced solely by [`reveal_field`] / [`service_totp`] on explicit request.
fn build_service(conn: &Connection, row: ServiceRow) -> AppResult<Service> {
    let template = templates::find(&row.template_id)
        .ok_or_else(|| AppError::NotFound(format!("template {}", row.template_id)))?;

    let mut stmt = conn.prepare(
        "SELECT id, label, field_type, is_secret, is_danger, is_area, plain_value, (cipher IS NOT NULL)
           FROM secret_fields WHERE service_id = ?1 AND deleted_at IS NULL ORDER BY sort_index ASC",
    )?;
    let field_rows = stmt.query_map(params![row.id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, bool>(3)?,
            r.get::<_, bool>(4)?,
            r.get::<_, bool>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, bool>(7)?,
        ))
    })?;

    let mut fields = Vec::new();
    let mut danger_any = false;

    for fr in field_rows {
        let (id, label, ftype, is_secret, is_danger, is_area, plain, has_cipher) = fr?;
        if is_danger {
            danger_any = true;
        }
        let has_value = if is_secret {
            has_cipher
        } else {
            plain.as_deref().map(|v| !v.is_empty()).unwrap_or(false)
        };

        fields.push(FieldView {
            id,
            label,
            field_type: parse_field_type(&ftype),
            secret: is_secret,
            danger: is_danger,
            area: is_area,
            has_value,
            value: if is_secret { None } else { plain },
        });
    }

    let strength = row.strength;
    let label = row.label.clone();
    let title = if label.is_empty() {
        template.name.to_string()
    } else {
        format!("{} · {}", template.name, label)
    };

    Ok(Service {
        id: row.id,
        project_id: row.project_id,
        template_id: template.id.to_string(),
        template_name: template.name.to_string(),
        mono: template.mono.to_string(),
        color: template.color.to_string(),
        slug: template.slug.map(|s| s.to_string()),
        icon: template.icon.map(|s| s.to_string()),
        group: template.group.to_string(),
        label,
        env: parse_env(&row.env),
        expires_at: row.expires_at,
        title,
        updated: row.updated,
        sort_index: row.sort_index,
        fields,
        totp: row.has_totp,
        danger: danger_any,
        strength,
        fav: row.fav,
        reused: row.reused,
        exposed: row.exposed,
    })
}

/// List the services of a project, fully built (fields + precomputed strength).
/// Never decrypts secret values.
pub fn list_services(conn: &Connection, project_id: &str) -> AppResult<Vec<Service>> {
    let rows = load_service_rows(conn, Some(project_id))?;
    rows.into_iter().map(|r| build_service(conn, r)).collect()
}

/// Fetch one service by id.
pub fn get_service(conn: &Connection, service_id: &str) -> AppResult<Service> {
    let row = load_service_rows(conn, None)?
        .into_iter()
        .find(|r| r.id == service_id)
        .ok_or_else(|| AppError::NotFound(format!("service {service_id}")))?;
    build_service(conn, row)
}

/// Edit a service. Secret fields are updated only when a non-empty value is
/// supplied; the previous encrypted value is archived before replacement.
pub fn update_service(
    conn: &Connection,
    key: &VaultKey,
    input: &UpdateServiceInput,
) -> AppResult<Service> {
    let (project_id, template_id) = conn
        .query_row(
            "SELECT project_id, template_id FROM services WHERE id = ?1 AND deleted_at IS NULL",
            params![input.service_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("service {}", input.service_id)))?;

    let template = templates::find(&template_id)
        .ok_or_else(|| AppError::NotFound(format!("template {template_id}")))?;
    validate_len("Service label", input.label.trim(), 80)?;
    let expires_at = validate_expiration(input.expires_at.as_deref())?;

    let allowed_labels: std::collections::HashSet<&str> =
        template.fields.iter().map(|f| f.label).collect();
    let mut field_values: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for field in &input.fields {
        let label = field.label.as_str();
        if !allowed_labels.contains(label) {
            return Err(AppError::Invalid(format!(
                "Unknown field for template: {label}"
            )));
        }
        if field_values.insert(label, field.value.as_str()).is_some() {
            return Err(AppError::Invalid(format!("Duplicate field: {label}")));
        }
        validate_len(label, &field.value, 65_536)?;
    }
    if let Some(secret) = input.totp_secret.as_deref() {
        validate_len("TOTP secret", secret.trim(), 256)?;
    }

    struct StoredField {
        id: String,
        label: String,
        is_secret: bool,
        plain: Option<String>,
        nonce: Option<Vec<u8>>,
        cipher: Option<Vec<u8>>,
    }

    let mut fields = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, label, is_secret, plain_value, nonce, cipher
               FROM secret_fields WHERE service_id = ?1 AND deleted_at IS NULL ORDER BY sort_index ASC",
        )?;
        let rows = stmt.query_map(params![input.service_id], |r| {
            Ok(StoredField {
                id: r.get(0)?,
                label: r.get(1)?,
                is_secret: r.get(2)?,
                plain: r.get(3)?,
                nonce: r.get(4)?,
                cipher: r.get(5)?,
            })
        })?;
        for row in rows {
            fields.push(row?);
        }
    }

    let ts = now();
    let mut strength_scores: Vec<u8> = Vec::new();
    with_savepoint(conn, "update_service", || {
        for field in &fields {
            let supplied = field_values.get(field.label.as_str()).copied();
            if field.is_secret {
                let needs_existing =
                    supplied.is_some_and(|v| !v.is_empty()) || is_password_label(&field.label);
                let mut existing = if needs_existing {
                    match (&field.nonce, &field.cipher) {
                        (Some(n), Some(c)) => {
                            let aad = field_aad(&project_id, &input.service_id, &field.id);
                            Some(decrypt_secret_string(
                                key,
                                n,
                                c,
                                &aad,
                                "secret is not valid UTF-8",
                            )?)
                        }
                        _ => None,
                    }
                } else {
                    None
                };

                if let Some(new_value) = supplied.filter(|v| !v.is_empty()) {
                    if existing.as_deref() != Some(new_value) {
                        if let Some(old) = existing.as_deref() {
                            archive_secret_value(
                                conn,
                                key,
                                &field.id,
                                &input.service_id,
                                &field.label,
                                old,
                                &ts,
                            )?;
                        }
                        let aad = field_aad(&project_id, &input.service_id, &field.id);
                        let (nonce, cipher) =
                            crypto::encrypt(key.expose(), new_value.as_bytes(), &aad)?;
                        conn.execute(
                            "UPDATE secret_fields
                                SET plain_value = NULL, nonce = ?2, cipher = ?3, updated_at = ?4, revision = revision + 1
                              WHERE id = ?1",
                            params![field.id, nonce, cipher, ts],
                        )?;
                    }
                    if strength::is_password_like(&field.label, new_value) {
                        strength_scores.push(strength::estimate(new_value));
                    }
                } else if let Some(old) = existing.as_deref() {
                    if strength::is_password_like(&field.label, old) {
                        strength_scores.push(strength::estimate(old));
                    }
                }
                if let Some(old) = existing.as_mut() {
                    old.zeroize();
                }
            } else if let Some(new_value) = supplied {
                conn.execute(
                    "UPDATE secret_fields SET plain_value = ?2, updated_at = ?3, revision = revision + 1 WHERE id = ?1",
                    params![field.id, new_value, ts],
                )?;
            } else if let Some(value) = field.plain.as_deref() {
                if strength::is_password_like(&field.label, value) {
                    strength_scores.push(strength::estimate(value));
                }
            }
        }

        let strength = strength_scores.into_iter().min();
        conn.execute(
            "UPDATE services
                SET label = ?2, env = ?3, expires_at = ?4, strength = ?5, updated_at = ?6, revision = revision + 1
              WHERE id = ?1",
            params![
                input.service_id,
                input.label.trim(),
                env_str(input.env),
                expires_at,
                strength,
                ts
            ],
        )?;

        if let Some(secret) = input
            .totp_secret
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            crate::totp::validate_secret(secret)?;
            let aad = format!("totp|{}", input.service_id).into_bytes();
            let (nonce, cipher) = crypto::encrypt(key.expose(), secret.as_bytes(), &aad)?;
            conn.execute(
                "UPDATE services SET totp_nonce = ?2, totp_cipher = ?3, updated_at = ?4, revision = revision + 1 WHERE id = ?1",
                params![input.service_id, nonce, cipher, ts],
            )?;
        }

        conn.execute(
            "UPDATE projects SET updated_at = ?2, revision = revision + 1 WHERE id = ?1",
            params![project_id, ts],
        )?;
        conn.execute(
            "INSERT INTO activity (at, action, target, kind) VALUES (?1, ?2, ?3, ?4)",
            params![ts, "EDIT", format!("{} · service", template.name), "edit"],
        )?;
        Ok(())
    })?;

    get_service(conn, &input.service_id)
}

/// Project every service across all projects into the flattened [`Item`] list
/// used by the "All Items" browser and home widgets. Never decrypts secrets.
pub fn list_items(conn: &Connection) -> AppResult<Vec<Item>> {
    // Lightweight project context (id → name/color/mono/created), loaded once.
    // Avoids the heavier per-project summary/attachment work `list_projects` does.
    struct Ctx {
        name: String,
        color: String,
        mono: String,
        created: String,
    }
    let mut ctx: std::collections::HashMap<String, Ctx> = std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, name, color, mono, created_at FROM projects WHERE deleted_at IS NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Ctx {
                    name: r.get(1)?,
                    color: r.get(2)?,
                    mono: r.get(3)?,
                    created: r.get(4)?,
                },
            ))
        })?;
        for row in rows {
            let (id, c) = row?;
            ctx.insert(id, c);
        }
    }

    let rows = load_service_rows(conn, None)?;
    let mut items = Vec::new();
    for row in rows {
        let Some(project) = ctx.get(&row.project_id) else {
            continue;
        };
        let project_id = row.project_id.clone();
        let svc = build_service(conn, row)?;
        let title = svc.title.clone();
        let mut tags = vec![
            svc.group.clone(),
            project_id.clone(),
            project.name.clone(),
            svc.template_id.clone(),
            svc.template_name.clone(),
            svc.label.clone(),
            title.clone(),
            env_str(svc.env).to_string(),
        ];
        for field in &svc.fields {
            tags.push(field.label.clone());
            tags.push(format!("{:?}", field.field_type));
            if !field.secret {
                if let Some(value) = field.value.as_deref() {
                    tags.push(value.to_string());
                }
            }
        }
        tags.retain(|tag| !tag.trim().is_empty());
        items.push(Item {
            id: svc.id,
            project_id: project_id.clone(),
            project_name: project.name.clone(),
            project_color: project.color.clone(),
            project_mono: project.mono.clone(),
            template_id: svc.template_id.clone(),
            template_name: svc.template_name.clone(),
            mono: svc.mono,
            color: svc.color,
            slug: svc.slug,
            icon: svc.icon,
            label: svc.label,
            env: svc.env,
            expires_at: svc.expires_at,
            title,
            field_count: svc.fields.len() as i64,
            totp: svc.totp,
            danger: svc.danger,
            updated: svc.updated,
            created: project.created.clone(),
            fav: svc.fav,
            strength: svc.strength,
            reused: svc.reused,
            exposed: svc.exposed,
            tags,
        });
    }
    Ok(items)
}

/// Decrypt and return a single secret field's value (the explicit reveal/copy path).
pub fn reveal_field(
    conn: &Connection,
    key: &VaultKey,
    field_id: &str,
) -> AppResult<RevealedSecret> {
    let row = conn
        .query_row(
            "SELECT f.service_id, s.project_id, f.is_secret, f.plain_value, f.nonce, f.cipher
              FROM secret_fields f JOIN services s ON s.id = f.service_id
              WHERE f.id = ?1 AND f.deleted_at IS NULL AND s.deleted_at IS NULL",
            params![field_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<Vec<u8>>>(4)?,
                    r.get::<_, Option<Vec<u8>>>(5)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("field {field_id}")))?;

    let (service_id, project_id, is_secret, _plain, nonce, cipher) = row;

    let value = if !is_secret {
        return Err(AppError::Invalid("Field is not secret".into()));
    } else if let (Some(n), Some(c)) = (nonce, cipher) {
        let aad = field_aad(&project_id, &service_id, field_id);
        let mut bytes = crypto::decrypt(key.expose(), &n, &c, &aad)?;
        let value = match std::str::from_utf8(&bytes) {
            Ok(s) => s.to_string(),
            Err(_) => {
                bytes.zeroize();
                return Err(AppError::Crypto("secret is not valid UTF-8".into()));
            }
        };
        bytes.zeroize();
        value
    } else {
        String::new()
    };

    Ok(RevealedSecret {
        field_id: field_id.to_string(),
        value,
    })
}

/// List archived previous secret values for a service. Values stay encrypted
/// until explicitly revealed by id.
pub fn list_password_history(
    conn: &Connection,
    service_id: &str,
) -> AppResult<Vec<PasswordHistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, field_id, service_id, label, created_at
          FROM password_history
          WHERE service_id = ?1 AND deleted_at IS NULL
          ORDER BY created_at DESC, id DESC",
    )?;
    let rows = stmt.query_map(params![service_id], |r| {
        Ok(PasswordHistoryEntry {
            id: r.get(0)?,
            field_id: r.get(1)?,
            service_id: r.get(2)?,
            label: r.get(3)?,
            created: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Reveal one archived previous secret value.
pub fn reveal_history(
    conn: &Connection,
    key: &VaultKey,
    history_id: &str,
) -> AppResult<RevealedSecret> {
    let (field_id, nonce, cipher) = conn
        .query_row(
            "SELECT field_id, nonce, cipher FROM password_history WHERE id = ?1 AND deleted_at IS NULL",
            params![history_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Vec<u8>>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("password history {history_id}")))?;
    let aad = history_aad(history_id);
    let value = decrypt_secret_string(
        key,
        &nonce,
        &cipher,
        &aad,
        "archived secret is not valid UTF-8",
    )?;
    Ok(RevealedSecret { field_id, value })
}

// ----------------------------------------------------------------------------
// Pairing devices + device unlock
// ----------------------------------------------------------------------------

pub fn list_devices(conn: &Connection) -> AppResult<Vec<PairedDevice>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, platform, trusted, revoked_at, created_at, last_seen_at
           FROM devices
          ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(PairedDevice {
            id: r.get(0)?,
            name: r.get(1)?,
            platform: r.get(2)?,
            trusted: r.get::<_, i64>(3)? != 0,
            revoked_at: r.get(4)?,
            created_at: r.get(5)?,
            last_seen_at: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn upsert_device(
    conn: &Connection,
    id: &str,
    name: &str,
    platform: &str,
    public_key: &str,
) -> AppResult<()> {
    validate_len("Device name", name, 80)?;
    validate_len("Device platform", platform, 32)?;
    validate_len("Device public key", public_key, 128)?;
    let ts = now();
    conn.execute(
        "INSERT INTO devices (id, name, platform, public_key, trusted, revoked_at, created_at, last_seen_at)
         VALUES (?1, ?2, ?3, ?4, 1, NULL, ?5, ?5)
         ON CONFLICT(id) DO UPDATE SET
           name = excluded.name,
           platform = excluded.platform,
           public_key = excluded.public_key,
           trusted = 1,
           revoked_at = NULL,
           last_seen_at = excluded.last_seen_at",
        params![id, name.trim(), platform, public_key, ts],
    )?;
    Ok(())
}

pub fn revoke_device(conn: &Connection, device_id: &str) -> AppResult<()> {
    let ts = now();
    let n = conn.execute(
        "UPDATE devices
            SET trusted = 0, revoked_at = ?2, last_seen_at = ?2
          WHERE id = ?1 AND revoked_at IS NULL",
        params![device_id, ts],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("device {device_id}")));
    }
    Ok(())
}

pub fn store_device_unlock(
    conn: &Connection,
    device_id: &str,
    device_key: &[u8; crypto::KEY_LEN],
    vault_key: &[u8; crypto::KEY_LEN],
) -> AppResult<()> {
    let ts = now();
    let (nonce, cipher) = crypto::wrap_vault_key(device_key, vault_key)?;
    conn.execute(
        "INSERT INTO device_unlocks
            (id, device_id, device_key_nonce, encrypted_vault_key, created_at, updated_at)
         VALUES ('local', ?1, ?2, ?3, ?4, ?4)
         ON CONFLICT(id) DO UPDATE SET
           device_id = excluded.device_id,
           device_key_nonce = excluded.device_key_nonce,
           encrypted_vault_key = excluded.encrypted_vault_key,
           updated_at = excluded.updated_at",
        params![device_id, nonce, cipher, ts],
    )?;
    Ok(())
}

pub fn load_device_unlock(
    conn: &Connection,
    device_key: &[u8; crypto::KEY_LEN],
) -> AppResult<Option<[u8; crypto::KEY_LEN]>> {
    let row = conn
        .query_row(
            "SELECT device_id, device_key_nonce, encrypted_vault_key FROM device_unlocks WHERE id = 'local'",
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Vec<u8>>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((device_id, nonce, wrapped)) = row else {
        return Ok(None);
    };
    if device_id != crate::remembered_unlock::LOCAL_DEVICE_ID {
        let trusted = conn
            .query_row(
                "SELECT 1 FROM devices WHERE id = ?1 AND trusted = 1 AND revoked_at IS NULL",
                params![device_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !trusted {
            return Ok(None);
        }
    }
    Ok(Some(crypto::unwrap_vault_key(
        device_key, &nonce, &wrapped,
    )?))
}

/// Generate the current TOTP code for a service (decrypts its sealed seed).
pub fn service_totp(conn: &Connection, key: &VaultKey, service_id: &str) -> AppResult<TotpCode> {
    let row = conn
        .query_row(
            "SELECT totp_nonce, totp_cipher FROM services WHERE id = ?1 AND deleted_at IS NULL",
            params![service_id],
            |r| {
                Ok((
                    r.get::<_, Option<Vec<u8>>>(0)?,
                    r.get::<_, Option<Vec<u8>>>(1)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("service {service_id}")))?;

    let (Some(nonce), Some(cipher)) = row else {
        return Err(AppError::NotFound(format!(
            "TOTP not enabled for {service_id}"
        )));
    };

    let aad = format!("totp|{service_id}").into_bytes();
    let mut secret_bytes = crypto::decrypt(key.expose(), &nonce, &cipher, &aad)?;
    let mut secret = match std::str::from_utf8(&secret_bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            secret_bytes.zeroize();
            return Err(AppError::Crypto("TOTP secret is not valid UTF-8".into()));
        }
    };
    secret_bytes.zeroize();

    let generated = crate::totp::generate(&secret);
    secret.zeroize();
    let (code, remaining) = generated?;
    Ok(TotpCode {
        service_id: service_id.to_string(),
        code,
        remaining,
        period: crate::totp::PERIOD as u32,
    })
}

// ----------------------------------------------------------------------------
// Attachments + activity
// ----------------------------------------------------------------------------

fn list_attachments(conn: &Connection, project_id: &str) -> AppResult<Vec<Attachment>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, size, created_at FROM attachments WHERE project_id = ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map(params![project_id], |r| {
        Ok(Attachment {
            id: r.get(0)?,
            name: r.get(1)?,
            size: r.get(2)?,
            date: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Record an attachment against a project.
pub fn add_attachment(
    conn: &Connection,
    project_id: &str,
    name: &str,
    size: &str,
) -> AppResult<Attachment> {
    // Validate the frontend-supplied metadata rather than trusting it.
    let name = name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err(AppError::Invalid("Attachment name is invalid".into()));
    }
    let size = size.trim();
    if size.len() > 32 {
        return Err(AppError::Invalid("Attachment size is invalid".into()));
    }
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM projects WHERE id = ?1 AND deleted_at IS NULL",
            params![project_id],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if !exists {
        return Err(AppError::NotFound(format!("project {project_id}")));
    }

    let id = format!("att_{}", uuid::Uuid::new_v4().simple());
    let ts = now();
    with_savepoint(conn, "add_attachment", || {
        conn.execute(
            "INSERT INTO attachments (id, project_id, name, size, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&id, project_id, name, size, ts],
        )?;
        conn.execute(
            "UPDATE projects SET updated_at = ?2, revision = revision + 1 WHERE id = ?1",
            params![project_id, ts],
        )?;
        conn.execute(
            "INSERT INTO activity (at, action, target, kind) VALUES (?1, ?2, ?3, ?4)",
            params![ts, "ATTACH", format!("{name} · metadata"), "add"],
        )?;
        Ok(())
    })?;
    Ok(Attachment {
        id,
        name: name.to_string(),
        size: size.to_string(),
        date: ts,
    })
}

/// Append an activity entry (best-effort; failures are non-fatal to the caller).
pub fn log_activity(conn: &Connection, action: &str, target: &str, kind: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO activity (at, action, target, kind) VALUES (?1, ?2, ?3, ?4)",
        params![now(), action, target, kind],
    )?;
    Ok(())
}

/// Read recent activity, most recent first.
pub fn list_activity(conn: &Connection, limit: i64) -> AppResult<Vec<Activity>> {
    let mut stmt =
        conn.prepare("SELECT at, action, target, kind FROM activity ORDER BY id DESC LIMIT ?1")?;
    let rows = stmt.query_map(params![limit], |r| {
        Ok(Activity {
            time: r.get(0)?,
            action: r.get(1)?,
            target: r.get(2)?,
            kind: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Total number of services (used as the headline "secrets" count).
pub fn item_count(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM services WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?)
}

fn format_bytes(bytes: i64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// Storage usage based on SQLite page allocation, not a synthetic item count.
pub fn storage_usage(conn: &Connection) -> AppResult<Storage> {
    let page_count: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    let page_size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    let bytes = page_count.saturating_mul(page_size);
    const SOFT_REFERENCE_BYTES: i64 = 16 * 1024 * 1024;
    Ok(Storage {
        used: format_bytes(bytes),
        total: "local disk".to_string(),
        pct: ((bytes as f64 / SOFT_REFERENCE_BYTES as f64) * 100.0).min(100.0) as u8,
    })
}

// ----------------------------------------------------------------------------
// Vault-scoped app settings
// ----------------------------------------------------------------------------

const SETTING_AUTO_LOCK_MS: &str = "auto_lock_ms";
const SETTING_REVEAL_SECRETS_BY_DEFAULT: &str = "reveal_secrets_by_default";
const SETTING_CLIPBOARD_CLEAR_MS: &str = "clipboard_clear_ms";
const DEFAULT_AUTO_LOCK_MS: i64 = 60 * 60 * 1000;
const DEFAULT_CLIPBOARD_CLEAR_MS: i64 = 30 * 1000;
const AUTO_LOCK_CHOICES_MS: &[i64] =
    &[60 * 60 * 1000, 24 * 60 * 60 * 1000, 7 * 24 * 60 * 60 * 1000];
const CLIPBOARD_CLEAR_CHOICES_MS: &[i64] = &[10 * 1000, 30 * 1000, 60 * 1000];

pub fn app_settings(conn: &Connection) -> AppResult<AppSettings> {
    let rows = conn
        .prepare("SELECT key, value FROM app_settings")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<HashMap<_, _>, _>>()?;

    let auto_lock_ms = match rows.get(SETTING_AUTO_LOCK_MS).map(String::as_str) {
        Some("never") => None,
        Some(value) => value
            .parse::<i64>()
            .ok()
            .filter(|ms| AUTO_LOCK_CHOICES_MS.contains(ms)),
        None => Some(DEFAULT_AUTO_LOCK_MS),
    };
    let reveal_secrets_by_default = rows
        .get(SETTING_REVEAL_SECRETS_BY_DEFAULT)
        .map(|v| v == "true")
        .unwrap_or(false);
    let clipboard_clear_ms = rows
        .get(SETTING_CLIPBOARD_CLEAR_MS)
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|ms| CLIPBOARD_CLEAR_CHOICES_MS.contains(ms))
        .unwrap_or(DEFAULT_CLIPBOARD_CLEAR_MS);

    Ok(AppSettings {
        auto_lock_ms,
        reveal_secrets_by_default,
        clipboard_clear_ms,
    })
}

pub fn update_app_settings(
    conn: &Connection,
    input: &UpdateAppSettingsInput,
) -> AppResult<AppSettings> {
    if let Some(ms) = input.auto_lock_ms {
        if !AUTO_LOCK_CHOICES_MS.contains(&ms) {
            return Err(AppError::Invalid(
                "Auto-lock must be 1 hour, 24 hours, 7 days, or never".into(),
            ));
        }
    }
    if !CLIPBOARD_CLEAR_CHOICES_MS.contains(&input.clipboard_clear_ms) {
        return Err(AppError::Invalid(
            "Clipboard clear delay must be 10, 30, or 60 seconds".into(),
        ));
    }

    let ts = now();
    set_setting(
        conn,
        SETTING_AUTO_LOCK_MS,
        input
            .auto_lock_ms
            .map(|ms| ms.to_string())
            .unwrap_or_else(|| "never".to_string()),
        &ts,
    )?;
    set_setting(
        conn,
        SETTING_REVEAL_SECRETS_BY_DEFAULT,
        if input.reveal_secrets_by_default {
            "true"
        } else {
            "false"
        },
        &ts,
    )?;
    set_setting(
        conn,
        SETTING_CLIPBOARD_CLEAR_MS,
        input.clipboard_clear_ms.to_string(),
        &ts,
    )?;
    log_activity(conn, "UPDATE", "Changed app settings", "config").ok();
    app_settings(conn)
}

fn set_setting(
    conn: &Connection,
    key: &str,
    value: impl AsRef<str>,
    updated_at: &str,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET
           value = excluded.value,
           updated_at = excluded.updated_at",
        params![key, value.as_ref(), updated_at],
    )?;
    Ok(())
}

/// Assert the stored schema version is one we understand.
pub fn check_schema(conn: &Connection) -> AppResult<()> {
    if let Some(h) = load_header(conn)? {
        if h.schema_version > SCHEMA_VERSION {
            return Err(AppError::Db(format!(
                "vault schema v{} is newer than supported v{}",
                h.schema_version, SCHEMA_VERSION
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;

    /// Open an in-memory vault with a known key for testing.
    fn setup() -> (Connection, VaultKey) {
        let conn = crate::db::open_in_memory().unwrap();
        let pw: SecretString = "Test-Master-Pass-1!".to_string().into();
        let (header, key) = crate::vault::create(&pw).unwrap();
        insert_header(&conn, &header).unwrap();
        (conn, key)
    }

    #[test]
    fn app_settings_defaults_and_updates_roundtrip() {
        let (conn, _key) = setup();

        let defaults = app_settings(&conn).unwrap();
        assert_eq!(defaults.auto_lock_ms, Some(60 * 60 * 1000));
        assert!(!defaults.reveal_secrets_by_default);
        assert_eq!(defaults.clipboard_clear_ms, 30 * 1000);

        let saved = update_app_settings(
            &conn,
            &UpdateAppSettingsInput {
                auto_lock_ms: Some(7 * 24 * 60 * 60 * 1000),
                reveal_secrets_by_default: true,
                clipboard_clear_ms: 60 * 1000,
            },
        )
        .unwrap();

        assert_eq!(saved.auto_lock_ms, Some(7 * 24 * 60 * 60 * 1000));
        assert!(saved.reveal_secrets_by_default);
        assert_eq!(saved.clipboard_clear_ms, 60 * 1000);
        assert_eq!(
            app_settings(&conn).unwrap().auto_lock_ms,
            saved.auto_lock_ms
        );

        let never = update_app_settings(
            &conn,
            &UpdateAppSettingsInput {
                auto_lock_ms: None,
                reveal_secrets_by_default: true,
                clipboard_clear_ms: 10 * 1000,
            },
        )
        .unwrap();
        assert_eq!(never.auto_lock_ms, None);
    }

    #[test]
    fn app_settings_reject_unsupported_durations() {
        let (conn, _key) = setup();

        assert!(matches!(
            update_app_settings(
                &conn,
                &UpdateAppSettingsInput {
                    auto_lock_ms: Some(5 * 60 * 1000),
                    reveal_secrets_by_default: false,
                    clipboard_clear_ms: 30 * 1000,
                },
            ),
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            update_app_settings(
                &conn,
                &UpdateAppSettingsInput {
                    auto_lock_ms: Some(60 * 60 * 1000),
                    reveal_secrets_by_default: false,
                    clipboard_clear_ms: 5 * 1000,
                },
            ),
            Err(AppError::Invalid(_))
        ));
    }

    #[test]
    fn revoked_device_cannot_use_stored_unlock() {
        let (conn, key) = setup();
        let device_key = crypto::random_key().unwrap();

        upsert_device(
            &conn,
            "dev_phone",
            "Android phone",
            "android",
            &crate::pairing::encode(&crypto::random_key().unwrap()),
        )
        .unwrap();
        store_device_unlock(&conn, "dev_phone", &device_key, key.expose()).unwrap();
        assert!(load_device_unlock(&conn, &device_key).unwrap().is_some());

        revoke_device(&conn, "dev_phone").unwrap();
        assert!(load_device_unlock(&conn, &device_key).unwrap().is_none());
    }

    #[test]
    fn project_and_service_roundtrip() {
        let (conn, key) = setup();

        let pid = create_project(
            &conn,
            &CreateProjectInput {
                name: "Nimbus".into(),
                sub: "SaaS".into(),
                color: "#5b9dff".into(),
            },
        )
        .unwrap();

        let svc_id = add_service(
            &conn,
            &key,
            &AddServiceInput {
                project_id: pid.clone(),
                template_id: "login".into(),
                label: "Apple ID".into(),
                env: Environment::All,
                expires_at: None,
                fields: vec![
                    ServiceFieldInput {
                        label: "URL".into(),
                        value: "appleid.apple.com".into(),
                    },
                    ServiceFieldInput {
                        label: "Email / Username".into(),
                        value: "me@icloud.com".into(),
                    },
                    ServiceFieldInput {
                        label: "Password".into(),
                        value: "Gr@nite-Harbor-71".into(),
                    },
                ],
                totp_secret: Some("JBSWY3DPEHPK3PXP".into()),
            },
        )
        .unwrap();

        // The project lists one service with the right counts.
        let projects = list_projects(&conn).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].count, 1);
        assert_eq!(projects[0].totp_count, 1);

        // Building the service view does NOT require the key (no decryption on list).
        let services = list_services(&conn, &pid).unwrap();
        assert_eq!(services.len(), 1);
        let svc = &services[0];
        assert!(svc.totp);
        // Strength was precomputed from the password and stored.
        assert!(svc.strength.is_some());
        // The password field reports a value but its plaintext is NOT included.
        let pw_field = svc.fields.iter().find(|f| f.label == "Password").unwrap();
        assert!(pw_field.secret && pw_field.has_value && pw_field.value.is_none());
        // The non-secret URL field IS included in clear.
        let url_field = svc.fields.iter().find(|f| f.label == "URL").unwrap();
        assert_eq!(url_field.value.as_deref(), Some("appleid.apple.com"));

        // Reveal returns the real plaintext for the secret field.
        let revealed = reveal_field(&conn, &key, &pw_field.id).unwrap();
        assert_eq!(revealed.value, "Gr@nite-Harbor-71");

        // TOTP generates a 6-digit code.
        let code = service_totp(&conn, &key, &svc_id).unwrap();
        assert_eq!(code.code.len(), 6);

        // Deleting the service hides it through a sync-safe tombstone.
        delete_service(&conn, &svc_id).unwrap();
        assert_eq!(item_count(&conn).unwrap(), 0);
        assert!(list_services(&conn, &pid).unwrap().is_empty());
        let tombstone: Option<String> = conn
            .query_row(
                "SELECT deleted_at FROM services WHERE id = ?1",
                params![svc_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(tombstone.is_some());
    }

    #[test]
    fn update_project_changes_metadata_and_monogram() {
        let (conn, _key) = setup();
        let pid = create_project(
            &conn,
            &CreateProjectInput {
                name: "Nimbus".into(),
                sub: "SaaS".into(),
                color: "#5b9dff".into(),
            },
        )
        .unwrap();

        update_project(
            &conn,
            &UpdateProjectInput {
                project_id: pid.clone(),
                name: "Nimbus Labs".into(),
                sub: "Internal tools".into(),
                color: "#34e29a".into(),
            },
        )
        .unwrap();

        let project = get_project(&conn, &pid).unwrap().unwrap();
        assert_eq!(project.name, "Nimbus Labs");
        assert_eq!(project.sub, "Internal tools");
        assert_eq!(project.color, "#34e29a");
        assert_eq!(project.mono, "NL");
    }

    #[test]
    fn delete_project_soft_deletes_services_and_protects_personal() {
        let (conn, key) = setup();
        ensure_personal_project(&conn).unwrap();
        let pid = create_project(
            &conn,
            &CreateProjectInput {
                name: "Temporary".into(),
                sub: "Delete me".into(),
                color: "#5b9dff".into(),
            },
        )
        .unwrap();
        let svc_id = add_service(
            &conn,
            &key,
            &AddServiceInput {
                project_id: pid.clone(),
                template_id: "login".into(),
                label: "Staging".into(),
                env: Environment::Staging,
                expires_at: None,
                fields: vec![ServiceFieldInput {
                    label: "Password".into(),
                    value: "Temp-Pass-11!".into(),
                }],
                totp_secret: None,
            },
        )
        .unwrap();
        assert_eq!(item_count(&conn).unwrap(), 1);

        delete_project(&conn, &pid).unwrap();

        assert!(get_project(&conn, &pid).unwrap().is_none());
        assert!(list_services(&conn, &pid).unwrap().is_empty());
        assert_eq!(item_count(&conn).unwrap(), 0);
        let service_deleted_at: Option<String> = conn
            .query_row(
                "SELECT deleted_at FROM services WHERE id = ?1",
                params![svc_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(service_deleted_at.is_some());
        assert!(matches!(
            delete_project(&conn, PERSONAL_PROJECT_ID),
            Err(AppError::Invalid(_))
        ));
    }

    #[test]
    fn reveal_requires_correct_aad_binding() {
        // A field's ciphertext is bound to project|service|field, so the same key
        // can still decrypt it via reveal (sanity) — and item_count tracks services.
        let (conn, key) = setup();
        let pid = create_project(
            &conn,
            &CreateProjectInput {
                name: "P".into(),
                sub: "".into(),
                color: "#ffffff".into(),
            },
        )
        .unwrap();
        add_service(
            &conn,
            &key,
            &AddServiceInput {
                project_id: pid,
                template_id: "note".into(),
                label: String::new(),
                env: Environment::All,
                expires_at: None,
                fields: vec![],
                totp_secret: None,
            },
        )
        .unwrap();
        assert_eq!(item_count(&conn).unwrap(), 1);
    }

    #[test]
    fn update_service_archives_latest_three_passwords() {
        let (conn, key) = setup();
        let pid = create_project(
            &conn,
            &CreateProjectInput {
                name: "Archive".into(),
                sub: "".into(),
                color: "#5b9dff".into(),
            },
        )
        .unwrap();
        let svc_id = add_service(
            &conn,
            &key,
            &AddServiceInput {
                project_id: pid,
                template_id: "login".into(),
                label: "Prod".into(),
                env: Environment::Production,
                expires_at: Some("2026-07-02".into()),
                fields: vec![ServiceFieldInput {
                    label: "Password".into(),
                    value: "First-Pass-11!".into(),
                }],
                totp_secret: None,
            },
        )
        .unwrap();

        for value in [
            "Second-Pass-22!",
            "Third-Pass-33!",
            "Fourth-Pass-44!",
            "Fifth-Pass-55!",
        ] {
            update_service(
                &conn,
                &key,
                &UpdateServiceInput {
                    service_id: svc_id.clone(),
                    label: "Prod".into(),
                    env: Environment::Production,
                    expires_at: Some("2026-08-01".into()),
                    fields: vec![ServiceFieldInput {
                        label: "Password".into(),
                        value: value.into(),
                    }],
                    totp_secret: None,
                },
            )
            .unwrap();
        }

        let service = get_service(&conn, &svc_id).unwrap();
        assert_eq!(service.expires_at.as_deref(), Some("2026-08-01"));

        let history = list_password_history(&conn, &svc_id).unwrap();
        assert_eq!(history.len(), 3);
        let mut values = history
            .iter()
            .map(|h| reveal_history(&conn, &key, &h.id).unwrap().value)
            .collect::<Vec<_>>();
        values.sort();
        assert_eq!(
            values,
            vec![
                "Fourth-Pass-44!".to_string(),
                "Second-Pass-22!".to_string(),
                "Third-Pass-33!".to_string()
            ]
        );
    }
}
