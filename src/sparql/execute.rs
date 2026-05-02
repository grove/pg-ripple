//! SPARQL SPI execution: SELECT, CONSTRUCT, DESCRIBE, UPDATE operations.
//!
//! All functions in this module call PostgreSQL SPI to run generated SQL and
//! map results back to RDF terms via the dictionary.

use std::collections::HashMap;

use pgrx::prelude::*;
use serde_json::{Map, Value as Json};
use spargebra::GraphUpdateOperation;
use spargebra::SparqlParser;
use spargebra::term::{GraphName, NamedOrBlankNode, Term};

use super::decode::batch_decode;
use super::plan_cache;
use super::sqlgen;
use crate::dictionary;
use crate::storage;

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
        let rows = client
            .select(sql, None, &[])
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

// ─── SPARQL CONSTRUCT ─────────────────────────────────────────────────────────

/// Execute a SPARQL CONSTRUCT query; returns raw `(s_id, p_id, o_id)` integer rows.
///
/// Used by the framing engine to obtain encoded triples that are then decoded
/// in a single batch SPI round-trip.
pub(crate) fn sparql_construct_rows(query_text: &str) -> Vec<(i64, i64, i64)> {
    let query = SparqlParser::new()
        .parse_query(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {}", e));

    let (template, pattern) = match query {
        spargebra::Query::Construct {
            template, pattern, ..
        } => (template, pattern),
        _ => pgrx::error!("sparql_construct_rows() requires a CONSTRUCT query"),
    };

    let trans = sqlgen::translate_select(&pattern, None);
    let (sql, variables) = (trans.sql, trans.variables);

    let mut raw_rows: Vec<Vec<Option<i64>>> = Vec::new();
    Spi::connect(|client| {
        let rows = client
            .select(&sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("SPARQL CONSTRUCT SPI error: {e}"));
        for row in rows {
            let mut row_vals: Vec<Option<i64>> = Vec::with_capacity(variables.len());
            for i in 1..=(variables.len() as i64) {
                row_vals.push(row.get::<i64>(i as _).ok().flatten());
            }
            raw_rows.push(row_vals);
        }
    });

    let var_set: std::collections::HashSet<&str> = variables.iter().map(|s| s.as_str()).collect();
    let resolve_idx = |var: &str| variables.iter().position(|v| v == var);

    let mut result = Vec::new();
    for row_vals in &raw_rows {
        for triple in &template {
            let s_id = match &triple.subject {
                spargebra::term::TermPattern::NamedNode(nn) => Some(crate::dictionary::encode(
                    nn.as_str(),
                    crate::dictionary::KIND_IRI,
                )),
                spargebra::term::TermPattern::Variable(v) if var_set.contains(v.as_str()) => {
                    resolve_idx(v.as_str()).and_then(|i| row_vals.get(i).copied().flatten())
                }
                _ => None,
            };
            let p_id = match &triple.predicate {
                spargebra::term::NamedNodePattern::NamedNode(nn) => Some(
                    crate::dictionary::encode(nn.as_str(), crate::dictionary::KIND_IRI),
                ),
                spargebra::term::NamedNodePattern::Variable(v) if var_set.contains(v.as_str()) => {
                    resolve_idx(v.as_str()).and_then(|i| row_vals.get(i).copied().flatten())
                }
                _ => None,
            };
            let o_id = match &triple.object {
                spargebra::term::TermPattern::NamedNode(nn) => Some(crate::dictionary::encode(
                    nn.as_str(),
                    crate::dictionary::KIND_IRI,
                )),
                spargebra::term::TermPattern::Variable(v) if var_set.contains(v.as_str()) => {
                    resolve_idx(v.as_str()).and_then(|i| row_vals.get(i).copied().flatten())
                }
                spargebra::term::TermPattern::Triple(inner) => {
                    // v0.24.0: ground quoted-triple in CONSTRUCT template.
                    let ts_str = match &inner.subject {
                        spargebra::term::TermPattern::NamedNode(nn) => Some(nn.as_str()),
                        _ => None,
                    };
                    let tp_id = match &inner.predicate {
                        spargebra::term::NamedNodePattern::NamedNode(nn) => Some(
                            crate::dictionary::encode(nn.as_str(), crate::dictionary::KIND_IRI),
                        ),
                        _ => None,
                    };
                    let to_id_opt = match &inner.object {
                        spargebra::term::TermPattern::NamedNode(nn) => Some(
                            crate::dictionary::encode(nn.as_str(), crate::dictionary::KIND_IRI),
                        ),
                        spargebra::term::TermPattern::Variable(v)
                            if var_set.contains(v.as_str()) =>
                        {
                            resolve_idx(v.as_str()).and_then(|i| row_vals.get(i).copied().flatten())
                        }
                        _ => None,
                    };
                    match (ts_str, tp_id, to_id_opt) {
                        (Some(ts_str), Some(tp_id), Some(to_id)) => {
                            let ts_id =
                                crate::dictionary::encode(ts_str, crate::dictionary::KIND_IRI);
                            Some(crate::dictionary::encode_quoted_triple(ts_id, tp_id, to_id))
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            if let (Some(s), Some(p), Some(o)) = (s_id, p_id, o_id) {
                result.push((s, p, o));
            }
        }
    }
    result
}

/// Execute a SPARQL CONSTRUCT query; returns one JSONB row per constructed triple.
///
/// Each row is `{"s": "<iri>", "p": "<iri>", "o": "..."}`.
pub(crate) fn sparql_construct(query_text: &str) -> Vec<pgrx::JsonB> {
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
    let trans = sqlgen::translate_select(&pattern, None);
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
                spargebra::term::TermPattern::Triple(inner) => {
                    // v0.51.0: emit ground quoted triples as N-Triples-star notation
                    // `<< s p o >>` (S2-6 / N5-5).  Only ground (all-IRI) inner
                    // triples are supported; variable-containing inner triples are
                    // handled via the outer variable bindings.
                    let ts_val = match &inner.subject {
                        spargebra::term::TermPattern::NamedNode(nn) => {
                            Some(format!("<{}>", nn.as_str()))
                        }
                        _ => None,
                    };
                    let tp_val = match &inner.predicate {
                        spargebra::term::NamedNodePattern::NamedNode(nn) => {
                            Some(format!("<{}>", nn.as_str()))
                        }
                        _ => None,
                    };
                    let to_val = match &inner.object {
                        spargebra::term::TermPattern::NamedNode(nn) => {
                            Some(format!("<{}>", nn.as_str()))
                        }
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
                        _ => None,
                    };
                    match (ts_val, tp_val, to_val) {
                        (Some(s), Some(p), Some(o)) => Some(format!("<< {s} {p} {o} >>")),
                        _ => None,
                    }
                }
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
pub(crate) fn sparql_describe(query_text: &str, strategy: &str) -> Vec<pgrx::JsonB> {
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

    let trans = sqlgen::translate_select(&pattern, None);
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
///
/// # C13-11 (v0.85.0)
/// Recursion depth is capped by `pg_ripple.describe_max_depth` GUC (default 16).
/// Raises a `PT_DEPTH_LIMIT` error when exceeded to prevent runaway traversal on
/// cyclic or very deep graphs.
fn describe_cbd(subject_id: i64, symmetric: bool) -> Vec<(i64, i64, i64)> {
    let max_depth = crate::gucs::storage::DESCRIBE_MAX_DEPTH.get() as usize;
    let mut triples: Vec<(i64, i64, i64)> = Vec::new();
    let mut visited: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut queue: Vec<(i64, usize)> = vec![(subject_id, 0)]; // (id, depth)

    while let Some((s_id, depth)) = queue.pop() {
        if !visited.insert(s_id) {
            continue;
        }
        if depth > max_depth {
            pgrx::error!(
                "PT_DEPTH_LIMIT: DESCRIBE CBD traversal exceeded describe_max_depth={max_depth}; \
                 set pg_ripple.describe_max_depth to a higher value or use DESCRIBE SIMPLE strategy"
            );
        }
        // Outgoing arcs from s_id across all predicates.
        let outgoing = storage::triples_for_subject(s_id);
        for (p_id, o_id) in outgoing {
            triples.push((s_id, p_id, o_id));
            // Recurse on blank node objects.
            if dictionary::is_blank_node(o_id) && !visited.contains(&o_id) {
                queue.push((o_id, depth + 1));
            }
        }
        // Symmetric CBD: also fetch incoming arcs.
        if symmetric {
            let incoming = storage::triples_for_object(s_id);
            for (s2_id, p_id) in incoming {
                triples.push((s2_id, p_id, s_id));
                if dictionary::is_blank_node(s2_id) && !visited.contains(&s2_id) {
                    queue.push((s2_id, depth + 1));
                }
            }
        }
    }

    triples
}

// ─── SPARQL Update ────────────────────────────────────────────────────────────

/// Execute a SPARQL Update statement.  Returns the total number of affected
/// triples (inserted + deleted).
pub(crate) fn sparql_update(query_text: &str) -> i64 {
    // v0.48.0: pre-process SPARQL Update operations not yet supported by spargebra:
    // ADD, COPY, and MOVE.  These are parsed from the raw query string before
    // handing off to spargebra.
    let query_trimmed = query_text.trim();
    if let Some(n) = try_execute_add_copy_move(query_trimmed) {
        return n;
    }

    let update = SparqlParser::new()
        .parse_update(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL Update parse error: {}", e));

    let mut affected: i64 = 0;
    for op in &update.operations {
        // Advance blank-node scope for each operation so that `_:b` in two
        // separate INSERT WHERE operations produces DISTINCT blank nodes
        // (per SPARQL Update section 3.1.3 — "INSERTing the same bnode
        // with two INSERT WHERE statement within one request is NOT the same bnode").
        storage::next_load_generation();
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
                using,
                pattern,
            } => {
                affected += execute_delete_insert(delete, insert, using.as_ref(), pattern);
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

    // H-3 (v0.56.0): Record operation in the SPARQL audit log when enabled.
    if crate::gucs::observability::AUDIT_LOG_ENABLED.get() {
        let op_name = detect_update_operation_type(query_text);
        let _ = Spi::run_with_args(
            "INSERT INTO _pg_ripple.audit_log (operation, query) VALUES ($1, $2)",
            &[
                pgrx::datum::DatumWithOid::from(op_name),
                pgrx::datum::DatumWithOid::from(query_text),
            ],
        );
    }

    // FLUSH-02-01 (v0.80.0): flush the mutation journal so CONSTRUCT writeback
    // rules fire after every SPARQL UPDATE statement.  This must run after all
    // graph mutations have been committed to the VP tables.
    crate::storage::mutation_journal::flush();

    affected
}

// ─── DELETE/INSERT WHERE ──────────────────────────────────────────────────────

/// Wrap a WHERE clause pattern in the graph context defined by a USING/WITH dataset.
///
/// `USING <g>` / `WITH <g>` means the bare triple patterns in the WHERE clause
/// should be evaluated against graph `<g>` rather than all graphs.
/// Multiple `USING <g>` clauses produce a UNION of GRAPH patterns.
fn wrap_pattern_for_dataset(
    dataset: &spargebra::algebra::QueryDataset,
    pattern: &spargebra::algebra::GraphPattern,
) -> spargebra::algebra::GraphPattern {
    use spargebra::algebra::GraphPattern;
    use spargebra::term::NamedNodePattern;

    if dataset.default.is_empty() {
        return pattern.clone();
    }

    dataset
        .default
        .iter()
        .map(|g| GraphPattern::Graph {
            name: NamedNodePattern::NamedNode(g.clone()),
            inner: Box::new(pattern.clone()),
        })
        .reduce(|l, r| GraphPattern::Union {
            left: Box::new(l),
            right: Box::new(r),
        })
        .unwrap_or_else(|| pattern.clone())
}

/// Execute a `DELETE/INSERT WHERE` pattern-based update.
/// Returns the total number of triples deleted + inserted.
fn execute_delete_insert(
    delete_templates: &[spargebra::term::GroundQuadPattern],
    insert_templates: &[spargebra::term::QuadPattern],
    using: Option<&spargebra::algebra::QueryDataset>,
    pattern: &spargebra::algebra::GraphPattern,
) -> i64 {
    // 1. Restrict the WHERE pattern to the USING/WITH dataset if specified.
    let wrapped: spargebra::algebra::GraphPattern;
    let pattern: &spargebra::algebra::GraphPattern = if let Some(dataset) = using {
        wrapped = wrap_pattern_for_dataset(dataset, pattern);
        &wrapped
    } else {
        pattern
    };

    // 2. Translate WHERE clause to SQL via the existing SELECT engine.
    let trans = sqlgen::translate_select(pattern, None);
    let sql = trans.sql;
    let variables = trans.variables;
    let raw_numeric_vars = trans.raw_numeric_vars;

    // 2. Execute the WHERE query and collect bound result rows.
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

    // Post-process: raw_numeric_vars (COUNT, SUM, etc.) return raw SQL integers,
    // not dictionary IDs.  Encode them as inline xsd:integer IDs so they can be
    // used as triple term IDs in INSERT templates.
    let raw_num_indices: Vec<usize> = variables
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            if raw_numeric_vars.contains(v) {
                Some(i)
            } else {
                None
            }
        })
        .collect();
    if !raw_num_indices.is_empty() {
        for row in &mut raw_rows {
            for &idx in &raw_num_indices {
                if let Some(Some(raw_val)) = row.get(idx).copied() {
                    let encoded = dictionary::inline::try_encode_integer(&raw_val.to_string())
                        .unwrap_or_else(|| {
                            dictionary::encode_typed_literal(
                                &raw_val.to_string(),
                                "http://www.w3.org/2001/XMLSchema#integer",
                            )
                        });
                    if let Some(slot) = row.get_mut(idx) {
                        *slot = Some(encoded);
                    }
                }
            }
        }
    }

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

// ─── Term resolution helpers ──────────────────────────────────────────────────

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
        spargebra::term::GroundTermPattern::Triple(inner) => {
            // v0.24.0: support quoted-triple patterns in DELETE templates.
            let s_id = resolve_ground_term(&inner.subject, row, var_index)?;
            let p_id = match &inner.predicate {
                spargebra::term::NamedNodePattern::NamedNode(nn) => {
                    dictionary::encode(nn.as_str(), dictionary::KIND_IRI)
                }
                spargebra::term::NamedNodePattern::Variable(v) => {
                    let idx = var_index.get(v.as_str())?;
                    (*row.get(*idx)?)?
                }
            };
            let o_id = resolve_ground_term(&inner.object, row, var_index)?;
            dictionary::lookup_quoted_triple(s_id, p_id, o_id)
        }
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
        spargebra::term::TermPattern::Triple(inner) => {
            // v0.24.0: support quoted-triple patterns in INSERT/CONSTRUCT templates.
            let s_id = resolve_term_pattern(&inner.subject, row, var_index)?;
            let p_id = match &inner.predicate {
                spargebra::term::NamedNodePattern::NamedNode(nn) => {
                    dictionary::encode(nn.as_str(), dictionary::KIND_IRI)
                }
                spargebra::term::NamedNodePattern::Variable(v) => {
                    let idx = var_index.get(v.as_str())?;
                    (*row.get(*idx)?)?
                }
            };
            let o_id = resolve_term_pattern(&inner.object, row, var_index)?;
            Some(dictionary::encode_quoted_triple(s_id, p_id, o_id))
        }
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
fn execute_load(url: &str, destination: &GraphName) -> Result<i64, String> {
    let g_id: i64 = match destination {
        GraphName::DefaultGraph => 0i64,
        GraphName::NamedNode(nn) => dictionary::encode(nn.as_str(), dictionary::KIND_IRI),
    };

    let response = ureq::get(url)
        .call()
        .map_err(|e| format!("HTTP fetch error for {url}: {e}"))?;

    let content_type = response.content_type().to_ascii_lowercase();

    let body = response
        .into_string()
        .map_err(|e| format!("HTTP body read error for {url}: {e}"))?;

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
        Ok(crate::bulk_load::load_ntriples_into_graph(&body, g_id))
    }
}

// ─── SPARQL CLEAR ────────────────────────────────────────────────────────────

/// Execute a SPARQL CLEAR operation.
///
/// # C13-04 (v0.85.0) — mutation journal flush obligation
/// The caller (`sparql_update`) is responsible for calling
/// `crate::storage::mutation_journal::flush()` after all graph update operations
/// have completed.  `execute_clear` records deletes implicitly through
/// `storage::clear_graph_by_id` but does NOT flush the journal itself.
/// This design allows batching multiple CLEAR/DROP operations in a single
/// UPDATE request before one flush at the end.
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

/// Execute a SPARQL DROP operation.
///
/// # C13-04 (v0.85.0) — mutation journal flush obligation
/// See `execute_clear`: the caller (`sparql_update`) flushes the mutation journal
/// after all operations complete.  `execute_drop` does NOT flush itself.
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

// ─── SPARQL ADD/COPY/MOVE ─────────────────────────────────────────────────────

/// Copy all triples from graph `src_g_id` into graph `dst_g_id`.
fn execute_add_by_ids(src_g_id: i64, dst_g_id: i64) -> Result<i64, String> {
    use pgrx::datum::DatumWithOid;

    let mut total = 0i64;

    let pred_ids: Vec<i64> = Spi::connect(|c| {
        let tup = c
            .select("SELECT id FROM _pg_ripple.predicates", None, &[])
            .unwrap_or_else(|e| pgrx::error!("ADD: predicates SPI error: {e}"));
        let mut out = Vec::new();
        for row in tup {
            if let Ok(Some(id)) = row.get::<i64>(1) {
                out.push(id);
            }
        }
        out
    });

    for pred_id in &pred_ids {
        let table_oid: Option<i64> = Spi::get_one_with_args::<i64>(
            "SELECT table_oid::bigint FROM _pg_ripple.predicates WHERE id = $1",
            &[DatumWithOid::from(*pred_id)],
        )
        .unwrap_or(None);

        if table_oid.is_some() {
            let delta = format!("_pg_ripple.vp_{pred_id}_delta");
            let insert_sql = format!(
                "WITH ins AS ( \
                     INSERT INTO {delta} (s, o, g) \
                     SELECT s, o, {dst_g_id} FROM _pg_ripple.vp_{pred_id} WHERE g = {src_g_id} \
                     ON CONFLICT (s, o, g) DO NOTHING \
                     RETURNING 1 \
                 ) SELECT count(*)::bigint FROM ins"
            );
            let n: i64 = Spi::get_one::<i64>(&insert_sql)
                .unwrap_or_else(|e| pgrx::error!("ADD VP insert error: {e}"))
                .unwrap_or(0);
            if n > 0 {
                Spi::run_with_args(
                    "UPDATE _pg_ripple.predicates \
                     SET triple_count = triple_count + $2 WHERE id = $1",
                    &[DatumWithOid::from(*pred_id), DatumWithOid::from(n)],
                )
                .unwrap_or_else(|e| pgrx::error!("ADD triple_count update SPI error: {e}"));
            }
            total += n;
        } else {
            let insert_sql = format!(
                "WITH ins AS ( \
                     INSERT INTO _pg_ripple.vp_rare (p, s, o, g) \
                     SELECT {pred_id}, s, o, {dst_g_id} \
                     FROM _pg_ripple.vp_rare WHERE p = {pred_id} AND g = {src_g_id} \
                     ON CONFLICT (p, s, o, g) DO NOTHING \
                     RETURNING 1 \
                 ) SELECT count(*)::bigint FROM ins"
            );
            let n: i64 = Spi::get_one::<i64>(&insert_sql)
                .unwrap_or_else(|e| pgrx::error!("ADD vp_rare insert error: {e}"))
                .unwrap_or(0);
            total += n;
        }
    }

    Ok(total)
}

/// Pre-parser for SPARQL Update ADD/COPY/MOVE operations.
///
/// Returns `Some(n)` where n is the affected triple count if the query is one
/// of these operations, or `None` if spargebra should handle it.
fn try_execute_add_copy_move(query: &str) -> Option<i64> {
    let upper = query.to_uppercase();
    let op: &str;
    if upper.starts_with("ADD") {
        op = "ADD";
    } else if upper.starts_with("COPY") {
        op = "COPY";
    } else if upper.starts_with("MOVE") {
        op = "MOVE";
    } else {
        return None;
    }

    let rest = query[op.len()..].trim_start();
    let (silent, rest) = if rest.to_uppercase().starts_with("SILENT") {
        (true, rest[6..].trim_start())
    } else {
        (false, rest)
    };

    let (from_iri_opt, rest) = parse_graph_target_token(rest)?;
    let rest = rest.trim_start();
    if !rest.to_uppercase().starts_with("TO") {
        return None;
    }
    let rest = rest[2..].trim_start();
    let (to_iri_opt, _rest) = parse_graph_target_token(rest)?;

    let src_g_id: i64 = match &from_iri_opt {
        None => 0,
        Some(iri) => match dictionary::lookup_iri(iri) {
            Some(id) => id,
            None => return Some(0),
        },
    };
    let dst_g_id: i64 = match &to_iri_opt {
        None => 0,
        Some(iri) => dictionary::encode(iri, dictionary::KIND_IRI),
    };

    let result: i64 = match op {
        "ADD" => match execute_add_by_ids(src_g_id, dst_g_id) {
            Ok(n) => n,
            Err(e) => {
                if silent {
                    pgrx::warning!("SPARQL ADD failed (silent): {e}");
                    0
                } else {
                    pgrx::error!("SPARQL ADD error: {e}");
                }
            }
        },
        "COPY" => {
            let _ = storage::clear_graph_by_id(dst_g_id);
            match execute_add_by_ids(src_g_id, dst_g_id) {
                Ok(n) => n,
                Err(e) => {
                    if silent {
                        pgrx::warning!("SPARQL COPY failed (silent): {e}");
                        0
                    } else {
                        pgrx::error!("SPARQL COPY error: {e}");
                    }
                }
            }
        }
        "MOVE" => {
            let _ = storage::clear_graph_by_id(dst_g_id);
            let n = match execute_add_by_ids(src_g_id, dst_g_id) {
                Ok(n) => n,
                Err(e) => {
                    if silent {
                        pgrx::warning!("SPARQL MOVE (ADD phase) failed (silent): {e}");
                        0
                    } else {
                        pgrx::error!("SPARQL MOVE (ADD phase) error: {e}");
                    }
                }
            };
            let dropped = storage::clear_graph_by_id(src_g_id);
            n + dropped
        }
        _ => 0,
    };

    Some(result)
}

/// Parse a graph target token from a SPARQL Update ADD/COPY/MOVE string.
fn parse_graph_target_token(s: &str) -> Option<(Option<String>, &str)> {
    let s = s.trim_start();
    if s.to_uppercase().starts_with("DEFAULT") {
        Some((None, &s[7..]))
    } else if s.starts_with('<') {
        let end = s.find('>')?;
        let iri = s[1..end].to_owned();
        Some((Some(iri), &s[end + 1..]))
    } else {
        None
    }
}

// ─── Encoding helpers ─────────────────────────────────────────────────────────

/// Encode a `NamedOrBlankNode` subject into a dictionary ID.
pub(super) fn encode_named_or_blank(node: &NamedOrBlankNode) -> i64 {
    match node {
        NamedOrBlankNode::NamedNode(nn) => dictionary::encode(nn.as_str(), dictionary::KIND_IRI),
        NamedOrBlankNode::BlankNode(bn) => {
            let scoped = format!("{}:{}", storage::current_load_generation(), bn.as_str());
            dictionary::encode(&scoped, dictionary::KIND_BLANK)
        }
    }
}

/// Encode a `Term` (IRI, blank node, or literal) from an INSERT DATA quad.
pub(super) fn encode_term_value(term: &Term) -> i64 {
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

// ─── Audit log helper ─────────────────────────────────────────────────────────

/// Return a short operation-type label for a SPARQL Update query text.
fn detect_update_operation_type(query: &str) -> &'static str {
    let upper = query.trim_start().to_uppercase();
    if upper.starts_with("INSERT DATA") {
        "INSERT DATA"
    } else if upper.starts_with("DELETE DATA") {
        "DELETE DATA"
    } else if upper.starts_with("DELETE") {
        "DELETE/INSERT"
    } else if upper.starts_with("INSERT") {
        "INSERT WHERE"
    } else if upper.starts_with("CLEAR") {
        "CLEAR"
    } else if upper.starts_with("DROP") {
        "DROP"
    } else if upper.starts_with("COPY") {
        "COPY"
    } else if upper.starts_with("MOVE") {
        "MOVE"
    } else if upper.starts_with("ADD") {
        "ADD"
    } else if upper.starts_with("LOAD") {
        "LOAD"
    } else if upper.starts_with("CREATE") {
        "CREATE"
    } else {
        "UPDATE"
    }
}

// ─── explain_sparql ───────────────────────────────────────────────────────────

/// Explain a SPARQL query with flexible format options.
///
/// - `format = 'sql'`: return the generated SQL without executing it.
/// - `format = 'text'` (default): run `EXPLAIN (ANALYZE, FORMAT TEXT)`.
/// - `format = 'json'`: run `EXPLAIN (ANALYZE, FORMAT JSON)`.
/// - `format = 'sparql_algebra'`: return the spargebra algebra tree via `Debug`.
pub(crate) fn explain_sparql(query_text: &str, format: &str) -> String {
    use spargebra::Query;

    let query = SparqlParser::new()
        .parse_query(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {e}"));

    if format == "sparql_algebra" {
        return std::format!("{query:#?}");
    }

    let inner_sql = match &query {
        Query::Select { pattern, .. } => {
            let trans = sqlgen::translate_select(pattern, None);
            trans.sql
        }
        Query::Ask { pattern, .. } => sqlgen::translate_ask(pattern),
        Query::Construct { pattern, .. } => {
            let trans = sqlgen::translate_select(pattern, None);
            trans.sql
        }
        Query::Describe { .. } => {
            return std::format!("DESCRIBE query algebra:\n{query:#?}");
        }
    };

    if format == "sql" {
        return inner_sql;
    }

    let explain_format = if format == "json" { "JSON" } else { "TEXT" };
    let explain_sql = std::format!("EXPLAIN (ANALYZE, FORMAT {explain_format}) {inner_sql}");

    let plan: String = Spi::connect(|client| {
        let rows = client
            .select(&explain_sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("explain_sparql EXPLAIN SPI error: {e}"));
        let lines: Vec<String> = rows
            .filter_map(|row| row.get::<String>(1).ok().flatten())
            .collect();
        lines.join("\n")
    });

    std::format!("-- Generated SQL --\n{inner_sql}\n\n-- EXPLAIN ({explain_format}) --\n{plan}")
}

// ─── Plan cache monitoring ────────────────────────────────────────────────────

/// Return SPARQL plan cache statistics as JSONB.
pub(crate) fn plan_cache_stats() -> pgrx::JsonB {
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
pub(crate) fn plan_cache_reset() {
    plan_cache::reset();
}
