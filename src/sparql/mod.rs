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

mod optimizer;
mod plan_cache;
mod property_path;
pub(crate) mod sqlgen;

use std::collections::HashMap;

use pgrx::prelude::*;
use serde_json::{Map, Value as Json};
use spargebra::GraphUpdateOperation;
use spargebra::SparqlParser;
use spargebra::term::{GraphName, NamedOrBlankNode, Term};

use crate::dictionary;
use crate::storage;

// ─── Batch dictionary decode ──────────────────────────────────────────────────

/// Decode a set of `i64` dictionary IDs to N-Triples–formatted strings in one
/// SPI round-trip.  Inline-encoded values (bit 63 = 1) are decoded directly
/// without a DB lookup; only true dictionary IDs are fetched from the table.
fn batch_decode(ids: &[i64]) -> HashMap<i64, String> {
    if ids.is_empty() {
        return HashMap::new();
    }

    let mut result = HashMap::with_capacity(ids.len());

    // Split: inline IDs (negative) are decoded locally; positives need DB lookup.
    let dict_ids: Vec<i64> = ids
        .iter()
        .copied()
        .filter(|&id| {
            if dictionary::inline::is_inline(id) {
                result.insert(id, dictionary::inline::format_inline(id));
                false
            } else {
                true
            }
        })
        .collect();

    if dict_ids.is_empty() {
        return result;
    }

    // Build `WHERE id IN (...)` with integer literals — safe because these are
    // i64 values produced by Rust, never user-controlled strings.
    let id_list = dict_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "SELECT id, value, kind, datatype, lang \
         FROM _pg_ripple.dictionary \
         WHERE id IN ({id_list})"
    );

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
/// Returns `(sql, variables, raw_numeric_vars)`.
fn prepare_select(query_text: &str) -> (String, Vec<String>, std::collections::HashSet<String>) {
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
    let entry = (trans.sql, trans.variables, trans.raw_numeric_vars);
    plan_cache::put(query_text, entry.clone());
    entry
}

/// Run a SELECT SQL and return rows as JSONB.
///
/// `raw_numeric_vars` lists variables that hold raw SQL numbers (aggregates)
/// and must NOT be dictionary-decoded.
fn execute_select(
    sql: &str,
    variables: &[String],
    raw_numeric_vars: &std::collections::HashSet<String>,
) -> Vec<pgrx::JsonB> {
    let mut all_ids: Vec<i64> = Vec::new();
    // First pass: collect result rows of i64s.
    let mut raw_rows: Vec<Vec<Option<i64>>> = Vec::new();

    Spi::connect_mut(|client| {
        // v0.13.0: When BGP reordering is active, lock the planner into our
        // computed join order by disabling join reordering heuristics.
        // Use connect_mut + update() (read_only=false) so that SET LOCAL is
        // accepted by PostgreSQL's SPI layer.
        if crate::BGP_REORDER.get() {
            let _ = client.update("SET LOCAL join_collapse_limit = 1", None, &[]);
            let _ = client.update("SET LOCAL enable_mergejoin = on", None, &[]);
        }

        // v0.13.0: Enable parallel query for queries that join multiple VP tables.
        // Count approximate number of VP-table scans by alias pattern in the SQL.
        let min_joins = crate::PARALLEL_QUERY_MIN_JOINS.get() as usize;
        let join_count = sql.matches(" AS _t").count();
        if join_count >= min_joins {
            let _ = client.update("SET LOCAL max_parallel_workers_per_gather = 4", None, &[]);
            let _ = client.update("SET LOCAL enable_parallel_hash = on", None, &[]);
            let _ = client.update("SET LOCAL parallel_setup_cost = 10", None, &[]);
        }
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
                let raw_val = row_vals.get(i).copied().flatten();
                let v = if raw_numeric_vars.contains(var) {
                    // Aggregate output: emit raw integer as JSON number.
                    raw_val
                        .map(|n| Json::Number(serde_json::Number::from(n)))
                        .unwrap_or(Json::Null)
                } else {
                    // Dictionary-encoded variable: decode to N-Triples string.
                    raw_val
                        .and_then(|id| decode_map.get(&id))
                        .map(|s| Json::String(s.clone()))
                        .unwrap_or(Json::Null)
                };
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
            let (sql, vars, raw_numeric) = prepare_select(query_text);
            execute_select(&sql, &vars, &raw_numeric)
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

// ─── SPARQL CONSTRUCT ─────────────────────────────────────────────────────────

/// Execute a SPARQL CONSTRUCT query; returns one JSONB row per constructed triple.
///
/// Each row is `{"s": "<iri>", "p": "<iri>", "o": "..."}`.
pub fn sparql_construct(query_text: &str) -> Vec<pgrx::JsonB> {
    let query = SparqlParser::new()
        .parse_query(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {}", e));

    let (template, pattern) = match query {
        spargebra::Query::Construct {
            template, pattern, ..
        } => (template, pattern),
        _ => pgrx::error!("sparql_construct() requires a CONSTRUCT query"),
    };

    // Translate the WHERE clause as a SELECT over all template variables.
    let trans = sqlgen::translate_select(&pattern);
    let (sql, variables) = (trans.sql, trans.variables);
    let var_set: std::collections::HashSet<&str> = variables.iter().map(|s| s.as_str()).collect();

    // Execute and collect raw rows.
    let mut all_ids: Vec<i64> = Vec::new();
    let mut raw_rows: Vec<Vec<Option<i64>>> = Vec::new();
    Spi::connect(|client| {
        let rows = client
            .select(&sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("SPARQL CONSTRUCT SPI error: {e}"));
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

    all_ids.sort_unstable();
    all_ids.dedup();
    let decode_map = batch_decode(&all_ids);

    // Build a var → decoded-value map helper.
    let resolve = |row_vals: &[Option<i64>], var: &str| -> Option<String> {
        let idx = variables.iter().position(|v| v == var)?;
        let id = row_vals.get(idx).copied().flatten()?;
        decode_map.get(&id).cloned()
    };

    // Instantiate the CONSTRUCT template for each result row.
    let mut result = Vec::new();
    for row_vals in &raw_rows {
        for triple in &template {
            // Resolve subject (TermPattern).
            let s_val = match &triple.subject {
                spargebra::term::TermPattern::NamedNode(nn) => Some(format!("<{}>", nn.as_str())),
                spargebra::term::TermPattern::Variable(v) => {
                    if var_set.contains(v.as_str()) {
                        resolve(row_vals, v.as_str())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            // Resolve predicate (NamedNodePattern).
            let p_val = match &triple.predicate {
                spargebra::term::NamedNodePattern::NamedNode(nn) => {
                    Some(format!("<{}>", nn.as_str()))
                }
                spargebra::term::NamedNodePattern::Variable(v) => {
                    if var_set.contains(v.as_str()) {
                        resolve(row_vals, v.as_str())
                    } else {
                        None
                    }
                }
            };
            // Resolve object.
            let o_val = match &triple.object {
                spargebra::term::TermPattern::NamedNode(nn) => Some(format!("<{}>", nn.as_str())),
                spargebra::term::TermPattern::Literal(lit) => {
                    let lang = lit.language();
                    let dt = lit.datatype().as_str();
                    let kind = if lang.is_some() {
                        dictionary::KIND_LANG_LITERAL
                    } else {
                        dictionary::KIND_TYPED_LITERAL
                    };
                    Some(dictionary::format_ntriples_term(
                        lit.value(),
                        kind,
                        Some(dt),
                        lang,
                        0,
                    ))
                }
                spargebra::term::TermPattern::BlankNode(_) => None,
                spargebra::term::TermPattern::Triple(_) => None,
                spargebra::term::TermPattern::Variable(v) => {
                    if var_set.contains(v.as_str()) {
                        resolve(row_vals, v.as_str())
                    } else {
                        None
                    }
                }
            };

            // Only emit the triple if all three components are bound.
            if let (Some(s), Some(p), Some(o)) = (s_val, p_val, o_val) {
                let mut obj = Map::new();
                obj.insert("s".to_owned(), Json::String(s));
                obj.insert("p".to_owned(), Json::String(p));
                obj.insert("o".to_owned(), Json::String(o));
                result.push(pgrx::JsonB(Json::Object(obj)));
            }
        }
    }

    result
}

// ─── SPARQL DESCRIBE ──────────────────────────────────────────────────────────

/// Execute a SPARQL DESCRIBE query using the Concise Bounded Description (CBD)
/// algorithm; returns one JSONB row per described triple.
///
/// CBD: for the described resource IRI, fetch all outgoing triples.  If any
/// object is a blank node, recursively fetch its outgoing triples too, until
/// no new blank nodes are encountered.
///
/// `strategy` selects the algorithm: `"cbd"` (default), `"scbd"` (symmetric
/// — also fetches incoming arcs), or `"simple"` (one-hop outgoing only).
pub fn sparql_describe(query_text: &str, strategy: &str) -> Vec<pgrx::JsonB> {
    let query = SparqlParser::new()
        .parse_query(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {}", e));

    // In spargebra 0.4, DESCRIBE resources are encoded as projected SELECT
    // variables in the `pattern`.  Execute the pattern as a SELECT to obtain
    // the dictionary IDs of the resources to describe.
    let pattern = match query {
        spargebra::Query::Describe { pattern, .. } => pattern,
        _ => pgrx::error!("sparql_describe() requires a DESCRIBE query"),
    };

    let trans = sqlgen::translate_select(&pattern);
    let (sql, variables) = (trans.sql, trans.variables);

    // Collect all result IDs from the projected variables.
    let mut resource_ids: Vec<i64> = Vec::new();
    Spi::connect(|client| {
        let rows = client
            .select(&sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("DESCRIBE SELECT SPI error: {e}"));
        for row in rows {
            for i in 1..=(variables.len() as i64) {
                if let Some(id) = row.get::<i64>(i as _).ok().flatten() {
                    resource_ids.push(id);
                }
            }
        }
    });
    resource_ids.sort_unstable();
    resource_ids.dedup();

    let symmetric = strategy == "scbd";
    let mut result = Vec::new();
    for subject_id in resource_ids {
        let triples = describe_cbd(subject_id, symmetric);
        for (s_id, p_id, o_id) in triples {
            let s = dictionary::format_ntriples(s_id);
            let p = dictionary::format_ntriples(p_id);
            let o = dictionary::format_ntriples(o_id);
            let mut obj = Map::new();
            obj.insert("s".to_owned(), Json::String(s));
            obj.insert("p".to_owned(), Json::String(p));
            obj.insert("o".to_owned(), Json::String(o));
            result.push(pgrx::JsonB(Json::Object(obj)));
        }
    }
    result
}

/// Collect CBD triples for a subject ID.
/// Returns `(s_id, p_id, o_id)` tuples.
fn describe_cbd(subject_id: i64, symmetric: bool) -> Vec<(i64, i64, i64)> {
    let mut triples: Vec<(i64, i64, i64)> = Vec::new();
    let mut visited: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut queue: Vec<i64> = vec![subject_id];

    while let Some(s_id) = queue.pop() {
        if !visited.insert(s_id) {
            continue;
        }
        // Outgoing arcs from s_id across all predicates.
        let outgoing = storage::triples_for_subject(s_id);
        for (p_id, o_id) in outgoing {
            triples.push((s_id, p_id, o_id));
            // Recurse on blank node objects.
            if dictionary::is_blank_node(o_id) && !visited.contains(&o_id) {
                queue.push(o_id);
            }
        }
        // Symmetric CBD: also fetch incoming arcs.
        if symmetric {
            let incoming = storage::triples_for_object(s_id);
            for (s2_id, p_id) in incoming {
                triples.push((s2_id, p_id, s_id));
                if dictionary::is_blank_node(s2_id) && !visited.contains(&s2_id) {
                    queue.push(s2_id);
                }
            }
        }
    }

    triples
}

// ─── SPARQL Update (all operations) ──────────────────────────────────────────

/// Execute a SPARQL Update statement.  Returns the total number of affected
/// triples (inserted + deleted).
pub fn sparql_update(query_text: &str) -> i64 {
    let update = SparqlParser::new()
        .parse_update(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL Update parse error: {}", e));

    let mut affected: i64 = 0;
    for op in &update.operations {
        match op {
            GraphUpdateOperation::InsertData { data } => {
                for quad in data {
                    let s_id = encode_named_or_blank(&quad.subject);
                    let p_id = dictionary::encode(quad.predicate.as_str(), dictionary::KIND_IRI);
                    let o_id = encode_term_value(&quad.object);
                    let g_id = match &quad.graph_name {
                        GraphName::DefaultGraph => 0i64,
                        GraphName::NamedNode(nn) => {
                            dictionary::encode(nn.as_str(), dictionary::KIND_IRI)
                        }
                    };
                    storage::insert_triple_by_ids(s_id, p_id, o_id, g_id);
                    affected += 1;
                }
            }
            GraphUpdateOperation::DeleteData { data } => {
                for quad in data {
                    let s_id = dictionary::lookup_iri(quad.subject.as_str());
                    let p_id = dictionary::lookup_iri(quad.predicate.as_str());
                    let o_id = lookup_ground_term_value(&quad.object);
                    let g_id: i64 = match &quad.graph_name {
                        GraphName::DefaultGraph => 0i64,
                        GraphName::NamedNode(nn) => {
                            dictionary::lookup_iri(nn.as_str()).unwrap_or(-1)
                        }
                    };
                    // Only attempt delete if all terms exist in the dictionary.
                    if let (Some(s), Some(p), Some(o)) = (s_id, p_id, o_id)
                        && g_id >= 0
                    {
                        affected += storage::delete_triple_by_ids(s, p, o, g_id);
                    }
                }
            }
            GraphUpdateOperation::DeleteInsert {
                delete,
                insert,
                pattern,
                ..
            } => {
                affected += execute_delete_insert(delete, insert, pattern);
            }
            GraphUpdateOperation::Load {
                source,
                destination,
                silent,
            } => {
                let result = execute_load(source.as_str(), destination);
                match result {
                    Ok(n) => affected += n,
                    Err(e) => {
                        if *silent {
                            pgrx::warning!("SPARQL LOAD failed (silent): {e}");
                        } else {
                            pgrx::error!("SPARQL LOAD error: {e}");
                        }
                    }
                }
            }
            GraphUpdateOperation::Clear { graph, silent } => {
                let result = execute_clear(graph);
                match result {
                    Ok(n) => affected += n,
                    Err(e) => {
                        if *silent {
                            pgrx::warning!("SPARQL CLEAR failed (silent): {e}");
                        } else {
                            pgrx::error!("SPARQL CLEAR error: {e}");
                        }
                    }
                }
            }
            GraphUpdateOperation::Create { graph, silent } => {
                // Encode the graph IRI to register it in the dictionary.
                let g_id = dictionary::encode(graph.as_str(), dictionary::KIND_IRI);
                if g_id <= 0 && !silent {
                    pgrx::error!("SPARQL CREATE GRAPH: failed to register graph IRI");
                }
                // No triples to count; graph is "created" by dictionary registration.
            }
            GraphUpdateOperation::Drop { graph, silent } => {
                let result = execute_drop(graph);
                match result {
                    Ok(n) => affected += n,
                    Err(e) => {
                        if *silent {
                            pgrx::warning!("SPARQL DROP failed (silent): {e}");
                        } else {
                            pgrx::error!("SPARQL DROP error: {e}");
                        }
                    }
                }
            }
        }
    }

    affected
}

// ─── DELETE/INSERT WHERE ──────────────────────────────────────────────────────

/// Execute a `DELETE/INSERT WHERE` pattern-based update.
/// Returns the total number of triples deleted + inserted.
fn execute_delete_insert(
    delete_templates: &[spargebra::term::GroundQuadPattern],
    insert_templates: &[spargebra::term::QuadPattern],
    pattern: &spargebra::algebra::GraphPattern,
) -> i64 {
    // 1. Translate WHERE clause to SQL via the existing SELECT engine.
    let trans = sqlgen::translate_select(pattern);
    let (sql, variables) = (trans.sql, trans.variables);

    // 2. Execute the WHERE query and collect bound result rows.
    //    We get back raw i64 dictionary IDs per variable.
    let mut raw_rows: Vec<Vec<Option<i64>>> = Vec::new();
    Spi::connect(|client| {
        let rows = client
            .select(&sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("DELETE/INSERT WHERE SPI error: {e}"));
        for row in rows {
            let mut row_vals: Vec<Option<i64>> = Vec::with_capacity(variables.len());
            for i in 1..=(variables.len() as i64) {
                row_vals.push(row.get::<i64>(i as _).ok().flatten());
            }
            raw_rows.push(row_vals);
        }
    });

    if raw_rows.is_empty() {
        return 0;
    }

    // Build a variable → column-index map.
    let var_index: HashMap<&str, usize> = variables
        .iter()
        .enumerate()
        .map(|(i, v)| (v.as_str(), i))
        .collect();

    let mut affected: i64 = 0;

    // 3. For each bound row, resolve and execute deletes, then inserts.
    for row_vals in &raw_rows {
        // DELETE phase.
        for qp in delete_templates {
            let s_id = resolve_ground_term(&qp.subject, row_vals, &var_index);
            let p_id = resolve_named_node_pattern(&qp.predicate, row_vals, &var_index);
            let o_id = resolve_ground_term(&qp.object, row_vals, &var_index);
            let g_id = resolve_graph_name_pattern(&qp.graph_name, row_vals, &var_index);
            if let (Some(s), Some(p), Some(o), Some(g)) = (s_id, p_id, o_id, g_id) {
                affected += storage::delete_triple_by_ids(s, p, o, g);
            }
        }

        // INSERT phase.
        for qp in insert_templates {
            let s_id = resolve_term_pattern(&qp.subject, row_vals, &var_index);
            let p_id = resolve_named_node_pattern(&qp.predicate, row_vals, &var_index);
            let o_id = resolve_term_pattern(&qp.object, row_vals, &var_index);
            let g_id = resolve_graph_name_pattern(&qp.graph_name, row_vals, &var_index);
            if let (Some(s), Some(p), Some(o), Some(g)) = (s_id, p_id, o_id, g_id) {
                storage::insert_triple_by_ids(s, p, o, g);
                affected += 1;
            }
        }
    }

    affected
}

/// Resolve a `GroundTermPattern` to a dictionary i64.
fn resolve_ground_term(
    gtp: &spargebra::term::GroundTermPattern,
    row: &[Option<i64>],
    var_index: &HashMap<&str, usize>,
) -> Option<i64> {
    match gtp {
        spargebra::term::GroundTermPattern::NamedNode(nn) => {
            Some(dictionary::encode(nn.as_str(), dictionary::KIND_IRI))
        }
        spargebra::term::GroundTermPattern::Literal(lit) => Some(encode_literal_id(lit)),
        spargebra::term::GroundTermPattern::Variable(v) => {
            let idx = var_index.get(v.as_str())?;
            *row.get(*idx)?
        }
        spargebra::term::GroundTermPattern::Triple(_) => None,
    }
}

/// Resolve a `TermPattern` to a dictionary i64.
fn resolve_term_pattern(
    tp: &spargebra::term::TermPattern,
    row: &[Option<i64>],
    var_index: &HashMap<&str, usize>,
) -> Option<i64> {
    match tp {
        spargebra::term::TermPattern::NamedNode(nn) => {
            Some(dictionary::encode(nn.as_str(), dictionary::KIND_IRI))
        }
        spargebra::term::TermPattern::Literal(lit) => Some(encode_literal_id(lit)),
        spargebra::term::TermPattern::BlankNode(bn) => {
            let scoped = format!("{}:{}", storage::current_load_generation(), bn.as_str());
            Some(dictionary::encode(&scoped, dictionary::KIND_BLANK))
        }
        spargebra::term::TermPattern::Variable(v) => {
            let idx = var_index.get(v.as_str())?;
            *row.get(*idx)?
        }
        spargebra::term::TermPattern::Triple(_) => None,
    }
}

/// Resolve a `NamedNodePattern` to a dictionary i64.
fn resolve_named_node_pattern(
    nnp: &spargebra::term::NamedNodePattern,
    row: &[Option<i64>],
    var_index: &HashMap<&str, usize>,
) -> Option<i64> {
    match nnp {
        spargebra::term::NamedNodePattern::NamedNode(nn) => {
            Some(dictionary::encode(nn.as_str(), dictionary::KIND_IRI))
        }
        spargebra::term::NamedNodePattern::Variable(v) => {
            let idx = var_index.get(v.as_str())?;
            *row.get(*idx)?
        }
    }
}

/// Resolve a `GraphNamePattern` to a dictionary i64 (0 = default graph).
fn resolve_graph_name_pattern(
    gnp: &spargebra::term::GraphNamePattern,
    row: &[Option<i64>],
    var_index: &HashMap<&str, usize>,
) -> Option<i64> {
    match gnp {
        spargebra::term::GraphNamePattern::DefaultGraph => Some(0i64),
        spargebra::term::GraphNamePattern::NamedNode(nn) => {
            Some(dictionary::encode(nn.as_str(), dictionary::KIND_IRI))
        }
        spargebra::term::GraphNamePattern::Variable(v) => {
            let idx = var_index.get(v.as_str())?;
            *row.get(*idx)?
        }
    }
}

/// Encode a `Literal` into a dictionary i64.
fn encode_literal_id(lit: &spargebra::term::Literal) -> i64 {
    let lang = lit.language();
    let value = lit.value();
    let dt = lit.datatype().as_str();
    if let Some(l) = lang {
        dictionary::encode_lang_literal(value, l)
    } else if dt == "http://www.w3.org/2001/XMLSchema#string"
        || dt == "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
    {
        dictionary::encode(value, dictionary::KIND_LITERAL)
    } else {
        dictionary::encode_typed_literal(value, dt)
    }
}

// ─── SPARQL LOAD ─────────────────────────────────────────────────────────────

/// Fetch a URL via HTTP and load the RDF into the given graph.
/// Supports Turtle (text/turtle, .ttl) and N-Triples (application/n-triples, .nt).
/// Returns number of triples inserted, or an error message.
fn execute_load(url: &str, destination: &GraphName) -> Result<i64, String> {
    // Determine destination graph ID.
    let g_id: i64 = match destination {
        GraphName::DefaultGraph => 0i64,
        GraphName::NamedNode(nn) => dictionary::encode(nn.as_str(), dictionary::KIND_IRI),
    };

    // Fetch the URL.
    let response = ureq::get(url)
        .call()
        .map_err(|e| format!("HTTP fetch error for {url}: {e}"))?;

    let content_type = response.content_type().to_ascii_lowercase();

    let body = response
        .into_string()
        .map_err(|e| format!("HTTP body read error for {url}: {e}"))?;

    // Detect format from Content-Type or URL extension.
    let is_turtle = content_type.contains("turtle")
        || content_type.contains("trig")
        || url.ends_with(".ttl")
        || url.ends_with(".trig");
    let is_xml = content_type.contains("rdf+xml") || url.ends_with(".rdf") || url.ends_with(".owl");

    if is_xml {
        Ok(crate::bulk_load::load_rdfxml_into_graph(&body, g_id))
    } else if is_turtle {
        Ok(crate::bulk_load::load_turtle_into_graph(&body, g_id))
    } else {
        // Default to N-Triples.
        Ok(crate::bulk_load::load_ntriples_into_graph(&body, g_id))
    }
}

// ─── SPARQL CLEAR ────────────────────────────────────────────────────────────

fn execute_clear(target: &spargebra::algebra::GraphTarget) -> Result<i64, String> {
    match target {
        spargebra::algebra::GraphTarget::NamedNode(nn) => {
            let g_id = dictionary::encode(nn.as_str(), dictionary::KIND_IRI);
            Ok(storage::clear_graph_by_id(g_id))
        }
        spargebra::algebra::GraphTarget::DefaultGraph => Ok(storage::clear_graph_by_id(0)),
        spargebra::algebra::GraphTarget::NamedGraphs => {
            let mut total = 0i64;
            for g_id in storage::all_graph_ids() {
                if g_id != 0 {
                    total += storage::clear_graph_by_id(g_id);
                }
            }
            Ok(total)
        }
        spargebra::algebra::GraphTarget::AllGraphs => {
            let mut total = 0i64;
            for g_id in storage::all_graph_ids() {
                total += storage::clear_graph_by_id(g_id);
            }
            Ok(total)
        }
    }
}

// ─── SPARQL DROP ─────────────────────────────────────────────────────────────

fn execute_drop(target: &spargebra::algebra::GraphTarget) -> Result<i64, String> {
    match target {
        spargebra::algebra::GraphTarget::NamedNode(nn) => Ok(storage::drop_graph(nn.as_str())),
        spargebra::algebra::GraphTarget::DefaultGraph => Ok(storage::clear_graph_by_id(0)),
        spargebra::algebra::GraphTarget::NamedGraphs => {
            let mut total = 0i64;
            for g_id in storage::all_graph_ids() {
                if g_id != 0 {
                    total += storage::clear_graph_by_id(g_id);
                }
            }
            Ok(total)
        }
        spargebra::algebra::GraphTarget::AllGraphs => {
            let mut total = 0i64;
            for g_id in storage::all_graph_ids() {
                total += storage::clear_graph_by_id(g_id);
            }
            Ok(total)
        }
    }
}

/// Encode a `NamedOrBlankNode` subject into a dictionary ID.
fn encode_named_or_blank(node: &NamedOrBlankNode) -> i64 {
    match node {
        NamedOrBlankNode::NamedNode(nn) => dictionary::encode(nn.as_str(), dictionary::KIND_IRI),
        NamedOrBlankNode::BlankNode(bn) => {
            // Use a load-generation-scoped encoding for blank nodes.
            let scoped = format!("{}:{}", storage::current_load_generation(), bn.as_str());
            dictionary::encode(&scoped, dictionary::KIND_BLANK)
        }
    }
}

/// Encode a `Term` (IRI, blank node, or literal) from an INSERT DATA quad.
fn encode_term_value(term: &Term) -> i64 {
    match term {
        Term::NamedNode(nn) => dictionary::encode(nn.as_str(), dictionary::KIND_IRI),
        Term::BlankNode(bn) => {
            let scoped = format!("{}:{}", storage::current_load_generation(), bn.as_str());
            dictionary::encode(&scoped, dictionary::KIND_BLANK)
        }
        Term::Literal(lit) => {
            let lang = lit.language();
            let value = lit.value();
            let dt = lit.datatype().as_str();
            if let Some(l) = lang {
                dictionary::encode_lang_literal(value, l)
            } else if dt == "http://www.w3.org/2001/XMLSchema#string"
                || dt == "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
            {
                dictionary::encode(value, dictionary::KIND_LITERAL)
            } else {
                dictionary::encode_typed_literal(value, dt)
            }
        }
        Term::Triple(t) => {
            let s_id = encode_named_or_blank(&t.subject);
            let p_id = dictionary::encode(t.predicate.as_str(), dictionary::KIND_IRI);
            let o_id = encode_term_value(&t.object);
            dictionary::encode_quoted_triple(s_id, p_id, o_id)
        }
    }
}

/// Look up a `GroundTerm` (IRI or literal) in the dictionary without inserting.
/// Returns `None` if the term has never been stored.
fn lookup_ground_term_value(term: &spargebra::term::GroundTerm) -> Option<i64> {
    match term {
        spargebra::term::GroundTerm::NamedNode(nn) => dictionary::lookup_iri(nn.as_str()),
        spargebra::term::GroundTerm::Literal(lit) => {
            let lang = lit.language();
            let value = lit.value();
            let dt = lit.datatype().as_str();
            if let Some(l) = lang {
                let canonical = format!("\"{}\"@{}", value, l);
                dictionary::lookup(&canonical, dictionary::KIND_LANG_LITERAL)
            } else if dt == "http://www.w3.org/2001/XMLSchema#string"
                || dt == "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
            {
                dictionary::lookup(value, dictionary::KIND_LITERAL)
            } else {
                // For inline-encodable types, build the inline ID directly.
                // For dictionary types, look up via canonical form.
                let inline_id = match dt {
                    "http://www.w3.org/2001/XMLSchema#integer"
                    | "http://www.w3.org/2001/XMLSchema#long"
                    | "http://www.w3.org/2001/XMLSchema#int" => {
                        dictionary::inline::try_encode_integer(value)
                    }
                    "http://www.w3.org/2001/XMLSchema#boolean" => {
                        dictionary::inline::try_encode_boolean(value)
                    }
                    "http://www.w3.org/2001/XMLSchema#dateTime" => {
                        dictionary::inline::try_encode_datetime(value)
                    }
                    "http://www.w3.org/2001/XMLSchema#date" => {
                        dictionary::inline::try_encode_date(value)
                    }
                    _ => None,
                };
                if let Some(id) = inline_id {
                    return Some(id);
                }
                let canonical = format!("\"{}\"^^<{}>", value, dt);
                dictionary::lookup(&canonical, dictionary::KIND_TYPED_LITERAL)
            }
        }
        spargebra::term::GroundTerm::Triple(t) => {
            let s_id = dictionary::lookup_iri(t.subject.as_str())?;
            let p_id = dictionary::lookup_iri(t.predicate.as_str())?;
            let o_id = lookup_ground_term_value(&t.object)?;
            dictionary::lookup_quoted_triple(s_id, p_id, o_id)
        }
    }
}

// ─── Plan cache monitoring (v0.13.0) ─────────────────────────────────────────

/// Return SPARQL plan cache statistics as JSONB.
///
/// Returns: `{"hits": N, "misses": N, "size": N, "capacity": N, "hit_rate": 0.xx}`
pub fn plan_cache_stats() -> pgrx::JsonB {
    let (hits, misses, size, cap) = plan_cache::stats();
    let total = hits + misses;
    let hit_rate = if total > 0 {
        hits as f64 / total as f64
    } else {
        0.0_f64
    };
    let mut obj = serde_json::Map::new();
    obj.insert(
        "hits".to_owned(),
        serde_json::Value::Number(serde_json::Number::from(hits)),
    );
    obj.insert(
        "misses".to_owned(),
        serde_json::Value::Number(serde_json::Number::from(misses)),
    );
    obj.insert(
        "size".to_owned(),
        serde_json::Value::Number(serde_json::Number::from(size as u64)),
    );
    obj.insert(
        "capacity".to_owned(),
        serde_json::Value::Number(serde_json::Number::from(cap as u64)),
    );
    // hit_rate as a JSON number with limited precision.
    let hit_rate_rounded = (hit_rate * 10000.0).round() / 10000.0;
    if let Some(n) = serde_json::Number::from_f64(hit_rate_rounded) {
        obj.insert("hit_rate".to_owned(), serde_json::Value::Number(n));
    } else {
        obj.insert(
            "hit_rate".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(0)),
        );
    }
    pgrx::JsonB(serde_json::Value::Object(obj))
}

/// Evict all cached SPARQL plans and reset hit/miss counters.
pub fn plan_cache_reset() {
    plan_cache::reset();
}
