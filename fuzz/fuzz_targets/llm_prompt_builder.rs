//! cargo-fuzz target for the LLM prompt sanitizer (v0.60.0 A7-1).
//!
//! Feeds arbitrary bytes through the same prompt-sanitization pipeline that
//! `src/llm/mod.rs` uses before constructing LLM requests.  Asserts:
//!
//!   1. The sanitizer never panics.
//!   2. No well-known prompt-injection marker survives sanitization.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run llm_prompt_builder -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;

/// Well-known prompt-injection markers that the sanitizer must strip or neutralise.
const INJECTION_MARKERS: &[&str] = &[
    "IGNORE PREVIOUS INSTRUCTIONS",
    "IGNORE ALL PREVIOUS",
    "###SYS",
    "---END---",
    "[SYSTEM]",
    "<|im_start|>",
    "<|im_end|>",
    "###INSTRUCTION",
    "OVERRIDE:",
    "JAILBREAK",
    "DAN MODE",
];

/// Minimal inline prompt sanitizer mirroring `src/llm/mod.rs`.
///
/// Removes prompt-injection markers using a case-insensitive replacement pass.
/// This is the same algorithm used in the production code; having it here
/// ensures the fuzzer can exercise it without depending on the pgrx extension.
fn sanitize_prompt(input: &str) -> String {
    let mut result = input.to_owned();
    for marker in INJECTION_MARKERS {
        // Case-insensitive removal: collect indices of the marker in `result`.
        let upper = result.to_uppercase();
        let marker_upper = marker.to_uppercase();
        let mut out = String::with_capacity(result.len());
        let mut last = 0usize;
        let mut search_from = 0usize;
        while let Some(pos) = upper[search_from..].find(&marker_upper) {
            let abs_pos = search_from + pos;
            out.push_str(&result[last..abs_pos]);
            search_from = abs_pos + marker.len();
            last = search_from;
        }
        out.push_str(&result[last..]);
        result = out;
    }
    result
}

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };

    // Sanitize the input — must not panic.
    let sanitized = sanitize_prompt(input);

    // Assert no injection marker survives sanitization.
    let sanitized_upper = sanitized.to_uppercase();
    for marker in INJECTION_MARKERS {
        let marker_upper = marker.to_uppercase();
        assert!(
            !sanitized_upper.contains(&marker_upper),
            "prompt-injection marker survived sanitization: {marker:?}\n\
             input (truncated): {:?}\n\
             sanitized (truncated): {:?}",
            &input[..input.len().min(200)],
            &sanitized[..sanitized.len().min(200)],
        );
    }
});
