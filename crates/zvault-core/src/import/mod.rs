//! Import parsers for external password manager formats.
//!
//! Supported formats:
//!
//! - **Bitwarden JSON** — unencrypted export from Bitwarden
//! - **Generic CSV** — columns: name, url, username, password, notes
//! - **`.zvault-export`** — ZVault's own encrypted export format
//!
//! All parsers return `Vec<VaultItem>` on success. Sensitive intermediate
//! buffers are wrapped in [`Zeroizing`] to ensure zeroing on drop.

use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{self, VaultKey};
use crate::error::Error;
use crate::vault::{IdentityFields, ItemKind, Uri, UriMatch, VaultItem};
use crate::Result;

// ─── Export format magic ─────────────────────────────────────────────────────

/// Magic bytes at the start of `.zvault-export` files.
pub const EXPORT_MAGIC: &[u8; 8] = b"ZVEXPT01";

// ─── Bitwarden JSON import ───────────────────────────────────────────────────

/// Top-level structure of a Bitwarden unencrypted JSON export.
#[derive(Debug, Deserialize)]
struct BitwardenExport {
    /// Optional — present in most exports but not strictly required.
    #[serde(default)]
    items: Vec<BitwardenItem>,
}

/// A single item in a Bitwarden export.
#[derive(Debug, Deserialize)]
struct BitwardenItem {
    /// Item type: 1 = Login, 2 = SecureNote, 3 = Card, 4 = Identity
    #[serde(rename = "type")]
    item_type: u32,
    /// Display name.
    name: Option<String>,
    /// Notes field.
    notes: Option<String>,
    /// Favourite flag.
    #[serde(default)]
    favorite: bool,
    /// Login fields (present when type == 1).
    login: Option<BitwardenLogin>,
    /// Card fields (present when type == 3).
    card: Option<BitwardenCard>,
    /// Identity fields (present when type == 4).
    identity: Option<BitwardenIdentity>,
}

/// Login sub-object in a Bitwarden export.
#[derive(Debug, Deserialize)]
struct BitwardenLogin {
    username: Option<String>,
    password: Option<String>,
    totp: Option<String>,
    #[serde(default)]
    uris: Vec<BitwardenUri>,
}

/// URI entry in a Bitwarden login.
#[derive(Debug, Deserialize)]
struct BitwardenUri {
    uri: Option<String>,
    /// Match type: 0=Domain, 1=Host, 2=StartsWith, 3=Exact, 4=Regex, 5=Never, null=Domain
    #[serde(rename = "match")]
    match_type: Option<u32>,
}

/// Card sub-object in a Bitwarden export.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BitwardenCard {
    cardholder_name: Option<String>,
    number: Option<String>,
    exp_month: Option<String>,
    exp_year: Option<String>,
    code: Option<String>,
}

/// Identity sub-object in a Bitwarden export.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BitwardenIdentity {
    first_name: Option<String>,
    last_name: Option<String>,
    address1: Option<String>,
    city: Option<String>,
    country: Option<String>,
    phone: Option<String>,
    email: Option<String>,
}

/// Import items from a Bitwarden unencrypted JSON export.
///
/// # Errors
///
/// Returns [`Error::Serialisation`] if the JSON is malformed or does not
/// match the expected Bitwarden format.
pub fn import_bitwarden_json(data: &[u8]) -> Result<Vec<VaultItem>> {
    let json_buf = Zeroizing::new(data.to_vec());
    let export: BitwardenExport = serde_json::from_slice(&json_buf)
        .map_err(|e| Error::Serialisation(format!("Bitwarden JSON parse error: {e}")))?;

    let now = Utc::now();
    let mut items = Vec::with_capacity(export.items.len());

    for bw_item in export.items {
        let kind = match bw_item.item_type {
            1 => ItemKind::Login,
            3 => ItemKind::Card,
            4 => ItemKind::Identity,
            // Type 2 = SecureNote; unknown types also become secure notes.
            _ => ItemKind::SecureNote,
        };

        let name = bw_item.name.unwrap_or_else(|| "Untitled".to_string());

        let mut item = VaultItem {
            id: Uuid::new_v4(),
            kind,
            name,
            folder: None,
            favourite: bw_item.favorite,
            created_at: now,
            updated_at: now,
            username: None,
            password: None,
            totp_secret: None,
            uris: Vec::new(),
            note: bw_item.notes,
            card_number: None,
            expiry: None,
            cvv: None,
            cardholder: None,
            identity: None,
        };

        // Populate type-specific fields.
        match bw_item.item_type {
            1 => {
                if let Some(login) = bw_item.login {
                    item.username = login.username;
                    item.password = login.password;
                    item.totp_secret = login.totp;
                    item.uris = login
                        .uris
                        .into_iter()
                        .filter_map(|u| {
                            u.uri.map(|uri| Uri {
                                uri,
                                r#match: match u.match_type {
                                    Some(1) => UriMatch::Host,
                                    Some(2) => UriMatch::StartsWith,
                                    Some(3) => UriMatch::Exact,
                                    Some(4) => UriMatch::Regex,
                                    Some(5) => UriMatch::Never,
                                    // 0, None, or unknown → Domain.
                                    _ => UriMatch::Domain,
                                },
                            })
                        })
                        .collect();
                }
            }
            3 => {
                if let Some(card) = bw_item.card {
                    item.cardholder = card.cardholder_name;
                    item.card_number = card.number;
                    item.cvv = card.code;
                    // Combine exp_month/exp_year into "MM/YY" format.
                    item.expiry = match (card.exp_month, card.exp_year) {
                        (Some(m), Some(y)) => {
                            let year = if y.len() > 2 { &y[y.len() - 2..] } else { &y };
                            Some(format!("{m:0>2}/{year}"))
                        }
                        (Some(m), None) => Some(m),
                        (None, Some(y)) => Some(y),
                        (None, None) => None,
                    };
                }
            }
            4 => {
                if let Some(ident) = bw_item.identity {
                    item.identity = Some(IdentityFields {
                        first_name: ident.first_name,
                        last_name: ident.last_name,
                        address: ident.address1,
                        city: ident.city,
                        country: ident.country,
                        phone: ident.phone,
                        email: ident.email,
                    });
                }
            }
            _ => {} // SecureNote and unknown — notes already set above.
        }

        items.push(item);
    }

    Ok(items)
}

// ─── Generic CSV import ──────────────────────────────────────────────────────

/// Import items from a generic CSV file.
///
/// Expected columns (case-insensitive, flexible ordering):
/// `name`, `url`, `username`, `password`, `notes`
///
/// If headers don't match exactly, falls back to positional parsing:
/// column 0 = name, 1 = url, 2 = username, 3 = password, 4 = notes.
///
/// All imported items are created as [`ItemKind::Login`].
///
/// # Errors
///
/// Returns [`Error::Serialisation`] if the CSV is malformed.
pub fn import_csv(data: &[u8]) -> Result<Vec<VaultItem>> {
    let csv_buf = Zeroizing::new(data.to_vec());
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(csv_buf.as_slice());

    // Detect column indices from headers.
    let (idx_name, idx_url, idx_username, idx_password, idx_notes) = {
        let headers = reader
            .headers()
            .map_err(|e| Error::Serialisation(format!("CSV header error: {e}")))?;

        let find_col = |names: &[&str]| -> Option<usize> {
            headers.iter().position(|h| {
                let lower = h.trim().to_lowercase();
                names.contains(&lower.as_str())
            })
        };

        (
            find_col(&["name", "title", "entry"]).unwrap_or(0),
            find_col(&["url", "uri", "website", "login_uri"]).unwrap_or(1),
            find_col(&["username", "user", "login", "email", "login_username"]).unwrap_or(2),
            find_col(&["password", "pass", "login_password"]).unwrap_or(3),
            find_col(&["notes", "note", "comments", "extra"]).unwrap_or(4),
        )
    };

    let now = Utc::now();
    let mut items = Vec::new();

    for result in reader.records() {
        let record = result.map_err(|e| Error::Serialisation(format!("CSV record error: {e}")))?;

        let get_field = |idx: usize| -> Option<String> {
            record
                .get(idx)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };

        let name = get_field(idx_name).unwrap_or_else(|| "Untitled".to_string());
        let url = get_field(idx_url);
        let username = get_field(idx_username);
        let password = get_field(idx_password);
        let notes = get_field(idx_notes);

        let uris = url
            .map(|u| {
                vec![Uri {
                    uri: u,
                    r#match: UriMatch::Domain,
                }]
            })
            .unwrap_or_default();

        let item = VaultItem {
            id: Uuid::new_v4(),
            kind: ItemKind::Login,
            name,
            folder: None,
            favourite: false,
            created_at: now,
            updated_at: now,
            username,
            password,
            totp_secret: None,
            uris,
            note: notes,
            card_number: None,
            expiry: None,
            cvv: None,
            cardholder: None,
            identity: None,
        };

        items.push(item);
    }

    Ok(items)
}

// ─── .zvault-export import ───────────────────────────────────────────────────

/// Import items from a `.zvault-export` encrypted file.
///
/// The `.zvault-export` format uses the same AES-256-GCM encryption as the
/// vault file, but with a different magic header (`ZVEXPT01`) to distinguish
/// it from a live vault. The plaintext payload is a JSON array of
/// [`VaultItem`]s.
///
/// # Errors
///
/// - [`Error::InvalidVaultFile`] — wrong magic, blob too short, or
///   authentication failure.
/// - [`Error::Serialisation`] — decrypted content is not valid JSON.
pub fn import_zvault_export(data: &[u8], password: &str) -> Result<Vec<VaultItem>> {
    // Validate export magic.
    if data.len() < crypto::HEADER_LEN + 16 {
        return Err(Error::InvalidVaultFile("export file too short".to_string()));
    }

    if &data[..8] != EXPORT_MAGIC {
        return Err(Error::InvalidVaultFile(format!(
            "bad export magic: expected {:?}, got {:?}",
            EXPORT_MAGIC,
            &data[..8]
        )));
    }

    // Replace export magic with vault magic for crypto::decrypt compatibility.
    let mut blob = data.to_vec();
    blob[..8].copy_from_slice(crypto::MAGIC);

    // Parse KDF params and derive key.
    let kdf_params = crypto::parse_kdf_params(&blob)?;
    let key = crypto::derive_key(password, &kdf_params)?;

    // Decrypt.
    let plaintext = Zeroizing::new(crypto::decrypt(&key, &blob)?);

    // Parse JSON array of VaultItems.
    let items: Vec<VaultItem> = serde_json::from_slice(&plaintext)
        .map_err(|e| Error::Serialisation(format!("export JSON parse error: {e}")))?;

    Ok(items)
}

/// Derive a [`VaultKey`] for use with `.zvault-export` format.
///
/// This is a convenience function that generates fresh KDF params and derives
/// a key, returning both so the caller can use them for encryption.
///
/// # Errors
///
/// Returns [`Error::Crypto`] if the Argon2id computation fails.
pub fn derive_export_key(password: &str) -> Result<(VaultKey, crypto::KdfParams)> {
    let params = crypto::KdfParams::generate();
    let key = crypto::derive_key(password, &params)?;
    Ok((key, params))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Bitwarden JSON tests ─────────────────────────────────────────────────

    #[test]
    fn bitwarden_import_login_item() {
        let json = r#"{
            "items": [{
                "type": 1,
                "name": "GitHub",
                "favorite": true,
                "notes": "My GitHub account",
                "login": {
                    "username": "alice@example.com",
                    "password": "s3cr3t!",
                    "totp": "JBSWY3DPEHPK3PXP",
                    "uris": [
                        {"uri": "https://github.com/login", "match": 0}
                    ]
                }
            }]
        }"#;

        let items = import_bitwarden_json(json.as_bytes()).unwrap();
        assert_eq!(items.len(), 1);

        let item = &items[0];
        assert_eq!(item.kind, ItemKind::Login);
        assert_eq!(item.name, "GitHub");
        assert!(item.favourite);
        assert_eq!(item.username.as_deref(), Some("alice@example.com"));
        assert_eq!(item.password.as_deref(), Some("s3cr3t!"));
        assert_eq!(item.totp_secret.as_deref(), Some("JBSWY3DPEHPK3PXP"));
        assert_eq!(item.note.as_deref(), Some("My GitHub account"));
        assert_eq!(item.uris.len(), 1);
        assert_eq!(item.uris[0].uri, "https://github.com/login");
        assert_eq!(item.uris[0].r#match, UriMatch::Domain);
    }

    #[test]
    fn bitwarden_import_secure_note() {
        let json = r#"{
            "items": [{
                "type": 2,
                "name": "Recovery Codes",
                "notes": "code1\ncode2\ncode3",
                "favorite": false
            }]
        }"#;

        let items = import_bitwarden_json(json.as_bytes()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ItemKind::SecureNote);
        assert_eq!(items[0].note.as_deref(), Some("code1\ncode2\ncode3"));
    }

    #[test]
    fn bitwarden_import_card() {
        let json = r#"{
            "items": [{
                "type": 3,
                "name": "Visa",
                "notes": null,
                "favorite": false,
                "card": {
                    "cardholderName": "Alice Smith",
                    "number": "4111111111111111",
                    "expMonth": "12",
                    "expYear": "2028",
                    "code": "123"
                }
            }]
        }"#;

        let items = import_bitwarden_json(json.as_bytes()).unwrap();
        assert_eq!(items.len(), 1);

        let item = &items[0];
        assert_eq!(item.kind, ItemKind::Card);
        assert_eq!(item.cardholder.as_deref(), Some("Alice Smith"));
        assert_eq!(item.card_number.as_deref(), Some("4111111111111111"));
        assert_eq!(item.expiry.as_deref(), Some("12/28"));
        assert_eq!(item.cvv.as_deref(), Some("123"));
    }

    #[test]
    fn bitwarden_import_identity() {
        let json = r#"{
            "items": [{
                "type": 4,
                "name": "Personal",
                "notes": null,
                "favorite": false,
                "identity": {
                    "firstName": "Alice",
                    "lastName": "Smith",
                    "address1": "123 Main St",
                    "city": "Portland",
                    "country": "US",
                    "phone": "+1-555-0100",
                    "email": "alice@example.com"
                }
            }]
        }"#;

        let items = import_bitwarden_json(json.as_bytes()).unwrap();
        assert_eq!(items.len(), 1);

        let item = &items[0];
        assert_eq!(item.kind, ItemKind::Identity);
        let ident = item.identity.as_ref().unwrap();
        assert_eq!(ident.first_name.as_deref(), Some("Alice"));
        assert_eq!(ident.last_name.as_deref(), Some("Smith"));
        assert_eq!(ident.address.as_deref(), Some("123 Main St"));
        assert_eq!(ident.city.as_deref(), Some("Portland"));
        assert_eq!(ident.country.as_deref(), Some("US"));
        assert_eq!(ident.phone.as_deref(), Some("+1-555-0100"));
        assert_eq!(ident.email.as_deref(), Some("alice@example.com"));
    }

    #[test]
    fn bitwarden_import_multiple_items() {
        let json = r#"{
            "items": [
                {"type": 1, "name": "Login1", "favorite": false, "login": {"username": "u1", "password": "p1", "uris": []}},
                {"type": 1, "name": "Login2", "favorite": false, "login": {"username": "u2", "password": "p2", "uris": []}},
                {"type": 2, "name": "Note1", "notes": "secret", "favorite": false}
            ]
        }"#;

        let items = import_bitwarden_json(json.as_bytes()).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind, ItemKind::Login);
        assert_eq!(items[1].kind, ItemKind::Login);
        assert_eq!(items[2].kind, ItemKind::SecureNote);
    }

    #[test]
    fn bitwarden_import_unknown_type_becomes_secure_note() {
        let json = r#"{
            "items": [{
                "type": 99,
                "name": "Unknown",
                "notes": "some data",
                "favorite": false
            }]
        }"#;

        let items = import_bitwarden_json(json.as_bytes()).unwrap();
        assert_eq!(items[0].kind, ItemKind::SecureNote);
    }

    #[test]
    fn bitwarden_import_missing_fields() {
        let json = r#"{
            "items": [{
                "type": 1,
                "name": null,
                "favorite": false,
                "login": {
                    "username": null,
                    "password": null,
                    "uris": []
                }
            }]
        }"#;

        let items = import_bitwarden_json(json.as_bytes()).unwrap();
        assert_eq!(items[0].name, "Untitled");
        assert!(items[0].username.is_none());
        assert!(items[0].password.is_none());
    }

    #[test]
    fn bitwarden_import_empty_items() {
        let json = r#"{"items": []}"#;
        let items = import_bitwarden_json(json.as_bytes()).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn bitwarden_import_invalid_json() {
        let result = import_bitwarden_json(b"not json");
        assert!(matches!(result, Err(Error::Serialisation(_))));
    }

    #[test]
    fn bitwarden_import_uri_match_types() {
        let json = r#"{
            "items": [{
                "type": 1,
                "name": "Test",
                "favorite": false,
                "login": {
                    "username": "u",
                    "password": "p",
                    "uris": [
                        {"uri": "https://a.com", "match": null},
                        {"uri": "https://b.com", "match": 1},
                        {"uri": "https://c.com", "match": 2},
                        {"uri": "https://d.com", "match": 3},
                        {"uri": "https://e.com", "match": 4},
                        {"uri": "https://f.com", "match": 5}
                    ]
                }
            }]
        }"#;

        let items = import_bitwarden_json(json.as_bytes()).unwrap();
        let uris = &items[0].uris;
        assert_eq!(uris[0].r#match, UriMatch::Domain);
        assert_eq!(uris[1].r#match, UriMatch::Host);
        assert_eq!(uris[2].r#match, UriMatch::StartsWith);
        assert_eq!(uris[3].r#match, UriMatch::Exact);
        assert_eq!(uris[4].r#match, UriMatch::Regex);
        assert_eq!(uris[5].r#match, UriMatch::Never);
    }

    // ── CSV import tests ─────────────────────────────────────────────────────

    #[test]
    fn csv_import_basic() {
        let csv_data = b"name,url,username,password,notes\nGitHub,https://github.com,alice,s3cr3t,my account\n";

        let items = import_csv(csv_data).unwrap();
        assert_eq!(items.len(), 1);

        let item = &items[0];
        assert_eq!(item.kind, ItemKind::Login);
        assert_eq!(item.name, "GitHub");
        assert_eq!(item.username.as_deref(), Some("alice"));
        assert_eq!(item.password.as_deref(), Some("s3cr3t"));
        assert_eq!(item.note.as_deref(), Some("my account"));
        assert_eq!(item.uris.len(), 1);
        assert_eq!(item.uris[0].uri, "https://github.com");
    }

    #[test]
    fn csv_import_multiple_rows() {
        let csv_data = b"name,url,username,password,notes\nA,https://a.com,u1,p1,n1\nB,https://b.com,u2,p2,n2\n";

        let items = import_csv(csv_data).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "A");
        assert_eq!(items[1].name, "B");
    }

    #[test]
    fn csv_import_missing_columns() {
        // Only 3 columns instead of 5; should still work with flexible mode.
        let csv_data = b"name,url,username\nGitHub,https://github.com,alice\n";

        let items = import_csv(csv_data).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "GitHub");
        assert_eq!(items[0].username.as_deref(), Some("alice"));
        assert!(items[0].password.is_none());
        assert!(items[0].note.is_none());
    }

    #[test]
    fn csv_import_empty_fields() {
        let csv_data = b"name,url,username,password,notes\n,,,,\n";

        let items = import_csv(csv_data).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Untitled");
        assert!(items[0].username.is_none());
        assert!(items[0].password.is_none());
        assert!(items[0].uris.is_empty());
    }

    #[test]
    fn csv_import_alternative_headers() {
        let csv_data = b"title,website,user,pass,comments\nGitHub,https://github.com,alice,s3cr3t,notes here\n";

        let items = import_csv(csv_data).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "GitHub");
        assert_eq!(items[0].username.as_deref(), Some("alice"));
        assert_eq!(items[0].password.as_deref(), Some("s3cr3t"));
        assert_eq!(items[0].note.as_deref(), Some("notes here"));
    }

    #[test]
    fn csv_import_empty_file_with_headers() {
        let csv_data = b"name,url,username,password,notes\n";
        let items = import_csv(csv_data).unwrap();
        assert!(items.is_empty());
    }

    // ── .zvault-export import tests ──────────────────────────────────────────

    #[test]
    fn zvault_export_roundtrip() {
        // Create items, export, then import.
        let items = vec![
            {
                let mut item = VaultItem::new(ItemKind::Login, "GitHub");
                item.username = Some("alice".into());
                item.password = Some("p@ss".into());
                item
            },
            {
                let mut item = VaultItem::new(ItemKind::SecureNote, "My Note");
                item.note = Some("secret content".into());
                item
            },
        ];

        // Export using the export module.
        let password = "test-export-password";
        let exported = crate::export::export_zvault_encrypted(&items, password).unwrap();

        // Import back.
        let imported = import_zvault_export(&exported, password).unwrap();
        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0].name, "GitHub");
        assert_eq!(imported[0].username.as_deref(), Some("alice"));
        assert_eq!(imported[0].password.as_deref(), Some("p@ss"));
        assert_eq!(imported[1].name, "My Note");
        assert_eq!(imported[1].note.as_deref(), Some("secret content"));
    }

    #[test]
    fn zvault_export_wrong_password() {
        let items = vec![VaultItem::new(ItemKind::Login, "Test")];
        let exported = crate::export::export_zvault_encrypted(&items, "correct").unwrap();

        let result = import_zvault_export(&exported, "wrong");
        assert!(result.is_err());
    }

    #[test]
    fn zvault_export_too_short() {
        let result = import_zvault_export(b"short", "pass");
        assert!(matches!(result, Err(Error::InvalidVaultFile(_))));
    }

    #[test]
    fn zvault_export_wrong_magic() {
        let mut data = vec![0u8; 100];
        data[..8].copy_from_slice(b"BADMAGIC");
        let result = import_zvault_export(&data, "pass");
        assert!(matches!(result, Err(Error::InvalidVaultFile(_))));
    }
}
