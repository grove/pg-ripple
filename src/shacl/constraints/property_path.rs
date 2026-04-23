//! Complex `sh:path` expression evaluation for SHACL validation (v0.48.0).
//!
//! Supports:
//! - `sh:inversePath`     — reverse traversal (query `(o, s)`)
//! - `sh:alternativePath` — union of multiple sub-paths
//! - Sequence paths       — chained joins
//! - `sh:zeroOrMorePath`  — `WITH RECURSIVE … CYCLE` (0+ hops)
//! - `sh:oneOrMorePath`   — `WITH RECURSIVE … CYCLE` (1+ hops)
//! - `sh:zeroOrOnePath`   — direct + zero-hop UNION

use pgrx::prelude::*;
use serde::{Deserialize, Serialize};

/// Structured representation of a SHACL `sh:path` expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShPath {
    /// A direct predicate IRI — the common case.
    Predicate(String),
    /// `sh:inversePath <iri>` — traverse `(o, s)`.
    Inverse(Box<ShPath>),
    /// `sh:alternativePath (p1 p2 ...)` — UNION of sub-paths.
    Alternative(Vec<ShPath>),
    /// Sequence `(p1 p2)` — chained joins.
    Sequence(Vec<ShPath>),
    /// `sh:zeroOrMorePath p` — 0+ hops via RECURSIVE CTE.
    ZeroOrMore(Box<ShPath>),
    /// `sh:oneOrMorePath p` — 1+ hops via RECURSIVE CTE.
    OneOrMore(Box<ShPath>),
    /// `sh:zeroOrOnePath p` — 0 or 1 hop.
    ZeroOrOne(Box<ShPath>),
}

impl ShPath {
    /// Return the direct predicate IRI if this is a simple `Predicate` path.
    pub fn as_direct_iri(&self) -> Option<&str> {
        match self {
            ShPath::Predicate(iri) => Some(iri.as_str()),
            _ => None,
        }
    }
}

/// Collect all object IDs reachable from `focus_id` via `path` in `graph_id`.
/// Returns an empty Vec when the path references an unknown predicate or when
/// no triples match.
pub fn traverse_sh_path(path: &ShPath, focus_id: i64, graph_id: i64) -> Vec<i64> {
    // Fast path: direct predicate lookup.
    if let Some(iri) = path.as_direct_iri() {
        return super::get_value_ids(
            focus_id,
            match crate::dictionary::lookup_iri(iri) {
                Some(id) => id,
                None => return vec![],
            },
            graph_id,
        );
    }

    let mut cte_count: u32 = 0;
    let sql = match compile_inner(path, Some(focus_id), graph_id, &mut cte_count) {
        Some(s) => s,
        None => return vec![],
    };

    let query = format!("SELECT o FROM ({sql}) AS _ph WHERE s = {focus_id}");

    Spi::connect(|c| {
        let tup = c
            .select(&query, None, &[])
            .unwrap_or_else(|e| pgrx::error!("sh:path traversal SPI error: {e}"));
        let mut out = Vec::new();
        for row in tup {
            if let Ok(Some(o)) = row.get::<i64>(1) {
                out.push(o);
            }
        }
        out
    })
}

// ─── Internal SQL compiler ────────────────────────────────────────────────────

fn pred_sql(iri: &str, graph_id: i64) -> Option<String> {
    use pgrx::datum::DatumWithOid;

    let pred_id = crate::dictionary::lookup_iri(iri)?;
    let g_cond = if graph_id == 0 {
        " AND g = 0".to_owned()
    } else if graph_id < 0 {
        String::new() // all graphs
    } else {
        format!(" AND g = {graph_id}")
    };

    let table_oid: Option<i64> = Spi::get_one_with_args::<i64>(
        "SELECT table_oid::bigint FROM _pg_ripple.predicates WHERE id = $1",
        &[DatumWithOid::from(pred_id)],
    )
    .unwrap_or(None);

    if table_oid.is_some() {
        Some(format!(
            "SELECT s, o FROM _pg_ripple.vp_{pred_id} WHERE TRUE{g_cond}"
        ))
    } else {
        Some(format!(
            "SELECT s, o FROM _pg_ripple.vp_rare WHERE p = {pred_id}{g_cond}"
        ))
    }
}

fn compile_inner(
    path: &ShPath,
    focus_id: Option<i64>,
    graph_id: i64,
    cte_count: &mut u32,
) -> Option<String> {
    match path {
        ShPath::Predicate(iri) => pred_sql(iri, graph_id),

        ShPath::Inverse(inner) => {
            let base = compile_inner(inner, None, graph_id, cte_count)?;
            Some(format!("SELECT o AS s, s AS o FROM ({base}) AS _inv"))
        }

        ShPath::Alternative(paths) => {
            let parts: Vec<String> = paths
                .iter()
                .filter_map(|p| compile_inner(p, None, graph_id, cte_count))
                .collect();
            if parts.is_empty() {
                return None;
            }
            Some(
                parts
                    .into_iter()
                    .map(|s| format!("({s})"))
                    .collect::<Vec<_>>()
                    .join(" UNION ALL "),
            )
        }

        ShPath::Sequence(steps) => {
            if steps.is_empty() {
                return None;
            }
            let parts: Vec<String> = steps
                .iter()
                .filter_map(|p| compile_inner(p, None, graph_id, cte_count))
                .collect();
            if parts.len() != steps.len() {
                return None; // some step had unknown predicate
            }
            if parts.len() == 1 {
                return parts.into_iter().next();
            }
            let mut result = parts[0].clone();
            for next_part in parts[1..].iter() {
                let n = *cte_count;
                *cte_count += 2;
                result = format!(
                    "SELECT _seq_l{n}.s, _seq_r{n}.o \
                     FROM ({result}) AS _seq_l{n} \
                     JOIN ({next_part}) AS _seq_r{n} ON _seq_l{n}.o = _seq_r{n}.s"
                );
            }
            Some(result)
        }

        ShPath::ZeroOrMore(inner) => {
            let base = compile_inner(inner, None, graph_id, cte_count)?;
            let n = *cte_count;
            *cte_count += 1;
            let anchor_filter = focus_id
                .map(|f| format!(" WHERE s = {f}"))
                .unwrap_or_default();
            Some(format!(
                "WITH RECURSIVE _zom_{n}(s, o) AS (
                    SELECT s, o FROM ({base}) AS _base_{n}{anchor_filter}
                    UNION ALL
                    SELECT _zom_{n}.s, _step_{n}.o
                    FROM _zom_{n}
                    JOIN ({base}) AS _step_{n} ON _zom_{n}.o = _step_{n}.s
                )
                CYCLE o SET _cyc_{n} USING _cycp_{n}
                SELECT s, o FROM _zom_{n} WHERE NOT _cyc_{n}
                UNION ALL
                SELECT DISTINCT s, s AS o FROM ({base}) AS _zero_{n}"
            ))
        }

        ShPath::OneOrMore(inner) => {
            let base = compile_inner(inner, None, graph_id, cte_count)?;
            let n = *cte_count;
            *cte_count += 1;
            let anchor_filter = focus_id
                .map(|f| format!(" WHERE s = {f}"))
                .unwrap_or_default();
            Some(format!(
                "WITH RECURSIVE _oom_{n}(s, o) AS (
                    SELECT s, o FROM ({base}) AS _base_{n}{anchor_filter}
                    UNION ALL
                    SELECT _oom_{n}.s, _step_{n}.o
                    FROM _oom_{n}
                    JOIN ({base}) AS _step_{n} ON _oom_{n}.o = _step_{n}.s
                )
                CYCLE o SET _cyc_{n} USING _cycp_{n}
                SELECT s, o FROM _oom_{n} WHERE NOT _cyc_{n}"
            ))
        }

        ShPath::ZeroOrOne(inner) => {
            let base = compile_inner(inner, None, graph_id, cte_count)?;
            let n = *cte_count;
            *cte_count += 1;
            Some(format!(
                "SELECT s, o FROM ({base}) AS _zoo_d_{n}
                 UNION ALL
                 SELECT DISTINCT s, s AS o FROM ({base}) AS _zoo_z_{n}"
            ))
        }
    }
}

/// Convenience wrapper: given a plain `path_iri` string, evaluate the path
/// for `focus_id` in `graph_id`.  For a simple predicate IRI this is equivalent
/// to `super::get_value_ids`; for a complex `ShPath` it compiles to SQL.
///
/// Called from the SHACL property-shape dispatcher (v0.51.0).
pub fn values_for_path_iri(path_iri: &str, focus_id: i64, graph_id: i64) -> Vec<i64> {
    traverse_sh_path(&ShPath::Predicate(path_iri.to_owned()), focus_id, graph_id)
}
