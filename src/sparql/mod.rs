//! SPARQL query engine for pg_ripple.
//!
//! # Public functions
//!
//! - `sparql(query TEXT) RETURNS SETOF JSONB` — execute SELECT/ASK
//! - `sparql_ask(query TEXT) RETURNS BOOLEAN` — execute ASK
//! - `sparql_explain(query TEXT, analyze BOOL) RETURNS TEXT` — show generated SQL
//!
//! # Pipeline
//!
//! 1. Parse with `spargebra::SparqlParser` (spargebra 0.4).
//! 2. Optimize with `sparopt::Optimizer::optimize_graph_pattern`.
//! 3. Translate to SQL via `sqlgen`.
//! 4. Check query plan cache; skip translation if hit.
//! 5. Execute via SPI; collect all i64 result values.
//! 6. Batch-decode i64s via a single `WHERE id = ANY(...)` query.
//! 7. Emit decoded rows as `JSONB`.

mod plan_cache;
mod sqlgen;

use std::collections::HashMap;

use pgrx::prelude::*;
use serde_json::{Map, Value as Json};
use spargebra::SparqlParser;

use crate::dictionary;

// ─── Batch dictionary decode ──────────────────────────────────────────────────

/// Decode a set of `i64` dictionary IDs to N-Triples–formatted strings in one
/// SPI round-trip.
fn batch_decode(ids: &[i64]) -> HashMap<i64, String> {
    if ids.is_empty() {
        return HashMap::new();
    }

    // Build `WHERE id IN (...)` with integer literals — safe because these are
    // i64 values produced by Rust, never user-controlled strings.
    let id_list = ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "SELECT id, value, kind, datatype, lang \
         FROM _pg_ripple.dictionary \
         WHERE id IN ({id_list})"
    );

    let mut result = HashMap::with_capacity(ids.len());

    Spi::connect(|client| {
        let rows = client
            .select(&sql, None, &[])
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

    result
}

// ─── Query execution helpers ──────────────────────────────────────────────────

/// Parse the query, optimize, translate to SQL, and cache the result.
/// Returns `(sql, variables)`.
fn prepare_select(query_text: &str) -> (String, Vec<String>) {
    if let Some(cached) = plan_cache::get(query_text) {
        return cached;
    }

    let query = SparqlParser::new()
        .parse_query(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {}", e));

    // NOTE: sparopt 0.3 uses its own algebra types (distinct from spargebra 0.4);
    // direct conversion is not yet available.  Filter-pushdown and constant-folding
    // are implemented in our own algebra pass (sqlgen.rs) as per the ROADMAP fallback.
    let pattern = match query {
        spargebra::Query::Select { pattern, .. } => pattern,
        spargebra::Query::Ask { pattern, .. } => pattern,
        spargebra::Query::Construct { .. } | spargebra::Query::Describe { .. } => {
            pgrx::error!("CONSTRUCT/DESCRIBE not yet supported in v0.3.0");
        }
    };

    let trans = sqlgen::translate_select(&pattern);
    let entry = (trans.sql, trans.variables);
    plan_cache::put(query_text, entry.clone());
    entry
}

/// Run a SELECT SQL and return rows as JSONB.
fn execute_select(sql: &str, variables: &[String]) -> Vec<pgrx::JsonB> {
    let mut all_ids: Vec<i64> = Vec::new();
    // First pass: collect result rows of i64s.
    let mut raw_rows: Vec<Vec<Option<i64>>> = Vec::new();

    Spi::connect(|client| {
        let rows = client
            .select(sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("SPARQL execute SPI error: {e}"));
        for row in rows {
            let mut row_vals: Vec<Option<i64>> = Vec::with_capacity(variables.len());
            for i in 1..=(variables.len() as i64) {
                let val = row.get::<i64>(i as _).ok().flatten();
                if let Some(id) = val {
                    all_ids.push(id);
                }
                row_vals.push(val);
            }
            raw_rows.push(row_vals);
        }
    });

    // Batch decode all collected IDs.
    all_ids.sort_unstable();
    all_ids.dedup();
    let decode_map = batch_decode(&all_ids);

    // Build JSONB rows.
    raw_rows
        .into_iter()
        .map(|row_vals| {
            let mut obj = Map::new();
            for (i, var) in variables.iter().enumerate() {
                let v = row_vals
                    .get(i)
                    .copied()
                    .flatten()
                    .and_then(|id| decode_map.get(&id))
                    .map(|s| Json::String(s.clone()))
                    .unwrap_or(Json::Null);
                obj.insert(var.clone(), v);
            }
            pgrx::JsonB(Json::Object(obj))
        })
        .collect()
}

// ─── Public functions exposed to PostgreSQL ───────────────────────────────────

/// Execute a SPARQL SELECT or ASK query; returns SETOF JSONB.
///
/// For SELECT queries each row is `{"var1": "value1", "var2": "value2", ...}`.
/// For ASK queries a single row `{"result": "true"}` or `{"result": "false"}` is returned.
pub fn sparql(query_text: &str) -> Vec<pgrx::JsonB> {
    // Determine query type.
    let query = SparqlParser::new()
        .parse_query(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {}", e));

    match query {
        spargebra::Query::Select { .. } => {
            let (sql, vars) = prepare_select(query_text);
            execute_select(&sql, &vars)
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
            let trans = sqlgen::translate_select(&pattern);
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
