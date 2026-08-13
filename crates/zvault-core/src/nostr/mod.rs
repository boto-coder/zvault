//! Nostr protocol integration for ZVault.
//!
//! **M4 implementation target.** This module will provide:
//!
//! - [`NostrKeypair`] — a device's Nostr identity (public key only in memory;
//!   secret key lives in [`crate::device::SecureStorage`]).
//! - [`create_keypair`] — generate a fresh secp256k1 keypair for a device.
//! - [`sign_event`] — construct and sign a NIP-01 Nostr event.
//! - [`nip44_encrypt`] — XChaCha20-Poly1305 encryption of a vault payload
//!   over ECDH (NIP-44 v2), producing per-recipient ciphertext.
//! - [`gift_wrap`] — wrap a signed event in a NIP-59 gift-wrap so relay
//!   operators cannot see the true sender, recipient, or event kind.
//!
//! All relay communication (WebSocket pub/sub, reconnect, event filtering) is
//! handled by the `sync` module, which depends on this one.

use serde::{Deserialize, Serialize};

use crate::Result;

// ─── NostrEvent ──────────────────────────────────────────────────────────────

/// A NIP-01 Nostr event (signed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostrEvent {
    /// Event ID: SHA-256 of the canonical serialisation.
    pub id: String,
    /// Author public key (hex, 64 chars).
    pub pubkey: String,
    /// Unix timestamp (seconds).
    pub created_at: i64,
    /// Event kind number.
    pub kind: u32,
    /// Tag array (each tag is a string array).
    pub tags: Vec<Vec<String>>,
    /// Event content (ciphertext for vault sync events).
    pub content: String,
    /// Schnorr signature over the event ID (hex, 128 chars).
    pub sig: String,
}

// ─── NostrKeypair ────────────────────────────────────────────────────────────

/// In-memory representation of a device's Nostr keypair.
///
/// The secret key is **not** stored here — it lives in
/// [`crate::device::SecureStorage`]. This struct carries only the public key
/// needed for outbound message construction and recipient addressing.
#[derive(Debug, Clone)]
pub struct NostrKeypair {
    /// secp256k1 public key (hex-encoded, 64 chars).
    pub pubkey_hex: String,
}

// ─── Stubs ───────────────────────────────────────────────────────────────────

/// Generate a fresh secp256k1 keypair for a Nostr device identity.
///
/// Stores the secret key in secure storage; returns only the public portion.
///
/// # Errors
///
/// Will be implemented in M3 (called from the device module).
pub fn create_keypair() -> Result<NostrKeypair> {
    todo!("M3")
}

/// Construct and sign a NIP-01 Nostr event.
///
/// # Errors
///
/// Will be implemented in M4.
pub fn sign_event(_keypair: &NostrKeypair, _content: &str, _kind: u32) -> Result<NostrEvent> {
    todo!("M4")
}

/// Encrypt `plaintext` from `sender_pubkey` to `recipient_pubkey` using
/// NIP-44 v2 (XChaCha20-Poly1305 over ECDH secp256k1).
///
/// # Errors
///
/// Will be implemented in M4.
pub fn nip44_encrypt(
    _sender_pubkey: &str,
    _recipient_pubkey: &str,
    _plaintext: &[u8],
) -> Result<Vec<u8>> {
    todo!("M4")
}

/// Wrap a signed Nostr event in a NIP-59 gift-wrap addressed to
/// `recipient_pubkey`.
///
/// Gift-wrap hides the true sender, recipient, and event kind from relay
/// operators.
///
/// # Errors
///
/// Will be implemented in M4.
pub fn gift_wrap(_event: NostrEvent, _recipient_pubkey: &str) -> Result<NostrEvent> {
    todo!("M4")
}
