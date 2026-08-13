//! Fuzz target for `zvault_core::crypto::decrypt`.
//!
//! Feeds arbitrary byte slices as encrypted vault blobs. The decrypt function
//! must never panic — it should return an error for any malformed input.

#![no_main]

use libfuzzer_sys::fuzz_target;
use zeroize::Zeroizing;
use zvault_core::crypto::{decrypt, VaultKey};

fuzz_target!(|data: &[u8]| {
    // Use a fixed key — the goal is to fuzz the parser/decryptor, not the KDF.
    let key = VaultKey::from_bytes(Zeroizing::new([0x42u8; 32]));

    // decrypt must not panic regardless of input.
    let _ = decrypt(&key, data);
});
