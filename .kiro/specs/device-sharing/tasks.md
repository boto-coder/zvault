# Implementation Plan: Device Sharing (Invite & Join-Request Flows)

## Overview

Implement the new device pairing UX with invite codes, join-request codes, QR display, and guided multi-step flows. The implementation proceeds: core codec → Tauri backend commands → WASM bindings → extension background → React UI. Each layer builds on the previous.

## Tasks

- [x] 1. Core: Pairing Codec Module (`crates/zvault-core/src/pairing.rs`)
  - [x] 1.1 Create `pairing.rs` module with PairingPayload struct and PairingType enum
    - Define `PairingPayload { v, t, p, l, vid, ts }` with Serialize/Deserialize
    - Define `PairingType` enum: `Invite`, `JoinRequest`, `InviteResponse`, `JoinResponse` with `#[serde(rename_all = "snake_case")]`
    - Add `pub mod pairing;` to `crates/zvault-core/src/lib.rs`
    - _Requirements: 4.1, 4.2, 4.3_

  - [x] 1.2 Implement `encode_pairing_code(payload) -> Result<String>`
    - Serialize payload to compact JSON
    - Base64url encode (no padding) using the `base64` crate with `URL_SAFE_NO_PAD` config
    - Prefix with `zvault:`
    - Verify total length is under 500 chars (return error if not)
    - _Requirements: 4.1, 4.4_

  - [x] 1.3 Implement `decode_pairing_code(code) -> Result<PairingPayload>`
    - Strip `zvault:` prefix (error if missing)
    - Base64url decode (error if invalid)
    - Parse JSON into `PairingPayload` (error if invalid)
    - Validate: `v == 1`, `p` is 64 hex chars, `l` is 1–64 after trim, `vid` is valid UUID if present, `ts > 0`
    - Return validated payload
    - _Requirements: 4.1, 4.2, 4.3, 10.1, 10.2, 10.3, 10.4_

  - [x] 1.4 Implement builder functions
    - `create_invite(pubkey_hex, label, vault_id) -> PairingPayload`
    - `create_join_request(pubkey_hex, label) -> PairingPayload`
    - `create_join_response(pubkey_hex, label) -> PairingPayload`
    - `create_invite_response(pubkey_hex, label, vault_id) -> PairingPayload`
    - All set `v: 1`, `ts: Utc::now().timestamp()`, appropriate `t` variant
    - _Requirements: 2.1, 3.1_

  - [x] 1.5 Add `vault_id: Uuid` field to `Vault` struct
    - Add field with `#[serde(default = "Uuid::new_v4")]` for backward compat with existing vault files
    - Generate a fresh vault_id in `Vault::new()` / `VaultFile::create()`
    - Ensure existing vaults without `vault_id` get one assigned on first open (migration)
    - _Requirements: 2.1_

  - [x] 1.6 Add `Error::InvalidPairingCode(String)` variant to error enum
    - Descriptive messages for each validation failure
    - _Requirements: 10.1, 10.2, 10.3, 10.4_

  - [x] 1.7 Write unit tests for pairing codec
    - Round-trip: encode → decode for all 4 types
    - Reject: missing prefix, invalid base64, bad JSON, bad version, bad pubkey (wrong length, non-hex), bad label (empty, >64), bad vid
    - Size check: typical payloads stay under 500 chars
    - Builder functions produce valid payloads (decode succeeds)
    - _Requirements: 4.4, 10.1–10.4_

- [x] 2. Checkpoint: Core pairing module compiles and tests pass
  - Run `cargo test -p zvault-core --all-features`
  - Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`

- [x] 3. Desktop Tauri Backend — Pairing Commands
  - [x] 3.1 Add `vault_id` to AppState/session if not already tracked
    - Ensure the vault_id is accessible from `session.vault.vault_id`
    - _Requirements: 2.1_

  - [x] 3.2 Implement `create_invite_code` Tauri command
    - Check vault unlocked, identity exists
    - Call `pairing::create_invite(identity.pubkey_hex, label, vault.vault_id)`
    - Encode and return `PairingCodeResult { code, payload }`
    - _Requirements: 2.1, 2.2, 2.3, 11.1_

  - [x] 3.3 Implement `create_join_request_code` Tauri command
    - Check vault unlocked, identity exists
    - Call `pairing::create_join_request(identity.pubkey_hex, label)`
    - Encode and return
    - _Requirements: 3.1, 3.2, 3.3, 11.1_

  - [x] 3.4 Implement `import_pairing_code` Tauri command
    - Check vault unlocked
    - Call `pairing::decode_pairing_code(code)`
    - Determine `action_needed` based on payload type:
      - `invite` or `join_request` → "confirm_and_respond"
      - `invite_response` or `join_response` → "confirm_final"
    - Build human-readable description
    - Return `PairingImportResult`
    - _Requirements: 2.4, 3.4, 8.3, 10.1–10.4_

  - [x] 3.5 Implement `confirm_pairing` Tauri command
    - Check vault unlocked, identity exists
    - Build `DeviceKeyMaterial` from payload's pubkey + label
    - Check for duplicate pubkey (device already admitted) → return informative error
    - Call `DeviceManager::admit` with proper admin identity
    - Flush + save vault atomically
    - Generate response code if needed (based on payload.t):
      - For `invite` → create `join_response`
      - For `join_request` → create `invite_response`
      - For responses → no further code needed
    - Return `PairingConfirmResult { admitted_device, response_code }`
    - _Requirements: 2.4, 2.5, 2.6, 3.4, 3.5, 5.3, 6.1–6.4, 8.5_

  - [x] 3.6 Register new commands with Tauri Builder
    - Add `create_invite_code`, `create_join_request_code`, `import_pairing_code`, `confirm_pairing` to invoke_handler
    - _Requirements: 2.1, 3.1_

  - [x] 3.7 Ensure device identity generation still works (from previous spec)
    - Verify `generate_device_identity` and `get_device_identity` commands exist and work
    - If not implemented yet, implement them now (prerequisite for pairing)
    - _Requirements: 1.1–1.6_

- [x] 4. Checkpoint: Desktop backend compiles and pairing commands work
  - Build: `cargo build` in `apps/desktop/src-tauri/`
  - Manually test with `cargo tauri dev` if possible

- [x] 5. WASM Bindings — Pairing Operations
  - [x] 5.1 Export pairing codec functions from `crates/zvault-wasm/src/lib.rs`
    - `create_invite_code(pubkey_hex, label, vault_id) -> String`
    - `create_join_request_code(pubkey_hex, label) -> String`
    - `decode_pairing_code(code) -> JsValue` (returns PairingPayload as JSON)
    - `create_response_code(response_type, pubkey_hex, label, vault_id?) -> String`
    - All return `Result<_, JsValue>` with descriptive error messages
    - _Requirements: 2.1, 3.1, 4.1, 10.1–10.4_

  - [x] 5.2 Export `admit_device_from_pairing` function
    - Takes vault_json, admin identifiers, remote pubkey + label
    - Uses DeviceManager::admit internally
    - Returns updated vault JSON
    - _Requirements: 6.1, 6.3_

- [x] 6. Checkpoint: WASM compiles with `wasm-pack build`
  - Run `wasm-pack build crates/zvault-wasm --target web`

- [x] 7. Browser Extension Background — Pairing Handlers
  - [x] 7.1 Add pairing message types to background.ts
    - `CREATE_INVITE_CODE`, `CREATE_JOIN_REQUEST_CODE`, `IMPORT_PAIRING_CODE`, `CONFIRM_PAIRING`
    - _Requirements: 2.1, 3.1_

  - [x] 7.2 Implement pairing message handlers
    - `CREATE_INVITE_CODE`: get device identity from storage, call WASM `create_invite_code`
    - `CREATE_JOIN_REQUEST_CODE`: get device identity, call WASM `create_join_request_code`
    - `IMPORT_PAIRING_CODE`: call WASM `decode_pairing_code`, return payload + action_needed
    - `CONFIRM_PAIRING`: call WASM `admit_device_from_pairing`, re-encrypt vault, generate response if needed
    - All check vault is unlocked first
    - _Requirements: 2.1–2.8, 3.1–3.6, 11.1, 11.2_

- [x] 8. React UI — Pairing Flow Components
  - [x] 8.1 Add QR code rendering dependency
    - Install `qrcode.react` in `apps/desktop/` (npm)
    - Install `qrcode.react` in `apps/extension/` (npm)
    - _Requirements: 2.3, 3.3, 9.1, 9.2_

  - [x] 8.2 Create `QRCodeDisplay` component
    - Takes a string value, renders as QR code
    - Shows fallback text if QR rendering fails
    - Add copy-to-clipboard button alongside QR
    - _Requirements: 2.3, 3.3, 4.5_

  - [x] 8.3 Create `ImportCodeInput` component
    - Text input / textarea for pasting a `zvault:` code
    - Auto-detects code type on paste
    - Shows validation errors inline
    - "Import" button triggers `import_pairing_code` call
    - _Requirements: 8.3, 10.1–10.4_

  - [x] 8.4 Create `InviteDeviceDialog` component (multi-step)
    - Step 1: Generate invite code → show QR + copyable text + instructions
    - Step 2: Input for response code (wait for invitee's response)
    - Step 3: Show confirmation (invitee's info) → confirm → done
    - Show step indicator (1/3, 2/3, 3/3)
    - _Requirements: 2.1–2.7, 8.1, 8.4, 8.6_

  - [x] 8.5 Create `JoinRequestDialog` component (multi-step)
    - Step 1: Generate join-request code → show QR + copyable text + instructions
    - Step 2: Input for response code (wait for existing device's response)
    - Step 3: Show confirmation → confirm → done
    - _Requirements: 3.1–3.6, 8.2, 8.4, 8.6_

  - [x] 8.6 Create `ConfirmPairingDialog` component
    - Shows remote device label + truncated pubkey
    - "Confirm" and "Cancel" buttons
    - Warning text about what admission means
    - _Requirements: 5.3, 8.5_

  - [x] 8.7 Update Device_Panel with new buttons and layout
    - Add "Invite Device" button → opens InviteDeviceDialog
    - Add "Request to Join" button → opens JoinRequestDialog
    - Add "Paste Code" button/input → opens ImportCodeInput
    - Move current "Admit by pubkey" to "Advanced" section
    - Always show "My Device" section with pubkey + copy button
    - _Requirements: 7.1, 8.1, 8.2, 8.3, 8.7_

  - [x] 8.8 Wire pairing UI to Tauri invoke (desktop)
    - InviteDeviceDialog calls `invoke("create_invite_code")`, `invoke("import_pairing_code")`, `invoke("confirm_pairing")`
    - JoinRequestDialog calls `invoke("create_join_request_code")`, `invoke("import_pairing_code")`, `invoke("confirm_pairing")`
    - Handle errors at each step with user-friendly messages
    - _Requirements: 2.1–2.7, 3.1–3.6_

  - [x] 8.9 Wire pairing UI to extension message passing
    - Same flow as desktop but using `browser.runtime.sendMessage` with pairing message types
    - _Requirements: 2.1–2.7, 3.1–3.6_

- [x] 9. Checkpoint: End-to-end pairing works in desktop app
  - Test invite flow manually between two instances
  - Test join-request flow manually

- [x] 10. CLI: Pairing Code Support
  - [x] 10.1 Add `zvault pair invite` subcommand
    - Prints invite code to stdout
    - _Requirements: 9.4_

  - [x] 10.2 Add `zvault pair request` subcommand
    - Prints join-request code to stdout
    - _Requirements: 9.4_

  - [x] 10.3 Add `zvault pair import <code>` subcommand
    - Accepts a pairing code as argument or via stdin
    - Decodes, shows info, prompts for confirmation (y/n)
    - On confirm: admits the device, prints response code if needed
    - _Requirements: 9.4, 9.5_

- [x] 11. Tests — Integration and Property
  - [x] 11.1 Integration test: full invite flow (A invites B)
    - A creates invite → B decodes → B confirms (admits A, gets response) → A decodes response → A confirms (admits B)
    - Assert: A's vault has B's device, B's vault has A's device
    - _Requirements: 2.7_

  - [x] 11.2 Integration test: full join-request flow (B requests, A accepts)
    - B creates request → A decodes → A confirms (admits B, gets response) → B decodes response → B confirms (admits A)
    - Assert: both vaults have both devices
    - _Requirements: 3.5_

  - [x] 11.3 Integration test: backward compat with manual admit
    - Device admitted via old `admit_device(pubkey, label)` → same vault state as pairing flow
    - Old and new devices coexist in device list
    - _Requirements: 7.1, 7.2, 7.3_

  - [x] 11.4 Property test: encode/decode round-trip
    - For all valid payloads: decode(encode(p)) == p
    - _Requirements: 4.1_

  - [x] 11.5 Property test: decode never panics
    - For all random strings: decode either returns Ok or Err (never panics)
    - _Requirements: 10.4_

  - [x] 11.6 Unit tests for edge cases
    - Code with trailing whitespace / newlines (from copy-paste) → still decodes
    - Code from URL fragment (`https://zvault.app/pair#zvault:...`) → strip URL prefix, decode
    - Empty code, code with only prefix, very long code
    - _Requirements: 10.1–10.4_

- [x] 12. Final Checkpoint
  - `cargo test --workspace --all-features`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `wasm-pack build crates/zvault-wasm --target web`
  - Desktop builds: `cargo tauri build` (from `apps/desktop/src-tauri/`)
  - Extension builds: `npx wxt build` and `npx wxt build --browser firefox` (from `apps/extension/`)

## Notes

- The pairing codec is intentionally simple (JSON + base64url) — no encryption of the code itself, since it contains only public data.
- QR code rendering is frontend-only. The core library doesn't depend on any QR library.
- The `vault_id` field addition to `Vault` requires a migration strategy for existing vaults (assign a random UUID on first open if missing).
- The "Paste Code" universal input auto-detects code type, making it the simplest entry point for users who already have a code.
- The old manual "Admit by pubkey" flow remains as an advanced option for power users and backward compatibility.

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "1.2", "1.3", "1.4", "1.5", "1.6"] },
    { "id": 1, "tasks": ["1.7"] },
    { "id": 2, "tasks": ["3.1", "3.7"] },
    { "id": 3, "tasks": ["3.2", "3.3", "3.4", "3.5", "3.6"] },
    { "id": 4, "tasks": ["5.1", "5.2"] },
    { "id": 5, "tasks": ["7.1", "7.2"] },
    { "id": 6, "tasks": ["8.1", "8.2", "8.3"] },
    { "id": 7, "tasks": ["8.4", "8.5", "8.6"] },
    { "id": 8, "tasks": ["8.7", "8.8", "8.9"] },
    { "id": 9, "tasks": ["10.1", "10.2", "10.3"] },
    { "id": 10, "tasks": ["11.1", "11.2", "11.3", "11.4", "11.5", "11.6"] }
  ]
}
```
