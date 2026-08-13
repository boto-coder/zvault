//! Tamper-evident audit log for ZVault.
//!
//! This module provides:
//!
//! - [`AuditEntry`] — a single log record with a monotonic sequence number and
//!   an HMAC-SHA256 chain link tying it to the previous entry.
//! - [`AuditLog`] — append-only collection of entries, encrypted with the
//!   same AES-256-GCM key as the vault file, stored at `<vault>.audit`.
//! - [`AuditLog::verify_chain`] — walk every entry, recompute each HMAC, and
//!   return `false` if any link is broken (indicating tampering or corruption).
//!
//! The chain key is derived from the vault master key via HKDF:
//! `HKDF(vault_master_key, info="audit_chain_key")`.

use chrono::{DateTime, Utc};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{decrypt, encrypt, VaultKey};
use crate::{Error, Result};

/// HKDF info string used to derive the audit chain key from the vault master key.
const CHAIN_KEY_INFO: &[u8] = b"audit_chain_key";

/// The HMAC used as `prev_hmac` for the very first entry in the log.
const ZERO_HMAC: [u8; 32] = [0u8; 32];

// ─── EventKind ───────────────────────────────────────────────────────────────

/// All security-relevant event categories that are written to the audit log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    // Vault access
    /// The vault was successfully unlocked.
    VaultUnlocked,
    /// An unlock attempt failed (wrong password / biometric rejected).
    VaultUnlockFailed,
    /// The vault was locked (manual or session timeout).
    VaultLocked,
    /// The session was locked due to inactivity timeout.
    VaultSessionTimeout,

    // Vault mutations
    /// A new vault item was created.
    ItemCreated,
    /// An existing vault item was updated.
    ItemUpdated,
    /// A vault item was deleted.
    ItemDeleted,
    /// The vault was re-keyed (new master password + new encryption key).
    VaultRekeyed,

    // Device lifecycle
    /// A new device was admitted to the vault.
    DeviceAdded,
    /// A device was revoked.
    DeviceRevoked,
    /// A device's label was renamed.
    DeviceRenamed,

    // Sync
    /// A Nostr sync cycle started.
    SyncStarted,
    /// A Nostr sync cycle completed.
    SyncCompleted,
    /// A conflict was resolved (LWW or OR-Set).
    ConflictResolved,
    /// An incoming sync message was rejected (bad signature / stale clock /
    /// revoked sender).
    MessageRejected,

    // Import / export
    /// An import completed successfully.
    ImportCompleted,
    /// An encrypted `.zvault-export` file was created.
    ExportCreated,
    /// A plaintext export was created (CSV or JSON).
    PlaintextExportCreated,

    // Biometric
    /// The user enabled biometric unlock.
    BiometricEnabled,
    /// The user disabled biometric unlock.
    BiometricDisabled,
    /// The OS invalidated the biometric key (e.g., new fingerprint enrolled).
    BiometricInvalidated,

    // Auth
    /// The master password was changed.
    MasterPasswordChanged,
    /// Biometric authentication succeeded.
    BiometricAuthSuccess,
    /// Biometric authentication failed.
    BiometricAuthFailure,
}

// ─── AuditEntry ──────────────────────────────────────────────────────────────

/// A single entry in the tamper-evident audit log.
///
/// Each entry's `hmac` covers `seq || timestamp || event || detail ||
/// prev_hmac` keyed with the audit chain key (derived from the vault master
/// key via HKDF). Any mutation to any field, or deletion / reordering of
/// entries, will break the chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Monotonically increasing per-device sequence number.
    pub seq: u64,
    /// When this event occurred (UTC).
    pub timestamp: DateTime<Utc>,
    /// Which device generated this entry.
    pub device_id: Uuid,
    /// The category of security-relevant event.
    pub event: EventKind,
    /// Human-readable summary. Must not contain plaintext credentials.
    pub detail: String,
    /// HMAC-SHA256 of the *previous* entry (or `[0u8; 32]` for the first entry).
    pub prev_hmac: [u8; 32],
    /// HMAC-SHA256 of this entry's own content including `prev_hmac`.
    pub hmac: [u8; 32],
}

// ─── Chain Key Derivation ────────────────────────────────────────────────────

/// Derive the audit chain key from the vault master key using HKDF-SHA256.
///
/// The chain key is used to compute HMAC-SHA256 chain links in the audit log.
/// It is derived deterministically from the vault key so it does not need to be
/// stored separately.
///
/// # Arguments
///
/// * `vault_key` — The vault master key (32 bytes).
///
/// # Returns
///
/// A `Zeroizing<[u8; 32]>` containing the derived chain key.
///
/// # Panics
///
/// Panics if HKDF expansion fails, which cannot happen when expanding to 32
/// bytes with SHA-256 (max output is 255 × 32 = 8160 bytes).
#[must_use]
pub fn derive_chain_key(vault_key: &VaultKey) -> Zeroizing<[u8; 32]> {
    // HKDF-extract with no salt (uses a zero-filled salt internally).
    let hk = Hkdf::<Sha256>::new(None, vault_key.as_bytes());
    let mut chain_key = Zeroizing::new([0u8; 32]);
    // expand cannot fail when output length <= 255 * HashLen (8160 bytes for SHA-256).
    hk.expand(CHAIN_KEY_INFO, chain_key.as_mut())
        .expect("infallible: HKDF expand for 32 bytes with SHA-256");
    chain_key
}

// ─── HMAC Computation ────────────────────────────────────────────────────────

/// Compute the HMAC-SHA256 for an audit entry.
///
/// The message is: `seq_bytes(8) || device_id(16) || timestamp_bytes || event_bytes || detail_bytes || prev_hmac(32)`
///
/// Where `timestamp_bytes`, `event_bytes`, and `detail_bytes` are the UTF-8
/// representation of the respective fields.
///
/// All variable-length fields are length-prefixed (4-byte LE length + data) to
/// prevent cross-field collision attacks.
fn compute_entry_hmac(
    chain_key: &[u8; 32],
    seq: u64,
    device_id: &Uuid,
    timestamp: &DateTime<Utc>,
    event: &EventKind,
    detail: &str,
    prev_hmac: &[u8; 32],
) -> [u8; 32] {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(chain_key).expect("infallible: HMAC accepts any key size");

    // seq as 8 bytes little-endian (fixed size)
    mac.update(&seq.to_le_bytes());

    // device_id as 16 bytes (fixed size UUID)
    mac.update(device_id.as_bytes());

    // timestamp as length-prefixed RFC 3339 string bytes
    let ts_str = timestamp.to_rfc3339();
    let ts_bytes = ts_str.as_bytes();
    #[allow(clippy::cast_possible_truncation)] // RFC3339 timestamp is always < 40 bytes
    mac.update(&(ts_bytes.len() as u32).to_le_bytes());
    mac.update(ts_bytes);

    // event as length-prefixed JSON string bytes (deterministic serialisation)
    let event_str =
        serde_json::to_string(event).expect("infallible: EventKind serialisation cannot fail");
    let event_bytes = event_str.as_bytes();
    #[allow(clippy::cast_possible_truncation)] // EventKind JSON is always < 100 bytes
    mac.update(&(event_bytes.len() as u32).to_le_bytes());
    mac.update(event_bytes);

    // detail as length-prefixed UTF-8 bytes
    let detail_bytes = detail.as_bytes();
    #[allow(clippy::cast_possible_truncation)] // detail is a short human-readable string
    mac.update(&(detail_bytes.len() as u32).to_le_bytes());
    mac.update(detail_bytes);

    // prev_hmac (32 bytes, fixed size)
    mac.update(prev_hmac);

    let result = mac.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result.into_bytes());
    out
}

// ─── AuditLog ────────────────────────────────────────────────────────────────

/// Append-only collection of audit entries for one device.
///
/// Stored encrypted alongside the vault file at `<vault_name>.audit`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditLog {
    entries: Vec<AuditEntry>,
}

impl AuditLog {
    /// Create an empty audit log.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Return the number of entries in the log.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return `true` if the log contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return a slice of all entries in the log.
    #[must_use]
    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    /// Append a new entry to the audit log, computing its HMAC chain link.
    ///
    /// The entry's `seq`, `prev_hmac`, and `hmac` fields are computed
    /// automatically. The caller provides the `timestamp`, `device_id`,
    /// `event`, and `detail`.
    ///
    /// # Arguments
    ///
    /// * `chain_key` — The HMAC chain key derived from the vault master key.
    /// * `timestamp` — When the event occurred (UTC).
    /// * `device_id` — Which device generated this entry.
    /// * `event` — The event category.
    /// * `detail` — Human-readable description (must not contain credentials).
    pub fn append(
        &mut self,
        chain_key: &Zeroizing<[u8; 32]>,
        timestamp: DateTime<Utc>,
        device_id: Uuid,
        event: EventKind,
        detail: String,
    ) {
        let seq = self.entries.len() as u64;
        let prev_hmac = self.entries.last().map_or(ZERO_HMAC, |prev| prev.hmac);

        let hmac = compute_entry_hmac(
            chain_key, seq, &device_id, &timestamp, &event, &detail, &prev_hmac,
        );

        self.entries.push(AuditEntry {
            seq,
            timestamp,
            device_id,
            event,
            detail,
            prev_hmac,
            hmac,
        });
    }

    /// Walk every entry and recompute each HMAC chain link.
    ///
    /// Returns `true` if the chain is intact, `false` if any entry has been
    /// tampered with, deleted, or reordered.
    ///
    /// # Arguments
    ///
    /// * `chain_key` — The HMAC chain key derived from the vault master key.
    #[must_use]
    pub fn verify_chain(&self, chain_key: &Zeroizing<[u8; 32]>) -> bool {
        let mut expected_prev_hmac = ZERO_HMAC;

        for (i, entry) in self.entries.iter().enumerate() {
            // Check sequence number is monotonically increasing from 0
            if entry.seq != i as u64 {
                return false;
            }

            // Check prev_hmac matches what we expect
            if entry.prev_hmac != expected_prev_hmac {
                return false;
            }

            // Recompute this entry's HMAC
            let expected_hmac = compute_entry_hmac(
                chain_key,
                entry.seq,
                &entry.device_id,
                &entry.timestamp,
                &entry.event,
                &entry.detail,
                &entry.prev_hmac,
            );

            if entry.hmac != expected_hmac {
                return false;
            }

            expected_prev_hmac = entry.hmac;
        }

        true
    }

    /// Serialise the audit log to JSON and encrypt it with the vault key
    /// using AES-256-GCM.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Serialisation`] if JSON serialisation fails, or
    /// [`Error::Crypto`] if encryption fails.
    pub fn to_encrypted_bytes(&self, vault_key: &VaultKey) -> Result<Vec<u8>> {
        let json =
            Zeroizing::new(serde_json::to_vec(self).map_err(|e| {
                Error::Serialisation(format!("audit log serialisation failed: {e}"))
            })?);
        encrypt(vault_key, &json)
    }

    /// Decrypt and deserialise an audit log from encrypted bytes.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidVaultFile`] if decryption fails (wrong key or
    /// tampered data), or [`Error::Serialisation`] if the decrypted JSON is
    /// malformed.
    pub fn from_encrypted_bytes(vault_key: &VaultKey, bytes: &[u8]) -> Result<Self> {
        let plaintext = Zeroizing::new(decrypt(vault_key, bytes)?);
        let log: Self = serde_json::from_slice(&plaintext)
            .map_err(|e| Error::Serialisation(format!("audit log deserialisation failed: {e}")))?;
        Ok(log)
    }
}

// ─── Helper functions for common audit entries ───────────────────────────────

/// Record that a vault item was created.
pub fn item_created(
    log: &mut AuditLog,
    chain_key: &Zeroizing<[u8; 32]>,
    device_id: Uuid,
    item_id: Uuid,
    item_name: &str,
) {
    log.append(
        chain_key,
        Utc::now(),
        device_id,
        EventKind::ItemCreated,
        format!("Item created: {item_name} ({item_id})"),
    );
}

/// Record that a vault item was updated.
pub fn item_updated(
    log: &mut AuditLog,
    chain_key: &Zeroizing<[u8; 32]>,
    device_id: Uuid,
    item_id: Uuid,
    item_name: &str,
) {
    log.append(
        chain_key,
        Utc::now(),
        device_id,
        EventKind::ItemUpdated,
        format!("Item updated: {item_name} ({item_id})"),
    );
}

/// Record that a vault item was deleted.
pub fn item_deleted(
    log: &mut AuditLog,
    chain_key: &Zeroizing<[u8; 32]>,
    device_id: Uuid,
    item_id: Uuid,
    item_name: &str,
) {
    log.append(
        chain_key,
        Utc::now(),
        device_id,
        EventKind::ItemDeleted,
        format!("Item deleted: {item_name} ({item_id})"),
    );
}

/// Record that the vault was successfully unlocked.
pub fn vault_unlocked(log: &mut AuditLog, chain_key: &Zeroizing<[u8; 32]>, device_id: Uuid) {
    log.append(
        chain_key,
        Utc::now(),
        device_id,
        EventKind::VaultUnlocked,
        "Vault unlocked".to_string(),
    );
}

/// Record that a new device was admitted.
pub fn device_admitted(
    log: &mut AuditLog,
    chain_key: &Zeroizing<[u8; 32]>,
    device_id: Uuid,
    admitted_device_id: Uuid,
    label: &str,
) {
    log.append(
        chain_key,
        Utc::now(),
        device_id,
        EventKind::DeviceAdded,
        format!("Device admitted: {label} ({admitted_device_id})"),
    );
}

/// Record that a device was revoked.
pub fn device_revoked(
    log: &mut AuditLog,
    chain_key: &Zeroizing<[u8; 32]>,
    device_id: Uuid,
    revoked_device_id: Uuid,
    label: &str,
) {
    log.append(
        chain_key,
        Utc::now(),
        device_id,
        EventKind::DeviceRevoked,
        format!("Device revoked: {label} ({revoked_device_id})"),
    );
}

/// Record that an import operation completed.
pub fn import_completed(
    log: &mut AuditLog,
    chain_key: &Zeroizing<[u8; 32]>,
    device_id: Uuid,
    source: &str,
    item_count: usize,
) {
    log.append(
        chain_key,
        Utc::now(),
        device_id,
        EventKind::ImportCompleted,
        format!("Import completed from {source}: {item_count} items"),
    );
}

/// Record that an export operation completed.
pub fn export_completed(
    log: &mut AuditLog,
    chain_key: &Zeroizing<[u8; 32]>,
    device_id: Uuid,
    format: &str,
) {
    log.append(
        chain_key,
        Utc::now(),
        device_id,
        EventKind::ExportCreated,
        format!("Export completed: {format}"),
    );
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{derive_key, KdfParams};

    /// Create a test vault key with minimal KDF params for fast tests.
    fn test_key() -> VaultKey {
        let params = KdfParams {
            salt: [0x42u8; 32],
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        };
        derive_key("test-password", &params).expect("test key derivation failed")
    }

    /// Create a test chain key from a test vault key.
    fn test_chain_key() -> Zeroizing<[u8; 32]> {
        let key = test_key();
        derive_chain_key(&key)
    }

    fn test_device_id() -> Uuid {
        Uuid::parse_str("12345678-1234-1234-1234-123456789abc").unwrap()
    }

    // ── Test 1: append single entry ──────────────────────────────────────────

    #[test]
    fn append_single_entry() {
        let chain_key = test_chain_key();
        let mut log = AuditLog::new();

        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::VaultUnlocked,
            "Vault unlocked".to_string(),
        );

        assert_eq!(log.len(), 1);
        let entry = &log.entries()[0];
        assert_eq!(entry.seq, 0);
        assert_eq!(entry.prev_hmac, ZERO_HMAC);
        assert_ne!(entry.hmac, ZERO_HMAC);
    }

    // ── Test 2: append multiple entries builds chain ─────────────────────────

    #[test]
    fn append_multiple_entries_chain_links() {
        let chain_key = test_chain_key();
        let mut log = AuditLog::new();

        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::VaultUnlocked,
            "First".to_string(),
        );
        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::ItemCreated,
            "Second".to_string(),
        );
        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::ItemUpdated,
            "Third".to_string(),
        );

        assert_eq!(log.len(), 3);
        // Each entry's prev_hmac should equal the previous entry's hmac
        assert_eq!(log.entries()[0].prev_hmac, ZERO_HMAC);
        assert_eq!(log.entries()[1].prev_hmac, log.entries()[0].hmac);
        assert_eq!(log.entries()[2].prev_hmac, log.entries()[1].hmac);
        // Sequence numbers
        assert_eq!(log.entries()[0].seq, 0);
        assert_eq!(log.entries()[1].seq, 1);
        assert_eq!(log.entries()[2].seq, 2);
    }

    // ── Test 3: verify_chain on valid log ────────────────────────────────────

    #[test]
    fn verify_chain_valid() {
        let chain_key = test_chain_key();
        let mut log = AuditLog::new();

        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::VaultUnlocked,
            "Unlocked".to_string(),
        );
        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::ItemCreated,
            "Created item".to_string(),
        );

        assert!(log.verify_chain(&chain_key));
    }

    // ── Test 4: verify_chain on empty log ────────────────────────────────────

    #[test]
    fn verify_chain_empty_log() {
        let chain_key = test_chain_key();
        let log = AuditLog::new();

        // An empty log has no entries to verify — should be considered valid
        assert!(log.verify_chain(&chain_key));
    }

    // ── Test 5: tamper detection — modified detail ───────────────────────────

    #[test]
    fn verify_chain_detects_tampered_detail() {
        let chain_key = test_chain_key();
        let mut log = AuditLog::new();

        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::VaultUnlocked,
            "Original detail".to_string(),
        );
        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::ItemCreated,
            "Second entry".to_string(),
        );

        // Tamper with the first entry's detail
        log.entries[0].detail = "Tampered detail".to_string();

        assert!(!log.verify_chain(&chain_key));
    }

    // ── Test 6: tamper detection — modified hmac ─────────────────────────────

    #[test]
    fn verify_chain_detects_tampered_hmac() {
        let chain_key = test_chain_key();
        let mut log = AuditLog::new();

        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::VaultUnlocked,
            "Entry".to_string(),
        );

        // Tamper with the entry's HMAC
        log.entries[0].hmac[0] ^= 0xFF;

        assert!(!log.verify_chain(&chain_key));
    }

    // ── Test 7: tamper detection — deleted entry ─────────────────────────────

    #[test]
    fn verify_chain_detects_deleted_entry() {
        let chain_key = test_chain_key();
        let mut log = AuditLog::new();

        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::VaultUnlocked,
            "First".to_string(),
        );
        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::ItemCreated,
            "Second".to_string(),
        );
        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::ItemDeleted,
            "Third".to_string(),
        );

        // Remove the middle entry
        log.entries.remove(1);

        // Chain should be broken (seq mismatch or prev_hmac mismatch)
        assert!(!log.verify_chain(&chain_key));
    }

    // ── Test 8: tamper detection — wrong chain key ───────────────────────────

    #[test]
    fn verify_chain_fails_with_wrong_key() {
        let chain_key = test_chain_key();
        let mut log = AuditLog::new();

        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::VaultUnlocked,
            "Entry".to_string(),
        );

        // Verify with a different key
        let wrong_key = Zeroizing::new([0xFFu8; 32]);
        assert!(!log.verify_chain(&wrong_key));
    }

    // ── Test 9: encryption roundtrip ─────────────────────────────────────────

    #[test]
    fn encryption_roundtrip() {
        let vault_key = test_key();
        let chain_key = derive_chain_key(&vault_key);
        let mut log = AuditLog::new();

        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::VaultUnlocked,
            "Unlocked".to_string(),
        );
        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::ItemCreated,
            "Created item X".to_string(),
        );

        // Encrypt
        let encrypted = log.to_encrypted_bytes(&vault_key).unwrap();

        // Decrypt
        let recovered = AuditLog::from_encrypted_bytes(&vault_key, &encrypted).unwrap();

        assert_eq!(recovered.len(), 2);
        assert!(recovered.verify_chain(&chain_key));
        assert_eq!(recovered.entries()[0].detail, "Unlocked");
        assert_eq!(recovered.entries()[1].detail, "Created item X");
    }

    // ── Test 10: encryption with wrong key fails ─────────────────────────────

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let vault_key = test_key();
        let chain_key = derive_chain_key(&vault_key);
        let mut log = AuditLog::new();

        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::VaultUnlocked,
            "Entry".to_string(),
        );

        let encrypted = log.to_encrypted_bytes(&vault_key).unwrap();

        // Try to decrypt with a different key
        let wrong_key_params = KdfParams {
            salt: [0xFFu8; 32],
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        };
        let wrong_key = derive_key("wrong-password", &wrong_key_params).unwrap();

        let result = AuditLog::from_encrypted_bytes(&wrong_key, &encrypted);
        assert!(result.is_err());
    }

    // ── Test 11: derive_chain_key is deterministic ───────────────────────────

    #[test]
    fn derive_chain_key_deterministic() {
        let key = test_key();
        let ck1 = derive_chain_key(&key);
        let ck2 = derive_chain_key(&key);
        assert_eq!(*ck1, *ck2);
    }

    // ── Test 12: derive_chain_key differs for different vault keys ────────────

    #[test]
    fn derive_chain_key_differs_for_different_vault_keys() {
        let params_a = KdfParams {
            salt: [0x01u8; 32],
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        };
        let params_b = KdfParams {
            salt: [0x02u8; 32],
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        };
        let key_a = derive_key("password-a", &params_a).unwrap();
        let key_b = derive_key("password-b", &params_b).unwrap();

        let ck_a = derive_chain_key(&key_a);
        let ck_b = derive_chain_key(&key_b);

        assert_ne!(*ck_a, *ck_b);
    }

    // ── Test 13: helper functions produce valid entries ───────────────────────

    #[test]
    fn helper_functions_produce_valid_chain() {
        let vault_key = test_key();
        let chain_key = derive_chain_key(&vault_key);
        let device_id = test_device_id();
        let item_id = Uuid::new_v4();
        let mut log = AuditLog::new();

        vault_unlocked(&mut log, &chain_key, device_id);
        item_created(&mut log, &chain_key, device_id, item_id, "GitHub Login");
        item_updated(&mut log, &chain_key, device_id, item_id, "GitHub Login");
        item_deleted(&mut log, &chain_key, device_id, item_id, "GitHub Login");
        device_admitted(&mut log, &chain_key, device_id, Uuid::new_v4(), "iPhone");
        device_revoked(
            &mut log,
            &chain_key,
            device_id,
            Uuid::new_v4(),
            "Old Laptop",
        );
        import_completed(&mut log, &chain_key, device_id, "Bitwarden JSON", 42);
        export_completed(&mut log, &chain_key, device_id, "zvault-export");

        assert_eq!(log.len(), 8);
        assert!(log.verify_chain(&chain_key));
    }

    // ── Test 14: tamper detection — reordered entries ─────────────────────────

    #[test]
    fn verify_chain_detects_reordered_entries() {
        let chain_key = test_chain_key();
        let mut log = AuditLog::new();

        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::VaultUnlocked,
            "First".to_string(),
        );
        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::ItemCreated,
            "Second".to_string(),
        );
        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::ItemDeleted,
            "Third".to_string(),
        );

        // Swap entries 1 and 2
        log.entries.swap(1, 2);

        assert!(!log.verify_chain(&chain_key));
    }

    // ── Test 15: tamper detection — modified prev_hmac ───────────────────────

    #[test]
    fn verify_chain_detects_tampered_prev_hmac() {
        let chain_key = test_chain_key();
        let mut log = AuditLog::new();

        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::VaultUnlocked,
            "First".to_string(),
        );
        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::ItemCreated,
            "Second".to_string(),
        );

        // Tamper with second entry's prev_hmac
        log.entries[1].prev_hmac[0] ^= 0xFF;

        assert!(!log.verify_chain(&chain_key));
    }

    // ── Test 16: large log verify performance ────────────────────────────────

    #[test]
    fn large_log_verify() {
        let chain_key = test_chain_key();
        let mut log = AuditLog::new();

        for i in 0..100 {
            log.append(
                &chain_key,
                Utc::now(),
                test_device_id(),
                EventKind::ItemCreated,
                format!("Item {i}"),
            );
        }

        assert_eq!(log.len(), 100);
        assert!(log.verify_chain(&chain_key));
    }

    // ── Test 17: tamper detection — modified device_id ────────────────────────

    #[test]
    fn verify_chain_detects_tampered_device_id() {
        let chain_key = test_chain_key();
        let mut log = AuditLog::new();

        log.append(
            &chain_key,
            Utc::now(),
            test_device_id(),
            EventKind::VaultUnlocked,
            "Entry".to_string(),
        );

        // Tamper with the device_id
        log.entries[0].device_id = Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap();

        assert!(!log.verify_chain(&chain_key));
    }
}
