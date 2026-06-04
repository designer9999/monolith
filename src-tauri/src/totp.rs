//! RFC 6238 TOTP generation.
//!
//! Real HMAC-SHA1 time-based codes (the design used a deterministic stand-in).
//! The base32 shared secret is stored *encrypted* like any other secret and only
//! decrypted in memory to generate a code on demand — the raw secret never
//! crosses to the frontend, only the 6-digit code and seconds remaining.

use totp_rs::{Algorithm, Secret, TOTP};

use crate::error::{AppError, AppResult};

/// Default TOTP parameters (the near-universal Google-Authenticator convention).
const DIGITS: usize = 6;
const SKEW: u8 = 1;
pub const PERIOD: u64 = 30;

/// Build a [`TOTP`] from a base32 secret.
///
/// Uses `new_unchecked` so short demo/seed secrets are accepted; real secrets
/// added by the user are validated for base32 correctness here (a parse error is
/// surfaced) but not rejected for length, matching how authenticator apps behave.
fn build(secret_base32: &str) -> AppResult<TOTP> {
    let secret = normalize_secret(secret_base32);
    if secret.is_empty() {
        return Err(AppError::Invalid("TOTP secret is required".into()));
    }
    let bytes = Secret::Encoded(secret)
        .to_bytes()
        .map_err(|_| AppError::Invalid("TOTP secret is not valid base32".into()))?;
    Ok(TOTP::new_unchecked(
        Algorithm::SHA1,
        DIGITS,
        SKEW,
        PERIOD,
        bytes,
    ))
}

fn normalize_secret(secret_base32: &str) -> String {
    secret_base32
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_whitespace() || ch == '-' {
                None
            } else {
                Some(ch.to_ascii_uppercase())
            }
        })
        .collect()
}

/// Validate that a string is a usable base32 TOTP secret.
pub fn validate_secret(secret_base32: &str) -> AppResult<()> {
    build(secret_base32).map(|_| ())
}

/// Generate the current code and the seconds remaining in the step.
pub fn generate(secret_base32: &str) -> AppResult<(String, u32)> {
    let totp = build(secret_base32)?;
    let code = totp
        .generate_current()
        .map_err(|e| AppError::Other(format!("system time error: {e}")))?;
    let remaining =
        totp.ttl()
            .map_err(|e| AppError::Other(format!("system time error: {e}")))? as u32;
    Ok((code, remaining))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_six_digit_code() {
        // A standard 16-char base32 secret.
        let (code, remaining) = generate("JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
        assert!(remaining >= 1 && remaining <= PERIOD as u32);
    }

    #[test]
    fn rejects_non_base32() {
        assert!(generate("not base32 !!!").is_err());
    }

    #[test]
    fn accepts_spaced_lowercase_base32_secret() {
        let compact = "VCXI3KVPODGSF5XXHI7ADQ7A6WFRL2AU";
        let spaced = "vcxi 3kvp odgs f5xx hi7a dq7a 6wfr l2au";

        assert_eq!(normalize_secret(spaced), compact);
        validate_secret(spaced).unwrap();
    }
}
