# Implementation Plan: Nostr Relay Settings

## Overview

Add relay configuration with sensible defaults to zvault-core, expose via CLI/desktop/extension, and integrate with the sync engine.

## Tasks

- [ ] 1. Core: Settings Model and Relay Operations
  - [ ] 1.1 Create `crates/zvault-core/src/settings.rs` module
    - Define `DEFAULT_RELAYS` constant array
    - Define `VaultSettings` struct with `relays: Vec<RelayEntry>`
    - Define `RelayEntry` struct with `url`, `enabled`, `added_at`
    - Implement `Default` for `VaultSettings` (populates with DEFAULT_RELAYS)
    - Add `pub mod settings;` to lib.rs
    - _Requirements: 1.1, 1.2, 1.3, 8.1, 8.2, 8.3, 8.4, 8.5, 8.6_

  - [ ] 1.2 Add `settings: VaultSettings` field to `Vault` struct
    - Use `#[serde(default)]` for backward compatibility
    - Existing vaults without settings will get default relays on deserialize
    - Verify: open an existing vault → settings field populated with defaults
    - _Requirements: 2.1, 2.5, 2.6_

  - [ ] 1.3 Implement relay URL validation and normalization
    - `validate_relay_url(url: &str) -> Result<String>` — checks scheme, host, returns normalized
    - Normalization: lowercase scheme+host, strip trailing slashes
    - Reject: non-ws/wss schemes, empty host, whitespace
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5_

  - [ ] 1.4 Implement relay CRUD operations
    - `add_relay(settings: &mut VaultSettings, url: &str) -> Result<()>` — validate, check duplicate, push
    - `remove_relay(settings: &mut VaultSettings, url: &str) -> Result<()>` — find by normalized URL, remove
    - `set_relay_enabled(settings: &mut VaultSettings, url: &str, enabled: bool) -> Result<()>`
    - `reset_relays(settings: &mut VaultSettings)` — replace with defaults
    - `enabled_relay_urls(settings: &VaultSettings) -> Vec<&str>` — filter enabled
    - _Requirements: 1.4, 1.5, 3.2, 3.3, 3.4, 3.5, 3.6_

  - [ ] 1.5 Write unit tests for settings module
    - Default relays populated correctly
    - Add relay: valid URL succeeds, duplicate rejected, invalid rejected
    - Remove relay: exists succeeds, not found errors
    - Toggle: enable/disable works
    - Reset: returns to defaults
    - enabled_relay_urls: only returns enabled
    - Backward compat: deserialize vault JSON without "settings" → defaults applied
    - _Requirements: 1.1–1.5, 6.1–6.5_

- [ ] 2. Checkpoint: `cargo test -p zvault-core --all-features` passes

- [ ] 3. CLI: Relay Subcommands
  - [ ] 3.1 Add `Relay` command with `RelayAction` subcommand enum to CLI
    - Variants: List, Add, Remove, Enable, Disable, Reset
    - All take `--vault <path>` argument
    - _Requirements: 3.1–3.8_

  - [ ] 3.2 Implement `cmd_relay_list`
    - Open vault (requires password), print relay table with URL + enabled status
    - Format: `[✓] wss://relay.damus.io` or `[✗] wss://relay.example.com`
    - _Requirements: 3.1, 3.7_

  - [ ] 3.3 Implement `cmd_relay_add`
    - Open vault, validate URL, add relay, save vault
    - Print confirmation: "Added: wss://..."
    - _Requirements: 3.2, 3.7, 3.8_

  - [ ] 3.4 Implement `cmd_relay_remove`
    - Open vault, remove relay by URL, save vault
    - _Requirements: 3.3, 3.7, 3.8_

  - [ ] 3.5 Implement `cmd_relay_enable` and `cmd_relay_disable`
    - Open vault, toggle relay, save vault
    - _Requirements: 3.4, 3.5, 3.7, 3.8_

  - [ ] 3.6 Implement `cmd_relay_reset`
    - Open vault, reset relays to defaults, save vault
    - _Requirements: 3.6, 3.7, 3.8_

  - [ ] 3.7 Update `sync send` and `sync receive` to use vault relays as default
    - Make `--relay` optional (currently required)
    - If not provided, load enabled relays from `vault.settings`
    - If provided, use only the specified relay (override)
    - Error if no relays available
    - _Requirements: 3.9, 7.1, 7.4_

- [ ] 4. Checkpoint: CLI builds, relay commands work

- [ ] 5. Desktop Tauri: Relay Settings Commands
  - [ ] 5.1 Implement Tauri commands for relay management
    - `get_relay_settings` → returns Vec<RelayEntryDto>
    - `add_relay(url)` → validates, adds, saves, returns updated list
    - `remove_relay(url)` → removes, saves, returns updated list
    - `toggle_relay(url, enabled)` → toggles, saves, returns updated list
    - `reset_relays` → resets, saves, returns updated list
    - _Requirements: 4.1–4.5_

  - [ ] 5.2 Register relay commands in Tauri Builder

- [ ] 6. WASM Bindings: Relay Operations
  - [ ] 6.1 Export relay operations for the extension
    - `validate_relay_url(url) -> Result<String, JsValue>`
    - `add_relay_to_vault(vault_json, url) -> Result<String, JsValue>`
    - `remove_relay_from_vault(vault_json, url) -> Result<String, JsValue>`
    - `toggle_relay_in_vault(vault_json, url, enabled) -> Result<String, JsValue>`
    - `reset_relays_in_vault(vault_json) -> Result<String, JsValue>`
    - `get_enabled_relays(vault_json) -> Result<JsValue, JsValue>`
    - _Requirements: 5.1, 5.2, 5.3_

- [ ] 7. Extension Background: Relay Message Handlers
  - [ ] 7.1 Add relay message types and handlers
    - `GET_RELAY_SETTINGS`, `ADD_RELAY`, `REMOVE_RELAY`, `TOGGLE_RELAY`, `RESET_RELAYS`
    - Each handler: load vault JSON, call WASM, re-encrypt, persist, return updated list
    - _Requirements: 5.1, 5.2, 5.3_

- [ ] 8. React UI: Relay Settings Component
  - [ ] 8.1 Create `RelaySettings` component
    - Lists all relays with toggle switches and remove buttons
    - "Add Relay" input with URL validation
    - "Reset to Defaults" button
    - Warning banner when all relays disabled
    - _Requirements: 4.1–4.6_

  - [ ] 8.2 Integrate RelaySettings into desktop Settings page
    - Wire to Tauri invoke commands
    - _Requirements: 4.1–4.6_

  - [ ] 8.3 Integrate RelaySettings into extension settings view
    - Wire to browser.runtime.sendMessage
    - _Requirements: 5.1–5.3_

- [ ] 9. Sync Engine: Use Vault Relays
  - [ ] 9.1 Update sync engine to accept relay list parameter
    - Modify sync functions to take `relays: &[String]` instead of single URL
    - Attempt connection to all relays, publish to all, subscribe on first available
    - Log warnings on connection failures, continue with remaining
    - _Requirements: 7.1, 7.2, 7.3, 7.5_

  - [ ] 9.2 Handle "no relays" case
    - Return clear error if relay list is empty
    - _Requirements: 7.4_

- [ ] 10. Final Checkpoint
  - `cargo test --workspace --all-features`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `wasm-pack build crates/zvault-wasm --target web`
  - Manual: CLI `relay list/add/remove/reset` work
  - Manual: CLI `sync send` without `--relay` uses vault relays
  - Desktop + extension build

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "1.2", "1.3", "1.4"] },
    { "id": 1, "tasks": ["1.5"] },
    { "id": 2, "tasks": ["3.1", "3.2", "3.3", "3.4", "3.5", "3.6", "3.7"] },
    { "id": 3, "tasks": ["5.1", "5.2", "6.1"] },
    { "id": 4, "tasks": ["7.1"] },
    { "id": 5, "tasks": ["8.1", "8.2", "8.3"] },
    { "id": 6, "tasks": ["9.1", "9.2"] }
  ]
}
```

## Notes

- Default relays chosen for reliability and geographic diversity.
- Settings are inside the encrypted vault payload — no separate config file needed.
- The `--relay` CLI override is preserved for debugging/testing against specific relays.
- Relay order matters: the sync engine uses the first responding relay for subscriptions, but publishes to all enabled relays for redundancy.
- No relay authentication is needed (Nostr relays are permissionless). The relay URL is not a secret.
