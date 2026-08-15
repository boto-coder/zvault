# Implementation Plan: TOTP All Interfaces

## Overview

Add full TOTP management (live code display with countdown, copy, add/edit secrets) to all ZVault interfaces: desktop (Tauri), browser extension (WXT), CLI, and Android. The core generation logic already exists — this spec surfaces it in the UI layer.

## Tasks

- [ ] 1. Add `generate_totp` Tauri command
  - [ ] 1.1 Add `TotpResponse` struct and `generate_totp` command to `apps/desktop/src-tauri/src/main.rs`
    - Returns `{ code, remaining_seconds }`
    - Uses `totp_rs::TOTP` with SHA-1, 6 digits, 30s period
    - Register in `invoke_handler`
    - _Requirements: 6.1–6.5_

  - [ ] 1.2 Add `validate_totp_secret` Tauri command
    - Takes `secret: String`, returns `Ok(())` or error string
    - Attempts to construct `TOTP::new(...)` — if it fails, return the error
    - Register in `invoke_handler`
    - _Requirements: 7.1, 7.2_

- [ ] 2. Add desktop `TotpDisplay` component
  - [ ] 2.1 Create `apps/desktop/src/components/TotpDisplay.tsx`
    - Props: `secret: string`
    - Calls `invoke("generate_totp", { secret })` every 1 second
    - Displays 6-digit code in large monospace font (split as "123 456" for readability)
    - Displays countdown (numeric seconds + optional progress indicator)
    - "Copy" button → copies code to clipboard, shows "Copied!" for 2s
    - Clipboard auto-clear after 30s
    - Uses Tailwind classes consistent with existing dark theme
    - _Requirements: 1.1–1.6_

  - [ ] 2.2 Integrate `TotpDisplay` into `ItemDetail.tsx`
    - Show below the password field for login items with `totpSecret`
    - Don't show when `totpSecret` is null/empty
    - _Requirements: 1.1, 1.7_

- [ ] 3. Add TOTP secret field to desktop forms
  - [ ] 3.1 Add "TOTP Secret" input to `ItemDetail.tsx` edit mode
    - Text field for login items, positioned after password field
    - Validate on blur/save using `invoke("validate_totp_secret")`
    - Show inline error if invalid (red text, role="alert")
    - _Requirements: 2.1, 2.3, 2.4_

  - [ ] 3.2 Add "TOTP Secret" input to `AddItemModal` in `VaultList.tsx`
    - Optional field shown when kind is "login"
    - Same validation as edit mode
    - _Requirements: 2.2, 2.5_

  - [ ] 3.3 Update `handleSave` in ItemDetail to include `totpSecret` field
    - Currently the `itemJson` for update doesn't include `totp_secret` — add it
    - Map from `editTotpSecret` state to `totp_secret` in the JSON payload
    - _Requirements: 2.5_

- [ ] 4. Add `--totp` flag to CLI `get` command
  - [ ] 4.1 Add `#[arg(long)] totp: bool` to `Get` command struct
    - _Requirements: 3.1_

  - [ ] 4.2 Implement TOTP code generation in `cmd_get`
    - When `--totp` and item has secret: generate code using `totp_rs::TOTP`, print with remaining seconds
    - When `--totp` and item has no secret: print "No TOTP configured for this item"
    - When `--totp` is set, suppress raw TOTP secret display (even with `--show-password`)
    - _Requirements: 3.2, 3.3, 3.4, 3.5_

  - [ ] 4.3 Write a test for CLI TOTP output
    - Create vault with item containing known TOTP secret
    - Invoke `zvault get --totp`, verify output format matches `TOTP: <6-digits> (expires in <N>s)`
    - _Requirements: 3.2_

- [ ] 5. Add TOTP display to extension item detail
  - [ ] 5.1 Update extension `GENERATE_TOTP` handler to also return `remainingSeconds`
    - Calculate `30 - (Math.floor(Date.now() / 1000) % 30)` and include in response
    - Backward compatible — adds field, doesn't change existing `code` field
    - _Requirements: 4.2, 4.6_

  - [ ] 5.2 Create TOTP display section in extension item detail view
    - Poll `GENERATE_TOTP` every 1 second
    - Show code (large monospace) + countdown (numeric seconds) + copy button
    - Toast "Copied!" on copy
    - Only show for login items with `totp_secret` present in item data
    - Use existing dark theme inline styles
    - _Requirements: 4.1–4.5_

- [ ] 6. Add TOTP badge and copy button to extension item list
  - [ ] 6.1 Update `LIST_ITEMS` response to include `hasTotp` boolean per item
    - Background `list_items` response checks if each item has a non-empty `totp_secret`
    - Exposes boolean flag without revealing the secret itself
    - _Requirements: 5.1_

  - [ ] 6.2 Add TOTP indicator and copy button to list items
    - Show 🕐 icon for items that have TOTP
    - Add "Copy TOTP" button (clock icon) alongside existing copy-password button
    - On click: fetch item's TOTP secret via `GET_ITEM`, send `GENERATE_TOTP`, copy result, show toast
    - _Requirements: 5.1–5.4_

- [ ] 7. Verification
  - `cargo build --workspace` succeeds
  - `cargo test --workspace --all-features` passes
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` zero warnings
  - Desktop: item with TOTP shows live code + countdown + copy
  - Extension: item with TOTP shows live code in detail and badge in list
  - CLI: `zvault get <id> --totp` prints live code with remaining seconds

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "1.2", "4.1"] },
    { "id": 1, "tasks": ["2.1", "3.1", "3.2", "3.3", "4.2", "4.3"] },
    { "id": 2, "tasks": ["2.2", "5.1"] },
    { "id": 3, "tasks": ["5.2", "6.1"] },
    { "id": 4, "tasks": ["6.2"] },
    { "id": 5, "tasks": ["7"] }
  ]
}
```

## Notes

- The `totp-rs` crate is already a workspace dependency and used in `zvault-wasm`
- The extension `GENERATE_TOTP` background handler already works — task 5.1 only adds `remainingSeconds` to the response
- Desktop `ItemDetail.tsx` already has `totpSecret` in the `ItemDetailData` interface but doesn't use it for code generation
- The 1-second polling interval for TOTP display is simple and works well for a 30-second period. A more efficient approach (timer synced to period boundary) is possible but unnecessary for v1
- Clipboard clear after 30s matches the product requirement for clipboard timeout
- TOTP validation uses the same `TOTP::new()` call as generation — if construction succeeds, the secret is valid
