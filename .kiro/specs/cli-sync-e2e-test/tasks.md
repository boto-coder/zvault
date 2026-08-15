# Implementation Plan: CLI Sync E2E Test

## Overview

Add a relay transport client to `zvault-core`, CLI sync subcommands (`sync send`, `sync receive`, `device init`), an embedded test relay, and an integration test proving two CLI processes can sync through a real WebSocket relay. Also document the full cross-platform test scenario matrix in DESIGN.md.

## Tasks

- [ ] 1. Implement `RelayClient` in `zvault-core`
  - [ ] 1.1 Create `crates/zvault-core/src/relay/mod.rs` with `RelayClient` struct
    - WebSocket connect via `tokio-tungstenite`
    - `publish(event)` sends `["EVENT", event]`, waits for `["OK", id, true/false]`
    - `subscribe(filter)` sends `["REQ", sub_id, filter]`, returns `mpsc::Receiver<NostrEvent>`
    - Background read loop spawned as tokio task, routes events to subscription channels
    - `close()` sends `["CLOSE"]` for all subscriptions, drops connection
    - Gated: `#[cfg(feature = "native")]`
    - _Requirements: 1.1–1.7_

  - [ ] 1.2 Add `SubscriptionFilter` struct with NIP-01 filter fields
    - Serialize to JSON matching NIP-01 filter format
    - Support `kinds`, `authors`, `#p` tag, `since`, `limit`
    - _Requirements: 1.4_

  - [ ] 1.3 Write unit tests for `RelayClient` against a mock WebSocket server
    - Mock accepts connections, echoes OK for EVENT, sends stored events for REQ
    - Test: publish succeeds, subscribe receives events, close is clean
    - _Requirements: 1.3, 1.4, 1.5_

  - [ ] 1.4 Register `relay` module in `crates/zvault-core/src/lib.rs`
    - Add `#[cfg(feature = "native")] pub mod relay;`
    - Add `Relay(String)` variant to `Error` enum
    - _Requirements: 1.1_

- [ ] 2. Implement embedded test relay
  - [ ] 2.1 Create `crates/zvault-core/src/relay/test_relay.rs` (gated behind `#[cfg(test)]` or `test-helpers` feature)
    - `TestRelay::start()` binds to `127.0.0.1:0`, accepts WS connections
    - Stores events in memory `Vec<NostrEvent>`
    - On `["REQ", sub_id, filter]`: send matching stored events, then stream new ones
    - On `["EVENT", event]`: store, forward to matching subscriptions, reply `["OK", id, true]`
    - On `["CLOSE", sub_id]`: remove subscription
    - `TestRelay::url()` returns `ws://127.0.0.1:<port>`
    - `TestRelay::shutdown()` stops the listener
    - _Requirements: 5.1, 5.9_

  - [ ] 2.2 Write tests for TestRelay itself
    - Verify event storage, subscription delivery, filter matching
    - _Requirements: 5.1_

- [ ] 3. Implement CLI `device init` subcommand
  - [ ] 3.1 Add `Init` variant to `DeviceAction` subcommand
    - Args: `--vault <path>`, `--label <name>`
    - _Requirements: 4.1_

  - [ ] 3.2 Implement `cmd_device_init` function
    - Generate secp256k1 keypair using `DeviceIdentity::generate` with an `InMemoryStorage`
    - Extract secret key bytes
    - Serialize `CliDeviceFile` to JSON
    - Encrypt with vault password + fresh KdfParams → write to `<vault_path>.device`
    - Bootstrap device into vault device list via `DeviceManager`
    - Save vault
    - Print pubkey hex and device_id
    - _Requirements: 4.2, 4.3, 4.5_

  - [ ] 3.3 Implement `load_device_identity(vault_path, password)` helper
    - Read `<vault_path>.device`, decrypt with password, parse `CliDeviceFile`
    - Return `(device_id, secret_key_bytes, pubkey_hex)`
    - Error if file doesn't exist: "Device identity not initialised"
    - _Requirements: 4.4_

  - [ ] 3.4 Guard against re-init
    - If sidecar file exists, exit with error
    - _Requirements: 4.4_

  - [ ] 3.5 Update `device admit` to accept `--pubkey` argument
    - Currently generates a placeholder pubkey — change to accept real pubkey from the other device
    - Add `--pubkey <hex>` arg to `Admit` variant
    - _Requirements: 5.4_

- [ ] 4. Implement CLI `sync send` subcommand
  - [ ] 4.1 Add `Sync` command with `Send` and `Receive` subcommands
    - `sync send --vault <path> --relay <url> --recipient <pubkey_hex>`
    - _Requirements: 2.1_

  - [ ] 4.2 Implement `cmd_sync_send`
    - Open vault + load device identity
    - `build_full_sync_message(vault, clock, device_id, sk, recipient_pubkey)`
    - `sign_event` + `gift_wrap` the sync message
    - `tokio::runtime::Runtime::new()` → block_on `RelayClient::connect` + `publish`
    - Print success
    - _Requirements: 2.2, 2.3, 2.4, 2.5, 2.6_

- [ ] 5. Implement CLI `sync receive` subcommand
  - [ ] 5.1 Implement `cmd_sync_receive`
    - Open vault + load device identity
    - `tokio::runtime::Runtime::new()` → block_on:
      - `RelayClient::connect`
      - `subscribe(filter: kinds=[1059], #p=[my_pubkey], since=now-timeout)`
      - Loop: receive events from channel with tokio timeout
      - For each event: `unwrap_gift_wrap` → deserialise `SyncMessage` → `apply_sync_message`
    - Save vault if any messages applied
    - Print summary or "no messages"
    - _Requirements: 3.1–3.6_

- [ ] 6. Write CLI-to-CLI integration test
  - [ ] 6.1 Create `tests/cli_sync_e2e.rs` in workspace root (or `crates/zvault-cli/tests/`)
    - Start `TestRelay` (requires `test-helpers` feature or inline relay)
    - Use `std::process::Command` to invoke `zvault` binary
    - Set `ZVAULT_PASSWORD` env var for all invocations
    - Follow the test scenario: init → device init → cross-admit → add item → sync send → sync receive → verify
    - Assert on stdout output and vault contents
    - Timeout: 30s total
    - _Requirements: 5.1–5.9_

  - [ ] 6.2 Add a second test case: revoked device sync rejected
    - A admits B, B sends sync, A revokes B, B sends another sync → A ignores it
    - _Requirements: 5.7 (security boundary)_

- [ ] 7. Write DESIGN.md test scenario matrix
  - [ ] 7.1 Add "§20 Integration & E2E Test Plan" section to DESIGN.md
    - Full scenario matrix table (T1–T10)
    - Automation Architecture subsection (tooling per platform)
    - CI Integration subsection (what runs automatically vs manually)
    - _Requirements: 6.1–6.5_

- [ ] 8. Verification
  - `cargo build --workspace` succeeds
  - `cargo test --workspace --all-features` passes (including new integration test)
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` zero warnings
  - `cargo fmt --all` clean
  - Integration test completes in <30s

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "1.2", "1.4"] },
    { "id": 1, "tasks": ["1.3", "2.1"] },
    { "id": 2, "tasks": ["2.2", "3.1", "3.2", "3.3", "3.4", "3.5"] },
    { "id": 3, "tasks": ["4.1", "4.2"] },
    { "id": 4, "tasks": ["5.1"] },
    { "id": 5, "tasks": ["6.1", "6.2"] },
    { "id": 6, "tasks": ["7.1"] },
    { "id": 7, "tasks": ["8"] }
  ]
}
```

## Notes

- `tokio-tungstenite` is already a workspace dependency (declared but unused) — no new dependency needed
- The `native` feature in `zvault-core` already gates `tokio` and `tokio-tungstenite` — the relay module fits naturally here
- The embedded test relay is intentionally minimal — it only needs to support the NIP-01 subset required for sync (EVENT, REQ, CLOSE, OK)
- The CLI currently uses interactive prompts for `add` — the integration test will need either a `--non-interactive` flag with JSON input, or pipe input via stdin
- The `device admit` command currently generates a placeholder pubkey — task 3.5 fixes this to accept a real pubkey argument
