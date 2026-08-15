//! Integration tests for the device pairing (invite/join-request) flows.
//!
//! These tests exercise the full pairing protocol end-to-end without network I/O:
//! 1. Full invite flow (A invites B)
//! 2. Full join-request flow (B requests, A accepts)
//! 3. Backward compatibility with manual admit
//! 4. Property tests for encode/decode round-trip

use uuid::Uuid;
use zvault_core::device::{DeviceIdentity, DeviceManager, InMemoryStorage};
use zvault_core::pairing::{
    create_invite, create_invite_response, create_join_request, create_join_response,
    decode_pairing_code, encode_pairing_code, PairingType,
};
use zvault_core::vault::Vault;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Create a device identity using InMemoryStorage.
fn create_device(label: &str) -> (DeviceIdentity, zvault_core::device::DeviceKeyMaterial) {
    let storage = InMemoryStorage::default();
    DeviceIdentity::generate(label, &storage).expect("generate device")
}

// ─── Test: Full Invite Flow ──────────────────────────────────────────────────

/// Full invite flow:
/// 1. Device A creates a vault and generates an invite code
/// 2. Device B decodes the invite code
/// 3. Device B generates an invite-response code
/// 4. Device A decodes the response and admits B
/// 5. Both devices are in the vault
#[test]
fn full_invite_flow() {
    // Device A creates vault
    let mut vault = Vault::new();
    let (identity_a, material_a) = create_device("Admin Laptop");
    let mut dm = DeviceManager::from_vault(&vault);
    dm.bootstrap(&material_a).unwrap();
    dm.flush(&mut vault);

    // Device A generates invite code
    let invite_payload =
        create_invite(&identity_a.pubkey_hex, "Admin Laptop", vault.vault_id).unwrap();
    let invite_code = encode_pairing_code(&invite_payload).unwrap();

    // Verify code is under 500 chars
    assert!(
        invite_code.len() <= 500,
        "invite code too long: {}",
        invite_code.len()
    );
    assert!(invite_code.starts_with("zvault:"));

    // Device B decodes the invite code
    let decoded = decode_pairing_code(&invite_code).unwrap();
    assert_eq!(decoded.t, PairingType::Invite);
    assert_eq!(decoded.p, identity_a.pubkey_hex);
    assert_eq!(decoded.l, "Admin Laptop");
    assert_eq!(decoded.vid, Some(vault.vault_id));

    // Device B generates response
    let (_identity_b, material_b) = create_device("Bob's Phone");
    let response_payload =
        create_invite_response(&material_b.pubkey_hex, &material_b.label).unwrap();
    let response_code = encode_pairing_code(&response_payload).unwrap();
    assert!(response_code.len() <= 500);

    // Device A decodes the response
    let decoded_response = decode_pairing_code(&response_code).unwrap();
    assert_eq!(decoded_response.t, PairingType::InviteResponse);
    assert_eq!(decoded_response.p, material_b.pubkey_hex);
    assert_eq!(decoded_response.l, material_b.label);

    // Device A admits B
    let admin_identity = DeviceIdentity {
        device_id: identity_a.device_id,
        pubkey_hex: identity_a.pubkey_hex.clone(),
    };
    let mut dm = DeviceManager::from_vault(&vault);
    dm.admit(&material_b, &admin_identity).unwrap();
    dm.flush(&mut vault);

    // Both devices in vault
    assert_eq!(vault.devices.len(), 2);
    assert_eq!(vault.devices.iter().filter(|d| !d.revoked).count(), 2);
    // Verify B's pubkey is in the vault
    assert!(vault
        .devices
        .iter()
        .any(|d| d.nostr_pubkey == material_b.pubkey_hex));
}

// ─── Test: Full Join-Request Flow ────────────────────────────────────────────

/// Full join-request flow:
/// 1. Device B generates a join-request code
/// 2. Device A (admin) decodes the join-request
/// 3. Device A generates a join-response and admits B
/// 4. Device B decodes the response and confirms
#[test]
fn full_join_request_flow() {
    // Device A creates vault
    let mut vault = Vault::new();
    let (identity_a, material_a) = create_device("Admin Desktop");
    let mut dm = DeviceManager::from_vault(&vault);
    dm.bootstrap(&material_a).unwrap();
    dm.flush(&mut vault);

    // Device B generates join-request
    let (_identity_b, material_b) = create_device("Bob's Tablet");
    let request_payload = create_join_request(&material_b.pubkey_hex, &material_b.label).unwrap();
    let request_code = encode_pairing_code(&request_payload).unwrap();
    assert!(request_code.len() <= 500);

    // Device A decodes the join-request
    let decoded = decode_pairing_code(&request_code).unwrap();
    assert_eq!(decoded.t, PairingType::JoinRequest);
    assert_eq!(decoded.p, material_b.pubkey_hex);
    assert_eq!(decoded.l, material_b.label);
    assert!(decoded.vid.is_none()); // B doesn't know the vault ID yet

    // Device A admits B and generates response
    let admin_identity = DeviceIdentity {
        device_id: identity_a.device_id,
        pubkey_hex: identity_a.pubkey_hex.clone(),
    };
    let mut dm = DeviceManager::from_vault(&vault);
    dm.admit(&material_b, &admin_identity).unwrap();
    dm.flush(&mut vault);

    let response_payload =
        create_join_response(&identity_a.pubkey_hex, "Admin Desktop", vault.vault_id).unwrap();
    let response_code = encode_pairing_code(&response_payload).unwrap();
    assert!(response_code.len() <= 500);

    // Device B decodes the response
    let decoded_response = decode_pairing_code(&response_code).unwrap();
    assert_eq!(decoded_response.t, PairingType::JoinResponse);
    assert_eq!(decoded_response.p, identity_a.pubkey_hex);
    assert_eq!(decoded_response.vid, Some(vault.vault_id));

    // Both devices in vault
    assert_eq!(vault.devices.len(), 2);
    assert!(vault
        .devices
        .iter()
        .any(|d| d.nostr_pubkey == material_b.pubkey_hex && !d.revoked));
}

// ─── Test: Backward Compat with Manual Admit ─────────────────────────────────

/// The old manual admit flow still works alongside pairing codes.
#[test]
fn backward_compat_manual_admit() {
    let mut vault = Vault::new();
    let (_identity_a, material_a) = create_device("Admin");
    let mut dm = DeviceManager::from_vault(&vault);
    dm.bootstrap(&material_a).unwrap();
    dm.flush(&mut vault);

    // Manual admit by directly constructing a DeviceEntry (the old way).
    let now = chrono::Utc::now();
    let manual_device_id = Uuid::new_v4();
    let manual_pubkey = "b".repeat(64);
    vault.devices.push(zvault_core::vault::DeviceEntry {
        device_id: manual_device_id,
        nostr_pubkey: manual_pubkey.clone(),
        label: "Manual Device".into(),
        added_at: now,
        added_by: material_a.device_id,
        revoked: false,
        revoked_at: None,
        revoked_by: None,
    });
    vault.version += 1;

    // Verify the manually admitted device is live
    let dm2 = DeviceManager::from_vault(&vault);
    assert_eq!(dm2.live_devices().len(), 2);
    assert!(dm2
        .live_devices()
        .iter()
        .any(|d| d.nostr_pubkey == manual_pubkey));

    // Now also admit a device via pairing
    let (_identity_c, material_c) = create_device("Paired Device");
    let admin_identity = DeviceIdentity {
        device_id: material_a.device_id,
        pubkey_hex: material_a.pubkey_hex.clone(),
    };
    let mut dm3 = DeviceManager::from_vault(&vault);
    dm3.admit(&material_c, &admin_identity).unwrap();
    dm3.flush(&mut vault);

    // All three devices live
    assert_eq!(vault.devices.len(), 3);
    assert_eq!(vault.devices.iter().filter(|d| !d.revoked).count(), 3);
}

// ─── Test: vault_id field backward compat ────────────────────────────────────

/// Vaults serialised without vault_id get a random one on deserialisation.
#[test]
fn vault_id_backward_compat() {
    // Simulate old vault JSON without vault_id
    let old_json = r#"{
        "id": "12345678-1234-1234-1234-123456789012",
        "version": 5,
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
        "items": [],
        "devices": []
    }"#;
    let vault: Vault = serde_json::from_str(old_json).unwrap();
    // vault_id should be generated (not nil)
    assert!(!vault.vault_id.is_nil());
    // The id field is preserved
    assert_eq!(vault.id.to_string(), "12345678-1234-1234-1234-123456789012");
}

/// New vaults always have a vault_id.
#[test]
fn new_vault_has_vault_id() {
    let vault = Vault::new();
    assert!(!vault.vault_id.is_nil());
    assert_ne!(vault.vault_id, vault.id); // distinct UUIDs
}

// ─── Property tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Strategy for valid hex pubkey (64 hex chars).
    fn arb_pubkey() -> impl Strategy<Value = String> {
        "[0-9a-f]{64}"
    }

    /// Strategy for valid label (1-64 printable chars, at least one non-space).
    fn arb_label() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_'-][a-zA-Z0-9 _'-]{0,63}"
    }

    proptest! {
        /// Encode/decode is a lossless round-trip for invite payloads.
        #[test]
        fn invite_roundtrip(pubkey in arb_pubkey(), label in arb_label()) {
            let vault_id = Uuid::new_v4();
            let payload = create_invite(&pubkey, &label, vault_id).unwrap();
            let code = encode_pairing_code(&payload).unwrap();
            let decoded = decode_pairing_code(&code).unwrap();
            prop_assert_eq!(decoded.p, pubkey);
            prop_assert_eq!(decoded.l, label.trim().to_string());
            prop_assert_eq!(decoded.vid, Some(vault_id));
            prop_assert_eq!(decoded.t, PairingType::Invite);
        }

        /// Encode/decode is a lossless round-trip for join-request payloads.
        #[test]
        fn join_request_roundtrip(pubkey in arb_pubkey(), label in arb_label()) {
            let payload = create_join_request(&pubkey, &label).unwrap();
            let code = encode_pairing_code(&payload).unwrap();
            let decoded = decode_pairing_code(&code).unwrap();
            prop_assert_eq!(decoded.p, pubkey);
            prop_assert_eq!(decoded.l, label.trim().to_string());
            prop_assert!(decoded.vid.is_none());
            prop_assert_eq!(decoded.t, PairingType::JoinRequest);
        }

        /// decode_pairing_code never panics on arbitrary input.
        #[test]
        fn decode_never_panics(data in "\\PC{0,600}") {
            let _ = decode_pairing_code(&data);
        }
    }
}
