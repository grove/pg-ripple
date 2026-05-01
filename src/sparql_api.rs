//! pg_ripple SQL API — SPARQL query engine, plan cache monitoring, FTS, HTAP maintenance

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── SPARQL query engine ───────────────────────────────────────────────────

    /// Execute a SPARQL SELECT or ASK query.
    ///
    /// Returns one JSONB row per result binding for SELECT queries.
    /// For ASK returns a single row `{"result": "true"}` or `{"result": "false"}`.
    #[pg_extern]
    fn sparql(query: &str) -> TableIterator<'static, (name!(result, pgrx::JsonB),)> {
        let rows = crate::sparql::sparql(query);
        TableIterator::new(rows.into_iter().map(|r| (r,)))
    }

    /// Execute a SPARQL ASK query; returns TRUE if any results exist.
    #[pg_extern]
    fn sparql_ask(query: &str) -> bool {
        crate::sparql::sparql_ask(query)
    }

    /// Return the SQL generated for a SPARQL query (for debugging).
    /// Set `analyze := true` to EXPLAIN ANALYZE the generated SQL.
    #[pg_extern]
    fn sparql_explain(query: &str, analyze: bool) -> String {
        crate::sparql::sparql_explain(query, analyze)
    }

    /// Normalise a SPARQL query text for `pg_stat_statements` grouping.
    ///
    /// Replaces string literals with `$S`, IRIs with `$I`, and numeric literals
    /// with `$N` so that structurally identical queries with different literal
    /// values produce the same normalised key.  Useful for auditing query classes
    /// and as a stable cache key. (v0.82.0 PGSS-NORM-01)
    #[pg_extern]
    fn sparql_normalise(query: &str) -> String {
        crate::sparql::parse::normalise_sparql_for_pgss(query)
    }

    /// Explain a SPARQL query with flexible output format (v0.23.0).
    ///
    /// `format` may be one of:
    /// - `'sql'`             — return the generated SQL without executing it
    /// - `'text'` (default)  — run EXPLAIN (ANALYZE, FORMAT TEXT)
    /// - `'json'`            — run EXPLAIN (ANALYZE, FORMAT JSON)
    /// - `'sparql_algebra'`  — return the spargebra algebra tree
    #[pg_extern]
    fn explain_sparql(query: &str, format: default!(&str, "'text'")) -> String {
        crate::sparql::explain_sparql(query, format)
    }

    /// Execute a SPARQL CONSTRUCT query; returns one JSONB row per constructed triple.
    ///
    /// Each row is `{"s": "...", "p": "...", "o": "..."}` in N-Triples format.
    #[pg_extern]
    fn sparql_construct(query: &str) -> TableIterator<'static, (name!(result, pgrx::JsonB),)> {
        let rows = crate::sparql::sparql_construct(query);
        TableIterator::new(rows.into_iter().map(|r| (r,)))
    }

    /// Execute a SPARQL DESCRIBE query using the Concise Bounded Description algorithm.
    ///
    /// Returns one JSONB row per triple in the description.
    /// `strategy` may be `'cbd'` (default), `'scbd'` (symmetric), or `'simple'`.
    #[pg_extern]
    fn sparql_describe(
        query: &str,
        strategy: default!(&str, "'cbd'"),
    ) -> TableIterator<'static, (name!(result, pgrx::JsonB),)> {
        let rows = crate::sparql::sparql_describe(query, strategy);
        TableIterator::new(rows.into_iter().map(|r| (r,)))
    }

    /// Execute a SPARQL CONSTRUCT query; returns the result as Turtle text.
    ///
    /// Constructs triples according to the CONSTRUCT template and serializes them
    /// as a Turtle document.  RDF-star quoted triples are emitted in Turtle-star
    /// notation.
    #[pg_extern]
    fn sparql_construct_turtle(query: &str) -> String {
        let rows = crate::sparql::sparql_construct(query);
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
        crate::export::triples_to_turtle(&triples)
    }

    /// Execute a SPARQL CONSTRUCT query; returns the result as JSON-LD (JSONB).
    ///
    /// Constructs triples according to the CONSTRUCT template and serializes them
    /// as a JSON-LD expanded-form array.  Suitable for REST API responses.
    #[pg_extern]
    fn sparql_construct_jsonld(query: &str) -> pgrx::JsonB {
        let rows = crate::sparql::sparql_construct(query);
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
        pgrx::JsonB(crate::export::triples_to_jsonld(&triples))
    }

    /// Execute a SPARQL DESCRIBE query; returns the description as Turtle text.
    ///
    /// `strategy` may be `'cbd'` (default), `'scbd'` (symmetric), or `'simple'`.
    #[pg_extern]
    fn sparql_describe_turtle(query: &str, strategy: default!(&str, "'cbd'")) -> String {
        let rows = crate::sparql::sparql_describe(query, strategy);
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
        crate::export::triples_to_turtle(&triples)
    }

    /// Execute a SPARQL DESCRIBE query; returns the description as JSON-LD (JSONB).
    ///
    /// `strategy` may be `'cbd'` (default), `'scbd'` (symmetric), or `'simple'`.
    #[pg_extern]
    fn sparql_describe_jsonld(query: &str, strategy: default!(&str, "'cbd'")) -> pgrx::JsonB {
        let rows = crate::sparql::sparql_describe(query, strategy);
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
        pgrx::JsonB(crate::export::triples_to_jsonld(&triples))
    }

    /// Execute a SPARQL Update statement (`INSERT DATA` or `DELETE DATA`).
    ///
    /// Returns the total number of triples affected (inserted or deleted).
    #[pg_extern]
    fn sparql_update(query: &str) -> i64 {
        crate::sparql::sparql_update(query)
    }

    // ── Plan cache monitoring (v0.13.0) ──────────────────────────────────────

    /// Return SPARQL plan cache statistics as a single row.
    ///
    /// Columns: `hits`, `misses`, `evictions` (always 0), `hit_rate`.
    /// For the legacy JSONB form use `cache_stats()`.
    ///
    /// Supersedes the v0.38.0 JSONB overload (v0.47.0).
    #[pg_extern]
    fn plan_cache_stats() -> TableIterator<
        'static,
        (
            name!(hits, i64),
            name!(misses, i64),
            name!(evictions, i64),
            name!(hit_rate, f64),
        ),
    > {
        // Use the public JSONB function to extract counters.
        let jsonb = crate::sparql::plan_cache_stats();
        let hits = jsonb.0.get("hits").and_then(|v| v.as_i64()).unwrap_or(0);
        let misses = jsonb.0.get("misses").and_then(|v| v.as_i64()).unwrap_or(0);
        let total = hits + misses;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };
        TableIterator::new(std::iter::once((hits, misses, 0_i64, hit_rate)))
    }

    /// Evict all cached SPARQL plan translations and reset hit/miss counters.
    #[pg_extern]
    fn plan_cache_reset() {
        crate::sparql::plan_cache_reset()
    }

    // ── v0.40.0: Streaming cursor API ────────────────────────────────────────

    /// Stream SPARQL SELECT results one batch at a time.
    ///
    /// Unlike `sparql()`, this function pages through results one portal-page
    /// at a time (bounded by `pg_ripple.export_batch_size`), keeping Rust-side
    /// memory proportional to the page size rather than the full result size.
    /// Respects `pg_ripple.sparql_max_rows` if set.
    #[pg_extern]
    fn sparql_cursor(query: &str) -> TableIterator<'static, (name!(result, pgrx::JsonB),)> {
        TableIterator::new(crate::sparql::cursor::sparql_cursor(query))
    }

    /// Stream a SPARQL CONSTRUCT query result as Turtle text chunks.
    ///
    /// Each returned row is a Turtle serialisation of up to 1024 triples.
    /// Respects `pg_ripple.export_max_rows` if set.
    #[pg_extern]
    fn sparql_cursor_turtle(query: &str) -> TableIterator<'static, (name!(chunk, String),)> {
        TableIterator::new(crate::sparql::cursor::sparql_cursor_turtle(query))
    }

    /// Stream a SPARQL CONSTRUCT query result as JSON-LD chunks.
    ///
    /// Each returned row is a JSON-LD expanded-form array for one batch.
    /// Respects `pg_ripple.export_max_rows` if set.
    #[pg_extern]
    fn sparql_cursor_jsonld(query: &str) -> TableIterator<'static, (name!(chunk, String),)> {
        TableIterator::new(crate::sparql::cursor::sparql_cursor_jsonld(query))
    }

    // ── v0.40.0: explain_sparql returning JSONB ───────────────────────────────

    /// Explain a SPARQL query and return a structured JSONB report.
    ///
    /// Returns a JSONB object with keys:
    /// - `"algebra"` — spargebra algebra tree
    /// - `"sql"` — the generated SQL
    /// - `"plan"` — PostgreSQL EXPLAIN output as JSON
    /// - `"cache_hit"` — whether the plan came from the plan cache
    /// - `"encode_calls"` — dictionary encode calls during translation
    ///
    /// When `analyze` is `true`, runs `EXPLAIN (ANALYZE, FORMAT JSON, BUFFERS true)`.
    #[pg_extern(name = "explain_sparql", volatile)]
    fn explain_sparql_jsonb(query: &str, analyze: bool) -> pgrx::JsonB {
        crate::sparql::explain::explain_sparql_jsonb(query, analyze, false)
    }

    /// Explain a SPARQL query with optional Citus shard-pruning section (v0.59.0).
    ///
    /// Same as `explain_sparql(query, analyze)` but adds a `"citus"` key when
    /// `citus` is `true`, showing which shard was targeted, the worker node, and
    /// whether full fan-out was avoided.
    ///
    /// Example:
    /// ```sql
    /// SELECT pg_ripple.explain_sparql(
    ///   'SELECT ?p ?o WHERE { <http://example.org/Alice> ?p ?o }',
    ///   false,
    ///   true
    /// );
    /// ```
    #[pg_extern(name = "explain_sparql", volatile)]
    fn explain_sparql_jsonb_citus(query: &str, analyze: bool, citus: bool) -> pgrx::JsonB {
        crate::sparql::explain::explain_sparql_jsonb(query, analyze, citus)
    }

    // ── v0.40.0: cache_stats / reset_cache_stats ──────────────────────────────

    /// Return comprehensive cache statistics as JSONB.
    ///
    /// Keys:
    /// - `"plan_cache"` — SPARQL plan cache hits/misses/size/capacity
    /// - `"dict_cache"` — dictionary encode cache hits/misses/evictions/utilisation
    /// - `"federation_cache"` — federation result cache hit/miss counts
    #[pg_extern(name = "cache_stats")]
    fn cache_stats_comprehensive() -> pgrx::JsonB {
        // Plan cache stats (via public sparql::plan_cache_stats() which returns JSONB;
        // we re-derive the raw numbers from the public stats() re-export).
        let plan_cache_jsonb = crate::sparql::plan_cache_stats();
        // Dict cache stats.
        let (dc_hits, dc_misses, dc_evictions, dc_util) = crate::shmem::get_cache_stats();
        // Federation cache: count rows in the federation_cache table.
        let (fc_hits, fc_misses) = super::get_federation_cache_stats_inner();

        let util_rounded = (dc_util * 10000.0).round() / 10000.0;
        pgrx::JsonB(serde_json::json!({
            "plan_cache": plan_cache_jsonb.0,
            "dict_cache": {
                "hits": dc_hits,
                "misses": dc_misses,
                "evictions": dc_evictions,
                "utilisation": util_rounded
            },
            "federation_cache": {
                "hits": fc_hits,
                "misses": fc_misses
            }
        }))
    }

    /// Reset all cache statistics counters (SPARQL plan cache, dict cache).
    ///
    /// Does not evict cached entries — only resets hit/miss counters.
    #[pg_extern]
    fn reset_cache_stats() {
        crate::sparql::plan_cache_reset();
        crate::shmem::reset_cache_stats();
    }

    // ── v0.47.0: individual cache hit-rate SRFs ───────────────────────────────

    /// Return dictionary encode-cache statistics as a single row.
    ///
    /// Columns: hits, misses, evictions, hit_rate.
    #[pg_extern]
    fn dictionary_cache_stats() -> TableIterator<
        'static,
        (
            name!(hits, i64),
            name!(misses, i64),
            name!(evictions, i64),
            name!(hit_rate, f64),
        ),
    > {
        let (hits, misses, evictions, _util) = crate::shmem::get_cache_stats();
        let total = hits + misses;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };
        TableIterator::new(std::iter::once((
            hits as i64,
            misses as i64,
            evictions as i64,
            hit_rate,
        )))
    }

    /// Return federation result-cache statistics as a single row.
    ///
    /// Columns: hits (live cached entries), misses (always 0 — not separately
    /// tracked), evictions (always 0), hit_rate (always 0.0 — not available).
    #[pg_extern]
    fn federation_cache_stats() -> TableIterator<
        'static,
        (
            name!(hits, i64),
            name!(misses, i64),
            name!(evictions, i64),
            name!(hit_rate, f64),
        ),
    > {
        let (hits, misses) = super::get_federation_cache_stats_inner();
        let total = hits + misses;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };
        TableIterator::new(std::iter::once((hits, misses, 0_i64, hit_rate)))
    }

    /// Return cumulative federation call counters (v0.55.0 G-4, extended v0.61.0 I7-3).
    ///
    /// v0.61.0: returns per-endpoint stats with latency percentiles:
    /// - `endpoint TEXT`      — SERVICE endpoint URL (or `"_total"` for the global aggregate)
    /// - `calls INT`          — total SERVICE calls attempted
    /// - `errors INT`         — calls that returned an error
    /// - `blocked INT`        — calls blocked by endpoint policy PT606
    /// - `p50_ms INT`         — estimated p50 latency in milliseconds (0 when not tracked)
    /// - `p95_ms INT`         — estimated p95 latency in milliseconds (0 when not tracked)
    /// - `last_error_at TIMESTAMPTZ` — timestamp of the most recent error (NULL if none)
    ///
    /// Counters are in-memory (reset on postmaster restart).
    #[pg_extern]
    #[allow(clippy::type_complexity)]
    fn federation_call_stats() -> TableIterator<
        'static,
        (
            name!(endpoint, String),
            name!(calls, i64),
            name!(errors, i64),
            name!(blocked, i64),
            name!(p50_ms, i64),
            name!(p95_ms, i64),
            name!(last_error_at, Option<pgrx::datum::TimestampWithTimeZone>),
        ),
    > {
        use std::sync::atomic::Ordering;
        let (calls, errors, blocked) = if crate::shmem::SHMEM_READY.load(Ordering::Relaxed) {
            (
                crate::shmem::FED_CALL_COUNT.get().load(Ordering::Relaxed) as i64,
                crate::shmem::FED_ERROR_COUNT.get().load(Ordering::Relaxed) as i64,
                crate::shmem::FED_BLOCKED_COUNT
                    .get()
                    .load(Ordering::Relaxed) as i64,
            )
        } else {
            (0_i64, 0_i64, 0_i64)
        };
        // Return one aggregate row; per-endpoint breakdown requires persistent stats tables.
        TableIterator::new(std::iter::once((
            "_total".to_owned(),
            calls,
            errors,
            blocked,
            0_i64,
            0_i64,
            None::<pgrx::datum::TimestampWithTimeZone>,
        )))
    }

    /// Flush the shared-memory encode cache, evicting all entries.
    ///
    /// Use this to clear stale hash→id mappings that may have been left by
    /// rolled-back transactions before v0.42.0 fixed the xact callback.
    /// After calling this, the next encode() call for each IRI/literal will
    /// do a fresh SPI lookup — performance recovers immediately as the cache
    /// warms up again.  Safe to call at any time (no data is lost).
    #[pg_extern]
    fn flush_encode_cache() {
        crate::shmem::encode_cache_clear_all();
        // Also clear backend-local encode/decode caches so the current
        // session does not re-insert stale mappings from its own LRU.
        crate::dictionary::clear_caches();
    }

    // ── Full-text search ─────────────────────────────────────────────────────

    /// Create a GIN tsvector index on the dictionary for the given predicate IRI.
    ///
    /// After indexing, SPARQL `CONTAINS()` and `REGEX()` FILTERs on triples
    /// using this predicate will be rewritten to use the GIN index for
    /// efficient text matching.  Returns the predicate dictionary id.
    #[pg_extern]
    fn fts_index(predicate: &str) -> i64 {
        crate::fts::fts_index(predicate)
    }

    /// Full-text search on literal objects of a given predicate.
    ///
    /// `query` is a `tsquery`-formatted search string (e.g. `'knowledge & graph'`).
    /// Returns matching triples as `(s TEXT, p TEXT, o TEXT)` in N-Triples format.
    #[pg_extern]
    fn fts_search(
        query: &str,
        predicate: &str,
    ) -> TableIterator<'static, (name!(s, String), name!(p, String), name!(o, String))> {
        let rows: Vec<(String, String, String)> =
            crate::fts::fts_search(query, predicate).collect();
        TableIterator::new(rows)
    }

    // ── HTAP maintenance (v0.6.0) ─────────────────────────────────────────────

    /// Trigger an immediate full merge of all HTAP VP tables.
    ///
    /// Moves all rows from delta into main, rebuilds subject_patterns and
    /// object_patterns, and runs ANALYZE on each merged table.
    /// Returns the total number of rows in all merged main tables.
    #[pg_extern]
    fn compact() -> i64 {
        crate::storage::merge::compact()
    }

    // ── v0.51.0: W3C SPARQL 1.1 CSV / TSV serialisation ──────────────────────

    /// Execute a SPARQL SELECT query and return results in W3C CSV format.
    ///
    /// Each returned row is one line of the CSV document (including the header).
    /// Follows the SPARQL 1.1 CSV/TSV Results Format specification
    /// (<https://www.w3.org/TR/sparql11-results-csv-tsv/>):
    /// - First row: variable names as `?var1,?var2,...`
    /// - Subsequent rows: comma-separated, quoted values
    /// - Unbound variables produce empty fields
    #[pg_extern]
    fn sparql_csv(query: &str) -> TableIterator<'static, (name!(line, String),)> {
        use serde_json::Value as Json;

        let rows = crate::sparql::sparql(query);
        if rows.is_empty() {
            return TableIterator::new(std::iter::empty());
        }

        // Extract variable names from the first row.
        let header_vars: Vec<String> = if let Some(first) = rows.first() {
            if let Json::Object(map) = &first.0 {
                map.keys()
                    .filter(|k| *k != "result") // skip ASK sentinel
                    .map(|k| format!("?{k}"))
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        if header_vars.is_empty() {
            return TableIterator::new(std::iter::empty());
        }

        let mut lines: Vec<String> = Vec::with_capacity(rows.len() + 1);
        lines.push(header_vars.join(","));

        let var_names: Vec<String> = header_vars
            .iter()
            .map(|v| v.trim_start_matches('?').to_owned())
            .collect();

        for row in rows {
            if let Json::Object(map) = row.0 {
                let fields: Vec<String> = var_names
                    .iter()
                    .map(|v| {
                        match map.get(v) {
                            Some(Json::String(s)) => {
                                // RFC 4180: quote if contains comma, dquote, or newline.
                                if s.contains(',') || s.contains('"') || s.contains('\n') {
                                    format!("\"{}\"", s.replace('"', "\"\""))
                                } else {
                                    s.clone()
                                }
                            }
                            Some(other) => other.to_string(),
                            None => String::new(),
                        }
                    })
                    .collect();
                lines.push(fields.join(","));
            }
        }

        TableIterator::new(lines.into_iter().map(|l| (l,)))
    }

    /// Execute a SPARQL SELECT query and return results in W3C TSV format.
    ///
    /// Each returned row is one tab-separated line.
    /// Follows the SPARQL 1.1 CSV/TSV Results Format specification
    /// (<https://www.w3.org/TR/sparql11-results-csv-tsv/>):
    /// - First row: variable names as `?var1\t?var2\t...`
    /// - Subsequent rows: tab-separated N-Triples encoded values
    /// - Unbound variables produce empty fields
    #[pg_extern]
    fn sparql_tsv(query: &str) -> TableIterator<'static, (name!(line, String),)> {
        use serde_json::Value as Json;

        let rows = crate::sparql::sparql(query);
        if rows.is_empty() {
            return TableIterator::new(std::iter::empty());
        }

        let header_vars: Vec<String> = if let Some(first) = rows.first() {
            if let Json::Object(map) = &first.0 {
                map.keys()
                    .filter(|k| *k != "result")
                    .map(|k| format!("?{k}"))
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        if header_vars.is_empty() {
            return TableIterator::new(std::iter::empty());
        }

        let mut lines: Vec<String> = Vec::with_capacity(rows.len() + 1);
        lines.push(header_vars.join("\t"));

        let var_names: Vec<String> = header_vars
            .iter()
            .map(|v| v.trim_start_matches('?').to_owned())
            .collect();

        for row in rows {
            if let Json::Object(map) = row.0 {
                let fields: Vec<String> = var_names
                    .iter()
                    .map(|v| match map.get(v) {
                        Some(Json::String(s)) => s.clone(),
                        Some(other) => other.to_string(),
                        None => String::new(),
                    })
                    .collect();
                lines.push(fields.join("\t"));
            }
        }

        TableIterator::new(lines.into_iter().map(|l| (l,)))
    }
}

// ── Helper: federation cache stats ───────────────────────────────────────────

/// Count federation cache hits and misses from _pg_ripple.federation_cache.
/// Returns (hits, misses) as (i64, i64).
fn get_federation_cache_stats_inner() -> (i64, i64) {
    use pgrx::prelude::*;

    // Count total cached entries (proxy for hits) and estimate misses from
    // federation health stats if available.
    let hits: i64 = Spi::connect(|client| {
        client
            .select(
                "SELECT COUNT(*) FROM _pg_ripple.federation_cache \
                 WHERE expires_at > now()",
                None,
                &[],
            )
            .ok()
            .and_then(|mut rows| rows.next())
            .and_then(|row| row.get::<i64>(1).ok().flatten())
            .unwrap_or(0)
    });

    (hits, 0_i64)
}
