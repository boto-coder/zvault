//! Export writers for vault items.
//!
//! Supported formats:
//!
//! - **`.zvault-export`** — AES-256-GCM encrypted export (same crypto as vault,
//!   different magic header `ZVEXPT01`). Payload is a JSON array of [`VaultItem`]s.
//! - **Plaintext JSON** — unencrypted JSON array of items.
//! - **Plaintext CSV** — columns: name, url, username, password, notes.
//!
//! Sensitive intermediate buffers are wrapped in [`Zeroizing`] to ensure
//! zeroing on drop.

use zeroize::Zeroizing;

use crate::crypto::{self, KdfParams, VaultKey};
use crate::error::Error;
use crate::import::EXPORT_MAGIC;
use crate::vault::VaultItem;
use crate::Result;

// ─── .zvault-export encrypted export ─────────────────────────────────────────

/// Export items as an encrypted `.zvault-export` blob.
///
/// Uses the same AES-256-GCM encryption as the vault file, but with a
/// different magic header (`ZVEXPT01`) so tools can distinguish an export
/// from a live vault.
///
/// The `password` is used to derive an encryption key via Argon2id with
/// fresh KDF parameters (fresh salt, default cost params).
///
/// # Errors
///
/// - [`Error::Serialisation`] — if item serialisation fails.
/// - [`Error::Crypto`] — if encryption fails.
pub fn export_zvault_encrypted(items: &[VaultItem], password: &str) -> Result<Vec<u8>> {
    // Serialise items to JSON.
    let json: Zeroizing<Vec<u8>> = Zeroizing::new(
        serde_json::to_vec(items)
            .map_err(|e| Error::Serialisation(format!("export serialisation error: {e}")))?,
    );

    // Generate fresh KDF params and derive key.
    let params = KdfParams::generate();
    let key = crypto::derive_key(password, &params)?;

    // Encrypt using the standard vault encryption.
    let mut blob = crypto::encrypt_with_params(&key, &json, &params)?;

    // Replace vault magic with export magic.
    blob[..8].copy_from_slice(EXPORT_MAGIC);

    Ok(blob)
}

/// Export items as an encrypted `.zvault-export` blob using a pre-derived key
/// and explicit KDF params.
///
/// This is useful when the caller has already derived a key (e.g. for
/// batch operations or testing with minimal KDF cost).
///
/// # Errors
///
/// - [`Error::Serialisation`] — if item serialisation fails.
/// - [`Error::Crypto`] — if encryption fails.
pub fn export_zvault_encrypted_with_key(
    items: &[VaultItem],
    key: &VaultKey,
    params: &KdfParams,
) -> Result<Vec<u8>> {
    // Serialise items to JSON.
    let json: Zeroizing<Vec<u8>> = Zeroizing::new(
        serde_json::to_vec(items)
            .map_err(|e| Error::Serialisation(format!("export serialisation error: {e}")))?,
    );

    // Encrypt using the provided key and params.
    let mut blob = crypto::encrypt_with_params(key, &json, params)?;

    // Replace vault magic with export magic.
    blob[..8].copy_from_slice(EXPORT_MAGIC);

    Ok(blob)
}

// ─── Plaintext JSON export ───────────────────────────────────────────────────

/// Export items as a plaintext JSON array.
///
/// Returns `Zeroizing<Vec<u8>>` to ensure the plaintext buffer is zeroed
/// when the caller drops it.
///
/// # Errors
///
/// Returns [`Error::Serialisation`] if serialisation fails.
pub fn export_json(items: &[VaultItem]) -> Result<Zeroizing<Vec<u8>>> {
    let json = serde_json::to_vec_pretty(items)
        .map_err(|e| Error::Serialisation(format!("JSON export error: {e}")))?;
    Ok(Zeroizing::new(json))
}

// ─── Plaintext CSV export ────────────────────────────────────────────────────

/// Export items as a plaintext CSV.
///
/// Columns: `name`, `url`, `username`, `password`, `notes`
///
/// Only Login items have meaningful values for `url`, `username`, `password`.
/// Other item types export with empty fields for those columns, but `notes`
/// is populated from the item's note field (or card/identity data serialised
/// as a string).
///
/// Returns `Zeroizing<Vec<u8>>` to ensure the plaintext buffer is zeroed
/// when the caller drops it.
///
/// # Errors
///
/// Returns [`Error::Serialisation`] if CSV writing fails.
pub fn export_csv(items: &[VaultItem]) -> Result<Zeroizing<Vec<u8>>> {
    let mut writer = csv::Writer::from_writer(Vec::new());

    // Write header.
    writer
        .write_record(["name", "url", "username", "password", "notes"])
        .map_err(|e| Error::Serialisation(format!("CSV write error: {e}")))?;

    for item in items {
        let url = item.uris.first().map_or("", |u| u.uri.as_str());
        let username = item.username.as_deref().unwrap_or("");
        let password = item.password.as_deref().unwrap_or("");
        let notes = item.note.as_deref().unwrap_or("");

        writer
            .write_record([item.name.as_str(), url, username, password, notes])
            .map_err(|e| Error::Serialisation(format!("CSV write error: {e}")))?;
    }

    let csv_bytes = writer
        .into_inner()
        .map_err(|e| Error::Serialisation(format!("CSV flush error: {e}")))?;

    Ok(Zeroizing::new(csv_bytes))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{IdentityFields, ItemKind, Uri, UriMatch};

    fn sample_items() -> Vec<VaultItem> {
        vec![
            {
                let mut item = VaultItem::new(ItemKind::Login, "GitHub");
                item.username = Some("alice@example.com".into());
                item.password = Some("hunter2".into());
                item.totp_secret = Some("JBSWY3DPEHPK3PXP".into());
                item.uris = vec![Uri {
                    uri: "https://github.com/login".into(),
                    r#match: UriMatch::Domain,
                }];
                item.note = Some("Primary account".into());
                item
            },
            {
                let mut item = VaultItem::new(ItemKind::SecureNote, "Recovery Codes");
                item.note = Some("code1\ncode2\ncode3".into());
                item
            },
            {
                let mut item = VaultItem::new(ItemKind::Card, "Visa");
                item.cardholder = Some("Alice Smith".into());
                item.card_number = Some("4111111111111111".into());
                item.expiry = Some("12/28".into());
                item.cvv = Some("123".into());
                item
            },
            {
                let mut item = VaultItem::new(ItemKind::Identity, "Personal");
                item.identity = Some(IdentityFields {
                    first_name: Some("Alice".into()),
                    last_name: Some("Smith".into()),
                    address: Some("123 Main St".into()),
                    city: Some("Portland".into()),
                    country: Some("US".into()),
                    phone: Some("+1-555-0100".into()),
                    email: Some("alice@example.com".into()),
                });
                item
            },
        ]
    }

    // ── Encrypted export tests ───────────────────────────────────────────────

    #[test]
    fn encrypted_export_has_correct_magic() {
        let items = sample_items();
        let blob = export_zvault_encrypted(&items, "password123").unwrap();
        assert_eq!(&blob[..8], EXPORT_MAGIC);
    }

    #[test]
    fn encrypted_export_roundtrip() {
        let items = sample_items();
        let password = "strong-passphrase-42";
        let blob = export_zvault_encrypted(&items, password).unwrap();

        let imported = crate::import::import_zvault_export(&blob, password).unwrap();
        assert_eq!(imported.len(), items.len());

        // Verify first item content.
        assert_eq!(imported[0].name, "GitHub");
        assert_eq!(imported[0].username.as_deref(), Some("alice@example.com"));
        assert_eq!(imported[0].password.as_deref(), Some("hunter2"));
    }

    #[test]
    fn encrypted_export_with_key_roundtrip() {
        let items = sample_items();
        let params = KdfParams {
            salt: [0x42u8; 32],
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        };
        let key = crypto::derive_key("testpw", &params).unwrap();

        let blob = export_zvault_encrypted_with_key(&items, &key, &params).unwrap();
        assert_eq!(&blob[..8], EXPORT_MAGIC);

        let imported = crate::import::import_zvault_export(&blob, "testpw").unwrap();
        assert_eq!(imported.len(), items.len());
    }

    #[test]
    fn encrypted_export_empty_items() {
        let blob = export_zvault_encrypted(&[], "password").unwrap();
        let imported = crate::import::import_zvault_export(&blob, "password").unwrap();
        assert!(imported.is_empty());
    }

    // ── JSON export tests ────────────────────────────────────────────────────

    #[test]
    fn json_export_valid_json() {
        let items = sample_items();
        let json = export_json(&items).unwrap();

        // Verify it's valid JSON by parsing it back.
        let parsed: Vec<VaultItem> = serde_json::from_slice(&json).unwrap();
        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0].name, "GitHub");
        assert_eq!(parsed[1].name, "Recovery Codes");
        assert_eq!(parsed[2].name, "Visa");
        assert_eq!(parsed[3].name, "Personal");
    }

    #[test]
    fn json_export_preserves_all_fields() {
        let items = sample_items();
        let json = export_json(&items).unwrap();
        let parsed: Vec<VaultItem> = serde_json::from_slice(&json).unwrap();

        // Login fields.
        assert_eq!(parsed[0].username.as_deref(), Some("alice@example.com"));
        assert_eq!(parsed[0].password.as_deref(), Some("hunter2"));
        assert_eq!(parsed[0].totp_secret.as_deref(), Some("JBSWY3DPEHPK3PXP"));
        assert_eq!(parsed[0].uris.len(), 1);

        // SecureNote.
        assert_eq!(parsed[1].note.as_deref(), Some("code1\ncode2\ncode3"));

        // Card.
        assert_eq!(parsed[2].card_number.as_deref(), Some("4111111111111111"));
        assert_eq!(parsed[2].cvv.as_deref(), Some("123"));

        // Identity.
        let ident = parsed[3].identity.as_ref().unwrap();
        assert_eq!(ident.first_name.as_deref(), Some("Alice"));
    }

    #[test]
    fn json_export_empty_items() {
        let json = export_json(&[]).unwrap();
        let parsed: Vec<VaultItem> = serde_json::from_slice(&json).unwrap();
        assert!(parsed.is_empty());
    }

    // ── CSV export tests ─────────────────────────────────────────────────────

    #[test]
    fn csv_export_has_correct_header() {
        let csv = export_csv(&[]).unwrap();
        let content = String::from_utf8(csv.to_vec()).unwrap();
        assert!(content.starts_with("name,url,username,password,notes"));
    }

    #[test]
    fn csv_export_login_item() {
        let items = vec![{
            let mut item = VaultItem::new(ItemKind::Login, "GitHub");
            item.username = Some("alice".into());
            item.password = Some("p@ss".into());
            item.uris = vec![Uri {
                uri: "https://github.com".into(),
                r#match: UriMatch::Domain,
            }];
            item.note = Some("notes here".into());
            item
        }];

        let csv = export_csv(&items).unwrap();
        let content = String::from_utf8(csv.to_vec()).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2); // header + 1 record
        assert_eq!(lines[1], "GitHub,https://github.com,alice,p@ss,notes here");
    }

    #[test]
    fn csv_export_multiple_items() {
        let items = sample_items();
        let csv_bytes = export_csv(&items).unwrap();

        // Parse back using csv reader to verify 4 records (ignoring header).
        let mut reader = csv::ReaderBuilder::new().from_reader(csv_bytes.as_slice());
        let records: Vec<_> = reader
            .records()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(records.len(), 4);
    }

    #[test]
    fn csv_export_escapes_commas_in_fields() {
        let items = vec![{
            let mut item = VaultItem::new(ItemKind::Login, "Site, Inc.");
            item.username = Some("user".into());
            item.password = Some("p,a,s,s".into());
            item
        }];

        let csv = export_csv(&items).unwrap();
        let content = String::from_utf8(csv.to_vec()).unwrap();
        // CSV should quote fields containing commas.
        assert!(content.contains("\"Site, Inc.\""));
        assert!(content.contains("\"p,a,s,s\""));
    }

    #[test]
    fn csv_export_roundtrip_via_import() {
        let items = vec![
            {
                let mut item = VaultItem::new(ItemKind::Login, "GitHub");
                item.username = Some("alice".into());
                item.password = Some("s3cr3t".into());
                item.uris = vec![Uri {
                    uri: "https://github.com".into(),
                    r#match: UriMatch::Domain,
                }];
                item.note = Some("my account".into());
                item
            },
            {
                let mut item = VaultItem::new(ItemKind::Login, "GitLab");
                item.username = Some("bob".into());
                item.password = Some("hunter2".into());
                item.uris = vec![Uri {
                    uri: "https://gitlab.com".into(),
                    r#match: UriMatch::Domain,
                }];
                item
            },
        ];

        let csv = export_csv(&items).unwrap();
        let imported = crate::import::import_csv(&csv).unwrap();

        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0].name, "GitHub");
        assert_eq!(imported[0].username.as_deref(), Some("alice"));
        assert_eq!(imported[0].password.as_deref(), Some("s3cr3t"));
        assert_eq!(imported[0].uris[0].uri, "https://github.com");
        assert_eq!(imported[0].note.as_deref(), Some("my account"));

        assert_eq!(imported[1].name, "GitLab");
        assert_eq!(imported[1].username.as_deref(), Some("bob"));
        assert_eq!(imported[1].password.as_deref(), Some("hunter2"));
    }
}
