//! [`SecureStorage`] implementation backed by the OS keychain via the `keyring` crate.
//!
//! This provides access to:
//! - macOS Keychain
//! - Windows Credential Manager
//! - Linux libsecret (GNOME Keyring / KDE Wallet)
//!
//! The module is gated behind `cfg(not(target_arch = "wasm32"))` because
//! browser extensions do not have access to OS keychains, and behind the
//! `keyring-storage` feature flag.
//!
//! ## Service name
//!
//! All entries are stored under the service name `"zvault"`.  The key string
//! passed to [`SecureStorage::store`] / [`SecureStorage::load`] /
//! [`SecureStorage::delete`] is used as the "user" field in the keyring entry.

use crate::{Error, Result};

use super::SecureStorage;

/// The fixed service name used for all keyring entries.
const SERVICE_NAME: &str = "zvault";

/// [`SecureStorage`] backend using the OS keychain via the `keyring` crate.
///
/// Each key-value pair is stored as a keyring entry with:
/// - **service:** `"zvault"`
/// - **user:** the storage key string (e.g. `"zvault/device/<uuid>/secret_key"`)
///
/// Values are stored as raw bytes (base64-encoded internally by the keyring
/// crate on platforms that require string storage).
#[derive(Debug, Default)]
pub struct KeyringStorage;

impl KeyringStorage {
    /// Create a new `KeyringStorage` instance.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Build a `keyring::Entry` for the given key.
    fn entry(key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(SERVICE_NAME, key)
            .map_err(|e| Error::Crypto(format!("keyring entry creation failed: {e}")))
    }
}

impl SecureStorage for KeyringStorage {
    fn store(&self, key: &str, value: &[u8]) -> Result<()> {
        let entry = Self::entry(key)?;
        entry
            .set_secret(value)
            .map_err(|e| Error::Crypto(format!("keyring store failed: {e}")))
    }

    fn load(&self, key: &str) -> Result<Vec<u8>> {
        let entry = Self::entry(key)?;
        entry
            .get_secret()
            .map_err(|e| Error::Crypto(format!("keyring load failed: {e}")))
    }

    fn delete(&self, key: &str) -> Result<()> {
        let entry = Self::entry(key)?;
        entry
            .delete_credential()
            .map_err(|e| Error::Crypto(format!("keyring delete failed: {e}")))
    }
}
