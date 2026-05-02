//! JSONB explain output for Datalog rule sets (v0.40.0).
//!
//! `explain_datalog(rule_set_name)` returns a JSONB document with keys:
//!
//! - `"strata"` — per-stratum dependency graph
//! - `"rules"` — rewritten rule text (after magic sets / demand transformation)
//! - `"sql_per_rule"` — compiled SQL for each rule
//! - `"last_run_stats"` — per-iteration delta row counts from the last inference run

use pgrx::prelude::*;

use crate::datalog::Term;
use crate::datalog::compiler;
use crate::datalog::parser::parse_rules;
use crate::datalog::stratify;

/// Produce a JSONB explain document for a named rule set.
pub fn explain_datalog(rule_set_name: &str) -> pgrx::JsonB {
    // Load rules from catalog.
    // Rules are stored individually in _pg_ripple.rules; concatenate them.
    let exists: bool = Spi::connect(|client| {
        client
            .select(
                "SELECT 1 FROM _pg_ripple.rule_sets WHERE name = $1",
                None,
                &[pgrx::datum::DatumWithOid::from(rule_set_name)],
            )
            .ok()
            .and_then(|mut rows| rows.next())
            .is_some()
    });

    if !exists {
        return pgrx::JsonB(serde_json::json!({
            "strata": [],
            "rules": [],
            "sql_per_rule": [],
            "last_run_stats": [],
            "error": format!("rule set '{}' not found", rule_set_name)
        }));
    }

    let rules_text: String = Spi::connect(|client| {
        let rows = client
            .select(
                "SELECT rule_text FROM _pg_ripple.rules WHERE rule_set = $1 ORDER BY id",
                None,
                &[pgrx::datum::DatumWithOid::from(rule_set_name)],
            )
            .unwrap_or_else(|e| pgrx::error!("explain_datalog SPI error: {e}"));
        rows.filter_map(|row| row.get::<String>(1).ok().flatten())
            .collect::<Vec<_>>()
            .join("\n")
    });

    // Parse rules.
    let rule_set = match parse_rules(&rules_text, rule_set_name) {
        Ok(rs) => rs,
        Err(e) => {
            return pgrx::JsonB(serde_json::json!({
                "error": format!("parse error: {e}")
            }));
        }
    };

    // Compute strata.
    let strata_result = stratify::stratify(&rule_set.rules);
    let stratified = match strata_result {
        Ok(sp) => sp,
        Err(e) => {
            return pgrx::JsonB(serde_json::json!({
                "error": format!("stratification error: {e}")
            }));
        }
    };

    // Build strata JSON.
    let strata_json: Vec<serde_json::Value> = stratified
        .strata
        .iter()
        .enumerate()
        .map(|(i, stratum)| {
            let pred_ids: Vec<i64> = stratum.derived_predicates.clone();
            serde_json::json!({
                "stratum": i,
                "derived_predicate_ids": pred_ids,
                "rule_count": stratum.rules.len(),
                "is_recursive": stratum.is_recursive
            })
        })
        .collect();

    // Compile SQL per rule.
    let mut rules_json: Vec<serde_json::Value> = Vec::new();
    let mut sql_per_rule: Vec<serde_json::Value> = Vec::new();

    for rule in &rule_set.rules {
        let rule_str = rule.rule_text.clone();
        rules_json.push(serde_json::Value::String(rule_str));

        if rule.head.is_none() {
            // Constraint rule — skip SQL compilation.
            sql_per_rule.push(serde_json::json!({
                "kind": "constraint",
                "ok": true
            }));
            continue;
        }

        let head = match rule.head.as_ref() {
            Some(h) => h,
            None => {
                pgrx::error!(
                    "internal: explain_datalog rule head is None after is_none() check — please report"
                )
            }
        };
        let head_pred_id = match &head.p {
            Term::Const(id) => *id,
            _ => {
                sql_per_rule.push(serde_json::json!({
                    "error": "variable predicate in head",
                    "ok": false
                }));
                continue;
            }
        };

        let target = format!("_pg_ripple.vp_{head_pred_id}");
        match compiler::compile_single_rule_to(rule, &target) {
            Ok(sql) => sql_per_rule.push(serde_json::json!({
                "head_pred_id": head_pred_id,
                "sql": sql,
                "ok": true
            })),
            Err(e) => sql_per_rule.push(serde_json::json!({
                "head_pred_id": head_pred_id,
                "error": e,
                "ok": false
            })),
        }
    }

    // Collect last run stats from the inference_stats table (if it exists).
    let last_run_stats: serde_json::Value = Spi::connect(|client| {
        // First check whether the table exists to avoid aborting the transaction.
        let table_exists: bool = client
            .select(
                "SELECT 1 FROM information_schema.tables \
                 WHERE table_schema = '_pg_ripple' AND table_name = 'inference_stats'",
                None,
                &[],
            )
            .ok()
            .and_then(|mut rows| rows.next())
            .is_some();

        if !table_exists {
            return serde_json::Value::Array(vec![]);
        }

        let rows = client.select(
            "SELECT iteration, delta_rows, completed_at::text \
             FROM _pg_ripple.inference_stats \
             WHERE rule_set = $1 \
             ORDER BY completed_at DESC \
             LIMIT 20",
            None,
            &[pgrx::datum::DatumWithOid::from(rule_set_name)],
        );
        match rows {
            Ok(rows) => {
                let entries: Vec<serde_json::Value> = rows
                    .map(|row| {
                        let iteration: i64 = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                        let delta_rows: i64 = row.get::<i64>(2).ok().flatten().unwrap_or(0);
                        let completed_at: String =
                            row.get::<String>(3).ok().flatten().unwrap_or_default();
                        serde_json::json!({
                            "iteration": iteration,
                            "delta_rows": delta_rows,
                            "completed_at": completed_at
                        })
                    })
                    .collect();
                serde_json::Value::Array(entries)
            }
            Err(_) => serde_json::Value::Array(vec![]),
        }
    });

    let result = serde_json::json!({
        "strata": strata_json,
        "rules": rules_json,
        "sql_per_rule": sql_per_rule,
        "last_run_stats": last_run_stats,
        "confidence": {
            "enabled": crate::PROBABILISTIC_DATALOG.get()
        }
    });

    pgrx::JsonB(result)
}

// ── v0.61.0: explain_inference ────────────────────────────────────────────────

/// Provenance row returned by `explain_inference_impl`.
pub type InferenceRow = (i32, String, Vec<i64>, pgrx::JsonB);

/// Walk the rule-firing provenance chain for a given inferred triple.
///
/// Returns one row per derivation step:
/// - `depth`         — tree depth (0 = the queried triple)
/// - `rule_id`       — Datalog rule identifier
/// - `source_sids`   — statement IDs of base facts that triggered the rule
/// - `child_triples` — JSONB array of child derivation nodes
///
/// When the triple is explicit (`source = 0`) or provenance logging is
/// disabled, an empty vector is returned.
pub fn explain_inference_impl(s: &str, p: &str, o: &str, g: Option<&str>) -> Vec<InferenceRow> {
    use pgrx::datum::DatumWithOid;

    // Encode the triple terms.
    let s_id = crate::dictionary::encode(s, crate::dictionary::KIND_IRI);
    let p_id = crate::dictionary::encode(p, crate::dictionary::KIND_IRI);
    // Object might be an IRI or a literal — try IRI first, then literal kind.
    let o_id = crate::dictionary::lookup_iri(o)
        .or_else(|| crate::dictionary::lookup(o, crate::dictionary::KIND_LITERAL))
        .unwrap_or_else(|| crate::dictionary::encode(o, crate::dictionary::KIND_IRI));
    let g_id: i64 = match g {
        Some(graph_iri) => crate::dictionary::encode(graph_iri, crate::dictionary::KIND_IRI),
        None => 0,
    };

    // Check if a rule_firing_log table exists (introduced in v0.61.0).
    let log_exists: bool = Spi::get_one::<bool>(
        "SELECT EXISTS (SELECT 1 FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = '_pg_ripple' AND c.relname = 'rule_firing_log')",
    )
    .unwrap_or(None)
    .unwrap_or(false);

    // Find the statement ID for this triple in any VP table.
    let sid: Option<i64> = Spi::get_one_with_args::<i64>(
        "SELECT i FROM _pg_ripple.vp_rare WHERE s = $1 AND o = $3 AND g = $4 AND source = 1 \
         UNION ALL \
         SELECT i FROM _pg_ripple.vp_rare WHERE s = $1 AND o = $3 AND g = $4 AND source = 1 \
         LIMIT 1",
        &[
            DatumWithOid::from(s_id),
            DatumWithOid::from(p_id),
            DatumWithOid::from(o_id),
            DatumWithOid::from(g_id),
        ],
    )
    .unwrap_or(None);

    // Build derivation chain from rule_firing_log if available.
    let mut rows: Vec<InferenceRow> = Vec::new();

    if log_exists && let Some(statement_id) = sid {
        collect_derivation_chain(statement_id, 0, &mut rows);
    }

    // If we found nothing or the log doesn't exist, provide a synthetic root node
    // showing at least that inference is tracked for this triple.
    if rows.is_empty() {
        // Emit one row indicating the inference was found (or not).
        let found = sid.is_some();
        let rule_id = if found {
            "inferred (provenance log unavailable)".to_owned()
        } else {
            "not found as inferred triple".to_owned()
        };
        rows.push((
            0_i32,
            rule_id,
            vec![],
            pgrx::JsonB(serde_json::json!({
                "note": "provenance chain not available; ensure pg_ripple.inference_mode != 'off'",
                "s_id": s_id,
                "p_id": p_id,
                "o_id": o_id,
                "g_id": g_id
            })),
        ));
    }

    rows
}

/// Recursively walk the rule_firing_log for a given statement ID.
fn collect_derivation_chain(sid: i64, depth: i32, rows: &mut Vec<InferenceRow>) {
    use pgrx::datum::DatumWithOid;

    if depth > 20 {
        // Guard against infinite recursion in cyclic derivation graphs.
        return;
    }

    // Query the rule firing log for the rule that produced this statement.
    let result: Option<(String, serde_json::Value)> = Spi::connect(|client| {
        let r = client.select(
            "SELECT rule_id, source_sids \
             FROM _pg_ripple.rule_firing_log \
             WHERE produced_sid = $1 \
             ORDER BY fired_at DESC \
             LIMIT 1",
            None,
            &[DatumWithOid::from(sid)],
        );
        match r {
            Ok(mut rows) => {
                if let Some(row) = rows.next() {
                    let rule_id = row.get::<String>(1).ok().flatten().unwrap_or_default();
                    let sids_json: serde_json::Value = row
                        .get::<pgrx::JsonB>(2)
                        .ok()
                        .flatten()
                        .map(|jb| jb.0)
                        .unwrap_or(serde_json::json!([]));
                    Some((rule_id, sids_json))
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    });

    let (rule_id, sids_json) = match result {
        Some(r) => r,
        None => {
            rows.push((
                depth,
                "base fact".to_owned(),
                vec![sid],
                pgrx::JsonB(serde_json::json!([])),
            ));
            return;
        }
    };

    // Parse source_sids from JSON array.
    let source_sids: Vec<i64> = match &sids_json {
        serde_json::Value::Array(arr) => arr.iter().filter_map(|v| v.as_i64()).collect(),
        _ => vec![],
    };

    rows.push((
        depth,
        rule_id.clone(),
        source_sids.clone(),
        pgrx::JsonB(serde_json::json!({
            "rule": rule_id,
            "source_sids": source_sids
        })),
    ));

    // Recurse into each source SID.
    for &src_sid in &source_sids {
        collect_derivation_chain(src_sid, depth + 1, rows);
    }
}
