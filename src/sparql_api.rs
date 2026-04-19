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
