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
use zeroize::Zeroize;

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

    Ok(session.vault.list_items().iter().map(ItemSummary::from).collect())
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

    session
        .vault
        .delete_item(uuid)
        .map_err(|e| e.to_string())?;
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
            rekey_vault,
            generate_totp,
            validate_totp_secret,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
