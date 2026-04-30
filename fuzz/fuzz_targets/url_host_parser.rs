//! Fuzz target for the Citus URL host parser (v0.75.0 FUZZ-URL-01 / MF-Q).
//!
//! The `extract_url_host()` function in `src/citus.rs` parses URLs provided
//! by database operators to extract hostnames for Citus shard-pruning.
//! Malformed inputs must never cause a panic; they must return an empty string
//! or a best-effort extraction.
//!
//! # What is fuzzed
//!
//! 1. Arbitrary byte sequences fed through the URL host extraction logic.
//! 2. The parser must handle:
//!    - Non-UTF-8 byte sequences (gracefully rejected).
//!    - Malformed schemes (no `http://` or `https://` prefix).
//!    - Truncated inputs (e.g., `http://[::1` with no closing `]`).
//!    - IPv6 literals with embedded special characters.
//!    - Excessively long inputs (controlled by `-max_len`).
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run url_host_parser -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;

/// Inline re-implementation of `extract_url_host` from `src/citus.rs`.
/// We cannot import it directly from the fuzz target (pgrx requires PostgreSQL),
/// so this is a faithful copy that is kept in sync with the source.
fn extract_url_host(url: &str) -> String {
    let rest = if let Some(r) = url.strip_prefix("https://") {
        r
    } else if let Some(r) = url.strip_prefix("http://") {
        r
    } else {
        return String::new();
    };
    if rest.starts_with('[') {
        if let Some(close) = rest.find(']') {
            return rest[..=close].to_owned();
        }
        return String::new();
    }
    let end = rest.find(['/', ':', '?']).unwrap_or(rest.len());
    rest[..end].to_owned()
}

fuzz_target!(|data: &[u8]| {
    // Non-UTF-8 sequences must not panic.
    let text = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Main invariant: extract_url_host must never panic for any input.
    let host = extract_url_host(text);

    // Sanity checks on the result.
    // The returned host must not contain path or query separators.
    if !host.is_empty() {
        assert!(
            !host.contains('/'),
            "host must not contain '/': got {host:?} from {text:?}"
        );
        // IPv6 brackets must be balanced.
        if host.starts_with('[') {
            assert!(
                host.ends_with(']'),
                "IPv6 host must end with ']': got {host:?} from {text:?}"
            );
        }
    }

    // Also exercise URL-like inputs via the `url` crate (must not panic).
    let _ = url::Url::parse(text);
});
