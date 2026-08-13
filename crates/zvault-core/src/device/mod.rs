//! Device lifecycle management for ZVault.
//!
//! ## Overview
//!
//! Each device that can access a ZVault vault has a secp256k1 identity keypair.
//! The public key (Nostr pubkey) identifies the device in the vault's device
//! list and in Nostr sync events.  The secret key never leaves the device and
//! is stored in OS-provided secure storage.
//!
//! ## Types
//!
//! - [`DeviceIdentity`] — the in-memory view of this device (device_id + pubkey).
//! - [`DeviceManager`] — owns the device list CRDT; handles admit, revoke, and merge.
//! - [`OrSet`] — OR-Set CRDT (add-wins) used for the device list.
//!
//! ## OR-Set CRDT semantics
//!
//! The device list is modelled as an OR-Set (Observed-Remove Set with add-wins
//! semantics).  Each add operation tags the element with a unique token; a
//! remove only removes elements whose tokens were observed at remove time.  Two
//! concurrent adds of the same element therefore both survive a merge, while a
//! concurrent add + remove leaves the add in place (add-wins).
//!
//! In ZVault this means:
//! - Admitting a device is an OR-Set add with a fresh random token.
//! - Revoking a device is an OR-Set remove: the revoked-flag is set in the
//!   vault's [`DeviceEntry`] AND the element is removed from the `adds` set.
//! - Merging two replicas is a standard OR-Set merge: union of both `adds`
//!   sets, then remove any element from `adds` whose unique token appears in
//!   the other replica's `removes` set.
//!
//! ## Secret key storage
//!
//! The device secret key is stored under the key `"zvault/device/<device_id>/secret_key"`
//! in whatever [`SecureStorage`] implementation the caller provides.  Only the
//! public key and device_id are kept in [`DeviceIdentity`].

use chrono::Utc;
use k256::ecdsa::SigningKey;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use aes_gcm::aead::OsRng as AeadOsRng;

use crate::vault::{DeviceEntry, Vault};
use crate::{Error, Result};

// ─── DeviceIdentity ──────────────────────────────────────────────────────────

/// In-memory representation of this device's identity.
///
/// The corresponding secret key is held in [`SecureStorage`], never in a
/// struct field.  Call [`DeviceIdentity::generate`] to create a new identity;
/// the secret key is written to storage at that point.
#[derive(Debug, Clone)]
pub struct DeviceIdentity {
    /// Stable random device identifier.
    pub device_id: Uuid,
    /// secp256k1 public key (hex-encoded, 64 hex chars = 32 bytes compressed X).
    ///
    /// This is the x-only (Nostr-style) public key: the 32-byte X coordinate of
    /// the public key point, hex-encoded.
    pub pubkey_hex: String,
}

impl DeviceIdentity {
    /// Generate a new secp256k1 keypair and persist the secret key in `storage`.
    ///
    /// The device is assigned a fresh random [`Uuid`].  The secret key is stored
    /// at `"zvault/device/<device_id>/secret_key"` and never returned.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Crypto`] if key generation fails, or [`Error::Crypto`] if
    /// the OS rejects the secure storage write.
    pub fn generate(label: &str, storage: &dyn SecureStorage) -> Result<(Self, DeviceKeyMaterial)> {
        let device_id = Uuid::new_v4();

        // Generate secp256k1 signing key using OsRng from rand_core 0.6 (via aes-gcm).
        // We use the same OsRng approach as the crypto module to avoid version conflicts.
        let signing_key = SigningKey::random(&mut AeadOsRng);

        // Extract the secret scalar bytes (32 bytes) wrapped in Zeroizing.
        let secret_bytes: Zeroizing<Vec<u8>> = Zeroizing::new(signing_key.to_bytes().to_vec());

        // Derive the x-only public key (Nostr-style).
        let verifying_key = signing_key.verifying_key();
        let pubkey_hex = verifying_key_to_hex(verifying_key);

        // Persist secret key in secure storage.
        let storage_key = device_secret_key_path(device_id);
        storage
            .store(&storage_key, &secret_bytes)
            .map_err(|e| Error::Crypto(format!("secure storage write failed: {e}")))?;

        let identity = DeviceIdentity {
            device_id,
            pubkey_hex: pubkey_hex.clone(),
        };

        let material = DeviceKeyMaterial {
            device_id,
            label: label.to_string(),
            pubkey_hex,
        };

        Ok((identity, material))
    }

    /// Load the secret key for this device from `storage` and return it
    /// wrapped in [`Zeroizing`].
    ///
    /// Callers should use the key immediately and let it drop.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DeviceNotFound`] if the key is not present in storage.
    pub fn load_secret_key(&self, storage: &dyn SecureStorage) -> Result<Zeroizing<Vec<u8>>> {
        let storage_key = device_secret_key_path(self.device_id);
        storage
            .load(&storage_key)
            .map(Zeroizing::new)
            .map_err(|_| Error::DeviceNotFound(self.device_id))
    }
}

/// Material produced alongside a new [`DeviceIdentity`]; used to build the
/// vault's [`DeviceEntry`] during admit.
#[derive(Debug)]
pub struct DeviceKeyMaterial {
    /// Device identifier.
    pub device_id: Uuid,
    /// Human-readable device label.
    pub label: String,
    /// secp256k1 public key (hex-encoded).
    pub pubkey_hex: String,
}

// ─── SecureStorage ───────────────────────────────────────────────────────────

/// Abstraction over OS-provided secure key storage.
///
/// Implementations expected per platform:
/// - Desktop: `keyring` crate → macOS Keychain / Windows Credential Manager /
///   libsecret (M5)
/// - Android: Android Keystore API via UniFFI (M10)
/// - Browser extension: `browser.storage.local` encrypted (M9)
///
/// Tests may use [`InMemoryStorage`].
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

// ─── InMemoryStorage ─────────────────────────────────────────────────────────

/// In-memory [`SecureStorage`] implementation for use in tests.
///
/// This is **not** a production-safe storage backend — secret bytes are held in
/// a plain `HashMap` with no encryption.  Use only in tests and integration
/// scenarios that do not require OS-level protection.
#[cfg(any(test, feature = "test-helpers"))]
#[derive(Debug, Default)]
pub struct InMemoryStorage {
    data: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
}

#[cfg(any(test, feature = "test-helpers"))]
impl SecureStorage for InMemoryStorage {
    fn store(&self, key: &str, value: &[u8]) -> Result<()> {
        self.data
            .lock()
            .expect("mutex poisoned")
            .insert(key.to_string(), value.to_vec());
        Ok(())
    }

    fn load(&self, key: &str) -> Result<Vec<u8>> {
        self.data
            .lock()
            .expect("mutex poisoned")
            .get(key)
            .cloned()
            .ok_or_else(|| Error::Crypto(format!("key not found in InMemoryStorage: {key}")))
    }

    fn delete(&self, key: &str) -> Result<()> {
        self.data
            .lock()
            .expect("mutex poisoned")
            .remove(key)
            .ok_or_else(|| Error::Crypto(format!("key not found in InMemoryStorage: {key}")))?;
        Ok(())
    }
}

// ─── OR-Set CRDT ─────────────────────────────────────────────────────────────

/// OR-Set (Observed-Remove Set with add-wins semantics) CRDT.
///
/// Elements are identified by a payload `T` plus a unique random token `Uuid`.
/// Each add tags the element with a fresh token; a remove removes all currently
/// observed tokens for that element.  Two concurrent adds therefore both survive
/// (they have different tokens), and a concurrent add + remove leaves the add
/// in place (add-wins).
///
/// The set is serialised as two collections:
/// - `adds`: the set of `(element, token)` pairs currently in the set.
/// - `removes`: the set of tokens that have been removed.
///
/// Merge is: union the `adds` and `removes` sets, then filter `adds` to remove
/// any element whose token is in `removes`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrSet<T>
where
    T: Clone + PartialEq + Eq + std::hash::Hash + Serialize,
{
    /// The currently live elements, each paired with a unique token.
    pub adds: Vec<(T, Uuid)>,
    /// Tokens that have been explicitly removed.
    pub removes: std::collections::HashSet<Uuid>,
}

impl<T> Default for OrSet<T>
where
    T: Clone + PartialEq + Eq + std::hash::Hash + Serialize,
{
    fn default() -> Self {
        Self {
            adds: Vec::new(),
            removes: std::collections::HashSet::new(),
        }
    }
}

impl<T> OrSet<T>
where
    T: Clone + PartialEq + Eq + std::hash::Hash + Serialize,
{
    /// Create an empty OR-Set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `element` with a fresh random token and return the token.
    pub fn add(&mut self, element: T) -> Uuid {
        let token = Uuid::new_v4();
        self.adds.push((element, token));
        token
    }

    /// Remove all currently-observed tokens for `element`.
    ///
    /// Returns `true` if at least one token was found and removed; `false` if
    /// the element was not in the live set (idempotent — safe to call on
    /// already-removed elements).
    pub fn remove(&mut self, element: &T) -> bool {
        let tokens: Vec<Uuid> = self
            .adds
            .iter()
            .filter_map(|(e, tok)| if e == element { Some(*tok) } else { None })
            .collect();

        if tokens.is_empty() {
            return false;
        }

        for tok in &tokens {
            self.removes.insert(*tok);
        }
        // Remove from adds so the live set is clean.
        self.adds.retain(|(_, tok)| !tokens.contains(tok));
        true
    }

    /// Returns `true` if `element` is in the live set.
    #[must_use]
    pub fn contains(&self, element: &T) -> bool {
        self.adds.iter().any(|(e, _)| e == element)
    }

    /// Return an iterator over the live elements (duplicates may appear if the
    /// same element was added multiple times concurrently).
    pub fn elements(&self) -> impl Iterator<Item = &T> {
        self.adds.iter().map(|(e, _)| e)
    }

    /// Merge `other` into `self` using OR-Set semantics:
    /// 1. Union the `removes` sets.
    /// 2. Union the `adds` sets (deduplicating by token).
    /// 3. Filter `adds` to drop any element whose token is in `removes`.
    pub fn merge(&mut self, other: &Self) {
        // 1. Union removes.
        self.removes.extend(&other.removes);

        // 2. Union adds — only insert tokens not already present.
        let existing_tokens: std::collections::HashSet<Uuid> =
            self.adds.iter().map(|(_, tok)| *tok).collect();
        for (elem, tok) in &other.adds {
            if !existing_tokens.contains(tok) {
                self.adds.push((elem.clone(), *tok));
            }
        }

        // 3. Apply removes to live adds.
        let removes = &self.removes;
        self.adds.retain(|(_, tok)| !removes.contains(tok));
    }
}

// ─── DeviceManager ───────────────────────────────────────────────────────────

/// Manages the device list for a vault.
///
/// `DeviceManager` wraps the vault's `devices` list and provides typed
/// operations: admit, revoke, and CRDT-merge.  It is not stored on disk
/// independently — it reconstructs its CRDT state from the `Vault::devices`
/// `Vec<DeviceEntry>` on each open.
///
/// ## Relationship to Vault
///
/// `DeviceManager` reads and mutates `vault.devices`.  Call
/// [`DeviceManager::flush`] to write the current device list back into the
/// vault (which bumps `vault.version`).
#[derive(Debug, Clone)]
pub struct DeviceManager {
    /// The OR-Set CRDT tracking which devices are live.
    ///
    /// Elements are device_ids (UUIDs); the full metadata is in `entries`.
    or_set: OrSet<Uuid>,
    /// Full metadata for every device that has ever been admitted.
    entries: Vec<DeviceEntry>,
}

impl DeviceManager {
    /// Construct a `DeviceManager` from an existing vault's device list.
    ///
    /// Live (non-revoked) devices are loaded into the OR-Set adds; revoked
    /// devices are loaded into the OR-Set removes.
    #[must_use]
    pub fn from_vault(vault: &Vault) -> Self {
        let mut dm = Self {
            or_set: OrSet::new(),
            entries: vault.devices.clone(),
        };

        // Reconstruct CRDT state from the flat device list.
        // Non-revoked devices are in the live set; revoked devices are absent.
        // We assign deterministic tokens derived from the device_id so that the
        // OR-Set can be rebuilt identically on every open.
        for entry in &vault.devices {
            if !entry.revoked {
                // Use the device_id itself as the token (deterministic; unique).
                dm.or_set.adds.push((entry.device_id, entry.device_id));
            }
        }

        dm
    }

    /// Write the current device list back into `vault` and bump `vault.version`.
    pub fn flush(&self, vault: &mut Vault) {
        vault.devices.clone_from(&self.entries);
        vault.version += 1;
        vault.updated_at = Utc::now();
    }

    /// Admit a new device to the vault.
    ///
    /// Adds the device to both the OR-Set and the entries list, then records
    /// which device (`admitted_by`) performed the operation.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DeviceNotFound`] if `admitted_by` is not a live device.
    /// Returns [`Error::DeviceRevoked`] if `admitted_by` has been revoked.
    pub fn admit(
        &mut self,
        material: &DeviceKeyMaterial,
        admitted_by: &DeviceIdentity,
    ) -> Result<DeviceEntry> {
        self.check_admin_live(admitted_by.device_id)?;

        let now = Utc::now();
        let entry = DeviceEntry {
            device_id: material.device_id,
            nostr_pubkey: material.pubkey_hex.clone(),
            label: material.label.clone(),
            added_at: now,
            added_by: admitted_by.device_id,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
        };

        // OR-Set add with deterministic token = device_id.
        self.or_set
            .adds
            .push((material.device_id, material.device_id));
        self.entries.push(entry.clone());

        Ok(entry)
    }

    /// Admit the very first device to a newly-created, empty vault.
    ///
    /// Unlike [`admit`], this method does not require an existing admin device
    /// — it bootstraps the device list with the first device.  Must only be
    /// called on an empty `DeviceManager`; returns an error if any device is
    /// already present.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Crypto`] if the device list is not empty.
    pub fn bootstrap(&mut self, material: &DeviceKeyMaterial) -> Result<DeviceEntry> {
        if !self.entries.is_empty() {
            return Err(Error::Crypto(
                "bootstrap called on a non-empty DeviceManager".into(),
            ));
        }

        let now = Utc::now();
        let entry = DeviceEntry {
            device_id: material.device_id,
            nostr_pubkey: material.pubkey_hex.clone(),
            label: material.label.clone(),
            added_at: now,
            added_by: material.device_id, // self-admitted
            revoked: false,
            revoked_at: None,
            revoked_by: None,
        };

        self.or_set
            .adds
            .push((material.device_id, material.device_id));
        self.entries.push(entry.clone());

        Ok(entry)
    }

    /// Revoke a device.
    ///
    /// The device is removed from the OR-Set's live set and its [`DeviceEntry`]
    /// is marked `revoked = true` with timestamp and revoker.
    ///
    /// # Errors
    ///
    /// - [`Error::DeviceNotFound`] if `target_device_id` is not known.
    /// - [`Error::DeviceRevoked`] if `target_device_id` is already revoked.
    /// - [`Error::DeviceNotFound`] if `revoked_by` is not a live device.
    /// - [`Error::DeviceRevoked`] if `revoked_by` has been revoked.
    pub fn revoke(&mut self, target_device_id: Uuid, revoked_by: &DeviceIdentity) -> Result<()> {
        self.check_admin_live(revoked_by.device_id)?;

        // Confirm target exists and is not already revoked.
        let entry = self
            .entries
            .iter()
            .find(|e| e.device_id == target_device_id)
            .ok_or(Error::DeviceNotFound(target_device_id))?;

        if entry.revoked {
            return Err(Error::DeviceRevoked(target_device_id));
        }

        // OR-Set remove.
        self.or_set.remove(&target_device_id);

        // Mark entry revoked.
        let now = Utc::now();
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.device_id == target_device_id)
        {
            entry.revoked = true;
            entry.revoked_at = Some(now);
            entry.revoked_by = Some(revoked_by.device_id);
        }

        Ok(())
    }

    /// Merge the device list from another `DeviceManager` (e.g. from a remote
    /// replica received over Nostr) into this one.
    ///
    /// Uses OR-Set merge semantics:
    /// - Any device added on the remote that is not known locally is added.
    /// - Any device revoked on the remote is also revoked locally.
    /// - Concurrent add + revoke resolves as revoked (the remote revoke is
    ///   applied to the merged result).
    ///
    /// The `entries` list is updated to match the merged CRDT state.
    pub fn merge(&mut self, remote: &DeviceManager) {
        // Merge OR-Sets.
        self.or_set.merge(&remote.or_set);

        // Union entries: add any device entry we don't know about yet.
        for remote_entry in &remote.entries {
            if !self
                .entries
                .iter()
                .any(|e| e.device_id == remote_entry.device_id)
            {
                self.entries.push(remote_entry.clone());
            }
        }

        // Apply revocations from remote.
        for remote_entry in remote.entries.iter().filter(|e| e.revoked) {
            if let Some(local_entry) = self
                .entries
                .iter_mut()
                .find(|e| e.device_id == remote_entry.device_id && !e.revoked)
            {
                local_entry.revoked = true;
                local_entry.revoked_at = remote_entry.revoked_at;
                local_entry.revoked_by = remote_entry.revoked_by;
            }
        }

        // Sync entries revoke-status with OR-Set: if a device_id is not in the
        // live adds set, ensure its entry is marked revoked.
        let live: std::collections::HashSet<Uuid> =
            self.or_set.adds.iter().map(|(id, _)| *id).collect();
        for entry in &mut self.entries {
            if !live.contains(&entry.device_id) && !entry.revoked {
                entry.revoked = true;
                entry.revoked_at = Some(Utc::now());
                // No revoker recorded for CRDT-implicit revocation.
            }
        }
    }

    /// Return a slice of all known device entries (live + revoked).
    #[must_use]
    pub fn entries(&self) -> &[DeviceEntry] {
        &self.entries
    }

    /// Return a vec of device entries for currently live (non-revoked) devices.
    #[must_use]
    pub fn live_devices(&self) -> Vec<&DeviceEntry> {
        self.entries.iter().filter(|e| !e.revoked).collect()
    }

    /// Return the entry for a specific device, or `None` if not known.
    #[must_use]
    pub fn get_entry(&self, device_id: Uuid) -> Option<&DeviceEntry> {
        self.entries.iter().find(|e| e.device_id == device_id)
    }

    // ── Private helpers ───────────────────────────────────────────────────

    /// Check that `admin_id` is a live, non-revoked device.
    fn check_admin_live(&self, admin_id: Uuid) -> Result<()> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.device_id == admin_id)
            .ok_or(Error::DeviceNotFound(admin_id))?;

        if entry.revoked {
            return Err(Error::DeviceRevoked(admin_id));
        }

        Ok(())
    }
}

// ─── Private helpers ─────────────────────────────────────────────────────────

/// Return the secure-storage key path for a device's secret key.
fn device_secret_key_path(device_id: Uuid) -> String {
    format!("zvault/device/{device_id}/secret_key")
}

/// Convert a secp256k1 verifying key to a Nostr-style x-only pubkey hex string
/// (32 bytes = 64 hex chars).
fn verifying_key_to_hex(vk: &k256::ecdsa::VerifyingKey) -> String {
    // Compress to SEC1 33-byte form: [0x02 or 0x03] || [x-coordinate 32 bytes]
    // The Nostr x-only pubkey is just the 32-byte x coordinate.
    // `to_encoded_point` is a method on VerifyingKey re-exported by k256.
    let point = vk.to_encoded_point(true); // compressed
    let bytes = point.as_bytes();
    // bytes[0] is the prefix (02/03); bytes[1..33] is the x coordinate.
    hex::encode(&bytes[1..33])
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// Create a fresh device identity + material using InMemoryStorage.
    fn new_device(label: &str) -> (DeviceIdentity, DeviceKeyMaterial, InMemoryStorage) {
        let storage = InMemoryStorage::default();
        let (identity, material) =
            DeviceIdentity::generate(label, &storage).expect("generate should succeed");
        (identity, material, storage)
    }

    /// Create an empty DeviceManager (as if for a brand-new vault).
    fn empty_dm() -> DeviceManager {
        DeviceManager {
            or_set: OrSet::new(),
            entries: Vec::new(),
        }
    }

    // ── DeviceIdentity::generate ──────────────────────────────────────────────

    #[test]
    fn generate_produces_valid_pubkey_hex() {
        let storage = InMemoryStorage::default();
        let (identity, material) =
            DeviceIdentity::generate("My Device", &storage).expect("generate should succeed");

        // Pubkey is 64 hex chars (32 bytes = x-coordinate of secp256k1 point).
        assert_eq!(identity.pubkey_hex.len(), 64, "pubkey must be 64 hex chars");
        assert!(
            identity.pubkey_hex.chars().all(|c| c.is_ascii_hexdigit()),
            "pubkey must be valid hex"
        );
        assert_eq!(identity.device_id, material.device_id);
        assert_eq!(identity.pubkey_hex, material.pubkey_hex);
    }

    #[test]
    fn generate_stores_secret_key_in_storage() {
        let storage = InMemoryStorage::default();
        let (identity, _) =
            DeviceIdentity::generate("My Device", &storage).expect("generate should succeed");

        // Secret key must be loadable from storage.
        let secret = identity
            .load_secret_key(&storage)
            .expect("load_secret_key should succeed");
        assert_eq!(secret.len(), 32, "secp256k1 secret key must be 32 bytes");
    }

    #[test]
    fn generate_unique_device_ids() {
        let storage = InMemoryStorage::default();
        let (id1, _) = DeviceIdentity::generate("A", &storage).expect("generate A");
        let (id2, _) = DeviceIdentity::generate("B", &storage).expect("generate B");

        assert_ne!(id1.device_id, id2.device_id);
        assert_ne!(id1.pubkey_hex, id2.pubkey_hex);
    }

    #[test]
    fn load_secret_key_wrong_storage_returns_error() {
        let storage1 = InMemoryStorage::default();
        let (identity, _) = DeviceIdentity::generate("A", &storage1).expect("generate");

        // Different empty storage — key not present.
        let storage2 = InMemoryStorage::default();
        let result = identity.load_secret_key(&storage2);
        assert!(result.is_err(), "load from wrong storage must fail");
    }

    // ── OR-Set CRDT ───────────────────────────────────────────────────────────

    #[test]
    fn or_set_add_contains() {
        let mut set: OrSet<&str> = OrSet::new();
        set.add("alice");
        assert!(set.contains(&"alice"));
        assert!(!set.contains(&"bob"));
    }

    #[test]
    fn or_set_remove() {
        let mut set: OrSet<&str> = OrSet::new();
        set.add("alice");
        let removed = set.remove(&"alice");
        assert!(
            removed,
            "remove should return true when element was present"
        );
        assert!(!set.contains(&"alice"));
    }

    #[test]
    fn or_set_remove_nonexistent_is_idempotent() {
        let mut set: OrSet<&str> = OrSet::new();
        let removed = set.remove(&"ghost");
        assert!(!removed, "remove of absent element should return false");
    }

    #[test]
    fn or_set_add_wins_concurrent_add_and_remove() {
        // Simulate concurrent add on replica B while replica A removes.
        let mut replica_a: OrSet<Uuid> = OrSet::new();
        let mut replica_b: OrSet<Uuid> = OrSet::new();

        let id = Uuid::new_v4();

        // Both replicas start with the element.
        let token = Uuid::new_v4();
        replica_a.adds.push((id, token));
        replica_b.adds.push((id, token));

        // Replica A removes it.
        replica_a.remove(&id);

        // Replica B concurrently adds it again (new token).
        replica_b.add(id);

        // Merge A's state into B.
        replica_b.merge(&replica_a);

        // Add-wins: the concurrent add from B survives.
        assert!(
            replica_b.contains(&id),
            "add-wins: concurrent add should survive merge"
        );
    }

    #[test]
    fn or_set_merge_union_of_elements() {
        let mut a: OrSet<&str> = OrSet::new();
        let mut b: OrSet<&str> = OrSet::new();

        a.add("alice");
        b.add("bob");

        a.merge(&b);

        assert!(a.contains(&"alice"));
        assert!(a.contains(&"bob"));
    }

    #[test]
    fn or_set_merge_idempotent() {
        let mut a: OrSet<&str> = OrSet::new();
        a.add("alice");

        let b = a.clone();
        a.merge(&b);
        a.merge(&b);

        // alice appears exactly once in the live set.
        assert_eq!(a.elements().count(), 1);
    }

    #[test]
    fn or_set_merge_remove_wins_observed() {
        // If replica A removes before B knows about it, B's merge should apply
        // the remove (the token was observed at add-time on B too).
        let mut a: OrSet<Uuid> = OrSet::new();
        let mut b: OrSet<Uuid> = OrSet::new();

        let id = Uuid::new_v4();
        let token = Uuid::new_v4();

        // Both replicas have the add.
        a.adds.push((id, token));
        b.adds.push((id, token));

        // A removes (the same token is now in A's removes set).
        a.remove(&id);

        // B merges A. Since A's removes set contains the only token for `id`,
        // the element should be gone from B after merge.
        b.merge(&a);

        assert!(
            !b.contains(&id),
            "remove of observed token should propagate via merge"
        );
    }

    // ── DeviceManager ─────────────────────────────────────────────────────────

    #[test]
    fn bootstrap_empty_vault() {
        let mut dm = empty_dm();
        let (_, material, _) = new_device("First Device");

        let entry = dm.bootstrap(&material).expect("bootstrap should succeed");
        assert_eq!(entry.device_id, material.device_id);
        assert_eq!(entry.nostr_pubkey, material.pubkey_hex);
        assert!(!entry.revoked);

        assert_eq!(dm.live_devices().len(), 1);
    }

    #[test]
    fn bootstrap_twice_returns_error() {
        let mut dm = empty_dm();
        let (_, material1, _) = new_device("Dev1");
        let (_, material2, _) = new_device("Dev2");

        dm.bootstrap(&material1).expect("first bootstrap ok");
        let err = dm.bootstrap(&material2).unwrap_err();
        assert!(
            matches!(err, Error::Crypto(_)),
            "expected Crypto error on second bootstrap"
        );
    }

    #[test]
    fn admit_device_by_existing_admin() {
        let mut dm = empty_dm();
        let (admin_id, admin_material, _) = new_device("Admin");
        let (_, new_material, _) = new_device("New Device");

        dm.bootstrap(&admin_material).expect("bootstrap");

        let admin_identity = DeviceIdentity {
            device_id: admin_id.device_id,
            pubkey_hex: admin_id.pubkey_hex.clone(),
        };

        let entry = dm
            .admit(&new_material, &admin_identity)
            .expect("admit should succeed");
        assert_eq!(entry.device_id, new_material.device_id);
        assert_eq!(entry.added_by, admin_id.device_id);
        assert!(!entry.revoked);

        assert_eq!(dm.live_devices().len(), 2);
    }

    #[test]
    fn admit_by_unknown_device_fails() {
        let mut dm = empty_dm();
        let (_, material, _) = new_device("New Device");

        let ghost_identity = DeviceIdentity {
            device_id: Uuid::new_v4(),
            pubkey_hex: "a".repeat(64),
        };

        let err = dm.admit(&material, &ghost_identity).unwrap_err();
        assert!(matches!(err, Error::DeviceNotFound(_)));
    }

    #[test]
    fn revoke_device() {
        let mut dm = empty_dm();
        let (admin_id, admin_material, _) = new_device("Admin");
        let (target_id, target_material, _) = new_device("Target");

        dm.bootstrap(&admin_material).expect("bootstrap");

        let admin_identity = DeviceIdentity {
            device_id: admin_id.device_id,
            pubkey_hex: admin_id.pubkey_hex.clone(),
        };

        dm.admit(&target_material, &admin_identity).expect("admit");
        assert_eq!(dm.live_devices().len(), 2);

        dm.revoke(target_id.device_id, &admin_identity)
            .expect("revoke should succeed");
        assert_eq!(dm.live_devices().len(), 1, "only admin should remain live");

        let revoked_entry = dm.get_entry(target_id.device_id).unwrap();
        assert!(revoked_entry.revoked);
        assert!(revoked_entry.revoked_at.is_some());
        assert_eq!(revoked_entry.revoked_by, Some(admin_id.device_id));
    }

    #[test]
    fn revoke_already_revoked_returns_error() {
        let mut dm = empty_dm();
        let (admin_id, admin_material, _) = new_device("Admin");
        let (target_id, target_material, _) = new_device("Target");

        dm.bootstrap(&admin_material).expect("bootstrap");
        let admin_identity = DeviceIdentity {
            device_id: admin_id.device_id,
            pubkey_hex: admin_id.pubkey_hex.clone(),
        };
        dm.admit(&target_material, &admin_identity).expect("admit");
        dm.revoke(target_id.device_id, &admin_identity)
            .expect("first revoke");

        let err = dm.revoke(target_id.device_id, &admin_identity).unwrap_err();
        assert!(matches!(err, Error::DeviceRevoked(_)));
    }

    #[test]
    fn revoke_by_revoked_device_fails() {
        let mut dm = empty_dm();
        let (admin_id, admin_material, _) = new_device("Admin");
        let (b_id, b_material, _) = new_device("B");
        let (c_id, c_material, _) = new_device("C");

        dm.bootstrap(&admin_material).expect("bootstrap");
        let admin_identity = DeviceIdentity {
            device_id: admin_id.device_id,
            pubkey_hex: admin_id.pubkey_hex.clone(),
        };
        dm.admit(&b_material, &admin_identity).expect("admit B");
        dm.admit(&c_material, &admin_identity).expect("admit C");

        // Revoke B.
        dm.revoke(b_id.device_id, &admin_identity)
            .expect("revoke B");

        // B tries to revoke C.
        let b_identity = DeviceIdentity {
            device_id: b_id.device_id,
            pubkey_hex: b_id.pubkey_hex.clone(),
        };
        let err = dm.revoke(c_id.device_id, &b_identity).unwrap_err();
        assert!(matches!(err, Error::DeviceRevoked(_)));
    }

    #[test]
    fn revoke_unknown_device_returns_not_found() {
        let mut dm = empty_dm();
        let (admin_id, admin_material, _) = new_device("Admin");
        dm.bootstrap(&admin_material).expect("bootstrap");
        let admin_identity = DeviceIdentity {
            device_id: admin_id.device_id,
            pubkey_hex: admin_id.pubkey_hex.clone(),
        };

        let err = dm.revoke(Uuid::new_v4(), &admin_identity).unwrap_err();
        assert!(matches!(err, Error::DeviceNotFound(_)));
    }

    #[test]
    fn flush_writes_devices_to_vault() {
        let mut vault = Vault::new();
        assert_eq!(vault.version, 0);

        let mut dm = DeviceManager::from_vault(&vault);
        let (_, material, _) = new_device("First");
        dm.bootstrap(&material).expect("bootstrap");
        dm.flush(&mut vault);

        assert_eq!(vault.devices.len(), 1);
        assert!(vault.version > 0, "flush must bump vault.version");
    }

    #[test]
    fn from_vault_reconstructs_live_set() {
        let mut vault = Vault::new();

        // Manually build a vault with one live and one revoked device entry.
        let now = Utc::now();
        let live_id = Uuid::new_v4();
        let revoked_id = Uuid::new_v4();

        vault.devices.push(DeviceEntry {
            device_id: live_id,
            nostr_pubkey: "a".repeat(64),
            label: "Live".into(),
            added_at: now,
            added_by: live_id,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
        });
        vault.devices.push(DeviceEntry {
            device_id: revoked_id,
            nostr_pubkey: "b".repeat(64),
            label: "Revoked".into(),
            added_at: now,
            added_by: live_id,
            revoked: true,
            revoked_at: Some(now),
            revoked_by: Some(live_id),
        });

        let dm = DeviceManager::from_vault(&vault);

        assert_eq!(dm.live_devices().len(), 1);
        assert_eq!(dm.live_devices()[0].device_id, live_id);
        assert_eq!(dm.entries().len(), 2);
    }

    // ── CRDT merge ────────────────────────────────────────────────────────────

    #[test]
    fn merge_disjoint_device_lists() {
        // Two replicas each admit a different device; after merge both should
        // be present.
        let (_, m1, _) = new_device("Root");
        let (_, m2, _) = new_device("DevA");
        let (_, m3, _) = new_device("DevB");

        let mut dm_a = empty_dm();
        dm_a.bootstrap(&m1).expect("bootstrap A");

        // Simulate replica B starting from the same state.
        let mut dm_b = dm_a.clone();

        // Create admin identity for both replicas.
        let admin_identity = DeviceIdentity {
            device_id: m1.device_id,
            pubkey_hex: m1.pubkey_hex.clone(),
        };

        dm_a.admit(&m2, &admin_identity)
            .expect("admit DevA on replica A");
        dm_b.admit(&m3, &admin_identity)
            .expect("admit DevB on replica B");

        // Merge B into A.
        dm_a.merge(&dm_b);

        assert_eq!(
            dm_a.live_devices().len(),
            3,
            "merge should produce 3 live devices"
        );
    }

    #[test]
    fn merge_remote_revocation_propagates() {
        let (_, m_root, _) = new_device("Root");
        let (_, m_target, _) = new_device("Target");

        let admin_identity = DeviceIdentity {
            device_id: m_root.device_id,
            pubkey_hex: m_root.pubkey_hex.clone(),
        };

        let mut dm_local = empty_dm();
        dm_local.bootstrap(&m_root).expect("bootstrap");
        dm_local
            .admit(&m_target, &admin_identity)
            .expect("admit Target");

        // Remote replica has revoked Target.
        let mut dm_remote = dm_local.clone();
        dm_remote
            .revoke(m_target.device_id, &admin_identity)
            .expect("remote revoke");

        // Local merges remote.
        dm_local.merge(&dm_remote);

        assert_eq!(
            dm_local.live_devices().len(),
            1,
            "revocation should propagate: only Root is live"
        );
        let revoked = dm_local.get_entry(m_target.device_id).unwrap();
        assert!(revoked.revoked, "Target must be marked revoked");
    }

    #[test]
    fn merge_is_idempotent() {
        let (_, m_root, _) = new_device("Root");
        let (_, m_b, _) = new_device("B");

        let admin_identity = DeviceIdentity {
            device_id: m_root.device_id,
            pubkey_hex: m_root.pubkey_hex.clone(),
        };

        let mut dm = empty_dm();
        dm.bootstrap(&m_root).expect("bootstrap");

        let mut remote = dm.clone();
        remote.admit(&m_b, &admin_identity).expect("admit B");

        dm.merge(&remote);
        dm.merge(&remote);
        dm.merge(&remote);

        // B should appear exactly once.
        let live = dm.live_devices();
        let b_count = live.iter().filter(|e| e.device_id == m_b.device_id).count();
        assert_eq!(b_count, 1, "merge should be idempotent");
    }

    // ── Integration: vault round-trip ────────────────────────────────────────

    #[test]
    fn vault_roundtrip_with_devices() {
        use crate::vault::Vault;

        let (_, m_admin, _) = new_device("Admin");
        let (_, m_laptop, _) = new_device("Laptop");

        let admin_identity = DeviceIdentity {
            device_id: m_admin.device_id,
            pubkey_hex: m_admin.pubkey_hex.clone(),
        };

        let mut vault = Vault::new();
        let mut dm = DeviceManager::from_vault(&vault);

        dm.bootstrap(&m_admin).expect("bootstrap");
        dm.admit(&m_laptop, &admin_identity).expect("admit Laptop");
        dm.flush(&mut vault);

        assert_eq!(vault.devices.len(), 2);
        assert!(vault.version > 0);

        // Reconstruct manager from flushed vault.
        let dm2 = DeviceManager::from_vault(&vault);
        assert_eq!(dm2.live_devices().len(), 2);
    }

    #[test]
    fn vault_roundtrip_with_revocation() {
        use crate::vault::Vault;

        let (_, m_admin, _) = new_device("Admin");
        let (target_id, m_target, _) = new_device("Target");

        let admin_identity = DeviceIdentity {
            device_id: m_admin.device_id,
            pubkey_hex: m_admin.pubkey_hex.clone(),
        };

        let mut vault = Vault::new();
        let mut dm = DeviceManager::from_vault(&vault);

        dm.bootstrap(&m_admin).expect("bootstrap");
        dm.admit(&m_target, &admin_identity).expect("admit Target");
        dm.revoke(target_id.device_id, &admin_identity)
            .expect("revoke Target");
        dm.flush(&mut vault);

        assert_eq!(vault.devices.len(), 2);

        // Reconstruct — only Admin is live.
        let dm2 = DeviceManager::from_vault(&vault);
        assert_eq!(dm2.live_devices().len(), 1);
        assert_eq!(dm2.live_devices()[0].device_id, m_admin.device_id);
    }

    // ── Edge-case tests (security review) ─────────────────────────────────────

    #[test]
    fn or_set_merge_is_commutative() {
        // A.merge(B) should produce the same live set as B.merge(A).
        let mut a: OrSet<Uuid> = OrSet::new();
        let mut b: OrSet<Uuid> = OrSet::new();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        // a has id1 and id2; b has id2 and id3.
        a.add(id1);
        let shared_token = Uuid::new_v4();
        a.adds.push((id2, shared_token));
        b.adds.push((id2, shared_token));
        b.add(id3);

        // a removes id2.
        a.remove(&id2);

        // Merge in both directions.
        let mut ab = a.clone();
        ab.merge(&b);

        let mut ba = b.clone();
        ba.merge(&a);

        // Extract live elements.
        let mut ab_elems: Vec<Uuid> = ab.elements().copied().collect();
        let mut ba_elems: Vec<Uuid> = ba.elements().copied().collect();
        ab_elems.sort();
        ba_elems.sort();

        assert_eq!(ab_elems, ba_elems, "OR-Set merge must be commutative");
    }

    #[test]
    fn device_manager_merge_commutative() {
        let (_, m_root, _) = new_device("Root");
        let (_, m_a, _) = new_device("DevA");
        let (_, m_b, _) = new_device("DevB");

        let admin_identity = DeviceIdentity {
            device_id: m_root.device_id,
            pubkey_hex: m_root.pubkey_hex.clone(),
        };

        let mut dm_base = empty_dm();
        dm_base.bootstrap(&m_root).expect("bootstrap");

        let mut dm_x = dm_base.clone();
        let mut dm_y = dm_base.clone();

        dm_x.admit(&m_a, &admin_identity).expect("admit A on X");
        dm_y.admit(&m_b, &admin_identity).expect("admit B on Y");

        // Merge X into Y.
        let mut xy = dm_x.clone();
        xy.merge(&dm_y);

        // Merge Y into X.
        let mut yx = dm_y.clone();
        yx.merge(&dm_x);

        // Both should have the same set of live devices.
        let mut xy_ids: Vec<Uuid> = xy.live_devices().iter().map(|e| e.device_id).collect();
        let mut yx_ids: Vec<Uuid> = yx.live_devices().iter().map(|e| e.device_id).collect();
        xy_ids.sort();
        yx_ids.sort();

        assert_eq!(xy_ids, yx_ids, "DeviceManager merge must be commutative");
        assert_eq!(xy_ids.len(), 3); // root + A + B
    }
}
