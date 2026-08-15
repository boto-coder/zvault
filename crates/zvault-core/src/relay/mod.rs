//! Nostr relay transport client and embedded test relay.
//!
//! This module provides:
//! - [`RelayClient`] — async WebSocket client for publishing and subscribing
//!   to NIP-01 events on a Nostr relay.
//! - [`TestRelay`] — minimal in-process relay for integration tests.
//! - [`SubscriptionFilter`] — NIP-01 filter object for subscriptions.
//!
//! All types are gated behind `#[cfg(feature = "native")]` (requires tokio +
//! tokio-tungstenite).

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::nostr::NostrEvent;
use crate::{Error, Result};

// Re-export test relay for test-helpers consumers.
#[cfg(any(test, feature = "test-helpers"))]
pub mod test_relay;

#[cfg(any(test, feature = "test-helpers"))]
pub use test_relay::TestRelay;

// ─── SubscriptionFilter ──────────────────────────────────────────────────────

/// NIP-01 filter object for relay subscriptions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubscriptionFilter {
    /// Filter by event kinds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<u32>>,

    /// Filter by author pubkeys (hex).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,

    /// Filter by `p` tag values (recipient pubkeys).
    #[serde(rename = "#p", skip_serializing_if = "Option::is_none")]
    pub p_tags: Option<Vec<String>>,

    /// Events created after this Unix timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,

    /// Events created before this Unix timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<i64>,

    /// Maximum number of events to return on initial query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

// ─── RelayMessage (internal) ─────────────────────────────────────────────────

/// Parsed relay-to-client messages (internal routing type).
#[derive(Debug)]
#[allow(dead_code)]
enum RelayResponse {
    /// `["EVENT", sub_id, event]`
    Event {
        subscription_id: String,
        event: NostrEvent,
    },
    /// `["OK", event_id, success, message]`
    Ok {
        event_id: String,
        success: bool,
        message: String,
    },
    /// `["EOSE", sub_id]`
    Eose { subscription_id: String },
    /// `["NOTICE", message]`
    Notice { message: String },
}

// ─── RelayClient ─────────────────────────────────────────────────────────────

/// Async WebSocket client for a Nostr relay.
///
/// Supports publishing events and subscribing to filtered event streams.
/// The client spawns a background read task that routes incoming messages to
/// the appropriate subscription channel.
pub struct RelayClient {
    /// Write half of the WebSocket connection.
    ws_sink: futures_util::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    /// Active subscriptions: sub_id → sender channel for events.
    subscriptions: Arc<tokio::sync::Mutex<HashMap<String, mpsc::UnboundedSender<NostrEvent>>>>,
    /// Channel for OK responses from publish operations.
    ok_rx: mpsc::UnboundedReceiver<(String, bool, String)>,
    /// Background read task handle.
    _read_task: JoinHandle<()>,
    /// Subscription counter for generating unique IDs.
    sub_counter: u64,
}

impl RelayClient {
    /// Connect to a Nostr relay at the given WebSocket URL.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SyncError`] if the connection fails.
    pub async fn connect(url: &str) -> Result<Self> {
        // Ensure the rustls crypto provider is installed (ring).
        let _ = rustls::crypto::ring::default_provider().install_default();

        let (ws_stream, _response) = connect_async(url)
            .await
            .map_err(|e| Error::SyncError(format!("relay connection failed: {e}")))?;

        let (ws_sink, ws_stream) = ws_stream.split();

        let subscriptions: Arc<
            tokio::sync::Mutex<HashMap<String, mpsc::UnboundedSender<NostrEvent>>>,
        > = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        let (ok_tx, ok_rx) = mpsc::unbounded_channel();

        let subs_clone = subscriptions.clone();
        let read_task = tokio::spawn(async move {
            Self::read_loop(ws_stream, subs_clone, ok_tx).await;
        });

        Ok(Self {
            ws_sink,
            subscriptions,
            ok_rx,
            _read_task: read_task,
            sub_counter: 0,
        })
    }

    /// Publish a signed event to the relay.
    ///
    /// Sends `["EVENT", event]` and waits for the relay's `["OK", ...]` response.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SyncError`] if publishing fails or the relay rejects
    /// the event.
    pub async fn publish(&mut self, event: &NostrEvent) -> Result<()> {
        let msg = serde_json::to_string(&serde_json::json!(["EVENT", event]))
            .map_err(|e| Error::Serialisation(format!("event serialisation: {e}")))?;

        self.ws_sink
            .send(Message::Text(msg.into()))
            .await
            .map_err(|e| Error::SyncError(format!("websocket send failed: {e}")))?;

        // Wait for OK response (with timeout).
        let event_id = event.id.clone();
        match tokio::time::timeout(std::time::Duration::from_secs(10), self.ok_rx.recv()).await {
            Ok(Some((id, success, message))) => {
                if id == event_id && success {
                    Ok(())
                } else if id == event_id {
                    Err(Error::SyncError(format!("relay rejected event: {message}")))
                } else {
                    // Got an OK for a different event — still treat as success
                    // since we don't multiplex publishes.
                    Ok(())
                }
            }
            Ok(None) => Err(Error::SyncError("relay connection closed".into())),
            Err(_) => Err(Error::SyncError(
                "timeout waiting for relay OK response".into(),
            )),
        }
    }

    /// Subscribe to events matching the given filter.
    ///
    /// Sends `["REQ", sub_id, filter]` and returns a receiver channel that
    /// yields matching events as they arrive from the relay.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SyncError`] if the subscription request fails.
    pub async fn subscribe(
        &mut self,
        filter: SubscriptionFilter,
    ) -> Result<mpsc::UnboundedReceiver<NostrEvent>> {
        self.sub_counter += 1;
        let sub_id = format!("sub_{}", self.sub_counter);

        let msg = serde_json::to_string(&serde_json::json!(["REQ", sub_id, filter]))
            .map_err(|e| Error::Serialisation(format!("filter serialisation: {e}")))?;

        let (tx, rx) = mpsc::unbounded_channel();
        self.subscriptions.lock().await.insert(sub_id.clone(), tx);

        self.ws_sink
            .send(Message::Text(msg.into()))
            .await
            .map_err(|e| Error::SyncError(format!("websocket send failed: {e}")))?;

        Ok(rx)
    }

    /// Close all subscriptions and disconnect.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SyncError`] if closing fails.
    pub async fn close(&mut self) -> Result<()> {
        let subs = self.subscriptions.lock().await;
        let sub_ids: Vec<String> = subs.keys().cloned().collect();
        drop(subs);

        for sub_id in sub_ids {
            let msg = serde_json::to_string(&serde_json::json!(["CLOSE", sub_id]))
                .map_err(|e| Error::Serialisation(format!("close serialisation: {e}")))?;
            // Best-effort send; ignore errors on close.
            let _ = self.ws_sink.send(Message::Text(msg.into())).await;
        }

        self.subscriptions.lock().await.clear();

        let _ = self.ws_sink.send(Message::Close(None)).await;
        Ok(())
    }

    /// Background read loop that routes relay messages to subscriptions.
    async fn read_loop(
        mut ws_stream: futures_util::stream::SplitStream<
            WebSocketStream<MaybeTlsStream<TcpStream>>,
        >,
        subscriptions: Arc<tokio::sync::Mutex<HashMap<String, mpsc::UnboundedSender<NostrEvent>>>>,
        ok_tx: mpsc::UnboundedSender<(String, bool, String)>,
    ) {
        while let Some(msg_result) = ws_stream.next().await {
            let Ok(msg) = msg_result else { break };

            let text = match msg {
                Message::Text(t) => t,
                Message::Close(_) => break,
                _ => continue,
            };

            if let Some(parsed) = Self::parse_relay_message(&text) {
                match parsed {
                    RelayResponse::Event {
                        subscription_id,
                        event,
                    } => {
                        let subs = subscriptions.lock().await;
                        if let Some(tx) = subs.get(&subscription_id) {
                            let _ = tx.send(event);
                        }
                    }
                    RelayResponse::Ok {
                        event_id,
                        success,
                        message,
                    } => {
                        let _ = ok_tx.send((event_id, success, message));
                    }
                    RelayResponse::Eose { .. } | RelayResponse::Notice { .. } => {
                        // No action needed for EOSE or NOTICE in current implementation.
                    }
                }
            }
        }

        // Connection closed — drop all subscription senders so receivers get None.
        subscriptions.lock().await.clear();
    }

    /// Parse a relay JSON message into a typed enum.
    fn parse_relay_message(text: &str) -> Option<RelayResponse> {
        let arr: serde_json::Value = serde_json::from_str(text).ok()?;
        let arr = arr.as_array()?;

        let msg_type = arr.first()?.as_str()?;

        match msg_type {
            "EVENT" => {
                let sub_id = arr.get(1)?.as_str()?.to_string();
                let event: NostrEvent = serde_json::from_value(arr.get(2)?.clone()).ok()?;
                Some(RelayResponse::Event {
                    subscription_id: sub_id,
                    event,
                })
            }
            "OK" => {
                let event_id = arr.get(1)?.as_str()?.to_string();
                let success = arr.get(2)?.as_bool().unwrap_or(false);
                let message = arr.get(3)?.as_str().unwrap_or("").to_string();
                Some(RelayResponse::Ok {
                    event_id,
                    success,
                    message,
                })
            }
            "EOSE" => {
                let sub_id = arr.get(1)?.as_str()?.to_string();
                Some(RelayResponse::Eose {
                    subscription_id: sub_id,
                })
            }
            "NOTICE" => {
                let message = arr.get(1)?.as_str()?.to_string();
                Some(RelayResponse::Notice { message })
            }
            _ => None,
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn relay_client_connect_and_publish() {
        // Start embedded test relay.
        let relay = TestRelay::start().await;
        let url = relay.url();

        // Connect client.
        let mut client = RelayClient::connect(&url).await.unwrap();

        // Create a minimal test event.
        let event = NostrEvent {
            id: "0".repeat(64),
            pubkey: "a".repeat(64),
            created_at: 1_700_000_000,
            kind: 1,
            tags: vec![],
            content: "hello".into(),
            sig: "b".repeat(128),
        };

        // Publish.
        client.publish(&event).await.unwrap();

        // Cleanup.
        client.close().await.unwrap();
        relay.shutdown().await;
    }

    #[tokio::test]
    async fn relay_client_subscribe_receives_events() {
        let relay = TestRelay::start().await;
        let url = relay.url();

        let mut client = RelayClient::connect(&url).await.unwrap();

        // Subscribe with filter for kind=1059.
        let filter = SubscriptionFilter {
            kinds: Some(vec![1059]),
            ..Default::default()
        };
        let mut rx = client.subscribe(filter).await.unwrap();

        // Publish a matching event.
        let event = NostrEvent {
            id: "c".repeat(64),
            pubkey: "d".repeat(64),
            created_at: 1_700_000_000,
            kind: 1059,
            tags: vec![vec!["p".into(), "e".repeat(64)]],
            content: "encrypted".into(),
            sig: "f".repeat(128),
        };
        client.publish(&event).await.unwrap();

        // Give the relay a moment to forward.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Receive the event.
        let received = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");

        assert_eq!(received.id, "c".repeat(64));
        assert_eq!(received.kind, 1059);

        client.close().await.unwrap();
        relay.shutdown().await;
    }

    #[tokio::test]
    async fn relay_client_subscribe_filters_by_p_tag() {
        let relay = TestRelay::start().await;
        let url = relay.url();

        let mut client = RelayClient::connect(&url).await.unwrap();

        let my_pubkey = "1".repeat(64);
        let other_pubkey = "2".repeat(64);

        // Subscribe for events tagged with my pubkey.
        let filter = SubscriptionFilter {
            kinds: Some(vec![1059]),
            p_tags: Some(vec![my_pubkey.clone()]),
            ..Default::default()
        };
        let mut rx = client.subscribe(filter).await.unwrap();

        // Publish event NOT addressed to us.
        let event_other = NostrEvent {
            id: "a".repeat(64),
            pubkey: "b".repeat(64),
            created_at: 1_700_000_000,
            kind: 1059,
            tags: vec![vec!["p".into(), other_pubkey.clone()]],
            content: "not for us".into(),
            sig: "c".repeat(128),
        };
        client.publish(&event_other).await.unwrap();

        // Publish event addressed to us.
        let event_mine = NostrEvent {
            id: "d".repeat(64),
            pubkey: "e".repeat(64),
            created_at: 1_700_000_001,
            kind: 1059,
            tags: vec![vec!["p".into(), my_pubkey.clone()]],
            content: "for us".into(),
            sig: "f".repeat(128),
        };
        client.publish(&event_mine).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Should only receive the event addressed to us.
        let received = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");

        assert_eq!(received.id, "d".repeat(64));
        assert_eq!(received.content, "for us");

        client.close().await.unwrap();
        relay.shutdown().await;
    }

    /// Smoke test against a real public Nostr relay.
    ///
    /// Requires network access. Gated with `#[ignore]` — run explicitly:
    /// ```sh
    /// cargo test -p zvault-core --all-features relay_client_real_relay_smoke -- --ignored
    /// ```
    ///
    /// Override relay URL via `ZVAULT_TEST_RELAY` env var (default: `wss://relay.damus.io`).
    #[tokio::test]
    #[ignore = "requires network access to a real Nostr relay"]
    async fn relay_client_real_relay_smoke() {
        use crate::nostr;
        use aes_gcm::aead::OsRng as AeadOsRng;
        use zeroize::Zeroizing;

        let relay_url = std::env::var("ZVAULT_TEST_RELAY")
            .unwrap_or_else(|_| "wss://relay.damus.io".to_string());

        // Generate a throwaway keypair for signing.
        let signing_key = k256::ecdsa::SigningKey::random(&mut AeadOsRng);
        let sk_bytes: Zeroizing<Vec<u8>> = Zeroizing::new(signing_key.to_bytes().to_vec());

        // Connect to relay.
        let mut client = RelayClient::connect(&relay_url)
            .await
            .expect("failed to connect to real relay");

        // Create and sign a throwaway kind=1 event.
        let content = format!("zvault-test-{}", uuid::Uuid::new_v4());
        #[allow(clippy::cast_possible_wrap)]
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let event =
            nostr::sign_event(&sk_bytes, &content, 1, vec![], now).expect("sign event failed");

        let event_id = event.id.clone();
        let event_pubkey = event.pubkey.clone();

        // Subscribe for events from our throwaway pubkey.
        let filter = SubscriptionFilter {
            kinds: Some(vec![1]),
            authors: Some(vec![event_pubkey.clone()]),
            since: Some(now - 10),
            ..Default::default()
        };
        let mut rx = client.subscribe(filter).await.expect("subscribe failed");

        // Publish to relay.
        client.publish(&event).await.expect("publish failed");

        // Wait for the event to come back through our subscription.
        let received = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .expect("timeout waiting for event from real relay")
            .expect("subscription channel closed");

        assert_eq!(received.id, event_id);
        assert_eq!(received.content, content);

        client.close().await.ok();
    }
}
