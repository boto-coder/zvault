//! Tamper-evident audit log for ZVault.
//!
//! **M8 implementation target.** This module will provide:
//!
//! - [`AuditEntry`] — a single log record with a monotonic sequence number and
//!   an HMAC-SHA256 chain link tying it to the previous entry.
//! - [`AuditLog`] — append-only collection of entries, encrypted with the
//!   same AES-256-GCM key as the vault file, stored at `<vault>.audit`.
//! - [`AuditLog::verify_chain`] — walk every entry, recompute each HMAC, and
//!   return `false` if any link is broken (indicating tampering or corruption).
//!
//! The chain key is derived from the vault master key via HKDF:
//! `HKDF(vault_master_key, "audit_chain_key")`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

    /// Walk every entry and recompute each HMAC chain link.
    ///
    /// Returns `true` if the chain is intact, `false` if any entry has been
    /// tampered with, deleted, or reordered.
    ///
    /// # Errors
    ///
    /// Will be implemented in M8; currently a stub.
    #[must_use]
    pub fn verify_chain(&self) -> bool {
        todo!("M8")
    }
}
