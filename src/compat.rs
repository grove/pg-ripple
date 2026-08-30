//! Extension compatibility check (v0.118.0 Feature 3).
//!
//! `pg_ripple.compat_check()` returns a JSON object describing the extension
//! version and the minimum pg_ripple_http version required to serve it.
//! The HTTP companion calls this at startup and compares its own version
//! against `http_min_version`, refusing to serve if it is too old.
//!
//! Closes C16-01 belt-and-suspenders (on top of the v0.112.0 CI gate).

use pgrx::prelude::*;

/// Minimum pg_ripple_http companion version required to fully support this
/// extension release.  Updated each release alongside `COMPATIBLE_EXTENSION_MIN`
/// in pg_ripple_http/src/main.rs.
const HTTP_COMPANION_MIN_VERSION: &str = "0.135.0";

/// Return a JSON compatibility descriptor for the HTTP companion.
///
/// # Returns
///
/// A TEXT value containing a JSON object with the following fields:
/// - `extension_version`: the running extension version.
/// - `http_min_version`: the minimum pg_ripple_http version required.
/// - `compatible`: always `true` from the extension side; the HTTP companion
///   must compare its own version against `http_min_version`.
///
/// # Example
///
/// ```sql
/// SELECT pg_ripple.compat_check();
/// -- {"extension_version":"0.136.0","http_min_version":"0.135.0","compatible":true}
/// ```
#[pg_extern(schema = "pg_ripple")]
pub fn compat_check() -> String {
    let ext_version = env!("CARGO_PKG_VERSION");
    serde_json::json!({
        "extension_version": ext_version,
        "http_min_version": HTTP_COMPANION_MIN_VERSION,
        "compatible": true,
    })
    .to_string()
}
