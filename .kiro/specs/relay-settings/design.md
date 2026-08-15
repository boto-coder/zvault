# Design Document: Nostr Relay Settings

## Overview

Add a relay configuration layer to ZVault that provides sensible defaults, persists relay settings inside the encrypted vault (so they sync across devices), and exposes management through CLI, desktop UI, and extension UI.

## Architecture

```
┌──────────────────────────────────────────────────┐
│              zvault-core                          │
│                                                  │
│  ┌────────────────────┐  ┌───────────────────┐  │
│  │  VaultSettings     │  │  DEFAULT_RELAYS   │  │
│  │  - relays: Vec<RE> │  │  (const array)    │  │
│  └────────────────────┘  └───────────────────┘  │
│                                                  │
│  ┌────────────────────────────────────────────┐  │
│  │  relay_settings.rs                         │  │
│  │  - validate_relay_url()                    │  │
│  │  - normalize_relay_url()                   │  │
│  │  - add_relay() / remove_relay() / toggle() │  │
│  └────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────┘
         │                    │
         ▼                    ▼
┌─────────────────┐  ┌────────────────────┐
│  CLI commands   │  │  Tauri + Extension │
│  relay list/add │  │  settings UI       │
│  /remove/reset  │  │                    │
└─────────────────┘  └────────────────────┘
```

## Components and Interfaces

### Component 1: Default Relays (`crates/zvault-core/src/settings.rs`)

```rust
/// Default relays included with every new vault.
pub const DEFAULT_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.nostr.band",
    "wss://relay.primal.net",
];
```

### Component 2: Data Model

```rust
/// Vault-level settings (stored encrypted inside the vault payload).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSettings {
    #[serde(default = "default_relays")]
    pub relays: Vec<RelayEntry>,
}

impl Default for VaultSettings {
    fn default() -> Self {
        Self {
            relays: default_relays(),
        }
    }
}

/// A single relay configuration entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayEntry {
    pub url: String,
    pub enabled: bool,
    pub added_at: DateTime<Utc>,
}

fn default_relays() -> Vec<RelayEntry> {
    let now = Utc::now();
    DEFAULT_RELAYS.iter().map(|url| RelayEntry {
        url: url.to_string(),
        enabled: true,
        added_at: now,
    }).collect()
}
```

### Component 3: Vault Integration

Add to `Vault` struct:

```rust
pub struct Vault {
    // ... existing fields ...
    #[serde(default)]
    pub settings: VaultSettings,
}
```

The `#[serde(default)]` attribute ensures backward compatibility — existing vault files without a `settings` field will deserialize with `VaultSettings::default()` (which includes the default relays).

### Component 4: Relay URL Validation and Operations

```rust
/// Validate and normalize a relay URL.
/// Returns the normalized URL or an error.
pub fn validate_relay_url(url: &str) -> Result<String>;

/// Add a relay to the settings. Returns error if duplicate or invalid.
pub fn add_relay(settings: &mut VaultSettings, url: &str) -> Result<()>;

/// Remove a relay by URL (normalized comparison).
pub fn remove_relay(settings: &mut VaultSettings, url: &str) -> Result<()>;

/// Enable/disable a relay by URL.
pub fn set_relay_enabled(settings: &mut VaultSettings, url: &str, enabled: bool) -> Result<()>;

/// Reset relays to defaults.
pub fn reset_relays(settings: &mut VaultSettings);

/// Get all enabled relay URLs.
pub fn enabled_relay_urls(settings: &VaultSettings) -> Vec<&str>;
```

**Normalization rules:**
1. Lowercase scheme + host
2. Remove trailing slashes
3. Preserve path/port if present

**Validation rules:**
1. Scheme must be `wss://` or `ws://`
2. Host must be non-empty and valid
3. No duplicate (compared after normalization)

### Component 5: Tauri Commands

```rust
#[tauri::command]
fn get_relay_settings(state: State<'_, AppState>) -> Result<Vec<RelayEntryDto>, String>;

#[tauri::command]
fn add_relay(url: String, state: State<'_, AppState>) -> Result<Vec<RelayEntryDto>, String>;

#[tauri::command]
fn remove_relay(url: String, state: State<'_, AppState>) -> Result<Vec<RelayEntryDto>, String>;

#[tauri::command]
fn toggle_relay(url: String, enabled: bool, state: State<'_, AppState>) -> Result<Vec<RelayEntryDto>, String>;

#[tauri::command]
fn reset_relays(state: State<'_, AppState>) -> Result<Vec<RelayEntryDto>, String>;
```

All relay commands return the full updated relay list so the UI can re-render without a separate fetch.

### Component 6: CLI Subcommands

```rust
#[derive(Subcommand, Debug)]
enum RelayAction {
    /// List configured relays.
    List {
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },
    /// Add a relay.
    Add {
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        #[arg(long)]
        url: String,
    },
    /// Remove a relay.
    Remove {
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        #[arg(long)]
        url: String,
    },
    /// Enable a relay.
    Enable {
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        #[arg(long)]
        url: String,
    },
    /// Disable a relay (keeps in list but won't be used for sync).
    Disable {
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        #[arg(long)]
        url: String,
    },
    /// Reset relay list to defaults.
    Reset {
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },
}
```

### Component 7: Sync Engine Integration

Update the sync send/receive flow:

```rust
// Before (CLI):
fn cmd_sync_send(vault_path, relay_url, recipient_pubkey) { ... }

// After (CLI):
fn cmd_sync_send(vault_path, relay_url: Option<String>, recipient_pubkey) {
    // If --relay provided, use it (override)
    // Otherwise, use vault.settings.relays (enabled ones)
    let relay_urls = match relay_url {
        Some(url) => vec![url],
        None => enabled_relay_urls(&vault.settings).map(String::from).collect(),
    };

    if relay_urls.is_empty() {
        bail!("No active relays configured. Add a relay with `zvault relay add`.");
    }

    // Connect to each relay and publish
    for url in &relay_urls {
        match RelayClient::connect(url).await {
            Ok(client) => { client.publish(&event).await?; }
            Err(e) => { tracing::warn!("Failed to connect to {url}: {e}"); }
        }
    }
}
```

### Component 8: React UI — Relay Settings Section

**Location:** Settings page (desktop) / Settings view (extension)

**Layout:**

```
┌──────────────────────────────────────────────┐
│  Nostr Relays                    [Add Relay] │
│                                              │
│  ┌────────────────────────────────────────┐  │
│  │ ✅ wss://relay.damus.io    [Remove]   │  │
│  │ ✅ wss://nos.lol           [Remove]   │  │
│  │ ✅ wss://relay.nostr.band  [Remove]   │  │
│  │ ✅ wss://relay.primal.net  [Remove]   │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  [Reset to Defaults]                         │
│                                              │
│  ⚠️ (shown if all disabled)                  │
│  Sync is disabled — no active relays.        │
└──────────────────────────────────────────────┘
```

Each row has:
- Toggle switch (enabled/disabled)
- URL text
- Remove button (trash icon)

"Add Relay" opens an inline input with URL validation.

## Data Flow: Adding a Relay

```
User types URL in UI
    → Frontend validates format (client-side quick check)
    → invoke("add_relay", { url })
    → Tauri command:
        1. validate_relay_url(url) → normalized URL
        2. Check for duplicates
        3. Push RelayEntry { url, enabled: true, added_at: now }
        4. Save vault (atomic write)
        5. Return updated relay list
    → Frontend updates relay list display
```

## Error Handling

| Condition | Response |
|-----------|----------|
| Invalid URL scheme (not ws/wss) | "Relay URL must start with wss:// or ws://" |
| Empty/invalid host | "Invalid relay URL: hostname is required" |
| Duplicate relay | "Relay already exists: <url>" |
| Relay not found (remove/toggle) | "Relay not found: <url>" |
| Vault locked | "Vault is locked" |
| No enabled relays on sync | "No active relays configured. Add relays in Settings." |
| Relay connection failure | Warning log, continue with other relays |

## Security Considerations

1. **Relay URLs are non-sensitive:** They are part of the Nostr network's public infrastructure. However, they ARE stored inside the encrypted vault because they're part of VaultSettings.
2. **No relay-side authentication:** Nostr relays don't authenticate clients. The relay URL itself is not a secret.
3. **WSS preferred:** The validation accepts `ws://` for local development but the default relays all use `wss://` (TLS).
4. **Relay metadata leakage:** A relay operator can see which pubkeys are publishing/subscribing (this is inherent to Nostr, not a ZVault issue). NIP-59 gift-wrap hides the actual sender/recipient.

## Dependencies

No new crate dependencies. Uses existing `url` parsing via standard library or simple string operations.

## Testing Strategy

- **Unit:** URL validation (valid wss://, ws://, reject http://, empty, spaces)
- **Unit:** Normalization (trailing slash, case, preserves path)
- **Unit:** add/remove/toggle operations on VaultSettings
- **Unit:** `enabled_relay_urls` returns only enabled entries
- **Integration:** CLI `relay list/add/remove/reset` with a test vault
- **Backward compat:** Open a vault file without `settings` field → default relays applied
- **Sync integration:** `sync send` without `--relay` uses vault settings
