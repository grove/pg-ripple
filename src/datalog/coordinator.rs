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
//! The implementation delegates to [`super::run_inference_seminaive`] and the
//! `parallel` sub-module.  Future refactors may inline the implementation here.

use crate::datalog::parallel::ParallelAnalysis;

/// Analyse the rule groups for a rule set and return the parallelism
/// statistics used by `infer_with_stats()`.
///
/// Delegates to [`super::parallel::partition_into_parallel_groups`].
#[allow(dead_code)]
pub fn analyze_groups(rules: &[super::Rule]) -> ParallelAnalysis {
    let parallel_workers = crate::DATALOG_PARALLEL_WORKERS.get();
    super::parallel::partition_into_parallel_groups(rules, parallel_workers)
}

/// Run full semi-naive inference, returning derived triples, iteration count,
/// per-stratum SQL fragments, parallel group count, and max group size.
///
/// This is the primary entry point for `infer_with_stats()`.
#[allow(dead_code)]
pub fn run_with_stats(rule_set_name: &str) -> (i64, i32, Vec<String>, usize, usize) {
    super::run_inference_seminaive_full(rule_set_name)
}
