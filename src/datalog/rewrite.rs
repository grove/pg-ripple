//! `owl:sameAs` entity canonicalization (v0.31.0).
//!
//! Implements the pre-pass described in ROADMAP.md §v0.31.0: before each
//! inference run, compute equivalence classes of `owl:sameAs` triples using
//! union-find over dictionary IDs, then rewrite all rule-body constants to
//! their canonical (lowest-ID) representative.
//!
//! # Integration
//!
//! Called from `run_inference_seminaive` and `run_inference` when the GUC
//! `pg_ripple.sameas_reasoning` is `true` (default).
//!
//! # SPARQL integration
//!
//! `canonicalize_id()` is also called from the SPARQL compiler when binding
//! IRI constants: if a SPARQL query references a non-canonical entity, it is
//! transparently rewritten to the canonical form before SQL generation.

use std::collections::HashMap;

use pgrx::datum::DatumWithOid;

use crate::datalog::{Atom, BodyLiteral, Rule, Term};

// ─── Union-Find ───────────────────────────────────────────────────────────────

struct UnionFind {
    parent: HashMap<i64, i64>,
}

impl UnionFind {
    fn new() -> Self {
        Self {
            parent: HashMap::new(),
        }
    }

    /// Find the root of `x`, with path compression.
    fn find(&mut self, x: i64) -> i64 {
        let p = *self.parent.get(&x).unwrap_or(&x);
        if p == x {
            return x;
        }
        let root = self.find(p);
        self.parent.insert(x, root);
        root
    }

    /// Union `x` and `y` — the lower ID becomes the canonical representative.
    fn union(&mut self, x: i64, y: i64) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return;
        }
        // Canonical = min ID (deterministic, independent of insertion order).
        if rx < ry {
            self.parent.insert(ry, rx);
        } else {
            self.parent.insert(rx, ry);
        }
    }
}

// ─── sameAs map computation ───────────────────────────────────────────────────

/// IRI for `owl:sameAs`.
const OWL_SAME_AS_IRI: &str = "http://www.w3.org/2002/07/owl#sameAs";

/// Compute the `owl:sameAs` canonicalization map.
///
/// Reads all triples with predicate `owl:sameAs` from the VP storage (both
/// dedicated VP tables and `vp_rare`), builds a union-find over the (s, o)
/// ID pairs, and returns a `HashMap` where each key is a **non-canonical** ID
/// and the value is its canonical (lowest-ID) representative.
///
/// IDs that are already canonical do not appear as keys.
/// Returns an empty map when there are no `owl:sameAs` triples or when SPI
/// is unavailable.
pub fn compute_sameas_map() -> HashMap<i64, i64> {
    let sameas_id = crate::dictionary::encode(OWL_SAME_AS_IRI, crate::dictionary::KIND_IRI);

    // Collect (s, o) pairs from vp_rare + dedicated VP table (if any).
    let pairs: Vec<(i64, i64)> = pgrx::Spi::connect(|client| {
        let mut result: Vec<(i64, i64)> = Vec::new();

        // Always scan vp_rare for sameAs entries.
        if let Ok(iter) = client.select(
            "SELECT s, o FROM _pg_ripple.vp_rare WHERE p = $1",
            None,
            &[DatumWithOid::from(sameas_id)],
        ) {
            for row in iter {
                let s = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                let o = row.get::<i64>(2).ok().flatten().unwrap_or(0);
                if s != 0 && o != 0 {
                    result.push((s, o));
                }
            }
        }

        // Check for a dedicated VP table (promoted predicate).
        let has_dedicated = client
            .select(
                "SELECT 1 FROM _pg_ripple.predicates \
                 WHERE id = $1 AND table_oid IS NOT NULL",
                None,
                &[DatumWithOid::from(sameas_id)],
            )
            .ok()
            .and_then(|mut iter| iter.next())
            .is_some();

        if has_dedicated {
            let table = format!("_pg_ripple.vp_{sameas_id}");
            let sql = format!("SELECT s, o FROM {table}");
            if let Ok(iter) = client.select(&sql, None, &[]) {
                for row in iter {
                    let s = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                    let o = row.get::<i64>(2).ok().flatten().unwrap_or(0);
                    if s != 0 && o != 0 {
                        result.push((s, o));
                    }
                }
            }
        }

        result
    });

    if pairs.is_empty() {
        return HashMap::new();
    }

    // ── v0.42.0: cluster size bound (PT550) ───────────────────────────────────
    // Before building the full union-find, check if any connected component would
    // exceed the configured maximum cluster size.  If so, emit PT550 WARNING and
    // return an empty map (no canonicalization) to avoid pathological overhead.
    let max_cluster = crate::SAMEAS_MAX_CLUSTER_SIZE.get();
    if max_cluster > 0 {
        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        for &(s, o) in &pairs {
            adj.entry(s).or_default().push(o);
            adj.entry(o).or_default().push(s);
        }
        // BFS/DFS to find maximum component size.
        let mut visited: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut max_seen: usize = 0;
        for &start in adj.keys() {
            if visited.contains(&start) {
                continue;
            }
            let mut stack = vec![start];
            let mut component_size: usize = 0;
            while let Some(node) = stack.pop() {
                if !visited.insert(node) {
                    continue;
                }
                component_size += 1;
                if let Some(neighbours) = adj.get(&node) {
                    for &nb in neighbours {
                        if !visited.contains(&nb) {
                            stack.push(nb);
                        }
                    }
                }
                // Early exit: already over limit
                if component_size > max_cluster as usize {
                    break;
                }
            }
            if component_size > max_seen {
                max_seen = component_size;
            }
            if max_seen > max_cluster as usize {
                pgrx::warning!(
                    "PT550: owl:sameAs equivalence class of {} members exceeds \
                     pg_ripple.sameas_max_cluster_size ({}); \
                     canonicalization skipped — check for data quality issues",
                    max_seen,
                    max_cluster
                );
                return HashMap::new();
            }
        }
    }

    // Build union-find from sameAs pairs.
    let mut uf = UnionFind::new();
    for &(s, o) in &pairs {
        uf.union(s, o);
    }

    // Collect all IDs mentioned in sameAs triples.
    let mut all_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for &(s, o) in &pairs {
        all_ids.insert(s);
        all_ids.insert(o);
    }

    // Build the canonicalization map: id → canonical (only non-canonical entries).
    let mut map: HashMap<i64, i64> = HashMap::new();
    for id in all_ids {
        let canonical = uf.find(id);
        if canonical != id {
            map.insert(id, canonical);
        }
    }

    map
}

// ─── Canonicalization helpers ─────────────────────────────────────────────────

/// Return the canonical ID for `id`.
///
/// If `id` appears in the map (i.e., it is a non-canonical alias), returns the
/// canonical representative.  Otherwise returns `id` unchanged.
pub fn canonicalize_id(id: i64, map: &HashMap<i64, i64>) -> i64 {
    *map.get(&id).unwrap_or(&id)
}

fn rewrite_term(term: &Term, map: &HashMap<i64, i64>) -> Term {
    match term {
        Term::Const(id) => Term::Const(canonicalize_id(*id, map)),
        other => other.clone(),
    }
}

fn rewrite_atom(atom: &Atom, map: &HashMap<i64, i64>) -> Atom {
    Atom {
        s: rewrite_term(&atom.s, map),
        p: rewrite_term(&atom.p, map),
        o: rewrite_term(&atom.o, map),
        g: rewrite_term(&atom.g, map),
    }
}

fn rewrite_body_literal(lit: &BodyLiteral, map: &HashMap<i64, i64>) -> BodyLiteral {
    use crate::datalog::AggregateLiteral;
    match lit {
        BodyLiteral::Positive(atom) => BodyLiteral::Positive(rewrite_atom(atom, map)),
        BodyLiteral::Negated(atom) => BodyLiteral::Negated(rewrite_atom(atom, map)),
        BodyLiteral::Compare(t1, op, t2) => {
            BodyLiteral::Compare(rewrite_term(t1, map), op.clone(), rewrite_term(t2, map))
        }
        BodyLiteral::StringBuiltin(_) => lit.clone(),
        BodyLiteral::Assign(var, t1, op, t2) => BodyLiteral::Assign(
            var.clone(),
            rewrite_term(t1, map),
            op.clone(),
            rewrite_term(t2, map),
        ),
        BodyLiteral::Aggregate(agg) => {
            let new_agg = AggregateLiteral {
                func: agg.func.clone(),
                agg_var: agg.agg_var.clone(),
                atom: rewrite_atom(&agg.atom, map),
                result_var: agg.result_var.clone(),
            };
            BodyLiteral::Aggregate(new_agg)
        }
    }
}

/// Rewrite all `Const` terms in `rules` using the sameAs canonicalization map.
///
/// Body atoms referencing non-canonical entities are rewritten to use the
/// canonical form so that rules are evaluated against a single canonical
/// representation of each equivalence class.
///
/// When `map` is empty, the original rules are returned unchanged (no cloning).
pub fn apply_sameas_to_rules(rules: &[Rule], map: &HashMap<i64, i64>) -> Vec<Rule> {
    if map.is_empty() {
        return rules.to_vec();
    }
    rules
        .iter()
        .map(|rule| Rule {
            head: rule.head.as_ref().map(|h| rewrite_atom(h, map)),
            body: rule
                .body
                .iter()
                .map(|lit| rewrite_body_literal(lit, map))
                .collect(),
            rule_text: rule.rule_text.clone(),
            weight: rule.weight,
        })
        .collect()
}
