//! Fuzz target for `zvault_core::import::import_bitwarden_json`.
//!
//! Feeds arbitrary byte slices as Bitwarden JSON export data. The parser must
//! never panic — it should return a serialisation error for any malformed input.

#![no_main]

use libfuzzer_sys::fuzz_target;
use zvault_core::import::import_bitwarden_json;

fuzz_target!(|data: &[u8]| {
    // import_bitwarden_json must not panic regardless of input.
    let _ = import_bitwarden_json(data);
});
