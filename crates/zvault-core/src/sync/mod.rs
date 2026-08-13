//! Vault synchronisation engine for ZVault.
//!
//! ## Overview
//!
//! - [`LamportClock`] — logical clock for ordering sync messages and detecting replays.
//! - [`SyncMessage`] — the sync payload (metadata + NIP-44 encrypted vault JSON).
//! - [`build_full_sync_message`] — serialise + NIP-44 encrypt entire vault for a recipient.
//! - [`apply_sync_message`] — validate, decrypt, and merge an incoming message into the local vault.
//!
//! ## Conflict resolution strategy
//!
//! - **Items:** last-write-wins at item granularity. The remote item replaces
//!   the local one if `remote.version > local.version`. New items are appended.
//! - **Device list:** OR-Set CRDT merge (via [`crate::device::DeviceManager`]).
//! - **Vault version:** the higher `vault_version` wins for full-sync messages.
//!   A full-sync message with a lower vault_version is discarded (stale).

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::device::DeviceManager;
use crate::nostr;
use crate::vault::Vault;
use crate::Result;

// ─── SyncOp ──────────────────────────────────────────────────────────────────

/// Whether a sync message carries the full vault or a compact delta patch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOp {
    /// The payload is the complete encrypted vault JSON.
    Full,
    /// The payload is a CRDT delta (added/changed/deleted items since last sync).
    Delta,
}

// ─── SyncMessage ─────────────────────────────────────────────────────────────

/// An encrypted vault sync message published to Nostr relays.
///
/// The actual content is NIP-44 encrypted; `payload` holds the ciphertext
/// (base64 string bytes). The surrounding Nostr event uses NIP-59 gift-wrap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage {
    /// Random UUID identifying this specific message.
    pub msg_id: Uuid,
    /// `device_id` of the sending device.
    pub sender: Uuid,
    /// The vault this message belongs to.
    pub vault_id: Uuid,
    /// Vault's monotonic write version at the time of sending.
    pub vault_version: u64,
    /// Lamport clock value at send time (used to detect replays and order
    /// concurrent updates).
    pub clock: u64,
    /// Full vault or delta patch.
    pub op: SyncOp,
    /// NIP-44 encrypted payload (base64 string).
    pub payload: String,
}

// ─── LamportClock ────────────────────────────────────────────────────────────

/// A simple Lamport logical clock for ordering distributed sync events.
///
/// Rules:
/// - Increment before sending a message ([`tick`](LamportClock::tick)).
/// - On receiving a message, update to `max(local, received) + 1`
///   ([`update`](LamportClock::update)).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LamportClock(pub u64);

impl LamportClock {
    /// Create a new clock starting at 0.
    #[must_use]
    pub fn new() -> Self {
        Self(0)
    }

    /// Increment the clock and return the new value. Call before sending a
    /// message.
    pub fn tick(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }

    /// Update the clock on receiving a message with timestamp `received`.
    pub fn update(&mut self, received: u64) {
        self.0 = self.0.max(received) + 1;
    }
}

// ─── build_full_sync_message ─────────────────────────────────────────────────

/// Serialise the full vault, NIP-44 encrypt it for `recipient_pubkey`, and
/// wrap it in a [`SyncMessage`] with `op = Full`.
///
/// The returned `SyncMessage` is ready to be JSON-serialised and placed into
/// a NIP-59 gift-wrap event's content field.
///
/// # Arguments
///
/// - `vault` — the current vault state.
/// - `clock` — the sender's Lamport clock (ticked before encoding).
/// - `sender` — the device_id of the sending device.
/// - `sender_secret_key` — 32-byte secp256k1 secret for NIP-44 ECDH.
/// - `recipient_pubkey_hex` — x-only pubkey hex of the target device.
///
/// # Errors
///
/// Returns [`crate::Error::Serialisation`] or [`crate::Error::Crypto`] on failure.
pub fn build_full_sync_message(
    vault: &Vault,
    clock: &mut LamportClock,
    sender: Uuid,
    sender_secret_key: &[u8],
    recipient_pubkey_hex: &str,
) -> Result<SyncMessage> {
    // Serialise vault to JSON (zeroed on drop).
    let plaintext: Zeroizing<Vec<u8>> = vault.to_json()?;

    // Derive NIP-44 conversation key.
    let conversation_key = nostr::get_conversation_key(sender_secret_key, recipient_pubkey_hex)?;

    // Encrypt.
    let encrypted_payload = nostr::nip44_encrypt(&conversation_key, &plaintext)?;

    // Tick clock.
    let clock_val = clock.tick();

    Ok(SyncMessage {
        msg_id: Uuid::new_v4(),
        sender,
        vault_id: vault.id,
        vault_version: vault.version,
        clock: clock_val,
        op: SyncOp::Full,
        payload: encrypted_payload,
    })
}

/// Validate, decrypt, and merge an incoming [`SyncMessage`] into the local vault.
///
/// ## Validation
///
/// 1. Sender must be a live (non-revoked) device in `vault.devices`.
/// 2. `msg.vault_id` must match `vault.id`.
/// 3. For `Full` sync: `msg.vault_version` must be > local `vault.version`
///    (otherwise discard as stale).
///
/// ## Merge strategy (Full sync)
///
/// - Items: remote items replace local items with the same ID if the remote
///   vault version is higher. New items (IDs not in local) are appended.
///   Local items not present in remote are kept (no delete propagation in
///   full sync — deletion is via item absence + higher version).
/// - Devices: OR-Set CRDT merge via `DeviceManager`.
/// - After merge, `vault.version` is set to `max(local, remote)`.
///
/// ## Lamport clock
///
/// Updated to `max(local, received) + 1` regardless of merge outcome.
///
/// # Errors
///
/// Returns [`crate::Error::SyncError`] for validation failures,
/// [`crate::Error::Crypto`] for decryption failures.
pub fn apply_sync_message(
    vault: &mut Vault,
    msg: &SyncMessage,
    clock: &mut LamportClock,
    recipient_secret_key: &[u8],
    sender_pubkey_hex: &str,
) -> Result<()> {
    // Always update clock.
    clock.update(msg.clock);

    // Validate vault ID.
    if msg.vault_id != vault.id {
        return Err(crate::Error::SyncError(format!(
            "vault ID mismatch: expected {}, got {}",
            vault.id, msg.vault_id
        )));
    }

    // Validate sender is a live device.
    let sender_entry = vault.devices.iter().find(|d| d.device_id == msg.sender);
    match sender_entry {
        None => {
            return Err(crate::Error::SyncError(format!(
                "unknown sender device: {}",
                msg.sender
            )));
        }
        Some(entry) if entry.revoked => {
            return Err(crate::Error::SyncError(format!(
                "sender device is revoked: {}",
                msg.sender
            )));
        }
        _ => {}
    }

    // For full sync: check version (stale guard).
    if msg.op == SyncOp::Full && msg.vault_version <= vault.version {
        // Stale message — discard silently. Clock was already updated.
        return Ok(());
    }

    // Decrypt payload.
    let conversation_key = nostr::get_conversation_key(recipient_secret_key, sender_pubkey_hex)?;
    let plaintext = nostr::nip44_decrypt(&conversation_key, &msg.payload)?;

    // Deserialise remote vault.
    let remote_vault = Vault::from_json(&plaintext)?;

    // Merge items: last-write-wins at item granularity.
    // Remote items override local items if same ID exists.
    // Items only in remote are added; items only in local are kept.
    for remote_item in remote_vault.items {
        if let Some(local_item) = vault.items.iter_mut().find(|i| i.id == remote_item.id) {
            // Remote wins if its updated_at is newer (version-based would be
            // better but items don't have individual versions — use timestamp).
            if remote_item.updated_at > local_item.updated_at {
                *local_item = remote_item;
            }
        } else {
            // New item from remote — add it.
            vault.items.push(remote_item);
        }
    }

    // Merge device list via OR-Set CRDT.
    let mut local_dm = DeviceManager::from_vault(vault);
    let remote_dm = DeviceManager::from_vault(&remote_vault_for_devices(&remote_vault_devices(
        &plaintext,
    )?));
    local_dm.merge(&remote_dm);
    local_dm.flush(vault);

    // Update vault version to max.
    if msg.vault_version > vault.version {
        vault.version = msg.vault_version;
    }

    Ok(())
}

/// Re-parse the remote vault's devices for CRDT merge.
/// We need a Vault with just the devices field populated.
fn remote_vault_for_devices(devices: &[crate::vault::DeviceEntry]) -> Vault {
    let mut v = Vault::new();
    v.devices = devices.to_vec();
    v
}

/// Extract devices from raw JSON bytes (avoids double-deserialisation overhead
/// but we already deserialised; this is a minimal helper).
fn remote_vault_devices(plaintext: &[u8]) -> Result<Vec<crate::vault::DeviceEntry>> {
    let v: Vault = Vault::from_json(plaintext)?;
    Ok(v.devices)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{DeviceIdentity, InMemoryStorage};
    use crate::vault::{ItemKind, Vault, VaultItem};

    fn test_device() -> (Vec<u8>, String) {
        // Generate a device and return (secret_key_bytes, pubkey_hex).
        let storage = InMemoryStorage::default();
        let (identity, _material) = DeviceIdentity::generate("Test", &storage).unwrap();
        let sk = identity.load_secret_key(&storage).unwrap();
        (sk.to_vec(), identity.pubkey_hex.clone())
    }

    // ── LamportClock ──────────────────────────────────────────────────────────

    #[test]
    fn lamport_clock_tick() {
        let mut c = LamportClock::new();
        assert_eq!(c.0, 0);
        assert_eq!(c.tick(), 1);
        assert_eq!(c.tick(), 2);
    }

    #[test]
    fn lamport_clock_update() {
        let mut c = LamportClock::new();
        c.update(5);
        assert_eq!(c.0, 6); // max(0, 5) + 1
        c.update(3);
        assert_eq!(c.0, 7); // max(6, 3) + 1
        c.update(100);
        assert_eq!(c.0, 101);
    }

    // ── build_full_sync_message ───────────────────────────────────────────────

    #[test]
    fn build_full_sync_message_roundtrip() {
        let (sk_a, pub_a) = test_device();
        let (sk_b, pub_b) = test_device();

        let mut vault = Vault::new();
        let mut item = VaultItem::new(ItemKind::Login, "GitHub");
        item.username = Some("alice".into());
        vault.add_item(item);

        let sender_id = Uuid::new_v4();
        let mut clock = LamportClock::new();

        let msg = build_full_sync_message(&vault, &mut clock, sender_id, &sk_a, &pub_b).unwrap();

        assert_eq!(msg.op, SyncOp::Full);
        assert_eq!(msg.vault_version, vault.version);
        assert_eq!(msg.sender, sender_id);
        assert_eq!(msg.vault_id, vault.id);
        assert_eq!(msg.clock, 1);
        assert!(!msg.payload.is_empty());

        // Recipient should be able to decrypt using B's secret + A's pubkey.
        let ck = crate::nostr::get_conversation_key(&sk_b, &pub_a).unwrap();
        let plaintext = crate::nostr::nip44_decrypt(&ck, &msg.payload).unwrap();
        let remote_vault = Vault::from_json(&plaintext).unwrap();
        assert_eq!(remote_vault.id, vault.id);
        assert_eq!(remote_vault.items.len(), 1);
    }

    // ── apply_sync_message ────────────────────────────────────────────────────

    #[test]
    fn apply_sync_message_merges_new_item() {
        let (sk_a, pub_a) = test_device();
        let (sk_b, pub_b) = test_device();

        // Create "remote" vault with one item.
        let mut remote_vault = Vault::new();
        let mut item = VaultItem::new(ItemKind::Login, "Remote Item");
        item.username = Some("bob".into());
        remote_vault.add_item(item);

        // Simulate the remote vault being at a higher version.
        remote_vault.version = 5;

        // Create local vault (same ID but empty).
        let mut local_vault = Vault::new();
        local_vault.id = remote_vault.id; // same vault
        local_vault.version = 1;

        // Add the sender device to local vault's device list.
        let sender_id = Uuid::new_v4();
        local_vault.devices.push(crate::vault::DeviceEntry {
            device_id: sender_id,
            nostr_pubkey: pub_a.clone(),
            label: "Remote Device".into(),
            added_at: chrono::Utc::now(),
            added_by: sender_id,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
        });

        // Build sync message from remote vault.
        let mut remote_clock = LamportClock::new();
        let msg =
            build_full_sync_message(&remote_vault, &mut remote_clock, sender_id, &sk_a, &pub_b)
                .unwrap();

        // Apply to local vault.
        let mut local_clock = LamportClock::new();
        apply_sync_message(&mut local_vault, &msg, &mut local_clock, &sk_b, &pub_a).unwrap();

        // Local vault should now have the remote item.
        assert_eq!(local_vault.items.len(), 1);
        assert_eq!(local_vault.items[0].name, "Remote Item");
        // Version should be updated.
        assert!(local_vault.version >= 5);
        // Clock should be updated.
        assert!(local_clock.0 > 0);
    }

    #[test]
    fn apply_sync_message_rejects_stale() {
        let (sk_a, pub_a) = test_device();
        let (sk_b, pub_b) = test_device();

        let mut remote_vault = Vault::new();
        remote_vault.version = 1; // low version

        let mut local_vault = Vault::new();
        local_vault.id = remote_vault.id;
        local_vault.version = 10; // higher version

        let sender_id = Uuid::new_v4();
        local_vault.devices.push(crate::vault::DeviceEntry {
            device_id: sender_id,
            nostr_pubkey: pub_a.clone(),
            label: "Remote".into(),
            added_at: chrono::Utc::now(),
            added_by: sender_id,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
        });

        let mut remote_clock = LamportClock::new();
        let msg =
            build_full_sync_message(&remote_vault, &mut remote_clock, sender_id, &sk_a, &pub_b)
                .unwrap();

        let mut local_clock = LamportClock::new();
        // Should succeed (stale messages are silently discarded).
        apply_sync_message(&mut local_vault, &msg, &mut local_clock, &sk_b, &pub_a).unwrap();

        // Version should NOT have changed (stale message discarded).
        assert_eq!(local_vault.version, 10);
    }

    #[test]
    fn apply_sync_message_rejects_unknown_sender() {
        let (sk_a, pub_a) = test_device();
        let (sk_b, pub_b) = test_device();

        let mut remote_vault = Vault::new();
        remote_vault.version = 5;

        let mut local_vault = Vault::new();
        local_vault.id = remote_vault.id;
        local_vault.version = 1;
        // No devices in local vault — sender is unknown.

        let sender_id = Uuid::new_v4();
        let mut remote_clock = LamportClock::new();
        let msg =
            build_full_sync_message(&remote_vault, &mut remote_clock, sender_id, &sk_a, &pub_b)
                .unwrap();

        let mut local_clock = LamportClock::new();
        let result = apply_sync_message(&mut local_vault, &msg, &mut local_clock, &sk_b, &pub_a);

        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), crate::Error::SyncError(_)),
            "should reject unknown sender"
        );
    }

    #[test]
    fn apply_sync_message_rejects_revoked_sender() {
        let (sk_a, pub_a) = test_device();
        let (sk_b, pub_b) = test_device();

        let mut remote_vault = Vault::new();
        remote_vault.version = 5;

        let mut local_vault = Vault::new();
        local_vault.id = remote_vault.id;
        local_vault.version = 1;

        let sender_id = Uuid::new_v4();
        local_vault.devices.push(crate::vault::DeviceEntry {
            device_id: sender_id,
            nostr_pubkey: pub_a.clone(),
            label: "Revoked".into(),
            added_at: chrono::Utc::now(),
            added_by: sender_id,
            revoked: true, // revoked!
            revoked_at: Some(chrono::Utc::now()),
            revoked_by: None,
        });

        let mut remote_clock = LamportClock::new();
        let msg =
            build_full_sync_message(&remote_vault, &mut remote_clock, sender_id, &sk_a, &pub_b)
                .unwrap();

        let mut local_clock = LamportClock::new();
        let result = apply_sync_message(&mut local_vault, &msg, &mut local_clock, &sk_b, &pub_a);

        assert!(result.is_err());
    }
}
