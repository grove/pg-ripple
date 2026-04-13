//! Dictionary encoder — maps RDF terms to i64 identifiers.
//!
//! Every IRI, blank node, and literal is encoded to a compact `i64` before
//! being stored in a VP table.  The encoding is:
//!
//! 1. Compute XXH3-128 of the UTF-8 bytes of the term string.
//! 2. Cast the high 64 bits to `i64` (the low 64 bits are stored as the
//!    `hash` column for collision detection).
//! 3. Attempt `INSERT INTO _pg_ripple.dictionary … ON CONFLICT DO NOTHING`.
//! 4. Return the `id` column (which equals the XXH3-128 high half).
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
//! # Backend-local cache (v0.1.0–v0.5.1)
//!
//! Each backend maintains an `LruCache<i64, String>` for the decode path.
//! A shared-memory dictionary cache is introduced in v0.6.0.

use lru::LruCache;
use pgrx::prelude::*;
use std::cell::RefCell;
use std::num::NonZeroUsize;
use xxhash_rust::xxh3::xxh3_128;

/// Maximum number of entries in the backend-local decode cache.
const CACHE_CAPACITY: usize = 16_384;

/// Encode a term kind integer into a PostgreSQL SMALLINT.
pub const KIND_IRI: i16 = 0;
pub const KIND_BLANK: i16 = 1;
pub const KIND_LITERAL: i16 = 2;
pub const KIND_TYPED_LITERAL: i16 = 3;
pub const KIND_LANG_LITERAL: i16 = 4;

thread_local! {
    static DECODE_CACHE: RefCell<LruCache<i64, String>> = RefCell::new(
        LruCache::new(NonZeroUsize::new(CACHE_CAPACITY).expect("capacity > 0"))
    );
}

/// Encode `term` to its dictionary `i64` identifier.
///
/// Creates a new dictionary row if `term` has not been seen before.
/// Returns the `id` (XXH3-128 high 64 bits cast to i64).
pub fn encode(term: &str, kind: i16) -> i64 {
    let hash128 = xxh3_128(term.as_bytes());
    // Use the high 64 bits as the ID.
    let id = (hash128 >> 64) as i64;
    let low = hash128 as i64;

    Spi::run_with_args(
        "INSERT INTO _pg_ripple.dictionary (id, hash, value, kind) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (id) DO NOTHING",
        Some(vec![
            (PgBuiltInOids::INT8OID.oid(), id.into_datum()),
            (PgBuiltInOids::INT8OID.oid(), low.into_datum()),
            (PgBuiltInOids::TEXTOID.oid(), term.into_datum()),
            (PgBuiltInOids::INT2OID.oid(), kind.into_datum()),
        ]),
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary encode SPI error: {e}"));

    // Warm the decode cache.
    DECODE_CACHE.with(|c| {
        c.borrow_mut().put(id, term.to_owned());
    });

    id
}

/// Decode a dictionary `id` back to its original term string.
///
/// Returns `None` if the id is not found in the dictionary.
pub fn decode(id: i64) -> Option<String> {
    // Check the backend-local cache first.
    if let Some(value) = DECODE_CACHE.with(|c| c.borrow_mut().get(&id).cloned()) {
        return Some(value);
    }

    let value = Spi::get_one_with_args::<String>(
        "SELECT value FROM _pg_ripple.dictionary WHERE id = $1",
        vec![(PgBuiltInOids::INT8OID.oid(), id.into_datum())],
    )
    .unwrap_or_else(|e| pgrx::error!("dictionary decode SPI error: {e}"));

    if let Some(ref v) = value {
        DECODE_CACHE.with(|c| {
            c.borrow_mut().put(id, v.clone());
        });
    }

    value
}
