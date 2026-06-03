//! Managed application state.
//!
//! Holds the SQLite connection and the unlocked vault key behind a single mutex.
//! The vault key is `None` while locked and is zeroized (via [`VaultKey`]'s `Drop`)
//! whenever it's replaced or cleared. Nothing here is ever exposed to the frontend.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::pairing::PairingStore;
use crate::vault::VaultKey;

/// The inner, mutex-guarded state.
pub struct Inner {
    pub conn: Connection,
    pub key: Option<VaultKey>,
}

/// The Tauri-managed state. All access goes through the mutex.
pub struct AppState {
    pub inner: Mutex<Inner>,
    pub db_path: PathBuf,
    pub pairing: Arc<PairingStore>,
}

impl AppState {
    /// Open the database at `db_path` and start locked.
    pub fn new(db_path: &Path) -> AppResult<Self> {
        let conn = crate::db::open(db_path)?;
        crate::db::repo::check_schema(&conn)?;
        Ok(AppState {
            db_path: db_path.to_path_buf(),
            pairing: Arc::new(PairingStore::default()),
            inner: Mutex::new(Inner { conn, key: None }),
        })
    }

    /// Run a closure with locked access to the inner state.
    pub fn with<T>(&self, f: impl FnOnce(&mut Inner) -> AppResult<T>) -> AppResult<T> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| AppError::Other("state mutex poisoned".into()))?;
        f(&mut guard)
    }

    /// Run a closure that requires an unlocked vault, providing the connection and key.
    pub fn with_unlocked<T>(
        &self,
        f: impl FnOnce(&Connection, &VaultKey) -> AppResult<T>,
    ) -> AppResult<T> {
        self.with(|inner| {
            let key = inner.key.as_ref().ok_or(AppError::Locked)?;
            f(&inner.conn, key)
        })
    }
}
