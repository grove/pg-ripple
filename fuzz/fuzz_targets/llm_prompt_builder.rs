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

/// Find the first occurrence of `needle_upper` (a pre-uppercased ASCII slice)
/// in `haystack` using full Unicode case expansion.
///
/// Each character of `haystack` is expanded via `char::to_uppercase()` before
/// comparison.  This catches Unicode look-alike characters such as
/// ſ (U+017F LATIN SMALL LETTER LONG S) whose Unicode uppercase is 'S'.
///
/// Returns `(start_byte, end_byte)` in `haystack` — the byte range to drain.
fn find_marker_unicode(haystack: &str, needle_upper: &[char]) -> Option<(usize, usize)> {
    let m = needle_upper.len();
    if m == 0 {
        return None;
    }

    // Expand every haystack character to its Unicode uppercase form, recording
    // the original byte range that each expanded character came from.
    let expanded: Vec<(char, usize, usize)> = haystack
        .char_indices()
        .flat_map(|(byte_start, ch)| {
            let byte_end = byte_start + ch.len_utf8();
            ch.to_uppercase().map(move |uc| (uc, byte_start, byte_end))
        })
        .collect();

    let n = expanded.len();
    if n < m {
        return None;
    }

    'outer: for i in 0..=(n - m) {
        for j in 0..m {
            if expanded[i + j].0 != needle_upper[j] {
                continue 'outer;
            }
        }
        let start_byte = expanded[i].1;
        let end_byte = expanded[i + m - 1].2;
        return Some((start_byte, end_byte));
    }
    None
}

/// Minimal inline prompt sanitizer mirroring `src/llm/mod.rs`.
///
/// Removes prompt-injection markers using a Unicode-aware, case-insensitive
/// scan.  Each character is expanded to its full Unicode uppercase form before
/// comparison so that look-alike characters (e.g. ſ U+017F → S, ȿ U+023F →
/// Ȿ U+2C9F) cannot be used to bypass ASCII-only case folding.
///
/// Iterates to fixpoint: removing one marker may expose another (e.g.
/// `###[SYSTEM]SYS` → strip `[SYSTEM]` → `###SYS`), so the outer loop
/// repeats until a complete pass over all markers produces no further changes.
fn sanitize_prompt(input: &str) -> String {
    // Pre-compute the uppercase char sequence for each marker.
    // All markers are ASCII, so to_uppercase() == to_ascii_uppercase() here.
    let marker_uppers: Vec<Vec<char>> = INJECTION_MARKERS
        .iter()
        .map(|m| m.chars().map(|c| c.to_ascii_uppercase()).collect())
        .collect();

    let mut result = input.to_owned();
    loop {
        let mut changed = false;
        for marker_upper in &marker_uppers {
            // Remove occurrences left-to-right until none remain.
            loop {
                match find_marker_unicode(&result, marker_upper) {
                    Some((start, end)) => {
                        result.drain(start..end);
                        changed = true;
                    }
                    None => break,
                }
            }
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
