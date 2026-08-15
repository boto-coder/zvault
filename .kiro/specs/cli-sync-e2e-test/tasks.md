# Implementation Plan: CLI Sync E2E Test

## Overview

Verify that two CLI process instances can sync through a real public Nostr relay over WebSocket. The `RelayClient` and CLI `sync send`/`sync receive` commands already exist — this spec adds a `device init` subcommand, fixes `device admit` to accept a real pubkey, and writes an integration test that exercises the full network path (WebSocket TLS → public relay → NIP-59 gift-wrap delivery). Also documents the cross-platform test scenario matrix in DESIGN.md.

## Tasks

- [ ] 1. Verify existing `RelayClient` and sync commands work
  - [ ] 1.1 Confirm `RelayClient` in `crates/zvault-core/src/relay/mod.rs` compiles and connects
    - Verify `publish`, `subscribe`, `close` work against a real relay
    - If any issues found, fix them (connection handling, TLS, timeout)
    - _Requirements: 1.1–1.7_

  - [ ] 1.2 Confirm `SubscriptionFilter` serialises correctly for NIP-01
    - Verify `kinds`, `authors`, `#p` tag, `since`, `limit` fields
    - _Requirements: 1.4_

  - [ ] 1.3 Write a smoke test for `RelayClient` against the real relay
    - Gated behind `#[ignore]` (requires network)
    - Connect to relay from `ZVAULT_TEST_RELAY` env var (default: `wss://relay.damus.io`)
    - Publish a throwaway event, subscribe, verify receipt
    - Timeout: 10s
    - _Requirements: 1.3, 1.4, 1.5_

- [ ] 2. Implement CLI `device init` subcommand
  - [ ] 2.1 Add `Init` variant to `DeviceAction` subcommand
    - Args: `--vault <path>`, `--label <name>`
    - _Requirements: 4.1_

  - [ ] 2.2 Implement `cmd_device_init` function
    - Generate secp256k1 keypair using `DeviceIdentity::generate` with an `InMemoryStorage`
    - Extract secret key bytes
    - Serialize `CliDeviceFile` to JSON
    - Encrypt with vault password + fresh KdfParams → write to `<vault_path>.device`
    - Bootstrap device into vault device list via `DeviceManager`
    - Save vault
    - Print pubkey hex and device_id
    - _Requirements: 4.2, 4.3, 4.5_

  - [ ] 2.3 Implement `load_device_identity(vault_path, password)` helper
    - Read `<vault_path>.device`, decrypt with password, parse `CliDeviceFile`
    - Return `(device_id, secret_key_bytes, pubkey_hex)`
    - Error if file doesn't exist: "Device identity not initialised"
    - _Requirements: 4.4_

  - [ ] 2.4 Guard against re-init
    - If sidecar file exists, exit with error: "Device already initialised"
    - _Requirements: 4.4_

  - [ ] 2.5 Update `device admit` to accept `--pubkey` argument
    - Currently generates a placeholder pubkey — change to accept real pubkey from the other device
    - Add `--pubkey <hex>` arg to `Admit` variant
    - _Requirements: 5.4_

- [ ] 3. Checkpoint: CLI builds with new device init
  - `cargo build --workspace`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`

- [ ] 4. Write CLI-to-CLI integration test (real relay)
  - [ ] 4.1 Create `crates/zvault-cli/tests/cli_sync_e2e.rs`
    - Gated with `#[ignore]` attribute (requires network access to public relay)
    - Reads relay URL from `ZVAULT_TEST_RELAY` env var, defaults to `wss://relay.damus.io`
    - Uses `std::process::Command` to invoke the `zvault` binary (built via `cargo build`)
    - Sets `ZVAULT_PASSWORD` env var for all invocations (non-interactive mode)
    - _Requirements: 5.1, 5.2, 5.9_

  - [ ] 4.2 Implement test scenario: full two-device sync via real relay
    - Step 1: Create two temp vault files (A and B)
    - Step 2: `zvault device init` on both vaults → capture pubkeys from stdout
    - Step 3: Cross-admit: `zvault device admit --pubkey <B_pub>` on A, and vice versa
    - Step 4: `zvault add` an item to vault A (via `--json` or stdin pipe)
    - Step 5: `zvault sync send --vault A --relay <url> --recipient <B_pub>`
    - Step 6: Short delay (1-2s) for relay propagation
    - Step 7: `zvault sync receive --vault B --relay <url>`
    - Step 8: `zvault list --vault B` → assert item from A appears
    - Timeout: 30s total for the test
    - _Requirements: 5.3, 5.4, 5.5, 5.6, 5.8_

  - [ ] 4.3 Implement test scenario: revoked device sync rejected
    - A admits B, B sends sync, A receives (succeeds)
    - A revokes B, B sends another sync → A receives → item NOT applied
    - Assert A's vault state unchanged after revoked sync
    - _Requirements: 5.7_

  - [ ] 4.4 Add CI configuration note
    - Document in test file header: run with `cargo test --test cli_sync_e2e -- --ignored`
    - Document `ZVAULT_TEST_RELAY` env var override
    - Note: test requires network access, skip in offline/sandboxed CI environments
    - _Requirements: 5.9_

- [ ] 5. Write DESIGN.md test scenario matrix
  - [ ] 5.1 Add "§20 Integration & E2E Test Plan" section to DESIGN.md
    - Full scenario matrix table (T1–T10) covering all platform combinations
    - Automation Architecture subsection (tooling per platform)
    - CI Integration subsection (what runs automatically vs what needs network/manual)
    - Note which tests use real relays vs mock/embedded
    - _Requirements: 6.1–6.5_

- [ ] 6. Final Verification
  - `cargo build --workspace` succeeds
  - `cargo test --workspace --all-features` passes (unit tests, no network)
  - `cargo test --test cli_sync_e2e -- --ignored` passes (requires network)
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` zero warnings
  - `cargo fmt --all` clean
  - Integration test completes in <30s with real relay

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "1.2"] },
    { "id": 1, "tasks": ["1.3", "2.1", "2.2", "2.3", "2.4", "2.5"] },
    { "id": 2, "tasks": ["3"] },
    { "id": 3, "tasks": ["4.1", "4.2", "4.3", "4.4"] },
    { "id": 4, "tasks": ["5.1"] },
    { "id": 5, "tasks": ["6"] }
  ]
}
```

## Notes

- `tokio-tungstenite` is already a workspace dependency — the relay module and CLI sync commands already exist and work
- The `RelayClient`, `TestRelay`, `sync send`, and `sync receive` are already implemented — this spec focuses on the `device init` gap and the E2E integration test
- The integration test uses `#[ignore]` so it doesn't run in normal `cargo test` — must be explicitly invoked with `-- --ignored`
- The test uses a **real public Nostr relay** (default: `wss://relay.damus.io`) to prove the full network path works — no mocks, no local relay
- Override relay via `ZVAULT_TEST_RELAY` env var for CI or testing against alternative relays
- The test is inherently non-deterministic (network latency, relay availability) — includes a short propagation delay and generous timeout
- The CLI currently uses interactive prompts for `add` — the integration test will need either a `--json` flag for non-interactive item input, or pipe JSON via stdin
- The `device admit` command currently generates a placeholder pubkey — task 2.5 fixes this to accept a real pubkey argument
- For CI: this test should run in a job that has outbound network access; skip in sandboxed/offline runners
