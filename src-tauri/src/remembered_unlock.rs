//! Local remembered unlock sessions.
//!
//! This stores a random device key in the OS credential manager and stores the
//! vault key encrypted under that device key in SQLite. If the OS credential
//! manager is unavailable, desktop builds fall back to an app-data file so the
//! user's chosen "trusted OS login" behavior still works. The master password is
//! never stored. Removing either side makes the remembered unlock unusable.

use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

use crate::error::{AppError, AppResult};
use crate::pairing;
use crate::vault::crypto;

pub const LOCAL_DEVICE_ID: &str = "local_desktop_session";

const CREDENTIAL_VERSION: u8 = 1;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
const SERVICE: &str = "MONOLITH";
#[cfg(not(any(target_os = "android", target_os = "ios")))]
const ACCOUNT_PREFIX: &str = "vault-session:";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RememberedCredential {
    version: u8,
    vault_id: String,
    device_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<String>,
    updated_at: String,
}

pub fn save(
    vault_id: &str,
    device_key: &[u8; crypto::KEY_LEN],
    auto_lock_ms: Option<i64>,
) -> AppResult<()> {
    let credential = RememberedCredential {
        version: CREDENTIAL_VERSION,
        vault_id: vault_id.to_string(),
        device_key: pairing::encode(device_key),
        expires_at: expiry_label(auto_lock_ms)?,
        updated_at: now_label()?,
    };
    save_raw(vault_id, &credential)
}

pub fn load_device_key(vault_id: &str) -> AppResult<Option<[u8; crypto::KEY_LEN]>> {
    let Some(credential) = (match load_raw(vault_id) {
        Ok(value) => value,
        Err(AppError::Invalid(_)) | Err(AppError::BadPassword) | Err(AppError::Crypto(_)) => {
            clear(vault_id)?;
            return Ok(None);
        }
        Err(err) => return Err(err),
    }) else {
        return Ok(None);
    };
    if credential.version != CREDENTIAL_VERSION || credential.vault_id != vault_id {
        clear(vault_id)?;
        return Ok(None);
    }
    if is_expired(&credential)? {
        clear(vault_id)?;
        return Ok(None);
    }
    Ok(Some(pairing::bytes32_from_b64(
        &credential.device_key,
        "remembered device key",
    )?))
}

pub fn refresh(vault_id: &str, auto_lock_ms: Option<i64>) -> AppResult<bool> {
    let Some(mut credential) = load_raw(vault_id)? else {
        return Ok(false);
    };
    if credential.version != CREDENTIAL_VERSION || credential.vault_id != vault_id {
        clear(vault_id)?;
        return Ok(false);
    }
    if is_expired(&credential)? {
        clear(vault_id)?;
        return Ok(false);
    }
    credential.expires_at = expiry_label(auto_lock_ms)?;
    credential.updated_at = now_label()?;
    save_raw(vault_id, &credential)?;
    Ok(true)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn clear(vault_id: &str) -> AppResult<()> {
    use keyring::Error as KeyringError;

    let keyring_result = match entry(vault_id) {
        Ok(entry) => match entry.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(err) => Err(keyring_error(err)),
        },
        Err(err) => Err(err),
    };
    let file_result = delete_fallback(vault_id);
    match (keyring_result, file_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(file_err)) => Err(file_err),
        (Err(keyring_err), Ok(())) => Err(keyring_err),
        (Err(keyring_err), Err(file_err)) => Err(AppError::Other(format!(
            "{keyring_err}; fallback clear also failed: {file_err}"
        ))),
    }
}

fn delete_fallback(vault_id: &str) -> AppResult<()> {
    let path = fallback_path(vault_id, false)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::Other(format!(
            "could not clear remembered unlock fallback: {err}"
        ))),
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn clear(vault_id: &str) -> AppResult<()> {
    delete_fallback(vault_id)
}

fn expiry_label(auto_lock_ms: Option<i64>) -> AppResult<Option<String>> {
    let Some(ms) = auto_lock_ms else {
        return Ok(None);
    };
    let expires_at = OffsetDateTime::now_utc() + Duration::milliseconds(ms);
    format_time(expires_at).map(Some)
}

fn is_expired(credential: &RememberedCredential) -> AppResult<bool> {
    let Some(expires_at) = credential.expires_at.as_deref() else {
        return Ok(false);
    };
    let parsed = OffsetDateTime::parse(expires_at, &Rfc3339)
        .map_err(|_| AppError::Invalid("remembered unlock expiry is invalid".into()))?;
    Ok(parsed <= OffsetDateTime::now_utc())
}

fn now_label() -> AppResult<String> {
    format_time(OffsetDateTime::now_utc())
}

fn format_time(value: OffsetDateTime) -> AppResult<String> {
    value
        .format(&Rfc3339)
        .map_err(|e| AppError::Other(format!("could not format session time: {e}")))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn load_raw(vault_id: &str) -> AppResult<Option<RememberedCredential>> {
    use keyring::Error as KeyringError;

    let keyring_credential = match entry(vault_id) {
        Ok(entry) => match entry.get_password() {
            Ok(value) => parse_credential(&value, "remembered unlock credential").map(Some),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(err) => Err(keyring_error(err)),
        },
        Err(keyring_err) => Err(keyring_err),
    };
    let fallback_credential = load_fallback(vault_id);

    match (keyring_credential, fallback_credential) {
        (Ok(keyring), Ok(fallback)) => Ok(newest_credential(keyring, fallback)),
        (Ok(Some(credential)), Err(err)) => {
            eprintln!("remembered unlock fallback unavailable: {err}");
            Ok(Some(credential))
        }
        (Ok(None), Err(err)) => Err(err),
        (Err(err), Ok(Some(credential))) => {
            eprintln!("remembered unlock keyring unavailable: {err}");
            Ok(Some(credential))
        }
        (Err(err), Ok(None)) => Err(err),
        (Err(keyring_err), Err(fallback_err)) => Err(AppError::Other(format!(
            "{keyring_err}; fallback also failed: {fallback_err}"
        ))),
    }
}

fn parse_credential(value: &str, label: &str) -> AppResult<RememberedCredential> {
    serde_json::from_str(value).map_err(|_| AppError::Invalid(format!("{label} is invalid")))
}

fn load_fallback(vault_id: &str) -> AppResult<Option<RememberedCredential>> {
    let path = fallback_path(vault_id, false)?;
    let value = match std::fs::read_to_string(path) {
        Ok(value) => value,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(AppError::Other(format!(
                "could not read remembered unlock fallback: {err}"
            )))
        }
    };
    parse_credential(&value, "remembered unlock fallback").map(Some)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn load_raw(vault_id: &str) -> AppResult<Option<RememberedCredential>> {
    load_fallback(vault_id)
}

fn save_fallback(vault_id: &str, value: &str) -> AppResult<()> {
    let path = fallback_path(vault_id, true)?;
    std::fs::write(path, value)
        .map_err(|err| AppError::Other(format!("could not save remembered unlock fallback: {err}")))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn save_raw(vault_id: &str, credential: &RememberedCredential) -> AppResult<()> {
    let value = serde_json::to_string(credential)?;
    let fallback_result = save_fallback(vault_id, &value);
    let keyring_result =
        entry(vault_id).and_then(|entry| entry.set_password(&value).map_err(keyring_error));

    match (keyring_result, fallback_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(fallback_err)) => {
            eprintln!("remembered unlock fallback save failed: {fallback_err}");
            Ok(())
        }
        (Err(keyring_err), Ok(())) => {
            eprintln!("remembered unlock keyring save failed: {keyring_err}");
            Ok(())
        }
        (Err(keyring_err), Err(fallback_err)) => Err(AppError::Other(format!(
            "{keyring_err}; fallback save also failed: {fallback_err}"
        ))),
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn save_raw(vault_id: &str, credential: &RememberedCredential) -> AppResult<()> {
    let value = serde_json::to_string(credential)?;
    save_fallback(vault_id, &value)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn entry(vault_id: &str) -> AppResult<keyring::Entry> {
    keyring::Entry::new(SERVICE, &format!("{ACCOUNT_PREFIX}{vault_id}")).map_err(keyring_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn keyring_error(err: keyring::Error) -> AppError {
    AppError::Other(format!(
        "could not access the OS credential manager for remembered unlock: {err}"
    ))
}

#[cfg(any(test, not(any(target_os = "android", target_os = "ios"))))]
fn newest_credential(
    left: Option<RememberedCredential>,
    right: Option<RememberedCredential>,
) -> Option<RememberedCredential> {
    match (left, right) {
        (Some(left), Some(right)) => {
            if credential_updated_at(&right) > credential_updated_at(&left) {
                Some(right)
            } else {
                Some(left)
            }
        }
        (Some(credential), None) | (None, Some(credential)) => Some(credential),
        (None, None) => None,
    }
}

#[cfg(any(test, not(any(target_os = "android", target_os = "ios"))))]
fn credential_updated_at(credential: &RememberedCredential) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(&credential.updated_at, &Rfc3339).ok()
}

fn fallback_path(vault_id: &str, create_dir: bool) -> AppResult<std::path::PathBuf> {
    let dir = fallback_base_dir()?.join("remembered-unlock");
    if create_dir {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir.join(format!(
        "{}.json",
        vault_id
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch
                } else {
                    '_'
                }
            })
            .take(96)
            .collect::<String>()
    )))
}

fn fallback_base_dir() -> AppResult<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("MONOLITH_APP_DATA_DIR") {
        return Ok(std::path::PathBuf::from(path));
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if let Some(path) = std::env::var_os("APPDATA").or_else(|| std::env::var_os("LOCALAPPDATA"))
        {
            return Ok(std::path::PathBuf::from(path).join("com.radionica.monolith"));
        }
        if let Some(home) = std::env::var_os("HOME") {
            return Ok(std::path::PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("com.radionica.monolith"));
        }
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return Ok(std::path::PathBuf::from(home).join("com.radionica.monolith"));
        }
    }

    Err(AppError::Other(
        "could not locate app-data directory".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential(updated_at: &str, device_key: &str) -> RememberedCredential {
        RememberedCredential {
            version: CREDENTIAL_VERSION,
            vault_id: "vault-test".to_string(),
            device_key: device_key.to_string(),
            expires_at: None,
            updated_at: updated_at.to_string(),
        }
    }

    #[test]
    fn newest_credential_prefers_latest_updated_at() {
        let older = credential("2026-06-03T10:00:00Z", "older");
        let newer = credential("2026-06-03T11:00:00Z", "newer");

        let selected = newest_credential(Some(older), Some(newer)).unwrap();

        assert_eq!(selected.device_key, "newer");
    }

    #[test]
    fn newest_credential_keeps_existing_when_other_side_missing() {
        let existing = credential("2026-06-03T10:00:00Z", "existing");

        let selected = newest_credential(Some(existing), None).unwrap();

        assert_eq!(selected.device_key, "existing");
    }
}
