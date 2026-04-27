//! cargo-fuzz target for the GeoSPARQL WKT geometry parser (v0.60.0 A7-1).
//!
//! Feeds arbitrary byte sequences through the WKT geometry string parser.
//! The parser is a Rust-side structural validator for the WKT geometry
//! literal type used by geof:asWKT and related GeoSPARQL functions in
//! src/sparql/expr.rs.  Asserts: no panic on any input.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run geosparql_wkt -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;

/// Validate the structural form of a WKT geometry string.
///
/// Mirrors the Rust-side pre-processing done in `src/sparql/expr.rs` before
/// passing the literal to PostGIS `ST_GeomFromText`.  The validator:
/// 1. Strips the optional CRS tag (`<crs> WKT`).
/// 2. Checks for a recognised WKT keyword.
/// 3. Verifies parentheses are balanced.
/// Returns `None` on structurally invalid input (not a panic).
fn validate_wkt(input: &str) -> Option<&str> {
    let trimmed = input.trim();

    // Strip optional CRS tag: "<http://…> POINT(…)" → "POINT(…)"
    let wkt = if trimmed.starts_with('<') {
        trimmed.find('>').map(|i| trimmed[i + 1..].trim())?
    } else {
        trimmed
    };

    // Recognised WKT geometry type keywords (GeoSPARQL 1.1 §8).
    const WKT_KEYWORDS: &[&str] = &[
        "POINT", "LINESTRING", "POLYGON", "MULTIPOINT",
        "MULTILINESTRING", "MULTIPOLYGON", "GEOMETRYCOLLECTION",
        "CIRCULARSTRING", "COMPOUNDCURVE", "CURVEPOLYGON",
        "MULTICURVE", "MULTISURFACE", "CURVE", "SURFACE",
        "POLYHEDRALSURFACE", "TIN", "TRIANGLE",
        "POINT Z", "LINESTRING Z", "POLYGON Z",
        "POINT M", "LINESTRING M", "POLYGON M",
        "POINT ZM", "LINESTRING ZM", "POLYGON ZM",
        "GEOMETRYCOLLECTION EMPTY",
    ];

    let wkt_upper = wkt.to_uppercase();
    if !WKT_KEYWORDS.iter().any(|kw| wkt_upper.starts_with(kw)) {
        return None;
    }

    // Check balanced parentheses (simple structural sanity check).
    let mut depth: i32 = 0;
    for ch in wkt.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }

    Some(wkt)
}

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    // Must not panic on any UTF-8 input.
    let _ = validate_wkt(input);
});
