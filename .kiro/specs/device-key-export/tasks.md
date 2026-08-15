# Implementation Plan: Device Key Export & Display

## Overview

Add NIP-19 bech32 encoding to zvault-core, then expose key display/export across CLI, desktop, and extension.

## Tasks

- [ ] 1. Core: NIP-19 Module
  - [ ] 1.1 Add `bech32 = "0.11"` to workspace dependencies and zvault-core
    - _Requirements: 4.1, 4.2, 4.3_

  - [ ] 1.2 Create `crates/zvault-core/src/nip19.rs` with encode/decode functions
    - `encode_npub(pubkey: &[u8; 32]) -> String`
    - `encode_nsec(seckey: &[u8; 32]) -> Zeroizing<String>`
    - `decode_npub(npub: &str) -> Result<[u8; 32]>`
    - `decode_nsec(nsec: &str) -> Result<Zeroizing<[u8; 32]>>`
    - Use bech32 variant (not bech32m) per NIP-19
    - Validate HRP on decode ("npub" / "nsec")
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5_

  - [ ] 1.3 Add `pub mod nip19;` to `crates/zvault-core/src/lib.rs`

  - [ ] 1.4 Write unit tests for NIP-19 codec
    - Round-trip encode/decode for npub and nsec
    - Known test vectors
    - Reject invalid HRP, invalid length, bech32m input
    - _Requirements: 4.3, 4.5_

- [ ] 2. Checkpoint: `cargo test -p zvault-core --all-features` passes

- [ ] 3. CLI: `device show` and `device export-key` subcommands
  - [ ] 3.1 Add `Show` and `ExportKey` variants to `DeviceAction` enum
    - Both take `--vault <path>` argument
    - _Requirements: 6.1, 6.2, 6.3_

  - [ ] 3.2 Implement `cmd_device_show`
    - Prompt for password, decrypt device sidecar
    - Print device_id, label, pubkey hex, npub
    - Exit 1 if no device identity
    - _Requirements: 1.4, 2.3, 6.1, 6.4_

  - [ ] 3.3 Implement `cmd_device_export_key`
    - Prompt for password, decrypt device sidecar
    - Print warning, then nsec + hex
    - Exit 1 if no device identity
    - _Requirements: 3.6, 5.2, 5.4, 6.2, 6.3_

- [ ] 4. Checkpoint: CLI builds, `device show` and `device export-key` work

- [ ] 5. Desktop Tauri: Key display and export commands
  - [ ] 5.1 Implement `get_device_pubkey` Tauri command
    - Load device identity from AppState or SecureStorage
    - Convert hex pubkey to npub via `nip19::encode_npub`
    - Return `DevicePubkeyInfo { device_id, label, pubkey_hex, npub }`
    - _Requirements: 1.1, 1.2, 1.6_

  - [ ] 5.2 Implement `export_device_secret_key` Tauri command
    - Accept password param, re-verify against vault
    - Load secret key from KeyringStorage
    - Encode as nsec + hex
    - Return `DeviceSecretKeyInfo`
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 5.1, 5.2_

  - [ ] 5.3 Register new commands in Tauri Builder

- [ ] 6. WASM: npub encoding export
  - [ ] 6.1 Add `encode_npub_from_hex(pubkey_hex: &str) -> Result<String, JsValue>` to zvault-wasm
    - Decode hex → bytes, call `nip19::encode_npub`
    - _Requirements: 1.3, 4.1_

- [ ] 7. Extension Background: Key export handler
  - [ ] 7.1 Add `GET_DEVICE_PUBKEY` message handler
    - Returns device info with npub (uses WASM `encode_npub_from_hex`)
    - _Requirements: 1.3_

  - [ ] 7.2 Add `EXPORT_DEVICE_SECRET_KEY` message handler
    - Requires password in payload, re-verify
    - Load secret from encrypted storage, encode as nsec via WASM
    - Return nsec + hex
    - _Requirements: 3.7, 5.2_

- [ ] 8. React UI: Key display and export components
  - [ ] 8.1 Create `DevicePubkeyCard` component
    - Shows label, device_id, hex pubkey (truncated + copy), npub (full + copy)
    - Integrate into Devices page "My Device" section
    - _Requirements: 1.1, 1.2, 2.1, 2.2_

  - [ ] 8.2 Create `ExportSecretKeyDialog` component
    - Warning text + password input → submit → display nsec + hex + copy
    - 30-second auto-hide countdown
    - "Done" button to dismiss
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5_

  - [ ] 8.3 Wire components to desktop (Tauri invoke) and extension (sendMessage)
    - _Requirements: 1.1, 3.1, 3.7_

- [ ] 9. Final Checkpoint
  - `cargo test --workspace --all-features`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `wasm-pack build crates/zvault-wasm --target web`
  - Manual test: CLI device show + export-key

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "1.2", "1.3"] },
    { "id": 1, "tasks": ["1.4"] },
    { "id": 2, "tasks": ["3.1", "3.2", "3.3"] },
    { "id": 3, "tasks": ["5.1", "5.2", "5.3", "6.1"] },
    { "id": 4, "tasks": ["7.1", "7.2"] },
    { "id": 5, "tasks": ["8.1", "8.2", "8.3"] }
  ]
}
```
