//! Property path compilation to SQL WITH RECURSIVE CTEs (v0.5.0).
//!
//! # SPARQL property path operators
//!
//! | SPARQL | Operator | SQL strategy |
//! |---|---|---|
//! | `p+`  | OneOrMore  | `WITH RECURSIVE` + `CYCLE` (PG18) |
//! | `p*`  | ZeroOrMore | `WITH RECURSIVE` + `CYCLE` + zero-hop UNION |
//! | `p?`  | ZeroOrOne  | `UNION` of direct + COALESCE identity |
//! | `a/b` | Sequence   | chained joins via subquery |
//! | `a\|b`| Alternative| `UNION ALL` |
//! | `^p`  | Reverse    | swap s and o |
//! | `!(p)`| NegatedPropertySet | `WHERE p NOT IN (...)` on vp_rare |
//!
//! Every generated subquery returns `(s BIGINT, o BIGINT)` columns.
//!
//! # Cycle detection
//!
//! Uses PostgreSQL 18's `CYCLE` clause for O(1) membership checks:
//! ```sql
//! WITH RECURSIVE _path(s, o) AS (...)
//! CYCLE o SET _is_cycle USING _cycle_path
//! SELECT s, o FROM _path WHERE NOT _is_cycle
//! ```

use spargebra::algebra::PropertyPathExpression;
use spargebra::term::NamedNode;

use crate::dictionary;

/// Resolve a NamedNode IRI to its VP table expression.
/// Returns `(table_expr, None)` for a dedicated VP table or
/// `(vp_rare, Some(pred_id))` for a rare predicate.
/// Returns `None` if the predicate is unknown (yields zero rows).
fn pred_table_expr(nn: &NamedNode) -> Option<String> {
    use pgrx::datum::DatumWithOid;
    use pgrx::prelude::*;

    let pred_id = dictionary::lookup_iri(nn.as_str())?;

    // Check whether the predicate has a dedicated VP table.
    match Spi::get_one_with_args::<i64>(
        "SELECT table_oid::bigint FROM _pg_ripple.predicates WHERE id = $1",
        &[DatumWithOid::from(pred_id)],
    ) {
        Ok(Some(_oid)) => Some(format!("SELECT s, o FROM _pg_ripple.vp_{pred_id}")),
        Ok(None) => Some(format!(
            "SELECT s, o FROM _pg_ripple.vp_rare WHERE p = {pred_id}"
        )),
        Err(_) => None,
    }
}

/// Counter to make CTE names unique within a query.
pub struct PathCtx {
    pub counter: u32,
}

impl PathCtx {
    pub fn new(start: u32) -> Self {
        Self { counter: start }
    }

    fn next(&mut self) -> u32 {
        let n = self.counter;
        self.counter += 1;
        n
    }
}

/// Compile a `PropertyPathExpression` to a SQL subquery that returns `(s, o)`.
///
/// `s_filter` / `o_filter` are optional SQL integer expressions that, when
/// provided, are pushed into the anchor (for start node) or final filter
/// (for end node) to reduce CTE work-table size.
///
/// Returns a SQL string representing an inline subquery `(SELECT s, o FROM ...)`.
pub fn compile_path(
    path: &PropertyPathExpression,
    s_filter: Option<&str>,
    o_filter: Option<&str>,
    ctx: &mut PathCtx,
    max_depth: i32,
) -> String {
    match path {
        // ── Simple predicate (degenerate case) ──────────────────────────────
        PropertyPathExpression::NamedNode(nn) => {
            let base = match pred_table_expr(nn) {
                Some(e) => e,
                None => "SELECT NULL::bigint AS s, NULL::bigint AS o LIMIT 0".to_owned(),
            };
            let mut conditions = Vec::new();
            if let Some(sf) = s_filter {
                conditions.push(format!("s = {sf}"));
            }
            if let Some(of) = o_filter {
                conditions.push(format!("o = {of}"));
            }
            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", conditions.join(" AND "))
            };
            format!("(SELECT s, o FROM ({base}) _pbase{where_clause})")
        }

        // ── Reverse: swap s and o ────────────────────────────────────────────
        PropertyPathExpression::Reverse(inner) => {
            // Swap s_filter and o_filter when descending.
            let inner_sql = compile_path(inner, o_filter, s_filter, ctx, max_depth);
            format!(
                "(SELECT o AS s, s AS o FROM {inner_sql} _prev{})",
                ctx.next()
            )
        }

        // ── Sequence: a/b → join on intermediate node ────────────────────────
        PropertyPathExpression::Sequence(left, right) => {
            let n = ctx.next();
            // left returns (?x, ?mid); right returns (?mid, ?y)
            let left_sql = compile_path(left, s_filter, None, ctx, max_depth);
            let right_sql = compile_path(right, None, o_filter, ctx, max_depth);
            format!(
                "(SELECT _lseq{n}.s, _rseq{n}.o \
                 FROM {left_sql} AS _lseq{n} \
                 JOIN {right_sql} AS _rseq{n} ON _lseq{n}.o = _rseq{n}.s)"
            )
        }

        // ── Alternative: a|b → UNION ALL ────────────────────────────────────
        PropertyPathExpression::Alternative(left, right) => {
            let left_sql = compile_path(left, s_filter, o_filter, ctx, max_depth);
            let right_sql = compile_path(right, s_filter, o_filter, ctx, max_depth);
            let n = ctx.next();
            let mut conditions = Vec::new();
            if let Some(sf) = s_filter {
                conditions.push(format!("s = {sf}"));
            }
            if let Some(of) = o_filter {
                conditions.push(format!("o = {of}"));
            }
            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", conditions.join(" AND "))
            };
            format!(
                "(SELECT s, o FROM (\
                 SELECT s, o FROM {left_sql} _altL{n} \
                 UNION ALL \
                 SELECT s, o FROM {right_sql} _altR{n}\
                 ) _alt{n}{where_clause})"
            )
        }

        // ── OneOrMore (p+) ───────────────────────────────────────────────────
        //
        // WITH RECURSIVE _path(s, o, depth) AS (
        //   -- anchor: all direct edges (optionally filtered to start node)
        //   SELECT s, o, 1 FROM {base}
        //   UNION ALL
        //   -- recurse: extend by one hop
        //   SELECT _path.s, vp.o, _path.depth + 1
        //   FROM _path
        //   JOIN {base} vp ON _path.o = vp.s
        //   WHERE _path.depth < {max_depth}
        // )
        // CYCLE s, o SET _is_cycle USING _cycle_path   ← v0.21.0 fix
        // SELECT DISTINCT s, o FROM _path WHERE NOT _is_cycle
        PropertyPathExpression::OneOrMore(inner) => {
            let n = ctx.next();
            let cte_name = format!("_opm{n}");
            let base_sql = compile_path(inner, None, None, ctx, max_depth);
            let depth_guard = depth_guard_clause(max_depth, &cte_name);
            let anchor_where = s_filter
                .map(|sf| format!(" WHERE _anchor{n}.s = {sf}"))
                .unwrap_or_default();
            let final_where = o_filter
                .map(|of| format!(" AND o = {of}"))
                .unwrap_or_default();
            format!(
                "(WITH RECURSIVE {cte_name}(s, o, _depth) AS (\
                 SELECT _anchor{n}.s, _anchor{n}.o, 1 \
                 FROM {base_sql} AS _anchor{n}{anchor_where} \
                 UNION ALL \
                 SELECT {cte_name}.s, _step{n}.o, {cte_name}._depth + 1 \
                 FROM {cte_name} \
                 JOIN {base_sql} AS _step{n} ON {cte_name}.o = _step{n}.s \
                 {depth_guard}\
                 ) CYCLE s, o SET _is_cycle USING _cycle_path \
                 SELECT DISTINCT s, o FROM {cte_name} \
                 WHERE NOT _is_cycle{final_where})"
            )
        }

        // ── ZeroOrMore (p*) ──────────────────────────────────────────────────
        //
        // Same as OneOrMore but adds a zero-hop anchor (identity: s = o).
        //
        // v0.21.0 fix: restrict the zero-hop identity row to subjects that
        // actually appear in the predicate's VP tables.  Previously, all graph
        // nodes (any subject in any triple) would get a spurious reflexive row
        // even if they never appear in the predicate being traversed.
        PropertyPathExpression::ZeroOrMore(inner) => {
            let n = ctx.next();
            let cte_name = format!("_zom{n}");
            let base_sql = compile_path(inner, None, None, ctx, max_depth);
            let depth_guard = depth_guard_clause(max_depth, &cte_name);
            // Filters for anchor arms (subject-start constraint).
            let sf_cond = s_filter
                .map(|sf| format!(" WHERE s = {sf}"))
                .unwrap_or_default();
            let final_where = o_filter
                .map(|of| format!(" AND o = {of}"))
                .unwrap_or_default();
            // v0.21.0: Zero-hop rows use the same {base_sql} source, so the
            // identity (s = o) is only generated for nodes that actually appear
            // as subjects (s column) in the predicate's VP tables.
            format!(
                "(WITH RECURSIVE {cte_name}(s, o, _depth) AS (\
                 SELECT _anc{n}.s, _anc{n}.o, _anc{n}._depth \
                 FROM (\
                   SELECT s, o, 1 AS _depth FROM {base_sql} AS _b1{n}{sf_cond} \
                   UNION ALL \
                   SELECT DISTINCT s, s AS o, 0 AS _depth FROM {base_sql} AS _b0{n}{sf_cond} \
                 ) AS _anc{n} \
                 UNION ALL \
                 SELECT {cte_name}.s, _step{n}.o, {cte_name}._depth + 1 \
                 FROM {cte_name} \
                 JOIN {base_sql} AS _step{n} ON {cte_name}.o = _step{n}.s \
                 {depth_guard}\
                 ) CYCLE s, o SET _is_cycle USING _cycle_path \
                 SELECT DISTINCT s, o FROM {cte_name} \
                 WHERE NOT _is_cycle{final_where})"
            )
        }

        // ── ZeroOrOne (p?) ───────────────────────────────────────────────────
        //
        // Direct edge OR identity (s = o if subject is in graph)
        PropertyPathExpression::ZeroOrOne(inner) => {
            let n = ctx.next();
            let base_sql = compile_path(inner, s_filter, o_filter, ctx, max_depth);
            let mut conditions = Vec::new();
            if let Some(sf) = s_filter {
                conditions.push(format!("s = {sf}"));
            }
            if let Some(of) = o_filter {
                conditions.push(format!("o = {of}"));
            }
            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", conditions.join(" AND "))
            };
            // Zero-hop: identity edges for all subjects that appear in the base
            format!(
                "(SELECT s, o FROM {base_sql} AS _onepart{n} \
                 UNION \
                 SELECT s, s AS o FROM ({base_sql}) AS _zeropart{n}{where_clause})"
            )
        }

        // ── NegatedPropertySet !(p1|p2|...) ────────────────────────────────
        //
        // v0.21.0: scan both dedicated VP tables and vp_rare, excluding the
        // listed predicates.  Previously only vp_rare was scanned, which missed
        // triples stored in dedicated tables.
        PropertyPathExpression::NegatedPropertySet(excluded) => {
            let n = ctx.next();
            let excluded_ids: Vec<String> = excluded
                .iter()
                .filter_map(|nn| dictionary::lookup_iri(nn.as_str()))
                .map(|id| id.to_string())
                .collect();
            let not_in_clause = if excluded_ids.is_empty() {
                String::new()
            } else {
                format!(" AND p NOT IN ({})", excluded_ids.join(", "))
            };
            let mut conditions = Vec::new();
            if let Some(sf) = s_filter {
                conditions.push(format!("s = {sf}"));
            }
            if let Some(of) = o_filter {
                conditions.push(format!("o = {of}"));
            }
            let where_clause = if conditions.is_empty() {
                format!("WHERE TRUE{not_in_clause}")
            } else {
                format!("WHERE {}{not_in_clause}", conditions.join(" AND "))
            };

            // Build a UNION ALL that covers all predicates: both dedicated VP
            // tables (projected with a `p` column equal to their predicate ID)
            // and vp_rare (which already has a `p` column).
            let all_preds_union = build_all_predicates_with_p();

            format!("(SELECT s, o FROM ({all_preds_union}) _neg{n} {where_clause})")
        }
    }
}

/// Build a UNION ALL subquery over all predicates with a `(p, s, o)` projection.
/// Used by NegatedPropertySet to scan every predicate.
fn build_all_predicates_with_p() -> String {
    use pgrx::prelude::*;
    let mut branches: Vec<String> = Vec::new();

    Spi::connect(|client| {
        let rows = client
            .select(
                "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("negated property set SPI error: {e}"));
        for row in rows {
            if let Ok(Some(pred_id)) = row.get::<i64>(1) {
                branches.push(format!(
                    "SELECT {pred_id}::bigint AS p, s, o FROM _pg_ripple.vp_{pred_id}"
                ));
            }
        }
    });

    // Always include vp_rare.
    branches.push("SELECT p, s, o FROM _pg_ripple.vp_rare".to_owned());

    if branches.len() == 1 {
        branches[0].clone()
    } else {
        branches.join(" UNION ALL ")
    }
}

/// Build a `WHERE _cte._depth < {max_depth}` guard clause for recursive CTEs.
/// If max_depth is 0 (GUC default interpretation: unlimited), no guard is emitted.
fn depth_guard_clause(max_depth: i32, cte_name: &str) -> String {
    if max_depth <= 0 {
        String::new()
    } else {
        format!("WHERE {cte_name}._depth < {max_depth} ")
    }
}
