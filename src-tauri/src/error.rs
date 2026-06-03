//! Application-wide error type.
//!
//! Every Tauri command returns `Result<T, AppError>`. `AppError` serializes to a
//! tagged `{ kind, message }` object so the frontend can branch on `err.kind`
//! while still showing a human-readable `message`.

use serde::Serialize;

/// The result type used throughout the vault core and command layer.
pub type AppResult<T> = Result<T, AppError>;

/// A categorized application error. The `kind` is a stable, machine-readable
/// discriminator; the `message` is human-readable and safe to display.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The vault is locked — unlock before performing this operation.
    #[error("Vault is locked")]
    Locked,

    /// The supplied master password was wrong (authentication tag mismatch).
    #[error("Incorrect master password")]
    BadPassword,

    /// A vault already exists / does not exist when the operation required the opposite.
    #[error("{0}")]
    VaultState(String),

    /// A requested entity (project, service, field, …) was not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// Input failed validation in Rust.
    #[error("{0}")]
    Invalid(String),

    /// A cryptographic operation failed.
    #[error("Cryptography error: {0}")]
    Crypto(String),

    /// A database / storage error.
    #[error("Storage error: {0}")]
    Db(String),

    /// Any other I/O or unexpected error.
    #[error("{0}")]
    Other(String),
}

impl AppError {
    /// Stable machine-readable discriminator mirrored by the TypeScript `AppError` type.
    fn kind(&self) -> &'static str {
        match self {
            AppError::Locked => "locked",
            AppError::BadPassword => "badPassword",
            AppError::VaultState(_) => "vaultState",
            AppError::NotFound(_) => "notFound",
            AppError::Invalid(_) => "invalid",
            AppError::Crypto(_) => "crypto",
            AppError::Db(_) => "db",
            AppError::Other(_) => "other",
        }
    }
}

/// Serialize as `{ "kind": "...", "message": "..." }` so the JS promise rejection
/// carries a structured, switchable error.
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

// --- conversions from lower-level errors (kept lossy-but-safe: no secrets in messages) ---

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Db(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Other(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Other(e.to_string())
    }
}
