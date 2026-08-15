//! End-to-end integration test for CLI sync over a real Nostr relay.
//!
//! These tests require network access to a public Nostr relay and are gated
//! with `#[ignore]`. Run explicitly:
//!
//! ```sh
//! cargo test --test cli_sync_e2e -- --ignored
//! ```
//!
//! Override relay URL via the `ZVAULT_TEST_RELAY` environment variable
//! (default: `wss://relay.damus.io`).
//!
//! Before running, build the CLI binary:
//! ```sh
//! cargo build -p zvault-cli
//! ```

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Get the path to the built `zvault` binary.
fn zvault_bin() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // crates/
    path.pop(); // workspace root
    path.push("target");
    path.push("debug");
    path.push("zvault");
    path
}

/// Get the relay URL from env or use default.
fn relay_url() -> String {
    std::env::var("ZVAULT_TEST_RELAY").unwrap_or_else(|_| "wss://relay.damus.io".to_string())
}

/// Run a zvault CLI command with the given args and password, returning stdout.
fn run_zvault(args: &[&str], password: &str, _timeout: Duration) -> (bool, String, String) {
    let bin = zvault_bin();
    assert!(
        bin.exists(),
        "zvault binary not found at {}. Run `cargo build -p zvault-cli` first.",
        bin.display()
    );

    let output = Command::new(&bin)
        .args(args)
        .env("ZVAULT_PASSWORD", password)
        .output()
        .expect("failed to execute zvault binary");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

/// Extract the public key from `device init` output.
/// Expects output like: "  Public key: <hex>"
fn extract_pubkey(stdout: &str) -> String {
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("  Public key: ") {
            return rest.trim().to_string();
        }
    }
    panic!("could not find 'Public key:' in output:\n{stdout}");
}

/// Extract the device ID from `device init` output.
/// Expects output like: "  Device ID: <uuid>"
fn extract_device_id(stdout: &str) -> String {
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("  Device ID: ") {
            return rest.trim().to_string();
        }
    }
    panic!("could not find 'Device ID:' in output:\n{stdout}");
}

/// Full two-device sync via a real Nostr relay.
///
/// Scenario:
/// 1. Create two vault files (A and B)
/// 2. `device init` on both
/// 3. Cross-admit: admit B's pubkey into A, and A's pubkey into B
/// 4. Add a login item to vault A
/// 5. `sync send` from A to B via the relay
/// 6. Short delay for relay propagation
/// 7. `sync receive` on B
/// 8. Verify the item arrived in B
#[test]
#[ignore]
fn full_two_device_sync_via_real_relay() {
    let timeout = Duration::from_secs(30);
    let relay = relay_url();
    let password = "test-password-e2e-sync";

    // Create temporary vault files.
    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let vault_a = tmp_dir.path().join("vault_a.zvault");
    let vault_b = tmp_dir.path().join("vault_b.zvault");

    // Step 1: Create vaults.
    let (ok, stdout, stderr) = run_zvault(&["init", vault_a.to_str().unwrap()], password, timeout);
    assert!(ok, "init vault A failed: {stderr}\n{stdout}");

    let (ok, stdout, stderr) = run_zvault(&["init", vault_b.to_str().unwrap()], password, timeout);
    assert!(ok, "init vault B failed: {stderr}\n{stdout}");

    // Step 2: Device init on both.
    let (ok, stdout_a, stderr) = run_zvault(
        &[
            "device",
            "init",
            "--vault",
            vault_a.to_str().unwrap(),
            "--label",
            "Device-A",
        ],
        password,
        timeout,
    );
    assert!(ok, "device init A failed: {stderr}\n{stdout_a}");
    let pubkey_a = extract_pubkey(&stdout_a);
    let _device_id_a = extract_device_id(&stdout_a);

    let (ok, stdout_b, stderr) = run_zvault(
        &[
            "device",
            "init",
            "--vault",
            vault_b.to_str().unwrap(),
            "--label",
            "Device-B",
        ],
        password,
        timeout,
    );
    assert!(ok, "device init B failed: {stderr}\n{stdout_b}");
    let pubkey_b = extract_pubkey(&stdout_b);
    let _device_id_b = extract_device_id(&stdout_b);

    // Step 3: Cross-admit.
    // Admit B into A.
    let (ok, stdout, stderr) = run_zvault(
        &[
            "device",
            "admit",
            "--vault",
            vault_a.to_str().unwrap(),
            "--label",
            "Device-B",
            "--pubkey",
            &pubkey_b,
        ],
        password,
        timeout,
    );
    assert!(ok, "admit B into A failed: {stderr}\n{stdout}");

    // Admit A into B.
    let (ok, stdout, stderr) = run_zvault(
        &[
            "device",
            "admit",
            "--vault",
            vault_b.to_str().unwrap(),
            "--label",
            "Device-A",
            "--pubkey",
            &pubkey_a,
        ],
        password,
        timeout,
    );
    assert!(ok, "admit A into B failed: {stderr}\n{stdout}");

    // Step 4: Add an item to vault A.
    let item_json = r#"{"kind":"login","name":"E2E Test Login","username":"user@test.com","password":"s3cr3t-pw-e2e"}"#;
    let (ok, stdout, stderr) = run_zvault(
        &[
            "add",
            "--vault",
            vault_a.to_str().unwrap(),
            "--json",
            item_json,
        ],
        password,
        timeout,
    );
    assert!(ok, "add item to A failed: {stderr}\n{stdout}");

    // Step 5: Sync send from A to B.
    let (ok, stdout, stderr) = run_zvault(
        &[
            "sync",
            "send",
            "--vault",
            vault_a.to_str().unwrap(),
            "--relay",
            &relay,
            "--recipient",
            &pubkey_b,
        ],
        password,
        timeout,
    );
    assert!(ok, "sync send A→B failed: {stderr}\n{stdout}");
    assert!(
        stdout.contains("Sync sent"),
        "expected 'Sync sent' in output: {stdout}"
    );

    // Step 6: Short delay for relay propagation.
    std::thread::sleep(Duration::from_secs(2));

    // Step 7: Sync receive on B.
    let (ok, stdout, stderr) = run_zvault(
        &[
            "sync",
            "receive",
            "--vault",
            vault_b.to_str().unwrap(),
            "--relay",
            &relay,
            "--timeout",
            "10",
        ],
        password,
        timeout,
    );
    assert!(ok, "sync receive on B failed: {stderr}\n{stdout}");
    assert!(
        stdout.contains("Received") && stdout.contains("sync message"),
        "expected sync receipt confirmation in output: {stdout}"
    );

    // Step 8: Verify item arrived in B.
    let (ok, stdout, stderr) = run_zvault(
        &["list", "--vault", vault_b.to_str().unwrap()],
        password,
        timeout,
    );
    assert!(ok, "list B failed: {stderr}\n{stdout}");
    assert!(
        stdout.contains("E2E Test Login"),
        "expected synced item 'E2E Test Login' in vault B listing:\n{stdout}"
    );
}

/// Test that a revoked device's sync messages are rejected.
///
/// Scenario:
/// 1. A creates vault, inits device, admits B
/// 2. B inits device, admits A
/// 3. B sends a sync to A (success — B is live)
/// 4. A revokes B
/// 5. B sends another sync to A → should be rejected
/// 6. Verify A's vault is unchanged after the rejected sync
#[test]
#[ignore]
fn revoked_device_sync_rejected() {
    let timeout = Duration::from_secs(30);
    let relay = relay_url();
    let password = "test-password-revoke";

    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let vault_a = tmp_dir.path().join("vault_a_revoke.zvault");
    let vault_b = tmp_dir.path().join("vault_b_revoke.zvault");

    // Create vaults.
    let (ok, _, stderr) = run_zvault(&["init", vault_a.to_str().unwrap()], password, timeout);
    assert!(ok, "init vault A failed: {stderr}");
    let (ok, _, stderr) = run_zvault(&["init", vault_b.to_str().unwrap()], password, timeout);
    assert!(ok, "init vault B failed: {stderr}");

    // Device init both.
    let (ok, stdout_a, stderr) = run_zvault(
        &[
            "device",
            "init",
            "--vault",
            vault_a.to_str().unwrap(),
            "--label",
            "Device-A",
        ],
        password,
        timeout,
    );
    assert!(ok, "device init A failed: {stderr}");
    let pubkey_a = extract_pubkey(&stdout_a);

    let (ok, stdout_b, stderr) = run_zvault(
        &[
            "device",
            "init",
            "--vault",
            vault_b.to_str().unwrap(),
            "--label",
            "Device-B",
        ],
        password,
        timeout,
    );
    assert!(ok, "device init B failed: {stderr}");
    let pubkey_b = extract_pubkey(&stdout_b);
    let device_id_b = extract_device_id(&stdout_b);

    // Cross-admit.
    let (ok, _, stderr) = run_zvault(
        &[
            "device",
            "admit",
            "--vault",
            vault_a.to_str().unwrap(),
            "--label",
            "Device-B",
            "--pubkey",
            &pubkey_b,
        ],
        password,
        timeout,
    );
    assert!(ok, "admit B into A failed: {stderr}");

    let (ok, _, stderr) = run_zvault(
        &[
            "device",
            "admit",
            "--vault",
            vault_b.to_str().unwrap(),
            "--label",
            "Device-A",
            "--pubkey",
            &pubkey_a,
        ],
        password,
        timeout,
    );
    assert!(ok, "admit A into B failed: {stderr}");

    // B adds an item.
    let item_json =
        r#"{"kind":"login","name":"From B before revoke","username":"b-user","password":"b-pass"}"#;
    let (ok, _, stderr) = run_zvault(
        &[
            "add",
            "--vault",
            vault_b.to_str().unwrap(),
            "--json",
            item_json,
        ],
        password,
        timeout,
    );
    assert!(ok, "add item to B failed: {stderr}");

    // B sends sync to A (should succeed — B is live).
    let (ok, stdout, stderr) = run_zvault(
        &[
            "sync",
            "send",
            "--vault",
            vault_b.to_str().unwrap(),
            "--relay",
            &relay,
            "--recipient",
            &pubkey_a,
        ],
        password,
        timeout,
    );
    assert!(ok, "B sync send to A failed: {stderr}\n{stdout}");

    std::thread::sleep(Duration::from_secs(2));

    // A receives (should apply B's item).
    let (ok, stdout, stderr) = run_zvault(
        &[
            "sync",
            "receive",
            "--vault",
            vault_a.to_str().unwrap(),
            "--relay",
            &relay,
            "--timeout",
            "10",
        ],
        password,
        timeout,
    );
    assert!(ok, "A sync receive failed: {stderr}\n{stdout}");

    // Verify A has the item from B.
    let (ok, stdout, _) = run_zvault(
        &["list", "--vault", vault_a.to_str().unwrap()],
        password,
        timeout,
    );
    assert!(ok, "list A failed");
    assert!(
        stdout.contains("From B before revoke"),
        "expected item from B in A's vault before revocation"
    );

    // A revokes B.
    let (ok, stdout, stderr) = run_zvault(
        &[
            "device",
            "revoke",
            &device_id_b,
            "--vault",
            vault_a.to_str().unwrap(),
        ],
        password,
        timeout,
    );
    assert!(ok, "revoke B failed: {stderr}\n{stdout}");

    // B adds another item and sends sync to A (should be rejected by A).
    let item_json2 = r#"{"kind":"login","name":"From B after revoke","username":"b-evil","password":"evil-pass"}"#;
    let (ok, _, stderr) = run_zvault(
        &[
            "add",
            "--vault",
            vault_b.to_str().unwrap(),
            "--json",
            item_json2,
        ],
        password,
        timeout,
    );
    assert!(ok, "add second item to B failed: {stderr}");

    let (ok, stdout, stderr) = run_zvault(
        &[
            "sync",
            "send",
            "--vault",
            vault_b.to_str().unwrap(),
            "--relay",
            &relay,
            "--recipient",
            &pubkey_a,
        ],
        password,
        timeout,
    );
    assert!(ok, "B sync send (after revoke) failed: {stderr}\n{stdout}");

    std::thread::sleep(Duration::from_secs(2));

    // A receives — should reject B's message (B is revoked).
    let (ok, _stdout, stderr) = run_zvault(
        &[
            "sync",
            "receive",
            "--vault",
            vault_a.to_str().unwrap(),
            "--relay",
            &relay,
            "--timeout",
            "10",
        ],
        password,
        timeout,
    );
    // The command itself may succeed (it just won't apply the message).
    // Check that no new message was applied OR that a warning was printed.
    let _ = (ok, stderr);

    // Verify A does NOT have the second item from B.
    let (ok, stdout, _) = run_zvault(
        &["list", "--vault", vault_a.to_str().unwrap()],
        password,
        timeout,
    );
    assert!(ok, "list A (after revoke) failed");
    assert!(
        !stdout.contains("From B after revoke"),
        "SECURITY FAILURE: revoked device's item appeared in A's vault!\nOutput:\n{stdout}"
    );
    // But the first item should still be there.
    assert!(
        stdout.contains("From B before revoke"),
        "original item from B should still be in A's vault"
    );
}
