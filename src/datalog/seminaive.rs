//! Semi-naive evaluator for Datalog inference (v0.53.0 module split).
//!
//! This module exposes the semi-naive fixpoint evaluation functions extracted
//! from the monolithic `datalog/mod.rs` for improved navigability.
//!
//! # Algorithm
//!
//! Semi-naive evaluation avoids recomputing previously derived facts on each
//! iteration by tracking a *delta* (new facts from the previous round) and
//! substituting the delta table for the full VP table for one body atom per
//! SQL variant.  This gives convergence proportional to the longest derivation
//! chain rather than to the total relation size.
//!
//! The implementation lives in the parent module (`super`) and is re-exported
//! here for structural clarity.  Future refactors may inline the implementation.

/// Execute semi-naive inference and return `(total_triples_derived, iterations)`.
///
/// Delegates to [`super::run_inference_seminaive`].
#[allow(dead_code)]
pub fn run(rule_set_name: &str) -> (i64, i32) {
    super::run_inference_seminaive(rule_set_name)
}

/// Execute semi-naive inference and return full statistics.
///
/// Delegates to [`super::run_inference_seminaive_full`].
#[allow(dead_code)]
pub fn run_full(rule_set_name: &str) -> (i64, i32, Vec<String>, usize, usize) {
    super::run_inference_seminaive_full(rule_set_name)
}
