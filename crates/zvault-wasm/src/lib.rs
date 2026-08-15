//! WebAssembly bindings for zvault-core.
//!
//! Exposes a subset of zvault-core functionality to JavaScript via wasm-bindgen.
//! Used by the ZVault browser extension for in-browser vault encryption/decryption.

use wasm_bindgen::prelude::*;
use zvault_core::crypto::{decrypt, derive_key, encrypt_with_params, parse_kdf_params, KdfParams};
use zvault_core::vault::{Vault, VaultItem};

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
