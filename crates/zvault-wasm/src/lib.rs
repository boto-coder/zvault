//! WebAssembly bindings for zvault-core.
//!
//! Exposes a subset of zvault-core functionality to JavaScript via wasm-bindgen.
//! Used by the ZVault browser extension for in-browser vault encryption/decryption.

use serde::Deserialize;
use wasm_bindgen::prelude::*;
use zvault_core::crypto::{decrypt, derive_key, encrypt_with_params, parse_kdf_params, KdfParams};
use zvault_core::vault::{IdentityFields, ItemKind, Uri, UriMatch, Vault, VaultItem};

// ─── AddItemInput ────────────────────────────────────────────────────────────

/// Input struct for adding a new item from the frontend.
///
/// The frontend sends partial JSON (no `id`, `created_at`, or `updated_at`).
/// This struct deserializes that partial payload and constructs a full
/// [`VaultItem`] with generated UUID and current timestamps.
///
/// Supports both camelCase and snake_case field names via serde aliases.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct AddItemInput {
    kind: ItemKind,
    name: String,

    #[serde(default, alias = "folder")]
    folder: Option<String>,

    #[serde(default)]
    favourite: bool,

    // Login fields
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default, alias = "totpSecret")]
    totp_secret: Option<String>,
    #[serde(default)]
    uris: Vec<AddItemUri>,

    // SecureNote fields
    #[serde(default)]
    note: Option<String>,

    // Card fields
    #[serde(default, alias = "cardNumber")]
    card_number: Option<String>,
    #[serde(default)]
    expiry: Option<String>,
    #[serde(default)]
    cvv: Option<String>,
    #[serde(default)]
    cardholder: Option<String>,

    // Identity fields
    #[serde(default)]
    identity: Option<AddItemIdentity>,
}

/// URI input matching the frontend payload.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct AddItemUri {
    uri: String,
    #[serde(default = "default_uri_match", alias = "match")]
    r#match: String,
}

fn default_uri_match() -> String {
    "domain".to_string()
}

/// Identity fields input from the frontend.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct AddItemIdentity {
    #[serde(default, alias = "firstName")]
    first_name: Option<String>,
    #[serde(default, alias = "lastName")]
    last_name: Option<String>,
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

impl AddItemInput {
    /// Convert to a full [`VaultItem`] with generated UUID and current timestamps.
    fn into_vault_item(self) -> VaultItem {
        let mut item = VaultItem::new(self.kind, self.name);
        item.favourite = self.favourite;
        item.username = self.username;
        item.password = self.password;
        item.totp_secret = self.totp_secret;
        item.note = self.note;
        item.card_number = self.card_number;
        item.expiry = self.expiry;
        item.cvv = self.cvv;
        item.cardholder = self.cardholder;

        if let Some(folder_str) = self.folder {
            item.folder = uuid::Uuid::parse_str(&folder_str).ok();
        }

        item.uris = self
            .uris
            .into_iter()
            .map(|u| Uri {
                uri: u.uri,
                r#match: match u.r#match.as_str() {
                    "host" => UriMatch::Host,
                    "starts_with" | "startsWith" => UriMatch::StartsWith,
                    "exact" => UriMatch::Exact,
                    "regex" => UriMatch::Regex,
                    "never" => UriMatch::Never,
                    _ => UriMatch::Domain,
                },
            })
            .collect();

        if let Some(identity) = self.identity {
            item.identity = Some(IdentityFields {
                first_name: identity.first_name,
                last_name: identity.last_name,
                address: identity.address,
                city: identity.city,
                country: identity.country,
                phone: identity.phone,
                email: identity.email,
            });
        }

        item
    }
}

/// Generate a cryptographically random password.
///
/// Guarantees at least one character from each of four classes:
/// uppercase (A-Z), lowercase (a-z), digit (0-9), special.
///
/// `length` defaults to 20 if `None`. Returns an error if length < 4.
#[wasm_bindgen]
pub fn generate_password(length: Option<u32>) -> Result<String, JsValue> {
    generate_password_inner(length).map_err(|e| JsValue::from_str(&e))
}

/// Inner implementation of password generation, testable on all targets.
fn generate_password_inner(length: Option<u32>) -> Result<String, String> {
    let len = length.unwrap_or(20) as usize;
    if len < 4 {
        return Err(
            "Password length must be at least 4 to include all character classes".to_string(),
        );
    }

    const UPPERCASE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const LOWERCASE: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
    const DIGITS: &[u8] = b"0123456789";
    const SPECIAL: &[u8] = b"!@#$%^&*()_+-=[]{}|;:,.<>?";

    let all_chars: Vec<u8> = [UPPERCASE, LOWERCASE, DIGITS, SPECIAL].concat();

    // We need `len` random bytes for character selection + `len * 4` bytes for
    // Fisher-Yates shuffle (using u32 per position).
    let random_bytes_needed = len + len * 4;
    let mut random_buf = vec![0u8; random_bytes_needed];
    getrandom::getrandom(&mut random_buf).map_err(|e| format!("RNG error: {e}"))?;

    let mut password = Vec::with_capacity(len);
    let mut rng_offset = 0;

    // Guarantee one character from each class
    let classes: &[&[u8]] = &[UPPERCASE, LOWERCASE, DIGITS, SPECIAL];
    for class in classes {
        let idx = random_buf[rng_offset] as usize % class.len();
        rng_offset += 1;
        password.push(class[idx]);
    }

    // Fill remaining positions from the combined character set
    for _ in 4..len {
        let idx = random_buf[rng_offset] as usize % all_chars.len();
        rng_offset += 1;
        password.push(all_chars[idx]);
    }

    // Fisher-Yates shuffle using remaining random bytes (4 bytes per position for u32)
    for i in (1..len).rev() {
        let r_bytes = &random_buf[rng_offset..rng_offset + 4];
        rng_offset += 4;
        let r = u32::from_le_bytes([r_bytes[0], r_bytes[1], r_bytes[2], r_bytes[3]]);
        let j = (r as usize) % (i + 1);
        password.swap(i, j);
    }

    String::from_utf8(password).map_err(|e| format!("UTF-8 error: {e}"))
}

/// Create a new empty vault, encrypt it with the given password, and return the
/// encrypted bytes as a `Uint8Array`.
///
/// Returns a JS object: `{ data: Uint8Array }` containing the encrypted vault blob.
#[wasm_bindgen]
pub fn create_vault(password: &str) -> Result<JsValue, JsValue> {
    let vault = Vault::new();
    let json = vault
        .to_json()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    let params = KdfParams::generate();
    let key = derive_key(password, &params).map_err(|e| JsValue::from_str(&e.to_string()))?;

    let blob =
        encrypt_with_params(&key, &json, &params).map_err(|e| JsValue::from_str(&e.to_string()))?;

    Ok(serde_wasm_bindgen::to_value(&blob).unwrap_or(JsValue::NULL))
}

/// Open an encrypted vault blob with the given password.
///
/// Returns the vault contents as a JSON string.
#[wasm_bindgen]
pub fn open_vault(password: &str, data: &[u8]) -> Result<String, JsValue> {
    let params = parse_kdf_params(data).map_err(|e| JsValue::from_str(&e.to_string()))?;

    let key = derive_key(password, &params).map_err(|e| JsValue::from_str(&e.to_string()))?;

    let plaintext = decrypt(&key, data).map_err(|e| JsValue::from_str(&e.to_string()))?;

    String::from_utf8(plaintext).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Encrypt vault JSON with the given password.
///
/// Returns the encrypted blob as bytes (Uint8Array via wasm-bindgen).
#[wasm_bindgen]
pub fn encrypt_vault(password: &str, vault_json: &str) -> Result<Vec<u8>, JsValue> {
    // Validate that the JSON is a valid Vault before encrypting.
    let _vault: Vault = serde_json::from_str(vault_json)
        .map_err(|e| JsValue::from_str(&format!("invalid vault JSON: {e}")))?;

    let params = KdfParams::generate();
    let key = derive_key(password, &params).map_err(|e| JsValue::from_str(&e.to_string()))?;

    encrypt_with_params(&key, vault_json.as_bytes(), &params)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Add a new item to a vault (given as JSON).
///
/// `vault_json` is the current vault state as JSON.
/// `item_json` is a JSON object describing the new item to add. Does not
/// require `id`, `created_at`, or `updated_at` — these are generated
/// automatically.
///
/// Returns the updated vault JSON string.
#[wasm_bindgen]
pub fn add_item(vault_json: &str, item_json: &str) -> Result<String, JsValue> {
    let mut vault: Vault = serde_json::from_str(vault_json)
        .map_err(|e| JsValue::from_str(&format!("invalid vault JSON: {e}")))?;

    let input: AddItemInput = serde_json::from_str(item_json)
        .map_err(|e| JsValue::from_str(&format!("invalid item JSON: {e}")))?;

    let item = input.into_vault_item();
    vault.add_item(item);

    serde_json::to_string(&vault)
        .map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))
}

/// List all items in a vault (given as JSON).
///
/// Returns a JSON array of vault items (serialised via serde-wasm-bindgen for
/// efficient JS interop).
#[wasm_bindgen]
pub fn list_items(vault_json: &str) -> Result<JsValue, JsValue> {
    let vault: Vault = serde_json::from_str(vault_json)
        .map_err(|e| JsValue::from_str(&format!("invalid vault JSON: {e}")))?;

    serde_wasm_bindgen::to_value(vault.list_items())
        .map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))
}

/// Generate a TOTP code from a base32-encoded secret.
///
/// Uses the current system time and default parameters (SHA-1, 6 digits, 30s period).
#[wasm_bindgen]
pub fn generate_totp(secret: &str) -> Result<String, JsValue> {
    use totp_rs::{Algorithm, TOTP};

    let totp = TOTP::new(Algorithm::SHA1, 6, 1, 30, secret.as_bytes().to_vec())
        .map_err(|e| JsValue::from_str(&format!("invalid TOTP secret: {e}")))?;

    let time = js_sys::Date::now() as u64 / 1000;
    Ok(totp.generate(time))
}

// ─── Relay settings WASM bindings ────────────────────────────────────────────

/// Validate a relay URL. Returns the normalised URL on success.
#[wasm_bindgen]
pub fn validate_relay_url(url: &str) -> Result<String, JsValue> {
    zvault_core::settings::validate_relay_url(url).map_err(|e| JsValue::from_str(&e))
}

/// Add a relay to a vault (given as JSON). Returns the updated vault JSON.
#[wasm_bindgen]
pub fn add_relay_to_vault(vault_json: &str, url: &str) -> Result<String, JsValue> {
    let mut vault: Vault = serde_json::from_str(vault_json)
        .map_err(|e| JsValue::from_str(&format!("invalid vault JSON: {e}")))?;

    zvault_core::settings::add_relay(&mut vault.settings, url)
        .map_err(|e| JsValue::from_str(&e))?;

    vault.version += 1;
    serde_json::to_string(&vault)
        .map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))
}

/// Remove a relay from a vault (given as JSON). Returns the updated vault JSON.
#[wasm_bindgen]
pub fn remove_relay_from_vault(vault_json: &str, url: &str) -> Result<String, JsValue> {
    let mut vault: Vault = serde_json::from_str(vault_json)
        .map_err(|e| JsValue::from_str(&format!("invalid vault JSON: {e}")))?;

    zvault_core::settings::remove_relay(&mut vault.settings, url)
        .map_err(|e| JsValue::from_str(&e))?;

    vault.version += 1;
    serde_json::to_string(&vault)
        .map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))
}

/// Toggle a relay's enabled state in a vault (given as JSON). Returns the updated vault JSON.
#[wasm_bindgen]
pub fn toggle_relay_in_vault(
    vault_json: &str,
    url: &str,
    enabled: bool,
) -> Result<String, JsValue> {
    let mut vault: Vault = serde_json::from_str(vault_json)
        .map_err(|e| JsValue::from_str(&format!("invalid vault JSON: {e}")))?;

    zvault_core::settings::set_relay_enabled(&mut vault.settings, url, enabled)
        .map_err(|e| JsValue::from_str(&e))?;

    vault.version += 1;
    serde_json::to_string(&vault)
        .map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))
}

/// Reset relays to defaults in a vault (given as JSON). Returns the updated vault JSON.
#[wasm_bindgen]
pub fn reset_relays_in_vault(vault_json: &str) -> Result<String, JsValue> {
    let mut vault: Vault = serde_json::from_str(vault_json)
        .map_err(|e| JsValue::from_str(&format!("invalid vault JSON: {e}")))?;

    zvault_core::settings::reset_relays(&mut vault.settings);

    vault.version += 1;
    serde_json::to_string(&vault)
        .map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))
}

/// Get the list of enabled relay URLs from a vault (given as JSON).
#[wasm_bindgen]
pub fn get_enabled_relays(vault_json: &str) -> Result<JsValue, JsValue> {
    let vault: Vault = serde_json::from_str(vault_json)
        .map_err(|e| JsValue::from_str(&format!("invalid vault JSON: {e}")))?;

    let urls = zvault_core::settings::enabled_relay_urls(&vault.settings);
    serde_wasm_bindgen::to_value(&urls)
        .map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))
}

/// Encode a hex public key as a NIP-19 npub bech32 string.
///
/// The input must be a 64-character hex string (32 bytes).
#[wasm_bindgen]
pub fn encode_npub_from_hex(pubkey_hex: &str) -> Result<String, JsValue> {
    if pubkey_hex.len() != 64 {
        return Err(JsValue::from_str("public key must be 64 hex characters"));
    }
    let bytes =
        hex::decode(pubkey_hex).map_err(|e| JsValue::from_str(&format!("invalid hex: {e}")))?;
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| JsValue::from_str("public key must be exactly 32 bytes"))?;
    Ok(zvault_core::nip19::encode_npub(&array))
}

// ─── Pairing codec WASM bindings ─────────────────────────────────────────────

/// Create an invite pairing code.
///
/// Returns a `zvault:` prefixed string suitable for display as a QR code.
#[wasm_bindgen]
pub fn create_invite_code(
    pubkey_hex: &str,
    label: &str,
    vault_id: &str,
) -> Result<String, JsValue> {
    let vid = uuid::Uuid::parse_str(vault_id)
        .map_err(|e| JsValue::from_str(&format!("invalid vault_id UUID: {e}")))?;
    let payload = zvault_core::pairing::create_invite(pubkey_hex, label, vid)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    zvault_core::pairing::encode_pairing_code(&payload)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Create a join-request pairing code.
///
/// Returns a `zvault:` prefixed string suitable for display as a QR code.
#[wasm_bindgen]
pub fn create_join_request_code(pubkey_hex: &str, label: &str) -> Result<String, JsValue> {
    let payload = zvault_core::pairing::create_join_request(pubkey_hex, label)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    zvault_core::pairing::encode_pairing_code(&payload)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Decode a pairing code and return the payload as a JSON object.
///
/// Returns a JS object with fields: v, t, p, l, vid, ts.
#[wasm_bindgen]
pub fn decode_pairing_code(code: &str) -> Result<JsValue, JsValue> {
    let payload = zvault_core::pairing::decode_pairing_code(code)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&payload)
        .map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))
}

/// Create a response pairing code (invite-response or join-response).
///
/// `response_type` must be "invite_response" or "join_response".
#[wasm_bindgen]
pub fn create_response_code(
    response_type: &str,
    pubkey_hex: &str,
    label: &str,
    vault_id: Option<String>,
) -> Result<String, JsValue> {
    let payload = match response_type {
        "invite_response" => zvault_core::pairing::create_invite_response(pubkey_hex, label)
            .map_err(|e| JsValue::from_str(&e.to_string()))?,
        "join_response" => {
            let vid_str = vault_id
                .as_deref()
                .ok_or_else(|| JsValue::from_str("vault_id required for join_response"))?;
            let vid = uuid::Uuid::parse_str(vid_str)
                .map_err(|e| JsValue::from_str(&format!("invalid vault_id: {e}")))?;
            zvault_core::pairing::create_join_response(pubkey_hex, label, vid)
                .map_err(|e| JsValue::from_str(&e.to_string()))?
        }
        _ => {
            return Err(JsValue::from_str(
                "response_type must be 'invite_response' or 'join_response'",
            ))
        }
    };
    zvault_core::pairing::encode_pairing_code(&payload)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Admit a device from a pairing payload and return the updated vault JSON.
///
/// `vault_json` is the current vault state.
/// `remote_pubkey` and `label` come from the decoded pairing payload.
#[wasm_bindgen]
pub fn admit_device_from_pairing(
    vault_json: &str,
    remote_pubkey: &str,
    label: &str,
) -> Result<String, JsValue> {
    // Validate remote_pubkey: must be exactly 64 hex characters.
    if remote_pubkey.len() != 64 || !remote_pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(JsValue::from_str(
            "Invalid public key: must be 64 hex characters",
        ));
    }
    // Validate label: must be non-empty after trimming.
    if label.trim().is_empty() {
        return Err(JsValue::from_str("Device label is required"));
    }

    let mut vault: Vault = serde_json::from_str(vault_json)
        .map_err(|e| JsValue::from_str(&format!("invalid vault JSON: {e}")))?;

    // Check for duplicate or revoked device with the same pubkey.
    let normalized_pubkey = remote_pubkey.to_lowercase();
    for existing in &vault.devices {
        if existing.nostr_pubkey == normalized_pubkey {
            if existing.revoked {
                return Err(JsValue::from_str(
                    "Device with this public key was previously revoked and cannot be re-admitted",
                ));
            } else {
                return Err(JsValue::from_str(
                    "Device with this public key is already admitted",
                ));
            }
        }
    }

    let device_id = uuid::Uuid::new_v4();
    let added_by = vault
        .devices
        .first()
        .map(|d| d.device_id)
        .unwrap_or(device_id);

    let entry = zvault_core::vault::DeviceEntry {
        device_id,
        nostr_pubkey: normalized_pubkey,
        label: label.trim().to_string(),
        added_at: chrono::Utc::now(),
        added_by,
        revoked: false,
        revoked_at: None,
        revoked_by: None,
    };
    vault.devices.push(entry);
    vault.version += 1;
    vault.updated_at = chrono::Utc::now();

    serde_json::to_string(&vault)
        .map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))
}

// ─── Sync / NIP-44 / NIP-59 WASM bindings ────────────────────────────────────

/// Build a full sync message from the current vault state.
///
/// NIP-44 encrypts the vault JSON for the specified recipient and returns
/// the serialised SyncMessage as a JSON string.
#[wasm_bindgen]
pub fn build_full_sync_message(
    vault_json: &str,
    device_id: &str,
    secret_key_hex: &str,
    recipient_pubkey_hex: &str,
) -> Result<String, JsValue> {
    let vault: zvault_core::vault::Vault = serde_json::from_str(vault_json)
        .map_err(|e| JsValue::from_str(&format!("invalid vault JSON: {e}")))?;

    let sender_uuid = uuid::Uuid::parse_str(device_id)
        .map_err(|e| JsValue::from_str(&format!("invalid device_id UUID: {e}")))?;

    let sk_bytes = zeroize::Zeroizing::new(
        hex::decode(secret_key_hex)
            .map_err(|e| JsValue::from_str(&format!("invalid secret key hex: {e}")))?,
    );

    let mut clock = zvault_core::sync::LamportClock::new();

    let msg = zvault_core::sync::build_full_sync_message(
        &vault,
        &mut clock,
        sender_uuid,
        &sk_bytes,
        recipient_pubkey_hex,
    )
    .map_err(|e| JsValue::from_str(&format!("build sync message failed: {e}")))?;

    serde_json::to_string(&msg).map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))
}

/// Apply an incoming sync message to the local vault.
///
/// Validates the sender, decrypts the payload, merges items (LWW), and returns
/// the updated vault as a JSON string.
#[wasm_bindgen]
pub fn apply_sync_message(
    vault_json: &str,
    sync_msg_json: &str,
    secret_key_hex: &str,
    sender_pubkey_hex: &str,
) -> Result<String, JsValue> {
    let mut vault: zvault_core::vault::Vault = serde_json::from_str(vault_json)
        .map_err(|e| JsValue::from_str(&format!("invalid vault JSON: {e}")))?;

    let msg: zvault_core::sync::SyncMessage = serde_json::from_str(sync_msg_json)
        .map_err(|e| JsValue::from_str(&format!("invalid sync message JSON: {e}")))?;

    let sk_bytes = zeroize::Zeroizing::new(
        hex::decode(secret_key_hex)
            .map_err(|e| JsValue::from_str(&format!("invalid secret key hex: {e}")))?,
    );

    let mut clock = zvault_core::sync::LamportClock::new();

    zvault_core::sync::apply_sync_message(
        &mut vault,
        &msg,
        &mut clock,
        &sk_bytes,
        sender_pubkey_hex,
    )
    .map_err(|e| JsValue::from_str(&format!("apply sync message failed: {e}")))?;

    serde_json::to_string(&vault)
        .map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))
}

/// NIP-44 encrypt plaintext for a recipient.
///
/// Derives the conversation key from sender's secret key and recipient's pubkey,
/// then encrypts. Returns base64-encoded ciphertext.
#[wasm_bindgen]
pub fn nip44_encrypt(
    sender_sk_hex: &str,
    recipient_pk_hex: &str,
    plaintext: &str,
) -> Result<String, JsValue> {
    let sk_bytes = zeroize::Zeroizing::new(
        hex::decode(sender_sk_hex)
            .map_err(|e| JsValue::from_str(&format!("invalid secret key hex: {e}")))?,
    );

    let conversation_key = zvault_core::nostr::get_conversation_key(&sk_bytes, recipient_pk_hex)
        .map_err(|e| JsValue::from_str(&format!("get_conversation_key failed: {e}")))?;

    zvault_core::nostr::nip44_encrypt(&conversation_key, plaintext.as_bytes())
        .map_err(|e| JsValue::from_str(&format!("nip44_encrypt failed: {e}")))
}

/// NIP-44 decrypt ciphertext from a sender.
///
/// Derives the conversation key from receiver's secret key and sender's pubkey,
/// then decrypts the base64-encoded payload. Returns plaintext string.
#[wasm_bindgen]
pub fn nip44_decrypt(
    receiver_sk_hex: &str,
    sender_pk_hex: &str,
    ciphertext_b64: &str,
) -> Result<String, JsValue> {
    let sk_bytes = zeroize::Zeroizing::new(
        hex::decode(receiver_sk_hex)
            .map_err(|e| JsValue::from_str(&format!("invalid secret key hex: {e}")))?,
    );

    let conversation_key = zvault_core::nostr::get_conversation_key(&sk_bytes, sender_pk_hex)
        .map_err(|e| JsValue::from_str(&format!("get_conversation_key failed: {e}")))?;

    let plaintext_bytes = zvault_core::nostr::nip44_decrypt(&conversation_key, ciphertext_b64)
        .map_err(|e| JsValue::from_str(&format!("nip44_decrypt failed: {e}")))?;

    String::from_utf8(plaintext_bytes)
        .map_err(|e| JsValue::from_str(&format!("decrypted payload is not valid UTF-8: {e}")))
}

/// Create a NIP-59 gift-wrapped event.
///
/// Triple-wraps the content (rumor → seal → gift-wrap) to hide sender identity.
/// Returns the gift-wrapped NostrEvent as a JSON string.
#[wasm_bindgen]
pub fn gift_wrap(
    sender_sk_hex: &str,
    recipient_pk_hex: &str,
    content: &str,
    kind: u32,
    tags_json: &str,
) -> Result<String, JsValue> {
    let sk_bytes = hex::decode(sender_sk_hex)
        .map_err(|e| JsValue::from_str(&format!("invalid secret key hex: {e}")))?;
    let sk = zeroize::Zeroizing::new(sk_bytes);

    let tags: Vec<Vec<String>> = serde_json::from_str(tags_json)
        .map_err(|e| JsValue::from_str(&format!("invalid tags JSON: {e}")))?;

    let event = zvault_core::nostr::gift_wrap(&sk, recipient_pk_hex, content, kind, &tags)
        .map_err(|e| JsValue::from_str(&format!("gift_wrap failed: {e}")))?;

    serde_json::to_string(&event)
        .map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))
}

/// Unwrap a NIP-59 gift-wrapped event.
///
/// Decrypts the triple-wrapped event and returns the inner rumor as a JSON string.
#[wasm_bindgen]
pub fn unwrap_gift_wrap(receiver_sk_hex: &str, event_json: &str) -> Result<String, JsValue> {
    let sk_bytes = hex::decode(receiver_sk_hex)
        .map_err(|e| JsValue::from_str(&format!("invalid secret key hex: {e}")))?;
    let sk = zeroize::Zeroizing::new(sk_bytes);

    let event: zvault_core::nostr::NostrEvent = serde_json::from_str(event_json)
        .map_err(|e| JsValue::from_str(&format!("invalid event JSON: {e}")))?;

    let rumor = zvault_core::nostr::unwrap_gift_wrap(&sk, &event)
        .map_err(|e| JsValue::from_str(&format!("unwrap_gift_wrap failed: {e}")))?;

    serde_json::to_string(&rumor)
        .map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))
}

/// Sign a NIP-01 Nostr event.
///
/// Takes a secret key and event fields as JSON, returns the signed NostrEvent JSON.
/// The event_json must contain: content, kind, tags, created_at.
#[wasm_bindgen]
pub fn sign_event(sk_hex: &str, event_json: &str) -> Result<String, JsValue> {
    let sk_bytes = hex::decode(sk_hex)
        .map_err(|e| JsValue::from_str(&format!("invalid secret key hex: {e}")))?;
    let sk = zeroize::Zeroizing::new(sk_bytes);

    #[derive(Deserialize)]
    struct EventInput {
        content: String,
        kind: u32,
        #[serde(default)]
        tags: Vec<Vec<String>>,
        created_at: i64,
    }

    let input: EventInput = serde_json::from_str(event_json)
        .map_err(|e| JsValue::from_str(&format!("invalid event JSON: {e}")))?;

    let event = zvault_core::nostr::sign_event(
        &sk,
        &input.content,
        input.kind,
        input.tags,
        input.created_at,
    )
    .map_err(|e| JsValue::from_str(&format!("sign_event failed: {e}")))?;

    serde_json::to_string(&event)
        .map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))
}

/// Verify a NIP-01 event signature.
///
/// Returns true if the event signature is valid, false otherwise.
#[wasm_bindgen]
pub fn verify_event(event_json: &str) -> bool {
    let event: std::result::Result<zvault_core::nostr::NostrEvent, _> =
        serde_json::from_str(event_json);
    match event {
        Ok(e) => zvault_core::nostr::verify_event(&e).is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
    const DIGITS: &str = "0123456789";
    const SPECIAL: &str = "!@#$%^&*()_+-=[]{}|;:,.<>?";

    #[test]
    fn test_generate_password_default_length() {
        let result = generate_password_inner(None);
        assert!(result.is_ok());
        let pw = result.unwrap();
        assert_eq!(pw.len(), 20);
    }

    #[test]
    fn test_generate_password_custom_length() {
        for len in [4, 8, 16, 32, 64, 128] {
            let result = generate_password_inner(Some(len));
            assert!(result.is_ok());
            let pw = result.unwrap();
            assert_eq!(pw.len(), len as usize);
        }
    }

    #[test]
    fn test_generate_password_character_class_coverage() {
        // Run multiple times to verify the guarantee holds
        for _ in 0..50 {
            let pw = generate_password_inner(Some(4)).unwrap();
            assert!(
                pw.chars().any(|c| UPPERCASE.contains(c)),
                "Missing uppercase in: {pw}"
            );
            assert!(
                pw.chars().any(|c| LOWERCASE.contains(c)),
                "Missing lowercase in: {pw}"
            );
            assert!(
                pw.chars().any(|c| DIGITS.contains(c)),
                "Missing digit in: {pw}"
            );
            assert!(
                pw.chars().any(|c| SPECIAL.contains(c)),
                "Missing special in: {pw}"
            );
        }
    }

    #[test]
    fn test_generate_password_longer_lengths_have_all_classes() {
        for len in [10, 20, 50, 100] {
            let pw = generate_password_inner(Some(len)).unwrap();
            assert_eq!(pw.len(), len as usize);
            assert!(pw.chars().any(|c| UPPERCASE.contains(c)));
            assert!(pw.chars().any(|c| LOWERCASE.contains(c)));
            assert!(pw.chars().any(|c| DIGITS.contains(c)));
            assert!(pw.chars().any(|c| SPECIAL.contains(c)));
        }
    }

    #[test]
    fn test_generate_password_rejects_length_below_4() {
        for len in [0, 1, 2, 3] {
            let result = generate_password_inner(Some(len));
            assert!(result.is_err(), "Should reject length {len}");
        }
    }

    #[test]
    fn test_generate_password_only_valid_characters() {
        let valid: String = format!("{UPPERCASE}{LOWERCASE}{DIGITS}{SPECIAL}");
        for _ in 0..20 {
            let pw = generate_password_inner(Some(30)).unwrap();
            for c in pw.chars() {
                assert!(
                    valid.contains(c),
                    "Invalid character '{c}' in password: {pw}"
                );
            }
        }
    }

    /// Property: for any valid length in [4, 128], the generated password
    /// has exactly that length AND contains all four character classes.
    #[test]
    fn test_generate_password_property_length_and_classes() {
        for len in 4u32..=128 {
            let pw = generate_password_inner(Some(len)).unwrap();
            assert_eq!(
                pw.len(),
                len as usize,
                "Length mismatch for requested {len}"
            );
            assert!(
                pw.chars().any(|c| UPPERCASE.contains(c)),
                "Missing uppercase for length {len}: {pw}"
            );
            assert!(
                pw.chars().any(|c| LOWERCASE.contains(c)),
                "Missing lowercase for length {len}: {pw}"
            );
            assert!(
                pw.chars().any(|c| DIGITS.contains(c)),
                "Missing digit for length {len}: {pw}"
            );
            assert!(
                pw.chars().any(|c| SPECIAL.contains(c)),
                "Missing special for length {len}: {pw}"
            );
        }
    }

    /// Property: passwords generated with the same parameters are different
    /// (randomness check — extremely unlikely to collide for length >= 8).
    #[test]
    fn test_generate_password_property_uniqueness() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..100 {
            let pw = generate_password_inner(Some(20)).unwrap();
            seen.insert(pw);
        }
        // With 20-char passwords from ~90 chars, collisions are astronomically unlikely
        assert!(
            seen.len() >= 99,
            "Too many collisions: only {} unique passwords out of 100",
            seen.len()
        );
    }

    /// Property: generate_password always returns valid UTF-8 (implied by
    /// the character sets being ASCII, but verified explicitly).
    #[test]
    fn test_generate_password_property_valid_utf8() {
        for len in [4, 10, 50, 128] {
            for _ in 0..10 {
                let pw = generate_password_inner(Some(len)).unwrap();
                // If this were invalid UTF-8, from_utf8 inside the function
                // would have returned Err. Double-check the string is valid.
                assert!(pw.is_ascii(), "Password should be pure ASCII: {pw}");
            }
        }
    }
}
