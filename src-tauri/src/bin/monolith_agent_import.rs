use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use monolith_lib::db::{self, repo};
use monolith_lib::error::{AppError, AppResult};
use monolith_lib::models::AgentImportBundle;
use monolith_lib::remembered_unlock;
use monolith_lib::vault::VaultKey;

const MAX_AGENT_IMPORT_FILE_BYTES: u64 = 8 * 1024 * 1024;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("MONOLITH agent import failed: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> AppResult<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }

    let bundle_path = PathBuf::from(&args[0]);
    let db_path = if args.len() >= 2 {
        PathBuf::from(&args[1])
    } else {
        default_db_path()?
    };
    if !bundle_path.exists() {
        return Err(AppError::NotFound(format!(
            "import bundle {}",
            bundle_path.display()
        )));
    }
    if !db_path.exists() {
        return Err(AppError::NotFound(format!(
            "vault db {}",
            db_path.display()
        )));
    }
    let metadata = fs::metadata(&bundle_path)
        .map_err(|err| AppError::Other(format!("could not inspect import bundle: {err}")))?;
    if metadata.len() > MAX_AGENT_IMPORT_FILE_BYTES {
        return Err(AppError::Invalid(
            "Import bundle is too large for local import".into(),
        ));
    }

    let bundle_text = fs::read_to_string(&bundle_path)
        .map_err(|err| AppError::Other(format!("could not read import bundle: {err}")))?;
    let bundle: AgentImportBundle = serde_json::from_str(&bundle_text)
        .map_err(|err| AppError::Invalid(format!("import bundle is not valid JSON: {err}")))?;

    let conn = db::open(&db_path)?;
    let vault_id =
        repo::vault_id(&conn)?.ok_or_else(|| AppError::VaultState("Vault id is missing".into()))?;
    let device_key = remembered_unlock::load_device_key(&vault_id)?.ok_or_else(|| {
        AppError::VaultState(
            "No remembered local unlock is available. Install v0.1.4+, unlock MONOLITH once, then rerun this importer."
                .into(),
        )
    })?;
    let vault_key = repo::load_device_unlock(&conn, &device_key)?.ok_or_else(|| {
        AppError::VaultState("Remembered unlock is not stored in the vault".into())
    })?;
    let key = VaultKey::from_bytes(vault_key);
    let result = monolith_lib::agent_import::import_bundle(&conn, &key, &bundle)?;

    println!(
        "MONOLITH agent import complete: {} new, {} updated, {} skipped, {} errors",
        result.created,
        result.updated,
        result.skipped,
        result.errors.len()
    );
    for error in result.errors {
        println!(
            "  error #{} {}: {}",
            error.index + 1,
            error.label,
            error.message
        );
    }
    Ok(())
}

fn default_db_path() -> AppResult<PathBuf> {
    if let Ok(path) = env::var("MONOLITH_DB_PATH") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    if let Ok(appdata) = env::var("APPDATA") {
        return Ok(PathBuf::from(appdata)
            .join("com.radionica.monolith")
            .join("monolith.vault.db"));
    }
    if let Ok(home) = env::var("HOME") {
        return Ok(
            PathBuf::from(home).join(".local/share/com.radionica.monolith/monolith.vault.db")
        );
    }
    Err(AppError::Other(
        "could not determine MONOLITH app-data path".into(),
    ))
}

fn print_help() {
    println!(
        "Usage: monolith_agent_import <bundle.monolith-import.json> [path-to-monolith.vault.db]"
    );
}
