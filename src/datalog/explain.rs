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
            None => unreachable!("already checked head.is_none() above"),
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
        "last_run_stats": last_run_stats
    });

    pgrx::JsonB(result)
}
