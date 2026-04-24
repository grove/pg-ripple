//! Parallel-stratum coordinator for Datalog inference.
//!
//! Orchestrates stratum evaluation, combining the semi-naive evaluator with
//! parallel group analysis and aggregation-aware inference.
//!
//! # Savepoint safety (v0.55.0)
//!
//! Each `ParallelGroup`'s SQL batch is wrapped in a PostgreSQL SAVEPOINT via
//! `parallel::execute_with_savepoint()`.  A failed group's delta tables are
//! left empty for this iteration (retried next round).

use pgrx::prelude::*;
use crate::datalog::parallel::{ParallelAnalysis, execute_with_savepoint};
use super::{Rule, Term, BodyLiteral, compile_aggregate_rule, check_aggregation_stratification, parse_rules};

/// Analyse rule groups and return parallelism statistics.
#[allow(dead_code)]
pub fn analyze_groups(rules: &[Rule]) -> ParallelAnalysis {
    let parallel_workers = crate::DATALOG_PARALLEL_WORKERS.get();
    super::parallel::partition_into_parallel_groups(rules, parallel_workers)
}

/// Run full semi-naive inference with statistics.
/// This is the primary entry point for `infer_with_stats()`.
#[allow(dead_code)]
pub fn run_with_stats(rule_set_name: &str) -> (i64, i32, Vec<String>, usize, usize) {
    super::seminaive::run_inference_seminaive_full(rule_set_name)
}

/// Execute a single stratum's SQL batch using savepoints for isolation.
#[allow(dead_code)]
pub fn execute_stratum_batch(stmts: &[String], stratum_index: usize, worker_id: usize) -> bool {
    let savepoint_name = format!("dl_stratum_{stratum_index}_w{worker_id}");
    execute_with_savepoint(stmts, &savepoint_name)
}

// ─── Aggregation-aware inference coordination (v0.30.0) ──────────────────────

/// Run inference for a rule set that may contain aggregate body literals.
///
/// Returns `(total_derived, aggregate_derived, iteration_count)`.
pub fn run_inference_agg(rule_set_name: &str) -> (i64, i64, i32) {
    super::ensure_catalog();

    let rule_rows: Vec<String> = {
        let sql = "SELECT rule_text \
                   FROM _pg_ripple.rules \
                   WHERE rule_set = $1 AND active = true \
                   ORDER BY stratum, id";
        Spi::connect(|client| {
            client
                .select(sql, None, &[pgrx::datum::DatumWithOid::from(rule_set_name)])
                .unwrap_or_else(|e| pgrx::error!("rule select SPI error: {e}"))
                .map(|row| row.get::<String>(1).ok().flatten().unwrap_or_default())
                .collect::<Vec<_>>()
        })
    };

    if rule_rows.is_empty() {
        return (0, 0, 0);
    }

    let mut all_rules: Vec<Rule> = Vec::new();
    for rule_text in &rule_rows {
        match parse_rules(rule_text, rule_set_name) {
            Ok(rs) => all_rules.extend(rs.rules),
            Err(e) => pgrx::warning!("infer_agg: rule parse error: {e}"),
        }
    }

    if all_rules.is_empty() {
        return (0, 0, 0);
    }

    if let Err(e) = check_aggregation_stratification(&all_rules) {
        pgrx::warning!(
            "infer_agg: aggregation stratification violation (PT510): {}; \
             aggregate rules will be skipped",
            e
        );
        let non_agg_rules: Vec<Rule> = all_rules
            .iter()
            .filter(|r| !r.body.iter().any(|lit| matches!(lit, BodyLiteral::Aggregate(_))))
            .cloned()
            .collect();
        let (derived, iters) = super::seminaive::run_seminaive_inner(&non_agg_rules, rule_set_name);
        return (derived, 0, iters);
    }

    let (agg_rules, non_agg_rules): (Vec<Rule>, Vec<Rule>) = all_rules.into_iter().partition(|r| {
        r.body.iter().any(|lit| matches!(lit, BodyLiteral::Aggregate(_)))
    });

    let (normal_derived, iterations) = if !non_agg_rules.is_empty() {
        super::seminaive::run_seminaive_inner(&non_agg_rules, rule_set_name)
    } else {
        (0, 0)
    };

    let mut agg_derived: i64 = 0;

    let cached_sqls = super::cache::lookup_agg(rule_set_name);
    let agg_sqls: Vec<String> = if let Some(sqls) = cached_sqls {
        sqls
    } else {
        let mut compiled = Vec::new();
        for rule in &agg_rules {
            let Some(head_atom) = &rule.head else { continue; };
            let head_pred = match &head_atom.p { Term::Const(id) => *id, _ => continue, };
            crate::storage::merge::ensure_htap_tables(head_pred);
            let target = format!("_pg_ripple.vp_{head_pred}_delta");
            match compile_aggregate_rule(rule, &target) {
                Ok(sql) => compiled.push(sql),
                Err(e) => pgrx::warning!("infer_agg: aggregate rule compile error: {e}"),
            }
        }
        super::cache::store_agg(rule_set_name, &compiled);
        compiled
    };

    for sql in &agg_sqls {
        match Spi::get_one::<i64>(&format!(
            "WITH ins AS ({sql} RETURNING 1) SELECT COUNT(*)::bigint FROM ins"
        )) {
            Ok(Some(n)) => agg_derived += n,
            Ok(None) => {}
            Err(e) => pgrx::warning!("infer_agg: aggregate SQL execution error: {e}"),
        }
    }

    (normal_derived + agg_derived, agg_derived, iterations)
}
