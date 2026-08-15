# Implementation Plan: Bugfix — Extension Save Item + Device Identity Not Found

## Overview

Fix two blocking bugs: (1) the WASM `add_item` function requires a full `VaultItem` with `id` but the frontend sends a partial payload without one, and (2) the pairing flow fails when no device identity has been initialized because the UI doesn't auto-init or guide the user.

## Tasks

- [x] 1. Fix WASM `add_item` to accept input without `id`
  - [x] 1.1 Create `AddItemInput` struct in `crates/zvault-wasm/src/lib.rs`
    - Fields: `kind`, `name`, `username?`, `password?`, `totp_secret?`, `uris?`, `note?`, `card_number?`, `expiry?`, `cvv?`, `cardholder?`, `first_name?`, `last_name?`, `email?`, `phone?`, `address?`, `city?`, `country?`, `favourite?`
    - All optional fields use `Option<T>` with `#[serde(default)]`
    - No `id`, `created_at`, or `updated_at` — these are generated server-side
    - _Bug 1_

  - [x] 1.2 Update `add_item` WASM function to use `AddItemInput`
    - Deserialize `item_json` into `AddItemInput` instead of `VaultItem`
    - Construct a `VaultItem` from the input with `id: Uuid::new_v4()`, `created_at: Utc::now()`, `updated_at: Utc::now()`
    - Call `vault.add_item(item)` as before
    - Increment `vault.version` and update `vault.updated_at`
    - _Bug 1_

  - [x] 1.3 Write a test for `add_item` with a payload missing `id`
    - Create a vault JSON, call `add_item` with `{"kind":"login","name":"Test","password":"pw"}`
    - Assert it succeeds and the returned vault JSON contains the new item with a generated UUID
    - _Bug 1_

- [x] 2. Checkpoint: WASM build + tests pass
  - `cargo build -p zvault-wasm`
  - `cargo test --workspace --all-features`

- [x] 3. Fix pairing flow to auto-initialize device identity
  - [x] 3.1 Desktop Tauri: auto-init device in `create_invite_code` and `create_join_request_code`
    - If `vault.devices` is empty (no active non-revoked device):
      - Generate a secp256k1 keypair (same logic as `init_device`)
      - Create a `DeviceEntry` with label "Desktop" (or derive from hostname)
      - Add to `vault.devices`, bump version, save vault
      - Then proceed with the pairing code generation
    - _Bug 2_

  - [x] 3.2 Extension background: auto-init device in `CREATE_INVITE_CODE` and `CREATE_JOIN_REQUEST_CODE`
    - If no active device found in vault:
      - Call the existing `INIT_DEVICE` logic (generate keypair via WASM, store in extension storage, add to vault)
      - Use label "Browser Extension" as default
      - Re-parse the vault JSON after init
      - Then proceed with pairing code generation
    - _Bug 2_

  - [x] 3.3 Ensure auto-init is idempotent
    - If a device already exists, the pairing flow proceeds as before (no re-init)
    - If the auto-init succeeds, subsequent calls find the device and skip init
    - _Bug 2_

- [x] 4. Checkpoint: Desktop + extension build
  - `cargo build --workspace`
  - Extension TypeScript check

- [x] 5. Regression tests
  - [x] 5.1 Test: add_item with minimal JSON (just kind + name) succeeds
  - [x] 5.2 Test: add_item with full JSON (all fields including uris array) succeeds
  - [x] 5.3 Test: pairing code generation on empty vault auto-creates device identity

- [x] 6. Final verification
  - `cargo build --workspace`
  - `cargo test --workspace --all-features`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo fmt --all -- --check`

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "1.2", "1.3"] },
    { "id": 1, "tasks": ["2"] },
    { "id": 2, "tasks": ["3.1", "3.2", "3.3"] },
    { "id": 3, "tasks": ["4"] },
    { "id": 4, "tasks": ["5.1", "5.2", "5.3"] },
    { "id": 5, "tasks": ["6"] }
  ]
}
```

## Notes

- Bug 1 is the same pattern that the CLI's `--json` flag solved with `JsonItemInput` — but the WASM crate has its own `add_item` that wasn't updated
- Bug 2 device auto-init should use the same keypair generation as the existing `INIT_DEVICE`/`init_device` handlers — don't duplicate the logic, call into it
- The desktop app stores device secret keys in the OS keychain (via `keyring` crate); the extension stores them in `browser.storage.local`
- Auto-init is preferable to blocking the user with a dialog — pairing is already a multi-step flow and adding another step before it starts would be confusing
- The default device label should be platform-identifiable ("Desktop — <hostname>" or "Firefox Extension") so users can distinguish devices in the list
