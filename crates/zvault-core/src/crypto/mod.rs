//! Cryptographic primitives for ZVault.
//!
//! **M1 implementation target.** This module will provide:
//!
//! - [`derive_key`] — Argon2id (RFC 9106) memory-hard KDF: derives a 256-bit vault
//!   master key from the user's master password and a random 32-byte salt.
//!   Parameters: `m_cost` = 64 MiB, `t_cost` = 3, `p_cost` = 4 (adjustable).
//!
//! - [`encrypt`] / [`decrypt`] — AES-256-GCM authenticated encryption of the vault
//!   payload. A fresh 96-bit random IV is generated on every write. The
//!   on-disk format is:
//!   ```text
//!   [magic: 8 bytes "ZVAULT01"]
//!   [kdf_params: 64 bytes — Argon2id salt + params]
//!   [encrypted_payload: N bytes]
//!   [auth_tag: 16 bytes]
//!   ```
//!
//! - [`VaultKey`] — a newtype wrapping `Zeroizing<[u8; 32]>` so the key material
//!   is always zeroed on drop.
//!
//! All sensitive material is zeroed via the [`zeroize`] crate.

#![allow(dead_code)]

use zeroize::Zeroizing;

use crate::Result;

/// Magic bytes at the start of every ZVault encrypted file.
pub const MAGIC: &[u8; 8] = b"ZVAULT01";

/// A 256-bit vault master key. The inner bytes are zeroed automatically on drop
/// via [`Zeroizing`].
pub struct VaultKey(pub Zeroizing<[u8; 32]>);

impl Drop for VaultKey {
    fn drop(&mut self) {
        // Zeroizing<_> handles the actual zeroing; this impl is here as an
        // explicit reminder that key material must not outlive its scope.
    }
}

/// Derive a [`VaultKey`] from a master password and a 32-byte random salt.
///
/// Uses Argon2id with high-memory parameters (m=64 MiB, t=3, p=4).
///
/// # Errors
///
/// Returns [`crate::Error::Crypto`] if the KDF fails.
pub fn derive_key(_password: &str, _salt: &[u8; 32]) -> Result<VaultKey> {
    todo!("M1")
}

/// Encrypt `plaintext` with `key` using AES-256-GCM.
///
/// Returns the full on-disk blob: magic header + KDF params + ciphertext +
/// auth tag.
///
/// # Errors
///
/// Returns [`crate::Error::Crypto`] if encryption fails.
pub fn encrypt(_key: &VaultKey, _plaintext: &[u8]) -> Result<Vec<u8>> {
    todo!("M1")
}

/// Decrypt a vault blob produced by [`encrypt`].
///
/// Verifies the magic header, re-derives internal state from the stored KDF
/// params, and authenticates + decrypts the payload.
///
/// # Errors
///
/// Returns [`crate::Error::InvalidVaultFile`] on header/tag mismatch, or
/// [`crate::Error::Crypto`] on decryption failure.
pub fn decrypt(_key: &VaultKey, _ciphertext: &[u8]) -> Result<Vec<u8>> {
    todo!("M1")
}
