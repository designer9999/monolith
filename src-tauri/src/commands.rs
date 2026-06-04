//! The Tauri command surface — the only bridge between the frontend and the
//! vault core. Each command is small, validates input, and returns a typed
//! `Result<T, AppError>`. The unlocked vault key only crosses this boundary
//! inside the app-layer encrypted local pairing package.

use std::sync::Arc;

use secrecy::SecretString;
use tauri::{AppHandle, State};
use time::format_description::well_known::Rfc3339;

use crate::agent_bridge;
use crate::agent_import;
use crate::db::repo;
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::pairing;
use crate::remembered_unlock;
use crate::state::AppState;
use crate::templates::{self, Template};
use crate::vault::{self, crypto, VaultKey};

const MAX_AGENT_IMPORT_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Whether a vault exists and whether it's currently unlocked.
#[tauri::command]
pub fn vault_status(state: State<'_, AppState>) -> AppResult<VaultStatus> {
    state.with(|inner| {
        let initialized = crate::db::is_initialized(&inner.conn)?;
        let unlocked = inner.key.is_some();
        let item_count = if initialized {
            repo::item_count(&inner.conn).unwrap_or(0)
        } else {
            0
        };
        let vault_id = if initialized {
            repo::vault_id(&inner.conn)?
        } else {
            None
        };
        Ok(VaultStatus {
            initialized,
            unlocked,
            item_count,
            vault_id,
        })
    })
}

#[tauri::command]
pub fn app_platform() -> &'static str {
    #[cfg(target_os = "android")]
    {
        "android"
    }
    #[cfg(target_os = "ios")]
    {
        "ios"
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        "desktop"
    }
}

/// Enforce the master-password policy in Rust — the command layer is the real
/// boundary, so this must match (and not merely trust) the onboarding UI checks.
fn validate_master_password(pw: &str) -> AppResult<()> {
    let long_enough = pw.chars().count() >= 12;
    let has_upper = pw.chars().any(|c| c.is_uppercase());
    let has_digit = pw.chars().any(|c| c.is_ascii_digit());
    let has_symbol = pw.chars().any(|c| !c.is_alphanumeric());
    if long_enough && has_upper && has_digit && has_symbol {
        Ok(())
    } else {
        Err(AppError::Invalid(
            "Master password must be at least 12 characters and include an uppercase letter, a number, and a symbol".into(),
        ))
    }
}

/// First run: create a new vault from a master password. `seed_demo` controls
/// whether the example projects are inserted (the UI offers this as a choice).
#[tauri::command]
pub fn create_vault(
    master_password: String,
    seed_demo: bool,
    state: State<'_, AppState>,
) -> AppResult<VaultStatus> {
    validate_master_password(&master_password)?;
    let secret: SecretString = master_password.into();

    state.with(|inner| {
        if crate::db::is_initialized(&inner.conn)? {
            return Err(AppError::VaultState("A vault already exists".into()));
        }
        let (header, key) = vault::create(&secret)?;
        repo::initialize_vault(&inner.conn, &header, &key, seed_demo)?;
        repo::ensure_personal_project(&inner.conn)?;
        remember_unlock_session(
            &inner.conn,
            &key,
            repo::app_settings(&inner.conn)?.auto_lock_ms,
        )?;
        let item_count = repo::item_count(&inner.conn)?;
        inner.key = Some(key);
        Ok(VaultStatus {
            initialized: true,
            unlocked: true,
            item_count,
            vault_id: repo::vault_id(&inner.conn)?,
        })
    })
}

/// Unlock an existing vault with the master password.
#[tauri::command]
pub fn unlock_vault(master_password: String, state: State<'_, AppState>) -> AppResult<VaultStatus> {
    let secret: SecretString = master_password.into();
    state.with(|inner| {
        let header = repo::load_header(&inner.conn)?
            .ok_or_else(|| AppError::VaultState("No vault to unlock".into()))?;
        let key = vault::unlock(&secret, &header)?;
        repo::ensure_personal_project(&inner.conn)?;
        remember_unlock_session(
            &inner.conn,
            &key,
            repo::app_settings(&inner.conn)?.auto_lock_ms,
        )?;
        let item_count = repo::item_count(&inner.conn)?;
        inner.key = Some(key);
        Ok(VaultStatus {
            initialized: true,
            unlocked: true,
            item_count,
            vault_id: repo::vault_id(&inner.conn)?,
        })
    })
}

/// Restore a still-valid local remembered unlock session from the OS
/// credential manager. This never stores or uses the master password.
#[tauri::command]
pub fn restore_remembered_unlock(state: State<'_, AppState>) -> AppResult<VaultStatus> {
    state.with(|inner| {
        let initialized = crate::db::is_initialized(&inner.conn)?;
        if !initialized {
            return Ok(VaultStatus {
                initialized: false,
                unlocked: false,
                item_count: 0,
                vault_id: None,
            });
        }
        if inner.key.is_some() {
            return Ok(VaultStatus {
                initialized: true,
                unlocked: true,
                item_count: repo::item_count(&inner.conn)?,
                vault_id: repo::vault_id(&inner.conn)?,
            });
        }

        let Some(vault_id) = repo::vault_id(&inner.conn)? else {
            return Err(AppError::VaultState("Vault id is missing".into()));
        };
        let locked_item_count = repo::item_count(&inner.conn).unwrap_or(0);
        let device_key = match remembered_unlock::load_device_key(&vault_id) {
            Ok(Some(device_key)) => device_key,
            Ok(None) => {
                return Ok(VaultStatus {
                    initialized: true,
                    unlocked: false,
                    item_count: locked_item_count,
                    vault_id: Some(vault_id),
                });
            }
            Err(err) => {
                eprintln!("remembered unlock unavailable: {err}");
                return Ok(VaultStatus {
                    initialized: true,
                    unlocked: false,
                    item_count: locked_item_count,
                    vault_id: Some(vault_id),
                });
            }
        };
        let vault_key = match repo::load_device_unlock(&inner.conn, &device_key) {
            Ok(Some(vault_key)) => vault_key,
            Ok(None) | Err(AppError::BadPassword) | Err(AppError::Crypto(_)) => {
                remembered_unlock::clear(&vault_id).ok();
                return Ok(VaultStatus {
                    initialized: true,
                    unlocked: false,
                    item_count: locked_item_count,
                    vault_id: Some(vault_id),
                });
            }
            Err(err) => return Err(err),
        };
        let settings = repo::app_settings(&inner.conn)?;
        remembered_unlock::refresh(&vault_id, settings.auto_lock_ms).ok();
        inner.key = Some(VaultKey::from_bytes(vault_key));
        Ok(VaultStatus {
            initialized: true,
            unlocked: true,
            item_count: repo::item_count(&inner.conn)?,
            vault_id: Some(vault_id),
        })
    })
}

/// Lock the vault for this running process only. The remembered local unlock
/// session remains valid, so auto-lock and app restart can restore it.
#[tauri::command]
pub fn lock_vault_memory(state: State<'_, AppState>) -> AppResult<()> {
    agent_bridge::stop(&state.agent_bridge).ok();
    let result = state.with(|inner| {
        if let Some(key) = inner.key.as_ref() {
            let settings = repo::app_settings(&inner.conn)?;
            remember_unlock_session(&inner.conn, key, settings.auto_lock_ms)?;
        }
        inner.key = None; // VaultKey::drop zeroizes
        Ok(())
    });
    agent_bridge::stop(&state.agent_bridge).ok();
    result
}

/// Manual lock: drop the in-memory key and forget the remembered local session.
#[tauri::command]
pub fn lock_vault(state: State<'_, AppState>) -> AppResult<()> {
    agent_bridge::stop(&state.agent_bridge).ok();
    let result = state.with(|inner| {
        if let Some(vault_id) = repo::vault_id(&inner.conn)? {
            remembered_unlock::clear(&vault_id)?;
        }
        inner.key = None; // VaultKey::drop zeroizes
        Ok(())
    });
    agent_bridge::stop(&state.agent_bridge).ok();
    result
}

// --- read views ---

/// List all projects (card previews, counts, attachments).
#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> AppResult<Vec<Project>> {
    state.with_unlocked(|conn, _key| repo::list_projects(conn))
}

/// List a project's services with fields and strength.
#[tauri::command]
pub fn list_services(project_id: String, state: State<'_, AppState>) -> AppResult<Vec<Service>> {
    state.with_unlocked(|conn, _key| repo::list_services(conn, &project_id))
}

/// Flattened items across all projects (for the All Items browser + home).
#[tauri::command]
pub fn list_items(state: State<'_, AppState>) -> AppResult<Vec<Item>> {
    state.with_unlocked(|conn, _key| repo::list_items(conn))
}

/// Recent activity entries.
#[tauri::command]
pub fn list_activity(state: State<'_, AppState>) -> AppResult<Vec<Activity>> {
    state.with_unlocked(|conn, _key| repo::list_activity(conn, 12))
}

/// The full template catalog (for the Add Service modal).
#[tauri::command]
pub fn list_templates() -> Vec<Template> {
    templates::catalog()
}

// --- mutations ---

/// Create a project. Returns the created project view.
#[tauri::command]
pub fn create_project(input: CreateProjectInput, state: State<'_, AppState>) -> AppResult<Project> {
    if input.name.trim().is_empty() {
        return Err(AppError::Invalid("Project name is required".into()));
    }
    state.with_unlocked(|conn, _key| {
        let id = repo::create_project(conn, &input)?;
        repo::get_project(conn, &id)?
            .ok_or_else(|| AppError::Other("project not found after create".into()))
    })
}

/// Edit project metadata. Returns the updated project view.
#[tauri::command]
pub fn update_project(input: UpdateProjectInput, state: State<'_, AppState>) -> AppResult<Project> {
    if input.name.trim().is_empty() {
        return Err(AppError::Invalid("Project name is required".into()));
    }
    state.with_unlocked(|conn, _key| {
        repo::update_project(conn, &input)?;
        repo::get_project(conn, &input.project_id)?
            .ok_or_else(|| AppError::Other("project not found after update".into()))
    })
}

/// Set (or clear, when `icon` is `None`) a project's icon.
#[tauri::command]
pub fn set_project_icon(
    project_id: String,
    icon: Option<ProjectIcon>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    state.with_unlocked(|conn, _key| repo::set_project_icon(conn, &project_id, icon.as_ref()))
}

/// Delete a project and all service data inside it. The Personal vault is protected.
#[tauri::command]
pub fn delete_project(project_id: String, state: State<'_, AppState>) -> AppResult<()> {
    state.with_unlocked(|conn, _key| repo::delete_project(conn, &project_id))
}

/// Persist a new project order (drag-to-reorder).
#[tauri::command]
pub fn reorder_projects(ordered_ids: Vec<String>, state: State<'_, AppState>) -> AppResult<()> {
    state.with_unlocked(|conn, _key| repo::reorder_projects(conn, &ordered_ids))
}

/// Add a service from a template, sealing its secret values. Returns the new id.
#[tauri::command]
pub fn add_service(input: AddServiceInput, state: State<'_, AppState>) -> AppResult<String> {
    state.with_unlocked(|conn, key| repo::add_service(conn, key, &input))
}

/// Import a local agent-generated JSON bundle. Secrets stay inside this command
/// path and are encrypted by the same repo functions as manual service edits.
#[tauri::command]
pub fn import_agent_bundle(
    bundle: AgentImportBundle,
    state: State<'_, AppState>,
) -> AppResult<AgentImportResult> {
    state.with_unlocked(|conn, key| agent_import::import_bundle(conn, key, &bundle))
}

/// Import a local agent bundle from an explicit dropped/selected JSON path.
#[tauri::command]
pub fn import_agent_bundle_file(
    path: String,
    state: State<'_, AppState>,
) -> AppResult<AgentImportResult> {
    let path = std::path::PathBuf::from(path.trim());
    if !path.exists() {
        return Err(AppError::NotFound(format!(
            "import bundle {}",
            path.display()
        )));
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !(name.ends_with(".json") || name.ends_with(".monolith-import")) {
        return Err(AppError::Invalid(
            "Import bundle must be a JSON file".into(),
        ));
    }
    let metadata = std::fs::metadata(&path)
        .map_err(|err| AppError::Other(format!("could not inspect import bundle: {err}")))?;
    if metadata.len() > MAX_AGENT_IMPORT_FILE_BYTES {
        return Err(AppError::Invalid(
            "Import bundle is too large for local import".into(),
        ));
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|err| AppError::Other(format!("could not read import bundle: {err}")))?;
    let bundle: AgentImportBundle = serde_json::from_str(&text)
        .map_err(|err| AppError::Invalid(format!("import bundle is not valid JSON: {err}")))?;
    state.with_unlocked(|conn, key| agent_import::import_bundle(conn, key, &bundle))
}

#[tauri::command]
pub fn start_agent_bridge(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<AgentBridgeSession> {
    let store = Arc::clone(&state.agent_bridge);
    let inner = Arc::clone(&state.inner);
    state.with_unlocked(|_, _| agent_bridge::start(app, store, inner))
}

#[tauri::command]
pub fn stop_agent_bridge(state: State<'_, AppState>) -> AppResult<()> {
    agent_bridge::stop(&state.agent_bridge)
}

#[tauri::command]
pub fn agent_bridge_status(state: State<'_, AppState>) -> AppResult<Option<AgentBridgeSession>> {
    agent_bridge::status(&state.agent_bridge)
}

/// Edit a service and archive replaced secret values.
#[tauri::command]
pub fn update_service(input: UpdateServiceInput, state: State<'_, AppState>) -> AppResult<Service> {
    state.with_unlocked(|conn, key| repo::update_service(conn, key, &input))
}

/// Remove a service.
#[tauri::command]
pub fn delete_service(service_id: String, state: State<'_, AppState>) -> AppResult<()> {
    state.with_unlocked(|conn, _key| repo::delete_service(conn, &service_id))
}

/// Reveal a single secret field's plaintext (explicit user action).
#[tauri::command]
pub fn reveal_field(field_id: String, state: State<'_, AppState>) -> AppResult<RevealedSecret> {
    state.with_unlocked(|conn, key| {
        let revealed = repo::reveal_field(conn, key, &field_id)?;
        repo::log_activity(conn, "VIEW", "Revealed a secret field", "view").ok();
        Ok(revealed)
    })
}

/// List the encrypted archive entries for a service.
#[tauri::command]
pub fn list_password_history(
    service_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<PasswordHistoryEntry>> {
    state.with_unlocked(|conn, _key| repo::list_password_history(conn, &service_id))
}

/// Reveal one archived previous secret value.
#[tauri::command]
pub fn reveal_history(history_id: String, state: State<'_, AppState>) -> AppResult<RevealedSecret> {
    state.with_unlocked(|conn, key| repo::reveal_history(conn, key, &history_id))
}

/// Generate the current TOTP code for a service.
#[tauri::command]
pub fn generate_totp(service_id: String, state: State<'_, AppState>) -> AppResult<TotpCode> {
    state.with_unlocked(|conn, key| repo::service_totp(conn, key, &service_id))
}

/// Record an encrypted attachment against a project (metadata only for now).
#[tauri::command]
pub fn add_attachment(
    project_id: String,
    name: String,
    size: String,
    state: State<'_, AppState>,
) -> AppResult<Attachment> {
    state.with_unlocked(|conn, _key| repo::add_attachment(conn, &project_id, &name, &size))
}

/// Storage usage summary based on SQLite page allocation.
#[tauri::command]
pub fn storage_usage(state: State<'_, AppState>) -> AppResult<Storage> {
    state.with_unlocked(|conn, _key| repo::storage_usage(conn))
}

#[tauri::command]
pub fn app_settings(state: State<'_, AppState>) -> AppResult<AppSettings> {
    state.with_unlocked(|conn, _key| repo::app_settings(conn))
}

#[tauri::command]
pub fn update_app_settings(
    input: UpdateAppSettingsInput,
    state: State<'_, AppState>,
) -> AppResult<AppSettings> {
    state.with_unlocked(|conn, key| {
        let settings = repo::update_app_settings(conn, &input)?;
        remember_unlock_session(conn, key, settings.auto_lock_ms)?;
        Ok(settings)
    })
}

// --- local QR pairing ---

#[tauri::command]
pub fn start_pairing_session(state: State<'_, AppState>) -> AppResult<PairingSession> {
    state.with_unlocked(|_, _| Ok(()))?;
    pairing::start_session(std::sync::Arc::clone(&state.pairing))
}

#[tauri::command]
pub fn pairing_session_status(
    session_id: String,
    state: State<'_, AppState>,
) -> AppResult<PairingSessionStatus> {
    pairing::session_status(&state.pairing, &session_id)
}

#[tauri::command]
pub fn cancel_pairing_session(session_id: String, state: State<'_, AppState>) -> AppResult<()> {
    pairing::cancel_session(&state.pairing, &session_id)
}

#[tauri::command]
pub fn approve_pairing_session(
    session_id: String,
    state: State<'_, AppState>,
) -> AppResult<PairingSessionStatus> {
    let pending = pairing::pending_device_for_approval(&state.pairing, &session_id)?;
    let db_path = state.db_path.clone();
    let package = state.with_unlocked(|conn, key| {
        repo::upsert_device(
            conn,
            &pending.id,
            &pending.name,
            &pending.platform,
            &pending.public_key,
        )?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        let db_bytes = std::fs::read(&db_path)
            .map_err(|e| AppError::Other(format!("could not read vault snapshot: {e}")))?;
        let device_key = crypto::random_key()?;
        let vault_id = repo::vault_id(conn)?
            .ok_or_else(|| AppError::VaultState("Vault id is missing".into()))?;
        Ok(pairing::PairingTransferPackage {
            vault_id,
            device_id: pending.id.clone(),
            device_key: pairing::encode(&device_key),
            vault_key: pairing::encode(key.expose()),
            db: pairing::encode(&db_bytes),
            created_at: time::OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_default(),
        })
    })?;
    let envelope = pairing::create_envelope(
        &session_id,
        &pending.code,
        pending.desktop_secret,
        &pending.public_key,
        &pending.id,
        &package,
    )?;
    pairing::approve_session(&state.pairing, &session_id, envelope)?;
    pairing::session_status(&state.pairing, &session_id)
}

#[tauri::command]
pub fn list_devices(state: State<'_, AppState>) -> AppResult<Vec<PairedDevice>> {
    state.with_unlocked(|conn, _| repo::list_devices(conn))
}

#[tauri::command]
pub fn revoke_device(device_id: String, state: State<'_, AppState>) -> AppResult<()> {
    state.with_unlocked(|conn, _| repo::revoke_device(conn, &device_id))
}

#[tauri::command]
pub fn complete_pairing(
    input: CompletePairingInput,
    state: State<'_, AppState>,
) -> AppResult<PairingImportResult> {
    agent_bridge::stop(&state.agent_bridge).ok();
    let qr = pairing::parse_qr_payload(&input.qr_payload)?;
    let device_id = format!("dev_{}", uuid::Uuid::new_v4().simple());
    let device_name = if input.device_name.trim().is_empty() {
        "Android phone".to_string()
    } else {
        input.device_name.trim().to_string()
    };
    let package = pairing::fetch_and_decrypt_pairing(&qr, &device_id, &device_name, "android")?;
    if package.device_id != device_id {
        return Err(AppError::Invalid("Pairing package device mismatch".into()));
    }
    let db_bytes = pairing::decode(&package.db)?;
    let vault_key = pairing::bytes32_from_b64(&package.vault_key, "vault key")?;
    let device_key = pairing::bytes32_from_b64(&package.device_key, "device key")?;
    let db_path = state.db_path.clone();

    state.with(|inner| {
        inner.key = None;
        let temp = rusqlite::Connection::open_in_memory()?;
        let old = std::mem::replace(&mut inner.conn, temp);
        drop(old);

        for suffix in ["", "-wal", "-shm"] {
            let path = if suffix.is_empty() {
                db_path.clone()
            } else {
                db_path.with_file_name(format!(
                    "{}{suffix}",
                    db_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("monolith.vault.db")
                ))
            };
            let _ = std::fs::remove_file(path);
        }
        std::fs::write(&db_path, &db_bytes)
            .map_err(|e| AppError::Other(format!("could not import paired vault: {e}")))?;
        let conn = crate::db::open(&db_path)?;
        repo::check_schema(&conn)?;
        repo::store_device_unlock(&conn, &device_id, &device_key, &vault_key)?;
        repo::ensure_personal_project(&conn)?;
        remembered_unlock::save(
            &package.vault_id,
            &device_key,
            repo::app_settings(&conn)?.auto_lock_ms,
        )?;
        let item_count = repo::item_count(&conn)?;
        inner.conn = conn;
        inner.key = Some(VaultKey::from_bytes(vault_key));
        Ok(PairingImportResult {
            vault_id: package.vault_id,
            device_id,
            item_count,
        })
    })
}

#[tauri::command]
pub fn unlock_device_vault(
    device_key: String,
    state: State<'_, AppState>,
) -> AppResult<VaultStatus> {
    state.with(|inner| {
        let device_key = pairing::bytes32_from_b64(device_key.trim(), "device key")?;
        let vault_key = repo::load_device_unlock(&inner.conn, &device_key)?
            .ok_or_else(|| AppError::VaultState("No paired device unlock is stored".into()))?;
        remember_unlock_session(
            &inner.conn,
            &VaultKey::from_bytes(vault_key),
            repo::app_settings(&inner.conn)?.auto_lock_ms,
        )?;
        inner.key = Some(VaultKey::from_bytes(vault_key));
        Ok(VaultStatus {
            initialized: true,
            unlocked: true,
            item_count: repo::item_count(&inner.conn)?,
            vault_id: repo::vault_id(&inner.conn)?,
        })
    })
}

fn remember_unlock_session(
    conn: &rusqlite::Connection,
    key: &VaultKey,
    auto_lock_ms: Option<i64>,
) -> AppResult<()> {
    remember_unlock_session_inner(conn, key, auto_lock_ms)
}

fn remember_unlock_session_inner(
    conn: &rusqlite::Connection,
    key: &VaultKey,
    auto_lock_ms: Option<i64>,
) -> AppResult<()> {
    let vault_id =
        repo::vault_id(conn)?.ok_or_else(|| AppError::VaultState("Vault id is missing".into()))?;
    let device_key = match remembered_unlock::load_device_key(&vault_id) {
        Ok(Some(existing)) => existing,
        Ok(None) => crypto::random_key()?,
        Err(AppError::Other(err)) => {
            eprintln!("remembered unlock credential read failed, replacing local session: {err}");
            crypto::random_key()?
        }
        Err(err) => return Err(err),
    };
    repo::store_device_unlock(
        conn,
        remembered_unlock::LOCAL_DEVICE_ID,
        &device_key,
        key.expose(),
    )?;
    remembered_unlock::save(&vault_id, &device_key, auto_lock_ms)
}
