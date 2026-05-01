//! Batch dictionary decoding for SPARQL query results.
//!
//! Provides `batch_decode`, which converts a slice of `i64` dictionary IDs
//! into N-Triples-formatted strings in a single SPI round-trip.

use std::collections::HashMap;

use pgrx::prelude::*;

use crate::dictionary;

// ─── Batch dictionary decode ──────────────────────────────────────────────────

/// Decode a set of `i64` dictionary IDs to N-Triples–formatted strings in one
/// SPI round-trip.  Inline-encoded values (bit 63 = 1) are decoded directly
/// without a DB lookup; only true dictionary IDs are fetched from the table.
///
/// # DECODE-BIND-01 (v0.82.0)
/// Uses `WHERE id = ANY($1::bigint[])` with a bind parameter instead of
/// `WHERE id IN (id1, id2, …)` with integer literals.  The bind-parameter form
/// shares a single query plan across all call sites regardless of cardinality,
/// eliminating plan-cache churn under high-concurrency workloads.
///
/// # DECODE-WARN-01 (v0.82.0)
/// After the SPI query, any requested ID that is absent from the dictionary
/// produces a WARNING.  Callers still receive an empty-string binding (existing
/// graceful-degradation behaviour), but the WARNING allows operators to detect
/// dictionary corruption early.
pub(crate) fn batch_decode(ids: &[i64]) -> HashMap<i64, String> {
    if ids.is_empty() {
        return HashMap::new();
    }

    let mut result = HashMap::with_capacity(ids.len());

    // Split: inline IDs (negative) are decoded locally; positives need DB lookup.
    let dict_ids: Vec<i64> = ids
        .iter()
        .copied()
        .filter(|&id| {
            if dictionary::inline::is_inline(id) {
                result.insert(id, dictionary::inline::format_inline(id));
                false
            } else {
                true
            }
        })
        .collect();

    if dict_ids.is_empty() {
        return result;
    }

    // DECODE-BIND-01 (v0.82.0): use ANY($1::bigint[]) bind parameter so PostgreSQL
    // generates one query plan for all call sites regardless of the number of IDs.
    // The array is passed as a DatumArray<INT8OID>.
    let sql = "SELECT id, value, kind, datatype, lang \
               FROM _pg_ripple.dictionary \
               WHERE id = ANY($1::bigint[])";

    let ids_array: Vec<Option<i64>> = dict_ids.iter().map(|&id| Some(id)).collect();

    Spi::connect(|client| {
        let rows = client
            .select(
                sql,
                None,
                &[pgrx::datum::DatumWithOid::from(ids_array.as_slice())],
            )
            .unwrap_or_else(|e| pgrx::error!("batch_decode SPI error: {e}"));
        for row in rows {
            let id: i64 = row
                .get::<i64>(1)
                .unwrap_or_else(|e| pgrx::error!("batch_decode id: {e}"))
                .unwrap_or(0);
            let value: String = row
                .get::<String>(2)
                .unwrap_or_else(|e| pgrx::error!("batch_decode value: {e}"))
                .unwrap_or_default();
            let kind: i16 = row
                .get::<i16>(3)
                .unwrap_or_else(|e| pgrx::error!("batch_decode kind: {e}"))
                .unwrap_or(0);
            let datatype: Option<String> = row.get::<String>(4).ok().flatten();
            let lang: Option<String> = row.get::<String>(5).ok().flatten();
            let term_str = dictionary::format_ntriples_term(
                &value,
                kind,
                datatype.as_deref(),
                lang.as_deref(),
                id,
            );
            result.insert(id, term_str);
        }
    });

    // DECODE-WARN-01 (v0.82.0): warn on any requested ID absent from the dictionary.
    // This detects dictionary corruption without changing fallback behaviour.
    // Skip id=0: that is the well-known default-graph sentinel which is intentionally
    // not stored in the dictionary.  Also skip negative IDs (error sentinels).
    for id in &dict_ids {
        if *id <= 0 {
            continue;
        }
        if !result.contains_key(id) {
            pgrx::warning!(
                "batch_decode: dictionary entry missing for id {id}; \
                 result binding will be empty string (possible dictionary corruption)"
            );
        }
    }

    result
}
