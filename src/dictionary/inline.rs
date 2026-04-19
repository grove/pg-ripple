//! Inline value encoding for common xsd: datatypes.
//!
//! Instead of inserting every numeric/date literal into the dictionary table,
//! we encode the value directly into the i64 key using a type-tagged bit
//! packing scheme.  This eliminates dictionary I/O for the most common
//! comparison types.
//!
//! # Bit layout
//!
//! ```text
//! Bit 63        → 1 (inline flag; all dictionary IDENTITY IDs have bit 63 = 0)
//! Bits 62–56    → 7-bit type code
//! Bits 55–0     → 56-bit encoded value (offset by 2^55 to preserve sort order)
//! ```
//!
//! # Type codes
//!
//! | Code | xsd: datatype   | Value interpretation       |
//! |------|-----------------|----------------------------|
//! |  0   | xsd:integer     | signed 56-bit (±2^55)      |
//! |  1   | xsd:boolean     | 0 = false, 1 = true        |
//! |  2   | xsd:dateTime    | microseconds since epoch   |
//! |  3   | xsd:date        | days since epoch            |
//!
//! The offset of `2^55` (= `INTEGER_OFFSET`) is added to the value before
//! storing in bits 55–0, which turns negative values into non-negative ones,
//! preserving i64 sort order across the full signed range for all types.
//!
//! # Range limits
//!
//! | Type        | Min                                     | Max                                    |
//! |-------------|------------------------------------------|----------------------------------------|
//! | integer     | -(2^55) = -36 028 797 018 963 968       | 2^55-1 = 36 028 797 018 963 967        |
//! | dateTime    | -2^55 µs (≈ AD 827)                     | 2^55-1 µs (≈ AD 3113)                  |
//! | date        | -(2^55) days (≈ -98 billion years)     | effectively unlimited                  |
//!
//! Values outside the range fall back to dictionary storage automatically.

use chrono::{DateTime, NaiveDate, Utc};

// ─── Constants ────────────────────────────────────────────────────────────────

// Bit masks and shifts.
const INLINE_FLAG: u64 = 1u64 << 63;
const TYPE_SHIFT: u32 = 56;
const VALUE_MASK: u64 = (1u64 << 56) - 1; // bits 55–0

// The offset applied to signed values before packing into bits 55–0:
// adding 2^55 maps the signed range [-2^55, 2^55-1] → [0, 2^56-1].
const INTEGER_OFFSET: i64 = 1i64 << 55;

// Type codes (7-bit, stored in bits 62–56).
pub const TYPE_INTEGER: u64 = 0;
pub const TYPE_BOOLEAN: u64 = 1;
pub const TYPE_DATETIME: u64 = 2;
pub const TYPE_DATE: u64 = 3;

// ─── Helpers ──────────────────────────────────────────────────────────────────

#[inline]
fn pack(type_code: u64, value_bits: u64) -> i64 {
    (INLINE_FLAG | (type_code << TYPE_SHIFT) | (value_bits & VALUE_MASK)) as i64
}

#[inline]
fn unpack_type(id: i64) -> u64 {
    ((id as u64) >> TYPE_SHIFT) & 0x7F
}

#[inline]
fn unpack_value_bits(id: i64) -> u64 {
    (id as u64) & VALUE_MASK
}

// ─── Public predicates ───────────────────────────────────────────────────────

/// Returns `true` if `id` is an inline-encoded value rather than a dictionary ID.
///
/// All IDENTITY-sequence dictionary IDs are positive (bit 63 = 0).
/// All inline IDs have bit 63 = 1, which makes them negative as signed i64.
#[inline]
pub fn is_inline(id: i64) -> bool {
    id < 0
}

/// Returns the 7-bit type code for an inline ID.
///
/// # Panics
///
/// Panics in debug mode if `id` is not an inline value.
#[inline]
pub fn inline_type(id: i64) -> u64 {
    debug_assert!(is_inline(id), "inline_type called on non-inline id");
    unpack_type(id)
}

// ─── Encoding ────────────────────────────────────────────────────────────────

/// Encode an `xsd:integer` lexical form to an inline ID.
///
/// Returns `None` if the value is outside the representable range (±2^55).
pub fn try_encode_integer(lexical: &str) -> Option<i64> {
    let v: i64 = lexical.trim().parse().ok()?;
    if !(-INTEGER_OFFSET..=INTEGER_OFFSET - 1).contains(&v) {
        return None; // out of 56-bit range — fall back to dictionary
    }
    // Apply offset so that all stored bits are non-negative.
    let bits = (v + INTEGER_OFFSET) as u64;
    Some(pack(TYPE_INTEGER, bits))
}

/// Encode an `xsd:boolean` lexical form ("true" / "false" / "1" / "0") to an inline ID.
pub fn try_encode_boolean(lexical: &str) -> Option<i64> {
    let bits: u64 = match lexical.trim() {
        "true" | "1" => 1,
        "false" | "0" => 0,
        _ => return None,
    };
    Some(pack(TYPE_BOOLEAN, bits))
}

/// Encode an `xsd:dateTime` lexical form ("YYYY-MM-DDTHH:MM:SSZ") to an inline ID.
///
/// Returns `None` if the value is outside the representable range or unparseable.
pub fn try_encode_datetime(lexical: &str) -> Option<i64> {
    let dt: DateTime<Utc> = lexical.trim().parse().ok()?;
    let micros = dt.timestamp_micros();
    if !(-INTEGER_OFFSET..=INTEGER_OFFSET - 1).contains(&micros) {
        return None;
    }
    let bits = (micros + INTEGER_OFFSET) as u64;
    Some(pack(TYPE_DATETIME, bits))
}

/// Encode an `xsd:date` lexical form ("YYYY-MM-DD") to an inline ID.
///
/// Returns `None` if the value is outside the representable range or unparseable.
pub fn try_encode_date(lexical: &str) -> Option<i64> {
    let d: NaiveDate = lexical.trim().parse().ok()?;
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1)?;
    let days = (d - epoch).num_days();
    if !(-INTEGER_OFFSET..=INTEGER_OFFSET - 1).contains(&days) {
        return None;
    }
    let bits = (days + INTEGER_OFFSET) as u64;
    Some(pack(TYPE_DATE, bits))
}

// ─── Decoding ────────────────────────────────────────────────────────────────

/// Decode an inline `xsd:integer` ID back to its signed integer value.
pub fn decode_integer(id: i64) -> i64 {
    let bits = unpack_value_bits(id) as i64;
    bits - INTEGER_OFFSET
}

/// Decode an inline `xsd:boolean` ID back to a bool.
pub fn decode_boolean(id: i64) -> bool {
    unpack_value_bits(id) != 0
}

/// Decode an inline `xsd:dateTime` ID to microseconds since Unix epoch.
pub fn decode_datetime_micros(id: i64) -> i64 {
    let bits = unpack_value_bits(id) as i64;
    bits - INTEGER_OFFSET
}

/// Decode an inline `xsd:date` ID to days since Unix epoch.
pub fn decode_date_days(id: i64) -> i64 {
    let bits = unpack_value_bits(id) as i64;
    bits - INTEGER_OFFSET
}

/// Format an inline ID as an N-Triples typed literal string.
///
/// Returns a string like `"42"^^<http://www.w3.org/2001/XMLSchema#integer>`.
pub fn format_inline(id: i64) -> String {
    debug_assert!(
        is_inline(id),
        "format_inline called with non-inline id {id}"
    );
    match inline_type(id) {
        TYPE_INTEGER => {
            let v = decode_integer(id);
            format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>", v)
        }
        TYPE_BOOLEAN => {
            let v = if decode_boolean(id) { "true" } else { "false" };
            format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#boolean>", v)
        }
        TYPE_DATETIME => {
            let micros = decode_datetime_micros(id);
            let dt = DateTime::<Utc>::from_timestamp_micros(micros)
                .map(|t| t.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string())
                .unwrap_or_else(|| format!("invalid_dt:{}", micros));
            format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#dateTime>", dt)
        }
        TYPE_DATE => {
            let days = decode_date_days(id);
            // SAFETY: 1970-01-01 is a valid calendar date.
            #[allow(clippy::unwrap_used)]
            let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
            use chrono::Duration;
            let d = epoch
                .checked_add_signed(Duration::days(days))
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| format!("invalid_date:{}", days));
            format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#date>", d)
        }
        other => {
            // Unknown type code — show raw representation.
            format!(
                "\"inline:{}:{}\"^^<urn:pg_ripple:inline>",
                other,
                decode_integer(id)
            )
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integer_roundtrip() {
        for &v in &[0i64, 1, -1, 42, -42, 1_000_000, -1_000_000] {
            let id = try_encode_integer(&v.to_string()).expect("should encode");
            assert!(is_inline(id));
            assert_eq!(inline_type(id), TYPE_INTEGER);
            assert_eq!(decode_integer(id), v);
        }
    }

    #[test]
    fn test_integer_ordering_preserved() {
        let id0 = try_encode_integer("0").unwrap();
        let id1 = try_encode_integer("1").unwrap();
        let id_neg = try_encode_integer("-1").unwrap();
        assert!(id1 > id0, "1 should have greater inline id than 0");
        assert!(id_neg < id0, "-1 should have smaller inline id than 0");
    }

    #[test]
    fn test_boolean_roundtrip() {
        let id_t = try_encode_boolean("true").unwrap();
        let id_f = try_encode_boolean("false").unwrap();
        assert!(is_inline(id_t));
        assert!(is_inline(id_f));
        assert_eq!(inline_type(id_t), TYPE_BOOLEAN);
        assert!(decode_boolean(id_t));
        assert!(!decode_boolean(id_f));
    }

    #[test]
    fn test_date_roundtrip() {
        let id = try_encode_date("2023-06-15").unwrap();
        assert!(is_inline(id));
        assert_eq!(inline_type(id), TYPE_DATE);
        let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        let d = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let expected_days = (d - epoch).num_days();
        assert_eq!(decode_date_days(id), expected_days);
    }

    #[test]
    fn test_datetime_roundtrip() {
        let id = try_encode_datetime("2023-06-15T12:00:00Z").unwrap();
        assert!(is_inline(id));
        assert_eq!(inline_type(id), TYPE_DATETIME);
        // Just verify it's representable.
        let micros = decode_datetime_micros(id);
        assert!(micros > 0);
    }

    #[test]
    fn test_format_integer() {
        let id = try_encode_integer("42").unwrap();
        assert_eq!(
            format_inline(id),
            "\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
    }

    #[test]
    fn test_format_boolean() {
        let id = try_encode_boolean("true").unwrap();
        assert_eq!(
            format_inline(id),
            "\"true\"^^<http://www.w3.org/2001/XMLSchema#boolean>"
        );
    }

    #[test]
    fn test_noninline_positive() {
        // Positive i64 values (dictionary IDs) must not be detected as inline.
        assert!(!is_inline(1));
        assert!(!is_inline(42));
        assert!(!is_inline(i64::MAX));
    }
}
