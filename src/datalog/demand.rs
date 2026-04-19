//! Demand transformation for goal-directed Datalog inference (v0.31.0).
//!
//! A generalisation of magic sets (v0.29.0) that computes demand sets for
//! **all** predicates simultaneously via a fixed-point on the program
//! dependency graph.  This is more powerful than per-predicate magic sets
//! when a program has multiple mutually-recursive predicates or when a SPARQL
//! query references several derived predicates at once.
//!
//! # API
//!
//! - `pg_ripple.infer_demand(rule_set TEXT, demands JSONB) RETURNS JSONB`
//!
//! `demands` is a JSON array of goal patterns:
//! ```json
//! [{"p": "<https://example.org/transitive>"}, {"s": "<https://ex.org/a>", "p": "<https://ex.org/childOf>"}]
//! ```
//! Each element is an object with optional `"s"`, `"p"`, `"o"` keys.
//! Values are IRI strings (`<…>`), prefixed names (`prefix:local`), or omitted
//! for unbound (free-variable) positions.
//!
//! The function derives only the facts needed to answer the demands.
//!
//! # GUC
//!
//! `pg_ripple.demand_transform` (bool, default `true`) — when `true`,
//! `create_datalog_view()` automatically applies demand transformation when
//! multiple goal patterns are specified.  `infer_demand()` always applies
//! demand filtering regardless of this GUC.

use std::collections::{HashMap, HashSet};

use pgrx::datum::DatumWithOid;

use crate::datalog::parser::parse_rules;
use crate::datalog::{BodyLiteral, Rule, Term};

// ─── Demand pattern ───────────────────────────────────────────────────────────

/// A parsed demand pattern with optional bound positions (encoded as dictionary IDs).
#[derive(Debug, Clone)]
pub struct DemandSpec {
    /// Bound subject IRI encoded as dictionary ID, or `None` for free variable.
    #[allow(dead_code)]
    pub s: Option<i64>,
    /// Bound predicate IRI encoded as dictionary ID, or `None` for free variable.
    pub p: Option<i64>,
    /// Bound object IRI/literal encoded as dictionary ID, or `None` for free variable.
    #[allow(dead_code)]
    pub o: Option<i64>,
}

// ─── JSON parsing ─────────────────────────────────────────────────────────────

/// Parse a JSONB demands array into a list of `DemandSpec` values.
///
/// The JSON format is:
/// ```json
/// [{"s": "<iri>", "p": "<iri>", "o": "<iri>"}, ...]
/// ```
/// Any key can be omitted (treated as free variable). Values are encoded via the
/// dictionary; missing or `null` values are treated as free.
pub fn parse_demands_json(json_str: &str) -> Vec<DemandSpec> {
    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    let arr = match parsed.as_array() {
        Some(a) => a,
        None => return vec![],
    };

    arr.iter()
        .filter_map(|elem| {
            let obj = elem.as_object()?;

            let encode_field = |key: &str| -> Option<i64> {
                let val = obj.get(key)?.as_str()?;
                if val.is_empty() || val.starts_with('?') {
                    return None; // free variable
                }
                let resolved = crate::datalog::resolve_prefix(val);
                if resolved.starts_with('<') {
                    // IRI
                    let iri = resolved.trim_start_matches('<').trim_end_matches('>');
                    Some(crate::datalog::encode_iri(iri))
                } else if resolved.starts_with('"') {
                    // Literal
                    Some(crate::dictionary::encode(
                        &resolved,
                        crate::dictionary::KIND_LITERAL,
                    ))
                } else {
                    // bare IRI
                    Some(crate::datalog::encode_iri(&resolved))
                }
            };

            Some(DemandSpec {
                s: encode_field("s"),
                p: encode_field("p"),
                o: encode_field("o"),
            })
        })
        .collect()
}

// ─── Dependency graph ─────────────────────────────────────────────────────────

/// Build a map: head_pred_id → set of body pred IDs that the rule depends on.
///
/// Only positive body atoms with constant predicates contribute (negated atoms
/// are treated as barriers — the demand does not propagate through them).
fn build_dep_graph(rules: &[Rule]) -> HashMap<i64, HashSet<i64>> {
    let mut graph: HashMap<i64, HashSet<i64>> = HashMap::new();

    for rule in rules {
        let head_pred = match rule.head.as_ref().and_then(|h| {
            if let Term::Const(id) = &h.p {
                Some(*id)
            } else {
                None
            }
        }) {
            Some(id) => id,
            None => continue,
        };

        let entry = graph.entry(head_pred).or_default();

        for lit in &rule.body {
            if let BodyLiteral::Positive(atom) = lit
                && let Term::Const(pid) = &atom.p
            {
                entry.insert(*pid);
            }
        }
    }

    graph
}

/// Compute the set of all predicate IDs that, directly or transitively,
/// contribute to deriving any of the `demand_preds`.
///
/// Uses BFS / fixed-point expansion on the transposed dependency graph:
/// "to derive P, I need predicates Q, R, …" → I need Q and R too.
fn find_relevant_predicates(rules: &[Rule], demand_preds: &HashSet<i64>) -> HashSet<i64> {
    // Build: pred → {body_preds that contribute to it}
    let dep_graph = build_dep_graph(rules);

    // BFS: start from demand predicates, expand to their dependencies.
    let mut relevant: HashSet<i64> = demand_preds.clone();
    let mut frontier: Vec<i64> = demand_preds.iter().copied().collect();

    while let Some(pred) = frontier.pop() {
        if let Some(deps) = dep_graph.get(&pred) {
            for &dep in deps {
                if relevant.insert(dep) {
                    frontier.push(dep);
                }
            }
        }
    }

    relevant
}

// ─── Core inference with demand filtering ─────────────────────────────────────

/// Run semi-naive inference for `rule_set_name`, restricted to rules that
/// can contribute to deriving the given `demands`.
///
/// Returns `(total_derived, iterations, demand_predicate_ids)`.
///
/// When `demands` is empty, runs full inference (same as `infer()`).
pub fn run_infer_demand(rule_set_name: &str, demands: &[DemandSpec]) -> (i64, i32, Vec<i64>) {
    crate::datalog::ensure_catalog();

    // Load rules from catalog.
    let rule_rows: Vec<String> = {
        let sql = "SELECT rule_text FROM _pg_ripple.rules \
                   WHERE rule_set = $1 AND active = true \
                   ORDER BY stratum, id";
        pgrx::Spi::connect(|client| {
            client
                .select(sql, None, &[DatumWithOid::from(rule_set_name)])
                .unwrap_or_else(|e| pgrx::error!("infer_demand: rule select error: {e}"))
                .map(|row| row.get::<String>(1).ok().flatten().unwrap_or_default())
                .collect::<Vec<_>>()
        })
    };

    if rule_rows.is_empty() {
        return (0, 0, vec![]);
    }

    // Parse all rules.
    let mut all_rules: Vec<Rule> = Vec::new();
    for rule_text in &rule_rows {
        match parse_rules(rule_text, rule_set_name) {
            Ok(rs) => all_rules.extend(rs.rules),
            Err(e) => pgrx::warning!("infer_demand: rule parse error: {e}"),
        }
    }

    if all_rules.is_empty() {
        return (0, 0, vec![]);
    }

    // Collect demand predicates (from bound `p` positions in the demand specs).
    let demand_pred_ids: HashSet<i64> = demands.iter().filter_map(|d| d.p).collect();

    let demand_pred_ids_vec: Vec<i64> = demand_pred_ids.iter().copied().collect();

    // If no predicate demands are specified, run full inference.
    let relevant_rules: Vec<Rule> = if demand_pred_ids.is_empty() {
        all_rules
    } else {
        let relevant_preds = find_relevant_predicates(&all_rules, &demand_pred_ids);
        all_rules
            .into_iter()
            .filter(|r| {
                r.head
                    .as_ref()
                    .and_then(|h| {
                        if let Term::Const(id) = &h.p {
                            Some(relevant_preds.contains(id))
                        } else {
                            None
                        }
                    })
                    .unwrap_or(false)
            })
            .collect()
    };

    if relevant_rules.is_empty() {
        return (0, 0, demand_pred_ids_vec);
    }

    // Apply sameAs canonicalization if enabled.
    let relevant_rules = if crate::SAMEAS_REASONING.get() {
        let sameas_map = crate::datalog::rewrite::compute_sameas_map();
        crate::datalog::rewrite::apply_sameas_to_rules(&relevant_rules, &sameas_map)
    } else {
        relevant_rules
    };

    let (derived, iterations) = crate::datalog::run_seminaive_inner(&relevant_rules, rule_set_name);

    (derived, iterations, demand_pred_ids_vec)
}
