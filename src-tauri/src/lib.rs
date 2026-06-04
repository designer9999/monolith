//! MONOLITH — a local-first credential & secrets vault.
//!
//! Architecture (three layers):
//! ```text
//! React UI  ──invoke──▶  commands.rs  ──▶  vault core (crypto, db, templates, totp)
//! ```
//! The Rust core owns all encryption, storage, and TOTP generation; the frontend
//! only ever sees masked metadata, single revealed values on request, and codes.

mod agent_bridge;
pub mod agent_import;
mod commands;
pub mod db;
pub mod error;
pub mod models;
mod pairing;
pub mod remembered_unlock;
mod seed;
mod state;
mod strength;
pub mod templates;
mod totp;
pub mod vault;

use tauri::Manager;

use state::AppState;

/// Build, configure and run the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(mobile)]
            app.handle().plugin(tauri_plugin_barcode_scanner::init())?;
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;

            // The encrypted vault lives in the OS app-data directory.
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            std::env::set_var("MONOLITH_APP_DATA_DIR", &dir);
            let db_path = dir.join("monolith.vault.db");

            let app_state = AppState::new(&db_path)?;
            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::app_platform,
            commands::create_vault,
            commands::unlock_vault,
            commands::restore_remembered_unlock,
            commands::lock_vault_memory,
            commands::lock_vault,
            commands::list_projects,
            commands::list_services,
            commands::list_items,
            commands::list_activity,
            commands::list_templates,
            commands::list_field_suggestions,
            commands::create_project,
            commands::update_project,
            commands::set_project_icon,
            commands::delete_project,
            commands::reorder_projects,
            commands::add_service,
            commands::import_agent_bundle,
            commands::import_agent_bundle_file,
            commands::start_agent_bridge,
            commands::stop_agent_bridge,
            commands::agent_bridge_status,
            commands::update_service,
            commands::delete_service,
            commands::reveal_field,
            commands::list_password_history,
            commands::reveal_history,
            commands::generate_totp,
            commands::add_attachment,
            commands::storage_usage,
            commands::app_settings,
            commands::update_app_settings,
            commands::start_pairing_session,
            commands::pairing_session_status,
            commands::cancel_pairing_session,
            commands::approve_pairing_session,
            commands::list_devices,
            commands::revoke_device,
            commands::complete_pairing,
            commands::unlock_device_vault,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
