//! Fuzz target for `zvault_core::nostr::nip44_decrypt`.
//!
//! Feeds arbitrary strings as NIP-44 encrypted payloads. The function must
//! never panic — it should return a crypto error for any malformed input.

#![no_main]

use libfuzzer_sys::fuzz_target;
use zvault_core::nostr::nip44_decrypt;

fuzz_target!(|data: &[u8]| {
    // Use a fixed conversation key.
    let conversation_key = [0x42u8; 32];

    // Try decoding as UTF-8 string (NIP-44 payloads are base64 strings).
    if let Ok(payload) = std::str::from_utf8(data) {
        let _ = nip44_decrypt(&conversation_key, payload);
    }
});
