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
    let mut scc_neg_edge: Vec<bool> = vec![false; n]; // SCC-internal negative edge?

    for e in edges {
        let src_scc = pred_scc.get(&e.from).copied().unwrap_or(0);
        let dst_scc = pred_scc.get(&e.to).copied().unwrap_or(0);
        if src_scc == dst_scc && e.negative {
            // Negative self-edge within an SCC → unstratifiable.
            let pred_names: Vec<String> = sccs[src_scc].iter().map(|p| p.to_string()).collect();
            return Err(format!(
                "unstratifiable rule set — negation cycle detected in SCC: [{}]",
                pred_names.join(", ")
            ));
        }
        if src_scc != dst_scc {
            scc_adj[src_scc].insert(dst_scc);
            if e.negative {
                // dst_scc must be in a lower stratum than src_scc.
                // This is enforced by topo sort.
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
        if rule.head.is_none() {
            if strata_rules[0]
                .iter()
                .all(|r| r.rule_text != rule.rule_text)
            {
                strata_rules[0].push(rule.clone());
            }
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
