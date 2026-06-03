//! Low-level cryptographic primitives.
//!
//! - **Key derivation:** Argon2id (64 MiB, 3 iterations, parallelism 1) turns the
//!   master password + salt into a 32-byte *wrapping key*.
//! - **Authenticated encryption:** XChaCha20-Poly1305 (24-byte nonce) seals values
//!   with optional *associated data* so a ciphertext cannot be silently relocated
//!   to a different field/service/project.
//!
//! The plaintext vault key never touches disk: only the salt, KDF params, nonce and
//! the *wrapped* (encrypted) vault key are persisted.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    Key, XChaCha20Poly1305, XNonce,
};
use rand::rngs::OsRng;
use rand::TryRngCore;
use zeroize::Zeroize;

use crate::error::{AppError, AppResult};

/// Length of every symmetric key (wrapping key and vault key) in bytes.
pub const KEY_LEN: usize = 32;
/// Length of the Argon2id salt in bytes.
pub const SALT_LEN: usize = 16;
/// Length of the XChaCha20-Poly1305 nonce in bytes.
pub const NONCE_LEN: usize = 24;

/// Argon2id cost parameters. 64 MiB of memory makes brute-forcing the master
/// password expensive while keeping unlock comfortable on a desktop.
pub const KDF_MEM_KIB: u32 = 64 * 1024; // 64 MiB, expressed in KiB
pub const KDF_ITERATIONS: u32 = 3;
pub const KDF_PARALLELISM: u32 = 1;

const MIN_KDF_MEM_KIB: u32 = 8 * 1024;
const MAX_KDF_MEM_KIB: u32 = 256 * 1024;
const MIN_KDF_ITERATIONS: u32 = 1;
const MAX_KDF_ITERATIONS: u32 = 10;
const MIN_KDF_PARALLELISM: u32 = 1;
const MAX_KDF_PARALLELISM: u32 = 4;

/// Argon2id parameters, persisted with the vault so unlock uses the SAME cost the
/// vault was created with — even if the defaults change in a future build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    pub mem_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl KdfParams {
    /// The current defaults used when creating a new vault.
    pub fn current() -> Self {
        KdfParams {
            mem_kib: KDF_MEM_KIB,
            iterations: KDF_ITERATIONS,
            parallelism: KDF_PARALLELISM,
        }
    }

    /// Serialize as `argon2id$m=...$t=...$p=...` for the vault header.
    pub fn to_label(self) -> String {
        format!(
            "argon2id$m={}$t={}$p={}",
            self.mem_kib, self.iterations, self.parallelism
        )
    }

    /// Parse a label written by [`KdfParams::to_label`]. Rejects unknown formats
    /// so an old/foreign vault fails loudly rather than silently using wrong cost.
    pub fn from_label(label: &str) -> AppResult<Self> {
        let rest = label
            .strip_prefix("argon2id$")
            .ok_or_else(|| AppError::Db("unsupported KDF (expected argon2id)".into()))?;
        let mut mem = None;
        let mut iters = None;
        let mut par = None;
        for part in rest.split('$') {
            if let Some(v) = part.strip_prefix("m=") {
                mem = v.parse().ok();
            } else if let Some(v) = part.strip_prefix("t=") {
                iters = v.parse().ok();
            } else if let Some(v) = part.strip_prefix("p=") {
                par = v.parse().ok();
            }
        }
        match (mem, iters, par) {
            (Some(mem_kib), Some(iterations), Some(parallelism)) => {
                let params = KdfParams {
                    mem_kib,
                    iterations,
                    parallelism,
                };
                params.validate()?;
                Ok(params)
            }
            _ => Err(AppError::Db(format!("malformed KDF params: {label}"))),
        }
    }

    fn validate(self) -> AppResult<()> {
        if !(MIN_KDF_MEM_KIB..=MAX_KDF_MEM_KIB).contains(&self.mem_kib) {
            return Err(AppError::Db(format!(
                "KDF memory cost is outside supported bounds: {} KiB",
                self.mem_kib
            )));
        }
        if !(MIN_KDF_ITERATIONS..=MAX_KDF_ITERATIONS).contains(&self.iterations) {
            return Err(AppError::Db(format!(
                "KDF iteration count is outside supported bounds: {}",
                self.iterations
            )));
        }
        if !(MIN_KDF_PARALLELISM..=MAX_KDF_PARALLELISM).contains(&self.parallelism) {
            return Err(AppError::Db(format!(
                "KDF parallelism is outside supported bounds: {}",
                self.parallelism
            )));
        }
        Ok(())
    }
}

/// Fill a buffer with cryptographically secure random bytes.
fn fill_random(buf: &mut [u8]) -> AppResult<()> {
    OsRng
        .try_fill_bytes(buf)
        .map_err(|e| AppError::Crypto(format!("RNG failure: {e}")))
}

/// Generate a fresh random 16-byte salt.
pub fn random_salt() -> AppResult<[u8; SALT_LEN]> {
    let mut salt = [0u8; SALT_LEN];
    fill_random(&mut salt)?;
    Ok(salt)
}

/// Generate a fresh random 24-byte nonce.
pub fn random_nonce() -> AppResult<[u8; NONCE_LEN]> {
    let mut nonce = [0u8; NONCE_LEN];
    fill_random(&mut nonce)?;
    Ok(nonce)
}

/// Generate a fresh random 32-byte key (used for the vault key).
pub fn random_key() -> AppResult<[u8; KEY_LEN]> {
    let mut key = [0u8; KEY_LEN];
    fill_random(&mut key)?;
    Ok(key)
}

/// Derive a 32-byte wrapping key from the master password and salt using Argon2id
/// with the given parameters (so unlock can re-derive with the vault's stored cost).
///
/// The returned key is sensitive; callers must zeroize it when done (the wider
/// `VaultKey`/unlock machinery does this).
pub fn derive_wrapping_key(
    password: &[u8],
    salt: &[u8],
    kdf: KdfParams,
) -> AppResult<[u8; KEY_LEN]> {
    let params = Params::new(kdf.mem_kib, kdf.iterations, kdf.parallelism, Some(KEY_LEN))
        .map_err(|e| AppError::Crypto(format!("invalid KDF params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut out = [0u8; KEY_LEN];
    argon
        .hash_password_into(password, salt, &mut out)
        .map_err(|e| AppError::Crypto(format!("key derivation failed: {e}")))?;
    Ok(out)
}

/// Encrypt `plaintext` under `key` with a fresh random nonce and the given
/// associated data. Returns `(nonce, ciphertext)`. The ciphertext includes the
/// Poly1305 authentication tag.
pub fn encrypt(key: &[u8; KEY_LEN], plaintext: &[u8], aad: &[u8]) -> AppResult<(Vec<u8>, Vec<u8>)> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    let nonce_bytes = random_nonce()?;
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| AppError::Crypto("encryption failed".into()))?;
    Ok((nonce_bytes.to_vec(), ciphertext))
}

/// Decrypt `ciphertext` produced by [`encrypt`]. The `aad` must match exactly or
/// decryption fails (authentication error).
pub fn decrypt(
    key: &[u8; KEY_LEN],
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> AppResult<Vec<u8>> {
    if nonce.len() != NONCE_LEN {
        return Err(AppError::Crypto("invalid nonce length".into()));
    }
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = XNonce::from_slice(nonce);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        // An auth failure here almost always means the wrong key (wrong password)
        // or tampering; surface it as a crypto error and let callers decide.
        .map_err(|_| AppError::Crypto("decryption failed (wrong key or corrupted data)".into()))
}

/// Wrap (encrypt) the 32-byte vault key under the wrapping key. Returns
/// `(nonce, wrapped_key)`. Bound to the constant AAD `"vault-key"` so a wrapped
/// key can't be confused with a field ciphertext.
pub fn wrap_vault_key(
    wrapping_key: &[u8; KEY_LEN],
    vault_key: &[u8; KEY_LEN],
) -> AppResult<(Vec<u8>, Vec<u8>)> {
    encrypt(wrapping_key, vault_key, b"vault-key")
}

/// Unwrap (decrypt) the vault key. Returns [`AppError::BadPassword`] on an auth
/// failure, since the overwhelmingly common cause is a wrong master password.
pub fn unwrap_vault_key(
    wrapping_key: &[u8; KEY_LEN],
    nonce: &[u8],
    wrapped: &[u8],
) -> AppResult<[u8; KEY_LEN]> {
    let mut plain =
        decrypt(wrapping_key, nonce, wrapped, b"vault-key").map_err(|_| AppError::BadPassword)?;
    if plain.len() != KEY_LEN {
        plain.zeroize();
        return Err(AppError::Crypto("unwrapped key has wrong length".into()));
    }
    let mut key = [0u8; KEY_LEN];
    key.copy_from_slice(&plain);
    plain.zeroize();
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = random_key().unwrap();
        let msg = b"super-secret-api-key";
        let aad = b"proj|svc|field";
        let (nonce, ct) = encrypt(&key, msg, aad).unwrap();
        let pt = decrypt(&key, &nonce, &ct, aad).unwrap();
        assert_eq!(pt, msg);
    }

    #[test]
    fn aad_mismatch_fails() {
        let key = random_key().unwrap();
        let (nonce, ct) = encrypt(&key, b"x", b"aad-a").unwrap();
        assert!(decrypt(&key, &nonce, &ct, b"aad-b").is_err());
    }

    #[test]
    fn wrong_key_fails() {
        let key = random_key().unwrap();
        let other = random_key().unwrap();
        let (nonce, ct) = encrypt(&key, b"x", b"").unwrap();
        assert!(decrypt(&other, &nonce, &ct, b"").is_err());
    }

    #[test]
    fn vault_key_wrap_roundtrip() {
        let pw = b"correct horse battery staple";
        let salt = random_salt().unwrap();
        let wrapping = derive_wrapping_key(pw, &salt, KdfParams::current()).unwrap();
        let vault_key = random_key().unwrap();
        let (nonce, wrapped) = wrap_vault_key(&wrapping, &vault_key).unwrap();
        let recovered = unwrap_vault_key(&wrapping, &nonce, &wrapped).unwrap();
        assert_eq!(recovered, vault_key);
    }

    #[test]
    fn kdf_params_label_roundtrip() {
        let p = KdfParams::current();
        assert_eq!(KdfParams::from_label(&p.to_label()).unwrap(), p);
        assert!(KdfParams::from_label("scrypt$x=1").is_err());
    }

    #[test]
    fn wrong_password_reports_bad_password() {
        let salt = random_salt().unwrap();
        let good = derive_wrapping_key(b"right", &salt, KdfParams::current()).unwrap();
        let bad = derive_wrapping_key(b"wrong", &salt, KdfParams::current()).unwrap();
        let vault_key = random_key().unwrap();
        let (nonce, wrapped) = wrap_vault_key(&good, &vault_key).unwrap();
        match unwrap_vault_key(&bad, &nonce, &wrapped) {
            Err(AppError::BadPassword) => {}
            other => panic!("expected BadPassword, got {other:?}"),
        }
    }
}
