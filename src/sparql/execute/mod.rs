//! SPARQL SPI execution: SELECT, CONSTRUCT, DESCRIBE, UPDATE operations.
//!
//! All functions in this module call PostgreSQL SPI to run generated SQL and
//! map results back to RDF terms via the dictionary.

// v0.90.0 CQ-02 / M15-13 v0.96.0: split sub-modules
pub mod construct;
pub mod describe;
// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[allow(dead_code)]
pub mod exec_core;
pub mod explain;
pub mod update;

pub(crate) use construct::{
    sparql_construct, sparql_construct_rows, sparql_construct_with_bindings,
};
pub(crate) use describe::{sparql_describe, sparql_describe_with_bindings};
pub(crate) use explain::{explain_sparql, plan_cache_reset, plan_cache_stats};
pub(crate) use update::sparql_update;

use pgrx::prelude::*;
use serde_json::{Map, Value as Json};

use super::decode::batch_decode;

// ─── SELECT execution ─────────────────────────────────────────────────────────

/// Run a SELECT SQL and return rows as JSONB.
///
/// `raw_numeric_vars` lists variables that hold raw SQL numbers (aggregates)
/// and must NOT be dictionary-decoded.
pub(super) fn execute_select(
    sql: &str,
    variables: &[String],
    raw_numeric_vars: &std::collections::HashSet<String>,
    raw_text_vars: &std::collections::HashSet<String>,
    raw_iri_vars: &std::collections::HashSet<String>,
    raw_double_vars: &std::collections::HashSet<String>,
    wcoj_preamble: bool,
) -> Vec<pgrx::JsonB> {
    execute_select_with_params(
        sql,
        variables,
        raw_numeric_vars,
        raw_text_vars,
        raw_iri_vars,
        raw_double_vars,
        wcoj_preamble,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_select_with_params(
    sql: &str,
    variables: &[String],
    raw_numeric_vars: &std::collections::HashSet<String>,
    raw_text_vars: &std::collections::HashSet<String>,
    raw_iri_vars: &std::collections::HashSet<String>,
    raw_double_vars: &std::collections::HashSet<String>,
    wcoj_preamble: bool,
    params: &[i64],
) -> Vec<pgrx::JsonB> {
    let mut all_ids: Vec<i64> = Vec::new();
    // First pass: collect result rows.
    // Columns that are raw text/IRI/double are stored as Err(String), others as Ok(i64).
    let mut raw_rows: Vec<Vec<Option<Result<i64, String>>>> = Vec::new();

    Spi::connect_mut(|client| {
        // v0.13.0: When BGP reordering is active, lock the planner into our
        // computed join order by disabling join reordering heuristics.
        // Use connect_mut + update() (read_only=false) so that SET LOCAL is
        // accepted by PostgreSQL's SPI layer.
        // P13-04 (v0.85.0): batch all planner-hint SET LOCAL calls into a single
        // SPI round-trip to reduce per-query SPI overhead.
        let bgp_reorder = crate::BGP_REORDER.get();
        let min_joins = crate::PARALLEL_QUERY_MIN_JOINS.get() as usize;
        let join_count = sql.matches(" AS _t").count();
        let wants_parallel = join_count >= min_joins;
        let mut set_stmts: Vec<&str> = Vec::new();
        if bgp_reorder {
            set_stmts.push("SET LOCAL join_collapse_limit = 1");
            set_stmts.push("SET LOCAL enable_mergejoin = on");
        }
        if wcoj_preamble {
            // v0.62.0: WCOJ Leapfrog-Triejoin preamble; each statement is included individually.
            let _ = client.update(crate::sparql::wcoj::wcoj_session_preamble(), None, &[]);
        }
        if wants_parallel {
            set_stmts.push("SET LOCAL max_parallel_workers_per_gather = 4");
            set_stmts.push("SET LOCAL enable_parallel_hash = on");
            set_stmts.push("SET LOCAL parallel_setup_cost = 10");
        }
        if !set_stmts.is_empty() {
            let batched = set_stmts.join("; ");
            let _ = client.update(&batched, None, &[]);
        }
        let args: Vec<pgrx::datum::DatumWithOid> = params
            .iter()
            .copied()
            .map(pgrx::datum::DatumWithOid::from)
            .collect();
        let rows = client
            .select(sql, None, &args)
            .unwrap_or_else(|e| pgrx::error!("SPARQL execute SPI error: {e}"));
        for row in rows {
            let mut row_vals: Vec<Option<Result<i64, String>>> =
                Vec::with_capacity(variables.len());
            for (col_idx, var) in variables.iter().enumerate() {
                let i = col_idx + 1;
                if raw_text_vars.contains(var)
                    || raw_iri_vars.contains(var)
                    || raw_double_vars.contains(var)
                {
                    // Read as text (GROUP_CONCAT / STRUUID / UUID / RAND result)
                    let text_val = row.get::<String>(i).ok().flatten().map(Err);
                    row_vals.push(text_val);
                } else {
                    // Read as i64 (dictionary ID or numeric aggregate)
                    let val = row.get::<i64>(i).ok().flatten();
                    // DECODE-WARN-01: only push to all_ids if this variable is not
                    // a raw numeric aggregate (COUNT/SUM/etc.). Raw numeric values
                    // are not dictionary IDs and must not be passed to batch_decode.
                    if let Some(id) = val
                        && !raw_numeric_vars.contains(var)
                    {
                        all_ids.push(id);
                    }
                    row_vals.push(val.map(Ok));
                }
            }
            raw_rows.push(row_vals);
        }
    });

    // Batch decode all collected IDs (skip raw numeric values).
    all_ids.sort_unstable();
    all_ids.dedup();
    let decode_map = batch_decode(&all_ids);

    // Build JSONB rows.
    raw_rows
        .into_iter()
        .map(|row_vals| {
            let mut obj = Map::new();
            for (i, var) in variables.iter().enumerate() {
                let raw_val = row_vals.get(i).and_then(|v| v.as_ref());
                let v = match raw_val {
                    None => Json::Null,
                    Some(Err(text)) => {
                        if raw_iri_vars.contains(var) {
                            // UUID() result: emit as `<iri>` IRI format.
                            Json::String(format!("<{}>", text))
                        } else if raw_double_vars.contains(var) {
                            // RAND() result: emit as `"val"^^xsd:double` format.
                            Json::String(format!(
                                "\"{}\"^^<http://www.w3.org/2001/XMLSchema#double>",
                                text
                            ))
                        } else {
                            // Raw text variable (GROUP_CONCAT / STRUUID): emit as JSON string literal.
                            Json::String(format!("\"{}\"", text.replace('"', "\\\"")))
                        }
                    }
                    Some(Ok(id)) => {
                        if raw_numeric_vars.contains(var) {
                            // Aggregate output: emit raw integer as JSON number.
                            Json::Number(serde_json::Number::from(*id))
                        } else {
                            // Dictionary-encoded variable: decode to N-Triples string.
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
        .collect()
}
