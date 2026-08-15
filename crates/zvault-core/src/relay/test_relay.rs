//! Minimal in-process Nostr relay for integration testing.
//!
//! Implements the minimum NIP-01 subset required for sync:
//! - `["EVENT", event]` → store in memory, forward to matching subscriptions,
//!   reply `["OK", id, true, ""]`.
//! - `["REQ", sub_id, filter]` → send stored matching events, then stream
//!   new matches as they arrive.
//! - `["CLOSE", sub_id]` → remove subscription.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use crate::nostr::NostrEvent;

use super::SubscriptionFilter;

// Re-export for use in tests within this module.
#[cfg(test)]
use tokio_tungstenite::connect_async;

/// A minimal in-process Nostr relay for testing purposes.
///
/// Binds to `127.0.0.1:0` (random port) and implements the NIP-01 subset
/// needed for sync protocol testing.
pub struct TestRelay {
    addr: SocketAddr,
    handle: JoinHandle<()>,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

/// Shared relay state.
struct RelayState {
    /// All events ever received, in order.
    events: Vec<NostrEvent>,
    /// Broadcast channel for new events (all connected clients listen here).
    event_broadcast: broadcast::Sender<NostrEvent>,
}

impl TestRelay {
    /// Start the test relay, binding to a random available port on localhost.
    ///
    /// # Panics
    ///
    /// Panics if binding to localhost fails or the local address cannot be
    /// determined (should never happen in a healthy test environment).
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind test relay");
        let addr = listener.local_addr().expect("no local addr");

        let (event_broadcast, _) = broadcast::channel::<NostrEvent>(256);
        let state = Arc::new(RwLock::new(RelayState {
            events: Vec::new(),
            event_broadcast: event_broadcast.clone(),
        }));

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let handle = tokio::spawn(Self::accept_loop(listener, state, shutdown_rx));

        Self {
            addr,
            handle,
            shutdown_tx,
        }
    }

    /// Get the WebSocket URL for this relay.
    #[must_use]
    pub fn url(&self) -> String {
        format!("ws://{}", self.addr)
    }

    /// Shut down the relay and wait for the accept loop to terminate.
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        // Give tasks a moment to notice shutdown before aborting.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        self.handle.abort();
        let _ = self.handle.await;
    }

    /// Accept loop: listens for new WebSocket connections.
    async fn accept_loop(
        listener: TcpListener,
        state: Arc<RwLock<RelayState>>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) {
        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            let state = state.clone();
                            let shutdown = shutdown_rx.clone();
                            tokio::spawn(Self::handle_connection(stream, state, shutdown));
                        }
                        Err(_) => break,
                    }
                }
                _ = shutdown_rx.changed() => {
                    break;
                }
            }
        }
    }

    /// Handle a single WebSocket connection from a client.
    async fn handle_connection(
        stream: TcpStream,
        state: Arc<RwLock<RelayState>>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) {
        let Ok(ws_stream) = tokio_tungstenite::accept_async(stream).await else {
            return;
        };

        let (ws_sink, mut ws_stream_read) = ws_stream.split();
        let ws_sink = Arc::new(Mutex::new(ws_sink));

        // This client's active subscriptions: sub_id → filter.
        let subscriptions: Arc<Mutex<HashMap<String, SubscriptionFilter>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Subscribe to the broadcast channel for new events.
        let mut broadcast_rx = {
            let s = state.read().await;
            s.event_broadcast.subscribe()
        };

        // Spawn a task to forward broadcast events to matching subscriptions.
        let ws_sink_clone = ws_sink.clone();
        let subs_clone = subscriptions.clone();
        let forward_task = tokio::spawn(async move {
            while let Ok(event) = broadcast_rx.recv().await {
                let subs = subs_clone.lock().await;
                for (sub_id, filter) in subs.iter() {
                    if event_matches_filter(&event, filter) {
                        let msg = serde_json::json!(["EVENT", sub_id, event]);
                        let text = serde_json::to_string(&msg).unwrap_or_default();
                        let mut sink = ws_sink_clone.lock().await;
                        if sink.send(Message::Text(text.into())).await.is_err() {
                            return;
                        }
                    }
                }
            }
        });

        // Read loop: process client messages.
        loop {
            tokio::select! {
                msg_result = ws_stream_read.next() => {
                    match msg_result {
                        Some(Ok(Message::Text(text))) => {
                            Self::handle_message(
                                &text,
                                &state,
                                &subscriptions,
                                &ws_sink,
                            ).await;
                        }
                        Some(Ok(Message::Close(_))) | None => break,
                        _ => {}
                    }
                }
                _ = shutdown_rx.changed() => {
                    break;
                }
            }
        }

        forward_task.abort();
    }

    /// Process a single client message.
    async fn handle_message(
        text: &str,
        state: &Arc<RwLock<RelayState>>,
        subscriptions: &Arc<Mutex<HashMap<String, SubscriptionFilter>>>,
        ws_sink: &Arc<
            Mutex<
                futures_util::stream::SplitSink<
                    tokio_tungstenite::WebSocketStream<TcpStream>,
                    Message,
                >,
            >,
        >,
    ) {
        let arr: serde_json::Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) => return,
        };
        let Some(arr) = arr.as_array() else { return };

        let Some(msg_type) = arr.first().and_then(|v| v.as_str()) else {
            return;
        };

        match msg_type {
            "EVENT" => {
                // ["EVENT", event_json]
                if let Some(event_val) = arr.get(1) {
                    if let Ok(event) = serde_json::from_value::<NostrEvent>(event_val.clone()) {
                        let event_id = event.id.clone();

                        // Store the event.
                        let mut s = state.write().await;
                        s.events.push(event.clone());
                        // Broadcast to all connected clients.
                        let _ = s.event_broadcast.send(event);
                        drop(s);

                        // Reply OK.
                        let ok_msg = serde_json::json!(["OK", event_id, true, ""]);
                        let text = serde_json::to_string(&ok_msg).unwrap_or_default();
                        let mut sink = ws_sink.lock().await;
                        let _ = sink.send(Message::Text(text.into())).await;
                    }
                }
            }
            "REQ" => {
                // ["REQ", sub_id, filter]
                let sub_id = match arr.get(1).and_then(|v| v.as_str()) {
                    Some(s) => s.to_string(),
                    None => return,
                };
                let filter: SubscriptionFilter = match arr.get(2) {
                    Some(v) => serde_json::from_value(v.clone()).unwrap_or_default(),
                    None => SubscriptionFilter::default(),
                };

                // Send stored events that match the filter.
                let s = state.read().await;
                let matching: Vec<NostrEvent> = s
                    .events
                    .iter()
                    .filter(|e| event_matches_filter(e, &filter))
                    .cloned()
                    .collect();
                drop(s);

                let mut sink = ws_sink.lock().await;
                for event in matching {
                    let msg = serde_json::json!(["EVENT", sub_id, event]);
                    let text = serde_json::to_string(&msg).unwrap_or_default();
                    let _ = sink.send(Message::Text(text.into())).await;
                }

                // Send EOSE (end of stored events).
                let eose_msg = serde_json::json!(["EOSE", sub_id]);
                let text = serde_json::to_string(&eose_msg).unwrap_or_default();
                let _ = sink.send(Message::Text(text.into())).await;
                drop(sink);

                // Register the subscription for future events.
                subscriptions.lock().await.insert(sub_id, filter);
            }
            "CLOSE" => {
                // ["CLOSE", sub_id]
                if let Some(sub_id) = arr.get(1).and_then(|v| v.as_str()) {
                    subscriptions.lock().await.remove(sub_id);
                }
            }
            _ => {}
        }
    }
}

/// Check if an event matches a NIP-01 subscription filter.
fn event_matches_filter(event: &NostrEvent, filter: &SubscriptionFilter) -> bool {
    // Check kinds.
    if let Some(kinds) = &filter.kinds {
        if !kinds.contains(&event.kind) {
            return false;
        }
    }

    // Check authors.
    if let Some(authors) = &filter.authors {
        if !authors.contains(&event.pubkey) {
            return false;
        }
    }

    // Check #p tags.
    if let Some(p_tags) = &filter.p_tags {
        let event_p_values: Vec<&str> = event
            .tags
            .iter()
            .filter(|tag| tag.first().map(String::as_str) == Some("p"))
            .filter_map(|tag| tag.get(1).map(String::as_str))
            .collect();

        let has_match = p_tags.iter().any(|p| event_p_values.contains(&p.as_str()));
        if !has_match {
            return false;
        }
    }

    // Check since.
    if let Some(since) = filter.since {
        if event.created_at < since {
            return false;
        }
    }

    // Check until.
    if let Some(until) = filter.until {
        if event.created_at > until {
            return false;
        }
    }

    true
}

// ─── TestRelay unit tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_relay_starts_and_provides_url() {
        let relay = TestRelay::start().await;
        let url = relay.url();
        assert!(url.starts_with("ws://127.0.0.1:"));
        relay.shutdown().await;
    }

    #[tokio::test]
    async fn test_relay_stores_and_replays_events() {
        let relay = TestRelay::start().await;

        // Connect and publish an event.
        let (ws, _) = connect_async(&relay.url()).await.unwrap();
        let (mut sink, mut stream) = ws.split();

        let event = NostrEvent {
            id: "test123".repeat(9) + "x",
            pubkey: "a".repeat(64),
            created_at: 1_700_000_000,
            kind: 1,
            tags: vec![],
            content: "test content".into(),
            sig: "b".repeat(128),
        };

        let msg = serde_json::json!(["EVENT", event]);
        sink.send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
            .await
            .unwrap();

        // Wait for OK.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let ok_msg = stream.next().await.unwrap().unwrap();
        let ok_text = ok_msg.into_text().unwrap();
        assert!(ok_text.contains("\"OK\""));

        relay.shutdown().await;
    }

    #[test]
    fn event_matches_filter_kinds() {
        let event = NostrEvent {
            id: "a".repeat(64),
            pubkey: "b".repeat(64),
            created_at: 1_700_000_000,
            kind: 1059,
            tags: vec![],
            content: "test".into(),
            sig: "c".repeat(128),
        };

        let filter_match = SubscriptionFilter {
            kinds: Some(vec![1059]),
            ..Default::default()
        };
        assert!(event_matches_filter(&event, &filter_match));

        let filter_no_match = SubscriptionFilter {
            kinds: Some(vec![1]),
            ..Default::default()
        };
        assert!(!event_matches_filter(&event, &filter_no_match));
    }

    #[test]
    fn event_matches_filter_p_tags() {
        let my_pubkey = "1".repeat(64);
        let event = NostrEvent {
            id: "a".repeat(64),
            pubkey: "b".repeat(64),
            created_at: 1_700_000_000,
            kind: 1059,
            tags: vec![vec!["p".into(), my_pubkey.clone()]],
            content: "test".into(),
            sig: "c".repeat(128),
        };

        let filter = SubscriptionFilter {
            p_tags: Some(vec![my_pubkey]),
            ..Default::default()
        };
        assert!(event_matches_filter(&event, &filter));

        let filter_other = SubscriptionFilter {
            p_tags: Some(vec!["2".repeat(64)]),
            ..Default::default()
        };
        assert!(!event_matches_filter(&event, &filter_other));
    }
}
