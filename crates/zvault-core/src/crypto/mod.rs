//! Cryptographic primitives for ZVault.
//!
//! ## Responsibilities
//!
//! - [`derive_key`] — Argon2id (RFC 9106) memory-hard KDF.
//!   Derives a 256-bit vault master key from the user's master password and a
//!   random 32-byte salt.  Default parameters: `m_cost` = 65536 KiB (64 MiB),
//!   `t_cost` = 3, `p_cost` = 4.
//!
//! - [`encrypt`] / [`decrypt`] — AES-256-GCM authenticated encryption of an
//!   arbitrary byte slice (typically a serialised [`crate::vault::Vault`]).
//!
//! ## On-disk format
//!
//! Every encrypted vault file begins with:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ magic         8 bytes  "ZVAULT01"                               │
//! │ salt         32 bytes  random Argon2id salt                     │
//! │ m_cost        4 bytes  Argon2id memory cost (KiB, little-endian)│
//! │ t_cost        4 bytes  Argon2id time cost (iterations, LE)      │
//! │ p_cost        4 bytes  Argon2id parallelism (LE)                │
//! │ iv           12 bytes  AES-GCM nonce (random per write)         │
//! │ ciphertext   N bytes   AES-GCM ciphertext                       │
//! │ tag          16 bytes  AES-GCM authentication tag               │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! Total fixed header overhead: 8 + 32 + 4 + 4 + 4 + 12 = **64 bytes**.
//!
//! ## Design decisions
//!
//! * **Why not `ring`?**  The `ring` crate does not expose Argon2id (it uses
//!   PBKDF2 for KDF). We use `argon2` for key derivation and `aes-gcm` for
//!   AEAD; both are well-audited RustCrypto crates.
//!
//! * **Why AES-256-GCM over ChaCha20-Poly1305?**  AES-NI hardware acceleration
//!   is available on every x86-64 target we ship to. AES-256-GCM is a FIPS
//!   140-3 approved algorithm which may matter for enterprise deployments.
//!   ChaCha20-Poly1305 is used for Nostr (NIP-44) transport, which is a
//!   different threat model.
//!
//! * **Why a 12-byte random IV?**  The GCM specification recommends 96-bit
//!   (12-byte) IVs.  With a fresh random IV per write, IV collision probability
//!   is negligible for vault files written at human rates.
//!
//! * **Why store KDF params in the header?**  Allows future-proofing: we can
//!   increase `m_cost`/`t_cost` on vault re-key without breaking existing
//!   files. The params are authenticated by the AES-GCM tag (they are included
//!   as AAD — Additional Authenticated Data).
//!
//! * **Zeroize on drop** — [`VaultKey`] wraps `Zeroizing<[u8; 32]>`, so key
//!   material is overwritten on drop even if `Drop` is called implicitly.

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng as AeadOsRng},
    Aes256Gcm, Nonce,
};
use argon2::{Argon2, Params as Argon2Params, Version as Argon2Version};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

// NOTE: `aes-gcm 0.10` depends on `rand_core 0.6`, while the workspace uses
// `rand_core 0.9`.  They are incompatible crates. We therefore avoid importing
// rand_core directly here and instead use the `rand_core 0.6` `OsRng` that
// `aes-gcm` re-exports as `aes_gcm::aead::OsRng`.  This single RNG satisfies
// all crypto operations in this module.
use aes_gcm::aead::rand_core::RngCore as _;

use crate::{
    error::Error,
    Result,
};

// ─── Constants ────────────────────────────────────────────────────────────────

/// Magic bytes at the start of every ZVault encrypted file.
pub const MAGIC: &[u8; 8] = b"ZVAULT01";

/// Size of the Argon2id salt in bytes.
const SALT_LEN: usize = 32;
/// Size of the AES-GCM nonce (IV) in bytes.
const IV_LEN: usize = 12;
/// Size of the AES-GCM authentication tag in bytes.
const TAG_LEN: usize = 16;
/// Size of the fixed fields in the header (m_cost + t_cost + p_cost).
const KDF_PARAM_BYTES: usize = 12; // 3 × u32 LE
/// Total header length before ciphertext begins.
///
/// magic(8) + salt(32) + kdf_params(12) + iv(12) = 64
pub const HEADER_LEN: usize = MAGIC.len() + SALT_LEN + KDF_PARAM_BYTES + IV_LEN;

// ─── Default KDF parameters ──────────────────────────────────────────────────

/// Default Argon2id memory cost: 64 MiB expressed as KiB.
pub const DEFAULT_M_COST: u32 = 65_536;
/// Default Argon2id time cost (iterations).
pub const DEFAULT_T_COST: u32 = 3;
/// Default Argon2id parallelism.
pub const DEFAULT_P_COST: u32 = 4;

// ─── VaultKey ────────────────────────────────────────────────────────────────

/// A 256-bit vault master key.
///
/// The inner bytes are zeroed automatically on drop via [`Zeroizing`].
/// Do not clone or copy this type; derive a new key instead.
pub struct VaultKey(pub(crate) Zeroizing<[u8; 32]>);

impl VaultKey {
    /// Create a `VaultKey` from raw bytes.
    ///
    /// Callers are responsible for ensuring the bytes represent a valid
    /// 256-bit key and for zeroing the source slice if needed.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Return a reference to the raw key bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

// ─── KdfParams ───────────────────────────────────────────────────────────────

/// Argon2id KDF parameters serialised into the vault file header.
///
/// These are stored in plaintext (but authenticated by the GCM tag) so that
/// the correct parameters can be recovered on decryption without the user
/// having to remember them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    /// Salt (32 random bytes).
    pub salt: [u8; SALT_LEN],
    /// Memory cost in KiB.
    pub m_cost: u32,
    /// Time cost (number of iterations).
    pub t_cost: u32,
    /// Parallelism degree.
    pub p_cost: u32,
}

impl KdfParams {
    /// Generate a new `KdfParams` with a fresh random salt and the default
    /// cost parameters.
    #[must_use]
    pub fn generate() -> Self {
        let mut salt = [0u8; SALT_LEN];
        AeadOsRng.fill_bytes(&mut salt);
        Self {
            salt,
            m_cost: DEFAULT_M_COST,
            t_cost: DEFAULT_T_COST,
            p_cost: DEFAULT_P_COST,
        }
    }

    /// Serialise KDF params into their on-disk binary representation
    /// (salt || m_cost_le || t_cost_le || p_cost_le = 44 bytes).
    fn to_bytes(&self) -> [u8; SALT_LEN + KDF_PARAM_BYTES] {
        let mut out = [0u8; SALT_LEN + KDF_PARAM_BYTES];
        out[..SALT_LEN].copy_from_slice(&self.salt);
        out[SALT_LEN..SALT_LEN + 4].copy_from_slice(&self.m_cost.to_le_bytes());
        out[SALT_LEN + 4..SALT_LEN + 8].copy_from_slice(&self.t_cost.to_le_bytes());
        out[SALT_LEN + 8..SALT_LEN + 12].copy_from_slice(&self.p_cost.to_le_bytes());
        out
    }

    /// Deserialise KDF params from their on-disk binary representation.
    fn from_bytes(data: &[u8; SALT_LEN + KDF_PARAM_BYTES]) -> Self {
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&data[..SALT_LEN]);
        let m_cost = u32::from_le_bytes(data[SALT_LEN..SALT_LEN + 4].try_into().unwrap());
        let t_cost = u32::from_le_bytes(data[SALT_LEN + 4..SALT_LEN + 8].try_into().unwrap());
        let p_cost = u32::from_le_bytes(data[SALT_LEN + 8..SALT_LEN + 12].try_into().unwrap());
        Self { salt, m_cost, t_cost, p_cost }
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Derive a [`VaultKey`] from `password` and the provided [`KdfParams`].
///
/// Uses Argon2id (RFC 9106) with the parameters stored in `params`.
///
/// # Errors
///
/// Returns [`Error::Crypto`] if the Argon2id computation fails (e.g. invalid
/// cost parameters).
pub fn derive_key(password: &str, params: &KdfParams) -> Result<VaultKey> {
    let argon2_params = Argon2Params::new(
        params.m_cost,
        params.t_cost,
        params.p_cost,
        Some(32),
    )
    .map_err(|e| Error::Crypto(format!("invalid Argon2 params: {e}")))?;

    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        Argon2Version::V0x13,
        argon2_params,
    );

    let mut key_bytes = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), &params.salt, key_bytes.as_mut())
        .map_err(|e| Error::Crypto(format!("Argon2id KDF failed: {e}")))?;

    Ok(VaultKey(key_bytes))
}

/// Encrypt `plaintext` with `key` using AES-256-GCM.
///
/// Generates fresh [`KdfParams`] (including a new random salt) and a fresh
/// random 12-byte IV on every call.  Returns the full on-disk blob described
/// in the module-level documentation.
///
/// The magic bytes + KDF params (salt, m/t/p_cost) + IV are passed as
/// **additional authenticated data (AAD)** to AES-GCM so any tampering with
/// the header is detected on decryption.
///
/// # Errors
///
/// Returns [`Error::Crypto`] if encryption fails.
pub fn encrypt(key: &VaultKey, plaintext: &[u8]) -> Result<Vec<u8>> {
    encrypt_with_params(key, plaintext, &KdfParams::generate())
}

/// Like [`encrypt`] but accepts explicit [`KdfParams`].
///
/// Intended for testing (deterministic salt) and re-key operations.
///
/// # Errors
///
/// Returns [`Error::Crypto`] if encryption fails.
pub fn encrypt_with_params(
    key: &VaultKey,
    plaintext: &[u8],
    kdf_params: &KdfParams,
) -> Result<Vec<u8>> {
    // Build the AES-256-GCM cipher from the vault key.
    let cipher = Aes256Gcm::new_from_slice(key.as_bytes())
        .map_err(|e| Error::Crypto(format!("AES-GCM key init failed: {e}")))?;

    // Generate a fresh random 12-byte nonce.
    let nonce = Aes256Gcm::generate_nonce(&mut AeadOsRng);

    // Assemble the header.
    let kdf_bytes = kdf_params.to_bytes();
    let mut aad = Vec::with_capacity(MAGIC.len() + kdf_bytes.len() + IV_LEN);
    aad.extend_from_slice(MAGIC);
    aad.extend_from_slice(&kdf_bytes);
    aad.extend_from_slice(nonce.as_slice());

    // Encrypt with AAD.
    let ciphertext_with_tag = cipher
        .encrypt(
            &nonce,
            aes_gcm::aead::Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|e| Error::Crypto(format!("AES-GCM encryption failed: {e}")))?;

    // Layout: magic || kdf_params_bytes || nonce || ciphertext+tag
    let mut blob = Vec::with_capacity(aad.len() + ciphertext_with_tag.len());
    blob.extend_from_slice(&aad);
    blob.extend_from_slice(&ciphertext_with_tag);

    Ok(blob)
}

/// Decrypt a vault blob produced by [`encrypt`] / [`encrypt_with_params`].
///
/// Verifies the magic bytes, parses the KDF params and IV from the header,
/// reconstructs the AAD, and authenticates + decrypts the payload.
///
/// # Errors
///
/// - [`Error::InvalidVaultFile`] — wrong magic bytes, blob too short, or
///   AES-GCM authentication tag mismatch (ciphertext or header was tampered).
/// - [`Error::Crypto`] — AES-GCM key init failed (should not happen in
///   practice).
pub fn decrypt(key: &VaultKey, blob: &[u8]) -> Result<Vec<u8>> {
    // Minimum length: header + at least 0 bytes of ciphertext + 16-byte tag.
    let min_len = HEADER_LEN + TAG_LEN;
    if blob.len() < min_len {
        return Err(Error::InvalidVaultFile(format!(
            "blob too short: {} bytes (minimum {min_len})",
            blob.len()
        )));
    }

    // Verify magic.
    let magic = &blob[..MAGIC.len()];
    if magic != MAGIC {
        return Err(Error::InvalidVaultFile(format!(
            "bad magic: expected {MAGIC:?}, got {magic:?}",
        )));
    }

    // Parse header fields.
    let kdf_start = MAGIC.len();
    let iv_start = kdf_start + SALT_LEN + KDF_PARAM_BYTES;
    let payload_start = iv_start + IV_LEN;

    let kdf_raw: &[u8; SALT_LEN + KDF_PARAM_BYTES] = blob[kdf_start..iv_start]
        .try_into()
        .map_err(|_| Error::InvalidVaultFile("cannot read KDF params".into()))?;

    let _kdf_params = KdfParams::from_bytes(kdf_raw);
    let nonce = Nonce::from_slice(&blob[iv_start..payload_start]);

    // Reconstruct AAD (everything before the ciphertext).
    let aad = &blob[..payload_start];

    // Build cipher and decrypt.
    let cipher = Aes256Gcm::new_from_slice(key.as_bytes())
        .map_err(|e| Error::Crypto(format!("AES-GCM key init failed: {e}")))?;

    let plaintext = cipher
        .decrypt(
            nonce,
            aes_gcm::aead::Payload {
                msg: &blob[payload_start..],
                aad,
            },
        )
        .map_err(|_| {
            Error::InvalidVaultFile(
                "AES-GCM authentication failed: wrong key or tampered ciphertext".into(),
            )
        })?;

    Ok(plaintext)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// Derive a key with a fixed, deterministic salt for test reproducibility.
    fn test_key(password: &str) -> VaultKey {
        let params = KdfParams {
            salt: [0x42u8; 32],
            // Use minimal costs for fast tests; never use these in production.
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        };
        derive_key(password, &params).expect("test key derivation failed")
    }

    // ── KDF tests ────────────────────────────────────────────────────────────

    #[test]
    fn derive_key_produces_32_bytes() {
        let key = test_key("hunter2");
        assert_eq!(key.as_bytes().len(), 32);
    }

    #[test]
    fn derive_key_different_passwords_produce_different_keys() {
        let k1 = test_key("password1");
        let k2 = test_key("password2");
        assert_ne!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn derive_key_different_salts_produce_different_keys() {
        let params_a = KdfParams { salt: [0u8; 32], m_cost: 8, t_cost: 1, p_cost: 1 };
        let params_b = KdfParams { salt: [1u8; 32], m_cost: 8, t_cost: 1, p_cost: 1 };
        let k1 = derive_key("same-password", &params_a).unwrap();
        let k2 = derive_key("same-password", &params_b).unwrap();
        assert_ne!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn derive_key_is_deterministic() {
        // Same password + same params → same key every time.
        let params = KdfParams { salt: [0xABu8; 32], m_cost: 8, t_cost: 1, p_cost: 1 };
        let k1 = derive_key("deterministic", &params).unwrap();
        let k2 = derive_key("deterministic", &params).unwrap();
        assert_eq!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn derive_key_rejects_invalid_params() {
        // m_cost = 0 is invalid for Argon2.
        let params = KdfParams { salt: [0u8; 32], m_cost: 0, t_cost: 1, p_cost: 1 };
        assert!(derive_key("pw", &params).is_err());
    }

    // ── Encrypt / decrypt roundtrip ──────────────────────────────────────────

    #[test]
    fn encrypt_decrypt_roundtrip_empty() {
        let key = test_key("roundtrip-empty");
        let blob = encrypt(&key, b"").unwrap();
        let pt = decrypt(&key, &blob).unwrap();
        assert_eq!(pt, b"");
    }

    #[test]
    fn encrypt_decrypt_roundtrip_small() {
        let key = test_key("roundtrip-small");
        let plaintext = b"Hello, ZVault!";
        let blob = encrypt(&key, plaintext).unwrap();
        let pt = decrypt(&key, &blob).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn encrypt_decrypt_roundtrip_large() {
        let key = test_key("roundtrip-large");
        let plaintext = vec![0xAAu8; 1_000_000]; // 1 MB
        let blob = encrypt(&key, &plaintext).unwrap();
        let pt = decrypt(&key, &blob).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn encrypt_produces_different_blobs_for_same_input() {
        // Each call generates a fresh IV; ciphertexts must differ.
        let key = test_key("randomness");
        let blob1 = encrypt(&key, b"same plaintext").unwrap();
        let blob2 = encrypt(&key, b"same plaintext").unwrap();
        assert_ne!(blob1, blob2, "ciphertexts should differ due to fresh IVs");
    }

    #[test]
    fn blob_starts_with_magic() {
        let key = test_key("magic");
        let blob = encrypt(&key, b"payload").unwrap();
        assert_eq!(&blob[..8], MAGIC);
    }

    #[test]
    fn blob_minimum_length() {
        let key = test_key("min-len");
        let blob = encrypt(&key, b"").unwrap();
        assert!(blob.len() >= HEADER_LEN + TAG_LEN);
    }

    // ── Wrong key ────────────────────────────────────────────────────────────

    #[test]
    fn decrypt_wrong_key_fails() {
        let key_good = test_key("correct-password");
        let key_bad = test_key("wrong-password");
        let blob = encrypt(&key_good, b"secret data").unwrap();
        let result = decrypt(&key_bad, &blob);
        assert!(
            result.is_err(),
            "decrypting with wrong key should fail"
        );
    }

    // ── Tampered ciphertext ──────────────────────────────────────────────────

    #[test]
    fn decrypt_tampered_ciphertext_fails() {
        let key = test_key("tamper-cipher");
        let plaintext = b"tamper me if you dare";
        let mut blob = encrypt(&key, plaintext).unwrap();
        // Flip a byte in the ciphertext portion (after the header).
        let idx = HEADER_LEN + 2;
        blob[idx] ^= 0xFF;
        let result = decrypt(&key, &blob);
        assert!(result.is_err(), "GCM tag should catch ciphertext tampering");
    }

    #[test]
    fn decrypt_tampered_header_aad_fails() {
        let key = test_key("tamper-header");
        let mut blob = encrypt(&key, b"payload").unwrap();
        // Flip a byte in the KDF params region of the header (part of AAD).
        blob[MAGIC.len() + 1] ^= 0x01;
        let result = decrypt(&key, &blob);
        assert!(result.is_err(), "GCM tag should catch header (AAD) tampering");
    }

    #[test]
    fn decrypt_bad_magic_fails() {
        let key = test_key("bad-magic");
        let mut blob = encrypt(&key, b"payload").unwrap();
        // Corrupt the magic bytes.
        blob[0] = b'X';
        let result = decrypt(&key, &blob);
        match result {
            Err(Error::InvalidVaultFile(_)) => {}
            other => panic!("expected InvalidVaultFile, got {other:?}"),
        }
    }

    #[test]
    fn decrypt_truncated_blob_fails() {
        let key = test_key("truncated");
        let result = decrypt(&key, &[0u8; 10]);
        assert!(result.is_err(), "truncated blob must fail");
    }

    #[test]
    fn decrypt_empty_blob_fails() {
        let key = test_key("empty-blob");
        let result = decrypt(&key, &[]);
        assert!(result.is_err());
    }

    // ── KdfParams serialisation ──────────────────────────────────────────────

    #[test]
    fn kdf_params_roundtrip_binary() {
        let params = KdfParams {
            salt: [0x55u8; 32],
            m_cost: 65_536,
            t_cost: 3,
            p_cost: 4,
        };
        let bytes = params.to_bytes();
        let recovered = KdfParams::from_bytes(&bytes);
        assert_eq!(params, recovered);
    }

    #[test]
    fn kdf_params_roundtrip_json() {
        let params = KdfParams::generate();
        let json = serde_json::to_string(&params).unwrap();
        let recovered: KdfParams = serde_json::from_str(&json).unwrap();
        assert_eq!(params, recovered);
    }

    #[test]
    fn kdf_params_generate_unique_salts() {
        let a = KdfParams::generate();
        let b = KdfParams::generate();
        // Statistically impossible to collide; good enough for a unit test.
        assert_ne!(a.salt, b.salt);
    }

    // ── Header layout ────────────────────────────────────────────────────────

    #[test]
    fn header_len_constant_is_correct() {
        // Verify the HEADER_LEN constant against the actual layout.
        let expected = MAGIC.len() + SALT_LEN + KDF_PARAM_BYTES + IV_LEN;
        assert_eq!(HEADER_LEN, expected);
    }
}
