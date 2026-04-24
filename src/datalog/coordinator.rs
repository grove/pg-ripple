//! Parallel-stratum coordinator for Datalog inference (v0.53.0 module split).
//!
//! This module provides the high-level orchestration of Datalog stratum
//! evaluation, combining the semi-naive evaluator with the parallel group
//! analysis from [`super::parallel`].
//!
//! # Concurrency model
//!
//! True cross-session parallelism is not used (PostgreSQL temporary delta
//! tables are not visible across backends).  Instead, this coordinator
//! implements *within-session* group-aware scheduling: independent rule groups
//! within a stratum are identified by `parallel::partition_into_parallel_groups()`
//! and interleaved for maximum throughput per fixpoint round.
//!
//! # Savepoint safety (v0.55.0)
//!
//! Each independent `ParallelGroup`'s compiled SQL batch is wrapped in a
//! PostgreSQL SAVEPOINT via `parallel::execute_with_savepoint()`.  A failed
//! group's delta tables are left empty for this iteration (retried next round),
//! while successful groups commit their delta results immediately, improving
//! throughput and isolation.

use crate::datalog::parallel::{ParallelAnalysis, execute_with_savepoint};

/// Analyse the rule groups for a rule set and return the parallelism
/// statistics used by `infer_with_stats()`.
///
/// Delegates to [`super::parallel::partition_into_parallel_groups`].
#[allow(dead_code)] // module API — called from external tools and future inference loop refactor
pub fn analyze_groups(rules: &[super::Rule]) -> ParallelAnalysis {
    let parallel_workers = crate::DATALOG_PARALLEL_WORKERS.get();
    super::parallel::partition_into_parallel_groups(rules, parallel_workers)
}

/// Run full semi-naive inference, returning derived triples, iteration count,
/// per-stratum SQL fragments, parallel group count, and max group size.
///
/// This is the primary entry point for `infer_with_stats()`.
#[allow(dead_code)] // module API — called from external tools and future inference loop refactor
pub fn run_with_stats(rule_set_name: &str) -> (i64, i32, Vec<String>, usize, usize) {
    super::run_inference_seminaive_full(rule_set_name)
}

/// Execute a single stratum's SQL batch using savepoints for isolation.
///
/// `stratum_index` and `worker_id` are combined to generate a unique savepoint
/// name.  Returns `true` if all statements succeeded, `false` if any failed
/// (in which case the savepoint was rolled back).
#[allow(dead_code)] // module API — wired into the inference loop in the next refactor
pub fn execute_stratum_batch(stmts: &[String], stratum_index: usize, worker_id: usize) -> bool {
    let savepoint_name = format!("dl_stratum_{stratum_index}_w{worker_id}");
    execute_with_savepoint(stmts, &savepoint_name)
}
