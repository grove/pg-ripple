//! SPARQL DESCRIBE execution (M15-13, v0.96.0).
//! Moved from execute/mod.rs lines 440-565.

use pgrx::prelude::*;
use serde_json::{Map, Value as Json};
use spargebra::SparqlParser;

use super::super::sqlgen;
use crate::dictionary;
use crate::storage;

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
///
/// SC13-04 (v0.86.0): the `pg_ripple.describe_form` GUC (values: `cbd`,
/// `scbd`, `symmetric`) overrides `pg_ripple.describe_strategy` when set.
/// `symmetric` is treated as an alias for `scbd`.
pub(crate) fn sparql_describe(query_text: &str, strategy: &str) -> Vec<pgrx::JsonB> {
    // SC13-04 (v0.86.0): resolve effective strategy from describe_form GUC or fallback.
    let describe_form_raw = crate::gucs::sparql::DESCRIBE_FORM.get();
    let effective_strategy: String = if let Some(form) = describe_form_raw {
        let s = form.to_str().unwrap_or("cbd");
        match s {
            "symmetric" => "scbd".to_owned(),
            other => other.to_owned(),
        }
    } else {
        strategy.to_owned()
    };
    let strategy = effective_strategy.as_str();

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
pub(crate) fn describe_cbd(subject_id: i64, symmetric: bool) -> Vec<(i64, i64, i64)> {
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
