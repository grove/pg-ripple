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

pub mod hot;
pub mod inline;

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
/// RDF-star quoted triple: the `value` holds the canonical N-Triples-star form;
/// `qt_s`, `qt_p`, `qt_o` hold the component dictionary IDs.
pub const KIND_QUOTED_TRIPLE: i16 = 5;

thread_local! {
    /// Encode cache: full XXH3-128 hash → sequence-generated id.
    static ENCODE_CACHE: RefCell<LruCache<u128, i64>> = RefCell::new(
        // SAFETY: CACHE_CAPACITY is a compile-time non-zero literal (4096).
        #[allow(clippy::expect_used)]
        LruCache::new(NonZeroUsize::new(CACHE_CAPACITY).expect("capacity > 0"))
    );

    /// Shmem inserts made in the current transaction.
    ///
    /// Every hash128 that is inserted into the shared-memory encode cache
    /// during this transaction is tracked here.  On ROLLBACK, these entries
    /// are evicted from shmem so that stale hash→id mappings cannot poison
    /// subsequent transactions (the dictionary rows are rolled back but the
    /// shmem entries would otherwise persist indefinitely).
    static TX_SHMEM_INSERTS: RefCell<Vec<u128>> = const { RefCell::new(Vec::new()) };
    /// Decode cache: sequence id → term value.
    static DECODE_CACHE: RefCell<LruCache<i64, String>> = RefCell::new(
        // SAFETY: CACHE_CAPACITY is a compile-time non-zero literal (4096).
        #[allow(clippy::expect_used)]
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
///
/// Lookup order:
/// 1. Shared-memory encode cache (v0.6.0 — shared across all backends)
/// 2. Backend-local LRU cache (fast, no lock)
/// 3. SPI round-trip to `_pg_ripple.dictionary`
pub fn encode(term: &str, kind: i16) -> i64 {
    // v0.55.0 C-1: NFC normalize IRIs (and blank nodes) when NORMALIZE_IRIS=true.
    // This ensures that semantically identical IRIs with different Unicode
    // representations hash to the same dictionary entry.
    if (kind == KIND_IRI || kind == KIND_BLANK) && crate::NORMALIZE_IRIS.get() {
        use unicode_normalization::UnicodeNormalization;
        let normalized: String = term.nfc().collect();
        return encode_inner(&normalized, kind);
    }
    encode_inner(term, kind)
}

fn encode_inner(term: &str, kind: i16) -> i64 {
    let hash128 = term_hash(term, kind);

    // Tier 1: shared-memory encode cache (v0.6.0).
    if let Some(id) = crate::shmem::encode_cache_lookup(hash128) {
        // Warm the backend-local cache too so subsequent lookups cost nothing.
        ENCODE_CACHE.with(|c| c.borrow_mut().put(hash128, id));
        return id;
    }

    // Tier 2: backend-local LRU cache.
    if let Some(id) = ENCODE_CACHE.with(|c| c.borrow_mut().get(&hash128).copied()) {
        return id;
    }

    // DICT-01: split u128 into (hi, lo) i64 pair — avoids varlena BYTEA overhead.
    let hash_hi = (hash128 >> 64) as i64;
    let hash_lo = hash128 as i64;

    // Tier 3: upsert + lookup in a single SPI round-trip.  The CTE inserts the
    // term when it is new (ON CONFLICT DO NOTHING) and the outer COALESCE
    // returns the id whether the row was just inserted or already existed.
    let id: i64 = Spi::get_one_with_args::<i64>(
        "WITH ins AS ( \
             INSERT INTO _pg_ripple.dictionary (hash_hi, hash_lo, value, kind) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (hash_hi, hash_lo) DO NOTHING \
             RETURNING id \
         ) \
         SELECT COALESCE( \
             (SELECT id FROM ins), \
             (SELECT id FROM _pg_ripple.dictionary WHERE hash_hi = $1 AND hash_lo = $2) \
         )",
        &[
            DatumWithOid::from(hash_hi),
            DatumWithOid::from(hash_lo),
            DatumWithOid::from(term),
            DatumWithOid::from(kind),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary encode SPI error: {e}"))
    .unwrap_or_else(|| pgrx::error!("dictionary encode: no id returned for term"));

    // Populate both caches.
    crate::shmem::encode_cache_insert(hash128, id);
    TX_SHMEM_INSERTS.with(|v| v.borrow_mut().push(hash128));
    ENCODE_CACHE.with(|c| c.borrow_mut().put(hash128, id));
    DECODE_CACHE.with(|c| c.borrow_mut().put(id, term.to_owned()));

    id
}

/// Encode a typed literal (`"value"^^<datatype>`) into the dictionary.
///
/// For `xsd:integer`, `xsd:boolean`, `xsd:dateTime`, and `xsd:date`, the
/// value is encoded inline (bit 63 = 1) — no dictionary row is inserted.
/// All other typed literals are stored in the dictionary as usual.
pub fn encode_typed_literal(value: &str, datatype: &str) -> i64 {
    // Fast path: try inline encoding for supported numeric / date types.
    let inline_id = match datatype {
        "http://www.w3.org/2001/XMLSchema#integer"
        | "http://www.w3.org/2001/XMLSchema#long"
        | "http://www.w3.org/2001/XMLSchema#int"
        | "http://www.w3.org/2001/XMLSchema#short"
        | "http://www.w3.org/2001/XMLSchema#byte"
        | "http://www.w3.org/2001/XMLSchema#nonNegativeInteger"
        | "http://www.w3.org/2001/XMLSchema#positiveInteger"
        | "http://www.w3.org/2001/XMLSchema#negativeInteger"
        | "http://www.w3.org/2001/XMLSchema#nonPositiveInteger" => {
            inline::try_encode_integer(value)
        }
        "http://www.w3.org/2001/XMLSchema#boolean" => inline::try_encode_boolean(value),
        "http://www.w3.org/2001/XMLSchema#dateTime" => inline::try_encode_datetime(value),
        "http://www.w3.org/2001/XMLSchema#date" => inline::try_encode_date(value),
        _ => None,
    };
    if let Some(id) = inline_id {
        return id;
    }

    // Build canonical form for hashing.
    let canonical = format!("\"{}\"^^<{}>", value, datatype);
    let hash128 = term_hash(&canonical, KIND_TYPED_LITERAL);

    // Tier 1: shared-memory encode cache.
    if let Some(id) = crate::shmem::encode_cache_lookup(hash128) {
        ENCODE_CACHE.with(|c| c.borrow_mut().put(hash128, id));
        return id;
    }
    // Tier 2: backend-local cache.
    if let Some(id) = ENCODE_CACHE.with(|c| c.borrow_mut().get(&hash128).copied()) {
        return id;
    }

    let hash_hi = (hash128 >> 64) as i64;
    let hash_lo = hash128 as i64;

    let id: i64 = Spi::get_one_with_args::<i64>(
        "WITH ins AS ( \
             INSERT INTO _pg_ripple.dictionary (hash_hi, hash_lo, value, kind, datatype) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (hash_hi, hash_lo) DO NOTHING \
             RETURNING id \
         ) \
         SELECT COALESCE( \
             (SELECT id FROM ins), \
             (SELECT id FROM _pg_ripple.dictionary WHERE hash_hi = $1 AND hash_lo = $2) \
         )",
        &[
            DatumWithOid::from(hash_hi),
            DatumWithOid::from(hash_lo),
            DatumWithOid::from(value),
            DatumWithOid::from(KIND_TYPED_LITERAL),
            DatumWithOid::from(datatype),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary encode_typed_literal SPI error: {e}"))
    .unwrap_or_else(|| pgrx::error!("dictionary encode_typed_literal: no id returned"));

    crate::shmem::encode_cache_insert(hash128, id);
    TX_SHMEM_INSERTS.with(|v| v.borrow_mut().push(hash128));
    ENCODE_CACHE.with(|c| c.borrow_mut().put(hash128, id));
    DECODE_CACHE.with(|c| c.borrow_mut().put(id, canonical));

    id
}

/// Encode a language-tagged literal (`"value"@lang`) into the dictionary.
pub fn encode_lang_literal(value: &str, lang: &str) -> i64 {
    let canonical = format!("\"{}\"@{}", value, lang);
    let hash128 = term_hash(&canonical, KIND_LANG_LITERAL);

    // Tier 1: shared-memory encode cache.
    if let Some(id) = crate::shmem::encode_cache_lookup(hash128) {
        ENCODE_CACHE.with(|c| c.borrow_mut().put(hash128, id));
        return id;
    }
    // Tier 2: backend-local cache.
    if let Some(id) = ENCODE_CACHE.with(|c| c.borrow_mut().get(&hash128).copied()) {
        return id;
    }

    let hash_hi = (hash128 >> 64) as i64;
    let hash_lo = hash128 as i64;

    let id: i64 = Spi::get_one_with_args::<i64>(
        "WITH ins AS ( \
             INSERT INTO _pg_ripple.dictionary (hash_hi, hash_lo, value, kind, lang) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (hash_hi, hash_lo) DO NOTHING \
             RETURNING id \
         ) \
         SELECT COALESCE( \
             (SELECT id FROM ins), \
             (SELECT id FROM _pg_ripple.dictionary WHERE hash_hi = $1 AND hash_lo = $2) \
         )",
        &[
            DatumWithOid::from(hash_hi),
            DatumWithOid::from(hash_lo),
            DatumWithOid::from(value),
            DatumWithOid::from(KIND_LANG_LITERAL),
            DatumWithOid::from(lang),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary encode_lang_literal SPI error: {e}"))
    .unwrap_or_else(|| pgrx::error!("dictionary encode_lang_literal: no id returned"));

    crate::shmem::encode_cache_insert(hash128, id);
    TX_SHMEM_INSERTS.with(|v| v.borrow_mut().push(hash128));
    ENCODE_CACHE.with(|c| c.borrow_mut().put(hash128, id));
    DECODE_CACHE.with(|c| c.borrow_mut().put(id, canonical));

    id
}

/// Encode a plain literal (no datatype, no language tag).
pub fn encode_plain_literal(value: &str) -> i64 {
    encode(value, KIND_LITERAL)
}

// ─── Quoted triple (RDF-star) encoding ───────────────────────────────────────

/// Compute the XXH3-128 hash for a quoted triple `(s_id, p_id, o_id)`.
///
/// Mixing in `KIND_QUOTED_TRIPLE` as a prefix guarantees that the same i64
/// triple never collides with an IRI or literal hash.
fn quoted_triple_hash(s_id: i64, p_id: i64, o_id: i64) -> u128 {
    let mut buf = [0u8; 2 + 8 + 8 + 8];
    buf[0..2].copy_from_slice(&KIND_QUOTED_TRIPLE.to_le_bytes());
    buf[2..10].copy_from_slice(&s_id.to_le_bytes());
    buf[10..18].copy_from_slice(&p_id.to_le_bytes());
    buf[18..26].copy_from_slice(&o_id.to_le_bytes());
    xxh3_128(&buf)
}

/// Encode a quoted triple `(s_id, p_id, o_id)` into the dictionary.
///
/// The `value` column stores the canonical N-Triples-star representation
/// (computed lazily at insert time).  The `qt_s`, `qt_p`, `qt_o` columns
/// hold the component dictionary IDs so they can be reconstructed without
/// re-parsing the value string.
pub fn encode_quoted_triple(s_id: i64, p_id: i64, o_id: i64) -> i64 {
    let hash128 = quoted_triple_hash(s_id, p_id, o_id);

    // Tier 1: shared-memory encode cache.
    if let Some(id) = crate::shmem::encode_cache_lookup(hash128) {
        ENCODE_CACHE.with(|c| c.borrow_mut().put(hash128, id));
        return id;
    }
    // Tier 2: backend-local cache.
    if let Some(id) = ENCODE_CACHE.with(|c| c.borrow_mut().get(&hash128).copied()) {
        return id;
    }

    let hash_hi = (hash128 >> 64) as i64;
    let hash_lo = hash128 as i64;
    // Build canonical value lazily — only stored once at insert time.
    let canonical = format!(
        "<< {} {} {} >>",
        format_ntriples(s_id),
        format_ntriples(p_id),
        format_ntriples(o_id)
    );

    let id: i64 = Spi::get_one_with_args::<i64>(
        "WITH ins AS ( \
             INSERT INTO _pg_ripple.dictionary (hash_hi, hash_lo, value, kind, qt_s, qt_p, qt_o) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (hash_hi, hash_lo) DO NOTHING \
             RETURNING id \
         ) \
         SELECT COALESCE( \
             (SELECT id FROM ins), \
             (SELECT id FROM _pg_ripple.dictionary WHERE hash_hi = $1 AND hash_lo = $2) \
         )",
        &[
            DatumWithOid::from(hash_hi),
            DatumWithOid::from(hash_lo),
            DatumWithOid::from(canonical.as_str()),
            DatumWithOid::from(KIND_QUOTED_TRIPLE),
            DatumWithOid::from(s_id),
            DatumWithOid::from(p_id),
            DatumWithOid::from(o_id),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary encode_quoted_triple SPI error: {e}"))
    .unwrap_or_else(|| pgrx::error!("dictionary encode_quoted_triple: no id returned"));

    crate::shmem::encode_cache_insert(hash128, id);
    TX_SHMEM_INSERTS.with(|v| v.borrow_mut().push(hash128));
    ENCODE_CACHE.with(|c| c.borrow_mut().put(hash128, id));
    DECODE_CACHE.with(|c| c.borrow_mut().put(id, canonical));
    id
}

/// Look up a quoted triple without inserting.
///
/// Returns `None` if the quoted triple has never been stored.
pub fn lookup_quoted_triple(s_id: i64, p_id: i64, o_id: i64) -> Option<i64> {
    let hash128 = quoted_triple_hash(s_id, p_id, o_id);
    if let Some(id) = ENCODE_CACHE.with(|c| c.borrow_mut().get(&hash128).copied()) {
        return Some(id);
    }
    let hash_hi = (hash128 >> 64) as i64;
    let hash_lo = hash128 as i64;
    Spi::connect(|client| {
        let tbl = client
            .select(
                "SELECT id FROM _pg_ripple.dictionary WHERE hash_hi = $1 AND hash_lo = $2",
                Some(1),
                &[DatumWithOid::from(hash_hi), DatumWithOid::from(hash_lo)],
            )
            .unwrap_or_else(|e| pgrx::error!("lookup_quoted_triple SPI error: {e}"));
        if tbl.is_empty() {
            None
        } else {
            tbl.first()
                .get_one::<i64>()
                .unwrap_or_else(|e| pgrx::error!("lookup_quoted_triple decode SPI error: {e}"))
        }
    })
}

/// Decode a quoted triple ID back to its component dictionary IDs `(s_id, p_id, o_id)`.
///
/// Returns `None` if the ID is not a quoted triple in the dictionary.
pub fn decode_quoted_triple_components(id: i64) -> Option<(i64, i64, i64)> {
    Spi::connect(|client| {
        client
            .select(
                "SELECT qt_s, qt_p, qt_o FROM _pg_ripple.dictionary \
                 WHERE id = $1 AND kind = $2",
                Some(1),
                &[
                    DatumWithOid::from(id),
                    DatumWithOid::from(KIND_QUOTED_TRIPLE),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("decode_quoted_triple_components SPI error: {e}"))
            .filter_map(|row| {
                let s: i64 = row.get::<i64>(1).ok().flatten()?;
                let p: i64 = row.get::<i64>(2).ok().flatten()?;
                let o: i64 = row.get::<i64>(3).ok().flatten()?;
                Some((s, p, o))
            })
            .next()
    })
}

/// Look up a term in the dictionary **without inserting** it.
///
/// Returns `None` if the term has never been stored.  Used by the SPARQL
/// translator to check whether a predicate IRI exists before generating SQL —
/// avoids polluting the dictionary with IRI strings from queries on empty
/// datasets.
pub fn lookup(term: &str, kind: i16) -> Option<i64> {
    let hash128 = term_hash(term, kind);

    // Fast path: encode cache already has the id.
    if let Some(id) = ENCODE_CACHE.with(|c| c.borrow_mut().get(&hash128).copied()) {
        return Some(id);
    }

    let hash_hi = (hash128 >> 64) as i64;
    let hash_lo = hash128 as i64;

    let id: Option<i64> = Spi::connect(|client| {
        let tbl = client
            .select(
                "SELECT id FROM _pg_ripple.dictionary WHERE hash_hi = $1 AND hash_lo = $2",
                Some(1),
                &[
                    pgrx::datum::DatumWithOid::from(hash_hi),
                    pgrx::datum::DatumWithOid::from(hash_lo),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("dictionary lookup SPI error: {e}"));

        if tbl.is_empty() {
            None
        } else {
            tbl.first()
                .get_one::<i64>()
                .unwrap_or_else(|e| pgrx::error!("dictionary lookup SPI error: {e}"))
        }
    });

    if let Some(i) = id {
        ENCODE_CACHE.with(|c| c.borrow_mut().put(hash128, i));
    }
    id
}

/// Look up an IRI (kind=0) without inserting.
pub fn lookup_iri(iri: &str) -> Option<i64> {
    lookup(iri, KIND_IRI)
}

/// Return `true` if `id` is a blank-node dictionary entry.
///
/// The function handles inline IDs (which are never blank nodes) gracefully.
pub fn is_blank_node(id: i64) -> bool {
    if inline::is_inline(id) {
        return false;
    }
    Spi::connect(|client| {
        client
            .select(
                "SELECT kind FROM _pg_ripple.dictionary WHERE id = $1 LIMIT 1",
                Some(1),
                &[DatumWithOid::from(id)],
            )
            .ok()
            .and_then(|rows| {
                rows.filter_map(|row| row.get::<i16>(1).ok().flatten())
                    .next()
            })
            .map(|k| k == KIND_BLANK)
            .unwrap_or(false)
    })
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
/// - Inline value → decoded literal, e.g. `"42"^^<xsd:integer>`
pub fn format_ntriples(id: i64) -> String {
    // Inline-encoded values (bit 63 = 1) are decoded without a DB round-trip.
    if inline::is_inline(id) {
        return inline::format_inline(id);
    }
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
        // KIND_QUOTED_TRIPLE: the `value` column already stores the canonical
        // N-Triples-star form `<< ... >>`, so we can reuse it directly.
        k if k == KIND_QUOTED_TRIPLE => value.to_owned(),
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
/// Inline-encoded values (bit 63 = 1) are decoded without a DB round-trip.
pub fn decode(id: i64) -> Option<String> {
    // Inline values are always decodable without a DB round-trip.
    if inline::is_inline(id) {
        return Some(inline::format_inline(id));
    }

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

/// Clear both the encode and decode thread-local caches.
///
/// Called on transaction abort (XACT_EVENT_ABORT, XACT_EVENT_PARALLEL_ABORT)
/// to ensure rolled-back dictionary entries do not leak into future transactions.
/// This is critical for correctness: if a transaction is rolled back, any
/// dictionary IDs inserted during that transaction should not be served by
/// the cache in subsequent encode calls, as the dictionary rows themselves
/// have been rolled back (v0.22.0 critical fix C-2).
///
/// v0.42.0: Also evicts entries from the shared-memory encode cache so that
/// stale hash→id mappings do not leak across transaction boundaries.  Any
/// IRI/literal encoded during a rolled-back transaction would otherwise remain
/// in shmem, causing subsequent transactions to skip the SPI INSERT (shmem
/// hit) and store VP triples with non-existent dictionary IDs.
pub(crate) fn clear_caches() {
    // Evict shmem entries that were inserted in this (now-aborting) transaction.
    TX_SHMEM_INSERTS.with(|v| {
        for &hash128 in v.borrow().iter() {
            crate::shmem::encode_cache_evict(hash128);
        }
        v.borrow_mut().clear();
    });
    ENCODE_CACHE.with(|c| {
        c.borrow_mut().clear();
    });
    DECODE_CACHE.with(|c| {
        c.borrow_mut().clear();
    });
}

/// Clear the per-transaction shmem-insert tracking list after a successful
/// commit.  The shmem entries themselves are correct (their dictionary rows
/// were committed), so we only need to drop the tracking list — no eviction.
pub(crate) fn commit_cleanup() {
    TX_SHMEM_INSERTS.with(|v| v.borrow_mut().clear());
}
