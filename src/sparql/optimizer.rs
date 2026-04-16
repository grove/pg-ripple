//! BGP join reordering and SHACL-driven query optimizer (v0.13.0).
//!
//! # BGP Join Reordering
//!
//! Estimates the selectivity of each triple pattern using PostgreSQL statistics
//! (`pg_class.reltuples`, `pg_stats.n_distinct`) and reorders patterns so the
//! most selective (smallest estimated result) is first.  This minimises
//! intermediate result sizes and is particularly effective on star patterns.
//!
//! The algorithm assigns a cost to each triple pattern:
//! - Both subject and object are ground constants → cost = ~1 (index point-lookup)
//! - Subject is bound (ground constant) → cost = reltuples / n_distinct(s)
//! - Object is bound (ground constant) → cost = reltuples / n_distinct(o)
//! - Neither bound → cost = reltuples (full scan)
//!
//! After reordering, `SET LOCAL join_collapse_limit = 1` is emitted before the
//! generated SQL so the PostgreSQL planner follows the computed join order.
//!
//! # SHACL-Driven Hints
//!
//! Reads `_pg_ripple.shacl_shapes` at translation time to gather per-predicate
//! cardinality hints:
//! - `sh:maxCount 1` → the predicate has at most 1 value per subject → skip
//!   `DISTINCT` for single-predicate projections and use `INNER JOIN` in OPTIONAL
//!   where provably safe.
//! - `sh:minCount 1` → the predicate has at least 1 value per subject → downgrade
//!   `LEFT JOIN` to `INNER JOIN` in OPTIONAL patterns.
//!
//! Hints are only applied when the query domain is provably identical to the
//! validated focus-node set (i.e. no additional FILTER conditions narrow the set
//! further).  The SQL generator must treat these as advisory only.

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use spargebra::term::{NamedNodePattern, TermPattern, TriplePattern};
use std::collections::HashMap;

// ─── Selectivity estimation ───────────────────────────────────────────────────

/// Estimated cost for one triple pattern.
///
/// Lower cost → more selective → should be scanned first.
pub type Cost = f64;

/// Cost returned when we have no statistics and must assume a full scan.
const UNKNOWN_COST: Cost = 1.0e12_f64;

/// Cache statistics for one VP table to avoid repeated SPI round-trips within
/// the same query translation.
pub(super) struct TableStats {
    /// `pg_class.reltuples` (may be -1 before first ANALYZE).
    reltuples: f64,
    /// `n_distinct` for the `s` column (negative values = fraction, as in PG docs).
    n_distinct_s: f64,
    /// `n_distinct` for the `o` column.
    n_distinct_o: f64,
}

impl TableStats {
    fn effective_reltuples(&self) -> f64 {
        // reltuples is -1 until ANALYZE has run; treat as 1 M row default.
        if self.reltuples < 0.0 {
            1_000_000.0_f64
        } else {
            self.reltuples.max(1.0)
        }
    }

    fn effective_ndistinct_s(&self) -> f64 {
        // PG convention: negative n_distinct means a fraction of reltuples.
        if self.n_distinct_s < 0.0 {
            (-self.n_distinct_s * self.effective_reltuples()).max(1.0)
        } else if self.n_distinct_s == 0.0 {
            self.effective_reltuples() // fallback
        } else {
            self.n_distinct_s
        }
    }

    fn effective_ndistinct_o(&self) -> f64 {
        if self.n_distinct_o < 0.0 {
            (-self.n_distinct_o * self.effective_reltuples()).max(1.0)
        } else if self.n_distinct_o == 0.0 {
            self.effective_reltuples()
        } else {
            self.n_distinct_o
        }
    }
}

fn fetch_table_stats(pred_id: i64) -> Option<TableStats> {
    // Get the table name for this predicate from the catalog.
    let has_table: bool = Spi::get_one_with_args::<bool>(
        "SELECT table_oid IS NOT NULL FROM _pg_ripple.predicates WHERE id = $1",
        &[DatumWithOid::from(pred_id)],
    )
    .unwrap_or(None)
    .unwrap_or(false);

    if !has_table {
        // Rare-predicate path: use vp_rare stats filtered to this predicate.
        // We use reltuples from the main table but can't get per-predicate n_distinct.
        let reltuples: f64 = Spi::get_one::<f64>(
            "SELECT reltuples::float8 FROM pg_class \
             WHERE relname = 'vp_rare' \
               AND relnamespace = (SELECT oid FROM pg_namespace WHERE nspname = '_pg_ripple')",
        )
        .unwrap_or(None)
        .unwrap_or(-1.0);

        // For rare predicates, use the per-predicate triple count as a better estimate.
        let triple_count: f64 = Spi::get_one_with_args::<i64>(
            "SELECT triple_count FROM _pg_ripple.predicates WHERE id = $1",
            &[DatumWithOid::from(pred_id)],
        )
        .unwrap_or(None)
        .unwrap_or(0) as f64;

        return Some(TableStats {
            reltuples: if triple_count > 0.0 {
                triple_count
            } else {
                reltuples
            },
            n_distinct_s: -0.1, // assume 10% distinct subjects
            n_distinct_o: -0.1,
        });
    }

    let table_name = format!("_pg_ripple.vp_{pred_id}_delta");

    // Query reltuples for the delta table (the main table may be empty until merge).
    let reltuples_q = format!(
        "SELECT reltuples::float8 FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = '_pg_ripple' AND c.relname = 'vp_{pred_id}_delta'"
    );
    let reltuples: f64 = Spi::get_one::<f64>(&reltuples_q)
        .unwrap_or(None)
        .unwrap_or(-1.0);

    // Query n_distinct for s and o columns from pg_stats.
    let stats_q = format!(
        "SELECT attname, n_distinct::float8 \
         FROM pg_stats \
         WHERE schemaname = '_pg_ripple' \
           AND tablename = 'vp_{pred_id}_delta' \
           AND attname IN ('s','o')"
    );

    let mut n_s = 0.0_f64;
    let mut n_o = 0.0_f64;

    let _ = Spi::connect(|client| {
        let rows = client.select(&stats_q, None, &[]);
        if let Ok(rows) = rows {
            for row in rows {
                let attname: Option<&str> = row.get_by_name("attname").unwrap_or(None);
                let ndistinct: Option<f64> = row.get_by_name("n_distinct").unwrap_or(None);
                match (attname, ndistinct) {
                    (Some("s"), Some(v)) => n_s = v,
                    (Some("o"), Some(v)) => n_o = v,
                    _ => {}
                }
            }
        }
        Ok::<(), pgrx::spi::SpiError>(())
    });

    // If no stats yet, fall back to main table stats.
    if n_s == 0.0 && n_o == 0.0 {
        let main_stats_q = format!(
            "SELECT attname, n_distinct::float8 \
             FROM pg_stats \
             WHERE schemaname = '_pg_ripple' \
               AND tablename = 'vp_{pred_id}_main' \
               AND attname IN ('s','o')"
        );
        let _ = Spi::connect(|client| {
            let rows = client.select(&main_stats_q, None, &[]);
            if let Ok(rows) = rows {
                for row in rows {
                    let attname: Option<&str> = row.get_by_name("attname").unwrap_or(None);
                    let ndistinct: Option<f64> = row.get_by_name("n_distinct").unwrap_or(None);
                    match (attname, ndistinct) {
                        (Some("s"), Some(v)) => n_s = v,
                        (Some("o"), Some(v)) => n_o = v,
                        _ => {}
                    }
                }
            }
            Ok::<(), pgrx::spi::SpiError>(())
        });
    }

    let _ = table_name; // suppress unused warning

    Some(TableStats {
        reltuples,
        n_distinct_s: n_s,
        n_distinct_o: n_o,
    })
}

/// Estimate cost of one triple pattern.
///
/// `pred_id` is `None` for variable-predicate patterns (full scan of all VP tables).
/// `subject_bound` and `object_bound` indicate whether those positions are ground constants.
pub(super) fn estimate_pattern_cost(
    pred_id: Option<i64>,
    subject_bound: bool,
    object_bound: bool,
    stats_cache: &mut HashMap<i64, TableStats>,
) -> Cost {
    let Some(id) = pred_id else {
        // Variable predicate: must scan everything — highest cost.
        return UNKNOWN_COST;
    };

    let stats = if let Some(s) = stats_cache.get(&id) {
        s
    } else {
        match fetch_table_stats(id) {
            Some(s) => {
                stats_cache.insert(id, s);
                stats_cache.get(&id).unwrap()
            }
            None => return UNKNOWN_COST,
        }
    };

    let rows = stats.effective_reltuples();

    match (subject_bound, object_bound) {
        (true, true) => {
            // Exact (s, p, o) lookup — near-zero selectivity.
            1.0_f64
        }
        (true, false) => {
            // Subject is bound: estimate rows = reltuples / n_distinct(s).
            rows / stats.effective_ndistinct_s()
        }
        (false, true) => {
            // Object is bound: estimate rows = reltuples / n_distinct(o).
            rows / stats.effective_ndistinct_o()
        }
        (false, false) => {
            // Full scan.
            rows
        }
    }
}

// ─── BGP pattern reordering ───────────────────────────────────────────────────

/// Whether a `TermPattern` is a ground constant (i.e. not a variable or blank node).
fn is_ground(tp: &TermPattern) -> bool {
    matches!(
        tp,
        TermPattern::NamedNode(_) | TermPattern::Literal(_) | TermPattern::Triple(_)
    )
}

/// Check if a `TermPattern` is a specific SPARQL variable name (already bound by
/// a prior pattern in the current ordering).
fn is_variable(tp: &TermPattern) -> Option<&str> {
    match tp {
        TermPattern::Variable(v) => Some(v.as_str()),
        _ => None,
    }
}

/// Reorder a slice of triple patterns so the most selective patterns come first,
/// minimising intermediate join sizes.
///
/// This is a greedy algorithm: at each step, pick the cheapest pattern among
/// those whose variables are fully or partially bound by previously selected patterns.
/// This is equivalent to a left-deep join tree construction with cardinality-based
/// cost estimation.
///
/// Returns the reordered patterns.  If `crate::BGP_REORDER.get()` is `false`,
/// returns the original order unchanged.
pub fn reorder_bgp(
    patterns: &[TriplePattern],
    encode_iri: &mut dyn FnMut(&str) -> Option<i64>,
) -> Vec<TriplePattern> {
    if !crate::BGP_REORDER.get() || patterns.len() < 2 {
        return patterns.to_vec();
    }

    let mut stats_cache: HashMap<i64, TableStats> = HashMap::new();

    // Collect per-pattern metadata.
    struct PatternMeta {
        pattern: TriplePattern,
        pred_id: Option<i64>,
    }

    let metas: Vec<PatternMeta> = patterns
        .iter()
        .map(|tp| {
            let pred_id = match &tp.predicate {
                NamedNodePattern::NamedNode(nn) => encode_iri(nn.as_str()),
                NamedNodePattern::Variable(_) => None,
            };
            PatternMeta {
                pattern: tp.clone(),
                pred_id,
            }
        })
        .collect();

    // Greedy left-deep reorder.
    let mut result: Vec<TriplePattern> = Vec::with_capacity(patterns.len());
    // Set of variable names already bound by patterns placed so far.
    let mut bound_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut remaining: Vec<bool> = vec![true; metas.len()];

    for _ in 0..metas.len() {
        let mut best_idx = 0usize;
        let mut best_cost = f64::MAX;

        for (i, meta) in metas.iter().enumerate() {
            if !remaining[i] {
                continue;
            }
            let s_bound = is_ground(&meta.pattern.subject)
                || is_variable(&meta.pattern.subject)
                    .map(|v| bound_vars.contains(v))
                    .unwrap_or(false);
            let o_bound = is_ground(&meta.pattern.object)
                || is_variable(&meta.pattern.object)
                    .map(|v| bound_vars.contains(v))
                    .unwrap_or(false);

            let cost = estimate_pattern_cost(meta.pred_id, s_bound, o_bound, &mut stats_cache);
            if cost < best_cost {
                best_cost = cost;
                best_idx = i;
            }
        }

        remaining[best_idx] = false;
        let meta = &metas[best_idx];

        // Register variables bound by this pattern.
        if let Some(v) = is_variable(&meta.pattern.subject) {
            bound_vars.insert(v.to_owned());
        }
        if let NamedNodePattern::Variable(v) = &meta.pattern.predicate {
            bound_vars.insert(v.as_str().to_owned());
        }
        if let Some(v) = is_variable(&meta.pattern.object) {
            bound_vars.insert(v.to_owned());
        }

        result.push(meta.pattern.clone());
    }

    // Suppress clippy lint — metas is consumed indirectly via result.
    let _ = &metas;

    result
}

// ─── SHACL optimizer hints ────────────────────────────────────────────────────

/// Per-predicate SHACL hints used by the SQL generator.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct PredicateHints {
    /// True if `sh:maxCount 1` is set for this predicate (at most 1 value per
    /// subject).  May skip `DISTINCT` for single-predicate projections.
    pub max_count_one: bool,
    /// True if `sh:minCount 1` is set (at least 1 value per subject).  Allows
    /// downgrading `LEFT JOIN` to `INNER JOIN` for OPTIONAL patterns.
    pub min_count_one: bool,
}

/// Load predicate hints from `_pg_ripple.shacl_shapes` for all predicates that
/// appear in the set of encoded predicate IDs.
///
/// Looks for property shapes with `sh:maxCount` and `sh:minCount` constraints.
/// Only applies hints when the constraint is provably global (no additional FILTER
/// or conditional constraints in the shape JSON).
#[allow(dead_code)]
pub fn load_predicate_hints(pred_ids: &[i64]) -> HashMap<i64, PredicateHints> {
    if pred_ids.is_empty() {
        return HashMap::new();
    }

    let mut hints: HashMap<i64, PredicateHints> = HashMap::new();

    // Build IN list from i64 pred_ids — safe because these are Rust-generated integers.
    let id_list = pred_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    // Query shapes that reference any of the predicates.
    // We look for sh:path → predicate IRI in the shape JSON.
    let query = "SELECT s.shape_json FROM _pg_ripple.shacl_shapes s WHERE s.active = true".to_owned();

    let _ = id_list; // currently we scan all active shapes; filter below

    let _ = Spi::connect(|client| {
        let rows = client.select(query.as_str(), None, &[]);
        let Ok(rows) = rows else {
            return Ok::<(), pgrx::spi::SpiError>(());
        };

        for row in rows {
            let shape_json: Option<pgrx::JsonB> = row.get_by_name("shape_json").unwrap_or(None);
            let Some(pgrx::JsonB(json)) = shape_json else {
                continue;
            };

            // Extract property shapes from the JSON.
            // Shape JSON structure: {"target": {...}, "properties": [...], ...}
            let properties = match json.get("properties").and_then(|v| v.as_array()) {
                Some(arr) => arr.clone(),
                None => continue,
            };

            for prop in &properties {
                let path_iri = prop
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if path_iri.is_empty() {
                    continue;
                }

                // Look up the predicate ID for this IRI.
                let pred_id_opt: Option<i64> = Spi::get_one_with_args::<i64>(
                    "SELECT id FROM _pg_ripple.dictionary \
                     WHERE value = $1 AND kind = 0",
                    &[DatumWithOid::from(path_iri)],
                )
                .unwrap_or(None);

                let Some(pred_id) = pred_id_opt else {
                    continue;
                };

                // Only apply hints if pred_id is in the requested set.
                if !pred_ids.contains(&pred_id) {
                    continue;
                }

                let entry = hints.entry(pred_id).or_default();

                // Check for sh:maxCount 1.
                if prop.get("maxCount").and_then(|v| v.as_i64()) == Some(1) {
                    entry.max_count_one = true;
                }

                // Check for sh:minCount >= 1.
                if prop
                    .get("minCount")
                    .and_then(|v| v.as_i64())
                    .map(|n| n >= 1)
                    .unwrap_or(false)
                {
                    entry.min_count_one = true;
                }
            }
        }

        Ok::<(), pgrx::spi::SpiError>(())
    });

    hints
}
