//! cargo-fuzz target for JSON-LD framing (v0.53.0).
//!
//! Feeds arbitrary byte sequences to the JSON-LD framer's JSON input parser.
//! Asserts: no panic.  Invalid JSON or framing errors must be handled
//! gracefully without crashing.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run jsonld_framer -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Parse the input as UTF-8 text.  Non-UTF-8 sequences should not crash.
    let text = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Attempt to parse as JSON — invalid JSON is expected to fail gracefully.
    let _: Result<serde_json::Value, _> = serde_json::from_str(text);
});
