//! The vault: envelope encryption and the in-memory unlocked state.
//!
//! Envelope model:
//! ```text
//! master password ──Argon2id──▶ wrapping key ──unwraps──▶ vault key ──seals──▶ every secret field + TOTP seed
//! ```
//! Only the salt, KDF params, wrap-nonce and *wrapped* vault key are persisted
//! (the [`VaultHeader`]). The plaintext vault key lives only in memory inside
//! [`VaultKey`], which zeroizes itself on drop and on lock.

pub mod crypto;

use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroize;

use crate::error::{AppError, AppResult};
use crypto::{KEY_LEN, NONCE_LEN, SALT_LEN};

/// The 32-byte vault key, held only in memory. Zeroized on drop.
pub struct VaultKey([u8; KEY_LEN]);

impl VaultKey {
    fn new(bytes: [u8; KEY_LEN]) -> Self {
        VaultKey(bytes)
    }

    /// Rehydrate an already-authorized vault key, used after encrypted device
    /// pairing imports a vault onto a trusted phone.
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        VaultKey(bytes)
    }

    /// Borrow the raw key bytes for an encryption/decryption call.
    pub fn expose(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl Drop for VaultKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Everything needed to unwrap the vault key, persisted in the `vault_meta` row.
#[derive(Clone)]
pub struct VaultHeader {
    pub schema_version: i64,
    pub kdf_params: String,
    pub salt: [u8; SALT_LEN],
    pub wrap_nonce: [u8; NONCE_LEN],
    pub wrapped_vault_key: Vec<u8>,
}

/// Create a brand-new vault: derive a wrapping key from the master password,
/// generate a random vault key, wrap it, and return both the persistable header
/// and the live (unlocked) key.
pub fn create(master_password: &SecretString) -> AppResult<(VaultHeader, VaultKey)> {
    let salt = crypto::random_salt()?;
    let kdf = crypto::KdfParams::current();
    let mut wrapping =
        crypto::derive_wrapping_key(master_password.expose_secret().as_bytes(), &salt, kdf)?;

    let vault_key = crypto::random_key()?;
    let (wrap_nonce, wrapped) = crypto::wrap_vault_key(&wrapping, &vault_key)?;
    wrapping.zeroize();

    let header = VaultHeader {
        schema_version: crate::db::SCHEMA_VERSION,
        kdf_params: kdf.to_label(),
        salt,
        wrap_nonce: to_nonce(&wrap_nonce)?,
        wrapped_vault_key: wrapped,
    };
    Ok((header, VaultKey::new(vault_key)))
}

/// Unlock an existing vault: re-derive the wrapping key using the vault's STORED
/// KDF parameters (so future default changes don't lock out old vaults) and
/// unwrap the vault key. Returns [`AppError::BadPassword`] if the password is wrong.
pub fn unlock(master_password: &SecretString, header: &VaultHeader) -> AppResult<VaultKey> {
    let kdf = crypto::KdfParams::from_label(&header.kdf_params)?;
    let mut wrapping = crypto::derive_wrapping_key(
        master_password.expose_secret().as_bytes(),
        &header.salt,
        kdf,
    )?;
    let result = crypto::unwrap_vault_key(&wrapping, &header.wrap_nonce, &header.wrapped_vault_key);
    wrapping.zeroize();
    Ok(VaultKey::new(result?))
}

/// Convert a byte slice into a fixed-size nonce array, validating length.
fn to_nonce(bytes: &[u8]) -> AppResult<[u8; NONCE_LEN]> {
    if bytes.len() != NONCE_LEN {
        return Err(AppError::Crypto("invalid nonce length".into()));
    }
    let mut n = [0u8; NONCE_LEN];
    n.copy_from_slice(bytes);
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;

    #[test]
    fn create_then_unlock_with_right_password() {
        let pw: SecretString = "MyStr0ng!Master#Pass".to_string().into();
        let (header, key) = create(&pw).unwrap();
        let key_bytes = *key.expose();
        drop(key);

        let unlocked = unlock(&pw, &header).unwrap();
        assert_eq!(*unlocked.expose(), key_bytes);
    }

    #[test]
    fn unlock_with_wrong_password_fails() {
        let pw: SecretString = "right-password-123!".to_string().into();
        let (header, _key) = create(&pw).unwrap();
        let wrong: SecretString = "wrong-password-123!".to_string().into();
        assert!(matches!(
            unlock(&wrong, &header),
            Err(AppError::BadPassword)
        ));
    }
}
