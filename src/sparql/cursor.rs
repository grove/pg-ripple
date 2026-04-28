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

/// Execute a SPARQL CONSTRUCT query and stream the result as Turtle text chunks.
///
/// Uses a PostgreSQL portal cursor so that triples are fetched in pages of
/// `pg_ripple.export_batch_size` rows — the full result set is never buffered
/// in Rust memory.  Each yielded `TEXT` value is a complete Turtle chunk for
/// one page of triples.
///
/// v0.68.0 (STREAM-01): replaced the materializing implementation.
pub fn sparql_cursor_turtle(query: &str) -> impl Iterator<Item = (String,)> + 'static {
    ConstructCursorIter::new(query, ConstructFormat::Turtle).map(|chunk| (chunk,))
}

/// Execute a SPARQL CONSTRUCT query and stream the result as JSON-LD chunks.
///
/// Uses a PostgreSQL portal cursor so that triples are fetched in pages of
/// `pg_ripple.export_batch_size` rows — the full result set is never buffered
/// in Rust memory.  Each yielded `TEXT` value is a JSON-LD expanded array for
/// one page of triples.
///
/// v0.68.0 (STREAM-01): replaced the materializing implementation.
pub fn sparql_cursor_jsonld(query: &str) -> impl Iterator<Item = (String,)> + 'static {
    ConstructCursorIter::new(query, ConstructFormat::JsonLd).map(|chunk| (chunk,))
}

// ─── CONSTRUCT portal streaming iterator (STREAM-01) ─────────────────────────

/// Output format for `ConstructCursorIter`.
#[derive(Clone, Copy)]
pub(crate) enum ConstructFormat {
    Turtle,
    JsonLd,
}

/// Lazy iterator over SPARQL CONSTRUCT results that pages through an SPI portal
/// and serializes each page to Turtle or JSON-LD without materializing the full
/// result set.
///
/// Memory use is bounded to `pg_ripple.export_batch_size` rows at any point.
/// Each page of WHERE-clause rows is fetched, the CONSTRUCT template is applied
/// in Rust to produce (s_id, p_id, o_id) tuples, and those IDs are batch-decoded
/// before serialization — no full document ever accumulates in memory.
pub struct ConstructCursorIter {
    /// Name of the detached PostgreSQL portal (transaction-scoped).
    portal_name: String,
    /// Variable names corresponding to SQL result columns.
    variables: Vec<String>,
    /// Pre-compiled CONSTRUCT template (constants encoded, variables indexed).
    template: super::ConstructTemplate,
    /// Rows to fetch per SPI session.
    page_size: i64,
    /// Maximum result triples to emit (0 = unlimited).
    max_rows: i64,
    /// Triples emitted so far.
    rows_emitted: i64,
    /// True when the portal is exhausted.
    done: bool,
    /// Output format.
    format: ConstructFormat,
}

impl ConstructCursorIter {
    /// Open a portal for the CONSTRUCT `query` and return a lazy iterator.
    pub fn new(query: &str, format: ConstructFormat) -> Self {
        let (sql, variables, template) = super::prepare_construct(query);

        let page_size = crate::gucs::storage::EXPORT_BATCH_SIZE.get().max(1) as i64;
        let max_rows = crate::EXPORT_MAX_ROWS.get() as i64;

        let portal_name = Spi::connect_mut(|client| {
            if crate::BGP_REORDER.get() {
                let _ = client.update("SET LOCAL join_collapse_limit = 1", None, &[]);
                let _ = client.update("SET LOCAL enable_mergejoin = on", None, &[]);
            }
            let cursor = client.open_cursor(sql.as_str(), &[]);
            Ok::<_, pgrx::spi::Error>(cursor.detach_into_name())
        })
        .unwrap_or_else(|e| pgrx::error!("ConstructCursorIter: failed to open portal: {e}"));

        crate::stats::increment_cursor_pages_opened();

        Self {
            portal_name,
            variables,
            template,
            page_size,
            max_rows,
            rows_emitted: 0,
            done: false,
            format,
        }
    }
}

impl Iterator for ConstructCursorIter {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        if self.done {
            return None;
        }

        // Check row limit before fetching.
        if self.max_rows > 0 && self.rows_emitted >= self.max_rows {
            pgrx::warning!(
                "PT642: CONSTRUCT export truncated to {} rows (export_max_rows)",
                self.max_rows
            );
            self.done = true;
            return None;
        }

        let page_size = if self.max_rows > 0 {
            self.page_size.min(self.max_rows - self.rows_emitted).max(1)
        } else {
            self.page_size
        };

        let name = self.portal_name.clone();
        let num_vars = self.variables.len();

        // Fetch one page of WHERE-clause bindings from the portal.
        let (raw_rows, exhausted) = Spi::connect_mut(|client| {
            let mut cursor = client
                .find_cursor(&name)
                .unwrap_or_else(|e| pgrx::error!("ConstructCursorIter: find_cursor failed: {e}"));

            let table = cursor
                .fetch(page_size)
                .unwrap_or_else(|e| pgrx::error!("ConstructCursorIter: fetch failed: {e}"));

            // Collect raw variable bindings (one i64 per variable per row).
            let mut rows: Vec<Vec<Option<i64>>> = Vec::new();
            for row in table {
                let mut vals = Vec::with_capacity(num_vars);
                for col_idx in 1..=num_vars {
                    vals.push(row.get::<i64>(col_idx).ok().flatten());
                }
                rows.push(vals);
            }
            let is_empty = rows.is_empty();
            if !is_empty {
                cursor.detach_into_name();
            }
            Ok::<_, pgrx::spi::Error>((rows, is_empty))
        })
        .unwrap_or_else(|e| pgrx::error!("ConstructCursorIter: page fetch failed: {e}"));

        crate::stats::increment_cursor_pages_fetched();

        if exhausted {
            self.done = true;
            return None;
        }

        // Apply the CONSTRUCT template to each row → (s_id, p_id, o_id) tuples.
        let mut id_triples: Vec<(i64, i64, i64)> = Vec::new();
        for row_vals in &raw_rows {
            let mut applied = super::apply_construct_template(&self.template, row_vals);
            id_triples.append(&mut applied);
        }

        if id_triples.is_empty() {
            // Template produced nothing for this page (all variables unbound).
            // Mark done so we don't loop forever on sparse results.
            self.done = true;
            return None;
        }

        // Batch-decode all dictionary IDs for this page.
        let mut all_ids: Vec<i64> = id_triples
            .iter()
            .flat_map(|(s, p, o)| [*s, *p, *o])
            .collect();
        all_ids.sort_unstable();
        all_ids.dedup();
        let decode_map = super::batch_decode(&all_ids);

        // Decode each (s_id, p_id, o_id) tuple to (String, String, String).
        let decoded: Vec<(String, String, String)> = id_triples
            .iter()
            .filter_map(|(s_id, p_id, o_id)| {
                let s = decode_map.get(s_id)?.clone();
                let p = decode_map.get(p_id)?.clone();
                let o = decode_map.get(o_id)?.clone();
                Some((s, p, o))
            })
            .collect();

        self.rows_emitted += decoded.len() as i64;

        // Serialize the page.
        let chunk = match self.format {
            ConstructFormat::Turtle => export::triples_to_turtle(&decoded),
            ConstructFormat::JsonLd => export::triples_to_jsonld(&decoded).to_string(),
        };

        Some(chunk)
    }
}
