//! JSONB explain output for SPARQL queries (v0.40.0 + v0.50.0 + v0.59.0).
//!
//! `explain_sparql_jsonb(query, analyze, citus)` returns a JSONB document with keys:
//!
//! - `"algebra"` — spargebra algebra tree as a JSON string
//! - `"sql"` — the generated SQL with IRI-decoded predicate names
//! - `"plan"` — PostgreSQL EXPLAIN output as text (includes actual timings when analyze=true)
//! - `"cache_hit"` — (legacy) whether the plan was served from the plan cache
//! - `"cache_status"` — `"hit"` / `"miss"` / `"bypass"` cache state
//! - `"encode_calls"` — number of dictionary encode lookups during translation
//! - `"actual_rows"` — flat list of per-operator actual row counts (only when analyze=true)
//! - `"citus"` — Citus shard-pruning details (only when citus=true, v0.59.0)

use pgrx::prelude::*;
use spargebra::Query;

use crate::sparql::plan_cache;
use crate::sparql::sqlgen;

/// Recursively collect all "Actual Rows" values from a PostgreSQL EXPLAIN JSON
/// plan tree.  The plan is a JSON value of the form returned by
/// `EXPLAIN (FORMAT JSON)`.
fn collect_actual_rows(node: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
    match node {
        serde_json::Value::Array(arr) => {
            for elem in arr {
                collect_actual_rows(elem, out);
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(rows) = map.get("Actual Rows") {
                out.push(rows.clone());
            }
            // Recurse into Plan / Plans / InitPlan / SubPlan children.
            for key in ["Plan", "Plans", "InitPlan", "SubPlan"] {
                if let Some(child) = map.get(key) {
                    collect_actual_rows(child, out);
                }
            }
        }
        _ => {}
    }
}

/// Extract aggregate buffer I/O statistics from an EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
/// plan tree.  Returns a JSON object with keys `shared_hit`, `shared_read`,
/// `shared_dirtied`, `shared_written` (all integers, zero if not present).
fn extract_buffers(plan: &serde_json::Value) -> serde_json::Value {
    let mut shared_hit: i64 = 0;
    let mut shared_read: i64 = 0;
    let mut shared_dirtied: i64 = 0;
    let mut shared_written: i64 = 0;

    fn walk(node: &serde_json::Value, h: &mut i64, r: &mut i64, d: &mut i64, w: &mut i64) {
        match node {
            serde_json::Value::Array(arr) => {
                for elem in arr {
                    walk(elem, h, r, d, w);
                }
            }
            serde_json::Value::Object(map) => {
                *h += map
                    .get("Shared Hit Blocks")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                *r += map
                    .get("Shared Read Blocks")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                *d += map
                    .get("Shared Dirtied Blocks")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                *w += map
                    .get("Shared Written Blocks")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                for key in ["Plan", "Plans", "InitPlan", "SubPlan"] {
                    if let Some(child) = map.get(key) {
                        walk(child, h, r, d, w);
                    }
                }
            }
            _ => {}
        }
    }

    walk(
        plan,
        &mut shared_hit,
        &mut shared_read,
        &mut shared_dirtied,
        &mut shared_written,
    );

    serde_json::json!({
        "shared_hit": shared_hit,
        "shared_read": shared_read,
        "shared_dirtied": shared_dirtied,
        "shared_written": shared_written
    })
}

/// Execute `explain_sparql` returning a structured JSONB document.
///
/// When `analyze` is `true`, runs `EXPLAIN (ANALYZE, FORMAT JSON, BUFFERS true)`;
/// when `false`, runs `EXPLAIN (FORMAT JSON)` only.
///
/// When `citus` is `true` (v0.59.0, CITUS-12), an additional `"citus"` key is
/// included in the output showing shard-pruning details for the query.
pub fn explain_sparql_jsonb(query_text: &str, analyze: bool, citus: bool) -> pgrx::JsonB {
    let query = crate::sparql::SparqlParser::new()
        .parse_query(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {e}"));

    // Algebra tree (string representation).
    let algebra_str = format!("{query:#?}");

    // Determine cache_status before translating.
    let cache_size = crate::gucs::PLAN_CACHE_SIZE.get();
    let cache_status: &str = if cache_size <= 0 {
        "bypass"
    } else if plan_cache::get(query_text).is_some() {
        "hit"
    } else {
        "miss"
    };
    let cache_hit = cache_status == "hit";

    // Translate to SQL.
    let (inner_sql, topn_applied) = match &query {
        Query::Select { pattern, .. } => {
            let trans = sqlgen::translate_select(pattern, None);
            (trans.sql, trans.topn_applied)
        }
        Query::Ask { pattern, .. } => (sqlgen::translate_ask(pattern), false),
        Query::Construct { pattern, .. } => {
            let trans = sqlgen::translate_select(pattern, None);
            (trans.sql, trans.topn_applied)
        }
        Query::Describe { .. } => {
            // DESCRIBE: return explain information with a synthetic SELECT.
            let describe_sql = "SELECT s.value AS subject, p.value AS predicate, \
                                o.value AS object \
                FROM _pg_ripple.dictionary s, _pg_ripple.dictionary p, \
                     _pg_ripple.dictionary o LIMIT 0"
                .to_owned();
            (describe_sql, false)
        }
    };

    // Run EXPLAIN on the generated SQL.
    // Use FORMAT TEXT for the `plan` field (backward-compatible, always non-empty),
    // and a separate EXPLAIN (ANALYZE, FORMAT JSON) pass for actual_rows when requested.
    let analyze_text_kw = if analyze { "ANALYZE, " } else { "" };
    let explain_text_sql = format!("EXPLAIN ({analyze_text_kw}FORMAT TEXT) {inner_sql}");

    let plan_text: String = Spi::connect(|client| {
        client
            .select(&explain_text_sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("explain_sparql_jsonb EXPLAIN SPI error: {e}"))
            .filter_map(|row| row.get::<String>(1).ok().flatten())
            .collect::<Vec<_>>()
            .join("\n")
    });

    // When analyze=true, run a separate EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) to extract
    // per-operator actual row counts and buffer I/O stats from the JSON plan tree.
    let actual_rows_json: serde_json::Value;
    let buffers_json: serde_json::Value;
    if analyze {
        let explain_json_sql = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {inner_sql}");
        let json_str: String = Spi::connect(|client| {
            client
                .select(&explain_json_sql, None, &[])
                .unwrap_or_else(|e| pgrx::error!("explain_sparql_jsonb ANALYZE SPI error: {e}"))
                .filter_map(|row| row.get::<String>(1).ok().flatten())
                .collect::<Vec<_>>()
                .join("")
        });
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);
        let mut rows_out = Vec::new();
        collect_actual_rows(&parsed, &mut rows_out);
        actual_rows_json = serde_json::Value::Array(rows_out);
        buffers_json = extract_buffers(&parsed);
    } else {
        actual_rows_json = serde_json::Value::Null;
        buffers_json = serde_json::Value::Null;
    };

    let mut result = serde_json::json!({
        "algebra": algebra_str,
        "sql": inner_sql,
        "plan": plan_text,
        "cache_hit": cache_hit,
        "cache_status": cache_status,
        "encode_calls": 0,
        "topn_applied": topn_applied
    });

    if analyze {
        result["actual_rows"] = actual_rows_json;
        result["buffers"] = buffers_json;
    }

    // v0.59.0 (CITUS-12): add Citus shard-pruning section when citus=true.
    if citus {
        result["citus"] = crate::citus::explain_citus_section(query_text);
    }

    pgrx::JsonB(result)
}
