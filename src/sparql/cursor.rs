//! Streaming SPARQL cursor API (v0.40.0, paged streaming in v0.66.0 — STREAM-01).
//!
//! Provides set-returning functions that page through large SPARQL result sets
//! one page at a time, using PostgreSQL SPI portals for truly bounded memory.
//!
//! # Functions
//!
//! - `sparql_cursor(query TEXT) RETURNS SETOF JSONB` — streams SELECT/ASK results
//! - `sparql_cursor_turtle(query TEXT) RETURNS SETOF TEXT` — streams Turtle lines
//! - `sparql_cursor_jsonld(query TEXT) RETURNS SETOF TEXT` — streams JSON-LD chunks
//!
//! # Memory model (v0.66.0 — STREAM-01)
//!
//! `sparql_cursor` uses a lazy `CursorIter` that:
//! 1. Opens a PostgreSQL SPI portal for the generated SPARQL SQL.
//! 2. Detaches the portal so it survives across SPI sessions within the transaction.
//! 3. In `Iterator::next()`, opens a fresh SPI session per page, fetches
//!    `pg_ripple.export_batch_size` rows (default 10,000), decodes dictionary IDs,
//!    and frees the SpiTupleTable before ending the session.
//!
//! Peak Rust-side memory ≈ export_batch_size × avg_row_width + decode overhead.
//! The SpiTupleTable for each page is freed at SPI session end (pgrx guarantee).

use std::collections::{HashMap, HashSet};

use pgrx::prelude::*;
use serde_json::{Map, Value as Json};

use crate::export;
use crate::sparql;

// ─── Lazy cursor iterator ─────────────────────────────────────────────────────

/// Lazy iterator over SPARQL SELECT results that fetches one page per SPI session.
///
/// Memory use is bounded to `pg_ripple.export_batch_size` rows at any point.
/// The underlying PostgreSQL portal is detached after each page fetch so it
/// survives across SPI sessions within the same transaction.
pub struct CursorIter {
    /// Name of the detached PostgreSQL portal (transaction-scoped).
    portal_name: String,
    /// Result variable names from the SPARQL translation.
    variables: Vec<String>,
    /// Variables that hold raw numeric aggregates — skip dictionary decode.
    raw_numeric_vars: HashSet<String>,
    raw_text_vars: HashSet<String>,
    raw_iri_vars: HashSet<String>,
    raw_double_vars: HashSet<String>,
    /// Rows to fetch per SPI session.
    page_size: i64,
    /// Current page of decoded rows.
    page: Vec<pgrx::JsonB>,
    /// Read position within the current page.
    page_pos: usize,
    /// True when the portal is exhausted or closed.
    done: bool,
    /// Maximum rows to return (0 = unlimited).
    max_rows: i64,
    /// Rows yielded so far.
    rows_returned: i64,
    /// Overflow action ("warn" | "error").
    overflow_action: String,
}

impl CursorIter {
    /// Open a PostgreSQL portal for `query` and return a lazy page iterator.
    ///
    /// The query must be a SPARQL SELECT or ASK query.  The portal is opened
    /// in the current transaction and detached so it outlives the SPI session.
    pub fn new(query: &str) -> Self {
        let (sql, variables, raw_numeric, raw_text, raw_iri, raw_double, wcoj_preamble) =
            super::prepare_select(query);

        let page_size = crate::gucs::storage::EXPORT_BATCH_SIZE.get().max(1) as i64;
        let max_rows = crate::SPARQL_MAX_ROWS.get() as i64;
        let overflow_action = crate::SPARQL_OVERFLOW_ACTION
            .get()
            .as_ref()
            .and_then(|s| s.to_str().ok().map(str::to_owned))
            .unwrap_or_else(|| "warn".to_owned());

        // Open the portal and immediately detach it so it lives across SPI sessions.
        let portal_name = Spi::connect_mut(|client| {
            if crate::BGP_REORDER.get() {
                let _ = client.update("SET LOCAL join_collapse_limit = 1", None, &[]);
                let _ = client.update("SET LOCAL enable_mergejoin = on", None, &[]);
            }
            if wcoj_preamble {
                let _ = client.update(crate::sparql::wcoj::wcoj_session_preamble(), None, &[]);
            }
            // open_cursor with no args — the SQL already has all values inlined
            // as integer literals by the SPARQL-to-SQL translator.
            let cursor = client.open_cursor(sql.as_str(), &[]);
            Ok::<_, pgrx::spi::Error>(cursor.detach_into_name())
        })
        .unwrap_or_else(|e| pgrx::error!("CursorIter: failed to open portal: {e}"));

        crate::stats::increment_cursor_pages_opened();

        Self {
            portal_name,
            variables,
            raw_numeric_vars: raw_numeric,
            raw_text_vars: raw_text,
            raw_iri_vars: raw_iri,
            raw_double_vars: raw_double,
            page_size,
            page: Vec::new(),
            page_pos: 0,
            done: false,
            max_rows,
            rows_returned: 0,
            overflow_action,
        }
    }

    /// Fetch the next page from the portal.
    ///
    /// Opens a new SPI session, fetches up to `page_size` rows, decodes
    /// dictionary IDs, and closes the SPI session (freeing SpiTupleTable).
    /// When the page is non-empty, the portal is detached to survive the
    /// next SPI session.  When empty, the portal drops (closes) naturally.
    fn fetch_page(&mut self) {
        let name = self.portal_name.clone();
        let page_size = self.page_size;
        let variables = self.variables.clone();
        let raw_numeric_vars = self.raw_numeric_vars.clone();
        let raw_text_vars = self.raw_text_vars.clone();
        let raw_iri_vars = self.raw_iri_vars.clone();
        let raw_double_vars = self.raw_double_vars.clone();

        // Each fetch opens its own SPI session; the SpiTupleTable is freed
        // when the session ends — this is the key to memory-bounded operation.
        let (rows, exhausted) = Spi::connect_mut(|client| {
            let mut cursor = client
                .find_cursor(&name)
                .unwrap_or_else(|e| pgrx::error!("CursorIter: find_cursor failed: {e}"));

            let table = cursor
                .fetch(page_size)
                .unwrap_or_else(|e| pgrx::error!("CursorIter: fetch failed: {e}"));

            // Collect raw values and all dictionary IDs in one pass.
            let mut raw_rows: Vec<Vec<Option<Result<i64, String>>>> = Vec::new();
            let mut all_ids: Vec<i64> = Vec::new();

            for row in table {
                let mut row_vals = Vec::with_capacity(variables.len());
                for (col_idx, var) in variables.iter().enumerate() {
                    let i = col_idx + 1;
                    if raw_text_vars.contains(var)
                        || raw_iri_vars.contains(var)
                        || raw_double_vars.contains(var)
                    {
                        let text_val = row.get::<String>(i).ok().flatten().map(Err);
                        row_vals.push(text_val);
                    } else {
                        let val = row.get::<i64>(i).ok().flatten();
                        if let Some(id) = val {
                            all_ids.push(id);
                        }
                        row_vals.push(val.map(Ok));
                    }
                }
                raw_rows.push(row_vals);
            }

            let is_empty = raw_rows.is_empty();

            if is_empty {
                // Portal naturally closes when cursor drops here (no detach).
                return Ok::<_, pgrx::spi::Error>((Vec::<pgrx::JsonB>::new(), true));
            }

            // Detach to keep the portal alive for the next page fetch.
            cursor.detach_into_name();

            // Batch-decode all dictionary IDs for this page.
            all_ids.sort_unstable();
            all_ids.dedup();
            let decode_map: HashMap<i64, String> = super::batch_decode(&all_ids);

            // Build JSONB rows.
            let page: Vec<pgrx::JsonB> = raw_rows
                .into_iter()
                .map(|row_vals| {
                    let mut obj = Map::new();
                    for (i, var) in variables.iter().enumerate() {
                        let raw_val = row_vals.get(i).and_then(|v| v.as_ref());
                        let v = match raw_val {
                            None => Json::Null,
                            Some(Err(text)) => {
                                if raw_iri_vars.contains(var) {
                                    Json::String(format!("<{text}>"))
                                } else if raw_double_vars.contains(var) {
                                    Json::String(format!(
                                        "\"{text}\"^^<http://www.w3.org/2001/XMLSchema#double>"
                                    ))
                                } else {
                                    Json::String(format!("\"{}\"", text.replace('"', "\\\"")))
                                }
                            }
                            Some(Ok(id)) => {
                                if raw_numeric_vars.contains(var) {
                                    Json::Number(serde_json::Number::from(*id))
                                } else {
                                    decode_map
                                        .get(id)
                                        .map(|s| Json::String(s.clone()))
                                        .unwrap_or(Json::Null)
                                }
                            }
                        };
                        obj.insert(var.clone(), v);
                    }
                    pgrx::JsonB(Json::Object(obj))
                })
                .collect();

            Ok::<_, pgrx::spi::Error>((page, false))
        })
        .unwrap_or_else(|e| pgrx::error!("CursorIter: page decode failed: {e}"));

        crate::stats::increment_cursor_pages_fetched();

        if exhausted {
            self.done = true;
        } else {
            self.page = rows;
            self.page_pos = 0;
        }
    }
}

impl Iterator for CursorIter {
    type Item = pgrx::JsonB;

    fn next(&mut self) -> Option<pgrx::JsonB> {
        if self.done {
            return None;
        }

        // Enforce max_rows limit.
        if self.max_rows > 0 && self.rows_returned >= self.max_rows {
            if self.overflow_action == "error" {
                pgrx::error!(
                    "PT640: SPARQL result set exceeded sparql_max_rows limit of {}",
                    self.max_rows
                );
            } else {
                if self.rows_returned == self.max_rows {
                    pgrx::warning!(
                        "PT640: SPARQL result set truncated to {} rows (sparql_max_rows)",
                        self.max_rows
                    );
                }
                return None;
            }
        }

        // Fetch next page when current page is exhausted.
        if self.page_pos >= self.page.len() {
            self.fetch_page();
            if self.done {
                return None;
            }
        }

        let item = pgrx::JsonB(self.page[self.page_pos].0.clone());
        self.page_pos += 1;
        self.rows_returned += 1;
        Some(item)
    }
}

// ─── Public cursor API ────────────────────────────────────────────────────────

/// Execute a SPARQL SELECT query and return a lazy page-by-page iterator.
///
/// Memory use is bounded to `pg_ripple.export_batch_size` rows (default 10,000)
/// at any point.  The portal is detached after each page so the underlying
/// PostgreSQL cursor persists across SPI sessions within the same transaction.
///
/// Respects `pg_ripple.sparql_max_rows` if set.
pub fn sparql_cursor(query: &str) -> impl Iterator<Item = (pgrx::JsonB,)> + 'static {
    CursorIter::new(query).map(|r| (r,))
}

/// Execute a SPARQL CONSTRUCT query and stream the result as Turtle text lines.
///
/// Each yielded `TEXT` value is a complete Turtle serialisation chunk.
/// Large result sets are chunked in batches of `pg_ripple.export_batch_size`.
pub fn sparql_cursor_turtle(query: &str) -> Vec<String> {
    let rows = sparql::sparql_construct(query);
    let max_rows = crate::EXPORT_MAX_ROWS.get();
    let batch_size = crate::gucs::storage::EXPORT_BATCH_SIZE.get().max(1) as usize;

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

    limited
        .chunks(batch_size)
        .map(export::triples_to_turtle)
        .collect()
}

/// Execute a SPARQL CONSTRUCT query and stream the result as JSON-LD chunks.
///
/// Each yielded `TEXT` value is a JSON-LD expanded-form array for one batch.
pub fn sparql_cursor_jsonld(query: &str) -> Vec<String> {
    let rows = sparql::sparql_construct(query);
    let max_rows = crate::EXPORT_MAX_ROWS.get();
    let batch_size = crate::gucs::storage::EXPORT_BATCH_SIZE.get().max(1) as usize;

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

    limited
        .chunks(batch_size)
        .map(|chunk| export::triples_to_jsonld(chunk).to_string())
        .collect()
}
