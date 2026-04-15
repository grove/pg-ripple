//! Full-text search on RDF literal objects.
//!
//! # How it works
//!
//! For each predicate IRI that is FTS-indexed, we create a GIN `tsvector` index
//! on the dictionary `value` column restricted to entries that are objects of
//! that predicate.  At search time we execute a parameterised SPI query that
//! joins the VP table (or `vp_rare`) with the dictionary on the `o` column and
//! applies a `@@` operator against the `tsquery`.
//!
//! The index is defined as a partial index:
//!
//! ```sql
//! CREATE INDEX IF NOT EXISTS pg_ripple_fts_{pred_id}
//! ON _pg_ripple.dictionary
//! USING GIN (to_tsvector('english', value))
//! WHERE id IN (SELECT o FROM _pg_ripple.vp_{pred_id})
//! ```
//!
//! For rare predicates the index is on `vp_rare`:
//!
//! ```sql
//! CREATE INDEX IF NOT EXISTS pg_ripple_fts_rare_{pred_id}
//! ON _pg_ripple.vp_rare (o)
//! WHERE p = {pred_id}
//! ```
//! combined with a dictionary GIN index.

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

use crate::dictionary;

/// Create a GIN full-text search index for the given predicate.
///
/// Looks up the predicate IRI in the dictionary, then creates a partial GIN
/// `tsvector` index on `_pg_ripple.dictionary(value)` filtered to object IDs
/// of that predicate.
///
/// Returns the predicate dictionary ID.
pub fn fts_index(predicate: &str) -> i64 {
    // Accept both raw IRI and N-Triples notation (<IRI>).
    let predicate = predicate
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(predicate);
    let pred_id = match dictionary::lookup_iri(predicate) {
        Some(id) => id,
        None => pgrx::error!(
            "predicate '{}' not found in dictionary — load some triples first",
            predicate
        ),
    };

    // Check if a dedicated VP table exists for this predicate.
    let table_oid: Option<i64> = Spi::get_one_with_args::<i64>(
        "SELECT table_oid::bigint FROM _pg_ripple.predicates WHERE id = $1",
        &[DatumWithOid::from(pred_id)],
    )
    .unwrap_or(None);

    // PostgreSQL partial index predicates may not contain subqueries, so we
    // create a single shared GIN index on the dictionary value column restricted
    // to plain string literals (kind = 2).  The fts_search query handles
    // predicate filtering via a JOIN to the VP table.
    let _ = table_oid; // consumed only for logging; not used for index type
    let idx_name = format!("pg_ripple_fts_{pred_id}");
    let sql = format!(
        "CREATE INDEX IF NOT EXISTS {idx_name} \
         ON _pg_ripple.dictionary \
         USING GIN (to_tsvector('english', value)) \
         WHERE kind = 2"
    );
    Spi::run_with_args(&sql, &[]).unwrap_or_else(|e| pgrx::error!("fts_index SPI error: {e}"));

    pred_id
}

/// Full-text search on literal objects of a given predicate.
///
/// Executes a `tsquery` match against the `value` column of the dictionary,
/// restricted to objects of the specified predicate.  Returns `(s, p, o)` as
/// N-Triples–formatted strings.
pub fn fts_search(
    tsquery: &str,
    predicate: &str,
) -> impl Iterator<Item = (String, String, String)> + 'static {
    // Accept both raw IRI and N-Triples notation (<IRI>).
    let predicate = predicate
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(predicate);
    let pred_id = match dictionary::lookup_iri(predicate) {
        Some(id) => id,
        None => return Vec::new().into_iter(),
    };

    // Determine which table to query.
    let table_oid: Option<i64> = Spi::get_one_with_args::<i64>(
        "SELECT table_oid::bigint FROM _pg_ripple.predicates WHERE id = $1",
        &[DatumWithOid::from(pred_id)],
    )
    .unwrap_or(None);

    // Escape tsquery string to prevent injection.  The tsquery value comes from
    // a user-supplied SQL parameter, so we pass it via $N binding.
    let rows: Vec<(i64, i64)> = if table_oid.is_some() {
        // Dedicated VP table.
        let vp_table = format!("_pg_ripple.vp_{pred_id}");
        Spi::connect(|client| {
            client
                .select(
                    &format!(
                        "SELECT vp.s, vp.o \
                         FROM {vp_table} vp \
                         JOIN _pg_ripple.dictionary d ON d.id = vp.o \
                         WHERE to_tsvector('english', d.value) @@ to_tsquery('english', $1)"
                    ),
                    None,
                    &[DatumWithOid::from(tsquery)],
                )
                .unwrap_or_else(|e| pgrx::error!("fts_search SPI error: {e}"))
                .filter_map(|row| {
                    Some((
                        row.get::<i64>(1).ok().flatten()?,
                        row.get::<i64>(2).ok().flatten()?,
                    ))
                })
                .collect()
        })
    } else {
        // Rare predicate.
        Spi::connect(|client| {
            client
                .select(
                    "SELECT vr.s, vr.o \
                     FROM _pg_ripple.vp_rare vr \
                     JOIN _pg_ripple.dictionary d ON d.id = vr.o \
                     WHERE vr.p = $1 \
                       AND to_tsvector('english', d.value) @@ to_tsquery('english', $2)",
                    None,
                    &[DatumWithOid::from(pred_id), DatumWithOid::from(tsquery)],
                )
                .unwrap_or_else(|e| pgrx::error!("fts_search rare SPI error: {e}"))
                .filter_map(|row| {
                    Some((
                        row.get::<i64>(1).ok().flatten()?,
                        row.get::<i64>(2).ok().flatten()?,
                    ))
                })
                .collect()
        })
    };

    let pred_ntriples = format!("<{predicate}>");
    let result: Vec<(String, String, String)> = rows
        .into_iter()
        .map(|(s_id, o_id)| {
            (
                dictionary::format_ntriples(s_id),
                pred_ntriples.clone(),
                dictionary::format_ntriples(o_id),
            )
        })
        .collect();

    result.into_iter()
}
