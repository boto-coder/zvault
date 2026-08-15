# Implementation Plan: UX Improvements

## Overview

UX audit and improvement pass for the Firefox browser extension and Tauri desktop app. Fixes critical usability gaps (placeholder copy, no detail view, no search), adds device management UI with invite flow to both platforms, and polishes the desktop experience (full add form, password generator, keyboard shortcuts, delete confirmation).

## Tasks

- [ ] 1. Extension — Fix copy password and add toast system
  - [ ] 1.1 Add `GET_PASSWORD` handler to `apps/extension/src/entrypoints/background.ts`
    - Parse vault JSON, find item by ID, return `{ password }` or `{ error }`
    - _Requirements: 1.2_

  - [ ] 1.2 Create Toast component and state management in extension popup
    - Positioned top-center of popup, auto-dismiss after 2s
    - `role="status"` for accessibility
    - New toast replaces previous (no stacking)
    - Add `showToast(msg)` helper to App component
    - _Requirements: 6.1–6.4_

  - [ ] 1.3 Update `handleCopyPassword` to use real password
    - Send `GET_PASSWORD` message, copy result to clipboard via `navigator.clipboard.writeText`
    - Show "Copied!" toast
    - Schedule clipboard clear after 30s via `setTimeout`
    - _Requirements: 1.1, 1.3, 1.4_

- [ ] 2. Extension — Search/filter and suggested items
  - [ ] 2.1 Add search input to `ItemListView`
    - Text input at top of list, styled with existing dark theme tokens
    - Filter items by name + username (case-insensitive substring match)
    - Auto-focus on popup open (`autoFocus` prop)
    - Show "No items match your search." when empty
    - _Requirements: 2.1–2.4_

  - [ ] 2.2 Add "Suggested for this site" section
    - Query `browser.tabs.query({ active: true, currentWindow: true })` for current URL
    - Parse domain from HTTPS URLs
    - Filter items whose `uris` array contains a matching domain
    - Render "Suggested" section header + matching items at top, "All Items" header + rest below
    - Hide "Suggested" section if no matches or non-HTTPS tab
    - _Requirements: 5.1–5.4_

- [ ] 3. Extension — Item detail view
  - [ ] 3.1 Add `GET_ITEM` handler to background worker
    - Returns full item data (all fields including password, notes, etc.) for a given item ID
    - _Requirements: 3.2_

  - [ ] 3.2 Add `"item-detail"` to View type and implement ItemDetailView component
    - Navigate on item click from list (pass item ID to view state)
    - Show all fields by item type:
      - Login: username (copy), password (masked, show/hide, copy), URIs, TOTP (if present)
      - SecureNote: note content
      - Card: cardholder, number (masked), expiry, CVV (masked)
      - Identity: all identity fields
    - Back button to return to item list
    - Copy buttons next to copyable fields
    - _Requirements: 3.1–3.6_

  - [ ] 3.3 Add kind icons and visual improvements to item list
    - Prefix each item with type icon: 🔑 Login, 📝 Note, 💳 Card, 👤 Identity
    - Show first URI domain as subtitle for login items
    - Show 🕐 icon for items with TOTP
    - _Requirements: 4.1–4.3_

- [ ] 4. Extension — Device management
  - [ ] 4.1 Add device-related background handlers
    - `LIST_DEVICES`: return `vault.devices` array from session JSON
    - `ADMIT_DEVICE`: validate pubkey (64 hex), create DeviceEntry, bump version, re-encrypt, persist
    - `REVOKE_DEVICE`: find device, mark revoked, bump version, re-encrypt, persist
    - `GET_DEVICE_IDENTITY`: return this extension instance's pubkey (stored in `browser.storage.local`)
    - `INIT_DEVICE`: generate keypair via WASM, store in extension storage, add to vault
    - _Requirements: 12.8_

  - [ ] 4.2 Create `DevicesView` component in extension popup
    - List all devices (label, truncated pubkey `first16...`, revoked badge)
    - "This device" indicator on current device
    - Prominently show current device's full pubkey with Copy button
    - "Admit Device" button → inline form (pubkey hex + label inputs)
    - Instructional text: "To sync, both devices must admit each other. Share your key and enter theirs."
    - "Revoke" button per non-current device with confirmation prompt
    - "Initialise Device" button if no identity exists yet
    - _Requirements: 12.1–12.7, 14.1–14.3_

  - [ ] 4.3 Add "Devices" navigation button to item list header
    - Icon button (📱) next to Lock button in ItemListView header
    - Sets view to `"devices"`
    - _Requirements: 12.1_

- [ ] 5. Desktop — Full add item form
  - [ ] 5.1 Replace `AddItemModal` with comprehensive form
    - Type selector dropdown (Login, Secure Note, Card, Identity)
    - Conditional field rendering per type:
      - Login: name*, username, password (+ Generate button), TOTP secret, URI list (add/remove)
      - SecureNote: name*, note textarea
      - Card: name*, cardholder, card number, expiry, CVV
      - Identity: name*, first name, last name, email, phone, address, city, country
    - Name required validation (inline error if empty on submit)
    - TOTP secret validation via `invoke("validate_totp_secret")` if provided
    - Submit builds full `ItemInput` JSON matching existing `add_item` Tauri command format
    - _Requirements: 7.1–7.7_

- [ ] 6. Desktop — Password generator
  - [ ] 6.1 Add `generate_password` Tauri command
    - Accepts optional `length: u32` (default 20, minimum 4)
    - Same algorithm as WASM version (guarantee all 4 character classes, Fisher-Yates shuffle)
    - Register in `invoke_handler`
    - _Requirements: 8.1, 8.5_

  - [ ] 6.2 Add "Generate" button to password field in add/edit forms
    - Calls `invoke("generate_password")`, populates password field with result
    - Shows generated password in cleartext (type="text" while generated)
    - Button styled consistently with existing form buttons
    - _Requirements: 8.2–8.4_

- [ ] 7. Desktop — Delete confirmation
  - [ ] 7.1 Add confirmation modal before item deletion
    - Triggered by delete button click (instead of immediate deletion)
    - Modal text: "Delete [item name]? This action cannot be undone."
    - "Cancel" button (neutral) + "Delete" button (red/danger colour)
    - Only proceed with `invoke("delete_item")` on explicit Delete click
    - _Requirements: 9.1–9.4_

- [ ] 8. Desktop — Keyboard shortcuts
  - [ ] 8.1 Register global keyboard handlers in `App.tsx`
    - `Ctrl/Cmd+L`: call `handleLocked()` to lock vault
    - `Ctrl/Cmd+N`: open add item modal
    - `Ctrl/Cmd+F`: focus search input (add ref to search input)
    - `Escape`: close any open modal, or navigate back from detail view
    - Only active when vault is unlocked (check view state)
    - Prevent default browser behaviour for these combos
    - _Requirements: 10.1–10.5_

- [ ] 9. Desktop — Device management view
  - [ ] 9.1 Add Tauri commands for device management
    - `admit_device(pubkey_hex: String, label: String)` → validate pubkey (64 hex), add DeviceEntry, save vault, return device_id
    - `revoke_device(device_id: String)` → find device, mark revoked, save vault
    - `get_device_pubkey()` → return this device's pubkey from OS keychain via `keyring` crate (or None if not initialised)
    - `init_device(label: String)` → generate keypair via `DeviceIdentity::generate`, store secret in keychain, add to vault, save, return pubkey
    - Register all in `invoke_handler`
    - _Requirements: 11.4, 11.6, 13.4_

  - [ ] 9.2 Create `Devices.tsx` page (`apps/desktop/src/pages/Devices.tsx`)
    - List all devices using `invoke("list_devices")`: label, truncated pubkey, admitted date, revoked badge
    - Current device marked "(this device)" based on `invoke("get_device_pubkey")`
    - Prominently display current device's full pubkey (monospace, selectable) with Copy button
    - "Admit Device" button → modal dialog:
      - Public key (hex) input with format validation (64 hex chars)
      - Device label input
      - Instructional text explaining mutual-admit flow
      - Submit calls `invoke("admit_device")`
    - "Revoke" button per non-current device → confirmation modal:
      - "Revoke [label]? This device will no longer receive vault updates and its messages will be rejected."
      - Cancel + Revoke (red) buttons
      - Submit calls `invoke("revoke_device")`
    - "Initialise Device" button if `get_device_pubkey` returns None
    - Refresh device list after admit/revoke
    - _Requirements: 11.1–11.9, 13.1–13.3_

  - [ ] 9.3 Add navigation to Devices view from VaultList
    - Add "Devices" button to VaultList header (next to Lock button)
    - Update `View` type in `App.tsx` to include `{ page: "devices" }`
    - Add routing case in App switch
    - _Requirements: 11.1_

- [ ] 10. Verification
  - Extension builds without TypeScript errors (`npm run build` in `apps/extension/`)
  - Extension popup: search filters items, item detail shows all fields, toast appears on copy, devices view shows device list with admit/revoke
  - Desktop builds without errors (`cargo build --workspace` + `npm run build` in `apps/desktop/`)
  - Desktop: full add form works for all item types, password generator fills field, delete shows confirmation, keyboard shortcuts work, devices view with admit/revoke
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` zero warnings

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "1.2", "3.1"] },
    { "id": 1, "tasks": ["1.3", "2.1", "3.2", "3.3", "4.1"] },
    { "id": 2, "tasks": ["2.2", "4.2", "4.3"] },
    { "id": 3, "tasks": ["5.1", "6.1", "7.1", "8.1", "9.1"] },
    { "id": 4, "tasks": ["6.2", "9.2", "9.3"] },
    { "id": 5, "tasks": ["10"] }
  ]
}
```

## Notes

- The extension currently uses inline styles (no CSS framework) — all new components follow this pattern
- The desktop app uses Tailwind CSS — all new components follow existing class patterns
- The `GET_ITEM` handler returns all fields including sensitive ones (password, notes) — this is acceptable because the vault is already decrypted in memory in the background worker
- The device admit flow is simplified (no cryptographic verification of the pubkey in this layer) — the security boundary is enforced at the sync layer where messages must be signed by the admitted key
- Password generator in desktop uses `aes_gcm::aead::OsRng` (same approach as `zvault-core`'s crypto module) to avoid the `rand_core` version conflict documented in tech.md
- The extension device identity (keypair) is stored in `browser.storage.local` encrypted with the vault password — this mirrors the CLI's sidecar approach but uses browser storage instead of a filesystem file
