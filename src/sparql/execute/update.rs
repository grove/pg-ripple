//! SPARQL UPDATE execution (M15-13, v0.96.0).
//! Moved from execute/mod.rs lines 566-1392.

use pgrx::prelude::*;
use spargebra::GraphUpdateOperation;
use spargebra::SparqlParser;
use spargebra::term::{GraphName, NamedOrBlankNode, Term};
use std::collections::HashMap;

use super::super::sqlgen;
use crate::dictionary;
use crate::storage;

// ─── SPARQL Update ────────────────────────────────────────────────────────────

/// Execute a SPARQL Update statement.  Returns the total number of affected
/// triples (inserted + deleted).
pub(crate) fn sparql_update(query_text: &str) -> i64 {
    // v0.48.0: pre-process SPARQL Update operations not yet supported by spargebra:
    // ADD, COPY, and MOVE.  These are parsed from the raw query string before
    // handing off to spargebra.
    // M15-12 (v0.95.0): pipe ADD/COPY/MOVE through the same post-processing
    // path as other SPARQL Update operations (mutation journal flush, audit log).
    let query_trimmed = query_text.trim();
    if let Some(n) = try_execute_add_copy_move(query_trimmed) {
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
        // rules fire after ADD/COPY/MOVE operations — previously missed because
        // these operations returned before the flush at the end of sparql_update().
        crate::storage::mutation_journal::flush();
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
    let op = if upper.starts_with("ADD") {
        "ADD"
    } else if upper.starts_with("COPY") {
        "COPY"
    } else if upper.starts_with("MOVE") {
        "MOVE"
    } else {
        return None;
    };

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
