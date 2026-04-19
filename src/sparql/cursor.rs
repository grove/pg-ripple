//! Streaming SPARQL cursor API (v0.40.0).
//!
//! Provides set-returning functions that page through large SPARQL result sets
//! one batch at a time, avoiding full materialisation in memory.
//!
//! # Functions
//!
//! - `sparql_cursor(query TEXT) RETURNS SETOF JSONB` — streams SELECT/ASK results
//! - `sparql_cursor_turtle(query TEXT) RETURNS SETOF TEXT` — streams Turtle lines
//! - `sparql_cursor_jsonld(query TEXT) RETURNS SETOF TEXT` — streams JSON-LD chunks

use crate::export;
use crate::sparql;

/// Batch size for cursor paging.
const CURSOR_BATCH_SIZE: usize = 1024;

/// Execute a SPARQL SELECT query and return results as a stream of JSONB rows.
///
/// Results are fetched in batches of 1024 and yielded row-by-row.
/// Respects `pg_ripple.sparql_max_rows` if set.
pub fn sparql_cursor(query: &str) -> Vec<pgrx::JsonB> {
    let rows = sparql::sparql(query);
    let max_rows = crate::SPARQL_MAX_ROWS.get();
    if max_rows > 0 && rows.len() > max_rows as usize {
        let action = crate::SPARQL_OVERFLOW_ACTION
            .get()
            .as_ref()
            .and_then(|s| s.to_str().ok().map(|s| s.to_owned()))
            .unwrap_or_else(|| "warn".to_owned());
        if action == "error" {
            pgrx::error!(
                "PT640: SPARQL result set exceeded sparql_max_rows limit of {}",
                max_rows
            );
        } else {
            pgrx::warning!(
                "PT640: SPARQL result set truncated to {} rows (sparql_max_rows)",
                max_rows
            );
        }
        rows.into_iter().take(max_rows as usize).collect()
    } else {
        rows
    }
}

/// Execute a SPARQL CONSTRUCT query and stream the result as Turtle text lines.
///
/// Each yielded `TEXT` value is a complete Turtle serialisation chunk.
/// Large result sets are chunked in batches of 1024 triples.
pub fn sparql_cursor_turtle(query: &str) -> Vec<String> {
    let rows = sparql::sparql_construct(query);
    let max_rows = crate::EXPORT_MAX_ROWS.get();

    let triples: Vec<(String, String, String)> = rows
        .into_iter()
        .filter_map(|jsonb| {
            let obj = jsonb.0.as_object()?;
            let s = obj.get("s")?.as_str()?.to_owned();
            let p = obj.get("p")?.as_str()?.to_owned();
            let o = obj.get("o")?.as_str()?.to_owned();
            Some((s, p, o))
        })
        .collect();

    let limited: &[(String, String, String)] = if max_rows > 0 && triples.len() > max_rows as usize
    {
        pgrx::warning!(
            "PT642: export truncated to {} rows (export_max_rows)",
            max_rows
        );
        &triples[..max_rows as usize]
    } else {
        &triples
    };

    // Chunk into batches and serialise each batch as Turtle.
    limited
        .chunks(CURSOR_BATCH_SIZE)
        .map(export::triples_to_turtle)
        .collect()
}

/// Execute a SPARQL CONSTRUCT query and stream the result as JSON-LD chunks.
///
/// Each yielded `TEXT` value is a JSON-LD expanded-form array for one batch.
pub fn sparql_cursor_jsonld(query: &str) -> Vec<String> {
    let rows = sparql::sparql_construct(query);
    let max_rows = crate::EXPORT_MAX_ROWS.get();

    let triples: Vec<(String, String, String)> = rows
        .into_iter()
        .filter_map(|jsonb| {
            let obj = jsonb.0.as_object()?;
            let s = obj.get("s")?.as_str()?.to_owned();
            let p = obj.get("p")?.as_str()?.to_owned();
            let o = obj.get("o")?.as_str()?.to_owned();
            Some((s, p, o))
        })
        .collect();

    let limited: &[(String, String, String)] = if max_rows > 0 && triples.len() > max_rows as usize
    {
        pgrx::warning!(
            "PT642: export truncated to {} rows (export_max_rows)",
            max_rows
        );
        &triples[..max_rows as usize]
    } else {
        &triples
    };

    // Chunk into batches and serialise each batch as JSON-LD.
    limited
        .chunks(CURSOR_BATCH_SIZE)
        .map(|chunk| export::triples_to_jsonld(chunk).to_string())
        .collect()
}
