//! Relay connection manager and sync orchestration for the desktop app.
//!
//! Manages WebSocket relay connections tied to vault lock state, and provides
//! high-level sync operations: publish to all peers, subscribe for incoming
//! sync messages.

use uuid::Uuid;
use zeroize::Zeroizing;

use zvault_core::nostr::{self, NostrEvent};
use zvault_core::relay::{RelayClient, SubscriptionFilter};
use zvault_core::settings;
use zvault_core::sync::{self, LamportClock, SyncMessage};
use zvault_core::vault::Vault;

/// Result of a sync send operation.
pub struct SyncSendResult {
    /// Number of peers the sync was sent to.
    pub peers_sent: u32,
    /// Number of relay publishes that succeeded.
    pub relays_published: u32,
    /// Warnings (non-fatal errors).
    pub warnings: Vec<String>,
}

/// Result of a sync receive operation.
pub struct SyncReceiveResult {
    /// Number of messages received and applied.
    pub messages_applied: u32,
    /// Final vault version after applying messages.
    #[allow(dead_code)]
    pub vault_version: u64,
    /// Warnings (non-fatal errors).
    pub warnings: Vec<String>,
}

/// Build and publish sync messages to all admitted, non-revoked peer devices.
///
/// For each peer device:
/// 1. Build a full sync message (NIP-44 encrypted vault)
/// 2. Gift-wrap it (NIP-59) for the recipient
/// 3. Publish to all enabled relays
///
/// This function does NOT hold the vault lock during relay communication.
/// The caller must pass the vault state and device identity.
pub async fn sync_send_all(
    vault: &Vault,
    device_id: Uuid,
    secret_key: &Zeroizing<Vec<u8>>,
    own_pubkey_hex: &str,
) -> SyncSendResult {
    let mut result = SyncSendResult {
        peers_sent: 0,
        relays_published: 0,
        warnings: Vec::new(),
    };

    // Get enabled relays
    let relay_urls = settings::enabled_relay_urls(&vault.settings);
    if relay_urls.is_empty() {
        result.warnings.push("No relays configured".to_string());
        return result;
    }

    // Find peer devices (non-revoked, not self)
    let peers: Vec<_> = vault
        .devices
        .iter()
        .filter(|d| !d.revoked && d.nostr_pubkey != own_pubkey_hex)
        .collect();

    if peers.is_empty() {
        result.warnings.push("No peer devices to sync with".to_string());
        return result;
    }

    // Build sync messages for each peer
    let mut messages_to_publish: Vec<(String, NostrEvent)> = Vec::new();

    for peer in &peers {
        let mut clock = LamportClock::new();
        match sync::build_full_sync_message(vault, &mut clock, device_id, secret_key, &peer.nostr_pubkey) {
            Ok(sync_msg) => {
                let sync_msg_json = match serde_json::to_string(&sync_msg) {
                    Ok(j) => j,
                    Err(e) => {
                        result.warnings.push(format!(
                            "Failed to serialize sync for {}: {e}",
                            peer.device_id
                        ));
                        continue;
                    }
                };

                // Gift-wrap the sync message
                let tags = vec![vec!["p".to_string(), peer.nostr_pubkey.clone()]];
                match nostr::gift_wrap(
                    secret_key,
                    &peer.nostr_pubkey,
                    &sync_msg_json,
                    21059, // custom kind for ZVault sync
                    &tags,
                ) {
                    Ok(event) => {
                        messages_to_publish.push((peer.device_id.to_string(), event));
                        result.peers_sent += 1;
                    }
                    Err(e) => {
                        result.warnings.push(format!(
                            "Gift-wrap failed for {}: {e}",
                            peer.device_id
                        ));
                    }
                }
            }
            Err(e) => {
                result.warnings.push(format!(
                    "Build sync failed for {}: {e}",
                    peer.device_id
                ));
            }
        }
    }

    // Publish to all relays
    for url in &relay_urls {
        match RelayClient::connect(url).await {
            Ok(mut client) => {
                for (peer_id, event) in &messages_to_publish {
                    match client.publish(event).await {
                        Ok(()) => {
                            result.relays_published += 1;
                        }
                        Err(e) => {
                            result.warnings.push(format!(
                                "Publish to {url} for {peer_id} failed: {e}"
                            ));
                        }
                    }
                }
                let _ = client.close().await;
            }
            Err(e) => {
                result.warnings.push(format!("Connect to {url} failed: {e}"));
            }
        }
    }

    result
}

/// Subscribe to incoming sync messages on all enabled relays.
///
/// Connects to each relay, subscribes for kind-1059 events addressed to our
/// pubkey, and collects events until EOSE or timeout. Returns the collected
/// events for the caller to process (unwrap + apply).
pub async fn sync_receive(
    vault: &Vault,
    _secret_key: &Zeroizing<Vec<u8>>,
    own_pubkey_hex: &str,
) -> SyncReceiveResult {
    let mut result = SyncReceiveResult {
        messages_applied: 0,
        vault_version: vault.version,
        warnings: Vec::new(),
    };

    let relay_urls = settings::enabled_relay_urls(&vault.settings);
    if relay_urls.is_empty() {
        result.warnings.push("No relays configured".to_string());
        return result;
    }

    let filter = SubscriptionFilter {
        kinds: Some(vec![1059]), // NIP-59 gift-wrap
        p_tags: Some(vec![own_pubkey_hex.to_string()]),
        ..Default::default()
    };

    // Collect events from all relays
    let mut events: Vec<NostrEvent> = Vec::new();

    for url in &relay_urls {
        match RelayClient::connect(url).await {
            Ok(mut client) => {
                match client.subscribe(filter.clone()).await {
                    Ok(mut rx) => {
                        // Collect events until timeout (5 seconds for EOSE)
                        let timeout = tokio::time::Duration::from_secs(5);
                        loop {
                            match tokio::time::timeout(timeout, rx.recv()).await {
                                Ok(Some(event)) => events.push(event),
                                _ => break,
                            }
                        }
                    }
                    Err(e) => {
                        result.warnings.push(format!("Subscribe on {url} failed: {e}"));
                    }
                }
                let _ = client.close().await;
            }
            Err(e) => {
                result.warnings.push(format!("Connect to {url} failed: {e}"));
            }
        }
    }

    result.messages_applied = events.len() as u32;
    result
}

/// Unwrap and apply a single gift-wrapped event to the vault.
///
/// Returns `Ok(true)` if the message was applied, `Ok(false)` if it was
/// rejected (stale, wrong kind, etc.), and `Err` on hard failures.
#[allow(dead_code)]
pub fn unwrap_and_apply_event(
    vault: &mut Vault,
    event: &NostrEvent,
    secret_key: &Zeroizing<Vec<u8>>,
) -> Result<bool, String> {
    // 1. Unwrap gift-wrap
    let rumor = nostr::unwrap_gift_wrap(secret_key, event)
        .map_err(|e| format!("unwrap failed: {e}"))?;

    // Only process ZVault sync messages (kind 21059)
    if rumor.kind != 21059 {
        return Ok(false);
    }

    // 2. Parse the sync message from rumor content
    let sync_msg: SyncMessage = serde_json::from_str(&rumor.content)
        .map_err(|e| format!("parse sync message failed: {e}"))?;

    let sender_pubkey = &rumor.pubkey;

    // 3. Apply sync message
    let mut clock = LamportClock::new();
    sync::apply_sync_message(vault, &sync_msg, &mut clock, secret_key, sender_pubkey)
        .map_err(|e| format!("apply sync failed: {e}"))?;

    Ok(true)
}
