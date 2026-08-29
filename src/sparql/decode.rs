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
    // C13-02 (v0.85.0): respect pg_ripple.strict_dictionary GUC — raise PT512 error
    //   when strict mode is on and an ID is missing; keep the graceful-degradation
    //   WARNING otherwise.
    // C13-07 (v0.85.0): guard is `id == 0` (not `id <= 0`); negative IDs should not
    //   exist after the v0.81.0 dict-subxact fix but were previously silently passed.
    //   Skip id=0: well-known default-graph sentinel not stored in the dictionary.
    let strict = crate::gucs::storage::STRICT_DICTIONARY.get();
    for id in &dict_ids {
        if *id == 0 {
            continue;
        }
        if !result.contains_key(id) {
            if strict {
                pgrx::error!(
                    "PT512: dictionary entry missing for id {id}; \
                     cannot decode result (set pg_ripple.strict_dictionary = off \
                     to use empty-string placeholders instead)"
                );
            } else {
                pgrx::warning!(
                    "batch_decode: dictionary entry missing for id {id}; \
                     result binding will be empty string (possible dictionary corruption)"
                );
            }
        }
    }

    result
}

/// Decode dictionary term metadata for one result page in a single SPI query.
/// Inline IDs are absent because their type and lexical form are packed locally.
pub(crate) fn batch_decode_full(ids: &[i64]) -> HashMap<i64, dictionary::TermInfo> {
    let dict_ids: Vec<i64> = ids
        .iter()
        .copied()
        .filter(|id| !dictionary::inline::is_inline(*id))
        .collect();
    if dict_ids.is_empty() {
        return HashMap::new();
    }

    let ids_array: Vec<Option<i64>> = dict_ids.iter().copied().map(Some).collect();
    let mut result = HashMap::with_capacity(dict_ids.len());
    Spi::connect(|client| {
        let rows = client
            .select(
                "SELECT id, value, kind, datatype, lang, qt_s, qt_p, qt_o \
                 FROM _pg_ripple.dictionary \
                 WHERE id = ANY($1::bigint[])",
                None,
                &[pgrx::datum::DatumWithOid::from(ids_array.as_slice())],
            )
            .unwrap_or_else(|e| pgrx::error!("batch_decode_full SPI error: {e}"));
        for row in rows {
            let id = row
                .get::<i64>(1)
                .unwrap_or_else(|e| pgrx::error!("batch_decode_full id: {e}"))
                .unwrap_or(0);
            let value = row
                .get::<String>(2)
                .unwrap_or_else(|e| pgrx::error!("batch_decode_full value: {e}"))
                .unwrap_or_default();
            let kind = row
                .get::<i16>(3)
                .unwrap_or_else(|e| pgrx::error!("batch_decode_full kind: {e}"))
                .unwrap_or(0);
            let datatype = row.get::<String>(4).ok().flatten();
            let lang = row.get::<String>(5).ok().flatten();
            let quoted_triple = match (
                row.get::<i64>(6).ok().flatten(),
                row.get::<i64>(7).ok().flatten(),
                row.get::<i64>(8).ok().flatten(),
            ) {
                (Some(s), Some(p), Some(o)) => Some((s, p, o)),
                _ => None,
            };
            result.insert(
                id,
                dictionary::TermInfo {
                    value,
                    kind,
                    datatype,
                    lang,
                    quoted_triple,
                },
            );
        }
    });
    result
}
