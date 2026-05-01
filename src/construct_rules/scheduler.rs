//! Rule scheduler — SPARQL CONSTRUCT parse helpers and topological sort.
//!
//! `compute_rule_order` determines the stratum ordering so that rules whose
//! CONSTRUCT output feeds another rule's source graph fire in the correct
//! topological sequence.

use pgrx::prelude::*;

// ─── SPARQL CONSTRUCT parsing helpers ────────────────────────────────────────

/// Collect the set of named-graph IRIs that appear inside `GRAPH <iri> { ... }`
/// clauses in a SPARQL algebra pattern.
pub(super) fn collect_source_graphs(
    pattern: &spargebra::algebra::GraphPattern,
) -> std::collections::HashSet<String> {
    use spargebra::algebra::GraphPattern;
    use spargebra::term::NamedNodePattern;

    let mut graphs = std::collections::HashSet::new();

    fn walk(pat: &GraphPattern, graphs: &mut std::collections::HashSet<String>) {
        match pat {
            GraphPattern::Graph {
                name: NamedNodePattern::NamedNode(nn),
                inner,
            } => {
                graphs.insert(nn.as_str().to_owned());
                walk(inner, graphs);
            }
            GraphPattern::Join { left, right }
            | GraphPattern::LeftJoin { left, right, .. }
            | GraphPattern::Minus { left, right }
            | GraphPattern::Union { left, right } => {
                walk(left, graphs);
                walk(right, graphs);
            }
            GraphPattern::Filter { inner, .. }
            | GraphPattern::Graph { inner, .. }
            | GraphPattern::Extend { inner, .. }
            | GraphPattern::OrderBy { inner, .. }
            | GraphPattern::Project { inner, .. }
            | GraphPattern::Distinct { inner }
            | GraphPattern::Reduced { inner }
            | GraphPattern::Slice { inner, .. }
            | GraphPattern::Group { inner, .. } => walk(inner, graphs),
            _ => {}
        }
    }

    walk(pattern, &mut graphs);
    graphs
}

// ─── Pipeline stratification ──────────────────────────────────────────────────

/// Compute the `rule_order` for a new rule.
///
/// Reads existing rules from the catalog and performs a topological sort.
/// Returns `Err` when a mutual-recursion cycle is detected.
pub(super) fn compute_rule_order(
    new_name: &str,
    target_graph: &str,
    source_graphs: &[String],
) -> Result<i32, String> {
    // Load existing rules: (name, target_graph_iri, source_graphs[])
    // SCHEMA-NORM-04: target_graph TEXT is dropped; decode target_graph_id via dictionary.
    let existing: Vec<(String, String, Vec<String>)> = Spi::connect(|c| {
        c.select(
            "SELECT cr.name, \
                    (SELECT value FROM _pg_ripple.dictionary WHERE id = cr.target_graph_id) AS target_graph, \
                    COALESCE(cr.source_graphs, '{}') \
             FROM _pg_ripple.construct_rules cr ORDER BY rule_order NULLS LAST",
            None,
            &[],
        )
        .map(|rows| {
            rows.filter_map(|row| {
                let name = row.get::<String>(1).ok().flatten()?;
                let tg = row.get::<String>(2).ok().flatten()?;
                let sgs: Vec<String> = row.get::<Vec<String>>(3).ok().flatten().unwrap_or_default();
                Some((name, tg, sgs))
            })
            .collect()
        })
        .unwrap_or_default()
    });

    // Build adjacency: edge (A → B) means rule A writes to graph G and rule B
    // reads from graph G.  We represent nodes as rule names.

    // Collect all nodes.
    let mut all_names: Vec<String> = existing.iter().map(|(n, _, _)| n.clone()).collect();
    all_names.push(new_name.to_owned());

    // Build the write-graph-to-rule map: graph_iri → rule_name that writes it.
    let mut writer_of: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (name, tg, _) in &existing {
        writer_of.insert(tg.clone(), name.clone());
    }
    writer_of.insert(target_graph.to_owned(), new_name.to_owned());

    // Build adjacency list: for each rule, which rules must run before it?
    // Rule B must run before rule A if B writes to a graph that A reads from.
    let mut predecessors: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for name in &all_names {
        predecessors.insert(name.clone(), Vec::new());
    }

    // Add existing rule edges.
    for (r_name, _, r_sources) in &existing {
        for sg in r_sources {
            if let Some(writer) = writer_of.get(sg.as_str())
                && writer != r_name
            {
                predecessors
                    .entry(r_name.clone())
                    .or_default()
                    .push(writer.clone());
            }
        }
    }
    // Add new rule edges.
    for sg in source_graphs {
        if let Some(writer) = writer_of.get(sg.as_str())
            && writer != new_name
        {
            predecessors
                .entry(new_name.to_owned())
                .or_default()
                .push(writer.clone());
        }
    }

    // Kahn's algorithm (topological sort with cycle detection).
    let mut in_degree: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for name in &all_names {
        in_degree.insert(name.clone(), 0);
    }
    for (rule, preds) in &predecessors {
        for pred in preds {
            *in_degree.entry(rule.clone()).or_insert(0) += 1;
            // Ensure pred is in the map.
            let _ = in_degree.entry(pred.clone()).or_insert(0);
        }
    }

    let mut queue: std::collections::VecDeque<String> = in_degree
        .iter()
        .filter(|&(_, &d)| d == 0)
        .map(|(n, _)| n.clone())
        .collect();
    queue = queue.into_iter().collect(); // deterministic order not needed

    let mut order: Vec<String> = Vec::new();
    while let Some(n) = queue.pop_front() {
        order.push(n.clone());
        // Find all rules that have n as a predecessor.
        for (rule, preds) in &predecessors {
            if preds.contains(&n) {
                // SCHEDULER-ERR-01 (v0.81.0): return Err instead of pgrx::error!()
                // so the caller can add context and handle the condition gracefully.
                let deg = in_degree.get_mut(rule).ok_or_else(|| {
                    format!(
                        "construct rule stratification: in_degree entry missing for rule \
                         \"{rule}\" — internal invariant violated"
                    )
                })?;
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(rule.clone());
                }
            }
        }
    }

    if order.len() < all_names.len() {
        // Cycle detected — find which rules are involved.
        let ordered_set: std::collections::HashSet<String> = order.into_iter().collect();
        let cycle_rules: Vec<String> = all_names
            .into_iter()
            .filter(|n| !ordered_set.contains(n))
            .collect();
        return Err(format!(
            "construct rules {} form a cycle — mutual writeback is not supported",
            cycle_rules.join(", ")
        ));
    }

    // The rule_order for new_name is its position in the topological order.
    let pos = order
        .iter()
        .position(|n| n == new_name)
        .unwrap_or(order.len()) as i32;
    Ok(pos)
}
