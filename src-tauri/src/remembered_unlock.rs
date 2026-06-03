//! Local remembered unlock sessions.
//!
//! This stores a random device key in the OS credential manager and stores the
//! vault key encrypted under that device key in SQLite. The master password is
//! never stored. Removing either side makes the remembered unlock unusable.

use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

use crate::error::{AppError, AppResult};
use crate::pairing;
use crate::vault::crypto;

pub const LOCAL_DEVICE_ID: &str = "local_desktop_session";

const CREDENTIAL_VERSION: u8 = 1;
const SERVICE: &str = "MONOLITH";
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

    match entry(vault_id)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(err) => Err(keyring_error(err)),
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn clear(_vault_id: &str) -> AppResult<()> {
    Ok(())
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

    let value = match entry(vault_id)?.get_password() {
        Ok(value) => value,
        Err(KeyringError::NoEntry) => return Ok(None),
        Err(err) => return Err(keyring_error(err)),
    };
    serde_json::from_str(&value)
        .map(Some)
        .map_err(|_| AppError::Invalid("remembered unlock credential is invalid".into()))
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn load_raw(_vault_id: &str) -> AppResult<Option<RememberedCredential>> {
    Ok(None)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn save_raw(vault_id: &str, credential: &RememberedCredential) -> AppResult<()> {
    let value = serde_json::to_string(credential)?;
    entry(vault_id)?.set_password(&value).map_err(keyring_error)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn save_raw(_vault_id: &str, _credential: &RememberedCredential) -> AppResult<()> {
    Ok(())
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
