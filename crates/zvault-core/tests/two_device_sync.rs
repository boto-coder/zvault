//! Full two-device sync integration test.
//!
//! Simulates the complete lifecycle:
//! 1. Device A creates a vault and saves an item
//! 2. Device A invites Device B (admits it to the device list)
//! 3. Device A sends a full sync message to Device B
//! 4. Device B receives and applies the sync → verifies it got the item
//! 5. Device A updates the item
//! 6. Device A sends another sync to Device B
//! 7. Device B receives → verifies the update was applied

use zvault_core::device::{DeviceIdentity, DeviceKeyMaterial, DeviceManager, InMemoryStorage};
use zvault_core::nostr;
use zvault_core::sync::{self, LamportClock, SyncOp};
use zvault_core::vault::{ItemKind, Vault, VaultItem};

/// Helper: generate a device identity and return (identity, material, secret_key, pubkey_hex, storage).
fn create_device(
    label: &str,
) -> (
    DeviceIdentity,
    DeviceKeyMaterial,
    Vec<u8>,
    String,
    InMemoryStorage,
) {
    let storage = InMemoryStorage::default();
    let (identity, material) = DeviceIdentity::generate(label, &storage).unwrap();
    let sk = identity.load_secret_key(&storage).unwrap().to_vec();
    let pubkey = identity.pubkey_hex.clone();
    (identity, material, sk, pubkey, storage)
}

#[test]
fn full_two_device_sync_cycle() {
    // ═══════════════════════════════════════════════════════════════════════════
    // STEP 1: Device A creates a vault and adds an item
    // ═══════════════════════════════════════════════════════════════════════════
    let (identity_a, material_a, sk_a, pubkey_a, _storage_a) = create_device("Device A - Laptop");
    let (_identity_b, material_b, sk_b, pubkey_b, _storage_b) = create_device("Device B - Phone");

    let mut vault_a = Vault::new();
    let mut clock_a = LamportClock::new();

    // Bootstrap device A as the first device
    let mut dm_a = DeviceManager::from_vault(&vault_a);
    dm_a.bootstrap(&material_a).unwrap();
    dm_a.flush(&mut vault_a);

    // Add a login item
    let mut github_item = VaultItem::new(ItemKind::Login, "GitHub");
    github_item.username = Some("alice@example.com".into());
    github_item.password = Some("super-secret-password-123".into());
    github_item.uris = vec![zvault_core::vault::Uri {
        uri: "https://github.com/login".into(),
        r#match: zvault_core::vault::UriMatch::Domain,
    }];
    let item_id = github_item.id;
    vault_a.add_item(github_item);

    assert_eq!(vault_a.items.len(), 1);
    assert_eq!(vault_a.version, 2); // bootstrap flush + add_item

    // ═══════════════════════════════════════════════════════════════════════════
    // STEP 2: Device A admits Device B
    // ═══════════════════════════════════════════════════════════════════════════
    let mut dm_a = DeviceManager::from_vault(&vault_a);
    dm_a.admit(&material_b, &identity_a).unwrap();
    dm_a.flush(&mut vault_a);

    assert_eq!(vault_a.devices.len(), 2);
    let live_devices: Vec<_> = vault_a.devices.iter().filter(|d| !d.revoked).collect();
    assert_eq!(live_devices.len(), 2, "both devices should be live");

    // ═══════════════════════════════════════════════════════════════════════════
    // STEP 3: Device A sends a full sync message to Device B
    // ═══════════════════════════════════════════════════════════════════════════
    let sync_msg_1 = sync::build_full_sync_message(
        &vault_a,
        &mut clock_a,
        identity_a.device_id,
        &sk_a,
        &pubkey_b,
    )
    .unwrap();

    assert_eq!(sync_msg_1.op, SyncOp::Full);
    assert_eq!(sync_msg_1.vault_id, vault_a.id);
    assert_eq!(sync_msg_1.sender, identity_a.device_id);
    assert_eq!(sync_msg_1.clock, 1);

    // ═══════════════════════════════════════════════════════════════════════════
    // STEP 4: Device B receives the sync and verifies it got the item
    // ═══════════════════════════════════════════════════════════════════════════

    // Device B starts with a bare vault (same ID, but empty — simulating
    // receiving the vault for the first time after being admitted)
    let mut vault_b = Vault::new();
    vault_b.id = vault_a.id; // same vault identity
    vault_b.version = 0;
    // Device B knows about both devices (it was told during admit)
    vault_b.devices = vault_a.devices.clone();

    let mut clock_b = LamportClock::new();

    sync::apply_sync_message(&mut vault_b, &sync_msg_1, &mut clock_b, &sk_b, &pubkey_a).unwrap();

    // Verify: Device B now has the item
    assert_eq!(
        vault_b.items.len(),
        1,
        "Device B should have 1 item after sync"
    );
    let received_item = vault_b.get_item(item_id).expect("item should exist on B");
    assert_eq!(received_item.name, "GitHub");
    assert_eq!(received_item.username.as_deref(), Some("alice@example.com"));
    assert_eq!(
        received_item.password.as_deref(),
        Some("super-secret-password-123")
    );
    assert_eq!(received_item.uris.len(), 1);
    assert_eq!(received_item.uris[0].uri, "https://github.com/login");

    // Verify: version and clock updated
    assert!(
        vault_b.version >= vault_a.version,
        "B's version should be >= A's version after sync"
    );
    assert!(clock_b.0 > 0, "B's clock should have advanced");

    // ═══════════════════════════════════════════════════════════════════════════
    // STEP 5: Device A updates the item
    // ═══════════════════════════════════════════════════════════════════════════
    let mut updated_item = VaultItem::new(ItemKind::Login, "GitHub");
    updated_item.id = item_id; // same ID
    updated_item.username = Some("alice@example.com".into());
    updated_item.password = Some("new-rotated-password-456".into());
    updated_item.uris = vec![zvault_core::vault::Uri {
        uri: "https://github.com/login".into(),
        r#match: zvault_core::vault::UriMatch::Domain,
    }];
    // Set updated_at to be newer (the constructor sets it to now, which is fine)
    vault_a.update_item(updated_item).unwrap();

    assert_eq!(vault_a.version, 4); // previous version was 3, +1 for update

    // ═══════════════════════════════════════════════════════════════════════════
    // STEP 6: Device A sends another sync to Device B
    // ═══════════════════════════════════════════════════════════════════════════
    let sync_msg_2 = sync::build_full_sync_message(
        &vault_a,
        &mut clock_a,
        identity_a.device_id,
        &sk_a,
        &pubkey_b,
    )
    .unwrap();

    assert_eq!(sync_msg_2.clock, 2);
    assert!(sync_msg_2.vault_version > sync_msg_1.vault_version);

    // ═══════════════════════════════════════════════════════════════════════════
    // STEP 7: Device B receives the update and verifies the password changed
    // ═══════════════════════════════════════════════════════════════════════════
    sync::apply_sync_message(&mut vault_b, &sync_msg_2, &mut clock_b, &sk_b, &pubkey_a).unwrap();

    let updated_on_b = vault_b
        .get_item(item_id)
        .expect("item should still exist on B");
    assert_eq!(updated_on_b.name, "GitHub");
    assert_eq!(
        updated_on_b.password.as_deref(),
        Some("new-rotated-password-456"),
        "Device B should see the updated password after second sync"
    );

    // Verify version propagated
    assert!(
        vault_b.version >= 4,
        "B's version should reflect A's update"
    );

    // ═══════════════════════════════════════════════════════════════════════════
    // BONUS: Verify a stale replay of sync_msg_1 is ignored
    // ═══════════════════════════════════════════════════════════════════════════
    let items_before = vault_b.items.len();
    let version_before = vault_b.version;

    // Replaying the old message should be silently discarded (stale guard)
    sync::apply_sync_message(&mut vault_b, &sync_msg_1, &mut clock_b, &sk_b, &pubkey_a).unwrap();

    assert_eq!(
        vault_b.items.len(),
        items_before,
        "replay should not duplicate items"
    );
    assert_eq!(
        vault_b.version, version_before,
        "replay should not change version"
    );
    // But clock still advances
    assert!(clock_b.0 > 2, "clock advances even on stale message");

    println!("✅ Full two-device sync cycle passed!");
    println!("   - Device A created vault + item");
    println!("   - Device A admitted Device B");
    println!("   - Sync 1: A→B transferred item successfully");
    println!("   - Device A updated password");
    println!("   - Sync 2: A→B propagated update");
    println!("   - Stale replay correctly ignored");
}

/// Test that a revoked device's sync messages are rejected.
#[test]
fn revoked_device_sync_rejected() {
    let (identity_a, material_a, sk_a, pubkey_a, _) = create_device("Admin");
    let (identity_b, material_b, sk_b, pubkey_b, _) = create_device("Compromised");

    // Setup: A creates vault, admits B
    let mut vault = Vault::new();
    let mut dm = DeviceManager::from_vault(&vault);
    dm.bootstrap(&material_a).unwrap();
    dm.admit(&material_b, &identity_a).unwrap();
    dm.flush(&mut vault);

    // B has a copy and tries to send a sync
    let mut vault_b = vault.clone();
    vault_b.add_item(VaultItem::new(ItemKind::SecureNote, "Malicious Note"));

    let mut clock_b = LamportClock::new();
    let msg = sync::build_full_sync_message(
        &vault_b,
        &mut clock_b,
        identity_b.device_id,
        &sk_b,
        &pubkey_a,
    )
    .unwrap();

    // A revokes B
    let mut dm = DeviceManager::from_vault(&vault);
    dm.revoke(identity_b.device_id, &identity_a).unwrap();
    dm.flush(&mut vault);

    // Now A tries to apply B's sync — should be rejected
    let mut clock_a = LamportClock::new();
    let result = sync::apply_sync_message(&mut vault, &msg, &mut clock_a, &sk_a, &pubkey_b);

    assert!(result.is_err(), "sync from revoked device must be rejected");
    assert_eq!(
        vault.items.len(),
        0,
        "no items should have been added from revoked device"
    );
}

/// Test NIP-59 gift-wrap end-to-end: A wraps a message for B, B unwraps it.
#[test]
fn gift_wrap_sync_message_end_to_end() {
    let (_identity_a, _material_a, sk_a, _pubkey_a, _) = create_device("Sender");
    let (_identity_b, _material_b, sk_b, pubkey_b, _) = create_device("Recipient");

    let sender_sk = zeroize::Zeroizing::new(sk_a.clone());
    let recipient_sk = zeroize::Zeroizing::new(sk_b);

    // A wraps a sync message for B
    let wrapped = nostr::gift_wrap(
        &sender_sk,
        &pubkey_b,
        "encrypted vault sync payload here",
        10050,
        &[vec!["vault_id".to_string(), "test-vault-123".to_string()]],
    )
    .unwrap();

    // The gift-wrap hides the real sender
    assert_eq!(wrapped.kind, 1059);

    // B unwraps
    let rumor = nostr::unwrap_gift_wrap(&recipient_sk, &wrapped).unwrap();
    assert_eq!(rumor.content, "encrypted vault sync payload here");
    assert_eq!(rumor.kind, 10050);

    println!("✅ Gift-wrap end-to-end sync verified");
}

/// Three-device sync: A creates vault + item, syncs to B.
/// B adds another item, then B invites C and syncs.
/// C should have all items from both A and B.
#[test]
fn three_device_sync_b_adds_item_invites_c() {
    // ═══════════════════════════════════════════════════════════════════════════
    // Setup: Create three devices
    // ═══════════════════════════════════════════════════════════════════════════
    let (identity_a, material_a, sk_a, pubkey_a, _) = create_device("Device A - Desktop");
    let (identity_b, material_b, sk_b, pubkey_b, _) = create_device("Device B - Laptop");
    let (identity_c, material_c, sk_c, pubkey_c, _) = create_device("Device C - Phone");

    // ═══════════════════════════════════════════════════════════════════════════
    // STEP 1: Device A creates vault, adds item, admits B
    // ═══════════════════════════════════════════════════════════════════════════
    let mut vault_a = Vault::new();
    let mut clock_a = LamportClock::new();

    let mut dm = DeviceManager::from_vault(&vault_a);
    dm.bootstrap(&material_a).unwrap();
    dm.admit(&material_b, &identity_a).unwrap();
    dm.flush(&mut vault_a);

    let mut item_from_a = VaultItem::new(ItemKind::Login, "GitHub");
    item_from_a.username = Some("alice@example.com".into());
    item_from_a.password = Some("github-pass-from-A".into());
    let item_a_id = item_from_a.id;
    vault_a.add_item(item_from_a);

    // ═══════════════════════════════════════════════════════════════════════════
    // STEP 2: A syncs to B
    // ═══════════════════════════════════════════════════════════════════════════
    let msg_a_to_b = sync::build_full_sync_message(
        &vault_a,
        &mut clock_a,
        identity_a.device_id,
        &sk_a,
        &pubkey_b,
    )
    .unwrap();

    // B starts with empty vault (same ID)
    let mut vault_b = Vault::new();
    vault_b.id = vault_a.id;
    vault_b.devices = vault_a.devices.clone();
    let mut clock_b = LamportClock::new();

    sync::apply_sync_message(&mut vault_b, &msg_a_to_b, &mut clock_b, &sk_b, &pubkey_a).unwrap();

    assert_eq!(vault_b.items.len(), 1, "B should have A's item");
    assert_eq!(vault_b.get_item(item_a_id).unwrap().name, "GitHub");

    // ═══════════════════════════════════════════════════════════════════════════
    // STEP 3: Device B adds its own item
    // ═══════════════════════════════════════════════════════════════════════════
    let mut item_from_b = VaultItem::new(ItemKind::Login, "GitLab");
    item_from_b.username = Some("bob@example.com".into());
    item_from_b.password = Some("gitlab-pass-from-B".into());
    let item_b_id = item_from_b.id;
    vault_b.add_item(item_from_b);

    assert_eq!(vault_b.items.len(), 2, "B now has 2 items (A's + B's own)");

    // ═══════════════════════════════════════════════════════════════════════════
    // STEP 4: Device B admits Device C
    // ═══════════════════════════════════════════════════════════════════════════
    let mut dm_b = DeviceManager::from_vault(&vault_b);
    dm_b.admit(&material_c, &identity_b).unwrap();
    dm_b.flush(&mut vault_b);

    assert_eq!(vault_b.devices.len(), 3, "B's vault should have 3 devices");
    let live: Vec<_> = vault_b.devices.iter().filter(|d| !d.revoked).collect();
    assert_eq!(live.len(), 3, "all 3 devices should be live");

    // Verify C was admitted by B
    let c_entry = vault_b
        .devices
        .iter()
        .find(|d| d.device_id == identity_c.device_id)
        .unwrap();
    assert_eq!(c_entry.added_by, identity_b.device_id);

    // ═══════════════════════════════════════════════════════════════════════════
    // STEP 5: B syncs to C — C should get ALL items (from A and B)
    // ═══════════════════════════════════════════════════════════════════════════
    let msg_b_to_c = sync::build_full_sync_message(
        &vault_b,
        &mut clock_b,
        identity_b.device_id,
        &sk_b,
        &pubkey_c,
    )
    .unwrap();

    // C starts with empty vault (same ID, knows all devices)
    let mut vault_c = Vault::new();
    vault_c.id = vault_b.id;
    vault_c.devices = vault_b.devices.clone();
    let mut clock_c = LamportClock::new();

    sync::apply_sync_message(&mut vault_c, &msg_b_to_c, &mut clock_c, &sk_c, &pubkey_b).unwrap();

    // ═══════════════════════════════════════════════════════════════════════════
    // VERIFY: C has all items from both A and B
    // ═══════════════════════════════════════════════════════════════════════════
    assert_eq!(
        vault_c.items.len(),
        2,
        "Device C should have 2 items (from A and B)"
    );

    // Check item from A
    let item_a_on_c = vault_c
        .get_item(item_a_id)
        .expect("A's item should be on C");
    assert_eq!(item_a_on_c.name, "GitHub");
    assert_eq!(item_a_on_c.username.as_deref(), Some("alice@example.com"));
    assert_eq!(item_a_on_c.password.as_deref(), Some("github-pass-from-A"));

    // Check item from B
    let item_b_on_c = vault_c
        .get_item(item_b_id)
        .expect("B's item should be on C");
    assert_eq!(item_b_on_c.name, "GitLab");
    assert_eq!(item_b_on_c.username.as_deref(), Some("bob@example.com"));
    assert_eq!(item_b_on_c.password.as_deref(), Some("gitlab-pass-from-B"));

    // Verify C knows about all 3 devices
    assert_eq!(vault_c.devices.len(), 3);

    // ═══════════════════════════════════════════════════════════════════════════
    // BONUS: C can also sync back to A (bidirectional trust)
    // ═══════════════════════════════════════════════════════════════════════════
    // First, A needs to know about C (A's device list is stale — only knows A+B)
    // B syncs device list update back to A
    let msg_b_to_a = sync::build_full_sync_message(
        &vault_b,
        &mut clock_b,
        identity_b.device_id,
        &sk_b,
        &pubkey_a,
    )
    .unwrap();

    sync::apply_sync_message(&mut vault_a, &msg_b_to_a, &mut clock_a, &sk_a, &pubkey_b).unwrap();

    // Now A knows about C and has B's item too
    assert_eq!(vault_a.devices.len(), 3, "A should now know about C");
    assert_eq!(vault_a.items.len(), 2, "A should have B's item too");
    assert!(
        vault_a.get_item(item_b_id).is_some(),
        "A should have GitLab item from B"
    );

    println!("✅ Three-device sync cycle passed!");
    println!("   - A created vault + GitHub item, admitted B");
    println!("   - A synced to B — B received GitHub item");
    println!("   - B added GitLab item");
    println!("   - B admitted C");
    println!("   - B synced to C — C received BOTH items (GitHub + GitLab)");
    println!("   - B synced back to A — A received GitLab + device C info");
    println!("   - Full mesh convergence achieved!");
}

/// Full Nostr protocol test: sync messages wrapped in NIP-59 gift-wrap.
/// This exercises the complete protocol stack as it would work in production:
/// NIP-44 encrypt → NIP-01 sign → NIP-59 gift-wrap → unwrap → decrypt → merge
#[test]
fn full_nostr_protocol_sync_with_gift_wrap() {
    use zeroize::Zeroizing;

    let (identity_a, material_a, sk_a, pubkey_a, _) = create_device("Device A");
    let (_identity_b, material_b, sk_b, pubkey_b, _) = create_device("Device B");

    // ── Setup vault on A ──────────────────────────────────────────────────────
    let mut vault_a = Vault::new();
    let mut dm = DeviceManager::from_vault(&vault_a);
    dm.bootstrap(&material_a).unwrap();
    dm.admit(&material_b, &identity_a).unwrap();
    dm.flush(&mut vault_a);

    let mut item = VaultItem::new(ItemKind::Login, "AWS Console");
    item.username = Some("admin@company.com".into());
    item.password = Some("Pr0d-Acc3ss-K3y!".into());
    item.totp_secret = Some("JBSWY3DPEHPK3PXP".into());
    let item_id = item.id;
    vault_a.add_item(item);

    // ── Build sync message ────────────────────────────────────────────────────
    let mut clock_a = LamportClock::new();
    let sync_msg = sync::build_full_sync_message(
        &vault_a,
        &mut clock_a,
        identity_a.device_id,
        &sk_a,
        &pubkey_b,
    )
    .unwrap();

    // ── Wrap in NIP-59 gift-wrap (as would happen before publishing to relay) ─
    let sync_json = serde_json::to_string(&sync_msg).unwrap();
    let sender_sk = Zeroizing::new(sk_a.clone());

    let gift_wrapped_event = nostr::gift_wrap(
        &sender_sk,
        &pubkey_b,
        &sync_json,
        10050, // custom vault sync kind
        &[vec!["p".to_string(), pubkey_b.clone()]],
    )
    .unwrap();

    // Verify gift-wrap properties
    assert_eq!(gift_wrapped_event.kind, 1059, "must be gift-wrap kind");
    assert_ne!(
        gift_wrapped_event.pubkey, pubkey_a,
        "outer event must use ephemeral key, not sender's real key"
    );

    // ── Simulate relay delivery: B receives the gift-wrapped event ────────────
    let recipient_sk = Zeroizing::new(sk_b.clone());

    // B unwraps the gift-wrap to get the rumor (inner event with sync payload)
    let rumor = nostr::unwrap_gift_wrap(&recipient_sk, &gift_wrapped_event).unwrap();

    assert_eq!(rumor.kind, 10050, "inner event should be vault sync kind");
    assert_eq!(
        rumor.pubkey, pubkey_a,
        "rumor should reveal the true sender (A)"
    );

    // ── B parses the sync message from the rumor content ──────────────────────
    let received_sync_msg: sync::SyncMessage = serde_json::from_str(&rumor.content).unwrap();

    assert_eq!(received_sync_msg.sender, identity_a.device_id);
    assert_eq!(received_sync_msg.vault_id, vault_a.id);
    assert_eq!(received_sync_msg.op, SyncOp::Full);

    // ── B applies the sync message ────────────────────────────────────────────
    let mut vault_b = Vault::new();
    vault_b.id = vault_a.id;
    vault_b.devices = vault_a.devices.clone();
    let mut clock_b = LamportClock::new();

    sync::apply_sync_message(
        &mut vault_b,
        &received_sync_msg,
        &mut clock_b,
        &sk_b,
        &pubkey_a,
    )
    .unwrap();

    // ── Verify B received everything correctly ────────────────────────────────
    assert_eq!(vault_b.items.len(), 1);
    let received = vault_b.get_item(item_id).unwrap();
    assert_eq!(received.name, "AWS Console");
    assert_eq!(received.username.as_deref(), Some("admin@company.com"));
    assert_eq!(received.password.as_deref(), Some("Pr0d-Acc3ss-K3y!"));
    assert_eq!(received.totp_secret.as_deref(), Some("JBSWY3DPEHPK3PXP"));

    // ── Verify metadata hiding: relay sees nothing useful ─────────────────────
    // The gift-wrapped event's content is encrypted (NIP-44 ciphertext)
    // The outer pubkey is ephemeral (random key, not A's real key)
    // The "p" tag reveals B's pubkey (necessary for relay filtering)
    // But the content, sender identity, and sync payload are all hidden

    assert!(
        gift_wrapped_event.content.len() > 200,
        "content should be NIP-44 encrypted (long base64)"
    );
    assert!(
        !gift_wrapped_event.content.contains("AWS Console"),
        "vault item name must NOT be visible in outer event"
    );
    assert!(
        !gift_wrapped_event.content.contains("Pr0d-Acc3ss-K3y"),
        "password must NOT be visible in outer event"
    );
    assert!(
        !gift_wrapped_event.content.contains(&pubkey_a),
        "sender's real pubkey must NOT be in outer event content"
    );

    println!("✅ Full Nostr protocol sync verified!");
    println!("   - Vault encrypted with NIP-44");
    println!("   - Event signed with Schnorr (NIP-01)");
    println!("   - Wrapped in NIP-59 gift-wrap (ephemeral sender)");
    println!("   - Recipient unwrapped and decrypted successfully");
    println!("   - All credentials transferred securely");
    println!("   - Relay sees only encrypted ciphertext + ephemeral key");
}
