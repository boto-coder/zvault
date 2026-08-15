//! ZVault desktop — Tauri v2 backend.
//!
//! Provides IPC commands that the React frontend calls to manage the vault.
//! Session state (VaultFile, VaultKey, Vault) is held in a `Mutex<Option<…>>`
//! inside Tauri's managed state.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use zvault_core::crypto::VaultKey;
use zvault_core::vault::{ItemKind, Vault, VaultFile, VaultItem};

// ─── Session state ───────────────────────────────────────────────────────────

/// The unlocked vault session. Holds the file handle, derived key, and
/// decrypted vault.
struct VaultSession {
    vault_file: VaultFile,
    key: VaultKey,
    vault: Vault,
}

/// Application state managed by Tauri.
struct AppState {
    session: Mutex<Option<VaultSession>>,
}

// ─── DTOs for the frontend ───────────────────────────────────────────────────

/// Summary of a vault item (no sensitive fields).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ItemSummary {
    id: String,
    kind: String,
    name: String,
    username: Option<String>,
    favourite: bool,
    created_at: String,
    updated_at: String,
}

impl From<&VaultItem> for ItemSummary {
    fn from(item: &VaultItem) -> Self {
        Self {
            id: item.id.to_string(),
            kind: format!("{:?}", item.kind).to_lowercase(),
            name: item.name.clone(),
            username: item.username.clone(),
            favourite: item.favourite,
            created_at: item.created_at.to_rfc3339(),
            updated_at: item.updated_at.to_rfc3339(),
        }
    }
}

/// Full item detail (includes sensitive fields).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ItemDetail {
    id: String,
    kind: String,
    name: String,
    username: Option<String>,
    password: Option<String>,
    totp_secret: Option<String>,
    uris: Vec<UriDto>,
    note: Option<String>,
    card_number: Option<String>,
    expiry: Option<String>,
    cvv: Option<String>,
    cardholder: Option<String>,
    favourite: bool,
    created_at: String,
    updated_at: String,
}

/// URI DTO for the frontend.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UriDto {
    uri: String,
    r#match: String,
}

impl From<&VaultItem> for ItemDetail {
    fn from(item: &VaultItem) -> Self {
        Self {
            id: item.id.to_string(),
            kind: format!("{:?}", item.kind).to_lowercase(),
            name: item.name.clone(),
            username: item.username.clone(),
            password: item.password.clone(),
            totp_secret: item.totp_secret.clone(),
            uris: item
                .uris
                .iter()
                .map(|u| UriDto {
                    uri: u.uri.clone(),
                    r#match: format!("{:?}", u.r#match).to_lowercase(),
                })
                .collect(),
            note: item.note.clone(),
            card_number: item.card_number.clone(),
            expiry: item.expiry.clone(),
            cvv: item.cvv.clone(),
            cardholder: item.cardholder.clone(),
            favourite: item.favourite,
            created_at: item.created_at.to_rfc3339(),
            updated_at: item.updated_at.to_rfc3339(),
        }
    }
}

/// Input for creating/updating an item.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemInput {
    /// If present, update existing item; if absent, create new.
    id: Option<String>,
    kind: String,
    name: String,
    username: Option<String>,
    password: Option<String>,
    totp_secret: Option<String>,
    uris: Option<Vec<UriDto>>,
    note: Option<String>,
    card_number: Option<String>,
    expiry: Option<String>,
    cvv: Option<String>,
    cardholder: Option<String>,
    favourite: Option<bool>,
}

/// Device summary for the frontend.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceSummary {
    device_id: String,
    label: String,
    nostr_pubkey: String,
    added_at: String,
    revoked: bool,
}

// ─── Helper functions ────────────────────────────────────────────────────────

fn parse_item_kind(kind: &str) -> Result<ItemKind, String> {
    match kind {
        "login" => Ok(ItemKind::Login),
        "secure_note" | "securenote" => Ok(ItemKind::SecureNote),
        "card" => Ok(ItemKind::Card),
        "identity" => Ok(ItemKind::Identity),
        other => Err(format!("Unknown item kind: {other}")),
    }
}

fn build_vault_item(input: &ItemInput) -> Result<VaultItem, String> {
    let kind = parse_item_kind(&input.kind)?;
    let mut item = VaultItem::new(kind, &input.name);
    item.username = input.username.clone();
    item.password = input.password.clone();
    item.totp_secret = input.totp_secret.clone();
    item.note = input.note.clone();
    item.card_number = input.card_number.clone();
    item.expiry = input.expiry.clone();
    item.cvv = input.cvv.clone();
    item.cardholder = input.cardholder.clone();
    item.favourite = input.favourite.unwrap_or(false);

    if let Some(uris) = &input.uris {
        item.uris = uris
            .iter()
            .map(|u| zvault_core::vault::Uri {
                uri: u.uri.clone(),
                r#match: match u.r#match.as_str() {
                    "host" => zvault_core::vault::UriMatch::Host,
                    "startswith" | "starts_with" => zvault_core::vault::UriMatch::StartsWith,
                    "exact" => zvault_core::vault::UriMatch::Exact,
                    "regex" => zvault_core::vault::UriMatch::Regex,
                    "never" => zvault_core::vault::UriMatch::Never,
                    _ => zvault_core::vault::UriMatch::Domain,
                },
            })
            .collect();
    }

    Ok(item)
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

/// Create a new vault file at the given path.
#[tauri::command]
fn create_vault(
    mut password: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let result = VaultFile::create(&password, &path).map_err(|e| e.to_string());
    password.zeroize();
    let (vault_file, key) = result?;
    let vault = Vault::new();

    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    *session = Some(VaultSession {
        vault_file,
        key,
        vault,
    });

    Ok(())
}

/// Open and decrypt an existing vault file.
#[tauri::command]
fn open_vault(
    mut password: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let result = VaultFile::open(&password, &path).map_err(|e| e.to_string());
    password.zeroize();
    let (vault_file, key, vault) = result?;

    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    *session = Some(VaultSession {
        vault_file,
        key,
        vault,
    });

    Ok(())
}

/// Lock the vault — drops the key and vault data from memory.
#[tauri::command]
fn lock_vault(state: State<'_, AppState>) -> Result<(), String> {
    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    // Dropping the session zeroes the VaultKey (Zeroizing<[u8; 32]>).
    *session = None;
    Ok(())
}

/// List all items (summaries only — no sensitive fields).
#[tauri::command]
fn list_items(state: State<'_, AppState>) -> Result<Vec<ItemSummary>, String> {
    let session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_ref().ok_or("Vault is locked")?;

    Ok(session
        .vault
        .list_items()
        .iter()
        .map(ItemSummary::from)
        .collect())
}

/// Get full item detail by ID.
#[tauri::command]
fn get_item(id: String, state: State<'_, AppState>) -> Result<ItemDetail, String> {
    let session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_ref().ok_or("Vault is locked")?;

    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let item = session
        .vault
        .get_item(uuid)
        .ok_or_else(|| format!("Item not found: {id}"))?;

    Ok(ItemDetail::from(item))
}

/// Add a new item to the vault and save.
#[tauri::command]
fn add_item(item_json: String, state: State<'_, AppState>) -> Result<ItemSummary, String> {
    let input: ItemInput =
        serde_json::from_str(&item_json).map_err(|e| format!("Invalid item JSON: {e}"))?;

    let item = build_vault_item(&input)?;
    let summary = ItemSummary::from(&item);

    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_mut().ok_or("Vault is locked")?;

    session.vault.add_item(item);
    session
        .vault_file
        .save(&session.key, &session.vault)
        .map_err(|e| e.to_string())?;

    Ok(summary)
}

/// Update an existing item in the vault and save.
#[tauri::command]
fn update_item(item_json: String, state: State<'_, AppState>) -> Result<ItemDetail, String> {
    let input: ItemInput =
        serde_json::from_str(&item_json).map_err(|e| format!("Invalid item JSON: {e}"))?;

    let id_str = input.id.as_ref().ok_or("Missing item id for update")?;
    let uuid = Uuid::parse_str(id_str).map_err(|e| e.to_string())?;

    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_mut().ok_or("Vault is locked")?;

    // Get existing item to preserve fields not in input
    let existing = session
        .vault
        .get_item(uuid)
        .ok_or_else(|| format!("Item not found: {id_str}"))?
        .clone();

    let kind = parse_item_kind(&input.kind)?;
    let mut updated = VaultItem::new(kind, &input.name);
    updated.id = existing.id;
    updated.created_at = existing.created_at;
    updated.username = input.username;
    updated.password = input.password;
    updated.totp_secret = input.totp_secret;
    updated.note = input.note;
    updated.card_number = input.card_number;
    updated.expiry = input.expiry;
    updated.cvv = input.cvv;
    updated.cardholder = input.cardholder;
    updated.favourite = input.favourite.unwrap_or(existing.favourite);

    if let Some(uris) = &input.uris {
        updated.uris = uris
            .iter()
            .map(|u| zvault_core::vault::Uri {
                uri: u.uri.clone(),
                r#match: match u.r#match.as_str() {
                    "host" => zvault_core::vault::UriMatch::Host,
                    "startswith" | "starts_with" => zvault_core::vault::UriMatch::StartsWith,
                    "exact" => zvault_core::vault::UriMatch::Exact,
                    "regex" => zvault_core::vault::UriMatch::Regex,
                    "never" => zvault_core::vault::UriMatch::Never,
                    _ => zvault_core::vault::UriMatch::Domain,
                },
            })
            .collect();
    }

    let detail = ItemDetail::from(&updated);

    session
        .vault
        .update_item(updated)
        .map_err(|e| e.to_string())?;
    session
        .vault_file
        .save(&session.key, &session.vault)
        .map_err(|e| e.to_string())?;

    Ok(detail)
}

/// Delete an item from the vault and save.
#[tauri::command]
fn delete_item(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_mut().ok_or("Vault is locked")?;

    session.vault.delete_item(uuid).map_err(|e| e.to_string())?;
    session
        .vault_file
        .save(&session.key, &session.vault)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// List all devices in the vault.
#[tauri::command]
fn list_devices(state: State<'_, AppState>) -> Result<Vec<DeviceSummary>, String> {
    let session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_ref().ok_or("Vault is locked")?;

    Ok(session
        .vault
        .devices
        .iter()
        .map(|d| DeviceSummary {
            device_id: d.device_id.to_string(),
            label: d.label.clone(),
            nostr_pubkey: d.nostr_pubkey.clone(),
            added_at: d.added_at.to_rfc3339(),
            revoked: d.revoked,
        })
        .collect())
}

/// Generate a random password with all 4 character classes.
#[tauri::command]
fn generate_password(length: Option<u32>) -> Result<String, String> {
    let len = length.unwrap_or(20) as usize;
    if len < 4 {
        return Err("Minimum password length is 4".into());
    }

    use aes_gcm::aead::rand_core::RngCore as _;
    use aes_gcm::aead::OsRng as AeadOsRng;

    const UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
    const DIGITS: &[u8] = b"0123456789";
    const SPECIAL: &[u8] = b"!@#$%^&*()_+-=[]{}|;:,.<>?";
    const ALL: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()_+-=[]{}|;:,.<>?";

    let mut password = Vec::with_capacity(len);
    let mut random_bytes = vec![0u8; len + 4];
    AeadOsRng.fill_bytes(&mut random_bytes);

    // Guarantee one from each class
    password.push(UPPER[(random_bytes[0] as usize) % UPPER.len()]);
    password.push(LOWER[(random_bytes[1] as usize) % LOWER.len()]);
    password.push(DIGITS[(random_bytes[2] as usize) % DIGITS.len()]);
    password.push(SPECIAL[(random_bytes[3] as usize) % SPECIAL.len()]);

    // Fill remaining
    for i in 4..len {
        password.push(ALL[(random_bytes[i] as usize) % ALL.len()]);
    }

    // Fisher-Yates shuffle
    let mut shuffle_bytes = vec![0u8; len];
    AeadOsRng.fill_bytes(&mut shuffle_bytes);
    for i in (1..len).rev() {
        let j = (shuffle_bytes[i] as usize) % (i + 1);
        password.swap(i, j);
    }

    Ok(String::from_utf8(password).expect("invariant: all chars are ASCII"))
}

/// Admit a new device to the vault's trust group.
#[tauri::command]
fn admit_device(
    pubkey_hex: String,
    label: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    if pubkey_hex.len() != 64 || !pubkey_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid public key: must be 64 hex characters".into());
    }
    if label.trim().is_empty() {
        return Err("Device label is required".into());
    }

    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_mut().ok_or("Vault is locked")?;

    let device_id = Uuid::new_v4();
    let added_by = session
        .vault
        .devices
        .first()
        .map(|d| d.device_id)
        .unwrap_or(device_id);

    let entry = zvault_core::vault::DeviceEntry {
        device_id,
        nostr_pubkey: pubkey_hex.to_lowercase(),
        label: label.trim().to_string(),
        added_at: chrono::Utc::now(),
        added_by,
        revoked: false,
        revoked_at: None,
        revoked_by: None,
    };
    session.vault.devices.push(entry);
    session.vault.version += 1;
    session.vault.updated_at = chrono::Utc::now();
    session
        .vault_file
        .save(&session.key, &session.vault)
        .map_err(|e| e.to_string())?;

    Ok(device_id.to_string())
}

/// Revoke a device from the vault's trust group.
#[tauri::command]
fn revoke_device(device_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let uuid = Uuid::parse_str(&device_id).map_err(|e| e.to_string())?;

    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_mut().ok_or("Vault is locked")?;

    let entry = session
        .vault
        .devices
        .iter_mut()
        .find(|d| d.device_id == uuid)
        .ok_or("Device not found")?;

    if entry.revoked {
        return Err("Device already revoked".into());
    }
    entry.revoked = true;
    entry.revoked_at = Some(chrono::Utc::now());
    session.vault.version += 1;
    session.vault.updated_at = chrono::Utc::now();
    session
        .vault_file
        .save(&session.key, &session.vault)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Change the vault master password.
#[tauri::command]
fn rekey_vault(
    mut old_password: String,
    mut new_password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    let current = session.as_ref().ok_or("Vault is locked")?;

    let result = current
        .vault_file
        .rekey(&old_password, &new_password)
        .map_err(|e| e.to_string());
    old_password.zeroize();
    new_password.zeroize();
    let (new_vf, new_key, vault) = result?;

    *session = Some(VaultSession {
        vault_file: new_vf,
        key: new_key,
        vault,
    });

    Ok(())
}

// ─── Relay settings commands ─────────────────────────────────────────────────

/// Relay entry DTO for the frontend.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RelayEntryDto {
    url: String,
    enabled: bool,
    added_at: String,
}

/// Get the current relay settings from the vault.
#[tauri::command]
fn get_relay_settings(state: State<'_, AppState>) -> Result<Vec<RelayEntryDto>, String> {
    let session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_ref().ok_or("Vault is locked")?;

    Ok(session
        .vault
        .settings
        .relays
        .iter()
        .map(|r| RelayEntryDto {
            url: r.url.clone(),
            enabled: r.enabled,
            added_at: r.added_at.to_rfc3339(),
        })
        .collect())
}

/// Add a relay to the vault settings.
#[tauri::command]
fn add_relay(url: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_mut().ok_or("Vault is locked")?;

    zvault_core::settings::add_relay(&mut session.vault.settings, &url)?;
    session.vault.version += 1;
    session.vault.updated_at = chrono::Utc::now();
    session
        .vault_file
        .save(&session.key, &session.vault)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Remove a relay from the vault settings.
#[tauri::command]
fn remove_relay(url: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_mut().ok_or("Vault is locked")?;

    zvault_core::settings::remove_relay(&mut session.vault.settings, &url)?;
    session.vault.version += 1;
    session.vault.updated_at = chrono::Utc::now();
    session
        .vault_file
        .save(&session.key, &session.vault)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Toggle a relay's enabled state.
#[tauri::command]
fn toggle_relay(url: String, enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_mut().ok_or("Vault is locked")?;

    zvault_core::settings::set_relay_enabled(&mut session.vault.settings, &url, enabled)?;
    session.vault.version += 1;
    session.vault.updated_at = chrono::Utc::now();
    session
        .vault_file
        .save(&session.key, &session.vault)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Reset relays to the default list.
#[tauri::command]
fn reset_relays(state: State<'_, AppState>) -> Result<(), String> {
    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_mut().ok_or("Vault is locked")?;

    zvault_core::settings::reset_relays(&mut session.vault.settings);
    session.vault.version += 1;
    session.vault.updated_at = chrono::Utc::now();
    session
        .vault_file
        .save(&session.key, &session.vault)
        .map_err(|e| e.to_string())?;

    Ok(())
// ─── Device key display/export commands ──────────────────────────────────────

/// Device public key information.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DevicePubkeyInfo {
    device_id: String,
    label: String,
    pubkey_hex: String,
    npub: String,
}

/// Device secret key export result.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceSecretKeyInfo {
    nsec: String,
    hex: String,
}

/// Get this device's public key info (device_id, label, pubkey_hex, npub).
///
/// Requires the vault to be unlocked and a device identity to be present in the
/// vault's device list for this application instance.
#[tauri::command]
fn get_device_pubkey(state: State<'_, AppState>) -> Result<DevicePubkeyInfo, String> {
    let session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_ref().ok_or("Vault is locked")?;

    // Find the first non-revoked device — in a desktop app, this device is
    // typically the first entry or the one whose pubkey matches the keyring.
    // For simplicity, return the first active device as "this device".
    let device = session
        .vault
        .devices
        .iter()
        .find(|d| !d.revoked)
        .ok_or("No active device identity found")?;

    // Encode the pubkey as npub
    let pubkey_bytes =
        hex::decode(&device.nostr_pubkey).map_err(|e| format!("Invalid pubkey hex: {e}"))?;
    let pubkey_array: [u8; 32] = pubkey_bytes
        .try_into()
        .map_err(|_| "Public key is not 32 bytes".to_string())?;
    let npub = zvault_core::nip19::encode_npub(&pubkey_array);

    Ok(DevicePubkeyInfo {
        device_id: device.device_id.to_string(),
        label: device.label.clone(),
        pubkey_hex: device.nostr_pubkey.clone(),
        npub,
    })
}

/// Export this device's secret key. Requires password re-verification.
///
/// Returns the nsec (bech32) and hex-encoded secret key.
#[tauri::command]
fn export_device_secret_key(
    mut password: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<DeviceSecretKeyInfo, String> {
    // Re-verify the password by trying to open the vault
    let verify_result = VaultFile::open(&password, &path).map_err(|e| e.to_string());
    password.zeroize();
    let (_vf, key, _vault) = verify_result?;

    // Check session is active
    let session = state.session.lock().map_err(|e| e.to_string())?;
    let _session = session.as_ref().ok_or("Vault is locked")?;

    // Load secret key from the device sidecar file
    let sidecar_path = {
        let mut p = std::ffi::OsString::from(&path);
        p.push(".device");
        std::path::PathBuf::from(p)
    };

    if !sidecar_path.exists() {
        return Err("Device identity not initialised. No .device sidecar file found.".into());
    }

    let blob =
        std::fs::read(&sidecar_path).map_err(|e| format!("Failed to read device sidecar: {e}"))?;
    let plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(
        zvault_core::crypto::decrypt(&key, &blob)
            .map_err(|e| format!("Failed to decrypt device sidecar: {e}"))?,
    );

    #[derive(serde::Deserialize)]
    struct DeviceFile {
        secret_key_hex: String,
    }
    let device_file: DeviceFile = serde_json::from_slice(&plaintext)
        .map_err(|e| format!("Failed to parse device sidecar: {e}"))?;

    let sk_bytes = hex::decode(&device_file.secret_key_hex)
        .map_err(|e| format!("Invalid secret key hex: {e}"))?;
    let sk_array: [u8; 32] = sk_bytes
        .try_into()
        .map_err(|_| "Secret key is not 32 bytes".to_string())?;
    let nsec = zvault_core::nip19::encode_nsec(&sk_array);

    Ok(DeviceSecretKeyInfo {
        nsec: (*nsec).clone(),
        hex: device_file.secret_key_hex,
    })
}

// ─── TOTP commands ───────────────────────────────────────────────────────────

/// Response from the TOTP generation command.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TotpResponse {
    code: String,
    remaining_seconds: u32,
}

/// Generate a TOTP code from a base32 secret.
#[tauri::command]
fn generate_totp(secret: String) -> Result<TotpResponse, String> {
    use totp_rs::{Algorithm, TOTP};

    let secret_bytes = secret.as_bytes().to_vec();
    let totp = TOTP::new(Algorithm::SHA1, 6, 1, 30, secret_bytes)
        .map_err(|e| format!("Invalid TOTP secret: {e}"))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("System clock error: {e}"))?
        .as_secs();

    let code = totp.generate(now);
    let remaining = 30 - (now % 30) as u32;

    Ok(TotpResponse {
        code,
        remaining_seconds: remaining,
    })
}

/// Validate a TOTP secret (check that it can produce a valid code).
#[tauri::command]
fn validate_totp_secret(secret: String) -> Result<(), String> {
    use totp_rs::{Algorithm, TOTP};

    let secret_bytes = secret.as_bytes().to_vec();
    TOTP::new(Algorithm::SHA1, 6, 1, 30, secret_bytes)
        .map_err(|e| format!("Invalid TOTP secret: {e}"))?;

    Ok(())
}

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            session: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            create_vault,
            open_vault,
            lock_vault,
            list_items,
            get_item,
            add_item,
            update_item,
            delete_item,
            list_devices,
            generate_password,
            admit_device,
            revoke_device,
            rekey_vault,
            get_relay_settings,
            add_relay,
            remove_relay,
            toggle_relay,
            reset_relays,
            generate_totp,
            validate_totp_secret,
            get_device_pubkey,
            export_device_secret_key,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
