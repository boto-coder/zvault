//! WebAssembly bindings for zvault-core.
//!
//! Exposes a subset of zvault-core functionality to JavaScript via wasm-bindgen.
//! Used by the ZVault browser extension for in-browser vault encryption/decryption.

use wasm_bindgen::prelude::*;
use zvault_core::crypto::{decrypt, derive_key, encrypt_with_params, parse_kdf_params, KdfParams};
use zvault_core::vault::{Vault, VaultItem};

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
/// `item_json` is a JSON object describing the new item to add.
///
/// Returns the updated vault JSON string.
#[wasm_bindgen]
pub fn add_item(vault_json: &str, item_json: &str) -> Result<String, JsValue> {
    let mut vault: Vault = serde_json::from_str(vault_json)
        .map_err(|e| JsValue::from_str(&format!("invalid vault JSON: {e}")))?;

    let item: VaultItem = serde_json::from_str(item_json)
        .map_err(|e| JsValue::from_str(&format!("invalid item JSON: {e}")))?;

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
