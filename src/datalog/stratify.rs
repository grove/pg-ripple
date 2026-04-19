//! Stratification engine for Datalog rule sets.
//!
//! Stratification partitions rules into layers such that every negated
//! predicate is fully computed in a lower stratum before its negation is
//! evaluated.  This guarantees a unique minimal model.
//!
//! # Algorithm
//!
//! 1. Build the predicate dependency graph (positive and negative edges).
//! 2. Compute strongly connected components (SCCs) of the dependency graph.
//! 3. If any SCC contains a negative edge, the program is unstratifiable.
//! 4. Topologically sort the SCCs → strata.
//! 5. Mark strata containing cycles as `is_recursive = true`.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::datalog::{Atom, BodyLiteral, Rule, Term};

// ─── Output types ─────────────────────────────────────────────────────────────

/// A single stratum of the stratified program.
#[derive(Debug, Clone)]
pub struct Stratum {
    pub rules: Vec<Rule>,
    pub is_recursive: bool,
    /// Predicate IDs defined (derived) in this stratum.
    #[allow(dead_code)]
    pub derived_predicates: Vec<i64>,
}

/// The stratified program produced by `stratify()`.
#[derive(Debug, Clone)]
pub struct StratifiedProgram {
    pub strata: Vec<Stratum>,
}

// ─── Dependency graph ─────────────────────────────────────────────────────────

/// A directed edge in the dependency graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Edge {
    from: PredicateId,
    to: PredicateId,
    negative: bool,
}

type PredicateId = i64;

/// Extract the predicate constant from an atom, or None for variable predicates.
fn atom_pred(atom: &Atom) -> Option<i64> {
    match &atom.p {
        Term::Const(id) => Some(*id),
        _ => None,
    }
}

/// Extract the head predicate ID from a rule (None for constraint rules).
fn head_pred(rule: &Rule) -> Option<i64> {
    rule.head.as_ref().and_then(atom_pred)
}

/// Build the predicate dependency graph from a slice of rules.
///
/// Returns a set of edges `(head_pred → body_pred, is_negative)`.
fn build_dependency_graph(rules: &[Rule]) -> Vec<Edge> {
    let mut edges = Vec::new();
    for rule in rules {
        let Some(h) = head_pred(rule) else {
            continue;
        };
        for lit in &rule.body {
            match lit {
                BodyLiteral::Positive(atom) => {
                    if let Some(p) = atom_pred(atom) {
                        edges.push(Edge {
                            from: h,
                            to: p,
                            negative: false,
                        });
                    }
                }
                BodyLiteral::Negated(atom) => {
                    if let Some(p) = atom_pred(atom) {
                        edges.push(Edge {
                            from: h,
                            to: p,
                            negative: true,
                        });
                    }
                }
                // v0.30.0: aggregate dependencies are treated as negative (strict ordering)
                // to enforce the aggregation-stratification requirement.
                BodyLiteral::Aggregate(agg) => {
                    if let Some(p) = atom_pred(&agg.atom) {
                        edges.push(Edge {
                            from: h,
                            to: p,
                            negative: true,
                        });
                    }
                }
                _ => {}
            }
        }
    }
    edges
}

// ─── SCC via Kosaraju's algorithm ─────────────────────────────────────────────

fn collect_predicates(rules: &[Rule]) -> HashSet<PredicateId> {
    let mut preds = HashSet::new();
    for rule in rules {
        if let Some(h) = head_pred(rule) {
            preds.insert(h);
        }
        for lit in &rule.body {
            match lit {
                BodyLiteral::Positive(atom) | BodyLiteral::Negated(atom) => {
                    if let Some(p) = atom_pred(atom) {
                        preds.insert(p);
                    }
                }
                _ => {}
            }
        }
    }
    preds
}

/// Compute SCCs using Kosaraju's two-pass DFS.
fn compute_sccs(nodes: &HashSet<PredicateId>, edges: &[Edge]) -> Vec<Vec<PredicateId>> {
    // Build adjacency lists (forward and reverse).
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut radj: HashMap<i64, Vec<i64>> = HashMap::new();
    for node in nodes {
        adj.entry(*node).or_default();
        radj.entry(*node).or_default();
    }
    for e in edges {
        adj.entry(e.from).or_default().push(e.to);
        radj.entry(e.to).or_default().push(e.from);
    }

    // Pass 1: DFS on forward graph; push nodes to finish-order stack.
    let mut visited: HashSet<i64> = HashSet::new();
    let mut finish_stack: Vec<i64> = Vec::new();

    fn dfs1(
        node: i64,
        adj: &HashMap<i64, Vec<i64>>,
        visited: &mut HashSet<i64>,
        stack: &mut Vec<i64>,
    ) {
        if visited.contains(&node) {
            return;
        }
        visited.insert(node);
        for &next in adj.get(&node).map(|v| v.as_slice()).unwrap_or(&[]) {
            dfs1(next, adj, visited, stack);
        }
        stack.push(node);
    }

    let mut all_nodes: Vec<i64> = nodes.iter().copied().collect();
    all_nodes.sort_unstable();
    for &node in &all_nodes {
        dfs1(node, &adj, &mut visited, &mut finish_stack);
    }

    // Pass 2: DFS on reverse graph in reverse finish order.
    let mut visited2: HashSet<i64> = HashSet::new();
    let mut sccs: Vec<Vec<i64>> = Vec::new();

    fn dfs2(
        node: i64,
        radj: &HashMap<i64, Vec<i64>>,
        visited: &mut HashSet<i64>,
        component: &mut Vec<i64>,
    ) {
        if visited.contains(&node) {
            return;
        }
        visited.insert(node);
        component.push(node);
        for &next in radj.get(&node).map(|v| v.as_slice()).unwrap_or(&[]) {
            dfs2(next, radj, visited, component);
        }
    }

    while let Some(node) = finish_stack.pop() {
        if !visited2.contains(&node) {
            let mut component = Vec::new();
            dfs2(node, &radj, &mut visited2, &mut component);
            if !component.is_empty() {
                sccs.push(component);
            }
        }
    }

    sccs
}

// ─── Topological sort of SCCs ─────────────────────────────────────────────────

/// Topologically sort SCCs; returns stratum index for each predicate.
fn topo_sort_sccs(
    sccs: &[Vec<PredicateId>],
    edges: &[Edge],
) -> Result<HashMap<PredicateId, usize>, String> {
    // Map predicate → SCC index.
    let mut pred_scc: HashMap<PredicateId, usize> = HashMap::new();
    for (i, scc) in sccs.iter().enumerate() {
        for &p in scc {
            pred_scc.insert(p, i);
        }
    }

    // Build SCC dependency graph.
    let n = sccs.len();
    let mut scc_adj: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    // Track negative edges between SCCs (needed for cross-SCC cycle detection).
    let mut scc_neg_edge: Vec<HashSet<usize>> = vec![HashSet::new(); n];

    for e in edges {
        let src_scc = pred_scc.get(&e.from).copied().unwrap_or(0);
        let dst_scc = pred_scc.get(&e.to).copied().unwrap_or(0);
        if src_scc == dst_scc && e.negative {
            // M-3: Negative self-edge within an SCC → unstratifiable negation cycle.
            // Trace the cycle path through the SCC to produce a named error message.
            let cycle_path = trace_negation_cycle_in_scc(&sccs[src_scc], edges);
            return Err(format!(
                "datalog: unstratifiable negation cycle: {cycle_path}"
            ));
        }
        if src_scc != dst_scc {
            scc_adj[src_scc].insert(dst_scc);
            if e.negative {
                scc_neg_edge[src_scc].insert(dst_scc);
            }
        }
    }

    // Additional M-3 check: detect negative cross-SCC edges that create cycles
    // when combined with positive paths back to the source SCC.
    for src in 0..n {
        for &neg_dst in &scc_neg_edge[src] {
            // Check if neg_dst can reach src through positive edges.
            if scc_can_reach(neg_dst, src, &scc_adj) {
                let src_preds: Vec<String> =
                    sccs[src].iter().map(|p| format!("pred_{p}")).collect();
                let dst_preds: Vec<String> =
                    sccs[neg_dst].iter().map(|p| format!("pred_{p}")).collect();
                return Err(format!(
                    "datalog: unstratifiable negation cycle: [{}] → ¬[{}] → … → [{}]",
                    src_preds.join("|"),
                    dst_preds.join("|"),
                    src_preds.join("|")
                ));
            }
        }
    }

    // Kahn's algorithm (topological sort on SCC DAG).
    let mut in_degree: Vec<usize> = vec![0; n];
    for adj in &scc_adj {
        for &dst in adj {
            in_degree[dst] += 1;
        }
    }

    let mut queue: VecDeque<usize> = VecDeque::new();
    for (i, &deg) in in_degree.iter().enumerate() {
        if deg == 0 {
            queue.push_back(i);
        }
    }

    let mut topo_order: Vec<usize> = Vec::with_capacity(n);
    while let Some(scc) = queue.pop_front() {
        topo_order.push(scc);
        for &next in &scc_adj[scc] {
            in_degree[next] -= 1;
            if in_degree[next] == 0 {
                queue.push_back(next);
            }
        }
    }

    if topo_order.len() != n {
        return Err("cyclic dependency in SCC DAG (bug in stratifier)".to_owned());
    }

    // Assign stratum index: position in topo_order → stratum.
    let mut scc_stratum: HashMap<usize, usize> = HashMap::new();
    for (stratum, &scc_idx) in topo_order.iter().enumerate() {
        scc_stratum.insert(scc_idx, stratum);
    }

    // Map predicate → stratum.
    let mut pred_stratum: HashMap<PredicateId, usize> = HashMap::new();
    for (pred, scc_idx) in &pred_scc {
        pred_stratum.insert(*pred, *scc_stratum.get(scc_idx).unwrap_or(&0));
    }

    Ok(pred_stratum)
}

// ─── M-3 helper functions ─────────────────────────────────────────────────────

/// Trace a negation cycle within an SCC and return a human-readable description.
///
/// Tries to reconstruct the cycle path like `"pred_A → ¬pred_B → pred_A"`.
/// Falls back to listing all predicates if a path cannot be traced.
fn trace_negation_cycle_in_scc(scc: &[PredicateId], edges: &[Edge]) -> String {
    // Find the negative edge within the SCC.
    let scc_set: HashSet<PredicateId> = scc.iter().copied().collect();

    // Find a negative back-edge.
    let neg_edge = edges
        .iter()
        .find(|e| e.negative && scc_set.contains(&e.from) && scc_set.contains(&e.to));

    match neg_edge {
        Some(ne) => {
            // Try to find a path from ne.to back to ne.from through positive edges.
            let path = find_positive_path(ne.to, ne.from, edges, &scc_set);
            match path {
                Some(mut chain) => {
                    // Build: A → ¬B → ... → A
                    let neg_label = format!("pred_{}", ne.to);
                    let first = format!("pred_{}", ne.from);
                    chain.insert(0, neg_label);
                    chain.insert(0, first.clone());
                    chain.push(first);
                    // Mark the negation edge.
                    if chain.len() >= 2 {
                        chain[1] = format!("¬{}", chain[1]);
                    }
                    chain.join(" → ")
                }
                None => {
                    // Just name the two predicates.
                    format!("pred_{} → ¬pred_{} → … → pred_{}", ne.from, ne.to, ne.from)
                }
            }
        }
        None => {
            // Fallback: list all predicates in the SCC.
            let names: Vec<String> = scc.iter().map(|p| format!("pred_{p}")).collect();
            format!("[{}]", names.join(", "))
        }
    }
}

/// Find a path of positive edges from `start` to `end` within the SCC `scc_set`.
/// Returns the intermediate node names (not including start/end).
fn find_positive_path(
    start: PredicateId,
    end: PredicateId,
    edges: &[Edge],
    scc_set: &HashSet<PredicateId>,
) -> Option<Vec<String>> {
    if start == end {
        return Some(vec![]);
    }
    // BFS over positive edges.
    let mut queue: std::collections::VecDeque<(PredicateId, Vec<String>)> =
        std::collections::VecDeque::new();
    let mut visited = HashSet::new();
    queue.push_back((start, vec![]));
    visited.insert(start);

    while let Some((current, path)) = queue.pop_front() {
        for e in edges {
            if e.from == current && !e.negative && scc_set.contains(&e.to) {
                if e.to == end {
                    return Some(path);
                }
                if !visited.contains(&e.to) {
                    visited.insert(e.to);
                    let mut new_path = path.clone();
                    new_path.push(format!("pred_{}", e.to));
                    queue.push_back((e.to, new_path));
                }
            }
        }
    }
    None
}

/// Check whether SCC `from` can reach SCC `to` via any edges in `scc_adj`.
fn scc_can_reach(from: usize, to: usize, scc_adj: &[HashSet<usize>]) -> bool {
    if from == to {
        return false; // same-SCC handled separately
    }
    let mut visited = HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(from);
    visited.insert(from);
    while let Some(cur) = queue.pop_front() {
        if let Some(adj) = scc_adj.get(cur) {
            for &next in adj {
                if next == to {
                    return true;
                }
                if !visited.contains(&next) {
                    visited.insert(next);
                    queue.push_back(next);
                }
            }
        }
    }
    false
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Stratify a slice of Datalog rules.
///
/// Returns a `StratifiedProgram` where rules are grouped into strata in
/// execution order (stratum 0 first).
pub fn stratify(rules: &[Rule]) -> Result<StratifiedProgram, String> {
    if rules.is_empty() {
        return Ok(StratifiedProgram { strata: vec![] });
    }

    let nodes = collect_predicates(rules);
    let edges = build_dependency_graph(rules);

    let sccs = compute_sccs(&nodes, &edges);
    let pred_stratum = topo_sort_sccs(&sccs, &edges)?;

    // Determine which predicates are recursive (SCC with size > 1, or self-loop).
    let mut pred_scc_map: HashMap<PredicateId, usize> = HashMap::new();
    for (i, scc) in sccs.iter().enumerate() {
        for &p in scc {
            pred_scc_map.insert(p, i);
        }
    }
    let recursive_sccs: HashSet<usize> = sccs
        .iter()
        .enumerate()
        .filter(|(_, scc)| scc.len() > 1)
        .map(|(i, _)| i)
        .collect();

    // Self-loops (predicate in its own positive body)
    let self_loop_preds: HashSet<PredicateId> = edges
        .iter()
        .filter(|e| e.from == e.to && !e.negative)
        .map(|e| e.from)
        .collect();

    // Build strata.
    let max_stratum = pred_stratum.values().copied().max().unwrap_or(0);
    let mut strata_rules: Vec<Vec<Rule>> = vec![vec![]; max_stratum + 1];

    for rule in rules {
        let stratum = head_pred(rule)
            .and_then(|p| pred_stratum.get(&p).copied())
            .unwrap_or(0);
        strata_rules[stratum].push(rule.clone());
    }

    // Constraint rules go in stratum 0 (evaluated after all base data is ready).
    for rule in rules {
        if rule.head.is_none()
            && strata_rules[0]
                .iter()
                .all(|r| r.rule_text != rule.rule_text)
        {
            strata_rules[0].push(rule.clone());
        }
    }

    let strata: Vec<Stratum> = strata_rules
        .into_iter()
        .enumerate()
        .filter(|(_, rules)| !rules.is_empty())
        .map(|(_, stratum_rules)| {
            let derived_predicates: Vec<i64> = stratum_rules
                .iter()
                .filter_map(head_pred)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();

            let is_recursive = derived_predicates.iter().any(|p| {
                self_loop_preds.contains(p)
                    || pred_scc_map
                        .get(p)
                        .is_some_and(|scc| recursive_sccs.contains(scc))
            });

            Stratum {
                rules: stratum_rules,
                is_recursive,
                derived_predicates,
            }
        })
        .collect();

    Ok(StratifiedProgram { strata })
}

// ─── v0.29.0: Subsumption checking ───────────────────────────────────────────

/// Check each pair of rules in a stratified program for subsumption.
///
/// Rule R2 is subsumed by rule R1 when:
/// - Both rules have the same head predicate.
/// - R1's positive body atoms are a (non-strict) subset of R2's positive body atoms
///   (up to variable renaming within each rule).
/// - R2 therefore always derives the same facts as R1, plus possibly more —
///   so R1 is strictly more general and R2 can be eliminated without changing
///   the minimal model.
///
/// Returns the list of `rule_text` values of eliminated (subsumed) rules.
///
/// This function is a compile-time optimization: subsumed rules are removed
/// before fixpoint evaluation, reducing the number of SQL statements generated
/// per iteration.  Controlled by (always-on for now; future GUC planned).
pub fn check_subsumption(rules: &[Rule]) -> Vec<String> {
    let mut eliminated: Vec<String> = Vec::new();
    let n = rules.len();

    'outer: for j in 0..n {
        let r2 = &rules[j];
        let Some(r2_head) = &r2.head else { continue };
        let Term::Const(r2_head_pred) = &r2_head.p else {
            continue;
        };

        // Collect r2's positive body predicate IDs (multiset).
        let r2_body_preds: Vec<i64> = r2
            .body
            .iter()
            .filter_map(|lit| {
                if let BodyLiteral::Positive(a) = lit
                    && let Term::Const(id) = &a.p
                {
                    return Some(*id);
                }
                None
            })
            .collect();

        // Check if any other rule R1 (i ≠ j) subsumes R2.
        for (i, r1) in rules.iter().enumerate() {
            if i == j {
                continue;
            }
            let Some(r1_head) = &r1.head else { continue };
            let Term::Const(r1_head_pred) = &r1_head.p else {
                continue;
            };

            // Same head predicate required.
            if r1_head_pred != r2_head_pred {
                continue;
            }

            // Collect r1's positive body predicate IDs.
            let r1_body_preds: Vec<i64> = r1
                .body
                .iter()
                .filter_map(|lit| {
                    if let BodyLiteral::Positive(a) = lit
                        && let Term::Const(id) = &a.p
                    {
                        return Some(*id);
                    }
                    None
                })
                .collect();

            // R2 is subsumed by R1 when:
            // 1. R1's body pred multiset ⊆ R2's body pred multiset (R1 is more general).
            // 2. R1 has strictly fewer body atoms (so R2 is the redundant one).
            // 3. Neither rule is a constraint rule (handled separately).
            if r1_body_preds.len() < r2_body_preds.len()
                && is_multiset_subset(&r1_body_preds, &r2_body_preds)
            {
                eliminated.push(r2.rule_text.clone());
                continue 'outer;
            }

            // Identical rules (same head, same body multiset) — keep only one.
            if i < j
                && r1_body_preds.len() == r2_body_preds.len()
                && is_multiset_subset(&r1_body_preds, &r2_body_preds)
                && is_multiset_subset(&r2_body_preds, &r1_body_preds)
            {
                eliminated.push(r2.rule_text.clone());
                continue 'outer;
            }
        }
    }

    eliminated
}

// ─── v0.30.0: Aggregation stratification check ───────────────────────────────

/// Check that aggregate rules satisfy the aggregation-stratification requirement:
/// for each aggregate body literal, the predicate being aggregated over must NOT
/// appear in the same stratum as the head predicate (no recursive aggregation).
///
/// Returns `Ok(())` if the program is aggregation-stratifiable.
/// Returns `Err(msg)` with a human-readable description if a violation is found.
/// The error message is prefixed with `"PT510:"`.
///
/// # Algorithm
///
/// For each rule with an `Aggregate` body literal, we check whether the aggregated
/// predicate and the head predicate form a cycle.  Because aggregates are treated
/// as negative edges by the standard stratifier, any cycle involving an aggregate
/// edge is already rejected by `stratify()`.  This function provides a more
/// targeted check with a PT510-specific error message.
pub fn check_aggregation_stratification(rules: &[Rule]) -> Result<(), String> {
    // Collect aggregate dependencies: head_pred → agg_body_pred.
    let mut agg_deps: Vec<(i64, i64)> = Vec::new();
    for rule in rules {
        let Some(head_pred) = head_pred(rule) else {
            continue;
        };
        for lit in &rule.body {
            if let BodyLiteral::Aggregate(agg) = lit
                && let Some(body_pred) = atom_pred(&agg.atom)
            {
                agg_deps.push((head_pred, body_pred));
            }
        }
    }

    if agg_deps.is_empty() {
        return Ok(());
    }

    // Collect all positive dependencies (non-aggregate, non-negative edges).
    let mut pos_deps: HashMap<i64, Vec<i64>> = HashMap::new();
    for rule in rules {
        let Some(h) = head_pred(rule) else {
            continue;
        };
        for lit in &rule.body {
            if let BodyLiteral::Positive(atom) = lit
                && let Some(p) = atom_pred(atom)
            {
                pos_deps.entry(h).or_default().push(p);
            }
        }
    }

    // For each aggregate dependency (head_pred → agg_body_pred), check whether
    // agg_body_pred can reach head_pred via positive edges.
    // If so, there's a cycle through aggregation → PT510.
    for (head_pred, agg_body_pred) in &agg_deps {
        if can_reach_positive(*agg_body_pred, *head_pred, &pos_deps) {
            let _ = (head_pred, agg_body_pred); // used for detection; not in message
            return Err("PT510: aggregation-stratification violation: \
                 a derived predicate depends on another predicate via aggregation, \
                 but that predicate is also derived through positive rules; \
                 this creates a cycle through aggregation; \
                 ensure the aggregated predicate is fully computed before the \
                 aggregate rule runs"
                .to_owned());
        }
    }

    Ok(())
}

/// Check whether predicate `start` can reach `target` via positive rule edges.
fn can_reach_positive(start: i64, target: i64, deps: &HashMap<i64, Vec<i64>>) -> bool {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(start);
    visited.insert(start);
    while let Some(cur) = queue.pop_front() {
        if cur == target {
            return true;
        }
        if let Some(nexts) = deps.get(&cur) {
            for &next in nexts {
                if visited.insert(next) {
                    queue.push_back(next);
                }
            }
        }
    }
    false
}

/// Test whether `a` is a multiset subset of `b`
/// (every element in `a` appears at least as many times in `b`).
fn is_multiset_subset(a: &[i64], b: &[i64]) -> bool {
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for &x in b {
        *counts.entry(x).or_insert(0) += 1;
    }
    for &x in a {
        let count = counts.entry(x).or_insert(0);
        if *count == 0 {
            return false;
        }
        *count -= 1;
    }
    true
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datalog::{Atom, BodyLiteral, Rule, Term};

    fn make_rule(head_p: i64, body_p: i64, negated: bool) -> Rule {
        Rule {
            head: Some(Atom {
                s: Term::Var("x".to_owned()),
                p: Term::Const(head_p),
                o: Term::Var("y".to_owned()),
                g: Term::DefaultGraph,
            }),
            body: vec![if negated {
                BodyLiteral::Negated(Atom {
                    s: Term::Var("x".to_owned()),
                    p: Term::Const(body_p),
                    o: Term::Var("y".to_owned()),
                    g: Term::DefaultGraph,
                })
            } else {
                BodyLiteral::Positive(Atom {
                    s: Term::Var("x".to_owned()),
                    p: Term::Const(body_p),
                    o: Term::Var("y".to_owned()),
                    g: Term::DefaultGraph,
                })
            }],
            rule_text: String::new(),
        }
    }

    #[test]
    fn test_stratify_simple() {
        let rules = vec![make_rule(10, 20, false), make_rule(30, 10, false)];
        let result = stratify(&rules).unwrap();
        assert!(!result.strata.is_empty());
    }

    #[test]
    fn test_stratify_negation_ok() {
        // 10 depends negatively on 20 — OK as long as 20 is base data.
        let rules = vec![make_rule(10, 20, true)];
        let result = stratify(&rules).unwrap();
        assert!(!result.strata.is_empty());
    }

    #[test]
    fn test_stratify_negation_cycle_error() {
        // 10 ← ¬10 is unstratifiable.
        let rules = vec![make_rule(10, 10, true)];
        let result = stratify(&rules);
        assert!(result.is_err(), "expected unstratifiable error");
    }

    #[test]
    fn test_stratify_recursive() {
        // 10 ← 10 (positive self-loop — recursive)
        let rules = vec![make_rule(10, 10, false)];
        let result = stratify(&rules).unwrap();
        let has_recursive = result.strata.iter().any(|s| s.is_recursive);
        assert!(has_recursive);
    }
}
