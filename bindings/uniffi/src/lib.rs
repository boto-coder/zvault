//! UniFFI bindings for `zvault-core`.
//!
//! This crate exposes a C-compatible shared library (`cdylib`) that UniFFI
//! uses to auto-generate Kotlin (Android) and Swift (iOS) bindings.
//!
//! The API surface is intentionally minimal: vault open/create/save and
//! item CRUD, with items exchanged as JSON strings. This keeps the FFI
//! boundary simple and avoids exposing complex Rust types across the bridge.

// Allow clippy lints in UniFFI-generated scaffolding code that we cannot control.
#![allow(clippy::empty_line_after_doc_comments)]

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use uuid::Uuid;
use zvault_core::crypto::VaultKey;
use zvault_core::vault::{Vault, VaultFile, VaultItem};

uniffi::include_scaffolding!("zvault");

// ─── Error mapping ───────────────────────────────────────────────────────────

/// FFI error type matching the UDL `ZVaultError` enum.
#[derive(Debug, thiserror::Error)]
pub enum ZVaultError {
    /// A cryptographic operation failed.
    #[error("crypto error: {0}")]
    CryptoError(String),
    /// An I/O error.
    #[error("io error: {0}")]
    IoError(String),
    /// Serialisation/deserialisation failure.
    #[error("serialisation error: {0}")]
    SerialisationError(String),
    /// Invalid vault file (bad magic, truncated, wrong password).
    #[error("invalid vault file: {0}")]
    InvalidVaultFile(String),
    /// Item not found.
    #[error("item not found: {0}")]
    ItemNotFound(String),
    /// Invalid input from the caller.
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

impl From<zvault_core::Error> for ZVaultError {
    fn from(e: zvault_core::Error) -> Self {
        match e {
            zvault_core::Error::Crypto(msg) => ZVaultError::CryptoError(msg),
            zvault_core::Error::Io(err) => ZVaultError::IoError(err.to_string()),
            zvault_core::Error::Serialisation(msg) => ZVaultError::SerialisationError(msg),
            zvault_core::Error::InvalidVaultFile(msg) => ZVaultError::InvalidVaultFile(msg),
            zvault_core::Error::ItemNotFound(id) => ZVaultError::ItemNotFound(id.to_string()),
            zvault_core::Error::DeviceNotFound(id) => {
                ZVaultError::ItemNotFound(format!("device {id}"))
            }
            zvault_core::Error::DeviceRevoked(id) => {
                ZVaultError::InvalidInput(format!("device revoked: {id}"))
            }
            zvault_core::Error::SyncError(msg) => ZVaultError::IoError(msg),
            zvault_core::Error::Utf8(err) => ZVaultError::SerialisationError(err.to_string()),
            zvault_core::Error::Base64(err) => ZVaultError::SerialisationError(err.to_string()),
        }
    }
}

// ─── Handle registry ─────────────────────────────────────────────────────────

/// Internal state for an open vault session.
struct VaultSession {
    vault_file: VaultFile,
    key: VaultKey,
    vault: Vault,
}

/// Global registry mapping handle IDs to vault sessions.
///
/// We use a simple incrementing counter as handle IDs. The registry is
/// protected by a `Mutex` for thread safety across FFI calls.
fn registry() -> &'static Mutex<HashMap<u64, VaultSession>> {
    static INSTANCE: OnceLock<Mutex<HashMap<u64, VaultSession>>> = OnceLock::new();
    INSTANCE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_handle_id() -> u64 {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

// ─── UDL dictionary ──────────────────────────────────────────────────────────

/// Opaque handle passed to the foreign side.
///
/// The `id` field is used to look up the vault session in the global registry.
pub struct VaultHandle {
    pub id: u64,
}

// ─── Namespace functions ─────────────────────────────────────────────────────

/// Create a new vault at `path`, encrypted with `password`.
///
/// Returns a [`VaultHandle`] that must be passed to subsequent operations.
pub fn create_vault(password: String, path: String) -> Result<VaultHandle, ZVaultError> {
    let (vault_file, key) = VaultFile::create(&password, &path)?;

    let vault = Vault::new();

    let id = next_handle_id();
    registry().lock().expect("registry lock poisoned").insert(
        id,
        VaultSession {
            vault_file,
            key,
            vault,
        },
    );

    Ok(VaultHandle { id })
}

/// Open an existing vault at `path` with `password`.
///
/// Returns a [`VaultHandle`] that must be passed to subsequent operations.
pub fn open_vault(password: String, path: String) -> Result<VaultHandle, ZVaultError> {
    let (vault_file, key, vault) = VaultFile::open(&password, &path)?;

    let id = next_handle_id();
    registry().lock().expect("registry lock poisoned").insert(
        id,
        VaultSession {
            vault_file,
            key,
            vault,
        },
    );

    Ok(VaultHandle { id })
}

/// Save the current vault state to disk.
pub fn save_vault(handle: &VaultHandle) -> Result<(), ZVaultError> {
    let reg = registry().lock().expect("registry lock poisoned");
    let session = reg
        .get(&handle.id)
        .ok_or_else(|| ZVaultError::InvalidInput("invalid handle".to_string()))?;
    session.vault_file.save(&session.key, &session.vault)?;
    Ok(())
}

/// List all vault items as a JSON array string.
pub fn list_items(handle: &VaultHandle) -> Result<String, ZVaultError> {
    let reg = registry().lock().expect("registry lock poisoned");
    let session = reg
        .get(&handle.id)
        .ok_or_else(|| ZVaultError::InvalidInput("invalid handle".to_string()))?;
    let items = session.vault.list_items();
    let json =
        serde_json::to_string(items).map_err(|e| ZVaultError::SerialisationError(e.to_string()))?;
    Ok(json)
}

/// Get a single item by UUID string, returned as JSON.
pub fn get_item(handle: &VaultHandle, item_id: String) -> Result<String, ZVaultError> {
    let id = Uuid::parse_str(&item_id)
        .map_err(|e| ZVaultError::InvalidInput(format!("invalid UUID: {e}")))?;

    let reg = registry().lock().expect("registry lock poisoned");
    let session = reg
        .get(&handle.id)
        .ok_or_else(|| ZVaultError::InvalidInput("invalid handle".to_string()))?;

    let item = session
        .vault
        .get_item(id)
        .ok_or(ZVaultError::ItemNotFound(id.to_string()))?;
    let json =
        serde_json::to_string(item).map_err(|e| ZVaultError::SerialisationError(e.to_string()))?;
    Ok(json)
}

/// Add a new item to the vault from a JSON string.
///
/// The JSON must deserialise to a valid [`VaultItem`].
pub fn add_item(handle: &VaultHandle, item_json: String) -> Result<(), ZVaultError> {
    let item: VaultItem = serde_json::from_str(&item_json)
        .map_err(|e| ZVaultError::SerialisationError(e.to_string()))?;

    let mut reg = registry().lock().expect("registry lock poisoned");
    let session = reg
        .get_mut(&handle.id)
        .ok_or_else(|| ZVaultError::InvalidInput("invalid handle".to_string()))?;

    session.vault.add_item(item);
    Ok(())
}

/// Delete an item by UUID string.
pub fn delete_item(handle: &VaultHandle, item_id: String) -> Result<(), ZVaultError> {
    let id = Uuid::parse_str(&item_id)
        .map_err(|e| ZVaultError::InvalidInput(format!("invalid UUID: {e}")))?;

    let mut reg = registry().lock().expect("registry lock poisoned");
    let session = reg
        .get_mut(&handle.id)
        .ok_or_else(|| ZVaultError::InvalidInput("invalid handle".to_string()))?;

    session.vault.delete_item(id)?;
    Ok(())
}

/// Close an open vault session, releasing all resources and zeroing key material.
///
/// After this call the handle is invalid; any subsequent calls using it will
/// return `InvalidInput`.  The [`VaultKey`] held in the session is dropped here,
/// triggering `Zeroizing<[u8; 32]>` to overwrite the key bytes with zeros.
pub fn close_vault(handle: &VaultHandle) -> Result<(), ZVaultError> {
    let mut reg = registry().lock().expect("registry lock poisoned");
    reg.remove(&handle.id)
        .ok_or_else(|| ZVaultError::InvalidInput("invalid handle".to_string()))?;
    // VaultSession (including VaultKey) is dropped here → key bytes zeroed.
    Ok(())
}
