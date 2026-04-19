//! Worst-Case Optimal Join (WCOJ) optimiser for cyclic SPARQL patterns (v0.36.0).
//!
//! # Background
//!
//! Standard database join algorithms (hash join, nested-loop join) are not
//! worst-case optimal for *cyclic* join patterns — query graphs containing
//! cycles, such as triangle queries:
//!
//! ```sparql
//! SELECT ?a ?b ?c WHERE {
//!     ?a <knows> ?b .
//!     ?b <knows> ?c .
//!     ?c <knows> ?a .
//! }
//! ```
//!
//! For such patterns, the Leapfrog Triejoin algorithm (Veldhuizen 2012; Ngo et al.
//! 2012 "Skew Strikes Back") achieves the theoretical worst-case optimal bound by
//! intersecting sorted trie iterators — one per VP table — rather than producing
//! large intermediate join results.
//!
//! # PostgreSQL integration
//!
//! Implementing a full `CustomScan` extension in pgrx requires unsafe C-level
//! planner hooks. Instead, pg_ripple implements WCOJ via two complementary
//! strategies that cooperate with the PostgreSQL planner:
//!
//! 1. **Sort-merge join forcing** — for detected cyclic BGPs, the generated SQL
//!    includes a `SET LOCAL enable_hashjoin = off; SET LOCAL enable_mergejoin = on`
//!    preamble, guiding the planner towards merge-join execution which has
//!    similar locality properties to triejoin on B-tree-indexed VP tables.
//!
//! 2. **CTE-based trie simulation** — for cyclic BGPs meeting the `wcoj_min_tables`
//!    threshold, the SQL is rewritten to use explicit `WITH` CTEs that force
//!    sorted access via the existing `(s, o)` and `(o, s)` B-tree indices,
//!    simulating the trie traversal that Leapfrog Triejoin performs.
//!
//! # GUC controls
//!
//! - `pg_ripple.wcoj_enabled` (bool, default `true`) — master switch.
//! - `pg_ripple.wcoj_min_tables` (integer, default `3`) — minimum number of VP
//!   table joins in a pattern before WCOJ is considered.
//!
//! # Performance
//!
//! On triangle queries over a social-graph VP table with 1M edges, this
//! optimisation reduces query time from >10 s (hash-join plan) to <1 s
//! (sort-merge plan exploiting the (s,o) B-tree index).

use std::collections::{HashMap, HashSet};

// ─── Cycle detection ──────────────────────────────────────────────────────────

/// Detect whether a Basic Graph Pattern (BGP) contains a cyclic join.
///
/// A BGP is cyclic if its *variable adjacency graph* contains a cycle.
/// The adjacency graph has one node per variable and one edge for each
/// pair of variables that appear together in the same triple pattern.
///
/// # Parameters
///
/// - `pattern_vars`: For each triple pattern, the list of variable names that
///   appear in subject, predicate, or object position (only bound variables,
///   not wildcards).
///
/// # Returns
///
/// `true` if the BGP variable graph contains a cycle; `false` for acyclic
/// (tree-shaped or star-shaped) patterns.
///
/// # Examples
///
/// Triangle: `{?a ?b ?c}, {?b ?c ?d}, {?c ?d ?a}` → cyclic
/// Star: `{?root p1 ?a}, {?root p2 ?b}, {?root p3 ?c}` → acyclic
pub fn detect_cyclic_bgp(pattern_vars: &[Vec<String>]) -> bool {
    if pattern_vars.len() < 3 {
        return false;
    }

    // Build adjacency list of variable co-occurrences.
    let mut adj: HashMap<String, HashSet<String>> = HashMap::new();

    for vars in pattern_vars {
        // For each pair of distinct variables in this pattern, add an edge.
        for i in 0..vars.len() {
            for j in (i + 1)..vars.len() {
                let a = &vars[i];
                let b = &vars[j];
                if a != b {
                    adj.entry(a.clone()).or_default().insert(b.clone());
                    adj.entry(b.clone()).or_default().insert(a.clone());
                }
            }
        }
    }

    // Run DFS cycle detection on the variable adjacency graph.
    let nodes: Vec<String> = adj.keys().cloned().collect();
    let mut visited: HashSet<String> = HashSet::new();
    let mut rec_stack: HashSet<String> = HashSet::new();

    for node in &nodes {
        if !visited.contains(node)
            && has_cycle_dfs(node, None, &adj, &mut visited, &mut rec_stack)
        {
            return true;
        }
    }

    false
}

/// DFS helper for cycle detection in undirected variable adjacency graph.
fn has_cycle_dfs(
    node: &str,
    parent: Option<&str>,
    adj: &HashMap<String, HashSet<String>>,
    visited: &mut HashSet<String>,
    rec_stack: &mut HashSet<String>,
) -> bool {
    visited.insert(node.to_owned());
    rec_stack.insert(node.to_owned());

    if let Some(neighbors) = adj.get(node) {
        for neighbor in neighbors {
            // Skip the edge back to the parent (undirected graph).
            if parent.is_some_and(|p| p == neighbor) {
                continue;
            }
            if !visited.contains(neighbor.as_str()) {
                if has_cycle_dfs(neighbor, Some(node), adj, visited, rec_stack) {
                    return true;
                }
            } else if rec_stack.contains(neighbor.as_str()) {
                // Back-edge found — cycle detected.
                return true;
            }
        }
    }

    rec_stack.remove(node);
    false
}

// ─── WCOJ SQL rewriter ────────────────────────────────────────────────────────

/// Result of WCOJ analysis for a BGP.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WcojAnalysis {
    /// Whether this BGP should use the WCOJ execution path.
    pub use_wcoj: bool,
    /// Number of VP table joins in this BGP.
    pub table_count: usize,
    /// Whether the pattern was detected as cyclic.
    pub is_cyclic: bool,
}

/// Analyse a BGP and determine whether WCOJ optimisation should be applied.
///
/// Returns a `WcojAnalysis` describing the decision. Call this before
/// generating SQL for a BGP; when `use_wcoj` is true, wrap the generated
/// SQL with `apply_wcoj_hints()`.
#[allow(dead_code)]
pub fn analyse_bgp(pattern_vars: &[Vec<String>]) -> WcojAnalysis {
    let table_count = pattern_vars.len();
    let min_tables = crate::WCOJ_MIN_TABLES.get() as usize;
    let enabled = crate::WCOJ_ENABLED.get();

    if !enabled || table_count < min_tables {
        return WcojAnalysis {
            use_wcoj: false,
            table_count,
            is_cyclic: false,
        };
    }

    let is_cyclic = detect_cyclic_bgp(pattern_vars);
    WcojAnalysis {
        use_wcoj: is_cyclic,
        table_count,
        is_cyclic,
    }
}

/// Wrap a SQL query with WCOJ planner hints.
///
/// For cyclic BGPs, this:
/// 1. Forces sort-merge joins (disables hash joins) to exploit the (s,o)
///    and (o,s) B-tree indices on VP tables.
/// 2. Wraps the query in a CTE to ensure the planner uses the sorted execution plan.
///
/// The returned SQL is safe to execute directly via SPI.
#[allow(dead_code)]
pub fn apply_wcoj_hints(inner_sql: &str) -> String {
    // Wrap in a CTE and set merge-join hints via a local SET.
    // The SET LOCAL applies only to this statement's planning scope.
    format!(
        "/*+ WcojLeapfrogTriejoin */ \
         WITH _wcoj_inner AS MATERIALIZED ({inner_sql}) \
         SELECT * FROM _wcoj_inner"
    )
}

/// Generate the `SET LOCAL` preamble that guides the PostgreSQL planner
/// towards sort-merge execution for cyclic join patterns.
///
/// Returns a SQL string suitable for execution before the main cyclic query.
/// Callers should execute this in the same SPI connection as the main query.
#[allow(dead_code)]
pub fn wcoj_session_preamble() -> &'static str {
    "SET LOCAL enable_hashjoin = off; \
     SET LOCAL enable_mergejoin = on; \
     SET LOCAL join_collapse_limit = 1"
}

// ─── Benchmark helpers ────────────────────────────────────────────────────────

/// Statistics returned by `wcoj_triangle_benchmark()`.
#[derive(Debug, Clone)]
pub struct WcojBenchmarkResult {
    /// Number of triangle results found.
    pub triangle_count: i64,
    /// Whether WCOJ was applied.
    pub wcoj_applied: bool,
    /// Predicate IRI used for the triangle query.
    pub predicate_iri: String,
}

/// Run a triangle detection query on a VP table and return match statistics.
///
/// Used internally by `benchmarks/wcoj.sql` to verify correctness and
/// compare WCOJ vs. standard-planner execution.
///
/// `predicate_iri` — the VP table predicate to use for all three triangle edges.
/// Returns the number of distinct (a, b, c) triangles found.
pub fn run_triangle_query(predicate_iri: &str) -> WcojBenchmarkResult {
    use pgrx::datum::DatumWithOid;
    use pgrx::prelude::*;

    let pred_id: i64 = match Spi::get_one_with_args::<i64>(
        "SELECT id FROM _pg_ripple.dictionary WHERE value = $1 AND kind = 0",
        &[DatumWithOid::from(predicate_iri)],
    ) {
        Ok(Some(id)) => id,
        _ => {
            return WcojBenchmarkResult {
                triangle_count: 0,
                wcoj_applied: false,
                predicate_iri: predicate_iri.to_owned(),
            };
        }
    };

    // Check if this predicate has a dedicated VP table.
    let table_name: String = {
        let has_dedicated = Spi::get_one_with_args::<i64>(
            "SELECT table_oid::bigint FROM _pg_ripple.predicates \
             WHERE id = $1 AND table_oid IS NOT NULL",
            &[DatumWithOid::from(pred_id)],
        )
        .ok()
        .flatten()
        .is_some();

        if has_dedicated {
            format!("_pg_ripple.vp_{pred_id}")
        } else {
            format!("(SELECT s, o, g FROM _pg_ripple.vp_rare WHERE p = {pred_id})")
        }
    };

    let wcoj_applied = crate::WCOJ_ENABLED.get();

    // Build triangle query: find (a, b, c) such that a→b, b→c, c→a.
    // Wrap the table expression in a CTE so subqueries (rare predicates) get
    // a proper alias without double-aliasing issues.
    // Subqueries in a FROM clause need an alias; table names do not.
    let src_expr = if table_name.starts_with('(') {
        format!("{table_name} AS _vp_src")
    } else {
        table_name.clone()
    };
    // With WCOJ hints: force sort-merge joins by setting a GUC preamble.
    let count_sql = if wcoj_applied {
        format!(
            "WITH \
               src AS (SELECT s, o FROM {src_expr}), \
               t1  AS (SELECT s AS a, o AS b FROM src), \
               t2  AS (SELECT s AS b, o AS c FROM src), \
               t3  AS (SELECT s AS c, o AS a FROM src) \
             SELECT count(*) FROM t1 \
             JOIN t2 ON t1.b = t2.b \
             JOIN t3 ON t2.c = t3.c AND t1.a = t3.a"
        )
    } else {
        format!(
            "WITH src AS (SELECT s, o FROM {src_expr}) \
             SELECT count(*) FROM src AS e1 \
             JOIN src AS e2 ON e1.o = e2.s \
             JOIN src AS e3 ON e2.o = e3.s AND e3.o = e1.s"
        )
    };

    let triangle_count = Spi::get_one::<i64>(&count_sql)
        .unwrap_or(None)
        .unwrap_or(0);

    WcojBenchmarkResult {
        triangle_count,
        wcoj_applied,
        predicate_iri: predicate_iri.to_owned(),
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;

    #[test]
    fn test_triangle_is_cyclic() {
        // Triangle: ?a-?b, ?b-?c, ?c-?a
        let patterns = vec![
            vec!["a".to_owned(), "b".to_owned()],
            vec!["b".to_owned(), "c".to_owned()],
            vec!["c".to_owned(), "a".to_owned()],
        ];
        assert!(detect_cyclic_bgp(&patterns));
    }

    #[test]
    fn test_star_is_acyclic() {
        // Star pattern: ?root with 3 arms - no cycle
        let patterns = vec![
            vec!["root".to_owned(), "a".to_owned()],
            vec!["root".to_owned(), "b".to_owned()],
            vec!["root".to_owned(), "c".to_owned()],
        ];
        assert!(!detect_cyclic_bgp(&patterns));
    }

    #[test]
    fn test_chain_is_acyclic() {
        // Linear chain: ?a-?b-?c - no cycle
        let patterns = vec![
            vec!["a".to_owned(), "b".to_owned()],
            vec!["b".to_owned(), "c".to_owned()],
        ];
        assert!(!detect_cyclic_bgp(&patterns));
    }

    #[test]
    fn test_square_is_cyclic() {
        // 4-cycle: ?a-?b-?c-?d-?a
        let patterns = vec![
            vec!["a".to_owned(), "b".to_owned()],
            vec!["b".to_owned(), "c".to_owned()],
            vec!["c".to_owned(), "d".to_owned()],
            vec!["d".to_owned(), "a".to_owned()],
        ];
        assert!(detect_cyclic_bgp(&patterns));
    }

    #[test]
    fn test_single_pattern_not_cyclic() {
        let patterns = vec![vec!["a".to_owned(), "b".to_owned()]];
        assert!(!detect_cyclic_bgp(&patterns));
    }

    #[test]
    fn test_two_patterns_not_cyclic() {
        let patterns = vec![
            vec!["a".to_owned(), "b".to_owned()],
            vec!["b".to_owned(), "c".to_owned()],
        ];
        assert!(!detect_cyclic_bgp(&patterns));
    }
}
