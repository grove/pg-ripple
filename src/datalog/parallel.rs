//! Parallel stratum evaluation for Datalog rules (v0.35.0).
//!
//! Within a single stratum, rules that derive *different* predicates and have no
//! shared body predicates derived in the same stratum are fully independent.
//! Their `INSERT … SELECT` statements touch distinct VP tables and cannot
//! interfere.  This module analyses the rule dependency graph to partition
//! a stratum into such independent groups and exposes the resulting statistics
//! for `infer_with_stats()`.
//!
//! # Concurrency model
//!
//! True cross-session parallelism (via `pgrx::BackgroundWorker`) is not used
//! here because PostgreSQL temporary tables — which back the semi-naive delta
//! tables — are not visible across backend connections.  Instead this module
//! implements *within-session* group-aware scheduling: independent groups are
//! executed in the optimal dependency order and the analysis results are exposed
//! via `infer_with_stats()` so operators can tune `datalog_parallel_workers`.
//!
//! When `pg_ripple.datalog_parallel_workers > 1` and the estimated total row
//! count for a stratum exceeds `pg_ripple.datalog_parallel_threshold`, the
//! inference loop interleaves rule execution across independent groups each
//! iteration, maximising the work done per round before the next fixpoint check.

use std::collections::{HashMap, HashSet};

use crate::datalog::{Atom, BodyLiteral, Rule, Term};

// ─── Parallel Group ───────────────────────────────────────────────────────────

/// A group of rules that can execute concurrently within a stratum.
///
/// All rules in the group derive *different* predicates and none references
/// a predicate derived by another rule in the same group.  Executing the group's
/// SQL statements concurrently (or sequentially within one pass) is safe: there
/// are no data-flow dependencies between them within this group.
#[derive(Debug, Clone)]
pub struct ParallelGroup {
    /// Rules belonging to this group.
    pub rules: Vec<Rule>,
    /// Head predicates derived by rules in this group.
    pub derived_predicates: Vec<i64>,
}

/// Statistics returned by `partition_into_parallel_groups`.
#[derive(Debug, Clone)]
pub struct ParallelAnalysis {
    /// The independent groups.  A single-element vec means no parallelism.
    /// Q13-05 (v0.85.0): accessed via `coordinator::analyze_groups()`; the field
    /// is part of the public API for future background-worker parallelism.  The
    /// `#[allow]` is retained because the field is accessed indirectly through the
    /// struct return value of public functions, not by direct field access in the crate.
    #[allow(dead_code)] // public API field; accessed via coordinator::analyze_groups()
    pub groups: Vec<ParallelGroup>,
    /// Number of independent groups (= `groups.len()`).
    pub parallel_groups: usize,
    /// `min(parallel_groups, datalog_parallel_workers)` — the effective worker
    /// count that would be used if background-worker parallelism were applied.
    pub max_concurrent: usize,
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Analyse rules within one stratum and partition them into maximally independent
/// parallel groups.
///
/// **Algorithm**
///
/// 1. Compute the *derived-predicate set* `D` for this stratum: the set of all
///    head predicates in the rule slice.
/// 2. For each rule assign it to its *head group* — rules with the same head
///    predicate must execute in the same group (they share a write target and
///    their delta tables must be merged consistently).
/// 3. Build a dependency graph among head groups: group A depends on group B if
///    any rule in A has a body atom whose predicate is in group B's derived set.
/// 4. Merge strongly-connected (or transitively dependent) groups.  After merging,
///    each remaining group is independent.
///
/// Returns a `ParallelAnalysis` with the partitioned groups and concurrency
/// statistics.
pub fn partition_into_parallel_groups(rules: &[Rule], parallel_workers: i32) -> ParallelAnalysis {
    if rules.is_empty() {
        return ParallelAnalysis {
            groups: vec![],
            parallel_groups: 0,
            max_concurrent: 0,
        };
    }

    // Step 1 & 2: collect head predicates and assign rules to head groups.
    let mut head_groups: HashMap<i64, Vec<Rule>> = HashMap::new();
    let mut var_pred_rules: Vec<Rule> = Vec::new(); // rules with variable head predicate

    for rule in rules {
        let head_pred = match rule.head.as_ref().and_then(head_pred_id) {
            Some(id) => id,
            None => {
                // Constraint rule (no head) or variable-predicate head — cannot
                // be parallelised; put it in the serial group.
                var_pred_rules.push(rule.clone());
                continue;
            }
        };
        head_groups.entry(head_pred).or_default().push(rule.clone());
    }

    // Collect all derived predicate IDs in this stratum.
    let derived_set: HashSet<i64> = head_groups.keys().copied().collect();

    // Step 3: build adjacency — group_id (head_pred) → set of group_ids it depends on.
    let mut depends_on: HashMap<i64, HashSet<i64>> = HashMap::new();
    for (&head_pred, rules_in_group) in &head_groups {
        let mut deps: HashSet<i64> = HashSet::new();
        for rule in rules_in_group {
            for body_pred in body_derived_preds(rule, &derived_set) {
                if body_pred != head_pred {
                    deps.insert(body_pred);
                }
            }
        }
        depends_on.insert(head_pred, deps);
    }

    // Step 4: compute connected components under the undirected version of the
    // dependency graph.  Groups in the same component must execute serially
    // (they share derived predicates in their bodies); groups in different
    // components are independent and can run in parallel.
    //
    // P13-06 (v0.85.0): pre-check the head-group dependency graph for directed
    // cycles BEFORE invoking the SCC partitioner.  A cycle in the directed graph
    // (A depends on B, B depends on A) means the two groups are mutually recursive
    // and must be merged.  Previously this was handled implicitly by the union-find,
    // but without the explicit check an unbounded recursion in the topological sort
    // could occur when the dependency graph is not a DAG.  We detect directed cycles
    // with a DFS and warn; the union-find merge ensures correctness.

    // Collect all predicate IDs upfront so they can be used by both the DFS
    // cycle-check and the union-find SCC computation below.
    let preds: Vec<i64> = head_groups.keys().copied().collect();

    let has_directed_cycle = {
        let mut visited: HashSet<i64> = HashSet::new();
        let mut on_stack: HashSet<i64> = HashSet::new();
        let mut found_cycle = false;

        fn dfs(
            node: i64,
            depends_on: &HashMap<i64, HashSet<i64>>,
            visited: &mut HashSet<i64>,
            on_stack: &mut HashSet<i64>,
            found_cycle: &mut bool,
        ) {
            if on_stack.contains(&node) {
                *found_cycle = true;
                return;
            }
            if visited.contains(&node) {
                return;
            }
            visited.insert(node);
            on_stack.insert(node);
            if let Some(deps) = depends_on.get(&node) {
                for &dep in deps {
                    dfs(dep, depends_on, visited, on_stack, found_cycle);
                    if *found_cycle {
                        return;
                    }
                }
            }
            on_stack.remove(&node);
        }

        for &pred in &preds {
            if !visited.contains(&pred) {
                dfs(
                    pred,
                    &depends_on,
                    &mut visited,
                    &mut on_stack,
                    &mut found_cycle,
                );
            }
            if found_cycle {
                break;
            }
        }
        found_cycle
    };
    if has_directed_cycle {
        // Directed cycles are expected for mutually-recursive rules (same stratum).
        // The union-find below merges them into one component, which is correct.
        pgrx::log!(
            "partition_into_parallel_groups: directed cycle detected in head-group \
             dependency graph; merging cyclic groups into a single serial group"
        );
    }

    let mut uf = UnionFind::new(&preds);

    for (&head_pred, deps) in &depends_on {
        for &dep in deps {
            uf.union(head_pred, dep);
        }
    }

    // Gather groups by their component root.
    let mut components: HashMap<i64, Vec<i64>> = HashMap::new();
    for &pred in &preds {
        let root = uf.find(pred);
        components.entry(root).or_default().push(pred);
    }

    // Build ParallelGroups from components.
    let mut groups: Vec<ParallelGroup> = components
        .into_values()
        .map(|pred_ids| {
            let mut rules_in_group: Vec<Rule> = Vec::new();
            for pred_id in &pred_ids {
                if let Some(rs) = head_groups.get(pred_id) {
                    rules_in_group.extend(rs.iter().cloned());
                }
            }
            ParallelGroup {
                rules: rules_in_group,
                derived_predicates: pred_ids,
            }
        })
        .collect();

    // Append var-pred and constraint rules to a dedicated serial group (last).
    if !var_pred_rules.is_empty() {
        groups.push(ParallelGroup {
            rules: var_pred_rules,
            derived_predicates: vec![],
        });
    }

    // DL-PAR-02 (v0.81.0): topologically sort groups so that a group whose
    // output feeds another group's input is executed before the consuming group.
    // This prevents non-terminating evaluation when groups have inter-group
    // dependencies (bug in previous code which sorted by rule count only).
    groups = topological_sort_groups(groups);

    let parallel_groups = groups.len();
    let max_concurrent = if parallel_workers <= 0 {
        1
    } else {
        parallel_groups.min(parallel_workers as usize)
    };

    ParallelAnalysis {
        groups,
        parallel_groups,
        max_concurrent,
    }
}

/// DL-PAR-02 (v0.81.0): topologically sort parallel groups so that groups
/// producing derived predicates consumed by other groups come first.
///
/// Uses Kahn's algorithm (BFS-based topological sort with in-degree tracking).
/// Groups that form cycles (mutually recursive, detected by DL-PAR-01 union-find)
/// are preserved in their original order since they must execute together.
fn topological_sort_groups(groups: Vec<ParallelGroup>) -> Vec<ParallelGroup> {
    let n = groups.len();
    if n <= 1 {
        return groups;
    }

    // Build a set of derived predicates per group index.
    let derived_by_group: Vec<HashSet<i64>> = groups
        .iter()
        .map(|g| g.derived_predicates.iter().copied().collect())
        .collect();

    // Build a set of body predicates per group index.
    let body_preds_by_group: Vec<HashSet<i64>> = groups
        .iter()
        .map(|g| {
            let all_derived: HashSet<i64> = groups
                .iter()
                .flat_map(|gg| gg.derived_predicates.iter().copied())
                .collect();
            g.rules
                .iter()
                .flat_map(|r| body_derived_preds(r, &all_derived))
                .collect()
        })
        .collect();

    // Compute edges: group i → group j if i produces predicates that j consumes.
    let mut in_degree = vec![0usize; n];
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];

    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            // Does group i produce any predicate that group j consumes?
            if derived_by_group[i]
                .iter()
                .any(|p| body_preds_by_group[j].contains(p))
            {
                adj[i].push(j);
                in_degree[j] += 1;
            }
        }
    }

    // Kahn's BFS.
    let mut queue: std::collections::VecDeque<usize> =
        (0..n).filter(|&i| in_degree[i] == 0).collect();
    // Sort for determinism.
    let mut q_sorted: Vec<usize> = queue.drain(..).collect();
    q_sorted.sort_unstable_by_key(|&i| {
        groups[i]
            .derived_predicates
            .iter()
            .copied()
            .min()
            .unwrap_or(i64::MAX)
    });
    queue.extend(q_sorted);

    let mut order: Vec<usize> = Vec::with_capacity(n);
    while let Some(i) = queue.pop_front() {
        order.push(i);
        let mut next: Vec<usize> = adj[i]
            .iter()
            .filter_map(|&j| {
                in_degree[j] -= 1;
                if in_degree[j] == 0 { Some(j) } else { None }
            })
            .collect();
        next.sort_unstable_by_key(|&j| {
            groups[j]
                .derived_predicates
                .iter()
                .copied()
                .min()
                .unwrap_or(i64::MAX)
        });
        queue.extend(next);
    }

    if order.len() < n {
        // Cycle in group dependency graph (shouldn't happen after DL-PAR-01 fix,
        // but preserve original order as fallback rather than losing groups).
        return groups;
    }

    // Reorder groups according to topological order.
    let mut indexed: Vec<Option<ParallelGroup>> = groups.into_iter().map(Some).collect();
    order
        .into_iter()
        .filter_map(|i| indexed[i].take())
        .collect()
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Extract the head predicate constant ID from an atom.
fn head_pred_id(atom: &Atom) -> Option<i64> {
    match &atom.p {
        Term::Const(id) => Some(*id),
        _ => None,
    }
}

/// Collect all body predicates in `rule` that are in `derived_set`.
fn body_derived_preds(rule: &Rule, derived_set: &HashSet<i64>) -> Vec<i64> {
    let mut out = Vec::new();
    for lit in &rule.body {
        let atom = match lit {
            BodyLiteral::Positive(a) => a,
            BodyLiteral::Negated(a) => a,
            BodyLiteral::Aggregate(agg) => &agg.atom,
            _ => continue,
        };
        if let Term::Const(id) = &atom.p
            && derived_set.contains(id)
        {
            out.push(*id);
        }
    }
    out
}

// ─── Union-Find ───────────────────────────────────────────────────────────────

/// Path-compressing union-find over `i64` predicate IDs.
struct UnionFind {
    parent: HashMap<i64, i64>,
    rank: HashMap<i64, u32>,
}

impl UnionFind {
    fn new(preds: &[i64]) -> Self {
        let mut parent = HashMap::new();
        let mut rank = HashMap::new();
        for &p in preds {
            parent.insert(p, p);
            rank.insert(p, 0);
        }
        Self { parent, rank }
    }

    fn find(&mut self, x: i64) -> i64 {
        if *self.parent.get(&x).unwrap_or(&x) == x {
            return x;
        }
        let root = {
            let p = *self.parent.get(&x).unwrap_or(&x);
            self.find(p)
        };
        self.parent.insert(x, root);
        root
    }

    fn union(&mut self, x: i64, y: i64) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return;
        }
        let rank_x = *self.rank.get(&rx).unwrap_or(&0);
        let rank_y = *self.rank.get(&ry).unwrap_or(&0);
        if rank_x < rank_y {
            self.parent.insert(rx, ry);
        } else if rank_x > rank_y {
            self.parent.insert(ry, rx);
        } else {
            self.parent.insert(ry, rx);
            self.rank.entry(rx).and_modify(|r| *r += 1);
        }
    }
}

// ─── Sequence range pre-allocation (v0.46.0) ─────────────────────────────────

/// Pre-allocate a contiguous SID range in the global statement-ID sequence before
/// launching `n_workers` parallel Datalog strata workers.
///
/// Each worker receives an exclusive `[start, start + batch_size)` slice and can
/// insert triples with pre-computed SIDs without touching the shared sequence.
/// This eliminates sequence contention under parallel inference.
///
/// Returns a `Vec` of `(range_start, range_end)` tuples — one per worker —
/// where `range_end` is exclusive.  The caller is responsible for routing each
/// worker to its assigned slice.
///
/// # Errors
///
/// Returns `None` if the sequence cannot be queried (e.g., the extension was
/// freshly created and the sequence has not been used yet) — callers fall back
/// to the serial path.
pub fn preallocate_sid_ranges(
    client: &pgrx::spi::SpiClient<'_>,
    n_workers: usize,
    batch_size: i32,
) -> Option<Vec<(i64, i64)>> {
    if n_workers == 0 {
        return Some(vec![]);
    }
    let total = n_workers as i64 * batch_size as i64;

    // Atomically advance the sequence by `total` and capture the new value.
    // `setval(seq, currval + total)` returns the new current value.
    let new_max: i64 = client
        .select(
            &format!(
                "SELECT setval(\
                   '_pg_ripple.statement_id_seq', \
                   nextval('_pg_ripple.statement_id_seq') + {} - 1\
                 )",
                total
            ),
            None,
            &[],
        )
        .ok()?
        .first()
        .get::<i64>(1)
        .ok()
        .flatten()?;

    // `new_max` is the last SID in the reserved block; `base` is the first.
    let base = new_max - total + 1;
    let ranges = (0..n_workers)
        .map(|i| {
            let start = base + i as i64 * batch_size as i64;
            let end = start + batch_size as i64;
            (start, end)
        })
        .collect();
    Some(ranges)
}

// ─── Savepoint helper (v0.51.0) ───────────────────────────────────────────────

/// Execute a batch of SQL statements inside a PostgreSQL SAVEPOINT.
///
/// If any statement in `stmts` fails, the savepoint is rolled back and the
/// error is logged as a warning; otherwise it is released.  This guarantees
/// that a failed parallel batch does not abort the enclosing transaction.
///
/// # Usage in the parallel-strata coordinator
///
/// Before launching each independent `ParallelGroup`'s rules, call this
/// function with the compiled SQL for that group.  A failed group's delta
/// tables are left empty for this iteration (the group will be retried next
/// round), while successful groups commit their results immediately.
// v0.60.0 A7-2: removed #[allow(dead_code)] — function is actively called
// from coordinator::execute_stratum_batch.
pub fn execute_with_savepoint(stmts: &[String], savepoint_name: &str) -> bool {
    use pgrx::Spi;

    let sp_begin = format!("SAVEPOINT {savepoint_name}");
    let sp_release = format!("RELEASE SAVEPOINT {savepoint_name}");
    let sp_rollback = format!("ROLLBACK TO SAVEPOINT {savepoint_name}");

    if Spi::run_with_args(&sp_begin, &[]).is_err() {
        pgrx::warning!("datalog parallel: failed to set SAVEPOINT {savepoint_name}");
        return false;
    }

    for sql in stmts {
        if let Err(e) = Spi::run_with_args(sql, &[]) {
            pgrx::warning!(
                "datalog parallel: batch error in SAVEPOINT {savepoint_name}: {e}; rolling back"
            );
            let _ = Spi::run_with_args(&sp_rollback, &[]);
            return false;
        }
    }

    if Spi::run_with_args(&sp_release, &[]).is_err() {
        pgrx::warning!(
            "datalog parallel: failed to RELEASE SAVEPOINT {savepoint_name}; rolling back"
        );
        let _ = Spi::run_with_args(&sp_rollback, &[]);
        return false;
    }

    true
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datalog::{Atom, Rule, Term};

    fn make_rule(head_pred: i64, body_preds: &[i64]) -> Rule {
        let head = Atom {
            s: Term::Var("x".to_owned()),
            p: Term::Const(head_pred),
            o: Term::Var("y".to_owned()),
            g: Term::DefaultGraph,
        };
        let body = body_preds
            .iter()
            .map(|&bp| {
                BodyLiteral::Positive(Atom {
                    s: Term::Var("x".to_owned()),
                    p: Term::Const(bp),
                    o: Term::Var("y".to_owned()),
                    g: Term::DefaultGraph,
                })
            })
            .collect();
        Rule {
            head: Some(head),
            body,
            rule_text: format!("?x <{head_pred}> ?y :- ..."),
            weight: None,
        }
    }

    #[test]
    fn test_independent_rules_form_separate_groups() {
        // Rules 10 and 20 derive different predicates and have no shared body deps.
        let r1 = make_rule(10, &[1, 2]);
        let r2 = make_rule(20, &[3, 4]);
        let analysis = partition_into_parallel_groups(&[r1, r2], 4);
        assert_eq!(
            analysis.parallel_groups, 2,
            "independent rules must form 2 groups"
        );
        assert_eq!(analysis.max_concurrent, 2);
    }

    #[test]
    fn test_dependent_rules_merged_into_one_group() {
        // Rule 20 body references predicate 10, which rule 10 derives.
        let r1 = make_rule(10, &[1, 2]);
        let r2 = make_rule(20, &[10, 3]);
        let analysis = partition_into_parallel_groups(&[r1, r2], 4);
        assert_eq!(
            analysis.parallel_groups, 1,
            "dependent rules must merge to 1 group"
        );
    }

    #[test]
    fn test_same_head_rules_in_same_group() {
        // Two rules both derive predicate 10.
        let r1 = make_rule(10, &[1]);
        let r2 = make_rule(10, &[2]);
        let analysis = partition_into_parallel_groups(&[r1, r2], 4);
        assert_eq!(analysis.parallel_groups, 1);
    }

    #[test]
    fn test_empty_rules() {
        let analysis = partition_into_parallel_groups(&[], 4);
        assert_eq!(analysis.parallel_groups, 0);
    }

    #[test]
    fn test_max_concurrent_capped_by_workers() {
        let r1 = make_rule(10, &[1]);
        let r2 = make_rule(20, &[2]);
        let r3 = make_rule(30, &[3]);
        let analysis = partition_into_parallel_groups(&[r1, r2, r3], 2);
        assert_eq!(analysis.parallel_groups, 3);
        assert_eq!(
            analysis.max_concurrent, 2,
            "max_concurrent capped at workers"
        );
    }
}
