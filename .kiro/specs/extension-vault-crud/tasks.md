# Implementation Plan: Extension Vault CRUD

## Overview

Add item creation capability to the ZVault browser extension. This involves a new `generate_password` Rust/WASM function, a TypeScript bridge update, a new `ItemCreateView` React component with type-specific forms, view routing, current-tab URL auto-fill, form validation, and fire-and-forget Nostr sync after save. All code uses Rust (WASM crate) and TypeScript/React (extension popup), matching the existing inline-style dark theme.

## Tasks

- [ ] 1. Add `generate_password` to the WASM crate
  - [ ] 1.1 Implement `generate_password` function in `crates/zvault-wasm/src/lib.rs`
    - Add a `#[wasm_bindgen]` function `generate_password(length: Option<u32>) -> Result<String, JsValue>`
    - Use `getrandom` crate (with `js` feature) as entropy source
    - Guarantee at least one character from each of four classes: uppercase (A-Z), lowercase (a-z), digit (0-9), special (`!@#$%^&*()_+-=[]{}|;:,.<>?`)
    - Fill remaining positions from the combined character set
    - Fisher-Yates shuffle the result using additional random bytes
    - Return error if length < 4
    - Default to length 20 when `None` is passed
    - _Requirements: 7.2, 7.4, 12.1, 12.2, 12.3, 12.4, 12.5_

  - [ ]* 1.2 Write unit tests for `generate_password` in `crates/zvault-wasm/src/lib.rs` or a tests module
    - **Property 5: Password generation guarantees character class coverage** — for random valid lengths (4..128), verify output length matches and all 4 character classes are present
    - **Property 6: Password generation rejects invalid lengths** — for lengths 0, 1, 2, 3, verify an error is returned
    - Test default length (None → 20 characters)
    - **Validates: Requirements 7.4, 12.2, 12.4, 12.5**

- [ ] 2. Update the WASM TypeScript bridge
  - [ ] 2.1 Add `generate_password` to the `ZVaultWasm` interface and `initWasm()` wiring in `apps/extension/src/lib/wasm.ts`
    - Add `generate_password(length?: number): string` to the `ZVaultWasm` interface
    - Wire `glueModule.generate_password` into the `wasmInstance` object in `initWasm()`
    - _Requirements: 12.1_

- [ ] 3. Implement view routing and Add button
  - [ ] 3.1 Update the `View` type and `App` component in `apps/extension/src/entrypoints/popup/App.tsx`
    - Add `"create-item"` to the `View` type union
    - Add a `case "create-item"` branch in the App switch that renders `<ItemCreateView>`
    - Pass `onSave` (calls `loadItems()` then `setView("items")`) and `onCancel` (calls `setView("items")`) props
    - _Requirements: 1.2, 1.3_

  - [ ] 3.2 Add an "Add" button to `ItemListView` in `apps/extension/src/entrypoints/popup/App.tsx`
    - Place a "+" / "Add" button in the header bar next to the Lock button
    - Wire it to call a new `onAdd` prop that triggers `setView("create-item")`
    - Use consistent inline styles (border, background, color matching existing buttons)
    - _Requirements: 1.1_

- [ ] 4. Implement the `ItemCreateView` component
  - [ ] 4.1 Create the `ItemCreateView` component shell and item type selector
    - Add a new `ItemCreateView` function component in `apps/extension/src/entrypoints/popup/App.tsx`
    - Accept props: `onSave: () => void`, `onCancel: () => void`
    - Render a header with "Add Item" title and a Cancel/Back button
    - Render a dropdown/select for item type with four options: Login, Secure Note, Card, Identity (default: Login)
    - Use local `useState` for all form state
    - Apply scrollable container with `overflowY: "auto"`
    - Handle Escape key to trigger cancel
    - _Requirements: 1.2, 1.3, 2.1, 2.2, 2.3, 13.2, 14.1, 14.2, 14.3_

  - [ ] 4.2 Implement the Login form fields
    - Conditionally render when `kind === "login"`: name (required), username, password (with generate button), TOTP secret, and URI list
    - Each URI entry has a text input and a match-strategy select (Domain, Host, StartsWith, Exact, Regex, Never)
    - Include "Add URI" button to append a new empty URI entry
    - Include "Remove" button on each URI entry (except when only one exists)
    - Auto-fill first URI from current tab URL on mount (query `browser.tabs`, HTTPS only, match=Domain)
    - Apply `aria-label` on icon-only buttons (generate, add URI, remove URI)
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 7.1, 8.1, 8.2, 8.3, 13.1, 13.4_

  - [ ] 4.3 Implement the Secure Note form fields
    - Conditionally render when `kind === "secure_note"`: name (required), note (multi-line textarea, min 4 visible lines)
    - _Requirements: 4.1, 4.2_

  - [ ] 4.4 Implement the Card form fields
    - Conditionally render when `kind === "card"`: name (required), cardholder name, card number, expiry date, CVV (masked by default using `type="password"`)
    - _Requirements: 5.1, 5.2_

  - [ ] 4.5 Implement the Identity form fields
    - Conditionally render when `kind === "identity"`: name (required), first name, last name, address, city, country, phone, email
    - _Requirements: 6.1_

  - [ ] 4.6 Implement password generation UI
    - Add a "Generate" button next to the password field in the Login form
    - On click, call `browser.runtime.sendMessage({ type: "GENERATE_PASSWORD" })` or import WASM bridge directly in popup
    - Populate the password field with the generated value
    - Handle errors (show inline error if generation fails)
    - _Requirements: 7.1, 7.3_

  - [ ] 4.7 Implement form validation and save
    - Validate name field is non-empty (trim whitespace) before submission
    - Show inline validation error (`role="alert"`, color `#ff6b6b`) adjacent to name field if empty
    - Disable save button while save is in progress (`saving` state)
    - On submit: build `AddItemPayload` from form state (omit empty optional fields), send `ADD_ITEM` message to background
    - On success: call `onSave()` to navigate back to item list
    - On error: display error banner at top of form with `role="alert"`, retain form data
    - _Requirements: 9.1, 9.2, 9.3, 10.1, 10.3, 10.4, 13.3_

  - [ ] 4.8 Ensure accessibility compliance across the form
    - Every input has a paired `<label htmlFor="...">` with matching `id`
    - Tab order follows visual layout (no `tabIndex` overrides)
    - Escape triggers cancel, Enter on submit button triggers save
    - Icon-only buttons have `aria-label` attributes
    - Validation errors use `role="alert"`
    - _Requirements: 13.1, 13.2, 13.3, 13.4_

- [ ] 5. Checkpoint — Ensure build and lint pass
  - Ensure `cargo build -p zvault-wasm` succeeds
  - Ensure TypeScript compiles without errors in `apps/extension/`
  - Ensure `cargo clippy --workspace --all-targets --all-features -- -D warnings` produces zero warnings
  - Ask the user if questions arise.

- [ ] 6. Add fire-and-forget Nostr sync after save
  - [ ] 6.1 Add async sync trigger in the `ADD_ITEM` handler in `apps/extension/src/entrypoints/background.ts`
    - After successful persist to `browser.storage.local`, fire an async `triggerNostrSync()` call
    - The sync function builds a NIP-44/NIP-59 message and publishes to configured relays
    - Catch and log any sync errors (`console.warn`) — never propagate to popup response
    - Return `{ success: true }` to the popup before sync completes
    - _Requirements: 11.1, 11.2, 11.3_

- [ ] 7. Add `GENERATE_PASSWORD` message handler to background (if popup calls via message)
  - [ ] 7.1 Add a `GENERATE_PASSWORD` case in `handleMessage` in `apps/extension/src/entrypoints/background.ts`
    - Import and call `wasm.generate_password(payload?.length)` 
    - Return `{ password: string }` on success or `{ error: string }` on failure
    - _Requirements: 7.2, 12.1_

- [ ] 8. Final checkpoint — Full verification
  - Run `cargo build --workspace` — must succeed
  - Run `cargo test --workspace --all-features` — all tests pass
  - Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` — zero warnings
  - Run TypeScript build/lint for `apps/extension/`
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- The implementation language is Rust for the WASM crate and TypeScript/React for the extension UI — both already established in the project
- The existing `ADD_ITEM` handler in background.ts already handles persistence; task 6.1 only adds async sync
- Password generation can be invoked from the popup either via a background message or by initialising the WASM bridge in the popup context. The design uses a `GENERATE_PASSWORD` background message for consistency with other operations
- All styles use inline React `CSSProperties` matching the existing dark theme — no CSS files
- Property tests (task 1.2) target the Rust WASM function directly; UI property tests for React are deferred to a dedicated testing setup

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "2.1"] },
    { "id": 1, "tasks": ["1.2", "3.1", "3.2"] },
    { "id": 2, "tasks": ["4.1"] },
    { "id": 3, "tasks": ["4.2", "4.3", "4.4", "4.5"] },
    { "id": 4, "tasks": ["4.6", "4.7", "4.8"] },
    { "id": 5, "tasks": ["6.1", "7.1"] },
    { "id": 6, "tasks": ["8.1"] }
  ]
}
```
