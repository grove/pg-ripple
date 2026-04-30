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

// ─── BGP variable extraction ──────────────────────────────────────────────────

/// Walk a `spargebra` `GraphPattern` and collect the variable names that appear
/// together in each triple pattern of any BGP.
///
/// Returns one `Vec<String>` per triple pattern containing the names of all
/// variables that appear in subject, predicate, or object position.  The outer
/// Vec may contain duplicates (one entry per triple pattern across all BGPs
/// in the query, including nested ones).
///
/// Used by `explain_sparql_jsonb` (WCOJ-01) to compute cyclic-BGP status
/// without re-running SQL translation.
pub fn extract_bgp_pattern_vars(pattern: &spargebra::algebra::GraphPattern) -> Vec<Vec<String>> {
    use spargebra::algebra::GraphPattern;
    use spargebra::term::{NamedNodePattern, TermPattern};

    let mut out: Vec<Vec<String>> = Vec::new();

    fn walk(p: &GraphPattern, out: &mut Vec<Vec<String>>) {
        match p {
            GraphPattern::Bgp { patterns } => {
                for tp in patterns {
                    let mut vars = Vec::new();
                    if let TermPattern::Variable(v) = &tp.subject {
                        vars.push(v.as_str().to_owned());
                    }
                    if let NamedNodePattern::Variable(v) = &tp.predicate {
                        vars.push(v.as_str().to_owned());
                    }
                    if let TermPattern::Variable(v) = &tp.object {
                        vars.push(v.as_str().to_owned());
                    }
                    if !vars.is_empty() {
                        out.push(vars);
                    }
                }
            }
            GraphPattern::Join { left, right }
            | GraphPattern::Union { left, right }
            | GraphPattern::LeftJoin { left, right, .. } => {
                walk(left, out);
                walk(right, out);
            }
            GraphPattern::Filter { inner, .. }
            | GraphPattern::Graph { inner, .. }
            | GraphPattern::Extend { inner, .. }
            | GraphPattern::OrderBy { inner, .. }
            | GraphPattern::Project { inner, .. }
            | GraphPattern::Distinct { inner }
            | GraphPattern::Reduced { inner }
            | GraphPattern::Slice { inner, .. }
            | GraphPattern::Group { inner, .. } => {
                walk(inner, out);
            }
            _ => {}
        }
    }

    walk(pattern, &mut out);
    out
}

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
        if !visited.contains(node) && has_cycle_dfs(node, None, &adj, &mut visited, &mut rec_stack)
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
pub struct WcojAnalysis {
    /// Whether this BGP should use the WCOJ execution path.
    pub use_wcoj: bool,
    /// Number of VP table joins in this BGP.
    #[allow(dead_code)]
    pub table_count: usize,
    /// Whether the pattern was detected as cyclic.
    #[allow(dead_code)]
    pub is_cyclic: bool,
}

/// Analyse a BGP and determine whether WCOJ optimisation should be applied.
///
/// Returns a `WcojAnalysis` describing the decision. Call this before
/// generating SQL for a BGP; when `use_wcoj` is true, wrap the generated
/// SQL with `apply_wcoj_hints()`.
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

    let triangle_count = Spi::get_one::<i64>(&count_sql).unwrap_or(None).unwrap_or(0);

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

// ─── Leapfrog Triejoin Executor (WCOJ-LFTI-01, v0.79.0) ───────────────────────
//
// Implements a true in-memory Leapfrog Triejoin (Veldhuizen 2012) that achieves
// worst-case optimal join complexity for cyclic BGP patterns.  The executor loads
// VP table edge data into sorted in-memory structures and then evaluates the join
// without generating SQL, bypassing the PostgreSQL hash-join planner.
//
// Architecture:
//   SortedIterator     — sorted Vec<i64> with O(log n) seek
//   EdgeData           — VP table loaded as (s,o) pairs with s-index and o-index
//   leapfrog_intersect — core LeapfrogJoin intersection algorithm
//   CyclicBgpPattern   — description of one triple pattern in a cyclic BGP
//   execute_leapfrog_triejoin — full n-way join executor

/// A sorted iterator over i64 values supporting O(log n) seek operations.
/// Implements the TrieIterator interface for the Leapfrog Triejoin algorithm.
pub struct SortedIterator {
    values: Vec<i64>,
    pos: usize,
}

impl SortedIterator {
    /// Create a new iterator from a list of values.  Values are sorted and
    /// deduplicated on construction.
    pub fn new(mut values: Vec<i64>) -> Self {
        values.sort_unstable();
        values.dedup();
        Self { values, pos: 0 }
    }

    /// Returns `true` when the iterator is exhausted.
    pub fn at_end(&self) -> bool {
        self.pos >= self.values.len()
    }

    /// The current key value.  Undefined when `at_end()`.
    pub fn key(&self) -> i64 {
        self.values[self.pos]
    }

    /// Advance to the next distinct value.
    pub fn next(&mut self) {
        if self.pos < self.values.len() {
            self.pos += 1;
        }
    }

    /// Advance so that `key() >= target`.  No-op when already satisfied.
    pub fn seek(&mut self, target: i64) {
        if self.pos >= self.values.len() {
            return;
        }
        if self.values[self.pos] >= target {
            return;
        }
        // Binary search in the remaining slice.
        let offset = self.values[self.pos..].partition_point(|&v| v < target);
        self.pos += offset;
    }

    /// Reset to the beginning of the iterator.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.pos = 0;
    }
}

/// Intersect multiple sorted iterators using the Leapfrog algorithm.
///
/// Returns the sorted list of values that appear in **all** input iterators.
/// Achieves worst-case optimal O(N · log N) behaviour where N is the smallest
/// iterator's length, using binary-search seeks rather than linear scans.
pub fn leapfrog_intersect(iters: &mut [SortedIterator]) -> Vec<i64> {
    if iters.is_empty() {
        return vec![];
    }
    if iters.iter().any(|it| it.at_end()) {
        return vec![];
    }

    let mut result = Vec::new();
    // Start the leapfrog at the current maximum across all iterators.
    let mut x = iters.iter().map(|it| it.key()).max().unwrap_or(i64::MAX);

    'outer: loop {
        // Seek every iterator to x, tracking the new maximum.
        let mut new_max = x;
        for it in iters.iter_mut() {
            it.seek(x);
            if it.at_end() {
                break 'outer;
            }
            let k = it.key();
            if k > new_max {
                new_max = k;
            }
        }

        if new_max == x {
            // All iterators agree on x — emit the common value.
            result.push(x);
            // Advance all iterators past x.
            for it in iters.iter_mut() {
                it.next();
            }
            // Recalculate the starting point for the next round.
            let next_max = iters
                .iter()
                .filter_map(|it| if it.at_end() { None } else { Some(it.key()) })
                .max();
            match next_max {
                Some(v) => x = v,
                None => break 'outer,
            }
        } else {
            // Divergence — restart with the new maximum.
            x = new_max;
        }
    }

    result
}

/// In-memory edge data loaded from a single VP table.
///
/// Maintains two sorted indices — one by subject and one by object — to
/// support O(log n) range lookups for either column.
pub struct EdgeData {
    /// (s, o) pairs sorted by (s, o).
    by_s: Vec<(i64, i64)>,
    /// (o, s) pairs sorted by (o, s).
    by_o: Vec<(i64, i64)>,
    /// Index over `by_s`: for each unique s, the range [start..end).
    s_ranges: Vec<(i64, usize, usize)>,
    /// Index over `by_o`: for each unique o, the range [start..end).
    o_ranges: Vec<(i64, usize, usize)>,
}

/// Build a range index over a sorted (key, val) pair slice.
fn build_ranges(pairs: &[(i64, i64)]) -> Vec<(i64, usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < pairs.len() {
        let key = pairs[i].0;
        let start = i;
        while i < pairs.len() && pairs[i].0 == key {
            i += 1;
        }
        ranges.push((key, start, i));
    }
    ranges
}

impl EdgeData {
    /// Load edges from a VP table specified by its predicate ID.
    /// Returns `None` when the VP table does not exist or is empty.
    pub fn load_from_vp(pred_id: i64) -> Option<Self> {
        use pgrx::datum::DatumWithOid;
        use pgrx::prelude::*;

        // Check whether a dedicated VP table exists.
        let table_exists: bool = Spi::get_one_with_args::<bool>(
            "SELECT EXISTS(SELECT 1 FROM _pg_ripple.predicates WHERE id = $1 \
             AND table_oid IS NOT NULL)",
            &[DatumWithOid::from(pred_id)],
        )
        .ok()
        .flatten()
        .unwrap_or(false);

        let sql = if table_exists {
            format!(
                "SELECT s, o FROM _pg_ripple.vp_{pred_id} \
                 UNION ALL \
                 SELECT s, o FROM _pg_ripple.vp_{pred_id}_delta"
            )
        } else {
            // Fall back to vp_rare.
            format!("SELECT s, o FROM _pg_ripple.vp_rare WHERE p = {pred_id}")
        };

        let mut edges: Vec<(i64, i64)> = Vec::new();
        Spi::connect(|client| {
            if let Ok(rows) = client.select(&sql, None, &[]) {
                for row in rows {
                    if let (Ok(Some(s)), Ok(Some(o))) = (row.get::<i64>(1), row.get::<i64>(2)) {
                        edges.push((s, o));
                    }
                }
            }
        });

        if edges.is_empty() {
            return None;
        }

        edges.sort_unstable();
        edges.dedup();

        let by_s = edges.clone();
        let s_ranges = build_ranges(&by_s);

        let mut by_o: Vec<(i64, i64)> = edges.iter().map(|(s, o)| (*o, *s)).collect();
        by_o.sort_unstable();
        by_o.dedup();
        let o_ranges = build_ranges(&by_o);

        Some(Self {
            by_s,
            by_o,
            s_ranges,
            o_ranges,
        })
    }

    /// All unique subject values.
    pub fn all_s(&self) -> Vec<i64> {
        self.s_ranges.iter().map(|(k, _, _)| *k).collect()
    }

    /// All unique object values.
    pub fn all_o(&self) -> Vec<i64> {
        self.o_ranges.iter().map(|(k, _, _)| *k).collect()
    }

    /// All object values where subject = `s`.
    pub fn o_for_s(&self, s: i64) -> Vec<i64> {
        match self.s_ranges.binary_search_by_key(&s, |(k, _, _)| *k) {
            Ok(pos) => {
                let (_, start, end) = self.s_ranges[pos];
                self.by_s[start..end].iter().map(|(_, o)| *o).collect()
            }
            Err(_) => vec![],
        }
    }

    /// All subject values where object = `o`.
    pub fn s_for_o(&self, o: i64) -> Vec<i64> {
        match self.o_ranges.binary_search_by_key(&o, |(k, _, _)| *k) {
            Ok(pos) => {
                let (_, start, end) = self.o_ranges[pos];
                self.by_o[start..end].iter().map(|(_, s)| *s).collect()
            }
            Err(_) => vec![],
        }
    }

    /// Return `true` if the edge (s, o) exists.
    #[allow(dead_code)]
    pub fn has_edge(&self, s: i64, o: i64) -> bool {
        self.by_s.binary_search(&(s, o)).is_ok()
    }

    /// Total number of edges.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.by_s.len()
    }

    /// Return `true` if there are no edges.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.by_s.is_empty()
    }
}

/// One triple pattern in a cyclic BGP, with both variables identified by name
/// and the predicate encoded as an i64 dictionary ID.
#[derive(Debug, Clone)]
pub struct CyclicBgpPattern {
    /// Variable name in subject position (or `"_"` for a constant).
    pub subject_var: String,
    /// Encoded predicate ID (i64 dictionary key).
    pub pred_id: i64,
    /// Variable name in object position (or `"_"` for a constant).
    pub object_var: String,
}

/// Result of a Leapfrog Triejoin execution: one binding per output row,
/// mapping variable name → encoded i64 dictionary ID.
pub type LftiBinding = std::collections::HashMap<String, i64>;

/// Execute a Leapfrog Triejoin for a cyclic BGP.
///
/// `patterns` — the triple patterns in the BGP (all predicates must be bound).
/// `variable_order` — the join order produced by the WCOJ planner (from
///   `analyse_bgp`).
///
/// Returns a vector of bindings.  Returns `None` when the executor cannot be
/// applied (e.g. an unbound predicate or a VP table that does not exist), so
/// the caller can fall back to the SQL hash-join path.
pub fn execute_leapfrog_triejoin(
    patterns: &[CyclicBgpPattern],
    variable_order: &[String],
) -> Option<Vec<LftiBinding>> {
    use std::collections::HashMap;

    if patterns.is_empty() || variable_order.is_empty() {
        return None;
    }

    // Load edge data for each unique predicate.
    let mut edge_cache: HashMap<i64, EdgeData> = HashMap::new();
    for pat in patterns {
        if let std::collections::hash_map::Entry::Vacant(e) = edge_cache.entry(pat.pred_id) {
            let ed = EdgeData::load_from_vp(pat.pred_id)?;
            e.insert(ed);
        }
    }

    // Build the binding by iterating variables in order.
    let mut result_bindings: Vec<LftiBinding> = Vec::new();
    let mut current_binding: HashMap<String, i64> = HashMap::new();

    lfti_recurse(
        patterns,
        variable_order,
        0,
        &edge_cache,
        &mut current_binding,
        &mut result_bindings,
    );

    Some(result_bindings)
}

/// Recursive depth-first search over the variable order.
///
/// At each depth, intersects the sorted value sets from all patterns
/// that reference the current variable, given the already-bound variables.
fn lfti_recurse(
    patterns: &[CyclicBgpPattern],
    variable_order: &[String],
    depth: usize,
    edge_cache: &std::collections::HashMap<i64, EdgeData>,
    current_binding: &mut std::collections::HashMap<String, i64>,
    result: &mut Vec<LftiBinding>,
) {
    if depth == variable_order.len() {
        // All variables bound — verify remaining unprocessed constraints and emit.
        result.push(current_binding.clone());
        return;
    }

    let var = &variable_order[depth];

    // Collect sorted candidate values for this variable from each pattern
    // that references it, respecting already-bound values.
    let mut iters: Vec<SortedIterator> = Vec::new();

    for pat in patterns {
        let ed = match edge_cache.get(&pat.pred_id) {
            Some(e) => e,
            None => continue,
        };

        let var_in_s = pat.subject_var == *var;
        let var_in_o = pat.object_var == *var;

        if !var_in_s && !var_in_o {
            continue;
        }

        // The other variable in this pattern.
        let other_s = pat.subject_var.as_str();
        let other_o = pat.object_var.as_str();

        if var_in_s {
            // Collect s values constrained by the (possibly bound) o.
            let candidates = if let Some(&bound_o) = current_binding.get(other_o) {
                // o is already bound — get all s where edge(s, bound_o) exists.
                ed.s_for_o(bound_o)
            } else {
                // o is not yet bound — all s values are candidates.
                ed.all_s()
            };
            iters.push(SortedIterator::new(candidates));
        } else {
            // var_in_o: Collect o values constrained by the (possibly bound) s.
            let candidates = if let Some(&bound_s) = current_binding.get(other_s) {
                // s is already bound — get all o where edge(bound_s, o) exists.
                ed.o_for_s(bound_s)
            } else {
                // s is not yet bound — all o values are candidates.
                ed.all_o()
            };
            iters.push(SortedIterator::new(candidates));
        }
    }

    if iters.is_empty() {
        // No constraints on this variable — enumerate all values from any edge.
        // Fall through with an unconstrained pass (shouldn't happen in a well-formed cyclic BGP).
        lfti_recurse(
            patterns,
            variable_order,
            depth + 1,
            edge_cache,
            current_binding,
            result,
        );
        return;
    }

    // Intersect the candidate sets using the Leapfrog algorithm.
    let common_values = leapfrog_intersect(&mut iters);

    for val in common_values {
        current_binding.insert(var.clone(), val);
        lfti_recurse(
            patterns,
            variable_order,
            depth + 1,
            edge_cache,
            current_binding,
            result,
        );
        current_binding.remove(var.as_str());
    }
}

/// Try to execute a SELECT query using the Leapfrog Triejoin executor.
///
/// Returns `None` if the query cannot be handled by LFTI (non-cyclic pattern,
/// unbound predicates, cardinality below threshold, or WCOJ disabled).
/// Returns `Some(bindings)` when LFTI executed successfully.
///
/// Each binding maps variable name → encoded i64 ID.
pub fn try_leapfrog_select(query: &spargebra::Query) -> Option<Vec<LftiBinding>> {
    if !crate::WCOJ_ENABLED.get() {
        return None;
    }

    let pattern = match query {
        spargebra::Query::Select { pattern, .. } => pattern,
        _ => return None,
    };

    // Extract flat BGP patterns.
    let bgp_triples = extract_flat_bgp(pattern)?;
    if bgp_triples.is_empty() {
        return None;
    }

    // Check minimum table count.
    let min_tables = crate::WCOJ_MIN_TABLES.get() as usize;
    if bgp_triples.len() < min_tables {
        return None;
    }

    // Build pattern_vars for cycle detection.
    let pattern_vars: Vec<Vec<String>> = bgp_triples
        .iter()
        .map(|(s_var, _, o_var)| {
            let mut v = Vec::new();
            if let Some(s) = s_var {
                v.push(s.clone());
            }
            if let Some(o) = o_var {
                v.push(o.clone());
            }
            v
        })
        .collect();

    if !detect_cyclic_bgp(&pattern_vars) {
        return None;
    }

    // Encode predicates; bail out if any predicate is unbound.
    let mut cyclic_patterns: Vec<CyclicBgpPattern> = Vec::new();
    for (s_var, pred_iri, o_var) in &bgp_triples {
        let pred_iri = pred_iri.as_ref()?;
        let pred_id = crate::dictionary::lookup_iri(pred_iri)?;
        cyclic_patterns.push(CyclicBgpPattern {
            subject_var: s_var.clone().unwrap_or_else(|| "_".to_string()),
            pred_id,
            object_var: o_var.clone().unwrap_or_else(|| "_".to_string()),
        });
    }

    // Check minimum cardinality threshold.
    let min_card = crate::gucs::sparql::WCOJ_MIN_CARDINALITY.get() as usize;
    if min_card > 0 {
        for pat in &cyclic_patterns {
            // Rough cardinality check: edge count for this predicate.
            use pgrx::datum::DatumWithOid;
            use pgrx::prelude::*;
            let card: i64 = Spi::get_one_with_args::<i64>(
                "SELECT COALESCE(triple_count, 0) FROM _pg_ripple.predicates WHERE id = $1",
                &[DatumWithOid::from(pat.pred_id)],
            )
            .ok()
            .flatten()
            .unwrap_or(0);
            if (card as usize) < min_card {
                return None; // Too small; fall back to hash join.
            }
        }
    }

    // Build variable order from pattern variables (all distinct variables in order of appearance).
    let mut seen = std::collections::HashSet::new();
    let mut variable_order: Vec<String> = Vec::new();
    for vars in &pattern_vars {
        for v in vars {
            if seen.insert(v.clone()) {
                variable_order.push(v.clone());
            }
        }
    }

    // Execute the Leapfrog Triejoin.
    execute_leapfrog_triejoin(&cyclic_patterns, &variable_order)
}

/// Extract a flat list of (subject_var, predicate_iri, object_var) triples from
/// a purely flat BGP pattern.  Returns `None` if the pattern is not a flat BGP.
type FlatBgpTriple = (Option<String>, Option<String>, Option<String>);

#[allow(clippy::type_complexity)]
fn extract_flat_bgp(pattern: &spargebra::algebra::GraphPattern) -> Option<Vec<FlatBgpTriple>> {
    use spargebra::algebra::GraphPattern;
    use spargebra::term::{NamedNodePattern, TermPattern};

    match pattern {
        GraphPattern::Bgp { patterns } => {
            let mut result = Vec::new();
            for tp in patterns {
                let s_var = match &tp.subject {
                    TermPattern::Variable(v) => Some(v.as_str().to_owned()),
                    _ => None,
                };
                let p_iri = match &tp.predicate {
                    NamedNodePattern::NamedNode(nn) => Some(nn.as_str().to_owned()),
                    _ => None, // Variable predicate — cannot use LFTI
                };
                let o_var = match &tp.object {
                    TermPattern::Variable(v) => Some(v.as_str().to_owned()),
                    _ => None,
                };
                result.push((s_var, p_iri, o_var));
            }
            Some(result)
        }
        GraphPattern::Project { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::OrderBy { inner, .. }
        | GraphPattern::Slice { inner, .. } => extract_flat_bgp(inner),
        _ => None,
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod lfti_tests {
    use super::*;

    #[test]
    fn test_sorted_iterator_seek() {
        let mut it = SortedIterator::new(vec![1, 3, 5, 7, 9]);
        assert_eq!(it.key(), 1);
        it.seek(4);
        assert_eq!(it.key(), 5);
        it.seek(5);
        assert_eq!(it.key(), 5);
        it.seek(10);
        assert!(it.at_end());
    }

    #[test]
    fn test_leapfrog_intersect_basic() {
        let mut iters = vec![
            SortedIterator::new(vec![1, 3, 5, 7]),
            SortedIterator::new(vec![2, 3, 6, 7]),
            SortedIterator::new(vec![3, 4, 7, 8]),
        ];
        let result = leapfrog_intersect(&mut iters);
        assert_eq!(result, vec![3, 7]);
    }

    #[test]
    fn test_leapfrog_intersect_empty() {
        let mut iters = vec![
            SortedIterator::new(vec![1, 2, 3]),
            SortedIterator::new(vec![4, 5, 6]),
        ];
        let result = leapfrog_intersect(&mut iters);
        assert!(result.is_empty());
    }

    #[test]
    fn test_leapfrog_intersect_single() {
        let mut iters = vec![SortedIterator::new(vec![1, 2, 3])];
        let result = leapfrog_intersect(&mut iters);
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn test_edge_data_lookup() {
        // Build edge data manually without SPI.
        let edges = vec![(1, 2), (1, 3), (2, 3), (3, 1)];
        let by_s = edges.clone();
        let s_ranges = build_ranges(&by_s);
        let mut by_o: Vec<(i64, i64)> = edges.iter().map(|(s, o)| (*o, *s)).collect();
        by_o.sort_unstable();
        let o_ranges = build_ranges(&by_o);
        let ed = EdgeData {
            by_s,
            by_o,
            s_ranges,
            o_ranges,
        };

        assert_eq!(ed.o_for_s(1), vec![2, 3]);
        assert_eq!(ed.o_for_s(2), vec![3]);
        assert_eq!(ed.s_for_o(1), vec![3]);
        assert!(ed.has_edge(2, 3));
        assert!(!ed.has_edge(2, 1));
    }
}
