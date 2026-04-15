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
mod property_path;
mod sqlgen;

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

// ─── SPARQL Update (INSERT DATA / DELETE DATA) ────────────────────────────────

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
                    if let (Some(s), Some(p), Some(o)) = (s_id, p_id, o_id) {
                        if g_id >= 0 {
                            affected += storage::delete_triple_by_ids(s, p, o, g_id);
                        }
                    }
                }
            }
            other => {
                pgrx::warning!(
                    "SPARQL Update operation not supported in v0.5.1 (INSERT DATA / DELETE DATA only): {:?}",
                    other
                );
            }
        }
    }

    affected
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
