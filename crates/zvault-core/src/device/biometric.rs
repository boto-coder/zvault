//! Biometric unlock helpers for ZVault.
//!
//! ## Overview
//!
//! Biometric unlock allows the user to unlock their vault without re-entering
//! the master password.  The flow works as follows:
//!
//! 1. **Enable:** The vault master key (`VaultKey`) is encrypted with an
//!    OS-enclave-bound key (provided by the biometric subsystem) and stored as
//!    a [`BiometricUnlockConfig`].
//!
//! 2. **Unlock:** On biometric authentication success, the OS releases the
//!    enclave key.  The wrapped vault key is decrypted and returned as a
//!    `VaultKey`, resuming the session without Argon2id derivation.
//!
//! 3. **Disable:** The stored config is deleted from secure storage.
//!
//! ## Security model
//!
//! The enclave key (`[u8; 32]`) is assumed to be:
//! - Generated and protected by the OS secure enclave / TEE / Keystore.
//! - Released only after successful biometric authentication.
//! - Never exposed to user-space in plaintext outside the unlock moment.
//!
//! The wrapped vault key uses AES-256-GCM with a fresh random 12-byte IV per
//! enable operation.  The IV is stored alongside the ciphertext in
//! [`BiometricUnlockConfig`].  The GCM tag authenticates the ciphertext,
//! preventing tampering.
//!
//! ## Platform integration
//!
//! This module provides the *cryptographic* wrapping/unwrapping only.  The
//! actual biometric prompt and enclave key retrieval are platform-specific:
//!
//! - **macOS:** `SecAccessControl` with `.biometryCurrentSet` via Tauri plugin
//! - **Windows:** Windows Hello via Tauri plugin
//! - **Android:** `BiometricPrompt` + Android Keystore biometric-bound key
//! - **Linux:** libsecret (no biometric; falls back to system unlock)

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng as AeadOsRng},
    Aes256Gcm, Key, Nonce,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::VaultKey;
use crate::{Error, Result};

use super::SecureStorage;

/// Size of the AES-GCM nonce used for wrapping.
const NONCE_LEN: usize = 12;

/// Storage key prefix for biometric unlock configs.
const BIOMETRIC_KEY_PREFIX: &str = "zvault/biometric";

/// Configuration produced by [`enable_biometric`], containing the wrapped
/// vault key and the IV used for wrapping.
///
/// This struct is persisted in secure storage.  It does **not** contain the
/// enclave key — that is held exclusively by the OS and released only on
/// biometric authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiometricUnlockConfig {
    /// AES-256-GCM ciphertext of the 32-byte vault key (32 bytes ct + 16 bytes tag).
    pub wrapped_vault_key: Vec<u8>,
    /// The 12-byte IV / nonce used for the wrapping encryption.
    pub iv: [u8; NONCE_LEN],
}

/// Encrypt the vault master key with the OS-enclave-bound key, producing a
/// [`BiometricUnlockConfig`] that can be stored in secure storage.
///
/// # Arguments
///
/// - `vault_key` — the active vault master key to wrap.
/// - `enclave_key` — the 32-byte key released by the OS after biometric auth.
///
/// # Returns
///
/// A [`BiometricUnlockConfig`] containing the wrapped key and IV.  The caller
/// is responsible for persisting this config (e.g. via [`SecureStorage`]).
///
/// # Panics
///
/// Panics if AES-256-GCM encryption fails (should never happen with valid
/// inputs; the only failure mode is an invalid key length, which is prevented
/// by the `[u8; 32]` type).
#[must_use]
pub fn enable_biometric(vault_key: &VaultKey, enclave_key: &[u8; 32]) -> BiometricUnlockConfig {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(enclave_key));
    let nonce = Aes256Gcm::generate_nonce(&mut AeadOsRng);

    // Encrypt the 32-byte vault key.
    let wrapped = cipher
        .encrypt(&nonce, vault_key.as_bytes().as_ref())
        .expect("AES-256-GCM encryption with valid key must not fail");

    let mut iv = [0u8; NONCE_LEN];
    iv.copy_from_slice(nonce.as_slice());

    BiometricUnlockConfig {
        wrapped_vault_key: wrapped,
        iv,
    }
}

/// Decrypt the wrapped vault key using the OS-enclave-bound key, returning
/// the original [`VaultKey`].
///
/// # Arguments
///
/// - `config` — the [`BiometricUnlockConfig`] previously produced by
///   [`enable_biometric`].
/// - `enclave_key` — the 32-byte key released by the OS after biometric auth.
///
/// # Errors
///
/// Returns [`Error::Crypto`] if decryption fails (wrong enclave key, tampered
/// ciphertext, or corrupted config).
pub fn unlock_biometric(
    config: &BiometricUnlockConfig,
    enclave_key: &[u8; 32],
) -> Result<VaultKey> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(enclave_key));
    let nonce = Nonce::from_slice(&config.iv);

    let plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(
        cipher
            .decrypt(nonce, config.wrapped_vault_key.as_ref())
            .map_err(|_| {
                Error::Crypto(
                    "biometric unlock failed: decryption error (wrong enclave key or tampered data)"
                        .into(),
                )
            })?,
    );

    // The plaintext must be exactly 32 bytes (the vault key).
    if plaintext.len() != 32 {
        return Err(Error::Crypto(format!(
            "biometric unlock: unexpected key length {} (expected 32)",
            plaintext.len()
        )));
    }

    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&plaintext);
    let vault_key = VaultKey::from_bytes(Zeroizing::new(key_bytes));

    Ok(vault_key)
}

/// Remove biometric unlock configuration from secure storage for the given
/// device.
///
/// # Arguments
///
/// - `storage` — the [`SecureStorage`] backend holding the config.
/// - `device_id` — the device whose biometric config should be removed.
///
/// # Errors
///
/// Returns an error if the config does not exist or deletion fails.
pub fn disable_biometric(storage: &dyn SecureStorage, device_id: Uuid) -> Result<()> {
    let key = biometric_storage_key(device_id);
    storage.delete(&key)
}

/// Store a [`BiometricUnlockConfig`] in secure storage for the given device.
///
/// # Errors
///
/// Returns an error if serialisation or storage fails.
pub fn store_biometric_config(
    storage: &dyn SecureStorage,
    device_id: Uuid,
    config: &BiometricUnlockConfig,
) -> Result<()> {
    let key = biometric_storage_key(device_id);
    let serialised = serde_json::to_vec(config)
        .map_err(|e| Error::Serialisation(format!("biometric config serialisation: {e}")))?;
    storage.store(&key, &serialised)
}

/// Load a [`BiometricUnlockConfig`] from secure storage for the given device.
///
/// # Errors
///
/// Returns an error if the config is not found or deserialisation fails.
pub fn load_biometric_config(
    storage: &dyn SecureStorage,
    device_id: Uuid,
) -> Result<BiometricUnlockConfig> {
    let key = biometric_storage_key(device_id);
    let data = storage.load(&key)?;
    serde_json::from_slice(&data)
        .map_err(|e| Error::Serialisation(format!("biometric config deserialisation: {e}")))
}

/// Return the secure-storage key for a device's biometric unlock config.
fn biometric_storage_key(device_id: Uuid) -> String {
    format!("{BIOMETRIC_KEY_PREFIX}/{device_id}/wrapped_key")
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::InMemoryStorage;

    /// Helper: create a VaultKey from a known byte pattern.
    fn test_vault_key(seed: u8) -> VaultKey {
        VaultKey::from_bytes(Zeroizing::new([seed; 32]))
    }

    /// Helper: create an enclave key from a known byte pattern.
    fn test_enclave_key(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    #[test]
    fn enable_unlock_roundtrip() {
        let vault_key = test_vault_key(0xAB);
        let enclave_key = test_enclave_key(0xCD);

        let config = enable_biometric(&vault_key, &enclave_key);

        // Config should contain a wrapped key (32 bytes plaintext + 16 bytes GCM tag = 48 bytes).
        assert_eq!(
            config.wrapped_vault_key.len(),
            48,
            "wrapped key should be 48 bytes (32 + 16 tag)"
        );
        assert_eq!(config.iv.len(), 12, "IV should be 12 bytes");

        // Unlock should recover the same vault key.
        let recovered = unlock_biometric(&config, &enclave_key).expect("unlock should succeed");
        assert_eq!(
            recovered.as_bytes(),
            vault_key.as_bytes(),
            "recovered key must match original"
        );
    }

    #[test]
    fn unlock_with_wrong_enclave_key_fails() {
        let vault_key = test_vault_key(0xAB);
        let enclave_key = test_enclave_key(0xCD);
        let wrong_key = test_enclave_key(0xEF);

        let config = enable_biometric(&vault_key, &enclave_key);

        let result = unlock_biometric(&config, &wrong_key);
        assert!(result.is_err(), "unlock with wrong key must fail");

        let err = result.unwrap_err();
        assert!(
            matches!(err, Error::Crypto(_)),
            "error must be Crypto variant"
        );
    }

    #[test]
    fn unlock_with_tampered_ciphertext_fails() {
        let vault_key = test_vault_key(0xAB);
        let enclave_key = test_enclave_key(0xCD);

        let mut config = enable_biometric(&vault_key, &enclave_key);

        // Tamper with the wrapped key.
        if let Some(byte) = config.wrapped_vault_key.first_mut() {
            *byte ^= 0xFF;
        }

        let result = unlock_biometric(&config, &enclave_key);
        assert!(result.is_err(), "unlock with tampered data must fail");
    }

    #[test]
    fn disable_clears_config_from_storage() {
        let storage = InMemoryStorage::default();
        let device_id = Uuid::new_v4();

        let vault_key = test_vault_key(0xAB);
        let enclave_key = test_enclave_key(0xCD);
        let config = enable_biometric(&vault_key, &enclave_key);

        // Store the config.
        store_biometric_config(&storage, device_id, &config).expect("store should succeed");

        // Verify it's loadable.
        let loaded = load_biometric_config(&storage, device_id);
        assert!(loaded.is_ok(), "config should be loadable after store");

        // Disable (delete).
        disable_biometric(&storage, device_id).expect("disable should succeed");

        // Verify it's gone.
        let result = load_biometric_config(&storage, device_id);
        assert!(result.is_err(), "config should not exist after disable");
    }

    #[test]
    fn disable_nonexistent_returns_error() {
        let storage = InMemoryStorage::default();
        let device_id = Uuid::new_v4();

        let result = disable_biometric(&storage, device_id);
        assert!(
            result.is_err(),
            "disabling non-existent config should error"
        );
    }

    #[test]
    fn store_load_roundtrip() {
        let storage = InMemoryStorage::default();
        let device_id = Uuid::new_v4();

        let vault_key = test_vault_key(0x42);
        let enclave_key = test_enclave_key(0x99);
        let config = enable_biometric(&vault_key, &enclave_key);

        store_biometric_config(&storage, device_id, &config).expect("store should succeed");

        let loaded = load_biometric_config(&storage, device_id).expect("load should succeed");

        assert_eq!(loaded.wrapped_vault_key, config.wrapped_vault_key);
        assert_eq!(loaded.iv, config.iv);

        // Full roundtrip: load config then unlock.
        let recovered = unlock_biometric(&loaded, &enclave_key).expect("unlock from loaded config");
        assert_eq!(recovered.as_bytes(), vault_key.as_bytes());
    }

    #[test]
    fn each_enable_produces_unique_iv() {
        let vault_key = test_vault_key(0xAB);
        let enclave_key = test_enclave_key(0xCD);

        let config1 = enable_biometric(&vault_key, &enclave_key);
        let config2 = enable_biometric(&vault_key, &enclave_key);

        // IVs should differ (fresh random per enable).
        assert_ne!(
            config1.iv, config2.iv,
            "each enable must produce a unique IV"
        );
        // Ciphertexts should also differ (different IV → different ciphertext).
        assert_ne!(config1.wrapped_vault_key, config2.wrapped_vault_key);
    }
}
