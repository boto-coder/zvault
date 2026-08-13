//! Fuzz target for `zvault_core::vault::Vault::from_json`.
//!
//! Feeds arbitrary byte slices as JSON input. The parser must never panic —
//! it should return a serialisation error for any malformed input.

#![no_main]

use libfuzzer_sys::fuzz_target;
use zvault_core::vault::Vault;

fuzz_target!(|data: &[u8]| {
    // from_json must not panic regardless of input.
    let _ = Vault::from_json(data);
});
