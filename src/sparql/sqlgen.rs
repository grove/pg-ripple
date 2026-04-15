//! SPARQL algebra → SQL translation.
//!
//! Translates a `spargebra` `GraphPattern` (after sparopt optimization) into a
//! SQL SELECT string.  All IRI/literal constants are encoded to `i64` before
//! appearing in SQL — no raw strings ever reach the generated query.
//!
//! # Supported algebra nodes (v0.5.0)
//!
//! - `Bgp` — basic graph patterns  → flat JOIN across VP tables
//! - `Path` — property path        → WITH RECURSIVE CTE (see property_path.rs)
//! - `Join` — AND of two patterns   → merge fragments (implicit cross join)
//! - `LeftJoin` — OPTIONAL          → SQL LEFT JOIN with a subquery
//! - `Union` — UNION               → SQL UNION
//! - `Minus` — MINUS               → SQL EXCEPT
//! - `Filter` — WHERE condition      → SQL WHERE clause (or HAVING for Group)
//! - `Graph` — GRAPH ?g / GRAPH <G> → filter on `g` column
//! - `Group` — aggregates / GROUP BY → SQL GROUP BY + aggregate functions
//! - `Extend` — BIND               → computed column alias
//! - `Values` — VALUES inline data → SQL VALUES clause
//! - `Project` — SELECT columns       → restrict output columns
//! - `Distinct` — DISTINCT            → SQL DISTINCT
//! - `Reduced` — treated same as Distinct for simplicity
//! - `Slice` — LIMIT / OFFSET
//! - `OrderBy` — ORDER BY

use std::collections::HashMap;

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use spargebra::algebra::{
    AggregateExpression, AggregateFunction, Expression, GraphPattern, OrderExpression,
};
use spargebra::term::{GroundTerm, Literal, NamedNodePattern, TermPattern};

use super::property_path::{PathCtx, compile_path};
use crate::dictionary;

// ─── VP table resolution ─────────────────────────────────────────────────────

/// How a predicate's triples are physically stored.
enum VpSource {
    /// Dedicated table, e.g. `_pg_ripple.vp_1234`.
    Dedicated(String),
    /// Stored in the shared `vp_rare` table with predicate filter `p = {id}`.
    Rare(i64),
    /// Predicate never stored — table expression yields 0 rows.
    Empty,
}

/// Resolve how to access triples for `pred_id`.
fn vp_source(pred_id: i64) -> VpSource {
    // Query without the IS NOT NULL filter so we get a row even when table_oid is NULL.
    // pgrx 0.17 returns Err(InvalidPosition) for 0-row results, Ok(None) for a NULL column.
    match Spi::get_one_with_args::<i64>(
        "SELECT table_oid::bigint FROM _pg_ripple.predicates WHERE id = $1",
        &[DatumWithOid::from(pred_id)],
    ) {
        Ok(Some(_oid)) => VpSource::Dedicated(format!("_pg_ripple.vp_{pred_id}")),
        Ok(None) => VpSource::Rare(pred_id),
        Err(_) => VpSource::Empty,
    }
}

/// Build a SQL table expression for one triple pattern (exposing `s`, `o`, `g`).
fn table_expr(src: &VpSource) -> String {
    match src {
        VpSource::Dedicated(name) => name.clone(),
        VpSource::Rare(p) => {
            format!("(SELECT s, o, g FROM _pg_ripple.vp_rare WHERE p = {p})")
        }
        VpSource::Empty => {
            "(SELECT NULL::bigint AS s, NULL::bigint AS o, NULL::bigint AS g LIMIT 0)".to_owned()
        }
    }
}

/// Build a UNION ALL subquery that covers every predicate — both dedicated VP
/// tables and `vp_rare`.  Each branch projects `(p, s, o, g)` so the caller
/// can bind the predicate variable.
///
/// The list of dedicated VP tables is fetched from `_pg_ripple.predicates` at
/// query-translation time via SPI.
fn build_all_predicates_union() -> String {
    let mut branches: Vec<String> = Vec::new();

    // Collect dedicated VP table predicate IDs.
    Spi::connect(|client| {
        let rows = client
            .select(
                "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("variable-predicate SPI error: {e}"));
        for row in rows {
            if let Ok(Some(pred_id)) = row.get::<i64>(1) {
                branches.push(format!(
                    "SELECT {pred_id}::bigint AS p, s, o, g FROM _pg_ripple.vp_{pred_id}"
                ));
            }
        }
    });

    // Always include vp_rare (it already has a `p` column).
    branches.push("SELECT p, s, o, g FROM _pg_ripple.vp_rare".to_owned());

    branches.join(" UNION ALL ")
}

// ─── Translation context ─────────────────────────────────────────────────────

/// Mutable state carried through recursive translation.
struct Ctx {
    alias_counter: u32,
    #[allow(dead_code)]
    opt_counter: u32,
    path_counter: u32,
    /// Per-query IRI/literal encoding cache — avoids repeated SPI look-ups.
    per_query: HashMap<String, Option<i64>>,
    /// Variables that hold raw SQL integers (COUNT, SUM, etc. aggregate outputs).
    /// FILTER constants compared against these must stay as raw SQL values,
    /// not be re-encoded as inline IDs.
    raw_numeric_vars: std::collections::HashSet<String>,
}

impl Ctx {
    fn new() -> Self {
        Self {
            alias_counter: 0,
            opt_counter: 0,
            path_counter: 0,
            per_query: HashMap::new(),
            raw_numeric_vars: std::collections::HashSet::new(),
        }
    }

    fn next_alias(&mut self) -> String {
        let n = self.alias_counter;
        self.alias_counter += 1;
        format!("_t{n}")
    }

    #[allow(dead_code)]
    fn next_opt(&mut self) -> String {
        let n = self.opt_counter;
        self.opt_counter += 1;
        format!("_opt{n}")
    }

    /// Encode an IRI to a dictionary id (read-only lookup; no insert).
    /// Returns `None` if the IRI has never been stored.
    fn encode_iri(&mut self, iri: &str) -> Option<i64> {
        if let Some(cached) = self.per_query.get(iri) {
            return *cached;
        }
        let id = dictionary::lookup_iri(iri);
        self.per_query.insert(iri.to_owned(), id);
        id
    }

    /// Encode a `spargebra::Literal` to a dictionary id (may insert).
    fn encode_literal(&mut self, lit: &Literal) -> i64 {
        let lang = lit.language();
        let value = lit.value();
        let dt = lit.datatype().as_str();

        if let Some(l) = lang {
            dictionary::encode_lang_literal(value, l)
        } else if dt == "http://www.w3.org/2001/XMLSchema#string"
            || dt == "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
        {
            dictionary::encode(value, dictionary::KIND_LITERAL)
        } else {
            dictionary::encode_typed_literal(value, dt)
        }
    }
}

// ─── Fragment ─────────────────────────────────────────────────────────────────

/// A SQL query fragment accumulating table joins, conditions, and variable bindings.
struct Fragment {
    /// FROM clause items: (alias, table expression).
    from_items: Vec<(String, String)>,
    /// WHERE conditions (logical AND).
    conditions: Vec<String>,
    /// SPARQL variable name → SQL column or expression.
    bindings: HashMap<String, String>,
}

impl Fragment {
    fn empty() -> Self {
        Self {
            from_items: vec![],
            conditions: vec![],
            bindings: HashMap::new(),
        }
    }

    /// Merge `other` into `self`, adding equality conditions for shared variables.
    fn merge(&mut self, other: Fragment) {
        for (alias, tbl) in other.from_items {
            self.from_items.push((alias, tbl));
        }
        for cond in other.conditions {
            self.conditions.push(cond);
        }
        for (var, col) in other.bindings {
            if let Some(existing) = self.bindings.get(&var) {
                // Variable already bound — equijoin.
                self.conditions.push(format!("{} = {}", col, existing));
            } else {
                self.bindings.insert(var, col);
            }
        }
    }

    fn build_from(&self) -> String {
        if self.from_items.is_empty() {
            // Return a dummy that produces one row (for ASK on empty patterns).
            return "(SELECT 1) _dummy".to_owned();
        }
        self.from_items
            .iter()
            .map(|(alias, tbl)| format!("{tbl} AS {alias}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn build_where(&self) -> String {
        if self.conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", self.conditions.join(" AND "))
        }
    }

    /// Render as a subquery SELECT for all bound variables.
    #[allow(dead_code)]
    fn as_subquery(&self, prefix: &str) -> String {
        if self.bindings.is_empty() {
            return format!(
                "(SELECT 1 AS _dummy_col FROM {} {})",
                self.build_from(),
                self.build_where()
            );
        }
        let cols = self
            .bindings
            .iter()
            .map(|(v, col)| format!("{col} AS {prefix}_{v}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "(SELECT {cols} FROM {} {})",
            self.build_from(),
            self.build_where()
        )
    }
}

// ─── TermPattern → SQL column ─────────────────────────────────────────────────

/// Try to evaluate a `TermPattern` as a ground constant (i64 dictionary ID).
/// Returns `None` if the pattern contains free variables.
fn ground_term_id(term: &TermPattern, ctx: &mut Ctx) -> Option<i64> {
    match term {
        TermPattern::NamedNode(nn) => ctx.encode_iri(nn.as_str()),
        TermPattern::Literal(lit) => Some(ctx.encode_literal(lit)),
        TermPattern::Triple(inner) => {
            let s_id = ground_term_id(&inner.subject, ctx)?;
            let p_id = match &inner.predicate {
                NamedNodePattern::NamedNode(nn) => ctx.encode_iri(nn.as_str())?,
                NamedNodePattern::Variable(_) => return None,
            };
            let o_id = ground_term_id(&inner.object, ctx)?;
            dictionary::lookup_quoted_triple(s_id, p_id, o_id)
        }
        TermPattern::Variable(_) | TermPattern::BlankNode(_) => None,
    }
}

/// Bind one end of a triple (subject or object) to the translation context.
/// Returns an optional SQL equality condition if the term is a constant.
fn bind_term(
    alias: &str,
    col: &str, // "s" or "o"
    term: &TermPattern,
    ctx: &mut Ctx,
    bindings: &mut HashMap<String, String>,
    conditions: &mut Vec<String>,
) {
    let col_expr = format!("{alias}.{col}");
    match term {
        TermPattern::Variable(v) => {
            let vname = v.as_str().to_owned();
            if let Some(existing) = bindings.get(&vname) {
                // Variable already bound → equijoin.
                conditions.push(format!("{col_expr} = {existing}"));
            } else {
                bindings.insert(vname, col_expr);
            }
        }
        TermPattern::NamedNode(nn) => match ctx.encode_iri(nn.as_str()) {
            Some(id) => conditions.push(format!("{col_expr} = {id}")),
            None => conditions.push("FALSE".to_owned()),
        },
        TermPattern::Literal(lit) => {
            let id = ctx.encode_literal(lit);
            conditions.push(format!("{col_expr} = {id}"));
        }
        TermPattern::BlankNode(bnode) => {
            // spargebra uses anonymous blank nodes as intermediate variables for
            // property path sequences (e.g. `p/q` → two BGP patterns sharing a
            // blank-node object/subject).  Treat them just like SPARQL variables:
            // bind on first occurrence, equijoin on subsequent occurrences.
            let vname = format!("_bn_{}", bnode);
            if let Some(existing) = bindings.get(&vname) {
                conditions.push(format!("{col_expr} = {existing}"));
            } else {
                bindings.insert(vname, col_expr);
            }
        }
        TermPattern::Triple(_) => {
            // Quoted triple pattern — try to evaluate as a ground constant.
            match ground_term_id(term, ctx) {
                Some(id) => conditions.push(format!("{col_expr} = {id}")),
                None => {
                    // Variable-inside-quoted-triple requires dictionary scan;
                    // not supported in v0.4.0.
                    pgrx::warning!(
                        "SPARQL-star: variable inside quoted triple pattern is not yet supported; \
                         pattern treated as no-match"
                    );
                    conditions.push("FALSE".to_owned());
                }
            }
        }
    }
}

// ─── Core graph-pattern translator ───────────────────────────────────────────

fn translate_bgp(patterns: &[spargebra::term::TriplePattern], ctx: &mut Ctx) -> Fragment {
    let mut frag = Fragment::empty();

    // Self-join elimination: detect duplicate triple patterns, only scan once.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for tp in patterns {
        let key = format!("{tp}");
        if !seen.insert(key) {
            continue;
        }

        let alias = ctx.next_alias();

        // --- Predicate ---
        let (pred_conditions, source) = match &tp.predicate {
            NamedNodePattern::NamedNode(nn) => {
                match ctx.encode_iri(nn.as_str()) {
                    None => {
                        // Predicate not in dictionary → no result rows.
                        let src = VpSource::Empty;
                        (vec![], src)
                    }
                    Some(id) => {
                        let src = vp_source(id);
                        (vec![], src)
                    }
                }
            }
            NamedNodePattern::Variable(v) => {
                // Unbound predicate: build UNION ALL of every dedicated VP table
                // plus vp_rare so that all predicates are covered.
                let vname = v.as_str().to_owned();
                let a = alias.clone();
                let union_subquery = build_all_predicates_union();
                frag.from_items
                    .push((a.clone(), format!("({union_subquery})")));
                if let Some(existing) = frag.bindings.get(&vname) {
                    frag.conditions.push(format!("{a}.p = {existing}"));
                } else {
                    frag.bindings.insert(vname, format!("{a}.p"));
                }
                bind_term(
                    &a,
                    "s",
                    &tp.subject,
                    ctx,
                    &mut frag.bindings,
                    &mut frag.conditions,
                );
                bind_term(
                    &a,
                    "o",
                    &tp.object,
                    ctx,
                    &mut frag.bindings,
                    &mut frag.conditions,
                );
                continue;
            }
        };

        let tbl = table_expr(&source);
        frag.from_items.push((alias.clone(), tbl));
        for c in pred_conditions {
            frag.conditions.push(c);
        }

        bind_term(
            &alias,
            "s",
            &tp.subject,
            ctx,
            &mut frag.bindings,
            &mut frag.conditions,
        );
        bind_term(
            &alias,
            "o",
            &tp.object,
            ctx,
            &mut frag.bindings,
            &mut frag.conditions,
        );
    }

    frag
}

fn translate_pattern(pattern: &GraphPattern, ctx: &mut Ctx) -> Fragment {
    match pattern {
        GraphPattern::Bgp { patterns } => translate_bgp(patterns, ctx),

        GraphPattern::Join { left, right } => {
            let mut frag = translate_pattern(left, ctx);
            let right_frag = translate_pattern(right, ctx);
            frag.merge(right_frag);
            frag
        }

        GraphPattern::LeftJoin {
            left,
            right,
            expression,
        } => {
            let left_frag = translate_pattern(left, ctx);
            let mut right_frag = translate_pattern(right, ctx);

            // Add the OPTIONAL filter expression to the right fragment, if any.
            if let Some(expr) = expression
                && let Some(cond) = translate_expr(expr, &right_frag.bindings, ctx)
            {
                right_frag.conditions.push(cond);
            }

            // Shared variables (present in both sides).
            let shared_vars: Vec<String> = left_frag
                .bindings
                .keys()
                .filter(|v| right_frag.bindings.contains_key(*v))
                .cloned()
                .collect();

            // Build left subquery with safe unqualified column aliases (_lc_<v>).
            let lft = ctx.next_alias();
            let left_select_parts: Vec<String> = left_frag
                .bindings
                .iter()
                .map(|(v, col)| format!("{col} AS _lc_{v}"))
                .collect();
            let left_select = if left_select_parts.is_empty() {
                "1 AS _lc_dummy".to_owned()
            } else {
                left_select_parts.join(", ")
            };
            let left_subq = format!(
                "(SELECT {left_select} FROM {} {})",
                left_frag.build_from(),
                left_frag.build_where()
            );

            // Build right subquery with safe unqualified column aliases (_rc_<v>).
            let rgt = ctx.next_alias();
            let right_select_parts: Vec<String> = right_frag
                .bindings
                .iter()
                .map(|(v, col)| format!("{col} AS _rc_{v}"))
                .collect();
            let right_select = if right_select_parts.is_empty() {
                "1 AS _rc_dummy".to_owned()
            } else {
                right_select_parts.join(", ")
            };
            let right_subq = format!(
                "(SELECT {right_select} FROM {} {})",
                right_frag.build_from(),
                right_frag.build_where()
            );

            // ON clause using safe aliases.
            let on_clause = if shared_vars.is_empty() {
                "ON TRUE".to_owned()
            } else {
                format!(
                    "ON {}",
                    shared_vars
                        .iter()
                        .map(|v| format!("{lft}._lc_{v} = {rgt}._rc_{v}"))
                        .collect::<Vec<_>>()
                        .join(" AND ")
                )
            };

            // Combined SELECT: left vars (always), right-only vars (nullable).
            let mut combined_cols: Vec<String> = left_frag
                .bindings
                .keys()
                .map(|v| format!("{lft}._lc_{v} AS _lj_{v}"))
                .collect();
            for v in right_frag.bindings.keys() {
                if !left_frag.bindings.contains_key(v) {
                    combined_cols.push(format!("{rgt}._rc_{v} AS _lj_{v}"));
                }
            }
            let combined_select = if combined_cols.is_empty() {
                "1 AS _dummy".to_owned()
            } else {
                combined_cols.join(", ")
            };

            let lj = ctx.next_alias();
            let lj_sql = format!(
                "(SELECT {combined_select} \
                 FROM {left_subq} AS {lft} \
                 LEFT JOIN {right_subq} AS {rgt} {on_clause})"
            );

            let mut frag = Fragment::empty();
            frag.from_items.push((lj.clone(), lj_sql));

            for v in left_frag.bindings.keys() {
                frag.bindings.insert(v.clone(), format!("{lj}._lj_{v}"));
            }
            for v in right_frag.bindings.keys() {
                if !left_frag.bindings.contains_key(v) {
                    frag.bindings.insert(v.clone(), format!("{lj}._lj_{v}"));
                }
            }

            frag
        }

        GraphPattern::Filter { expr, inner } => {
            // Special case: Filter wrapping Group → HAVING clause.
            if let GraphPattern::Group {
                inner: group_inner,
                variables,
                aggregates,
            } = inner.as_ref()
            {
                return translate_group(group_inner, variables, aggregates, Some(expr), ctx);
            }
            let mut frag = translate_pattern(inner, ctx);
            if let Some(cond) = translate_expr(expr, &frag.bindings, ctx) {
                frag.conditions.push(cond);
            }
            frag
        }

        GraphPattern::Graph { name, inner } => {
            let mut frag = translate_pattern(inner, ctx);
            // Add graph filter to all tables that expose a `g` column.
            match name {
                NamedNodePattern::NamedNode(nn) => {
                    match ctx.encode_iri(nn.as_str()) {
                        Some(gid) => {
                            // Apply g = gid to every table alias in the fragment.
                            for (alias, _) in &frag.from_items {
                                frag.conditions.push(format!("{alias}.g = {gid}"));
                            }
                        }
                        None => {
                            frag.conditions.push("FALSE".to_owned());
                        }
                    }
                }
                NamedNodePattern::Variable(v) => {
                    let vname = v.as_str().to_owned();
                    if let Some((alias, _)) = frag.from_items.first() {
                        let col = format!("{alias}.g");
                        if let Some(existing) = frag.bindings.get(&vname) {
                            frag.conditions.push(format!("{col} = {existing}"));
                        } else {
                            frag.bindings.insert(vname, col);
                        }
                    }
                }
            }
            frag
        }

        // Modifiers are peeled off by translate_query — these are fall-throughs
        // for when they appear in nested positions.
        GraphPattern::Project { inner, variables } => {
            let mut frag = translate_pattern(inner, ctx);
            let var_set: std::collections::HashSet<String> =
                variables.iter().map(|v| v.as_str().to_owned()).collect();
            frag.bindings.retain(|v, _| var_set.contains(v));
            frag
        }
        GraphPattern::Distinct { inner } | GraphPattern::Reduced { inner } => {
            translate_pattern(inner, ctx)
        }
        GraphPattern::Slice { inner, .. } | GraphPattern::OrderBy { inner, .. } => {
            translate_pattern(inner, ctx)
        }

        // ── Property path (p+, p*, p?, p/q, p|q, ^p, !(p)) ────────────────────
        GraphPattern::Path {
            subject,
            path,
            object,
        } => {
            let max_depth = crate::MAX_PATH_DEPTH.get();
            let mut path_ctx = PathCtx::new(ctx.path_counter);

            // Determine bound constants for subject / object to push into the CTE.
            let s_const: Option<String> = ground_term_id(subject, ctx).map(|id| id.to_string());
            let o_const: Option<String> = ground_term_id(object, ctx).map(|id| id.to_string());

            let path_sql = compile_path(
                path,
                s_const.as_deref(),
                o_const.as_deref(),
                &mut path_ctx,
                max_depth,
            );
            ctx.path_counter = path_ctx.counter;

            let alias = ctx.next_alias();
            let mut frag = Fragment::empty();
            frag.from_items.push((alias.clone(), path_sql));

            // Bind subject variable if free.
            match subject {
                TermPattern::Variable(v) => {
                    let vname = v.as_str().to_owned();
                    let col = format!("{alias}.s");
                    if let Some(existing) = frag.bindings.get(&vname) {
                        frag.conditions.push(format!("{col} = {existing}"));
                    } else {
                        frag.bindings.insert(vname, col);
                    }
                }
                TermPattern::NamedNode(nn) => {
                    if s_const.is_none() {
                        // Predicate not in dictionary → no rows
                        frag.conditions.push("FALSE".to_owned());
                    } else {
                        // Already filtered inside path SQL
                        let _ = nn;
                    }
                }
                _ => {}
            }

            // Bind object variable if free.
            match object {
                TermPattern::Variable(v) => {
                    let vname = v.as_str().to_owned();
                    let col = format!("{alias}.o");
                    if let Some(existing) = frag.bindings.get(&vname) {
                        frag.conditions.push(format!("{col} = {existing}"));
                    } else {
                        frag.bindings.insert(vname, col);
                    }
                }
                TermPattern::NamedNode(nn) => {
                    if o_const.is_none() {
                        frag.conditions.push("FALSE".to_owned());
                    } else {
                        let _ = nn;
                    }
                }
                _ => {}
            }

            frag
        }

        // ── UNION ────────────────────────────────────────────────────────────
        GraphPattern::Union { left, right } => translate_union(left, right, ctx),

        // ── MINUS (EXCEPT) ──────────────────────────────────────────────────
        GraphPattern::Minus { left, right } => translate_minus(left, right, ctx),

        // ── GROUP BY / Aggregates ────────────────────────────────────────────
        GraphPattern::Group {
            inner,
            variables,
            aggregates,
        } => translate_group(inner, variables, aggregates, None, ctx),

        // ── BIND (Extend) ────────────────────────────────────────────────────
        GraphPattern::Extend {
            inner,
            variable,
            expression,
        } => {
            let mut frag = translate_pattern(inner, ctx);
            // Use translate_expr_value first so Variable references are bound to
            // their raw SQL column (not the boolean `IS NOT NULL` wrapper that
            // translate_expr produces). This is critical for COUNT/SUM aggregate
            // results re-bound via Extend (e.g. `SELECT (COUNT(?p) AS ?cnt)`).
            let sql_expr = translate_expr_value(expression, &frag.bindings, ctx)
                .or_else(|| translate_expr(expression, &frag.bindings, ctx));
            if let Some(expr) = sql_expr {
                frag.bindings.insert(variable.as_str().to_owned(), expr);
            }
            // If the expression is a simple variable reference to a raw_numeric
            // variable (e.g. the internal aggregate hash mapped to user-facing
            // ?cnt via Extend), propagate the raw_numeric status to the new name.
            if let Expression::Variable(src_var) = expression
                && ctx.raw_numeric_vars.contains(src_var.as_str())
            {
                ctx.raw_numeric_vars.insert(variable.as_str().to_owned());
            }
            frag
        }

        // ── VALUES ───────────────────────────────────────────────────────────
        GraphPattern::Values {
            variables,
            bindings,
        } => translate_values(variables, bindings, ctx),

        other => {
            // Return empty fragment with FALSE for unsupported patterns.
            let mut frag = Fragment::empty();
            frag.conditions.push("FALSE".to_owned());
            let _ = other; // silence unused var warning
            frag
        }
    }
}

// ─── UNION translator ─────────────────────────────────────────────────────────

/// Translate UNION to SQL UNION of two subqueries.
/// Both sides must expose the same set of variables; missing variables are NULL.
fn translate_union(left: &GraphPattern, right: &GraphPattern, ctx: &mut Ctx) -> Fragment {
    let left_frag = translate_pattern(left, ctx);
    let right_frag = translate_pattern(right, ctx);

    // Union of variable sets — each side may have different variables.
    let mut all_vars: Vec<String> = left_frag
        .bindings
        .keys()
        .chain(right_frag.bindings.keys())
        .cloned()
        .collect::<std::collections::HashSet<String>>()
        .into_iter()
        .collect();
    all_vars.sort();

    let build_union_arm = |frag: &Fragment| -> String {
        let cols: Vec<String> = all_vars
            .iter()
            .map(|v| {
                frag.bindings
                    .get(v)
                    .map(|col| format!("{col} AS _u_{v}"))
                    .unwrap_or_else(|| format!("NULL::bigint AS _u_{v}"))
            })
            .collect();
        let select_list = if cols.is_empty() {
            "1 AS _dummy".to_owned()
        } else {
            cols.join(", ")
        };
        format!(
            "SELECT {select_list} FROM {} {}",
            frag.build_from(),
            frag.build_where()
        )
    };

    let left_sql = build_union_arm(&left_frag);
    let right_sql = build_union_arm(&right_frag);

    let alias = ctx.next_alias();
    let union_subquery = format!("(({left_sql}) UNION ({right_sql}))");

    let mut frag = Fragment::empty();
    frag.from_items.push((alias.clone(), union_subquery));

    for v in &all_vars {
        frag.bindings.insert(v.clone(), format!("{alias}._u_{v}"));
    }

    frag
}

// ─── MINUS translator ────────────────────────────────────────────────────────

/// Translate MINUS to SQL EXCEPT.
fn translate_minus(left: &GraphPattern, right: &GraphPattern, ctx: &mut Ctx) -> Fragment {
    let left_frag = translate_pattern(left, ctx);
    let right_frag = translate_pattern(right, ctx);

    // SPARQL MINUS excludes left rows that have a compatible match in right.
    // Shared variables determine compatibility.
    let shared_vars: Vec<String> = left_frag
        .bindings
        .keys()
        .filter(|v| right_frag.bindings.contains_key(*v))
        .cloned()
        .collect();

    let alias = ctx.next_alias();

    if shared_vars.is_empty() {
        // No shared variables → MINUS has no effect (return left unchanged).
        return left_frag;
    }

    // Build left SELECT with shared columns.
    let left_cols: Vec<String> = shared_vars
        .iter()
        .map(|v| {
            let col = left_frag.bindings.get(v).unwrap();
            format!("{col} AS _m_{v}")
        })
        .collect();
    let left_all_cols: Vec<String> = left_frag
        .bindings
        .iter()
        .map(|(v, col)| format!("{col} AS _ma_{v}"))
        .collect();

    let right_cols: Vec<String> = shared_vars
        .iter()
        .map(|v| {
            let col = right_frag.bindings.get(v).unwrap();
            format!("{col} AS _m_{v}")
        })
        .collect();

    // Strategy: LEFT JOIN with right, keep rows where right side is null.
    let left_sql = format!(
        "SELECT {}, {} FROM {} {}",
        left_all_cols.join(", "),
        left_cols.join(", "),
        left_frag.build_from(),
        left_frag.build_where()
    );
    let right_sql = format!(
        "SELECT {} FROM {} {}",
        right_cols.join(", "),
        right_frag.build_from(),
        right_frag.build_where()
    );

    let on_clause: String = shared_vars
        .iter()
        .map(|v| format!("_lminus._m_{v} = _rminus._m_{v}"))
        .collect::<Vec<_>>()
        .join(" AND ");

    let minus_sql = format!(
        "(SELECT {lout} FROM ({left_sql}) AS _lminus \
         LEFT JOIN ({right_sql}) AS _rminus ON {on_clause} \
         WHERE {null_check})",
        lout = left_frag
            .bindings
            .keys()
            .map(|v| format!("_lminus._ma_{v} AS _mn_{v}"))
            .collect::<Vec<_>>()
            .join(", "),
        null_check = shared_vars
            .iter()
            .map(|v| format!("_rminus._m_{v} IS NULL"))
            .collect::<Vec<_>>()
            .join(" AND ")
    );

    let mut frag = Fragment::empty();
    frag.from_items.push((alias.clone(), minus_sql));
    for v in left_frag.bindings.keys() {
        frag.bindings.insert(v.clone(), format!("{alias}._mn_{v}"));
    }
    frag
}

// ─── GROUP BY / Aggregate translator ──────────────────────────────────────────

/// Translate a GROUP pattern (with optional HAVING expression) to SQL GROUP BY.
fn translate_group(
    inner: &GraphPattern,
    group_vars: &[spargebra::term::Variable],
    aggregates: &[(spargebra::term::Variable, AggregateExpression)],
    having: Option<&Expression>,
    ctx: &mut Ctx,
) -> Fragment {
    let inner_frag = translate_pattern(inner, ctx);

    // Build inner SQL with safe unqualified column aliases (_gi_<v>) so the
    // outer GROUP BY and aggregate expressions can reference them without
    // table-qualified names that become invalid inside a subquery wrapper.
    let inner_select_parts: Vec<String> = inner_frag
        .bindings
        .iter()
        .map(|(v, col)| format!("{col} AS _gi_{v}"))
        .collect();
    let inner_select = if inner_select_parts.is_empty() {
        "1 AS _gi_dummy".to_owned()
    } else {
        inner_select_parts.join(", ")
    };
    let inner_sql = format!(
        "SELECT {inner_select} FROM {} {}",
        inner_frag.build_from(),
        inner_frag.build_where()
    );

    // Build alias lookup: variable name → safe alias in _grp_inner.
    let inner_alias: HashMap<String, String> = inner_frag
        .bindings
        .keys()
        .map(|v| (v.clone(), format!("_gi_{v}")))
        .collect();

    // Map group variables to their safe aliases.
    let group_cols: Vec<(String, String)> = group_vars
        .iter()
        .filter_map(|v| {
            inner_alias
                .get(v.as_str())
                .map(|alias| (v.as_str().to_owned(), alias.clone()))
        })
        .collect();

    // Build SELECT list: group-by columns + aggregate expressions.
    let mut select_parts: Vec<String> = group_cols
        .iter()
        .map(|(v, alias)| format!("{alias} AS _g_{v}"))
        .collect();

    let mut agg_bindings: Vec<(String, String)> = Vec::new();
    for (agg_var, agg_expr) in aggregates {
        let sql_agg = translate_aggregate(agg_expr, &inner_alias);
        let vname = agg_var.as_str().to_owned();
        select_parts.push(format!("{sql_agg} AS _g_{vname}"));
        agg_bindings.push((vname, sql_agg));
    }

    let group_by_clause = if group_cols.is_empty() {
        String::new()
    } else {
        format!(
            "GROUP BY {}",
            group_cols
                .iter()
                .map(|(_, alias)| alias.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    // HAVING clause (from Filter wrapping Group in the caller).
    let having_clause = if let Some(having_expr) = having {
        // Build temporary bindings that include aggregate aliases for HAVING.
        let mut having_bindings = inner_alias.clone();
        for (vname, _) in &agg_bindings {
            having_bindings.insert(vname.clone(), format!("_g_{vname}"));
        }
        // Mark aggregate vars as raw numeric so FILTER constants (e.g. >= 2) are
        // not encoded as inline IDs — COUNT(*) returns a raw SQL integer, not an
        // inline-encoded value.
        for (vname, _) in &agg_bindings {
            ctx.raw_numeric_vars.insert(vname.clone());
        }
        let result = translate_expr(having_expr, &having_bindings, ctx)
            .map(|c| format!("HAVING {c}"))
            .unwrap_or_default();
        // Remove them again — only raw in HAVING scope of this group fragment.
        for (vname, _) in &agg_bindings {
            ctx.raw_numeric_vars.remove(vname.as_str());
        }
        result
    } else {
        String::new()
    };

    let select_list = if select_parts.is_empty() {
        "COUNT(*) AS _g__count".to_owned()
    } else {
        select_parts.join(", ")
    };

    let group_sql = format!(
        "(SELECT {select_list} FROM ({inner_sql}) AS _grp_inner \
         {group_by_clause} {having_clause})"
    );

    let alias = ctx.next_alias();
    let mut frag = Fragment::empty();
    frag.from_items.push((alias.clone(), group_sql));

    // Bind group-by variables.
    for (v, _) in &group_cols {
        frag.bindings.insert(v.clone(), format!("{alias}._g_{v}"));
    }
    // Bind aggregate output variables and mark them as raw numeric.
    // This ensures that FILTER(?cnt >= 2) in an outer pattern (e.g. a subquery
    // wrapping a GROUP BY) uses raw integer comparison rather than inline IDs.
    for (vname, _) in &agg_bindings {
        frag.bindings
            .insert(vname.clone(), format!("{alias}._g_{vname}"));
        ctx.raw_numeric_vars.insert(vname.clone());
    }

    frag
}

/// Translate an AggregateExpression to a SQL aggregate expression string.
fn translate_aggregate(agg: &AggregateExpression, bindings: &HashMap<String, String>) -> String {
    match agg {
        AggregateExpression::CountSolutions { distinct } => {
            if *distinct {
                "COUNT(DISTINCT *)".to_owned()
            } else {
                "COUNT(*)".to_owned()
            }
        }
        AggregateExpression::FunctionCall {
            name,
            expr,
            distinct,
        } => {
            let distinct_kw = if *distinct { "DISTINCT " } else { "" };
            // Try to obtain the SQL column expression for the argument.
            let arg = translate_agg_expr(expr, bindings).unwrap_or_else(|| "NULL".to_owned());
            match name {
                AggregateFunction::Count => format!("COUNT({distinct_kw}{arg})"),
                AggregateFunction::Sum => format!("SUM({distinct_kw}{arg})"),
                AggregateFunction::Avg => format!("AVG({distinct_kw}{arg})"),
                AggregateFunction::Min => format!("MIN({arg})"),
                AggregateFunction::Max => format!("MAX({arg})"),
                AggregateFunction::GroupConcat { separator } => {
                    let sep = separator.as_deref().unwrap_or(" ");
                    format!(
                        "STRING_AGG({arg}::text, {sep_lit} ORDER BY {arg})",
                        sep_lit = quote_sql_string(sep)
                    )
                }
                AggregateFunction::Sample => format!("MIN({arg})"),
                AggregateFunction::Custom(_) => format!("MIN({arg})"),
            }
        }
    }
}

/// Obtain a SQL column reference for an expression used inside an aggregate.
fn translate_agg_expr(expr: &Expression, bindings: &HashMap<String, String>) -> Option<String> {
    match expr {
        Expression::Variable(v) => bindings.get(v.as_str()).cloned(),
        _ => None,
    }
}

/// Quote a string as a SQL string literal (single quotes, escaping internal
/// single quotes by doubling them).  Safe because the input comes from the
/// SPARQL query string, not user-controlled raw SQL.
fn quote_sql_string(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

// ─── VALUES translator ────────────────────────────────────────────────────────

fn translate_values(
    variables: &[spargebra::term::Variable],
    bindings: &[Vec<Option<GroundTerm>>],
    ctx: &mut Ctx,
) -> Fragment {
    if variables.is_empty() || bindings.is_empty() {
        // Empty VALUES: return a fragment that yields zero rows.
        let mut frag = Fragment::empty();
        frag.conditions.push("FALSE".to_owned());
        return frag;
    }

    // Build a VALUES clause: VALUES (v1, v2, ...), (v1, v2, ...) ...
    // Each cell is a dictionary ID (or NULL for unbound).
    let mut rows: Vec<String> = Vec::with_capacity(bindings.len());
    let mut encode_ctx: Ctx = Ctx::new();

    for row in bindings {
        let cells: Vec<String> = variables
            .iter()
            .zip(row.iter())
            .map(|(_, cell)| match cell {
                None => "NULL::bigint".to_owned(),
                Some(gt) => {
                    let id = encode_ground_term(gt, &mut encode_ctx);
                    id.to_string()
                }
            })
            .collect();
        rows.push(format!("({})", cells.join(", ")));
    }

    let col_names: Vec<String> = variables
        .iter()
        .map(|v| format!("_val_{}", v.as_str()))
        .collect();

    let col_names_str = col_names.join(", ");
    // Wrap in SELECT * so the outer build_from can add AS {alias} without
    // creating a double-alias.  The VALUES alias (_vi{n}) is internal.
    let n = ctx.alias_counter;
    ctx.alias_counter += 1;
    let values_expr = format!(
        "(SELECT * FROM (VALUES {}) AS _vi{n}({col_names_str}))",
        rows.join(", ")
    );

    let alias = ctx.next_alias();

    let mut frag = Fragment::empty();
    frag.from_items.push((alias.clone(), values_expr));

    for v in variables {
        frag.bindings.insert(
            v.as_str().to_owned(),
            format!("{alias}._val_{}", v.as_str()),
        );
    }

    frag
}

/// Encode a `GroundTerm` (IRI or literal, no variables) to a dictionary ID.
fn encode_ground_term(gt: &GroundTerm, ctx: &mut Ctx) -> i64 {
    match gt {
        GroundTerm::NamedNode(nn) => ctx.encode_iri(nn.as_str()).unwrap_or(0),
        GroundTerm::Literal(lit) => ctx.encode_literal(lit),
        // Triple terms (RDF-star) — look up quoted triple dictionary entry.
        GroundTerm::Triple(t) => {
            let s_id = ctx.encode_iri(t.subject.as_str()).unwrap_or(0);
            let p_id = ctx.encode_iri(t.predicate.as_str()).unwrap_or(0);
            let o_id = encode_ground_term(&t.object, ctx);
            dictionary::lookup_quoted_triple(s_id, p_id, o_id).unwrap_or(0)
        }
    }
}

// ─── Expression translator ───────────────────────────────────────────────────

fn translate_expr(
    expr: &Expression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<String> {
    match expr {
        Expression::Variable(v) => {
            let col = bindings.get(v.as_str())?;
            // Treat a bare variable as a boolean — true when col IS NOT NULL.
            Some(format!("({col} IS NOT NULL)"))
        }

        Expression::Equal(a, b) => {
            let (la, ra) = translate_comparison_sides(a, b, bindings, ctx)?;
            Some(format!("({la} = {ra})"))
        }
        Expression::SameTerm(a, b) => {
            let (la, ra) = translate_comparison_sides(a, b, bindings, ctx)?;
            Some(format!("({la} = {ra})"))
        }
        Expression::Greater(a, b) => {
            let (la, ra) = translate_comparison_sides(a, b, bindings, ctx)?;
            Some(format!("({la} > {ra})"))
        }
        Expression::GreaterOrEqual(a, b) => {
            let (la, ra) = translate_comparison_sides(a, b, bindings, ctx)?;
            Some(format!("({la} >= {ra})"))
        }
        Expression::Less(a, b) => {
            let (la, ra) = translate_comparison_sides(a, b, bindings, ctx)?;
            Some(format!("({la} < {ra})"))
        }
        Expression::LessOrEqual(a, b) => {
            let (la, ra) = translate_comparison_sides(a, b, bindings, ctx)?;
            Some(format!("({la} <= {ra})"))
        }

        Expression::And(a, b) => {
            let la = translate_expr(a, bindings, ctx)?;
            let ra = translate_expr(b, bindings, ctx)?;
            Some(format!("({la} AND {ra})"))
        }
        Expression::Or(a, b) => {
            let la = translate_expr(a, bindings, ctx)?;
            let ra = translate_expr(b, bindings, ctx)?;
            Some(format!("({la} OR {ra})"))
        }
        Expression::Not(inner) => {
            let c = translate_expr(inner, bindings, ctx)?;
            Some(format!("(NOT {c})"))
        }

        Expression::Bound(v) => {
            let col = bindings.get(v.as_str())?;
            Some(format!("({col} IS NOT NULL)"))
        }

        Expression::In(var, values) => {
            let col = translate_expr_value(var, bindings, ctx)?;
            let ids: Vec<_> = values
                .iter()
                .filter_map(|v| translate_expr_value(v, bindings, ctx))
                .collect();
            if ids.is_empty() {
                Some("FALSE".to_owned())
            } else {
                Some(format!("({col} IN ({}))", ids.join(", ")))
            }
        }

        // Unsupported expressions: skip (safe — omitting a FILTER is conservative,
        // potentially returns more rows than strictly correct but never corrupts data).
        _ => None,
    }
}

/// Translate an expression to a SQL integer value (dictionary id or column ref).
///
/// For SPARQL literals of inline-encodable types (xsd:integer, xsd:boolean,
/// xsd:dateTime, xsd:date), we return the inline-encoded i64 so that
/// FILTER comparisons on stored inline values work correctly (both sides use
/// the same encoding).  When the other side of a comparison is a raw numeric
/// variable (aggregate output), callers should use `translate_expr_value_raw`
/// instead.
fn translate_expr_value(
    expr: &Expression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<String> {
    match expr {
        Expression::Variable(v) => Some(bindings.get(v.as_str())?.clone()),
        Expression::NamedNode(nn) => {
            let id = ctx.encode_iri(nn.as_str())?;
            Some(id.to_string())
        }
        Expression::Literal(lit) => {
            // use inline encoding (or dict if out of range / unsupported type)
            let id = ctx.encode_literal(lit);
            Some(id.to_string())
        }
        _ => None,
    }
}

/// Like `translate_expr_value`, but always returns raw numeric SQL values for
/// numeric literals — used when the comparison context is a raw aggregate
/// output (COUNT, SUM, etc.) rather than a stored inline-encoded triple value.
fn translate_expr_value_raw(
    expr: &Expression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<String> {
    match expr {
        Expression::Variable(v) => Some(bindings.get(v.as_str())?.clone()),
        Expression::NamedNode(nn) => {
            let id = ctx.encode_iri(nn.as_str())?;
            Some(id.to_string())
        }
        Expression::Literal(lit) => {
            let dt = lit.datatype().as_str();
            // For numeric types compared with aggregate results, return the
            // raw lexical value so COUNT(*) = 2 comparisons work correctly.
            if dt == "http://www.w3.org/2001/XMLSchema#integer"
                || dt == "http://www.w3.org/2001/XMLSchema#long"
                || dt == "http://www.w3.org/2001/XMLSchema#int"
                || dt == "http://www.w3.org/2001/XMLSchema#short"
                || dt == "http://www.w3.org/2001/XMLSchema#decimal"
                || dt == "http://www.w3.org/2001/XMLSchema#float"
                || dt == "http://www.w3.org/2001/XMLSchema#double"
            {
                Some(lit.value().to_owned())
            } else {
                let id = ctx.encode_literal(lit);
                Some(id.to_string())
            }
        }
        _ => None,
    }
}

/// Determine whether an expression is a raw-numeric variable (aggregate output).
fn expr_is_raw_numeric(expr: &Expression, ctx: &Ctx) -> bool {
    if let Expression::Variable(v) = expr {
        ctx.raw_numeric_vars.contains(v.as_str())
    } else {
        false
    }
}

/// Translate both sides of a comparison, using raw encoding for numeric
/// literals when either side is a raw-numeric aggregate variable.
fn translate_comparison_sides(
    a: &Expression,
    b: &Expression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<(String, String)> {
    if expr_is_raw_numeric(a, ctx) || expr_is_raw_numeric(b, ctx) {
        let la = translate_expr_value_raw(a, bindings, ctx)?;
        let ra = translate_expr_value_raw(b, bindings, ctx)?;
        Some((la, ra))
    } else {
        let la = translate_expr_value(a, bindings, ctx)?;
        let ra = translate_expr_value(b, bindings, ctx)?;
        Some((la, ra))
    }
}

// ─── ORDER BY translator ──────────────────────────────────────────────────────

fn translate_order_by(exprs: &[OrderExpression], bindings: &HashMap<String, String>) -> String {
    let parts: Vec<String> = exprs
        .iter()
        .filter_map(|oe| match oe {
            OrderExpression::Asc(expr) => {
                if let Expression::Variable(v) = expr {
                    bindings.get(v.as_str()).map(|col| format!("{col} ASC"))
                } else {
                    None
                }
            }
            OrderExpression::Desc(expr) => {
                if let Expression::Variable(v) = expr {
                    bindings.get(v.as_str()).map(|col| format!("{col} DESC"))
                } else {
                    None
                }
            }
        })
        .collect();
    parts.join(", ")
}

// ─── Modifier extraction helpers ─────────────────────────────────────────────

struct Modifiers<'a> {
    pattern: &'a GraphPattern,
    project_vars: Option<Vec<String>>,
    distinct: bool,
    limit: Option<usize>,
    offset: usize,
    order_by: Option<String>, // resolved later after translating inner
    order_exprs: Vec<OrderExpression>,
}

fn extract_modifiers(mut p: &GraphPattern) -> Modifiers<'_> {
    let mut project_vars: Option<Vec<String>> = None;
    let mut distinct = false;
    let mut limit: Option<usize> = None;
    let mut offset = 0usize;
    let mut order_exprs: Vec<OrderExpression> = vec![];

    loop {
        match p {
            GraphPattern::Project { inner, variables } => {
                if project_vars.is_none() {
                    project_vars = Some(variables.iter().map(|v| v.as_str().to_owned()).collect());
                }
                p = inner;
            }
            GraphPattern::Distinct { inner } | GraphPattern::Reduced { inner } => {
                distinct = true;
                p = inner;
            }
            GraphPattern::Slice {
                inner,
                start,
                length,
            } => {
                offset = *start;
                limit = *length;
                p = inner;
            }
            GraphPattern::OrderBy { inner, expression } => {
                order_exprs = expression.clone();
                p = inner;
            }
            _ => break,
        }
    }

    Modifiers {
        pattern: p,
        project_vars,
        distinct,
        limit,
        offset,
        order_by: None,
        order_exprs,
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Translation result: a SQL SELECT and the projected variable names in order.
pub struct Translation {
    pub sql: String,
    pub variables: Vec<String>,
    /// Variables that hold raw SQL numbers (aggregates like COUNT, SUM).
    /// These must NOT be dictionary-decoded; they should be emitted as JSON
    /// numbers directly.
    pub raw_numeric_vars: std::collections::HashSet<String>,
}

/// Translate a SPARQL SELECT query pattern to SQL.
pub fn translate_select(pattern: &GraphPattern) -> Translation {
    let mut mods = extract_modifiers(pattern);
    let mut ctx = Ctx::new();
    let frag = translate_pattern(mods.pattern, &mut ctx);

    // Resolve ORDER BY now that we have the final bindings.
    let order_str = if mods.order_exprs.is_empty() {
        String::new()
    } else {
        let s = translate_order_by(&mods.order_exprs, &frag.bindings);
        if s.is_empty() {
            String::new()
        } else {
            format!("ORDER BY {s}")
        }
    };
    mods.order_by = Some(order_str);

    // Determine projected variables.
    let variables: Vec<String> = match &mods.project_vars {
        Some(vars) => vars.clone(),
        None => {
            let mut vs: Vec<String> = frag.bindings.keys().cloned().collect();
            vs.sort();
            vs
        }
    };

    // Build SELECT clause: project variables as `col AS _v_{name}`.
    let select_cols: Vec<String> = variables
        .iter()
        .map(|v| {
            frag.bindings
                .get(v)
                .map(|col| format!("{col} AS _v_{v}"))
                .unwrap_or_else(|| format!("NULL::bigint AS _v_{v}"))
        })
        .collect();

    let distinct_kw = if mods.distinct { "DISTINCT " } else { "" };
    let from = frag.build_from();
    let where_clause = frag.build_where();
    let order_clause = mods.order_by.unwrap_or_default();
    let limit_clause = mods.limit.map(|l| format!("LIMIT {l}")).unwrap_or_default();
    let offset_clause = if mods.offset > 0 {
        format!("OFFSET {}", mods.offset)
    } else {
        String::new()
    };

    let sql = format!(
        "SELECT {distinct_kw}{} FROM {from} {where_clause} {order_clause} {limit_clause} {offset_clause}",
        if select_cols.is_empty() {
            "1 AS _dummy".to_owned()
        } else {
            select_cols.join(", ")
        }
    );

    Translation {
        sql,
        variables,
        raw_numeric_vars: ctx.raw_numeric_vars,
    }
}

/// Translate a SPARQL ASK query pattern to SQL.
pub fn translate_ask(pattern: &GraphPattern) -> String {
    let mods = extract_modifiers(pattern);
    let inner = mods.pattern;
    let mut ctx = Ctx::new();
    let frag = translate_pattern(inner, &mut ctx);
    let from = frag.build_from();
    let where_clause = frag.build_where();
    format!("SELECT EXISTS(SELECT 1 FROM {from} {where_clause})")
}
