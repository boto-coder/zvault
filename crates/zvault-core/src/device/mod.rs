//! Device lifecycle management for ZVault.
//!
//! **M3 implementation target.** This module will provide:
//!
//! - [`generate_device_identity`] — generate a secp256k1 keypair on first run.
//!   The secret key is stored via [`SecureStorage`]; only the public key is
//!   kept in memory.
//! - [`SecureStorage`] — trait abstracting OS-specific secure key storage:
//!   macOS Keychain, Windows Credential Manager, libsecret on Linux, Android
//!   Keystore for the Android build.
//! - [`admit_device`] — construct a signed `VaultInvite` for a new device and
//!   add it to the vault device list.
//! - [`revoke_device`] — tombstone a device in the OR-Set CRDT device list and
//!   rebroadcast the updated vault excluding the revoked device.

use uuid::Uuid;

use crate::{vault::DeviceEntry, Result};

// ─── DeviceIdentity ──────────────────────────────────────────────────────────

/// In-memory representation of this device's identity.
///
/// The corresponding secret key is held in [`SecureStorage`], never in a
/// struct field.
#[derive(Debug, Clone)]
pub struct DeviceIdentity {
    /// Stable random device identifier.
    pub device_id: Uuid,
    /// secp256k1 public key (hex-encoded, 64 chars).
    pub pubkey_hex: String,
}

// ─── SecureStorage ───────────────────────────────────────────────────────────

/// Abstraction over OS-provided secure key storage.
///
/// Implementations:
/// - Desktop: `keyring` crate → macOS Keychain / Windows Credential Manager /
///   libsecret
/// - Android: Android Keystore API via UniFFI
/// - Browser extension: `browser.storage.local` (encrypted; session key in memory)
pub trait SecureStorage: Send + Sync {
    /// Persist `value` bytes under the given `key` in secure storage.
    ///
    /// # Errors
    ///
    /// Returns an error if the OS rejects the store operation.
    fn store(&self, key: &str, value: &[u8]) -> Result<()>;

    /// Load bytes previously stored under `key`.
    ///
    /// # Errors
    ///
    /// Returns an error if the key does not exist or access is denied.
    fn load(&self, key: &str) -> Result<Vec<u8>>;

    /// Delete the entry for `key`.
    ///
    /// # Errors
    ///
    /// Returns an error if the key does not exist or deletion fails.
    fn delete(&self, key: &str) -> Result<()>;
}

// ─── Stubs ───────────────────────────────────────────────────────────────────

/// Generate a new secp256k1 keypair and return the in-memory [`DeviceIdentity`].
///
/// The secret key is stored in `storage`; only the public key is returned.
///
/// # Errors
///
/// Will be implemented in M3.
pub fn generate_device_identity() -> Result<DeviceIdentity> {
    todo!("M3")
}

/// Construct a `VaultInvite` for `invitee_pubkey` signed by `inviter`, add the
/// new device to the vault, and return the resulting [`DeviceEntry`].
///
/// # Errors
///
/// Will be implemented in M3.
pub fn admit_device(_inviter: &DeviceIdentity, _invitee_pubkey: &str) -> Result<DeviceEntry> {
    todo!("M3")
}

/// Revoke a device by tombstoning it in the vault's OR-Set device list.
///
/// Increments the vault version, re-encrypts, and marks the device as revoked.
/// Subsequent sync broadcasts will exclude the revoked device's public key.
///
/// # Errors
///
/// Will be implemented in M3.
pub fn revoke_device(
    _admin: &DeviceIdentity,
    _target_device_id: Uuid,
    _vault: &mut crate::vault::Vault,
) -> Result<()> {
    todo!("M3")
}
