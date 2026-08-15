# Design Document: CLI Sync E2E Test

## Architecture Overview

This feature adds three layers to the system:

1. **Relay transport** (`zvault-core::relay`) — async WebSocket client for Nostr relay communication
2. **CLI sync commands** — `sync send`, `sync receive`, `device init` subcommands
3. **Integration test harness** — embedded relay + multi-process test orchestration

```
┌──────────────────────────────────────────────────────────────┐
│                   Integration Test Harness                     │
│                                                                │
│  ┌─────────────┐    ┌─────────────────┐    ┌─────────────┐  │
│  │  CLI Inst A  │    │ Embedded Relay   │    │  CLI Inst B  │  │
│  │  (Process)   │◄──►│ (In-process WS)  │◄──►│  (Process)   │  │
│  └──────┬──────┘    └─────────────────┘    └──────┬──────┘  │
│         │                                          │          │
│         ▼                                          ▼          │
│  ┌─────────────┐                           ┌─────────────┐  │
│  │  vault_a.zv  │                           │  vault_b.zv  │  │
│  │  .device     │                           │  .device     │  │
│  └─────────────┘                           └─────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

## Components

### 1. RelayClient (`crates/zvault-core/src/relay/mod.rs`)

Async WebSocket client using `tokio-tungstenite`:

```rust
pub struct RelayClient {
    ws_sink: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    event_rx: mpsc::Receiver<RelayMessage>,
    subscriptions: HashMap<String, mpsc::Sender<NostrEvent>>,
    _read_task: JoinHandle<()>,
}

impl RelayClient {
    pub async fn connect(url: &str) -> Result<Self>;
    pub async fn publish(&mut self, event: NostrEvent) -> Result<()>;
    pub async fn subscribe(&mut self, filter: SubscriptionFilter) -> Result<mpsc::Receiver<NostrEvent>>;
    pub async fn close(&mut self) -> Result<()>;
}
```

The client runs a background read loop (spawned task) that routes incoming `["EVENT", sub_id, event]` messages to the correct subscription channel.

### 2. SubscriptionFilter

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    #[serde(rename = "#p", skip_serializing_if = "Option::is_none")]
    pub p_tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}
```

### 3. CLI Device Identity Storage

For CLI (no OS keychain), device secrets are stored in a sidecar file:

```
<vault_path>.device  →  encrypted blob containing:
{
  "device_id": "uuid",
  "secret_key_hex": "64-char hex",
  "pubkey_hex": "64-char hex",
  "label": "My CLI"
}
```

The sidecar is encrypted with the same vault password (fresh KdfParams) using `encrypt_with_params`. This means unlocking the vault also unlocks the device identity.

### 4. Embedded Test Relay

A minimal in-process relay for integration tests:

```rust
pub struct TestRelay {
    addr: SocketAddr,
    handle: JoinHandle<()>,
}

impl TestRelay {
    pub async fn start() -> Self;           // binds to 127.0.0.1:0 (random port)
    pub fn url(&self) -> String;            // ws://127.0.0.1:<port>
    pub async fn shutdown(self);
}
```

The relay implements the minimum NIP-01 subset:
- Accept `["EVENT", event]` → store in memory, forward to matching subscriptions, reply `["OK", id, true]`
- Accept `["REQ", sub_id, filter]` → send stored matching events, then stream new matches
- Accept `["CLOSE", sub_id]` → remove subscription

### 5. CLI Sync Commands Flow

```
zvault sync send --vault a.zvault --relay ws://localhost:4736 --recipient <B_pubkey>
  1. Open vault (password from ZVAULT_PASSWORD)
  2. Load device identity from a.zvault.device
  3. build_full_sync_message(vault, clock, device_id, sk, recipient_pubkey)
  4. sign_event (NIP-01) → gift_wrap (NIP-59)
  5. RelayClient::connect(relay_url)
  6. RelayClient::publish(gift_wrapped_event)
  7. Print "✓ Sync sent (vault version N)"

zvault sync receive --vault b.zvault --relay ws://localhost:4736 --timeout 10
  1. Open vault (password from ZVAULT_PASSWORD)
  2. Load device identity from b.zvault.device
  3. RelayClient::connect(relay_url)
  4. RelayClient::subscribe(filter: kind=1059, #p=my_pubkey)
  5. For each event within timeout:
     a. unwrap_gift_wrap(event, my_sk) → inner event
     b. Deserialise SyncMessage from inner content
     c. apply_sync_message(vault, msg, clock, my_sk, sender_pubkey)
  6. Save vault if any messages applied
  7. Print "✓ Received N sync message(s), M items updated"
```

## Data Models

### CliDeviceFile (sidecar `.device` file content, JSON before encryption)

```json
{
  "device_id": "550e8400-e29b-41d4-a716-446655440000",
  "secret_key_hex": "deadbeef...",
  "pubkey_hex": "cafe0123...",
  "label": "Alice's Laptop CLI"
}
```

### SubscriptionFilter (NIP-01 filter object)

```json
{
  "kinds": [1059],
  "#p": ["<recipient_pubkey_hex>"],
  "since": 1692000000
}
```

### RelayMessage (internal enum for routing)

```rust
enum RelayMessage {
    Event { subscription_id: String, event: NostrEvent },
    Ok { event_id: String, success: bool, message: String },
    Eose { subscription_id: String },
    Notice { message: String },
}
```

## Integration Test Scenario

```
Test: full_cli_to_cli_sync

Setup:
  - Start embedded relay on random port
  - Create temp dir with vault_a.zvault and vault_b.zvault

Steps:
  1. zvault init vault_a.zvault                     (ZVAULT_PASSWORD=testpw)
  2. zvault device init --vault vault_a.zvault --label "Device A"
     → capture A's pubkey from stdout
  3. zvault init vault_b.zvault                     (ZVAULT_PASSWORD=testpw)
  4. zvault device init --vault vault_b.zvault --label "Device B"
     → capture B's pubkey from stdout
  5. Cross-admit: A admits B, B admits A
     zvault device admit --vault vault_a.zvault --label "Device B" --pubkey <B_pub>
     zvault device admit --vault vault_b.zvault --label "Device A" --pubkey <A_pub>
  6. A adds an item:
     echo input | zvault add --vault vault_a.zvault
     (or use a non-interactive add flag)
  7. A sends sync:
     zvault sync send --vault vault_a.zvault --relay ws://127.0.0.1:<port> --recipient <B_pub>
  8. B receives sync:
     zvault sync receive --vault vault_b.zvault --relay ws://127.0.0.1:<port> --timeout 5
  9. Assert B has the item:
     zvault list --vault vault_b.zvault | grep "GitHub"

Teardown:
  - Shutdown relay
  - Remove temp dir
```

## Error Handling

| Error | Source | Handling |
|---|---|---|
| Relay connection refused | `RelayClient::connect` | Return `Error::Relay("connection refused: {url}")` |
| Relay timeout (no OK response) | `RelayClient::publish` | Return `Error::Relay("timeout waiting for OK")` |
| No device identity | CLI sync commands | Exit with "Run `zvault device init` first" |
| Sidecar file not found | CLI device loading | Return `Error::DeviceNotInitialised` |
| Wrong password for sidecar | Sidecar decrypt | Return `Error::InvalidVaultFile` (same as vault) |
| Subscription timeout | `sync receive` | Exit 0 with "No sync messages received" |
| Gift-wrap unwrap failure | `sync receive` | Log warning, skip event, continue |

## Test Scenario Matrix (for DESIGN.md §20)

| # | Scenario | Platforms | What's Tested | Automation Approach | CI Feasible? |
|---|---|---|---|---|---|
| T1 | CLI↔CLI full sync | 2× CLI process | Relay transport, NIP-44/59, item merge | `assert_cmd` + embedded relay | ✅ Yes |
| T2 | CLI↔CLI revoked device rejected | 2× CLI process | Revocation enforcement over relay | `assert_cmd` + embedded relay | ✅ Yes |
| T3 | CLI↔CLI concurrent edits (conflict) | 2× CLI process | LWW merge, version ordering | `assert_cmd` + embedded relay | ✅ Yes |
| T4 | Desktop↔CLI sync | Tauri + CLI | Cross-platform interop, same relay | Tauri test + `assert_cmd` + relay | ✅ Headless |
| T5 | Desktop↔Extension sync | Tauri + WXT | WASM↔native crypto compat | Tauri test + Playwright + relay | ⚠️ Needs display |
| T6 | Extension self-test | WXT (Firefox) | WASM crypto, storage, background | Playwright + web-ext | ✅ Yes |
| T7 | Extension↔Desktop↔Android | All three | Full mesh convergence | Playwright + Tauri + Espresso + relay | ❌ Manual / Nightly |
| T8 | Three CLI devices mesh | 3× CLI process | Multi-device convergence | `assert_cmd` + embedded relay | ✅ Yes |
| T9 | Stale message replay | 2× CLI process | Replay protection via version | `assert_cmd` + embedded relay | ✅ Yes |
| T10 | Large vault sync (1000 items) | 2× CLI process | Performance, correctness at scale | `assert_cmd` + embedded relay | ✅ Yes |

### Automation Architecture

| Platform | Tooling | Notes |
|---|---|---|
| CLI | `assert_cmd` + `predicates` crates | Invoke binary, assert stdout/exit code |
| Desktop (Tauri) | `tauri-driver` + WebDriver | Headless Tauri testing, invoke commands programmatically |
| Extension (WXT) | Playwright + `web-ext` | Load extension in browser, automate popup interactions |
| Android | Espresso + UI Automator | Instrumented tests, requires emulator |
| Relay | Embedded `TestRelay` (in-process) or Docker `nostr-rs-relay` | In-process preferred for speed |

### CI Integration

| Test Category | CI Runner | Requirements | Run Frequency |
|---|---|---|---|
| T1–T3, T8–T10 (CLI-only) | Ubuntu runner | Rust toolchain, no special deps | Every PR |
| T4 (Desktop↔CLI) | Ubuntu runner | Rust + Node.js, headless | Every PR |
| T5 (Desktop↔Extension) | Ubuntu runner + Xvfb | Rust + Node.js + Firefox | Nightly |
| T6 (Extension self-test) | Ubuntu runner | Node.js + Firefox | Every PR |
| T7 (Full mesh) | Self-hosted with emulator | All toolchains + Android emulator | Weekly / Manual |
