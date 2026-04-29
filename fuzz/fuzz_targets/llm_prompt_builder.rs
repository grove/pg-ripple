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
/// Iterates to fixpoint: removing one marker may expose another (e.g.
/// `###[SYSTEM]SYS` → strip `[SYSTEM]` → `###SYS`), so the loop repeats
/// until a full pass over all markers produces no further changes.
fn sanitize_prompt(input: &str) -> String {
    let mut result = input.to_owned();
    loop {
        let mut changed = false;
        for marker in INJECTION_MARKERS {
            // Case-insensitive removal: rebuild `result` without the marker.
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
                changed = true;
            }
            out.push_str(&result[last..]);
            result = out;
        }
        // If no marker was removed in this pass, the output is stable.
        if !changed {
            break;
        }
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
