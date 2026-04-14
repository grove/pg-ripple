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
