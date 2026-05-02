//! SPARQL query planning: algebra → SQL translation with plan cache.
//!
//! Provides `prepare_select` and `prepare_construct` which take SPARQL text,
//! check complexity, translate to SQL, and populate the plan cache.

use spargebra::SparqlParser;

use super::parse::check_query_complexity;
use super::plan_cache;
use super::sqlgen;
use crate::dictionary;

// ─── CONSTRUCT template types ─────────────────────────────────────────────────

/// One slot in a CONSTRUCT template triple: either a constant encoded ID
/// or a reference to a WHERE-clause variable by index.
#[derive(Clone)]
pub(crate) enum TemplateSlot {
    Constant(i64),
    Var(usize),
}

/// A CONSTRUCT template: one entry per template triple.
pub(crate) type ConstructTemplate = Vec<(TemplateSlot, TemplateSlot, TemplateSlot)>;

// ─── Plan preparation ─────────────────────────────────────────────────────────

/// Parse the query, optimize, translate to SQL, and cache the result.
/// Returns `(sql, variables, raw_numeric_vars, raw_text_vars, raw_iri_vars, raw_double_vars, wcoj_preamble)`.
#[allow(clippy::type_complexity)]
pub(crate) fn prepare_select(
    query_text: &str,
) -> (
    String,
    Vec<String>,
    std::collections::HashSet<String>,
    std::collections::HashSet<String>,
    std::collections::HashSet<String>,
    std::collections::HashSet<String>,
    bool,
) {
    // P13-01 (v0.84.0): parse first, then check the cache using the canonical
    // form — eliminates the double-parse that occurred when cache_key() re-parsed
    // the query text after prepare_select() had already parsed it.
    let query = SparqlParser::new()
        .parse_query(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {}", e));
    let canonical = format!("{query}");

    if let Some(cached) = plan_cache::get_canonical(&canonical) {
        return cached;
    }

    let base_iri: Option<String> = query.base_iri().map(|b| b.as_str().to_owned());
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

    // v0.51.0: reject over-limit queries before any SQL translation (PT440).
    check_query_complexity(&pattern);

    let trans = sqlgen::translate_select(&pattern, base_iri.as_deref());
    let entry = (
        trans.sql,
        trans.variables,
        trans.raw_numeric_vars,
        trans.raw_text_vars,
        trans.raw_iri_vars,
        trans.raw_double_vars,
        trans.wcoj_preamble,
    );
    // Skip plan cache for queries that contain SERVICE clauses — remote results
    // are baked into the generated SQL as VALUES literals; caching would return
    // stale data from a previous execution.
    if !canonical.to_ascii_uppercase().contains("SERVICE") {
        plan_cache::put_canonical(&canonical, entry.clone());
    }
    entry
}

/// Prepare a SPARQL CONSTRUCT query for cursor-based streaming (STREAM-01).
///
/// Returns:
/// - `sql`: the WHERE-clause SQL whose columns are the bound variable IDs.
/// - `variables`: the variable names (same order as SQL result columns).
/// - `template`: the CONSTRUCT template expressed as (TemplateSlot, TemplateSlot,
///   TemplateSlot) triples.
pub(crate) fn prepare_construct(query_text: &str) -> (String, Vec<String>, ConstructTemplate) {
    let query = SparqlParser::new()
        .parse_query(query_text)
        .unwrap_or_else(|e| pgrx::error!("SPARQL parse error: {}", e));

    let (template, pattern) = match query {
        spargebra::Query::Construct {
            template, pattern, ..
        } => (template, pattern),
        _ => pgrx::error!("prepare_construct() requires a CONSTRUCT query"),
    };

    check_query_complexity(&pattern);

    let trans = sqlgen::translate_select(&pattern, None);
    let sql = trans.sql;
    let variables = trans.variables;

    // Pre-encode constant IRIs/literals in the template to i64 once.
    let ct: ConstructTemplate = template
        .iter()
        .map(|triple| {
            let s_slot = match &triple.subject {
                spargebra::term::TermPattern::NamedNode(nn) => {
                    TemplateSlot::Constant(dictionary::encode(nn.as_str(), dictionary::KIND_IRI))
                }
                spargebra::term::TermPattern::Variable(v) => {
                    let idx = variables
                        .iter()
                        .position(|var| var == v.as_str())
                        .unwrap_or(usize::MAX);
                    TemplateSlot::Var(idx)
                }
                _ => TemplateSlot::Var(usize::MAX), // blank node or unsupported → skip
            };
            let p_slot = match &triple.predicate {
                spargebra::term::NamedNodePattern::NamedNode(nn) => {
                    TemplateSlot::Constant(dictionary::encode(nn.as_str(), dictionary::KIND_IRI))
                }
                spargebra::term::NamedNodePattern::Variable(v) => {
                    let idx = variables
                        .iter()
                        .position(|var| var == v.as_str())
                        .unwrap_or(usize::MAX);
                    TemplateSlot::Var(idx)
                }
            };
            let o_slot = match &triple.object {
                spargebra::term::TermPattern::NamedNode(nn) => {
                    TemplateSlot::Constant(dictionary::encode(nn.as_str(), dictionary::KIND_IRI))
                }
                spargebra::term::TermPattern::Literal(lit) => {
                    let lang = lit.language();
                    let dt = lit.datatype().as_str();
                    let id = if let Some(l) = lang {
                        dictionary::encode_lang_literal(lit.value(), l)
                    } else {
                        dictionary::encode_typed_literal(lit.value(), dt)
                    };
                    TemplateSlot::Constant(id)
                }
                spargebra::term::TermPattern::Variable(v) => {
                    let idx = variables
                        .iter()
                        .position(|var| var == v.as_str())
                        .unwrap_or(usize::MAX);
                    TemplateSlot::Var(idx)
                }
                _ => TemplateSlot::Var(usize::MAX), // blank node, RDF-star → skip
            };
            (s_slot, p_slot, o_slot)
        })
        .collect();

    (sql, variables, ct)
}

/// Apply a CONSTRUCT template to a row of variable bindings, returning
/// `(s_id, p_id, o_id)` for each template triple that is fully bound.
///
/// `row_vals` must be indexed by the same variable order as `template` was built from.
pub(crate) fn apply_construct_template(
    template: &ConstructTemplate,
    row_vals: &[Option<i64>],
) -> Vec<(i64, i64, i64)> {
    let resolve = |slot: &TemplateSlot| -> Option<i64> {
        match slot {
            TemplateSlot::Constant(id) => Some(*id),
            TemplateSlot::Var(idx) => {
                if *idx == usize::MAX {
                    return None;
                }
                row_vals.get(*idx).copied().flatten()
            }
        }
    };

    template
        .iter()
        .filter_map(|(s_slot, p_slot, o_slot)| {
            let s = resolve(s_slot)?;
            let p = resolve(p_slot)?;
            let o = resolve(o_slot)?;
            Some((s, p, o))
        })
        .collect()
}
