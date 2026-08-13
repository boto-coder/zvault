//! Vault data model, serialisation, and CRUD operations.
//!
//! **M2 implementation target** for CRUD. The structs are defined here in M0
//! so all other modules can reference them without import cycles.
//!
//! # Core types
//!
//! - [`Vault`] — top-level container; holds items and the authorised device list.
//! - [`VaultItem`] — a single credential entry (Login, SecureNote, Card, or Identity).
//! - [`DeviceEntry`] — metadata about an authorised (or revoked) device.
//! - [`BiometricUnlockConfig`] — per-device config for biometric unlock (feature-gated).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

// ─── Item kind ───────────────────────────────────────────────────────────────

/// Discriminant for the four supported vault item types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// Username / password / TOTP / URIs.
    Login,
    /// Free-form encrypted note.
    SecureNote,
    /// Credit or debit card.
    Card,
    /// Personal identity fields (name, address, phone, email).
    Identity,
}

// ─── URI ─────────────────────────────────────────────────────────────────────

/// URI matching strategy for auto-fill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UriMatch {
    /// Match on the registered domain (e.g. `example.com`).
    Domain,
    /// Match on full host including subdomains.
    Host,
    /// Match if the page URI starts with this value.
    StartsWith,
    /// Exact string match.
    Exact,
    /// Regular expression match.
    Regex,
    /// Never auto-fill this URI.
    Never,
}

/// A URI associated with a Login item, with a configurable match strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Uri {
    /// The URI string.
    pub uri: String,
    /// How auto-fill should match this URI against the current page.
    pub r#match: UriMatch,
}

// ─── Identity fields ─────────────────────────────────────────────────────────

/// Personal identity data stored in an Identity vault item.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdentityFields {
    /// First / given name.
    pub first_name: Option<String>,
    /// Last / family name.
    pub last_name: Option<String>,
    /// Street address.
    pub address: Option<String>,
    /// City / locality.
    pub city: Option<String>,
    /// Country.
    pub country: Option<String>,
    /// Phone number.
    pub phone: Option<String>,
    /// Email address.
    pub email: Option<String>,
}

// ─── VaultItem ───────────────────────────────────────────────────────────────

/// A single credential entry inside a vault.
///
/// Fields are nullable; which fields are populated depends on [`ItemKind`].
///
/// Sensitive `String` fields (`password`, `totp_secret`, `note`,
/// `card_number`, `cvv`) are zeroed on drop via the manual `Drop` impl.
/// Non-zeroizable fields (`Uuid`, `DateTime<Utc>`) contain no secret material.
///
/// # ⚠ Clone warning
///
/// This type derives [`Clone`] because it is required for data-model
/// usability (e.g. passing items across API boundaries).  Each clone is an
/// independent allocation whose sensitive fields are zeroed independently
/// by its own `Drop`.  However, clones must be dropped promptly — do not
/// store clones in long-lived collections unless necessary.  Before M5
/// (desktop UI), this accepted risk will be re-evaluated.
///
/// Do not pass a cloned `VaultItem` to code that may log or serialise it
/// without encryption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultItem {
    /// Stable item identifier.
    pub id: Uuid,
    /// Item type discriminant.
    pub kind: ItemKind,
    /// Display name.
    pub name: String,
    /// Optional folder UUID.
    pub folder: Option<Uuid>,
    /// Whether this item is marked as a favourite.
    pub favourite: bool,
    /// Creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
    /// Last-update timestamp (UTC).
    pub updated_at: DateTime<Utc>,

    // Login fields
    /// Username. Present on Login items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Password. Present on Login items. Zeroed on drop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Base-32 TOTP secret. Present on Login items with TOTP. Zeroed on drop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totp_secret: Option<String>,
    /// Associated URIs for auto-fill matching.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uris: Vec<Uri>,

    // SecureNote fields
    /// Free-form note text. Present on SecureNote items. Zeroed on drop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,

    // Card fields
    /// Card number. Zeroed on drop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_number: Option<String>,
    /// Expiry date string (e.g. `"12/28"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<String>,
    /// CVV / security code. Zeroed on drop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cvv: Option<String>,
    /// Cardholder name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cardholder: Option<String>,

    // Identity fields
    /// Structured identity data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<IdentityFields>,
}

impl Drop for VaultItem {
    /// Zero all sensitive string fields on drop to prevent credential leakage.
    fn drop(&mut self) {
        if let Some(p) = &mut self.password {
            p.zeroize();
        }
        if let Some(t) = &mut self.totp_secret {
            t.zeroize();
        }
        if let Some(n) = &mut self.note {
            n.zeroize();
        }
        if let Some(c) = &mut self.card_number {
            c.zeroize();
        }
        if let Some(c) = &mut self.cvv {
            c.zeroize();
        }
    }
}

impl VaultItem {
    /// Create a new, empty item of the given kind with a random UUID and
    /// current timestamps.
    #[must_use]
    pub fn new(kind: ItemKind, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            kind,
            name: name.into(),
            folder: None,
            favourite: false,
            created_at: now,
            updated_at: now,
            username: None,
            password: None,
            totp_secret: None,
            uris: Vec::new(),
            note: None,
            card_number: None,
            expiry: None,
            cvv: None,
            cardholder: None,
            identity: None,
        }
    }
}

// ─── DeviceEntry ─────────────────────────────────────────────────────────────

/// Metadata for a device that has been admitted to (or revoked from) a vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceEntry {
    /// Stable per-device random identifier.
    pub device_id: Uuid,
    /// secp256k1 public key (hex-encoded, 64 chars).
    pub nostr_pubkey: String,
    /// Human-readable device label (e.g. `"Alice's MacBook Pro"`).
    pub label: String,
    /// When this device was admitted.
    pub added_at: DateTime<Utc>,
    /// Which device admitted this one.
    pub added_by: Uuid,
    /// Whether this device has been revoked.
    pub revoked: bool,
    /// When the device was revoked, if applicable.
    pub revoked_at: Option<DateTime<Utc>>,
    /// Which device performed the revocation.
    pub revoked_by: Option<Uuid>,
}

// ─── BiometricUnlockConfig ───────────────────────────────────────────────────

/// Per-device biometric unlock configuration.
///
/// Stored in OS secure storage (Keychain / Credential Manager / libsecret).
/// Never written to the vault file and never synced.
///
/// Only compiled on platforms that support biometric unlock
/// (`feature = "biometric"`).
#[cfg(feature = "biometric")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiometricUnlockConfig {
    /// Whether biometric unlock is currently enabled.
    pub enabled: bool,
    /// OS-specific key handle reference (Keychain item name / Keystore alias /
    /// Credential Manager target). Not the key material itself.
    pub key_handle: String,
    /// The vault master key encrypted with the OS-held biometric enclave key.
    /// AES-256-GCM ciphertext.
    pub wrapped_vault_key: Vec<u8>,
    /// IV used when wrapping the vault key (12 bytes for AES-GCM).
    pub iv: [u8; 12],
    /// When this config was created.
    pub created_at: DateTime<Utc>,
    /// When biometric unlock was last used successfully.
    pub last_used_at: Option<DateTime<Utc>>,
}

// ─── Vault ───────────────────────────────────────────────────────────────────

/// Top-level vault container.
///
/// Serialised to JSON and encrypted with AES-256-GCM before being written to
/// disk. The `version` field is incremented on every write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vault {
    /// Stable vault identifier (random UUID, never changes).
    pub id: Uuid,
    /// Monotonically increasing write counter. Incremented on every mutation.
    pub version: u64,
    /// Creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
    /// Last-mutation timestamp (UTC).
    pub updated_at: DateTime<Utc>,
    /// All credential items in this vault.
    pub items: Vec<VaultItem>,
    /// All devices that have been admitted to (or revoked from) this vault.
    pub devices: Vec<DeviceEntry>,
}

impl Vault {
    /// Create an empty vault with a new random UUID and current timestamps.
    #[must_use]
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            version: 0,
            created_at: now,
            updated_at: now,
            items: Vec::new(),
            devices: Vec::new(),
        }
    }

    // ── M2 CRUD ───────────────────────────────────────────────────────────

    /// Append a new item to the vault.
    ///
    /// Bumps [`Vault::version`] and updates [`Vault::updated_at`].
    pub fn add_item(&mut self, item: VaultItem) {
        self.items.push(item);
        self.version += 1;
        self.updated_at = Utc::now();
    }

    /// Replace an existing item (matched by [`VaultItem::id`]) with the
    /// supplied value.
    ///
    /// Bumps [`Vault::version`] and updates [`Vault::updated_at`].
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::ItemNotFound`] if no item with that ID exists.
    pub fn update_item(&mut self, item: VaultItem) -> crate::Result<()> {
        let id = item.id;
        let slot = self
            .items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or(crate::Error::ItemNotFound(id))?;
        *slot = item;
        self.version += 1;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Remove the item with the given UUID from the vault.
    ///
    /// Bumps [`Vault::version`] and updates [`Vault::updated_at`].
    ///
    /// Uses [`Vec::remove`] (order-preserving, O(n)) rather than
    /// `swap_remove` so that item order is stable across mutations.
    /// Stable order matters for deterministic JSON serialisation and for
    /// future CRDT merge logic in M4.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::ItemNotFound`] if no item with that ID exists.
    pub fn delete_item(&mut self, id: Uuid) -> crate::Result<()> {
        let pos = self
            .items
            .iter()
            .position(|i| i.id == id)
            .ok_or(crate::Error::ItemNotFound(id))?;
        self.items.remove(pos);
        self.version += 1;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Return a reference to the item with the given UUID, or `None`.
    #[must_use]
    pub fn get_item(&self, id: Uuid) -> Option<&VaultItem> {
        self.items.iter().find(|i| i.id == id)
    }

    /// Return a slice over all items in the vault.
    #[must_use]
    pub fn list_items(&self) -> &[VaultItem] {
        &self.items
    }

    // ── M2 Serialisation ──────────────────────────────────────────────────

    /// Serialise the vault to JSON bytes, wrapped in [`Zeroizing`] so the
    /// plaintext buffer is overwritten on drop.
    ///
    /// The resulting bytes are passed to the crypto layer for encryption.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Serialisation`] if serialisation fails.
    pub fn to_json(&self) -> crate::Result<Zeroizing<Vec<u8>>> {
        serde_json::to_vec(self)
            .map(Zeroizing::new)
            .map_err(|e| crate::Error::Serialisation(e.to_string()))
    }

    /// Deserialise a vault from JSON bytes produced by [`Vault::to_json`].
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Serialisation`] if deserialisation fails.
    pub fn from_json(data: &[u8]) -> crate::Result<Self> {
        serde_json::from_slice(data).map_err(|e| crate::Error::Serialisation(e.to_string()))
    }
}

impl Default for Vault {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Submodules ──────────────────────────────────────────────────────────────

pub mod vault_file;
pub use vault_file::VaultFile;

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn login_item(name: &str) -> VaultItem {
        let mut item = VaultItem::new(ItemKind::Login, name);
        item.username = Some("alice@example.com".into());
        item.password = Some("hunter2".into());
        item
    }

    // ── add_item / get_item / list_items ─────────────────────────────────────

    #[test]
    fn add_item_appends_and_is_retrievable() {
        let mut vault = Vault::new();
        let item = login_item("GitHub");
        let id = item.id;

        vault.add_item(item);

        assert!(vault.get_item(id).is_some());
        assert_eq!(vault.list_items().len(), 1);
        assert_eq!(vault.get_item(id).unwrap().name, "GitHub");
    }

    #[test]
    fn get_item_unknown_id_returns_none() {
        let vault = Vault::new();
        assert!(vault.get_item(Uuid::new_v4()).is_none());
    }

    #[test]
    fn list_items_empty_vault() {
        let vault = Vault::new();
        assert!(vault.list_items().is_empty());
    }

    #[test]
    fn list_items_returns_all() {
        let mut vault = Vault::new();
        vault.add_item(login_item("GitHub"));
        vault.add_item(login_item("GitLab"));
        assert_eq!(vault.list_items().len(), 2);
    }

    // ── update_item ──────────────────────────────────────────────────────────

    #[test]
    fn update_item_replaces_in_place() {
        let mut vault = Vault::new();
        let item = login_item("GitHub");
        let id = item.id;
        vault.add_item(item);

        let mut updated = login_item("GitHub Updated");
        updated.id = id; // same id, new name
        vault.update_item(updated).expect("update should succeed");

        assert_eq!(vault.get_item(id).unwrap().name, "GitHub Updated");
        assert_eq!(vault.list_items().len(), 1);
    }

    #[test]
    fn update_item_not_found_returns_error() {
        let mut vault = Vault::new();
        let missing = login_item("Ghost");

        let err = vault.update_item(missing).unwrap_err();
        assert!(
            matches!(err, crate::Error::ItemNotFound(_)),
            "expected ItemNotFound, got {err:?}"
        );
    }

    // ── delete_item ──────────────────────────────────────────────────────────

    #[test]
    fn delete_item_removes_it() {
        let mut vault = Vault::new();
        let item = login_item("GitHub");
        let id = item.id;
        vault.add_item(item);

        vault.delete_item(id).expect("delete should succeed");

        assert!(vault.get_item(id).is_none());
        assert!(vault.list_items().is_empty());
    }

    #[test]
    fn delete_item_not_found_returns_error() {
        let mut vault = Vault::new();

        let err = vault.delete_item(Uuid::new_v4()).unwrap_err();
        assert!(
            matches!(err, crate::Error::ItemNotFound(_)),
            "expected ItemNotFound, got {err:?}"
        );
    }

    // ── version bumps ────────────────────────────────────────────────────────

    #[test]
    fn add_item_bumps_version() {
        let mut vault = Vault::new();
        assert_eq!(vault.version, 0);
        vault.add_item(login_item("A"));
        assert_eq!(vault.version, 1);
        vault.add_item(login_item("B"));
        assert_eq!(vault.version, 2);
    }

    #[test]
    fn update_item_bumps_version() {
        let mut vault = Vault::new();
        let item = login_item("A");
        let id = item.id;
        vault.add_item(item); // version = 1

        let mut replacement = login_item("A v2");
        replacement.id = id;
        vault.update_item(replacement).unwrap(); // version = 2
        assert_eq!(vault.version, 2);
    }

    #[test]
    fn delete_item_bumps_version() {
        let mut vault = Vault::new();
        let item = login_item("A");
        let id = item.id;
        vault.add_item(item); // version = 1

        vault.delete_item(id).unwrap(); // version = 2
        assert_eq!(vault.version, 2);
    }

    // ── to_json / from_json round-trip ───────────────────────────────────────

    #[test]
    fn json_roundtrip_empty_vault() {
        let vault = Vault::new();
        let json = vault.to_json().expect("serialise should succeed");
        let restored = Vault::from_json(&json).expect("deserialise should succeed");

        assert_eq!(vault.id, restored.id);
        assert_eq!(vault.version, restored.version);
        assert!(restored.items.is_empty());
    }

    #[test]
    fn json_roundtrip_with_items() {
        let mut vault = Vault::new();
        let item = login_item("GitHub");
        let id = item.id;
        vault.add_item(item);

        let json = vault.to_json().expect("serialise should succeed");
        let restored = Vault::from_json(&json).expect("deserialise should succeed");

        assert_eq!(restored.list_items().len(), 1);
        assert_eq!(restored.get_item(id).unwrap().name, "GitHub");
        assert_eq!(
            restored.get_item(id).unwrap().username.as_deref(),
            Some("alice@example.com")
        );
    }

    #[test]
    fn from_json_invalid_bytes_returns_serialisation_error() {
        let err = Vault::from_json(b"not json at all").unwrap_err();
        assert!(
            matches!(err, crate::Error::Serialisation(_)),
            "expected Serialisation error, got {err:?}"
        );
    }

    // ── Edge-case tests (security review) ─────────────────────────────────────

    #[test]
    fn delete_item_preserves_order_of_remaining() {
        // Verify that Vec::remove (not swap_remove) is used: order of remaining
        // items must be stable after deletion.
        let mut vault = Vault::new();
        let a = login_item("A");
        let b = login_item("B");
        let c = login_item("C");
        let d = login_item("D");

        let id_a = a.id;
        let id_b = b.id;
        let id_c = c.id;
        let id_d = d.id;

        vault.add_item(a);
        vault.add_item(b);
        vault.add_item(c);
        vault.add_item(d);

        // Delete B (index 1).
        vault.delete_item(id_b).unwrap();

        // Remaining order must be [A, C, D] — NOT [A, D, C] (which swap_remove would produce).
        let items = vault.list_items();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].id, id_a);
        assert_eq!(items[1].id, id_c);
        assert_eq!(items[2].id, id_d);
    }
}

// ─── Property-based tests ────────────────────────────────────────────────────

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Strategy to generate a VaultItem with an arbitrary name.
    fn arb_vault_item() -> impl Strategy<Value = VaultItem> {
        "[a-zA-Z0-9 ]{1,64}".prop_map(|name| {
            let mut item = VaultItem::new(ItemKind::Login, name);
            item.username = Some("user@example.com".into());
            item.password = Some("secret123".into());
            item
        })
    }

    proptest! {
        /// Deleting one item must preserve all other items unchanged.
        #[test]
        fn delete_item_preserves_others(
            items in proptest::collection::vec(arb_vault_item(), 2..20),
            delete_idx in any::<proptest::sample::Index>(),
        ) {
            let mut vault = Vault::new();
            let mut ids = Vec::new();

            for item in &items {
                ids.push(item.id);
                vault.add_item(item.clone());
            }

            // Pick a valid index to delete.
            let idx = delete_idx.index(ids.len());
            let deleted_id = ids[idx];

            vault.delete_item(deleted_id).unwrap();

            // All other items must still be present, in order.
            let remaining: Vec<Uuid> = vault.list_items().iter().map(|i| i.id).collect();
            let expected: Vec<Uuid> = ids.iter().copied().filter(|id| *id != deleted_id).collect();
            prop_assert_eq!(remaining, expected);
        }

        /// Adding items always increases the count by exactly one.
        #[test]
        fn add_item_increases_count(name in "[a-zA-Z0-9]{1,32}") {
            let mut vault = Vault::new();
            let before = vault.list_items().len();
            vault.add_item(VaultItem::new(ItemKind::SecureNote, name));
            prop_assert_eq!(vault.list_items().len(), before + 1);
        }

        /// from_json never panics on arbitrary data.
        #[test]
        fn from_json_never_panics(data in proptest::collection::vec(any::<u8>(), 0..2048)) {
            let _ = Vault::from_json(&data);
        }
    }
}
