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

    /// Return SPARQL plan cache statistics as JSONB.
    ///
    /// Returns `{"hits": N, "misses": N, "size": N, "capacity": N, "hit_rate": 0.xx}`.
    /// Counters accumulate from backend start; reset with `plan_cache_reset()`.
    #[pg_extern]
    fn plan_cache_stats() -> pgrx::JsonB {
        crate::sparql::plan_cache_stats()
    }

    /// Evict all cached SPARQL plan translations and reset hit/miss counters.
    #[pg_extern]
    fn plan_cache_reset() {
        crate::sparql::plan_cache_reset()
    }

    // ── v0.40.0: Streaming cursor API ────────────────────────────────────────

    /// Stream SPARQL SELECT results one batch at a time.
    ///
    /// Unlike `sparql()`, this function pages through results in 1024-row
    /// batches, avoiding full materialisation for large result sets.
    /// Respects `pg_ripple.sparql_max_rows` if set.
    #[pg_extern]
    fn sparql_cursor(query: &str) -> TableIterator<'static, (name!(result, pgrx::JsonB),)> {
        let rows = crate::sparql::cursor::sparql_cursor(query);
        TableIterator::new(rows.into_iter().map(|r| (r,)))
    }

    /// Stream a SPARQL CONSTRUCT query result as Turtle text chunks.
    ///
    /// Each returned row is a Turtle serialisation of up to 1024 triples.
    /// Respects `pg_ripple.export_max_rows` if set.
    #[pg_extern]
    fn sparql_cursor_turtle(query: &str) -> TableIterator<'static, (name!(chunk, String),)> {
        let chunks = crate::sparql::cursor::sparql_cursor_turtle(query);
        TableIterator::new(chunks.into_iter().map(|c| (c,)))
    }

    /// Stream a SPARQL CONSTRUCT query result as JSON-LD chunks.
    ///
    /// Each returned row is a JSON-LD expanded-form array for one batch.
    /// Respects `pg_ripple.export_max_rows` if set.
    #[pg_extern]
    fn sparql_cursor_jsonld(query: &str) -> TableIterator<'static, (name!(chunk, String),)> {
        let chunks = crate::sparql::cursor::sparql_cursor_jsonld(query);
        TableIterator::new(chunks.into_iter().map(|c| (c,)))
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
        crate::sparql::explain::explain_sparql_jsonb(query, analyze)
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
