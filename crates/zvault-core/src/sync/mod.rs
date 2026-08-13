//! Vault synchronisation engine for ZVault.
//!
//! **M4 implementation target.** This module will provide:
//!
//! - [`LamportClock`] — a simple logical clock for ordering sync messages and
//!   detecting replays. Each device maintains its own clock; on receiving a
//!   message the local clock is updated to `max(local, received) + 1`.
//! - [`SyncMessage`] — the encrypted payload published to Nostr relays. The
//!   `op` field distinguishes full-vault syncs from CRDT delta patches.
//! - [`build_full_sync_message`] — serialise + encrypt the entire vault for a
//!   full sync broadcast.
//! - [`apply_sync_message`] — validate, decrypt, and merge an incoming sync
//!   message using last-write-wins (items) and OR-Set (devices).
//!
//! The relay WebSocket client (connect, subscribe, reconnect loop) will live
//! in a `relay` submodule, also implemented in M4.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
/// Published via NIP-59 gift-wrap so relays cannot see the sender, recipient,
/// or vault ID in plaintext. The `payload` bytes are NIP-44 ciphertext.
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
    /// NIP-44 encrypted payload bytes.
    pub payload: Vec<u8>,
}

// ─── LamportClock ────────────────────────────────────────────────────────────

/// A simple Lamport logical clock for ordering distributed sync events.
///
/// Rules:
/// - Increment before sending a message ([`tick`](LamportClock::tick)).
/// - On receiving a message, update to `max(local, received) + 1`
///   ([`update`](LamportClock::update)).
#[derive(Debug, Clone, Default)]
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

// ─── Stubs ───────────────────────────────────────────────────────────────────

/// Serialise the full vault, encrypt it for all authorised recipients, and
/// wrap it in a [`SyncMessage`] with `op = Full`.
///
/// # Errors
///
/// Will be implemented in M4.
pub fn build_full_sync_message(
    _vault: &crate::vault::Vault,
    _clock: &mut LamportClock,
    _sender: Uuid,
) -> Result<SyncMessage> {
    todo!("M4")
}

/// Validate, decrypt, and merge an incoming [`SyncMessage`] into the local
/// vault.
///
/// Validation checks: sender is in the authorised device list, Nostr event
/// signature is valid, `vault_version` is not stale (replay guard).
/// Merge strategy: last-write-wins at item granularity (by `updated_at`);
/// OR-Set CRDT for device list updates.
///
/// # Errors
///
/// Will be implemented in M4.
pub fn apply_sync_message(
    _vault: &mut crate::vault::Vault,
    _msg: SyncMessage,
    _clock: &mut LamportClock,
) -> Result<()> {
    todo!("M4")
}
