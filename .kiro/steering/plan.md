# ZVault — Development Plan Steering

## Current status

All milestones M0-M12 complete. Preparing for release.

## Backlog

<!-- New plan items are appended here by the planning subagent -->
<!-- Format: see process.md "Plan Item Format" -->

### P1 — Make Nostr sync actually work across all interfaces

**Status:** ✅ Done
**Branch:** `feat/nostr-sync-all-interfaces`
**Requested:** 2026-08-16

#### Description

The core sync engine (M4) is complete and the CLI has full working sync (send/receive via relays with NIP-44 + NIP-59 gift-wrap). However, the desktop app, browser extension, and Android app all have placeholder or missing sync implementations:

- **Desktop (Tauri):** Has relay settings CRUD but no actual sync send/receive — no WebSocket relay connectivity, no `build_full_sync_message`/`apply_sync_message` calls, no gift-wrap.
- **Browser extension:** `triggerNostrSync()` is an explicit no-op placeholder. The WASM crate (`zvault-wasm`) exports zero sync/NIP-44/NIP-59 functions. No WebSocket relay client exists in the extension.
- **Android:** `VaultRepository.syncNow()` is a TODO stub. UniFFI bindings don't expose sync functions. No WorkManager-based background sync worker exists.

This plan item delivers working Nostr sync on all four interfaces (CLI already works; desktop, extension, and Android need implementation).

#### Scope

**WASM crate (`crates/zvault-wasm/src/lib.rs`):**
- Export `build_full_sync_message(vault_json, device_id, secret_key_hex, recipient_pubkey_hex) → SyncMessage JSON`
- Export `apply_sync_message(vault_json, sync_msg_json, secret_key_hex, sender_pubkey_hex) → updated vault JSON`
- Export `nip44_encrypt(sender_sk_hex, recipient_pk_hex, plaintext) → ciphertext base64`
- Export `nip44_decrypt(receiver_sk_hex, sender_pk_hex, ciphertext_b64) → plaintext`
- Export `gift_wrap(sender_sk_hex, recipient_pk_hex, content, kind, tags) → NostrEvent JSON`
- Export `unwrap_gift_wrap(receiver_sk_hex, event_json) → rumor JSON`
- Export `sign_event(sk_hex, event_json) → signed NostrEvent JSON`
- Export `verify_event(event_json) → bool`

**Browser extension (`apps/extension/`):**
- Implement WebSocket relay client in background service worker (native `WebSocket` API — no tokio needed)
- Implement `triggerNostrSync()`: build sync message → NIP-44 encrypt for each peer → gift-wrap → publish to all enabled relays
- Implement subscribe-on-unlock flow: connect to enabled relays → subscribe for kind-1059 events with `#p` = own pubkey → unwrap → decrypt → merge into local vault → re-encrypt and persist
- Handle relay reconnection (exponential backoff) and EOSE (end of stored events)
- Store device secret key in `browser.storage.session` (memory-only, cleared on browser close)

**Desktop app (`apps/desktop/src-tauri/`):**
- Add Tauri commands: `sync_send_all` (publish to all peers via all enabled relays), `sync_receive` (subscribe and apply incoming messages)
- Implement publish-on-change: after every vault mutation (`add_item`, `update_item`, `delete_item`), trigger sync send to all admitted devices
- Implement subscribe-on-unlock: when vault is unlocked, connect to enabled relays, subscribe for gift-wrapped events, apply incoming sync messages automatically
- Add background relay connection management (connect/disconnect/reconnect lifecycle tied to vault lock state)

**Android app (`apps/android/`):**
- Add sync functions to UniFFI UDL/bindings: `build_full_sync_message`, `apply_sync_message`, `gift_wrap`, `unwrap_gift_wrap`
- Implement `NostrSyncWorker` (extends `CoroutineWorker`) for background sync via WorkManager
- Wire `VaultRepository.syncNow()` to actual relay publish/subscribe via the UniFFI-exposed relay client
- Schedule periodic sync with WorkManager (configurable interval, default 15 min)
- Trigger immediate sync on vault mutation

**Dependencies:**
- P1 depends on no other plan items (core engine and CLI are already complete)
- Browser extension WebSocket client uses the browser's native `WebSocket` API, not the tokio-based `RelayClient`
- Desktop and Android use `zvault_core::relay::RelayClient` (tokio-based, behind `native` feature)

#### Definition of Done

- [ ] WASM crate exports all sync/NIP-44/NIP-59/event functions listed above
- [ ] `wasm-pack build` succeeds for `zvault-wasm` with sync exports
- [ ] Browser extension `triggerNostrSync()` publishes gift-wrapped sync messages to configured relays via WebSocket
- [ ] Browser extension subscribes on unlock and applies incoming sync messages
- [ ] Desktop app publishes sync messages on vault mutation
- [ ] Desktop app subscribes on unlock and applies incoming sync messages automatically
- [ ] Android UniFFI bindings expose sync functions
- [ ] Android `NostrSyncWorker` runs via WorkManager and performs sync
- [ ] Integration test: two simulated devices sync via TestRelay (already exists in `two_device_sync.rs` — verify it still passes)
- [ ] End-to-end test: CLI sends sync → extension receives and merges (manual verification)
- [ ] All tests pass (`cargo test --workspace --all-features`)
- [ ] Zero clippy warnings
- [ ] Security review completed (no CRITICAL/MEDIUM open)
- [ ] Committed and pushed to branch

#### Expected Outputs

- `crates/zvault-wasm/src/lib.rs` — new sync/NIP-44/NIP-59 WASM exports
- `apps/extension/src/entrypoints/background.ts` — real `triggerNostrSync()` implementation + relay WebSocket client
- `apps/extension/src/lib/relay-client.ts` — new file: WebSocket relay client for extension
- `apps/desktop/src-tauri/src/main.rs` — new Tauri commands for sync send/receive + background subscription
- `apps/desktop/src-tauri/src/sync.rs` — new file: relay connection manager and sync orchestration
- `bindings/uniffi/src/lib.rs` — new sync function exports
- `bindings/uniffi/src/zvault.udl` — UDL additions for sync types and functions
- `apps/android/app/src/main/java/com/zvault/sync/NostrSyncWorker.kt` — new file: WorkManager worker
- `apps/android/app/src/main/java/com/zvault/VaultRepository.kt` — `syncNow()` wired to real implementation

---

### P2 — Add force sync (push to all / pull from all) command across all interfaces

**Status:** ✅ Done
**Branch:** `feat/force-sync-command`
**Requested:** 2026-08-16

#### Description

Add a user-triggered "Sync Now" action that performs a full bidirectional sync: builds a full sync message from the current vault state and publishes it to ALL admitted devices via all configured relays, then pulls any pending events from relays and applies them. This is useful for:

- Initial sync after pairing a new device (auto-sync hasn't kicked in yet)
- Recovering from a period of offline operation
- User confidence ("I want to make sure everything is up to date")
- Debugging sync issues (explicit trigger with visible feedback)

The action must be available on all four interfaces: CLI command, desktop UI button, browser extension UI button, and Android UI button.

**Behaviour:**
1. **Push phase:** Build a `SyncMessage` (op: Full) from the current vault state. For each admitted, non-revoked device (excluding self), NIP-44 encrypt the message for that device's pubkey, NIP-59 gift-wrap it, and publish the gift-wrapped event to every enabled relay.
2. **Pull phase:** Connect to all enabled relays (if not already connected). Subscribe with filter `{ kinds: [1059], #p: [own_pubkey], since: last_sync_timestamp }`. Collect events until EOSE (or timeout). Unwrap, decrypt, and apply each valid sync message. Save the updated vault.
3. **Feedback:** Report to the user: number of messages sent, number of messages received and applied, final vault version. Surface any errors (relay connection failures, rejected messages) as warnings, not hard failures.

**Prerequisite:** P1 must be complete (sync infrastructure must exist on each interface before "force sync" can be implemented on top of it).

#### Scope

**CLI (`crates/zvault-cli/`):**
- Add `zvault sync` subcommand (no `send`/`receive` qualifier) that performs the full push+pull cycle to all peers on all enabled relays
- Retain existing `zvault sync send --recipient <pubkey>` and `zvault sync receive` for targeted operations
- Output: `✓ Pushed to N devices via M relays. Received K messages. Vault version: V`

**Desktop app (`apps/desktop/`):**
- Add Tauri command `force_sync() → SyncResult { sent: u32, received: u32, version: u64, warnings: Vec<String> }`
- Add "Sync Now" button in the UI header/toolbar (visible when vault is unlocked)
- Show spinner during sync, then toast with result summary
- Disable auto-sync briefly during force sync to avoid race conditions

**Browser extension (`apps/extension/`):**
- Add "Sync Now" button in the popup UI (visible when vault is unlocked)
- Invoke background script's `forceSyncAll()` function (push + pull cycle)
- Show sync status indicator (syncing / synced / error) in popup
- Badge icon update on successful sync (brief checkmark overlay)

**Android app (`apps/android/`):**
- Add "Sync Now" button on the vault list screen toolbar
- Trigger immediate WorkManager one-shot sync (or direct coroutine if foreground)
- Show snackbar with result: "Synced: sent to N devices, received K updates"
- Pull-to-refresh on vault list also triggers force sync

#### Definition of Done

- [ ] `zvault sync` CLI command performs full push+pull and reports results
- [ ] Desktop "Sync Now" button triggers force sync and shows result toast
- [ ] Extension "Sync Now" button triggers force sync and shows status
- [ ] Android "Sync Now" button triggers force sync and shows snackbar result
- [ ] Push phase sends to ALL admitted non-revoked devices (not just one recipient)
- [ ] Pull phase applies all pending messages and saves vault
- [ ] Partial failures (one relay down) do not block sync on other relays — errors reported as warnings
- [ ] Stale/replay messages are correctly rejected (vault_version and Lamport clock guards)
- [ ] All tests pass (`cargo test --workspace --all-features`)
- [ ] Zero clippy warnings
- [ ] Security review completed (no CRITICAL/MEDIUM open)
- [ ] Committed and pushed to branch

#### Expected Outputs

- `crates/zvault-cli/src/main.rs` — new `SyncAction::All` variant and `cmd_sync_all()` implementation
- `apps/desktop/src-tauri/src/main.rs` — new `force_sync` Tauri command
- `apps/desktop/src/components/SyncButton.tsx` — new file: Sync Now button component
- `apps/extension/src/entrypoints/background.ts` — new `forceSyncAll()` function
- `apps/extension/src/entrypoints/popup/components/SyncButton.tsx` — new file: popup sync button
- `apps/android/app/src/main/java/com/zvault/VaultViewModel.kt` — `forceSyncAll()` action
- `apps/android/app/src/main/java/com/zvault/ui/screens/VaultListScreen.kt` — Sync Now button in toolbar

## Bugs

<!-- Bug items are appended here by the triage subagent -->
<!-- Format: see process.md "Bug Item Format" -->

## Milestone overview

| Milestone | Name | Status |
|---|---|---|
| M0 | Foundation — repo, CI, workspace scaffold | ✅ Done |
| M1 | Core crypto — Argon2id KDF, AES-256-GCM | ✅ Done |
| M2 | Vault data model — CRUD, serialisation, on-disk format | ✅ Done |
| M3 | Device lifecycle — keypair, admit/revoke, CRDT | ✅ Done |
| M4 | Nostr sync — NIP-44/59, relay pub/sub, conflict resolution | ✅ Done |
| M5 | Desktop app shell — Tauri, React UI, vault CRUD | ✅ Done |
| M6 | Biometric unlock — keychain, OS secure enclave integration | ✅ Done |
| M7 | Import / Export — Bitwarden, 1Password, CSV, KDBX, `.zvault-export` | ✅ Done |
| M8 | Audit log — hash chain, storage, verification, UI | ✅ Done |
| M9 | Browser extension — WXT, WASM core, auto-fill | ✅ Done |
| M10 | Android app — UniFFI, Compose UI, Keystore, AutofillService | ✅ Done |
| M11 | CLI tool — clap subcommands, scripting support | ✅ Done |
| M12 | Hardening & v1.0 — pentest, fuzz, docs, release artefacts | ✅ Done |

## Phase summary

- **Phase 1 (M0–M4):** Core library — ~10–14 weeks
- **Phase 2 (M5–M8):** Desktop app — ~8–10 weeks
- **Phase 3 (M9–M11):** Additional clients — ~10–14 weeks
- **Phase 4 (M12):** Hardening & release — ~4 weeks

**Total estimated duration:** 32–36 weeks (single engineer full-time)

## Definition of done per milestone

1. All features for the milestone are implemented
2. `cargo test --workspace` passes
3. No `clippy` warnings
4. `cargo audit` reports no vulnerabilities
5. Relevant integration tests pass
6. PR reviewed and merged to `main`
7. DESIGN.md updated to reflect any design decisions made during implementation

## Guiding principles

- Core-first: `zvault-core` complete before any UI is built on top
- Desktop-first UI: Tauri desktop is the primary v1 client
- Vertical slices: each phase ships a working end-to-end slice
- Security review at each milestone before proceeding

## Current phase: Release preparation

All milestones are complete. Current activities:
- Final documentation review and update
- README usage examples and platform sections
- Steering docs aligned with delivered state
- Preparing v1.0 release artefacts

## Completed milestones

### M0 — Foundation (complete)
- Cargo workspace (`zvault-core`, `zvault-cli`)
- GitHub Actions CI (`cargo test`, `cargo clippy`, `cargo audit`, `cargo fmt`)
- Dependabot, PR template, issue templates
- All module stubs with doc comments and `todo!("Mx")` markers
- DESIGN.md full architecture and threat model
- CONTRIBUTING.md, branch protection rules

### M1 — Core Crypto (complete, commit 6075ad1)
Delivered:
- `VaultKey` newtype wrapping `Zeroizing<[u8; 32]>` — key material zeroed on drop
- `KdfParams` struct: salt (32 bytes) + `m_cost`/`t_cost`/`p_cost`; binary and JSON serde
- `derive_key(password, &KdfParams) -> Result<VaultKey>` — Argon2id RFC 9106
- `encrypt(key, plaintext) -> Result<Vec<u8>>` — AES-256-GCM, fresh 12-byte IV per write
- `encrypt_with_params(key, plaintext, &KdfParams) -> Result<Vec<u8>>` — for rekey / testing
- `decrypt(key, blob) -> Result<Vec<u8>>` — authenticates header + ciphertext via GCM tag
- 21 tests: all pass, 0 clippy warnings

On-disk format (64-byte header):
```
magic(8) | salt(32) | m_cost_le(4) | t_cost_le(4) | p_cost_le(4) | iv(12) | ct | tag(16)
```
The full header (magic + KDF params + IV) is included in AES-GCM AAD, so any tampering with the header is detected during authentication.

### M2 — Vault Data Model (complete, commit b054e7e + security fixes)
Delivered:
- `Vault` CRUD: `add_item`, `update_item`, `delete_item` (order-preserving `Vec::remove`), `get_item`, `list_items`
- `Vault::to_json() -> Result<Zeroizing<Vec<u8>>>` and `from_json` — plaintext zeroed on drop
- `Error::ItemNotFound(Uuid)` variant — clean separation from `InvalidVaultFile`
- `VaultFile` struct holding `path` + `kdf_params` (essential for `save` correctness)
- `VaultFile::create(password, path) -> (VaultFile, VaultKey)` — fresh KdfParams, atomic write
- `VaultFile::open(password, path) -> (VaultFile, VaultKey, Vault)` — two-step parse→derive→decrypt
- `VaultFile::save(&key, &vault)` — uses stored `kdf_params` so in-memory key stays valid
- `VaultFile::rekey(old_pw, new_pw) -> (VaultFile, VaultKey, Vault)` — fresh KdfParams, atomic write
- `atomic_write`: appends `.tmp` to full filename (not extension replacement)
- All intermediate plaintext buffers wrapped in `Zeroizing<Vec<u8>>`
- 13 CRUD unit tests + 15 VaultFile integration tests (tempfile)

Security review findings addressed:
- MEDIUM: `save` used `encrypt()` generating new KDF params — fixed to `encrypt_with_params` with stored params
- MEDIUM: plaintext JSON buffers not zeroed — `to_json` now returns `Zeroizing<Vec<u8>>`; `decrypt` output wrapped at call sites
- MEDIUM: `delete_item` used `swap_remove` — fixed to `Vec::remove` (order-preserving)
- LOW: `atomic_write` used `with_extension("tmp")` — fixed to append `.tmp` to full filename
- LOW: `VaultItem::Clone` copies sensitive fields — accepted risk; doc warning added; re-evaluate at M5
- INFO: Timestamp vs version counter — documented in tech.md; M4 must use `version` for conflict detection

### M3 — Device Lifecycle (complete)
Delivered:
- `DeviceIdentity` — in-memory view of this device: `device_id` (UUID) + `pubkey_hex`; secret key never stored in struct
- `DeviceIdentity::generate(label, storage)` — generates secp256k1 keypair, persists secret to `SecureStorage`, returns identity + key material
- `DeviceIdentity::load_secret_key(storage)` — retrieves secret key wrapped in `Zeroizing<Vec<u8>>`
- `SecureStorage` trait — abstraction over OS secure key storage; `InMemoryStorage` for tests (gated behind `#[cfg(any(test, feature = "test-helpers"))]`)
- `OrSet<T>` — generic OR-Set CRDT: `add`, `remove`, `contains`, `elements`, `merge`; add-wins semantics
- `DeviceManager` — wraps vault's device list with CRDT-backed operations: `bootstrap`, `admit`, `revoke`, `merge`, `flush`
- `DeviceManager::from_vault(vault)` — reconstructs CRDT state from `Vault::devices` using deterministic tokens
- `DeviceManager::flush(vault)` — writes device list back to vault, bumps `vault.version`
- `Cargo.toml`: `test-helpers` feature declared so downstream crates can use `InMemoryStorage`
- 27 device module tests (keypair, OR-Set CRDT, DeviceManager lifecycle, CRDT merge, vault round-trips); 82 total workspace tests pass

Security review findings addressed:
- INFO: `k256::ecdsa::SigningKey` implements `ZeroizeOnDrop` — secret scalar zeroed on drop automatically
- INFO: `DeviceManager::Clone` only clones `OrSet<Uuid>` and `Vec<DeviceEntry>` — no secret material
- INFO: `InMemoryStorage` gated to test code; documented as non-production in doc comment

### M4 — Nostr Sync (complete)
Delivered:
- **NIP-44 v2 encryption/decryption** — ECDH (k256), HKDF-extract (conversation key), HKDF-expand (message keys: ChaCha20 key + nonce + HMAC key), ChaCha20 stream cipher, HMAC-SHA256 MAC, NIP-44 padding (power-of-2 based), base64 encode/decode
- **NIP-01 event signing** — Schnorr BIP-340 via k256; `sign_event` and `verify_event`; canonical event ID = SHA-256 of `[0, pubkey, created_at, kind, tags, content]`
- **NIP-59 gift-wrap** — triple-wrap (rumor → seal → gift-wrap); ephemeral random key for outer wrap; timestamp randomisation (±2 days); `gift_wrap` and `unwrap_gift_wrap`
- **Sync engine** — `build_full_sync_message` (serialise vault → NIP-44 encrypt → SyncMessage); `apply_sync_message` (validate sender, stale guard, decrypt, merge items LWW, merge devices CRDT, update version)
- **LamportClock** — tick (send) and update (receive) for causal ordering
- **Dependencies:** `chacha20 = "0.9.1"` added to workspace
- **Test vectors:** NIP-44 spec test vector verified (sec1=0x01, sec2=0x02, nonce=0x01, plaintext="a")
- 24 new tests (17 nostr + 7 sync); 106 total workspace tests pass

Security review findings addressed:
- LOW: message keys (chacha_key, hmac_key) on stack not wrapped in Zeroizing — short-lived, fresh per message, acceptable risk
- INFO: item conflict resolution uses `updated_at` timestamp (LWW per design doc); vault-level `version` counter gates stale messages