# Requirements Document: Nostr Relay Settings

## Introduction

ZVault uses Nostr relays as the transport layer for vault sync between devices. Currently, the relay URL must be manually specified on every sync command (CLI) and is not wired up at all in the desktop app or extension. This feature adds:

1. Sensible default relays that work out-of-the-box
2. A relay settings model persisted in the vault (synced across devices)
3. Settings UI in desktop and extension for managing relays
4. CLI subcommands for adding/removing relays
5. Automatic relay connection in the sync engine

## Glossary

- **Relay**: A Nostr relay server that accepts and forwards events over WebSocket (WSS).
- **Relay List**: The ordered list of relay URLs stored in the vault settings.
- **Default Relays**: Built-in relay URLs used when no custom relays are configured.
- **Active Relay**: A relay the sync engine will connect to and use for publishing/subscribing.
- **Settings**: User-configurable vault-level options stored inside the vault's encrypted payload.

## Requirements

### Requirement 1: Default Relays

**User Story:** As a new user, I want ZVault to work with relays out-of-the-box without me needing to find and configure relay URLs manually.

#### Acceptance Criteria

1. THE system SHALL include a hardcoded list of default relays:
   - `wss://relay.damus.io`
   - `wss://nos.lol`
   - `wss://relay.nostr.band`
   - `wss://relay.primal.net`
2. WHEN a vault is created with no custom relay configuration, THE default relays SHALL be used.
3. THE default relay list SHALL be defined as a constant in `zvault-core` so all platforms share the same defaults.
4. THE user SHALL be able to remove default relays if they don't want to use them.
5. IF the user removes all relays and adds none, THE system SHALL warn that sync is disabled.

### Requirement 2: Vault-Level Relay Settings

**User Story:** As a user with multiple devices, I want my relay configuration to sync across devices so I don't have to configure each device separately.

#### Acceptance Criteria

1. THE vault data model SHALL include a `settings` section containing a relay list.
2. THE relay list SHALL be an ordered Vec of relay entries, each with: `url` (String), `enabled` (bool), `added_at` (timestamp).
3. THE relay settings SHALL be encrypted at rest as part of the vault payload (same protection as vault items).
4. WHEN a vault is synced to another device, THE relay settings SHALL be included in the sync payload.
5. THE settings section SHALL be extensible (future settings can be added without migration).
6. IF an existing vault is opened that has no `settings` field, THE system SHALL initialize it with the default relays (backward-compatible migration).

### Requirement 3: CLI Relay Management

**User Story:** As a CLI user, I want subcommands to list, add, and remove relays.

#### Acceptance Criteria

1. `zvault relay list --vault <path>` SHALL print all configured relays with their enabled/disabled status.
2. `zvault relay add --vault <path> --url <wss://...>` SHALL add a relay to the vault's relay list.
3. `zvault relay remove --vault <path> --url <wss://...>` SHALL remove a relay from the list.
4. `zvault relay enable --vault <path> --url <wss://...>` SHALL set enabled=true.
5. `zvault relay disable --vault <path> --url <wss://...>` SHALL set enabled=false.
6. `zvault relay reset --vault <path>` SHALL reset the relay list to defaults.
7. ALL relay management commands SHALL require the vault password (the vault is encrypted).
8. ALL relay management commands SHALL save the vault after modification.
9. THE `sync send` and `sync receive` CLI commands SHALL use the vault's relay list if `--relay` is not explicitly provided. If `--relay` is provided, it overrides the stored list for that invocation only.

### Requirement 4: Desktop UI — Relay Settings

**User Story:** As a desktop user, I want a settings screen where I can manage my relays.

#### Acceptance Criteria

1. THE Settings page SHALL include a "Nostr Relays" section showing all configured relays.
2. EACH relay entry SHALL show: URL, enabled/disabled toggle, remove button.
3. THE UI SHALL provide an "Add Relay" input that validates the URL format (must start with `wss://` or `ws://`).
4. THE UI SHALL provide a "Reset to Defaults" button that restores the default relay list.
5. CHANGES SHALL be saved to the vault immediately on modification (no separate "Save" button needed).
6. THE UI SHALL show a warning banner if all relays are disabled: "Sync is disabled — no active relays configured."
7. THE relay list SHALL be reorderable (drag-and-drop or up/down arrows) to set priority.

### Requirement 5: Extension UI — Relay Settings

**User Story:** As an extension user, I want to manage relays from the extension popup.

#### Acceptance Criteria

1. THE extension settings view SHALL include a "Nostr Relays" section matching the desktop UI.
2. THE same add/remove/enable/disable/reset operations SHALL be available.
3. CHANGES SHALL be persisted to the encrypted vault in browser.storage.local.

### Requirement 6: Relay URL Validation

**User Story:** As a user, I want the system to reject invalid relay URLs before I save them.

#### Acceptance Criteria

1. Relay URLs MUST start with `wss://` or `ws://` (WebSocket schemes only).
2. Relay URLs MUST have a valid hostname (no empty host, no spaces).
3. Relay URLs MUST NOT be duplicates of existing entries (case-insensitive comparison, trailing slash normalized).
4. IF a URL fails validation, THE system SHALL show a specific error message explaining why.
5. THE system SHALL normalize URLs by: lowercasing the scheme+host, removing trailing slashes.

### Requirement 7: Sync Engine Integration

**User Story:** As a user, I want the sync engine to automatically use my configured relays without me specifying them each time.

#### Acceptance Criteria

1. WHEN sync is triggered (manual or automatic), THE sync engine SHALL connect to ALL enabled relays and attempt to publish/subscribe on each.
2. IF a relay connection fails, THE sync engine SHALL continue with the remaining relays (best-effort, not all-or-nothing).
3. THE sync engine SHALL log warnings for failed relay connections but not block the overall sync operation.
4. IF no enabled relays are configured, THE sync engine SHALL return an error: "No active relays configured."
5. THE sync engine SHALL try relays in order (first in list = highest priority for subscription).

### Requirement 8: Data Model

**User Story:** As a developer, I want a clean data model for relay settings that integrates with the existing vault.

#### Acceptance Criteria

1. THE `Vault` struct SHALL include an optional `settings: Option<VaultSettings>` field.
2. `VaultSettings` SHALL contain `relays: Vec<RelayEntry>`.
3. `RelayEntry` SHALL contain: `url: String`, `enabled: bool`, `added_at: DateTime<Utc>`.
4. THE `VaultSettings` struct SHALL derive `Serialize, Deserialize, Clone, Default`.
5. `VaultSettings::default()` SHALL return settings with the default relay list pre-populated.
6. THE settings field SHALL use `#[serde(default)]` for backward compatibility with existing vaults.
