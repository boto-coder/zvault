//! High-level on-disk vault API.
//!
//! [`VaultFile`] sits on top of the M1 crypto layer and provides the four
//! operations a caller needs to manage an encrypted vault file:
//!
//! | Operation | Description |
//! |-----------|-------------|
//! | [`VaultFile::create`] | Initialise an empty vault, encrypt, write atomically |
//! | [`VaultFile::open`]   | Read, decrypt, and deserialise an existing vault file |
//! | [`VaultFile::save`]   | Re-encrypt and atomically overwrite with updated vault |
//! | [`VaultFile::rekey`]  | Change the master password, re-encrypt, write atomically |
//!
//! ## Atomic writes
//!
//! Every write goes through the private [`atomic_write`] helper:
//!
//! 1. Write the new ciphertext to `<original_path>.tmp` (full filename + `.tmp`,
//!    not extension-replacement).
//! 2. Rename `.tmp` → original path (atomic on POSIX; best-effort on Windows).
//!
//! This means a crash mid-write leaves the original file intact; the orphaned
//! `.tmp` file is safe to delete.
//!
//! ## Key consistency on `save`
//!
//! [`VaultFile::save`] takes both a [`VaultKey`] *and* the [`KdfParams`] that
//! were used to derive that key.  It calls [`encrypt_with_params`] with those
//! exact params (generating only a fresh IV, not a new salt), ensuring that
//! the key in the caller's memory remains valid for future saves without
//! re-running Argon2id.
//!
//! To obtain a new salt (e.g. after a period of inactivity), use
//! [`VaultFile::rekey`] with the same password.
//!
//! ## Memory safety
//!
//! All intermediate plaintext buffers (`Vec<u8>` containing JSON or decrypted
//! vault bytes) are wrapped in [`zeroize::Zeroizing`] so they are overwritten
//! on drop even if an early-return error path is taken.

use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

use crate::crypto::{
    decrypt, derive_key, encrypt_with_params, parse_kdf_params, KdfParams, VaultKey,
};
use crate::vault::Vault;
use crate::Result;

// ─── VaultFile ───────────────────────────────────────────────────────────────

/// A handle to an encrypted vault file on disk.
///
/// The struct holds the file path and the [`KdfParams`] that were used when
/// the file was last written.  Callers must hold the corresponding [`VaultKey`]
/// in memory for subsequent [`VaultFile::save`] calls; the key is not stored
/// here.
///
/// # Example
///
/// ```no_run
/// use zvault_core::vault::VaultFile;
///
/// // Create a new vault
/// let (_vf, _key) = VaultFile::create("my-master-password", "/tmp/my.zvault").unwrap();
///
/// // Open an existing vault
/// let (_vf, _key, _vault) = VaultFile::open("my-master-password", "/tmp/my.zvault").unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct VaultFile {
    /// Path to the encrypted vault file.
    pub path: PathBuf,
    /// The KDF params embedded in the on-disk blob.
    ///
    /// Stored here so [`VaultFile::save`] can call [`encrypt_with_params`]
    /// with the same params the caller's [`VaultKey`] was derived from,
    /// keeping the key consistent with the file across multiple saves.
    pub kdf_params: KdfParams,
}

impl VaultFile {
    // ── Public API ────────────────────────────────────────────────────────

    /// Create a new, empty vault at `path`, encrypted with `password`.
    ///
    /// Returns the [`VaultFile`] handle and the derived [`VaultKey`] so the
    /// caller can immediately start using the vault without a second `open`.
    ///
    /// # Errors
    ///
    /// - [`Error::Crypto`] — key derivation or encryption failed.
    /// - [`Error::Serialisation`] — vault JSON serialisation failed.
    /// - [`Error::Io`] — filesystem write or rename failed.
    pub fn create(password: &str, path: impl AsRef<Path>) -> Result<(Self, VaultKey)> {
        let path = path.as_ref().to_path_buf();

        // Serialise a fresh, empty vault.  to_json() returns Zeroizing<Vec<u8>>
        // so the JSON bytes (which contain the vault structure) are zeroed on drop.
        let vault = Vault::new();
        let json = vault.to_json()?;

        // Derive key from fresh params.
        let params = KdfParams::generate();
        let key = derive_key(password, &params)?;

        // Encrypt and write.
        let blob = encrypt_with_params(&key, &json, &params)?;
        atomic_write(&path, &blob)?;

        Ok((
            Self {
                path,
                kdf_params: params,
            },
            key,
        ))
    }

    /// Open an existing vault file, decrypt it, and return the [`Vault`] and
    /// the derived [`VaultKey`].
    ///
    /// The returned [`VaultKey`] should be kept in memory for the session and
    /// passed to [`VaultFile::save`] on every mutation.
    ///
    /// # Errors
    ///
    /// - [`Error::Io`] — cannot read the file.
    /// - [`Error::InvalidVaultFile`] — bad magic, truncated file, or wrong
    ///   password (AES-GCM authentication tag mismatch).
    /// - [`Error::Crypto`] — KDF failure.
    /// - [`Error::Serialisation`] — plaintext is not valid vault JSON.
    pub fn open(password: &str, path: impl AsRef<Path>) -> Result<(Self, VaultKey, Vault)> {
        let path = path.as_ref().to_path_buf();

        let blob = std::fs::read(&path)?;
        let params = parse_kdf_params(&blob)?;
        let key = derive_key(password, &params)?;

        // Wrap the plaintext in Zeroizing so the JSON bytes are zeroed on drop.
        let plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(decrypt(&key, &blob)?);
        let vault = Vault::from_json(&plaintext)?;

        Ok((
            Self {
                path,
                kdf_params: params,
            },
            key,
            vault,
        ))
    }

    /// Re-encrypt `vault` with `key` and overwrite the vault file atomically.
    ///
    /// Uses the [`KdfParams`] stored in this [`VaultFile`] handle (the same
    /// params that `key` was derived from) so the caller's key remains valid
    /// after the save.  A fresh random IV is generated on every call, so the
    /// on-disk blob always differs from the previous write even when the vault
    /// contents are unchanged.
    ///
    /// # Key consistency
    ///
    /// `key` **must** have been derived with `self.kdf_params`.  If you derived
    /// the key from a different set of params the GCM authentication will fail
    /// on the next `open`.  The normal flow is:
    ///
    /// ```text
    /// let (vf, key, mut vault) = VaultFile::open(password, path)?;
    /// vault.add_item(item);
    /// vf.save(&key, &vault)?;  // uses vf.kdf_params — consistent
    /// ```
    ///
    /// # Errors
    ///
    /// - [`Error::Serialisation`] — vault JSON serialisation failed.
    /// - [`Error::Crypto`] — encryption failed.
    /// - [`Error::Io`] — filesystem write or rename failed.
    pub fn save(&self, key: &VaultKey, vault: &Vault) -> Result<()> {
        // to_json() returns Zeroizing<Vec<u8>> — vault plaintext zeroed on drop.
        let json = vault.to_json()?;

        // Use the stored KDF params so the caller's key stays consistent with
        // the file.  encrypt_with_params generates a fresh IV but keeps the
        // same salt, meaning the key does not go stale after a save.
        let blob = encrypt_with_params(key, &json, &self.kdf_params)?;
        atomic_write(&self.path, &blob)
    }

    /// Change the master password of this vault file.
    ///
    /// Reads and decrypts the vault with `old_password`, then re-encrypts it
    /// under `new_password` with fresh [`KdfParams`] (new salt + new IV) and
    /// overwrites the file atomically.
    ///
    /// Returns the updated [`VaultFile`] handle (with the new `kdf_params`),
    /// the new [`VaultKey`], and the decrypted [`Vault`] so the caller can
    /// resume working without a second `open` call.
    ///
    /// # Errors
    ///
    /// - [`Error::Io`] — cannot read/write the file.
    /// - [`Error::InvalidVaultFile`] — wrong `old_password` or corrupt file.
    /// - [`Error::Crypto`] — KDF or encryption failure.
    /// - [`Error::Serialisation`] — vault JSON round-trip failure.
    pub fn rekey(&self, old_password: &str, new_password: &str) -> Result<(Self, VaultKey, Vault)> {
        // Decrypt with old password.
        let blob = std::fs::read(&self.path)?;
        let old_params = parse_kdf_params(&blob)?;
        let old_key = derive_key(old_password, &old_params)?;

        // Wrap plaintext in Zeroizing — contains full vault JSON with secrets.
        let plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(decrypt(&old_key, &blob)?);

        // Re-encrypt with new password + fresh KdfParams (new salt + new IV).
        let new_params = KdfParams::generate();
        let new_key = derive_key(new_password, &new_params)?;
        let new_blob = encrypt_with_params(&new_key, &plaintext, &new_params)?;

        atomic_write(&self.path, &new_blob)?;

        // Deserialise and return — plaintext is still in scope (Zeroizing drops
        // it after this function returns).
        let vault = Vault::from_json(&plaintext)?;
        let new_vf = Self {
            path: self.path.clone(),
            kdf_params: new_params,
        };
        Ok((new_vf, new_key, vault))
    }
}

// ─── Private helpers ─────────────────────────────────────────────────────────

/// Write `data` to `path` atomically by writing to `<path>.tmp` first and then
/// renaming.
///
/// The temp path is constructed by appending `.tmp` to the **full filename**
/// (not by replacing the last extension via `with_extension`).  For example:
/// - `my.zvault`    → `my.zvault.tmp`
/// - `backup`       → `backup.tmp`
/// - `v2.bak.zvault` → `v2.bak.zvault.tmp`
///
/// This avoids the `with_extension` pitfall where a path ending in `.tmp`
/// would produce the same temp and destination path, making the rename a no-op.
///
/// On POSIX, `rename(2)` is atomic: observers see either the old content or
/// the new content, never a partial write.  On Windows, Rust uses
/// `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` which is close-to-atomic.
///
/// # Errors
///
/// Returns [`Error::Io`] if either the write or the rename fails.
fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::ItemKind;
    use crate::Error;
    use tempfile::tempdir;

    // ── Helpers ──────────────────────────────────────────────────────────

    /// Low-cost KDF params for tests — fast Argon2id, never use in production.
    fn test_params() -> KdfParams {
        KdfParams {
            salt: [0x42u8; 32],
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        }
    }

    /// Like VaultFile::create but with fast test KDF params.
    fn create_test(password: &str, path: &Path) -> Result<(VaultFile, VaultKey)> {
        let vault = Vault::new();
        let json = vault.to_json()?;
        let params = test_params();
        let key = derive_key(password, &params)?;
        let blob = encrypt_with_params(&key, &json, &params)?;
        atomic_write(path, &blob)?;
        Ok((
            VaultFile {
                path: path.to_path_buf(),
                kdf_params: params,
            },
            key,
        ))
    }

    /// Like VaultFile::open but re-derives with the params in the file (test-aware).
    fn open_test(password: &str, path: &Path) -> Result<(VaultFile, VaultKey, Vault)> {
        let blob = std::fs::read(path)?;
        let params = parse_kdf_params(&blob)?;
        let key = derive_key(password, &params)?;
        let plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(decrypt(&key, &blob)?);
        let vault = Vault::from_json(&plaintext)?;
        Ok((
            VaultFile {
                path: path.to_path_buf(),
                kdf_params: params,
            },
            key,
            vault,
        ))
    }

    // ── create + open round-trip ──────────────────────────────────────────

    #[test]
    fn create_writes_file_open_reads_back() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.zvault");

        let (vf, _key) = create_test("correct-horse-battery-staple", &path).unwrap();
        assert!(path.exists(), "vault file should exist after create");

        let (_, _, vault) = open_test("correct-horse-battery-staple", &path).unwrap();
        assert_eq!(vf.path, path);
        assert!(vault.items.is_empty(), "new vault should have no items");
        assert_eq!(vault.version, 0);
    }

    // ── save round-trip ───────────────────────────────────────────────────

    #[test]
    fn save_roundtrip_version_persisted() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("save_test.zvault");

        let (vf, key) = create_test("save-password", &path).unwrap();
        let (_, _, mut vault) = open_test("save-password", &path).unwrap();

        vault.version = 42;
        vf.save(&key, &vault).unwrap();

        let (_, _, vault2) = open_test("save-password", &path).unwrap();
        assert_eq!(vault2.version, 42, "version should survive save/reopen");
    }

    /// Key stays valid across multiple saves (kdf_params consistent).
    #[test]
    fn save_key_remains_valid_across_multiple_saves() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("multi_save.zvault");

        let (vf, key) = create_test("multi-save", &path).unwrap();
        let (_, _, mut vault) = open_test("multi-save", &path).unwrap();

        // Three consecutive saves with the same key.
        for i in 1u64..=3 {
            vault.version = i;
            vf.save(&key, &vault).unwrap();
        }

        // The key is still valid after multiple saves.
        let (_, _, vault3) = open_test("multi-save", &path).unwrap();
        assert_eq!(vault3.version, 3);
    }

    // ── password mismatch ─────────────────────────────────────────────────

    #[test]
    fn open_wrong_password_returns_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mismatch.zvault");
        create_test("correct-password", &path).unwrap();

        let result = open_test("wrong-password", &path);
        assert!(result.is_err());
    }

    #[test]
    fn open_wrong_password_returns_invalid_vault_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mismatch2.zvault");
        create_test("my-password", &path).unwrap();

        let err = open_test("bad-password", &path).unwrap_err();
        assert!(
            matches!(err, Error::InvalidVaultFile(_)),
            "expected InvalidVaultFile, got: {err:?}"
        );
    }

    // ── rekey ─────────────────────────────────────────────────────────────

    #[test]
    fn rekey_new_password_opens() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rekey.zvault");
        let (vf, _) = create_test("old-password", &path).unwrap();

        // Perform rekey inline with test params.
        {
            let blob = std::fs::read(&path).unwrap();
            let old_params = parse_kdf_params(&blob).unwrap();
            let old_key = derive_key("old-password", &old_params).unwrap();
            let plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(decrypt(&old_key, &blob).unwrap());
            let new_params = KdfParams {
                salt: [0xBBu8; 32],
                m_cost: 8,
                t_cost: 1,
                p_cost: 1,
            };
            let new_key = derive_key("new-password", &new_params).unwrap();
            let new_blob = encrypt_with_params(&new_key, &plaintext, &new_params).unwrap();
            atomic_write(&vf.path, &new_blob).unwrap();
        }

        let result = open_test("new-password", &path);
        assert!(result.is_ok(), "new password should work after rekey");
    }

    #[test]
    fn rekey_old_password_fails() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rekey2.zvault");
        let (vf, _) = create_test("old-pass", &path).unwrap();

        {
            let blob = std::fs::read(&path).unwrap();
            let old_params = parse_kdf_params(&blob).unwrap();
            let old_key = derive_key("old-pass", &old_params).unwrap();
            let plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(decrypt(&old_key, &blob).unwrap());
            let new_params = KdfParams {
                salt: [0xCCu8; 32],
                m_cost: 8,
                t_cost: 1,
                p_cost: 1,
            };
            let new_key = derive_key("new-pass", &new_params).unwrap();
            let new_blob = encrypt_with_params(&new_key, &plaintext, &new_params).unwrap();
            atomic_write(&vf.path, &new_blob).unwrap();
        }

        let result = open_test("old-pass", &path);
        assert!(result.is_err(), "old password must be rejected after rekey");
    }

    /// VaultFile::rekey (public API) — uses production Argon2id params.
    #[test]
    #[ignore = "uses production Argon2id params (64 MiB) — too slow for unit tests"]
    fn rekey_public_api_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rekey_api.zvault");

        let (_vf, _key) = VaultFile::create("old-password", &path).unwrap();

        let vf = VaultFile::open("old-password", &path).unwrap().0;
        let (new_vf, _new_key, vault) = vf.rekey("old-password", "new-password").unwrap();
        assert_eq!(vault.version, 0);

        let (_, _, vault2) = VaultFile::open("new-password", &path).unwrap();
        assert_eq!(vault2.id, vault.id);

        // new_vf kdf_params must match what's in the file now.
        let blob = std::fs::read(&path).unwrap();
        let on_disk_params = parse_kdf_params(&blob).unwrap();
        assert_eq!(new_vf.kdf_params, on_disk_params);

        let result_old = VaultFile::open("old-password", &path);
        assert!(result_old.is_err());
    }

    // ── corrupt file ─────────────────────────────────────────────────────

    #[test]
    fn corrupt_ciphertext_returns_invalid_vault_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("corrupt.zvault");
        create_test("tamper-password", &path).unwrap();

        let mut raw = std::fs::read(&path).unwrap();
        raw[64 + 2] ^= 0xFF;
        std::fs::write(&path, &raw).unwrap();

        let err = open_test("tamper-password", &path).unwrap_err();
        assert!(matches!(err, Error::InvalidVaultFile(_)));
    }

    #[test]
    fn corrupt_header_returns_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("corrupt_header.zvault");
        create_test("tamper-header", &path).unwrap();

        let mut raw = std::fs::read(&path).unwrap();
        raw[10] ^= 0x01;
        std::fs::write(&path, &raw).unwrap();

        assert!(open_test("tamper-header", &path).is_err());
    }

    #[test]
    fn truncated_file_returns_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("truncated.zvault");
        std::fs::write(&path, [0u8; 20]).unwrap();
        assert!(open_test("any-password", &path).is_err());
    }

    // ── atomic write ─────────────────────────────────────────────────────

    #[test]
    fn no_tmp_file_left_after_create() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("notmp.zvault");
        // The .tmp file should be <name>.zvault.tmp, not <name>.tmp
        let tmp_path = dir.path().join("notmp.zvault.tmp");

        create_test("no-tmp-test", &path).unwrap();

        assert!(path.exists(), "vault file must exist");
        assert!(
            !tmp_path.exists(),
            ".zvault.tmp must not remain after successful write"
        );
    }

    #[test]
    fn tmp_path_appends_not_replaces_extension() {
        // Verify atomic_write uses <name>.zvault.tmp not <name>.tmp
        let dir = tempdir().unwrap();
        let path = dir.path().join("my.zvault");
        create_test("ext-test", &path).unwrap();

        // The bad old path (with_extension replacement) would be "my.tmp"
        let bad_tmp = dir.path().join("my.tmp");
        assert!(
            !bad_tmp.exists(),
            "my.tmp must not exist — extension must be appended"
        );
    }

    // ── path is preserved ────────────────────────────────────────────────

    #[test]
    fn vault_file_path_is_canonical() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("path_test.zvault");

        let (vf, _) = create_test("path-test", &path).unwrap();
        assert_eq!(vf.path, path);

        let (vf2, _, _) = open_test("path-test", &path).unwrap();
        assert_eq!(vf2.path, path);
    }

    // ── kdf_params consistent ─────────────────────────────────────────────

    #[test]
    fn kdf_params_in_handle_matches_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("params_check.zvault");

        let (vf, _key) = create_test("params-test", &path).unwrap();

        // Parse params directly from the written file and compare.
        let blob = std::fs::read(&path).unwrap();
        let on_disk = parse_kdf_params(&blob).unwrap();
        assert_eq!(
            vf.kdf_params, on_disk,
            "VaultFile.kdf_params must match what is in the file"
        );
    }

    // ── wrong password does not panic ─────────────────────────────────────

    #[test]
    fn wrong_password_does_not_panic() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nopanic.zvault");
        create_test("correct", &path).unwrap();

        for bad_pw in &["", "wrong", "CORRECT", "correc", "correct "] {
            let r = open_test(bad_pw, &path);
            assert!(r.is_err(), "wrong password '{bad_pw}' must return Err");
        }
    }

    // ── CRUD items survive save/reopen ────────────────────────────────────

    #[test]
    fn items_survive_save_reopen() {
        use crate::vault::VaultItem;
        let dir = tempdir().unwrap();
        let path = dir.path().join("items.zvault");

        let (vf, key) = create_test("items-test", &path).unwrap();
        let (_, _, mut vault) = open_test("items-test", &path).unwrap();

        let mut item = VaultItem::new(ItemKind::Login, "GitHub");
        item.username = Some("alice@example.com".into());
        item.password = Some("s3cr3t".into());
        let item_id = item.id;
        vault.add_item(item);
        vf.save(&key, &vault).unwrap();

        let (_, _, vault2) = open_test("items-test", &path).unwrap();
        let retrieved = vault2
            .get_item(item_id)
            .expect("item must survive save/reopen");
        assert_eq!(retrieved.name, "GitHub");
        assert_eq!(retrieved.username.as_deref(), Some("alice@example.com"));
    }
}
