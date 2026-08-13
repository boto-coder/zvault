//! Fuzz target for `zvault_core::import::import_csv`.
//!
//! Feeds arbitrary byte slices as CSV data. The parser must never panic —
//! it should return a serialisation error for any malformed input.

#![no_main]

use libfuzzer_sys::fuzz_target;
use zvault_core::import::import_csv;

fuzz_target!(|data: &[u8]| {
    // import_csv must not panic regardless of input.
    let _ = import_csv(data);
});
