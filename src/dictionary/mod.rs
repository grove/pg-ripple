//! Dictionary encoder — maps RDF terms to i64 identifiers.
//!
//! Every IRI, blank node, and literal is encoded to a compact `i64` before
//! being stored in a VP table.  The encoding uses the full XXH3-128 hash as a
//! collision-resistant key: the 16-byte hash is stored in the `hash BYTEA`
//! column with a UNIQUE constraint, and a PostgreSQL IDENTITY sequence
//! generates the dense `i64` join key.  This eliminates the birthday-problem
//! collision risk present in schemes that truncate the hash to 64 bits.
//!
//! The `kind` discriminant is mixed into the hash input so that the same
//! string encoded as different term types (e.g., IRI vs. blank node) always
//! produces distinct dictionary IDs.
//!
//! # Encoding path
//!
//! 1. Check backend-local encode cache (`u128 → i64`); return on hit.
//! 2. Compute XXH3-128 of `kind_le_bytes || term_utf8`.
//! 3. `INSERT INTO _pg_ripple.dictionary (hash, value, kind) VALUES ($1, $2, $3)
//!    ON CONFLICT (hash) DO NOTHING RETURNING id`.
//! 4. If INSERT returned no row (term already existed), `SELECT id … WHERE hash = $1`.
//! 5. Populate both caches; return the `i64`.
//!
//! # Term kinds
//!
//! | `kind` | Meaning |
//! |--------|---------|
//! | 0      | IRI |
//! | 1      | Blank node |
//! | 2      | Plain literal |
//! | 3      | Typed literal |
//! | 4      | Language-tagged literal |
//!
//! # Backend-local caches (v0.1.0–v0.5.1)
//!
//! Each backend maintains an encode `LruCache<u128, i64>` (hash → sequence id)
//! and a decode `LruCache<i64, String>` (sequence id → term value).
//! Shared-memory caches are introduced in v0.6.0.

use lru::LruCache;
use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use std::cell::RefCell;
use std::num::NonZeroUsize;
use xxhash_rust::xxh3::xxh3_128;

const CACHE_CAPACITY: usize = 16_384;

pub const KIND_IRI: i16 = 0;
#[allow(dead_code)]
pub const KIND_BLANK: i16 = 1;
#[allow(dead_code)]
pub const KIND_LITERAL: i16 = 2;
#[allow(dead_code)]
pub const KIND_TYPED_LITERAL: i16 = 3;
#[allow(dead_code)]
pub const KIND_LANG_LITERAL: i16 = 4;

thread_local! {
    /// Encode cache: full XXH3-128 hash → sequence-generated id.
    static ENCODE_CACHE: RefCell<LruCache<u128, i64>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(CACHE_CAPACITY).expect("capacity > 0"))
    );
    /// Decode cache: sequence id → term value.
    static DECODE_CACHE: RefCell<LruCache<i64, String>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(CACHE_CAPACITY).expect("capacity > 0"))
    );
}

/// Compute the XXH3-128 hash for a term, mixing in `kind` so that the same
/// string with different term types maps to different identifiers.
fn term_hash(term: &str, kind: i16) -> u128 {
    let mut buf = Vec::with_capacity(2 + term.len());
    buf.extend_from_slice(&kind.to_le_bytes());
    buf.extend_from_slice(term.as_bytes());
    xxh3_128(&buf)
}

/// Encode `term` to its dictionary `i64` identifier.
///
/// Creates a new dictionary row if the term has not been seen before.
/// The full 128-bit hash is stored in the `hash` column; the IDENTITY-
/// generated `id` is the dense join key used in VP tables.
pub fn encode(term: &str, kind: i16) -> i64 {
    let hash128 = term_hash(term, kind);

    // Fast path: encode cache hit.
    if let Some(id) = ENCODE_CACHE.with(|c| c.borrow_mut().get(&hash128).copied()) {
        return id;
    }

    let hash_bytes = hash128.to_be_bytes();

    // Upsert + lookup in a single round-trip.  The CTE inserts the term when it
    // is new (ON CONFLICT DO NOTHING) and the outer COALESCE returns the id
    // whether the row was just inserted or already existed.  This always returns
    // exactly one row, which is safe even in pgrx 0.17 where get_one_with_args
    // returns Err(InvalidPosition) on 0-row results.
    let id: i64 = Spi::get_one_with_args::<i64>(
        "WITH ins AS ( \
             INSERT INTO _pg_ripple.dictionary (hash, value, kind) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (hash) DO NOTHING \
             RETURNING id \
         ) \
         SELECT COALESCE( \
             (SELECT id FROM ins), \
             (SELECT id FROM _pg_ripple.dictionary WHERE hash = $1) \
         )",
        &[
            DatumWithOid::from(hash_bytes.as_slice()),
            DatumWithOid::from(term),
            DatumWithOid::from(kind),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary encode SPI error: {e}"))
    .unwrap_or_else(|| pgrx::error!("dictionary encode: no id returned for term"));

    ENCODE_CACHE.with(|c| c.borrow_mut().put(hash128, id));
    DECODE_CACHE.with(|c| c.borrow_mut().put(id, term.to_owned()));

    id
}

/// Encode a typed literal (`"value"^^<datatype>`) into the dictionary.
///
/// The hash is computed over `kind_le_bytes || value_utf8 || "^^<" || datatype_utf8 || ">"`,
/// so two literals with the same value but different datatypes always map to distinct IDs.
pub fn encode_typed_literal(value: &str, datatype: &str) -> i64 {
    // Build canonical form for hashing.
    let canonical = format!("\"{}\"^^<{}>", value, datatype);
    let hash128 = term_hash(&canonical, KIND_TYPED_LITERAL);

    if let Some(id) = ENCODE_CACHE.with(|c| c.borrow_mut().get(&hash128).copied()) {
        return id;
    }

    let hash_bytes = hash128.to_be_bytes();

    let id: i64 = Spi::get_one_with_args::<i64>(
        "WITH ins AS ( \
             INSERT INTO _pg_ripple.dictionary (hash, value, kind, datatype) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (hash) DO NOTHING \
             RETURNING id \
         ) \
         SELECT COALESCE( \
             (SELECT id FROM ins), \
             (SELECT id FROM _pg_ripple.dictionary WHERE hash = $1) \
         )",
        &[
            DatumWithOid::from(hash_bytes.as_slice()),
            DatumWithOid::from(value),
            DatumWithOid::from(KIND_TYPED_LITERAL),
            DatumWithOid::from(datatype),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary encode_typed_literal SPI error: {e}"))
    .unwrap_or_else(|| pgrx::error!("dictionary encode_typed_literal: no id returned"));

    ENCODE_CACHE.with(|c| c.borrow_mut().put(hash128, id));
    DECODE_CACHE.with(|c| c.borrow_mut().put(id, canonical));

    id
}

/// Encode a language-tagged literal (`"value"@lang`) into the dictionary.
pub fn encode_lang_literal(value: &str, lang: &str) -> i64 {
    let canonical = format!("\"{}\"@{}", value, lang);
    let hash128 = term_hash(&canonical, KIND_LANG_LITERAL);

    if let Some(id) = ENCODE_CACHE.with(|c| c.borrow_mut().get(&hash128).copied()) {
        return id;
    }

    let hash_bytes = hash128.to_be_bytes();

    let id: i64 = Spi::get_one_with_args::<i64>(
        "WITH ins AS ( \
             INSERT INTO _pg_ripple.dictionary (hash, value, kind, lang) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (hash) DO NOTHING \
             RETURNING id \
         ) \
         SELECT COALESCE( \
             (SELECT id FROM ins), \
             (SELECT id FROM _pg_ripple.dictionary WHERE hash = $1) \
         )",
        &[
            DatumWithOid::from(hash_bytes.as_slice()),
            DatumWithOid::from(value),
            DatumWithOid::from(KIND_LANG_LITERAL),
            DatumWithOid::from(lang),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary encode_lang_literal SPI error: {e}"))
    .unwrap_or_else(|| pgrx::error!("dictionary encode_lang_literal: no id returned"));

    ENCODE_CACHE.with(|c| c.borrow_mut().put(hash128, id));
    DECODE_CACHE.with(|c| c.borrow_mut().put(id, canonical));

    id
}

/// Encode a plain literal (no datatype, no language tag).
pub fn encode_plain_literal(value: &str) -> i64 {
    encode(value, KIND_LITERAL)
}

/// Full decoded representation of a dictionary entry.
pub struct TermInfo {
    pub value: String,
    pub kind: i16,
    pub datatype: Option<String>,
    pub lang: Option<String>,
}

/// Decode a dictionary `id` to its full representation (value, kind, datatype, lang).
///
/// Returns `None` if the id is not in the dictionary.
pub fn decode_full(id: i64) -> Option<TermInfo> {
    Spi::connect(|client| {
        client
            .select(
                "SELECT value, kind, datatype, lang \
                 FROM _pg_ripple.dictionary WHERE id = $1",
                Some(1),
                &[DatumWithOid::from(id)],
            )
            .unwrap_or_else(|e| pgrx::error!("dictionary decode_full SPI error: {e}"))
            .filter_map(|row| {
                let value: String = row.get::<String>(1).ok().flatten()?;
                let kind: i16 = row.get::<i16>(2).ok().flatten()?;
                let datatype: Option<String> = row.get::<String>(3).ok().flatten();
                let lang: Option<String> = row.get::<String>(4).ok().flatten();
                Some(TermInfo {
                    value,
                    kind,
                    datatype,
                    lang,
                })
            })
            .next()
    })
}

/// Format a dictionary entry as an N-Triples term string.
///
/// - IRI → `<iri>`
/// - Blank node → `_:id` (using dictionary sequence id for stable uniqueness)
/// - Plain literal → `"value"`
/// - Typed literal → `"value"^^<datatype>`
/// - Lang literal → `"value"@lang`
pub fn format_ntriples(id: i64) -> String {
    match decode_full(id) {
        None => format!("<unknown:{}>", id),
        Some(t) => format_ntriples_term(
            &t.value,
            t.kind,
            t.datatype.as_deref(),
            t.lang.as_deref(),
            id,
        ),
    }
}

/// Format from components.
pub fn format_ntriples_term(
    value: &str,
    kind: i16,
    datatype: Option<&str>,
    lang: Option<&str>,
    fallback_id: i64,
) -> String {
    match kind {
        k if k == KIND_IRI => format!("<{}>", value),
        k if k == KIND_BLANK => format!("_:b{}", fallback_id),
        k if k == KIND_LITERAL => format!("\"{}\"", escape_literal(value)),
        k if k == KIND_TYPED_LITERAL => {
            let dt = datatype.unwrap_or("http://www.w3.org/2001/XMLSchema#string");
            format!("\"{}\"^^<{}>", escape_literal(value), dt)
        }
        k if k == KIND_LANG_LITERAL => {
            let l = lang.unwrap_or("und");
            format!("\"{}\"@{}", escape_literal(value), l)
        }
        _ => format!("\"{}\"", escape_literal(value)),
    }
}

/// Escape a string value for N-Triples literal output.
fn escape_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// Decode a dictionary `id` back to its original term string.
///
/// Returns `None` if the id is not found in the dictionary.
pub fn decode(id: i64) -> Option<String> {
    if let Some(value) = DECODE_CACHE.with(|c| c.borrow_mut().get(&id).cloned()) {
        return Some(value);
    }

    // Use Spi::connect + select to safely handle 0-row results.  pgrx 0.17's
    // get_one_with_args returns Err(InvalidPosition) on empty results, which
    // would be misinterpreted as an error rather than "not found".
    let value: Option<String> = Spi::connect(|client| {
        let tbl = client
            .select(
                "SELECT value FROM _pg_ripple.dictionary WHERE id = $1",
                Some(1),
                &[DatumWithOid::from(id)],
            )
            .unwrap_or_else(|e| pgrx::error!("dictionary decode SPI error: {e}"));

        if tbl.is_empty() {
            None
        } else {
            tbl.first()
                .get_one::<String>()
                .unwrap_or_else(|e| pgrx::error!("dictionary decode SPI error: {e}"))
        }
    });

    if let Some(ref v) = value {
        DECODE_CACHE.with(|c| {
            c.borrow_mut().put(id, v.clone());
        });
    }

    value
}
