# Requirements Document

## Introduction

This feature adds real end-to-end sync testing between two separate CLI process instances communicating through an actual Nostr relay over WebSocket. It requires implementing a relay transport client in `zvault-core`, adding `sync` and `device init` subcommands to the CLI, writing an integration test that verifies the full sync round-trip, and documenting a comprehensive cross-platform test scenario matrix in DESIGN.md with an automation plan.

## Glossary

- **RelayClient**: A WebSocket client in `zvault-core` that connects to a Nostr relay and can publish/subscribe to NIP-01 events.
- **Embedded Relay**: A minimal in-process Nostr relay for test purposes that accepts WebSocket connections, stores events, and forwards them to matching subscribers.
- **CLI Instance**: A single invocation of the `zvault` binary operating on a vault file.
- **Device Identity**: A secp256k1 keypair associated with a device, used for NIP-44 encryption and NIP-01 event signing.
- **Gift-Wrap**: NIP-59 envelope that hides the real sender, recipient, and event kind from relay operators.
- **Test Scenario Matrix**: A comprehensive table of cross-platform integration test cases covering all supported platform combinations.

## Requirements

### Requirement 1: Relay Transport Client

**User Story:** As a developer, I want a relay transport client in `zvault-core` so that CLI and other native clients can publish and subscribe to Nostr events over WebSocket.

#### Acceptance Criteria

1. THE `zvault-core` crate SHALL expose a `RelayClient` struct (gated behind the `native` feature) that connects to a relay via WebSocket (WS or WSS).
2. WHEN `RelayClient::connect(url)` is called, THE client SHALL establish a WebSocket connection to the given URL and return an error if the connection fails.
3. THE `RelayClient` SHALL implement `publish(event: NostrEvent) -> Result<()>` that sends a `["EVENT", <event>]` JSON message to the relay per NIP-01.
4. THE `RelayClient` SHALL implement `subscribe(filter) -> Result<Receiver<NostrEvent>>` that sends a `["REQ", <sub_id>, <filter>]` message and returns a channel yielding matching events received from the relay.
5. THE `RelayClient` SHALL implement `close()` that sends `["CLOSE", <sub_id>]` for active subscriptions and disconnects the WebSocket.
6. THE `RelayClient` SHALL handle relay `["OK", ...]` and `["NOTICE", ...]` messages without crashing.
7. IF the WebSocket connection drops unexpectedly, THEN `subscribe` receivers SHALL receive an error or the channel shall close.

### Requirement 2: CLI `sync send` Subcommand

**User Story:** As a CLI user with multiple devices, I want to send my vault state to another device via a Nostr relay so that my other device receives my latest credentials.

#### Acceptance Criteria

1. THE CLI SHALL expose a `sync send` subcommand with arguments: `--vault <path>`, `--relay <url>`, `--recipient <pubkey_hex>`.
2. WHEN `sync send` is invoked, THE CLI SHALL open the vault, load the local device identity, build a full sync message (NIP-44 encrypted for the recipient), gift-wrap it (NIP-59), and publish the gift-wrapped event to the specified relay.
3. WHEN the relay acknowledges the event, THE CLI SHALL print a success message and exit with code 0.
4. IF the local device identity is not initialised, THEN THE CLI SHALL exit with an error message instructing the user to run `device init`.
5. IF the relay connection fails, THEN THE CLI SHALL exit with code 1 and an error message.
6. THE CLI SHALL accept the vault password via `ZVAULT_PASSWORD` environment variable for non-interactive use.

### Requirement 3: CLI `sync receive` Subcommand

**User Story:** As a CLI user, I want to receive and apply sync messages from another device via a Nostr relay so that my vault is updated with their changes.

#### Acceptance Criteria

1. THE CLI SHALL expose a `sync receive` subcommand with arguments: `--vault <path>`, `--relay <url>`, `--timeout <seconds>` (default: 10).
2. WHEN `sync receive` is invoked, THE CLI SHALL open the vault, load the local device identity, subscribe to the relay for gift-wrapped events addressed to this device's pubkey, and wait for events until the timeout expires.
3. FOR each received event, THE CLI SHALL unwrap the NIP-59 gift-wrap, decrypt the NIP-44 payload, validate the sender against the device list, and apply the sync message to the local vault.
4. WHEN at least one sync message is successfully applied, THE CLI SHALL save the updated vault and print a summary (items received/updated count).
5. IF no messages are received within the timeout, THE CLI SHALL print "No sync messages received" and exit with code 0.
6. IF a received message fails validation (unknown sender, revoked device, stale), THE CLI SHALL log a warning and continue listening for other messages.

### Requirement 4: CLI `device init` Subcommand

**User Story:** As a CLI user, I want to initialise a device identity for my CLI instance so that it can participate in vault sync.

#### Acceptance Criteria

1. THE CLI SHALL expose a `device init` subcommand with arguments: `--vault <path>`, `--label <name>`.
2. WHEN `device init` is invoked, THE CLI SHALL generate a secp256k1 keypair, store the secret key in a sidecar file (`<vault_path>.device`), bootstrap the device into the vault's device list, and save the vault.
3. THE sidecar `.device` file SHALL be encrypted with the vault password (AES-256-GCM with the same KDF params) so that the device secret is at-rest encrypted.
4. WHEN `device init` is invoked on a vault that already has a local device identity (sidecar file exists), THE CLI SHALL exit with an error: "Device identity already initialised."
5. AFTER successful init, THE CLI SHALL print the device's public key (hex) and device_id (UUID) so the user can share it with other devices for admission.

### Requirement 5: CLI-to-CLI Sync Integration Test

**User Story:** As a developer, I want an automated integration test that proves two CLI processes can sync a vault item through a real Nostr relay, so that I have confidence the full stack works end-to-end.

#### Acceptance Criteria

1. THE test SHALL start a Nostr relay (embedded in-process or Docker container) accessible via WebSocket.
2. THE test SHALL create two separate vault files (A and B) with the same vault ID.
3. THE test SHALL initialise device identities for both instances.
4. THE test SHALL admit device B into vault A's device list, and admit device A into vault B's device list.
5. THE test SHALL use CLI instance A to add an item and `sync send` to the relay targeting B.
6. THE test SHALL use CLI instance B to `sync receive` from the relay.
7. THE test SHALL assert that vault B now contains the item that A added, with correct field values.
8. THE test SHALL complete in under 30 seconds.
9. THE test SHALL be runnable via `cargo test` without requiring external infrastructure (relay is self-contained).

### Requirement 6: Test Scenario Matrix in DESIGN.md

**User Story:** As a developer or QA engineer, I want a comprehensive list of cross-platform sync test scenarios documented in DESIGN.md so that I know what needs to be tested and how to automate it.

#### Acceptance Criteria

1. DESIGN.md SHALL contain a new section "§20 Integration & E2E Test Plan" with a scenario matrix table.
2. THE matrix SHALL cover these platform combinations: CLI↔CLI, Desktop↔CLI, Desktop↔Extension, Extension alone (self-test), Extension↔Desktop↔Android.
3. EACH scenario SHALL specify: name, platforms involved, preconditions, test steps, expected outcomes, pass/fail criteria, automation approach, and CI feasibility.
4. THE section SHALL include a subsection "Automation Architecture" describing the recommended tooling per platform.
5. THE section SHALL include a subsection "CI Integration" describing which tests run in GitHub Actions automatically and which require manual execution or special infrastructure.
