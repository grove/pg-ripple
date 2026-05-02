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
//! Uses PostgreSQL 18's `CYCLE` clause for O(1) membership checks.
//! PROPPATH-CYCLE-01 (v0.80.0): the CYCLE clause tracks both `s` and `o` so
//! that directed graphs with parallel edges (same s, same o via different
//! intermediate predicates) and self-loops are correctly detected as cycles:
//! ```sql
//! WITH RECURSIVE _path(s, o) AS (...)
//! CYCLE s, o SET _is_cycle USING _cycle_path
//! SELECT s, o FROM _path WHERE NOT _is_cycle
//! ```

use spargebra::algebra::PropertyPathExpression;
use spargebra::term::NamedNode;

use crate::dictionary;

/// Resolve a NamedNode IRI to its VP table expression returning `(s, o[, g])`.
/// When `graph_filter` is `Some(gid)`, the expression filters to triples in
/// graph `gid` — baked into the leaf so WITH RECURSIVE paths work correctly
/// inside `GRAPH <G> { }` (v0.40.0 fix).
/// When `include_g` is true, the graph column is included in the output so
/// that `GRAPH ?g { path }` can bind the graph variable (v0.41.x fix).
fn pred_table_expr(nn: &NamedNode, graph_filter: Option<i64>, include_g: bool) -> Option<String> {
    use pgrx::datum::DatumWithOid;
    use pgrx::prelude::*;

    let pred_id = dictionary::lookup_iri(nn.as_str())?;
    let g_cond = graph_filter
        .map(|gid| format!(" AND g = {gid}"))
        .unwrap_or_default();
    let g_sel = if include_g { ", g" } else { "" };

    // Check whether the predicate has a dedicated VP table.
    match Spi::get_one_with_args::<i64>(
        "SELECT table_oid::bigint FROM _pg_ripple.predicates WHERE id = $1",
        &[DatumWithOid::from(pred_id)],
    ) {
        Ok(Some(_oid)) => {
            if g_cond.is_empty() {
                Some(format!("SELECT s, o{g_sel} FROM _pg_ripple.vp_{pred_id}"))
            } else {
                Some(format!(
                    "SELECT s, o{g_sel} FROM _pg_ripple.vp_{pred_id} WHERE TRUE{g_cond}"
                ))
            }
        }
        Ok(None) => Some(format!(
            "SELECT s, o{g_sel} FROM _pg_ripple.vp_rare WHERE p = {pred_id}{g_cond}"
        )),
        Err(_) => None,
    }
}

/// Counter to make CTE names unique within a query.
pub struct PathCtx {
    /// P13-07 (v0.85.0): field is private; use `next_alias()` as the sole mutation point.
    counter: u32,
}

impl PathCtx {
    pub fn new(start: u32) -> Self {
        Self { counter: start }
    }

    /// Return the current counter value and advance it.
    pub fn next_alias(&mut self) -> u32 {
        self.next()
    }

    fn next(&mut self) -> u32 {
        let n = self.counter;
        self.counter += 1;
        n
    }

    /// Read the current counter value (e.g. to sync back to the parent context).
    pub fn value(&self) -> u32 {
        self.counter
    }
}

/// Compile a `PropertyPathExpression` to a SQL subquery that returns `(s, o[, g])`.
///
/// `s_filter` / `o_filter` are optional SQL integer expressions that, when
/// provided, are pushed into the anchor (for start node) or final filter
/// (for end node) to reduce CTE work-table size.
///
/// `graph_filter` — when `Some(gid)`, every leaf VP scan filters to `g = gid`
/// so that property paths inside `GRAPH <G> { }` stay within that named graph.
///
/// `include_g` — when `true`, includes a `g` column in the output (graph ID).
/// Required when translating `GRAPH ?g { path }` so that the graph variable
/// can be bound and sequence paths correctly restrict both hops to the same
/// named graph (v0.41.x).
///
/// Returns a SQL string representing an inline subquery `(SELECT s, o[, g] FROM ...)`.
pub fn compile_path(
    path: &PropertyPathExpression,
    s_filter: Option<&str>,
    o_filter: Option<&str>,
    ctx: &mut PathCtx,
    max_depth: i32,
    graph_filter: Option<i64>,
    include_g: bool,
) -> String {
    let g_sel = if include_g { ", g" } else { "" };
    match path {
        // ── Simple predicate (degenerate case) ──────────────────────────────
        PropertyPathExpression::NamedNode(nn) => {
            let iri = nn.as_str();
            let null_g = if include_g { ", NULL::bigint AS g" } else { "" };

            // v0.87.0 FUZZY-PATH-01: pg:confPath special IRI prefix.
            // Encoded as: <http://pg-ripple.org/conf_path/{predicate_iri}/{min_confidence}>
            // e.g. <http://pg-ripple.org/conf_path/http://example.org/relatedTo/0.7>
            if let Some(rest) = iri.strip_prefix("http://pg-ripple.org/conf_path/") {
                return compile_conf_path(
                    rest,
                    s_filter,
                    o_filter,
                    ctx,
                    max_depth,
                    graph_filter,
                    g_sel,
                );
            }

            let base = match pred_table_expr(nn, graph_filter, include_g) {
                Some(e) => e,
                None => format!("SELECT NULL::bigint AS s, NULL::bigint AS o{null_g} LIMIT 0"),
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
            format!("(SELECT s, o{g_sel} FROM ({base}) _pbase{where_clause})")
        }

        // ── Reverse: swap s and o ────────────────────────────────────────────
        PropertyPathExpression::Reverse(inner) => {
            // Swap s_filter and o_filter when descending.
            let inner_sql = compile_path(
                inner,
                o_filter,
                s_filter,
                ctx,
                max_depth,
                graph_filter,
                include_g,
            );
            format!(
                "(SELECT o AS s, s AS o{g_sel} FROM {inner_sql} _prev{})",
                ctx.next_alias()
            )
        }

        // ── Sequence: a/b → join on intermediate node ────────────────────────
        PropertyPathExpression::Sequence(left, right) => {
            let n = ctx.next_alias();
            // left returns (?x, ?mid[, ?g]); right returns (?mid, ?y[, ?g])
            let left_sql = compile_path(
                left,
                s_filter,
                None,
                ctx,
                max_depth,
                graph_filter,
                include_g,
            );
            let right_sql = compile_path(
                right,
                None,
                o_filter,
                ctx,
                max_depth,
                graph_filter,
                include_g,
            );
            if include_g {
                // Both hops must be in the SAME named graph.
                format!(
                    "(SELECT _lseq{n}.s, _rseq{n}.o, _lseq{n}.g AS g \
                     FROM {left_sql} AS _lseq{n} \
                     JOIN {right_sql} AS _rseq{n} ON _lseq{n}.o = _rseq{n}.s AND _lseq{n}.g = _rseq{n}.g)"
                )
            } else {
                format!(
                    "(SELECT _lseq{n}.s, _rseq{n}.o \
                     FROM {left_sql} AS _lseq{n} \
                     JOIN {right_sql} AS _rseq{n} ON _lseq{n}.o = _rseq{n}.s)"
                )
            }
        }

        // ── Alternative: a|b → UNION ALL ────────────────────────────────────
        PropertyPathExpression::Alternative(left, right) => {
            let left_sql = compile_path(
                left,
                s_filter,
                o_filter,
                ctx,
                max_depth,
                graph_filter,
                include_g,
            );
            let right_sql = compile_path(
                right,
                s_filter,
                o_filter,
                ctx,
                max_depth,
                graph_filter,
                include_g,
            );
            let n = ctx.next_alias();
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
                "(SELECT s, o{g_sel} FROM (\
                 SELECT s, o{g_sel} FROM {left_sql} _altL{n} \
                 UNION ALL \
                 SELECT s, o{g_sel} FROM {right_sql} _altR{n}\
                 ) _alt{n}{where_clause})"
            )
        }

        // ── OneOrMore (p+) ───────────────────────────────────────────────────
        PropertyPathExpression::OneOrMore(inner) => {
            let n = ctx.next_alias();
            let cte_name = format!("_opm{n}");
            let base_sql = compile_path(inner, None, None, ctx, max_depth, graph_filter, include_g);
            let depth_guard = depth_guard_clause(max_depth, &cte_name);
            let anchor_where = s_filter
                .map(|sf| format!(" WHERE _anchor{n}.s = {sf}"))
                .unwrap_or_default();
            let final_where = o_filter
                .map(|of| format!(" AND o = {of}"))
                .unwrap_or_default();
            if include_g {
                format!(
                    "(WITH RECURSIVE {cte_name}(s, o, g, _depth) AS (\
                     SELECT _anchor{n}.s, _anchor{n}.o, _anchor{n}.g, 1 \
                     FROM {base_sql} AS _anchor{n}{anchor_where} \
                     UNION ALL \
                     SELECT {cte_name}.s, _step{n}.o, {cte_name}.g, {cte_name}._depth + 1 \
                     FROM {cte_name} \
                     JOIN {base_sql} AS _step{n} ON {cte_name}.o = _step{n}.s AND {cte_name}.g = _step{n}.g \
                     {depth_guard}\
                     ) CYCLE s, o SET _is_cycle USING _cycle_path \
                     SELECT DISTINCT s, o, g FROM {cte_name} \
                     WHERE NOT _is_cycle{final_where})"
                )
            } else {
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
        }

        // ── ZeroOrMore (p*) ──────────────────────────────────────────────────
        PropertyPathExpression::ZeroOrMore(inner) => {
            let n = ctx.next_alias();
            let cte_name = format!("_zom{n}");
            let base_sql = compile_path(inner, None, None, ctx, max_depth, graph_filter, include_g);
            let depth_guard = depth_guard_clause(max_depth, &cte_name);

            // One-hop anchor: start from s_filter if given, otherwise from all.
            let one_hop_where = s_filter
                .map(|sf| format!(" WHERE s = {sf}"))
                .unwrap_or_default();

            if include_g {
                // With graph tracking: zero-hop rows carry their graph ID.
                let zero_hop = if let Some(sf) = s_filter {
                    // Constant start: emit (sf, sf, g) for each graph containing sf.
                    let all_nodes_g = build_all_nodes_sql(graph_filter, true);
                    format!(
                        "SELECT DISTINCT {sf} AS s, {sf} AS o, g, 0 AS _depth \
                             FROM ({all_nodes_g}) _sfg{n} WHERE node = {sf}"
                    )
                } else {
                    let all_nodes_g = build_all_nodes_sql(graph_filter, true);
                    let mut parts = vec![format!(
                        "SELECT DISTINCT node AS s, node AS o, g, 0 AS _depth \
                         FROM ({all_nodes_g}) AS _all0{n}"
                    )];
                    if let Some(of) = o_filter {
                        parts.push(format!(
                            "SELECT {of} AS s, {of} AS o, NULL::bigint AS g, 0 AS _depth"
                        ));
                    }
                    parts.join(" UNION ALL ")
                };

                let mut final_parts = Vec::new();
                if let Some(of) = o_filter {
                    final_parts.push(format!("o = {of}"));
                }
                if let Some(sf) = s_filter {
                    final_parts.push(format!("s = {sf}"));
                }
                let final_where = if final_parts.is_empty() {
                    String::new()
                } else {
                    format!(" AND {}", final_parts.join(" AND "))
                };

                format!(
                    "(WITH RECURSIVE {cte_name}(s, o, g, _depth) AS (\
                     SELECT _anc{n}.s, _anc{n}.o, _anc{n}.g, _anc{n}._depth \
                     FROM (\
                       SELECT s, o, g, 1 AS _depth FROM {base_sql} AS _b1{n}{one_hop_where} \
                       UNION ALL \
                       {zero_hop} \
                     ) AS _anc{n} \
                     UNION ALL \
                     SELECT {cte_name}.s, _step{n}.o, {cte_name}.g, {cte_name}._depth + 1 \
                     FROM {cte_name} \
                     JOIN {base_sql} AS _step{n} ON {cte_name}.o = _step{n}.s AND {cte_name}.g = _step{n}.g \
                     {depth_guard}\
                     ) CYCLE s, o SET _is_cycle USING _cycle_path \
                     SELECT DISTINCT s, o, g FROM {cte_name} \
                     WHERE NOT _is_cycle{final_where})"
                )
            } else {
                // Without graph tracking: existing behavior.
                // Zero-hop (reflexive) anchor:
                // - If s_filter is a constant: only emit (sf, sf).
                // - Otherwise: all nodes in the active graph, plus any constant o endpoint.
                let zero_hop = if let Some(sf) = s_filter {
                    format!("SELECT {sf} AS s, {sf} AS o, 0 AS _depth")
                } else {
                    let all_nodes = build_all_nodes_sql(graph_filter, false);
                    let mut parts = vec![format!(
                        "SELECT DISTINCT node AS s, node AS o, 0 AS _depth \
                         FROM ({all_nodes}) AS _all0{n}"
                    )];
                    if let Some(of) = o_filter {
                        parts.push(format!("SELECT {of} AS s, {of} AS o, 0 AS _depth"));
                    }
                    parts.join(" UNION ALL ")
                };

                let mut final_parts = Vec::new();
                if let Some(of) = o_filter {
                    final_parts.push(format!("o = {of}"));
                }
                if let Some(sf) = s_filter {
                    final_parts.push(format!("s = {sf}"));
                }
                let final_where = if final_parts.is_empty() {
                    String::new()
                } else {
                    format!(" AND {}", final_parts.join(" AND "))
                };

                format!(
                    "(WITH RECURSIVE {cte_name}(s, o, _depth) AS (\
                     SELECT _anc{n}.s, _anc{n}.o, _anc{n}._depth \
                     FROM (\
                       SELECT s, o, 1 AS _depth FROM {base_sql} AS _b1{n}{one_hop_where} \
                       UNION ALL \
                       {zero_hop} \
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
        }

        // ── ZeroOrOne (p?) ───────────────────────────────────────────────────
        PropertyPathExpression::ZeroOrOne(inner) => {
            let n = ctx.next_alias();
            let base_sql = compile_path(
                inner,
                s_filter,
                o_filter,
                ctx,
                max_depth,
                graph_filter,
                include_g,
            );

            // Zero-hop (reflexive) part:
            // - Constant start (s_filter): emit (sf, sf[, g]).
            // - Constant end (o_filter): emit (of, of[, g]).
            // - Both variable: all nodes in the active graph.
            let g_null = if include_g { ", NULL::bigint AS g" } else { "" };
            let zero_hop = match (s_filter, o_filter) {
                (Some(sf), _) => format!("SELECT {sf} AS s, {sf} AS o{g_null}"),
                (None, Some(of)) => format!("SELECT {of} AS s, {of} AS o{g_null}"),
                (None, None) => {
                    let all_nodes = build_all_nodes_sql(graph_filter, include_g);
                    if include_g {
                        format!(
                            "SELECT DISTINCT node AS s, node AS o, g \
                             FROM ({all_nodes}) AS _all1{n}"
                        )
                    } else {
                        format!(
                            "SELECT DISTINCT node AS s, node AS o \
                             FROM ({all_nodes}) AS _all1{n}"
                        )
                    }
                }
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
            format!(
                "(SELECT s, o{g_sel} FROM (\
                 SELECT s, o{g_sel} FROM {base_sql} AS _onepart{n} \
                 UNION \
                 {zero_hop}\
                 ) AS _zo{n}{where_clause})"
            )
        }

        // ── NegatedPropertySet !(p1|p2|...) ────────────────────────────────
        PropertyPathExpression::NegatedPropertySet(excluded) => {
            let n = ctx.next_alias();
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
            let g_cond = graph_filter
                .map(|gid| format!(" AND g = {gid}"))
                .unwrap_or_default();
            let mut conditions = Vec::new();
            if let Some(sf) = s_filter {
                conditions.push(format!("s = {sf}"));
            }
            if let Some(of) = o_filter {
                conditions.push(format!("o = {of}"));
            }
            let where_clause = if conditions.is_empty() {
                format!("WHERE TRUE{not_in_clause}{g_cond}")
            } else {
                format!("WHERE {}{not_in_clause}{g_cond}", conditions.join(" AND "))
            };

            let all_preds_union = build_all_predicates_with_p();

            format!("(SELECT s, o{g_sel} FROM ({all_preds_union}) _neg{n} {where_clause})")
        }
    }
}

/// Build a SQL expression that returns all distinct nodes (subjects and objects)
/// from the active graph. Used for zero-hop (reflexive) rows in `p*` and `p?`.
/// When `include_g` is true, returns `(node, g)` pairs; otherwise just `node`.
fn build_all_nodes_sql(graph_filter: Option<i64>, include_g: bool) -> String {
    use pgrx::prelude::*;
    let mut parts: Vec<String> = Vec::new();

    let g_cond = graph_filter
        .map(|gid| format!(" WHERE g = {gid}"))
        .unwrap_or_default();
    let g_sel = if include_g { ", g" } else { "" };

    // PROPPATH-UNBOUNDED-01 (v0.82.0): limit the number of predicates scanned
    // to avoid generating an unbounded UNION ALL on large schemas.
    let pred_limit = crate::ALL_NODES_PREDICATE_LIMIT.get().max(1) as i64;

    Spi::connect(|client| {
        let rows = client
            .select(
                "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL \
                 ORDER BY triple_count DESC NULLS LAST LIMIT $1",
                None,
                &[pgrx::datum::DatumWithOid::from(pred_limit)],
            )
            .unwrap_or_else(|e| pgrx::error!("all-nodes SPI error: {e}"));
        for row in rows {
            if let Ok(Some(pred_id)) = row.get::<i64>(1) {
                parts.push(format!(
                    "SELECT s AS node{g_sel} FROM _pg_ripple.vp_{pred_id}{g_cond}"
                ));
                parts.push(format!(
                    "SELECT o AS node{g_sel} FROM _pg_ripple.vp_{pred_id}{g_cond}"
                ));
            }
        }
    });

    // Warn if predicate count in DB exceeds the limit — the UNION ALL has been truncated.
    let total_pred_count: i64 = Spi::get_one::<i64>(
        "SELECT count(*)::bigint FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
    )
    .unwrap_or(None)
    .unwrap_or(0);
    if total_pred_count > pred_limit {
        pgrx::warning!(
            "pg_ripple: all-nodes property path limited to {} of {} predicates \
             (pg_ripple.all_nodes_predicate_limit); query may miss nodes",
            pred_limit,
            total_pred_count
        );
    }

    // Always include vp_rare.
    let rare_g = if g_cond.is_empty() {
        String::new()
    } else {
        g_cond.clone()
    };
    parts.push(format!(
        "SELECT s AS node{g_sel} FROM _pg_ripple.vp_rare{rare_g}"
    ));
    parts.push(format!(
        "SELECT o AS node{g_sel} FROM _pg_ripple.vp_rare{rare_g}"
    ));

    if parts.is_empty() {
        if include_g {
            "SELECT NULL::bigint AS node, NULL::bigint AS g LIMIT 0".to_owned()
        } else {
            "SELECT NULL::bigint AS node LIMIT 0".to_owned()
        }
    } else {
        parts.join(" UNION ALL ")
    }
}

/// Build a UNION ALL subquery over all predicates with a `(p, s, o)` projection.
/// Used by NegatedPropertySet to scan every predicate.
fn build_all_predicates_with_p() -> String {
    use pgrx::prelude::*;
    let mut branches: Vec<String> = Vec::new();

    // PROPPATH-UNBOUNDED-01 (v0.82.0): limit predicates in NegatedPropertySet scan.
    let pred_limit = crate::ALL_NODES_PREDICATE_LIMIT.get().max(1) as i64;

    Spi::connect(|client| {
        let rows = client
            .select(
                "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL \
                 ORDER BY triple_count DESC NULLS LAST LIMIT $1",
                None,
                &[pgrx::datum::DatumWithOid::from(pred_limit)],
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

// ─── v0.87.0 FUZZY-PATH-01: confidence-threshold property path ────────────────

/// Compile a `pg:confPath/predicate_iri/min_confidence` special IRI to a
/// confidence-filtered `WITH RECURSIVE … CYCLE` CTE.
///
/// `rest` is the portion after the `http://pg-ripple.org/conf_path/` prefix, in
/// the form `{predicate_iri}/{min_confidence}` (last path component is the float).
///
/// Edges without a confidence row in `_pg_ripple.confidence` are treated as
/// `confidence = 1.0` (explicit facts are always confident — CONF-GC-01c).
fn compile_conf_path(
    rest: &str,
    s_filter: Option<&str>,
    o_filter: Option<&str>,
    ctx: &mut PathCtx,
    max_depth: i32,
    graph_filter: Option<i64>,
    g_sel: &str,
) -> String {
    // Split off the last "/" component as the min_confidence threshold.
    let (pred_part, threshold_str) = match rest.rfind('/') {
        Some(pos) => (&rest[..pos], &rest[pos + 1..]),
        None => (rest, ""),
    };

    let min_conf: f64 = threshold_str
        .parse()
        .unwrap_or_else(|_| crate::DEFAULT_FUZZY_THRESHOLD.get());

    // Resolve the predicate IRI to a VP table expression.
    let pred_id = crate::dictionary::lookup_iri(pred_part);

    let vp_expr = match pred_id {
        Some(id) => {
            let g_cond = graph_filter
                .map(|gid| format!(" AND vp.g = {gid}"))
                .unwrap_or_default();
            // Check for dedicated VP table.
            let has_dedicated: bool = pgrx::Spi::get_one_with_args::<i64>(
                "SELECT table_oid::bigint FROM _pg_ripple.predicates WHERE id = $1",
                &[pgrx::datum::DatumWithOid::from(id)],
            )
            .map(|v| v.is_some())
            .unwrap_or(false);
            if has_dedicated {
                format!(
                    "SELECT vp.s, vp.o, vp.i AS _sid FROM _pg_ripple.vp_{id} vp WHERE 1=1{g_cond}"
                )
            } else {
                format!(
                    "SELECT vp.s, vp.o, vp.i AS _sid FROM _pg_ripple.vp_rare vp \
                     WHERE vp.p = {id}{g_cond}"
                )
            }
        }
        None => {
            // Unknown predicate — return empty result.
            return format!("(SELECT NULL::bigint AS s, NULL::bigint AS o{g_sel} LIMIT 0)");
        }
    };

    let n = ctx.next_alias();
    let depth_guard = if max_depth > 0 {
        format!("AND _cp{n}._depth < {max_depth}")
    } else {
        String::new()
    };

    let s_anchor_cond = s_filter
        .map(|sf| format!("AND edge_anchor.s = {sf}"))
        .unwrap_or_default();
    let o_final_cond = o_filter
        .map(|of| format!("AND NOT _cp{n}._is_cycle AND _cp{n}.o = {of}"))
        .unwrap_or_default();

    // Confidence filter: edges without a row are treated as confidence = 1.0.
    let conf_cond = format!(
        "COALESCE((\
           SELECT MAX(c.confidence) FROM _pg_ripple.confidence c \
           WHERE c.statement_id = edge.\"_sid\"\
         ), 1.0) >= {min_conf}"
    );

    format!(
        "(WITH RECURSIVE _cp{n}(s, o, _depth) AS (\
           SELECT edge.s, edge.o, 1 \
           FROM ({vp_expr}) edge \
           WHERE {conf_cond} {s_anchor_cond} \
           UNION ALL \
           SELECT _cp{n}.s, edge2.o, _cp{n}._depth + 1 \
           FROM _cp{n} \
           JOIN ({vp_expr}) edge2 ON edge2.s = _cp{n}.o \
           WHERE {conf_cond2} {depth_guard} \
         ) CYCLE s, o SET _is_cycle USING _cp{n}_path \
         SELECT s, o{g_sel} FROM _cp{n} \
         WHERE NOT _is_cycle {o_final_cond}\
        )",
        conf_cond2 = format!(
            "COALESCE((\
               SELECT MAX(c.confidence) FROM _pg_ripple.confidence c \
               WHERE c.statement_id = edge2.\"_sid\"\
             ), 1.0) >= {min_conf}"
        ),
    )
}
