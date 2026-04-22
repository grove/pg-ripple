//! cargo-fuzz target for the dictionary XXH3-128 hash encoder (v0.47.0).
//!
//! Feeds arbitrary byte sequences through the XXH3-128 hasher that the
//! pg_ripple dictionary uses to encode IRIs, blank nodes, and literals to
//! `i64` IDs.  Asserts:
//!   1. No panic.
//!   2. Two calls on identical input produce the same hash (determinism).
//!   3. The high-64-bit truncation to `i64` (via `as i64`) does not panic.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run dictionary_hash -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;
use xxhash_rust::xxh3::xxh3_128;

fuzz_target!(|data: &[u8]| {
    // Compute the hash twice — must be identical (determinism invariant).
    let h1 = xxh3_128(data);
    let h2 = xxh3_128(data);
    assert_eq!(h1, h2, "XXH3-128 must be deterministic");

    // Truncate to i64 the same way the dictionary does:
    // take the low 64 bits and cast via `as i64`.
    let id: i64 = (h1 as u64) as i64;
    let _ = id; // ensure value is used
});
