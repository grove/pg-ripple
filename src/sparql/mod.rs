//! SPARQL query engine for pg_ripple.
//!
//! # Public functions
//!
//! - `sparql(query TEXT) RETURNS SETOF JSONB` — execute SELECT/ASK
//! - `sparql_ask(query TEXT) RETURNS BOOLEAN` — execute ASK
//! - `sparql_explain(query TEXT, analyze BOOL) RETURNS TEXT` — show generated SQL
//! - `sparql_construct(query TEXT) RETURNS SETOF JSONB` — execute CONSTRUCT
//! - `sparql_describe(iri TEXT) RETURNS SETOF JSONB` — execute DESCRIBE (CBD)
//! - `sparql_update(query TEXT) RETURNS BIGINT` — execute INSERT/DELETE DATA
//! - `plan_cache_stats() RETURNS JSONB` — hit/miss/size/capacity counters
//! - `plan_cache_reset() RETURNS VOID` — evict all cached plans and reset counters
//!
//! # Pipeline
//!
//! 1. Parse with `spargebra::SparqlParser` (spargebra 0.4).
//! 2. Optimize with `sparopt::Optimizer::optimize_graph_pattern`.
//! 3. Translate to SQL via `sqlgen` (with BGP reordering if enabled).
//! 4. Check query plan cache; skip translation if hit.
//! 5. Execute via SPI; collect all i64 result values.
//! 6. Batch-decode i64s via a single `WHERE id = ANY(...)` query.
//! 7. Emit decoded rows as `JSONB`.

// ─── Sub-modules ─────────────────────────────────────────────────────────────

pub(crate) mod cursor;
pub(crate) mod embedding;
pub(crate) mod explain;
mod expr;
pub(crate) mod federation;
pub(crate) mod federation_planner;
mod optimizer;
mod plan_cache;
mod property_path;
pub(crate) mod ql_rewrite;
pub(crate) mod sparqldl;
pub(crate) mod sqlgen;
pub mod translate;
pub(crate) mod wcoj;

// New single-responsibility sub-modules (v0.69.0 ARCH-01).
pub(crate) mod decode;
pub(crate) mod execute;
pub(crate) mod parse;
pub(crate) mod plan;

// ─── Re-exports for external callers ─────────────────────────────────────────
// framing/mod.rs and other modules call these via `crate::sparql::*`.

pub(crate) use decode::batch_decode;
pub(crate) use execute::{
    explain_sparql, plan_cache_reset, plan_cache_stats, sparql_construct, sparql_construct_rows,
    sparql_describe, sparql_update,
};
pub(crate) use plan::{
    ConstructTemplate, apply_construct_template, prepare_construct, prepare_select,
};

use pgrx::prelude::*;
use serde_json::{Map, Value as Json};
use spargebra::SparqlParser;

pub fn sparql(query_text: &str) -> Vec<pgrx::JsonB> {
    // Normalize ARQ aggregate extensions (MEDIAN/MODE) before parsing.
    let preprocessed = parse::preprocess_arq_aggregates(query_text);
    let query_text = preprocessed.as_str();

    // Determine query type.
    let query = SparqlParser::new()
        .parse_query(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {}", e));

    match query {
        spargebra::Query::Select { .. } => {
            let (sql, vars, raw_numeric, raw_text, raw_iri, raw_double, wcoj) =
                prepare_select(query_text);
            execute::execute_select(
                &sql,
                &vars,
                &raw_numeric,
                &raw_text,
                &raw_iri,
                &raw_double,
                wcoj,
            )
        }
        spargebra::Query::Ask { pattern, .. } => {
            let sql = sqlgen::translate_ask(&pattern);
            let result: bool = Spi::get_one::<bool>(&sql)
                .unwrap_or_else(|e| pgrx::error!("SPARQL ASK SPI error: {e}"))
                .unwrap_or(false);
            let mut obj = Map::new();
            obj.insert("result".to_owned(), Json::String(result.to_string()));
            vec![pgrx::JsonB(Json::Object(obj))]
        }
        _ => {
            pgrx::error!("sparql() supports SELECT and ASK; use sparql_explain() for debugging");
        }
    }
}

/// Execute a SPARQL ASK query; returns a boolean.
pub fn sparql_ask(query_text: &str) -> bool {
    let query = SparqlParser::new()
        .parse_query(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {}", e));

    let pattern = match query {
        spargebra::Query::Ask { pattern, .. } => pattern,
        _ => pgrx::error!("sparql_ask() requires an ASK query"),
    };

    let sql = sqlgen::translate_ask(&pattern);

    Spi::get_one::<bool>(&sql)
        .unwrap_or_else(|e| pgrx::error!("SPARQL ASK SPI error: {e}"))
        .unwrap_or(false)
}

/// Return the generated SQL for a SPARQL SELECT query (for debugging/explain).
/// If `analyze` is true, wraps in EXPLAIN ANALYZE.
pub fn sparql_explain(query_text: &str, analyze: bool) -> String {
    let query = SparqlParser::new()
        .parse_query(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {}", e));

    let (inner_sql, vars) = match query {
        spargebra::Query::Select { pattern, .. } => {
            let trans = sqlgen::translate_select(&pattern, None);
            (trans.sql, trans.variables)
        }
        spargebra::Query::Ask { pattern, .. } => {
            let sql = sqlgen::translate_ask(&pattern);
            (sql, vec!["result".to_owned()])
        }
        _ => pgrx::error!("sparql_explain() supports SELECT and ASK queries"),
    };

    if !analyze {
        return format!("-- Generated SQL --\n{inner_sql}\n-- Variables: {vars:?}");
    }

    // EXPLAIN ANALYZE the generated SQL.
    let explain_sql = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT) {inner_sql}");
    let plan: String = Spi::connect(|client| {
        let rows = client
            .select(&explain_sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("EXPLAIN SPI error: {e}"));
        let lines: Vec<String> = rows
            .filter_map(|row| row.get::<String>(1).ok().flatten())
            .collect();
        lines.join("\n")
    });

    format!("-- Generated SQL --\n{inner_sql}\n\n-- EXPLAIN ANALYZE --\n{plan}")
}

/// Execute a SPARQL SELECT query and return the result as a JSON string.
///
/// Used by the subscription notification path (SUB-01) to compute the
/// current result set and send it via `pg_notify`.
///
/// Returns `Err` with a descriptive message if the query fails or is not a SELECT.
#[allow(dead_code)]
pub fn sparql_query_to_json(query_text: &str) -> Result<String, String> {
    let preprocessed = parse::preprocess_arq_aggregates(query_text);
    let query_text = preprocessed.as_str();

    let query = SparqlParser::new()
        .parse_query(query_text)
        .map_err(|e| format!("SPARQL parse error: {e}"))?;

    let results = match query {
        spargebra::Query::Select { .. } => {
            let (sql, vars, raw_numeric, raw_text, raw_iri, raw_double, wcoj) =
                prepare_select(query_text);
            execute::execute_select(
                &sql,
                &vars,
                &raw_numeric,
                &raw_text,
                &raw_iri,
                &raw_double,
                wcoj,
            )
        }
        _ => return Err("subscribe_sparql only supports SELECT queries".to_string()),
    };

    // Serialize results to a compact JSON array string.
    let arr: Vec<serde_json::Value> = results.into_iter().map(|j| j.0).collect();
    let out = serde_json::to_string(&serde_json::Value::Array(arr))
        .map_err(|e| format!("JSON serialise error: {e}"))?;
    Ok(out)
}
