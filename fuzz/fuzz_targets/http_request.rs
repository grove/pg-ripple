//! cargo-fuzz target for HTTP request parsing (v0.53.0).
//!
//! Feeds arbitrary byte sequences through basic HTTP header / URI parsing
//! logic representative of what `pg_ripple_http` handles at its SPARQL
//! endpoint.  Asserts: no panic.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run http_request -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Parse as UTF-8 text. Non-UTF-8 should fail gracefully.
    let text = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Attempt to parse as a URL — invalid URIs should fail without panic.
    let _ = url::Url::parse(text);

    // Parse as an HTTP-like query string (key=value&...).
    let _pairs: Vec<(&str, &str)> = text
        .split('&')
        .filter_map(|p| {
            let mut it = p.splitn(2, '=');
            let k = it.next()?;
            let v = it.next().unwrap_or("");
            Some((k, v))
        })
        .collect();
});
