//! cargo-fuzz target for the Datalog rule parser (v0.47.0).
//!
//! Feeds arbitrary byte sequences through the Datalog rule line tokenizer.
//! Asserts: no panic.  Invalid input must produce an error or be ignored,
//! never cause a crash or infinite loop.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run datalog_parser -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;

/// Minimal re-implementation of the Datalog rule line tokenizer.
///
/// The production tokenizer in `src/datalog/parser.rs` runs inside PostgreSQL
/// and cannot be linked into a standalone fuzzer binary.  This function
/// mirrors the key parsing steps that could panic on malformed input.
fn tokenize_rule_line(s: &str) -> Option<(String, Vec<String>)> {
    // Expected form:  Head :- Body1, Body2, ... .
    let s = s.trim();
    if s.is_empty() || s.starts_with('%') {
        return None;
    }
    let (head_part, body_part) = s.split_once(":-")?;
    let head = head_part.trim().to_string();
    let body_raw = body_part.trim().trim_end_matches('.');
    let body: Vec<String> = body_raw
        .split(',')
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect();
    Some((head, body))
}

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    for line in s.lines() {
        let _ = tokenize_rule_line(line);
    }
});
