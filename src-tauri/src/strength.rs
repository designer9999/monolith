//! Password-strength estimation.
//!
//! A small heuristic score (0–100) ported from the MONOLITH design's `estimate()`
//! so the strength bars render identically. This runs on decrypted values inside
//! Rust and only the resulting score crosses to the frontend — never the value.

/// Estimate the strength of a password-like value on a 0–100 scale.
pub fn estimate(v: &str) -> u8 {
    if v.is_empty() {
        return 0;
    }
    let mut s: i32 = (v.chars().count() as i32 * 4).min(48);
    if v.chars().any(|c| c.is_ascii_uppercase()) {
        s += 10;
    }
    if v.chars().any(|c| c.is_ascii_lowercase()) {
        s += 5;
    }
    if v.chars().any(|c| c.is_ascii_digit()) {
        s += 10;
    }
    if v.chars().any(|c| !c.is_ascii_alphanumeric()) {
        s += 16;
    }
    if v.chars().all(|c| c.is_ascii_lowercase()) {
        s -= 24;
    }
    if v.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && v.chars().count() < 14
    {
        s -= 10;
    }
    s.clamp(2, 100) as u8
}

/// Whether a field should count toward *password* health: a genuine
/// human-chosen password / passphrase / PIN — NOT machine-generated secrets like
/// API keys, tokens, or CVVs (whose "strength" is not user-controlled and would
/// skew the vault-health score). Also excludes obvious structured blobs.
pub fn is_password_like(label: &str, value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    let l = label.to_ascii_lowercase();
    let is_password_field = l.contains("password") || l.contains("passphrase") || l.contains("pin");
    let structured =
        value.starts_with("ssh-") || value.starts_with("eyJ") || value.starts_with("-----");
    is_password_field && !structured
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weak_lowercase_scores_low() {
        assert!(estimate("password") < 45);
    }

    #[test]
    fn strong_mixed_scores_high() {
        assert!(estimate("Gr@nite-Harbor-71!") >= 75);
    }

    #[test]
    fn empty_is_zero() {
        assert_eq!(estimate(""), 0);
    }

    #[test]
    fn only_real_password_fields_are_scored() {
        // Genuine passwords / passphrases / PINs are scored.
        assert!(is_password_like("Database Password", "hunter2!"));
        assert!(is_password_like("SSH Passphrase", "fsn1-PASS-key-9#"));
        // Machine secrets are NOT (their "strength" isn't user-chosen).
        assert!(!is_password_like("API Key", "sk-proj-aA1bB2cC3"));
        assert!(!is_password_like(
            "Personal Access Token",
            "github_pat_11AABB"
        ));
        assert!(!is_password_like("CVV", "441"));
        assert!(!is_password_like("Service Role Key", "eyJhbGciOi"));
        // Structured blobs that happen to sit in a password field are excluded.
        assert!(!is_password_like(
            "Private Key Passphrase",
            "-----BEGIN-----"
        ));
    }
}
