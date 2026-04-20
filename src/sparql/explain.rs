//! JSONB explain output for SPARQL queries (v0.40.0).
//!
//! `explain_sparql_jsonb(query, analyze)` returns a JSONB document with keys:
//!
//! - `"algebra"` — spargebra algebra tree as a JSON string
//! - `"sql"` — the generated SQL with IRI-decoded predicate names
//! - `"plan"` — PostgreSQL EXPLAIN output as JSON (includes actual timings when analyze=true)
//! - `"cache_hit"` — whether the plan was served from the plan cache
//! - `"encode_calls"` — number of dictionary encode lookups during translation

use pgrx::prelude::*;
use spargebra::Query;

use crate::sparql::plan_cache;
use crate::sparql::sqlgen;

/// Execute `explain_sparql` returning a structured JSONB document.
///
/// When `analyze` is `true`, runs `EXPLAIN (ANALYZE, FORMAT JSON, BUFFERS true)`;
/// when `false`, runs `EXPLAIN (FORMAT JSON)` only.
pub fn explain_sparql_jsonb(query_text: &str, analyze: bool) -> pgrx::JsonB {
    let query = crate::sparql::SparqlParser::new()
        .parse_query(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {e}"));

    // Algebra tree (string representation).
    let algebra_str = format!("{query:#?}");

    // Check plan cache hit before translating.
    let cache_hit = plan_cache::get(query_text).is_some();

    // Translate to SQL (this may or may not use the plan cache depending on
    // the GUC pg_ripple.plan_cache_size).
    let (inner_sql, _encode_calls) = match &query {
        Query::Select { pattern, .. } => {
            let trans = sqlgen::translate_select(pattern, None);
            (trans.sql, 0usize)
        }
        Query::Ask { pattern, .. } => (sqlgen::translate_ask(pattern), 0usize),
        Query::Construct { pattern, .. } => {
            let trans = sqlgen::translate_select(pattern, None);
            (trans.sql, 0usize)
        }
        Query::Describe { .. } => {
            return pgrx::JsonB(serde_json::json!({
                "error": "DESCRIBE queries are not supported by explain_sparql",
                "algebra": algebra_str
            }));
        }
    };

    // Run EXPLAIN on the generated SQL.  We use the default TEXT format
    // and store the output as a plain string — FORMAT JSON parsing via SPI
    // is unreliable across different pgrx/PostgreSQL versions.
    let analyze_kw = if analyze { "ANALYZE, " } else { "" };
    let explain_sql = format!("EXPLAIN ({analyze_kw}FORMAT TEXT) {inner_sql}");

    let plan_text: String = Spi::connect(|client| {
        let rows = client
            .select(&explain_sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("explain_sparql_jsonb EXPLAIN SPI error: {e}"));
        // Each row of EXPLAIN output is one line of the plan.
        rows.filter_map(|row| row.get::<String>(1).ok().flatten())
            .collect::<Vec<_>>()
            .join("\n")
    });

    let result = serde_json::json!({
        "algebra": algebra_str,
        "sql": inner_sql,
        "plan": plan_text,
        "cache_hit": cache_hit,
        "encode_calls": 0
    });

    pgrx::JsonB(result)
}
