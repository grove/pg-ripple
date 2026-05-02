//! Delta derivation — compile SPARQL CONSTRUCT to INSERT SQL and run full/delta recompute.

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

use super::scheduler::collect_source_graphs;

// ─── CONSTRUCT SQL compilation ────────────────────────────────────────────────

/// Parse a SPARQL CONSTRUCT query and generate INSERT + provenance SQL plans.
///
/// Returns `(insert_plans, source_graphs)` where each plan is
/// `(pred_id, Option<plain_insert_sql>, prov_sql)`:
/// - promoted VP: `(pred_id, None, combined_returning_cte)` — one CTE does INSERT + prov
/// - vp_rare:     `(pred_id, Some(insert_sql), exists_prov_sql)` — two steps; prov uses
///   an EXISTS join so that shared-target rules both record provenance even when the
///   second rule's INSERT is a no-op due to ON CONFLICT (CWB-FIX-04 / CWB-10).
#[allow(clippy::type_complexity)]
pub(super) fn compile_construct_to_inserts(
    query_text: &str,
    target_graph_id: i64,
) -> Result<(Vec<(i64, Option<String>, String)>, Vec<String>), String> {
    use spargebra::SparqlParser;
    use spargebra::term::{NamedNodePattern, TermPattern};

    let query = SparqlParser::new()
        .parse_query(query_text)
        .map_err(|e| format!("SPARQL parse error: {e}"))?;

    let (template, pattern) = match query {
        spargebra::Query::Construct {
            template, pattern, ..
        } => (template, pattern),
        _ => return Err("sparql must be a CONSTRUCT query".to_owned()),
    };

    if template.is_empty() {
        return Err("CONSTRUCT template is empty".to_owned());
    }

    // Collect source graphs.
    let source_graphs: Vec<String> = collect_source_graphs(&pattern).into_iter().collect();

    let trans = crate::sparql::sqlgen::translate_select(&pattern, None);
    let variables = trans.variables;
    let var_set: std::collections::HashSet<&str> = variables.iter().map(|s| s.as_str()).collect();

    // Validate template: no blank nodes; all variables bound.
    for triple in &template {
        match &triple.subject {
            TermPattern::BlankNode(_) => {
                return Err(
                    "CONSTRUCT template contains a blank node subject; replace blank \
                     nodes with IRIs or use skolemisation before registering as a rule"
                        .to_owned(),
                );
            }
            TermPattern::Variable(v) if !var_set.contains(v.as_str()) => {
                return Err(format!(
                    "variable ?{} appears in the CONSTRUCT template but is not bound \
                     by the WHERE pattern",
                    v.as_str()
                ));
            }
            _ => {}
        }
        match &triple.predicate {
            NamedNodePattern::Variable(v) if !var_set.contains(v.as_str()) => {
                return Err(format!(
                    "variable ?{} appears in the CONSTRUCT template but is not bound \
                     by the WHERE pattern",
                    v.as_str()
                ));
            }
            _ => {}
        }
        match &triple.object {
            TermPattern::BlankNode(_) => {
                return Err(
                    "CONSTRUCT template contains a blank node object; replace blank \
                     nodes with IRIs or use skolemisation before registering as a rule"
                        .to_owned(),
                );
            }
            TermPattern::Variable(v) if !var_set.contains(v.as_str()) => {
                return Err(format!(
                    "variable ?{} appears in the CONSTRUCT template but is not bound \
                     by the WHERE pattern",
                    v.as_str()
                ));
            }
            _ => {}
        }
    }

    // Remap column aliases and build INSERT SQL for each template triple.
    let clean_sql = remap_cols(&trans.sql, &variables);
    let inner_alias = "_cr_inner_";
    let var_col = |v: &str| -> String { format!("{inner_alias}.{v}") };

    let mut results: Vec<(i64, Option<String>, String)> = Vec::new();

    for triple in &template {
        // Resolve subject expression.
        let s_expr = match &triple.subject {
            TermPattern::NamedNode(nn) => {
                let id = crate::dictionary::encode(nn.as_str(), crate::dictionary::KIND_IRI);
                format!("{id}::bigint")
            }
            TermPattern::Variable(v) => var_col(v.as_str()),
            _ => return Err(
                "internal: blank node or RDF-star subject reached template encoder — please report"
                    .to_owned(),
            ),
        };

        // Resolve predicate expression and extract pred_id.
        let (p_expr, pred_id) = match &triple.predicate {
            NamedNodePattern::NamedNode(nn) => {
                let id = crate::dictionary::encode(nn.as_str(), crate::dictionary::KIND_IRI);
                (format!("{id}::bigint"), id)
            }
            NamedNodePattern::Variable(v) => (var_col(v.as_str()), 0_i64),
        };

        // Resolve object expression.
        let o_expr = match &triple.object {
            TermPattern::NamedNode(nn) => {
                let id = crate::dictionary::encode(nn.as_str(), crate::dictionary::KIND_IRI);
                format!("{id}::bigint")
            }
            TermPattern::Literal(lit) => {
                let id = if let Some(lang) = lit.language() {
                    crate::dictionary::encode_lang_literal(lit.value(), lang)
                } else {
                    crate::dictionary::encode_typed_literal(lit.value(), lit.datatype().as_str())
                };
                format!("{id}::bigint")
            }
            TermPattern::Variable(v) => var_col(v.as_str()),
            TermPattern::BlankNode(_) => {
                return Err(
                    "internal: blank node object reached template encoder — please report"
                        .to_owned(),
                );
            }
            TermPattern::Triple(_) => {
                return Err("CONSTRUCT template contains an RDF-star quoted triple; \
                     RDF-star template terms are not supported in writeback rules"
                    .to_owned());
            }
        };

        // Choose the target table.
        let has_vp_table = pred_id != 0 && {
            Spi::get_one_with_args::<bool>(
                "SELECT EXISTS(SELECT 1 FROM _pg_ripple.predicates \
                  WHERE id = $1 AND table_oid IS NOT NULL)",
                &[DatumWithOid::from(pred_id)],
            )
            .unwrap_or(Some(false))
            .unwrap_or(false)
        };

        let (plain_insert, prov_sql) = if has_vp_table {
            // Promoted VP table: one combined RETURNING CTE handles INSERT + provenance.
            // The INSERT may be a no-op for the promoted-VP shared-target case, but
            // promoted predicates are rarely shared across rules in practice.
            let combined_cte = format!(
                "WITH inserted AS ( \
                     INSERT INTO _pg_ripple.vp_{pred_id} (s, o, g, source) \
                     SELECT DISTINCT {s_expr}, {o_expr}, {target_graph_id}::bigint, 1 \
                     FROM ({clean_sql}) AS {inner_alias} \
                     WHERE ({s_expr}) IS NOT NULL AND ({o_expr}) IS NOT NULL \
                     ON CONFLICT DO NOTHING \
                     RETURNING s, o, g \
                 ) \
                 INSERT INTO _pg_ripple.construct_rule_triples (rule_name, pred_id, s, o, g) \
                 SELECT $1, {pred_id}, s, o, g FROM inserted \
                 ON CONFLICT DO NOTHING"
            );
            (None, combined_cte)
        } else {
            // vp_rare path (rare pred or variable pred).
            // Step 1: plain INSERT (no $1 param needed — all values are inlined).
            let p_col = if pred_id != 0 {
                format!("{pred_id}::bigint")
            } else {
                p_expr.clone()
            };
            let insert_sql = format!(
                "INSERT INTO _pg_ripple.vp_rare (p, s, o, g, source) \
                 SELECT DISTINCT {p_col}, {s_expr}, {o_expr}, {target_graph_id}::bigint, 1 \
                 FROM ({clean_sql}) AS {inner_alias} \
                 WHERE ({s_expr}) IS NOT NULL AND ({o_expr}) IS NOT NULL \
                 ON CONFLICT DO NOTHING"
            );
            // Step 2: EXISTS-based provenance INSERT (CWB-FIX-04 / CWB-10).
            // Joins with vp_rare to record provenance for triples that now exist,
            // even if this rule's INSERT was a no-op (shared-target race case).
            let (prov_pred_col, prov_p_filter) = if pred_id != 0 {
                (format!("{pred_id}::bigint"), format!("vr.p = {pred_id}"))
            } else {
                (format!("({p_expr})"), format!("vr.p = ({p_expr})"))
            };
            let p_is_not_null = if pred_id == 0 {
                format!(" AND ({p_expr}) IS NOT NULL")
            } else {
                String::new()
            };
            let prov_sql = format!(
                "INSERT INTO _pg_ripple.construct_rule_triples (rule_name, pred_id, s, o, g) \
                 SELECT DISTINCT $1, {prov_pred_col}, ({s_expr}), ({o_expr}), \
                        {target_graph_id}::bigint \
                 FROM ({clean_sql}) AS {inner_alias} \
                 WHERE ({s_expr}) IS NOT NULL AND ({o_expr}) IS NOT NULL{p_is_not_null} \
                   AND EXISTS ( \
                       SELECT 1 FROM _pg_ripple.vp_rare vr \
                       WHERE {prov_p_filter} \
                         AND vr.s = ({s_expr}) \
                         AND vr.o = ({o_expr}) \
                         AND vr.g = {target_graph_id} \
                         AND vr.source = 1 \
                   ) \
                 ON CONFLICT DO NOTHING"
            );
            (Some(insert_sql), prov_sql)
        };

        results.push((pred_id, plain_insert, prov_sql));
    }

    Ok((results, source_graphs))
}

// ─── SQL helpers ─────────────────────────────────────────────────────────────

/// Remap `_v_{var}` column aliases in a SQL string to plain `{var}`.
pub(super) fn remap_cols(sql: &str, variables: &[String]) -> String {
    let mut result = sql.to_owned();
    for v in variables {
        let old = format!("AS _v_{v}");
        let new = format!("AS {v}");
        result = result.replace(&old, &new);
    }
    result
}

pub(super) fn run_full_recompute(
    rule_name: &str,
    insert_sqls: &[(i64, Option<String>, String)],
    _target_graph_id: i64,
) -> i64 {
    for (_pred_id, plain_insert, prov_sql) in insert_sqls {
        if let Some(plain) = plain_insert {
            // vp_rare step 1: plain INSERT (no rule_name param).
            Spi::run(plain)
                .unwrap_or_else(|e| pgrx::warning!("run_full_recompute insert (vp_rare): {e}"));
        }
        // Step 2 (or combined for promoted VP): provenance SQL with $1 = rule_name.
        Spi::run_with_args(prov_sql, &[DatumWithOid::from(rule_name)])
            .unwrap_or_else(|e| pgrx::warning!("run_full_recompute prov: {e}"));
    }

    // Return exact count of provenance rows for this rule.
    let final_count = Spi::get_one_with_args::<i64>(
        "SELECT COUNT(*)::bigint FROM _pg_ripple.construct_rule_triples \
         WHERE rule_name = $1",
        &[DatumWithOid::from(rule_name)],
    )
    .unwrap_or(Some(0))
    .unwrap_or(0);

    Spi::run_with_args(
        "UPDATE _pg_ripple.construct_rules \
         SET derived_triple_count = $2 WHERE name = $1",
        &[
            DatumWithOid::from(rule_name),
            DatumWithOid::from(final_count),
        ],
    )
    .unwrap_or_else(|e| pgrx::warning!("run_full_recompute: update derived_triple_count: {e}"));

    // CONF-CWB-01b: if cwb_confidence_propagation GUC names a SPARQL CONSTRUCT rule
    // that tracks pg:sourceTrust, propagate the minimum source-trust confidence to
    // all newly derived triples. This is a best-effort sweep — exact propagation
    // requires a SPARQL query that joins confidence to the construct results.
    if crate::PROV_CONFIDENCE.get() {
        Spi::run(
            "INSERT INTO _pg_ripple.confidence (statement_id, confidence, model) \
             SELECT crt.triple_id, 1.0, 'cwb' \
             FROM _pg_ripple.construct_rule_triples crt \
             WHERE NOT EXISTS ( \
               SELECT 1 FROM _pg_ripple.confidence c \
               WHERE c.statement_id = crt.triple_id AND c.model = 'cwb' \
             ) \
             ON CONFLICT DO NOTHING",
        )
        .unwrap_or(());
    }

    final_count
}

/// Record a successful incremental run in health counters (CWB-FIX-07).
pub(super) fn record_run_success(rule_name: &str, derived_count: i64) {
    Spi::run_with_args(
        "UPDATE _pg_ripple.construct_rules \
         SET successful_run_count  = successful_run_count + 1, \
             last_incremental_run  = now(), \
             last_error            = NULL, \
             derived_triple_count  = $2 \
         WHERE name = $1",
        &[
            DatumWithOid::from(rule_name),
            DatumWithOid::from(derived_count),
        ],
    )
    .unwrap_or_else(|e| pgrx::warning!("record_run_success: {e}"));
}

/// Record a failed incremental run in health counters (CWB-FIX-07).
///
/// CWB-FIX-STAB-1: retraction/derivation failures are correctness-critical —
/// a warning is emitted so the operator can detect and investigate.
pub(super) fn record_run_failure(rule_name: &str, error: &str) {
    pgrx::warning!(
        "construct rule '{}' maintenance failed: {}",
        rule_name,
        error
    );
    Spi::run_with_args(
        "UPDATE _pg_ripple.construct_rules \
         SET failed_run_count  = failed_run_count + 1, \
             last_error        = $2 \
         WHERE name = $1",
        &[DatumWithOid::from(rule_name), DatumWithOid::from(error)],
    )
    .unwrap_or_else(|e| pgrx::warning!("record_run_failure: {e}"));
}
