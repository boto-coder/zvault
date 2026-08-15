//! Vault-level settings: relay configuration and future preferences.
//!
//! `VaultSettings` is stored inside the encrypted vault alongside items and
//! devices.  It uses `#[serde(default)]` on the parent `Vault` struct so that
//! existing vault files without a `settings` field deserialise correctly.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Default Nostr relay URLs populated for new vaults.
pub const DEFAULT_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.nostr.band",
];

// ─── Types ───────────────────────────────────────────────────────────────────

/// A single relay entry in the vault settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelayEntry {
    /// Normalised WebSocket URL of the relay.
    pub url: String,
    /// Whether this relay is enabled for sync operations.
    pub enabled: bool,
    /// When this relay was added to the vault settings.
    pub added_at: DateTime<Utc>,
}

/// Vault-level settings stored alongside items and devices.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultSettings {
    /// Configured Nostr relays for sync.
    pub relays: Vec<RelayEntry>,
}

impl Default for VaultSettings {
    /// Creates settings with the default relay list (all enabled).
    fn default() -> Self {
        let now = Utc::now();
        Self {
            relays: DEFAULT_RELAYS
                .iter()
                .map(|url| RelayEntry {
                    url: (*url).to_string(),
                    enabled: true,
                    added_at: now,
                })
                .collect(),
        }
    }
}

// ─── Relay URL validation and normalisation ──────────────────────────────────

/// Validates and normalises a relay URL.
///
/// Requirements:
/// - Scheme must be `ws://` or `wss://`
/// - Host must be non-empty
/// - URL is lowercased (scheme + host) and trailing slashes are stripped
///
/// # Errors
///
/// Returns an error string describing the validation failure.
pub fn validate_relay_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("Relay URL cannot be empty".to_string());
    }
    if trimmed.contains(char::is_whitespace) {
        return Err("Relay URL cannot contain whitespace".to_string());
    }

    // Parse the URL to extract scheme and host.
    let lower = trimmed.to_string();

    // Check scheme.
    let (scheme, rest) = if let Some(r) = lower.strip_prefix("wss://") {
        ("wss", r)
    } else if let Some(r) = lower.strip_prefix("ws://") {
        ("ws", r)
    } else {
        return Err("Relay URL must use ws:// or wss:// scheme".to_string());
    };

    if rest.is_empty() {
        return Err("Relay URL must have a non-empty host".to_string());
    }

    // Extract host (everything before first '/' after scheme).
    let host_and_path = rest;
    let (host_port, path) = match host_and_path.find('/') {
        Some(idx) => (&host_and_path[..idx], &host_and_path[idx..]),
        None => (host_and_path, ""),
    };

    if host_port.is_empty() {
        return Err("Relay URL must have a non-empty host".to_string());
    }

    // Normalise: lowercase scheme + host, strip trailing slashes from path.
    let normalised_host = host_port.to_lowercase();
    let normalised_path = path.trim_end_matches('/');

    let normalised = format!("{scheme}://{normalised_host}{normalised_path}");
    Ok(normalised)
}

// ─── CRUD operations ─────────────────────────────────────────────────────────

/// Adds a relay to the settings after validation and duplicate check.
///
/// # Errors
///
/// Returns an error if the URL is invalid or already present.
pub fn add_relay(settings: &mut VaultSettings, url: &str) -> Result<(), String> {
    let normalised = validate_relay_url(url)?;

    // Check for duplicate.
    if settings.relays.iter().any(|r| r.url == normalised) {
        return Err(format!("Relay already exists: {normalised}"));
    }

    settings.relays.push(RelayEntry {
        url: normalised,
        enabled: true,
        added_at: Utc::now(),
    });
    Ok(())
}

/// Removes a relay from the settings by its URL.
///
/// # Errors
///
/// Returns an error if the relay is not found.
pub fn remove_relay(settings: &mut VaultSettings, url: &str) -> Result<(), String> {
    let normalised = validate_relay_url(url)?;

    let pos = settings
        .relays
        .iter()
        .position(|r| r.url == normalised)
        .ok_or_else(|| format!("Relay not found: {normalised}"))?;

    settings.relays.remove(pos);
    Ok(())
}

/// Sets the enabled/disabled state of a relay.
///
/// # Errors
///
/// Returns an error if the relay is not found.
pub fn set_relay_enabled(
    settings: &mut VaultSettings,
    url: &str,
    enabled: bool,
) -> Result<(), String> {
    let normalised = validate_relay_url(url)?;

    let entry = settings
        .relays
        .iter_mut()
        .find(|r| r.url == normalised)
        .ok_or_else(|| format!("Relay not found: {normalised}"))?;

    entry.enabled = enabled;
    Ok(())
}

/// Resets the relay list to defaults.
pub fn reset_relays(settings: &mut VaultSettings) {
    *settings = VaultSettings::default();
}

/// Returns the URLs of all enabled relays.
#[must_use]
pub fn enabled_relay_urls(settings: &VaultSettings) -> Vec<String> {
    settings
        .relays
        .iter()
        .filter(|r| r.enabled)
        .map(|r| r.url.clone())
        .collect()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_has_three_relays() {
        let settings = VaultSettings::default();
        assert_eq!(settings.relays.len(), 3);
        assert!(settings.relays.iter().all(|r| r.enabled));
    }

    #[test]
    fn default_relays_are_correct() {
        let settings = VaultSettings::default();
        let urls: Vec<&str> = settings.relays.iter().map(|r| r.url.as_str()).collect();
        assert!(urls.contains(&"wss://relay.damus.io"));
        assert!(urls.contains(&"wss://nos.lol"));
        assert!(urls.contains(&"wss://relay.nostr.band"));
    }

    // ── validate_relay_url ───────────────────────────────────────────────────

    #[test]
    fn validate_valid_wss_url() {
        let result = validate_relay_url("wss://relay.damus.io");
        assert_eq!(result.unwrap(), "wss://relay.damus.io");
    }

    #[test]
    fn validate_valid_ws_url() {
        let result = validate_relay_url("ws://localhost:8080");
        assert_eq!(result.unwrap(), "ws://localhost:8080");
    }

    #[test]
    fn validate_strips_trailing_slash() {
        let result = validate_relay_url("wss://relay.damus.io/");
        assert_eq!(result.unwrap(), "wss://relay.damus.io");
    }

    #[test]
    fn validate_strips_multiple_trailing_slashes() {
        let result = validate_relay_url("wss://relay.damus.io///");
        assert_eq!(result.unwrap(), "wss://relay.damus.io");
    }

    #[test]
    fn validate_lowercases_host() {
        let result = validate_relay_url("wss://Relay.Damus.IO");
        assert_eq!(result.unwrap(), "wss://relay.damus.io");
    }

    #[test]
    fn validate_preserves_path() {
        let result = validate_relay_url("wss://relay.example.com/nostr");
        assert_eq!(result.unwrap(), "wss://relay.example.com/nostr");
    }

    #[test]
    fn validate_rejects_empty() {
        assert!(validate_relay_url("").is_err());
    }

    #[test]
    fn validate_rejects_whitespace_only() {
        assert!(validate_relay_url("   ").is_err());
    }

    #[test]
    fn validate_rejects_whitespace_in_url() {
        assert!(validate_relay_url("wss://relay .damus.io").is_err());
    }

    #[test]
    fn validate_rejects_http_scheme() {
        assert!(validate_relay_url("http://relay.damus.io").is_err());
    }

    #[test]
    fn validate_rejects_https_scheme() {
        assert!(validate_relay_url("https://relay.damus.io").is_err());
    }

    #[test]
    fn validate_rejects_no_scheme() {
        assert!(validate_relay_url("relay.damus.io").is_err());
    }

    #[test]
    fn validate_rejects_scheme_only() {
        assert!(validate_relay_url("wss://").is_err());
    }

    #[test]
    fn validate_trims_leading_trailing_whitespace() {
        let result = validate_relay_url("  wss://relay.damus.io  ");
        assert_eq!(result.unwrap(), "wss://relay.damus.io");
    }

    // ── add_relay ────────────────────────────────────────────────────────────

    #[test]
    fn add_relay_success() {
        let mut settings = VaultSettings { relays: vec![] };
        let result = add_relay(&mut settings, "wss://new-relay.example.com");
        assert!(result.is_ok());
        assert_eq!(settings.relays.len(), 1);
        assert_eq!(settings.relays[0].url, "wss://new-relay.example.com");
        assert!(settings.relays[0].enabled);
    }

    #[test]
    fn add_relay_duplicate_rejected() {
        let mut settings = VaultSettings::default();
        let result = add_relay(&mut settings, "wss://relay.damus.io");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }

    #[test]
    fn add_relay_duplicate_normalised() {
        let mut settings = VaultSettings::default();
        // Same relay with trailing slash should be detected as duplicate.
        let result = add_relay(&mut settings, "wss://relay.damus.io/");
        assert!(result.is_err());
    }

    #[test]
    fn add_relay_invalid_url_rejected() {
        let mut settings = VaultSettings::default();
        let result = add_relay(&mut settings, "not a url");
        assert!(result.is_err());
    }

    // ── remove_relay ─────────────────────────────────────────────────────────

    #[test]
    fn remove_relay_success() {
        let mut settings = VaultSettings::default();
        let initial_len = settings.relays.len();
        let result = remove_relay(&mut settings, "wss://relay.damus.io");
        assert!(result.is_ok());
        assert_eq!(settings.relays.len(), initial_len - 1);
    }

    #[test]
    fn remove_relay_not_found() {
        let mut settings = VaultSettings::default();
        let result = remove_relay(&mut settings, "wss://nonexistent.relay.com");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // ── set_relay_enabled ────────────────────────────────────────────────────

    #[test]
    fn set_relay_enabled_disable() {
        let mut settings = VaultSettings::default();
        let result = set_relay_enabled(&mut settings, "wss://relay.damus.io", false);
        assert!(result.is_ok());
        let entry = settings
            .relays
            .iter()
            .find(|r| r.url == "wss://relay.damus.io")
            .unwrap();
        assert!(!entry.enabled);
    }

    #[test]
    fn set_relay_enabled_enable() {
        let mut settings = VaultSettings::default();
        // First disable, then re-enable.
        set_relay_enabled(&mut settings, "wss://relay.damus.io", false).unwrap();
        set_relay_enabled(&mut settings, "wss://relay.damus.io", true).unwrap();
        let entry = settings
            .relays
            .iter()
            .find(|r| r.url == "wss://relay.damus.io")
            .unwrap();
        assert!(entry.enabled);
    }

    #[test]
    fn set_relay_enabled_not_found() {
        let mut settings = VaultSettings::default();
        let result = set_relay_enabled(&mut settings, "wss://no.such.relay", true);
        assert!(result.is_err());
    }

    // ── reset_relays ─────────────────────────────────────────────────────────

    #[test]
    fn reset_relays_restores_defaults() {
        let mut settings = VaultSettings { relays: vec![] };
        reset_relays(&mut settings);
        assert_eq!(settings.relays.len(), 3);
    }

    // ── enabled_relay_urls ───────────────────────────────────────────────────

    #[test]
    fn enabled_relay_urls_all_enabled() {
        let settings = VaultSettings::default();
        let urls = enabled_relay_urls(&settings);
        assert_eq!(urls.len(), 3);
    }

    #[test]
    fn enabled_relay_urls_some_disabled() {
        let mut settings = VaultSettings::default();
        set_relay_enabled(&mut settings, "wss://relay.damus.io", false).unwrap();
        let urls = enabled_relay_urls(&settings);
        assert_eq!(urls.len(), 2);
        assert!(!urls.contains(&"wss://relay.damus.io".to_string()));
    }

    #[test]
    fn enabled_relay_urls_all_disabled() {
        let mut settings = VaultSettings::default();
        for relay in DEFAULT_RELAYS {
            set_relay_enabled(&mut settings, relay, false).unwrap();
        }
        let urls = enabled_relay_urls(&settings);
        assert!(urls.is_empty());
    }

    // ── Backward compatibility ───────────────────────────────────────────────

    #[test]
    fn vault_settings_serialisation_roundtrip() {
        let settings = VaultSettings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let restored: VaultSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(settings.relays.len(), restored.relays.len());
        for (a, b) in settings.relays.iter().zip(restored.relays.iter()) {
            assert_eq!(a.url, b.url);
            assert_eq!(a.enabled, b.enabled);
        }
    }

    #[test]
    fn vault_without_settings_deserialises_with_default() {
        // Simulate a vault JSON from before settings existed.
        let vault_json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "version": 1,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z",
            "items": [],
            "devices": []
        }"#;

        let vault: crate::vault::Vault = serde_json::from_str(vault_json).unwrap();
        // The settings field should get its Default value.
        assert_eq!(vault.settings.relays.len(), 3);
    }
}
