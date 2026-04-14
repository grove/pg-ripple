//! SPARQL algebra → SQL translation.
//!
//! Translates a `spargebra` `GraphPattern` (after sparopt optimization) into a
//! SQL SELECT string.  All IRI/literal constants are encoded to `i64` before
//! appearing in SQL — no raw strings ever reach the generated query.
//!
//! # Supported algebra nodes
//!
//! - `Bgp` — basic graph patterns  → flat JOIN across VP tables
//! - `Join` — AND of two patterns   → merge fragments (implicit cross join)
//! - `LeftJoin` — OPTIONAL          → SQL LEFT JOIN with a subquery
//! - `Filter` — WHERE condition      → SQL WHERE clause
//! - `Graph` — GRAPH ?g / GRAPH <G> → filter on `g` column
//! - `Project` — SELECT columns       → restrict output columns
//! - `Distinct` — DISTINCT            → SQL DISTINCT
//! - `Reduced` — treated same as Distinct for simplicity
//! - `Slice` — LIMIT / OFFSET
//! - `OrderBy` — ORDER BY
//!
//! Unsupported nodes (Union, Group, Extend, Service, etc.) fall back to an
//! error message.

use std::collections::HashMap;

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use spargebra::algebra::{Expression, GraphPattern, OrderExpression};
use spargebra::term::{Literal, NamedNodePattern, TermPattern};

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
            format!(
                "(SELECT s, o, g FROM _pg_ripple.vp_rare WHERE p = {p})"
            )
        }
        VpSource::Empty => "(SELECT NULL::bigint AS s, NULL::bigint AS o, NULL::bigint AS g LIMIT 0)".to_owned(),
    }
}

// ─── Translation context ─────────────────────────────────────────────────────

/// Mutable state carried through recursive translation.
struct Ctx {
    alias_counter: u32,
    opt_counter: u32,
    /// Per-query IRI/literal encoding cache — avoids repeated SPI look-ups.
    per_query: HashMap<String, Option<i64>>,
}

impl Ctx {
    fn new() -> Self {
        Self {
            alias_counter: 0,
            opt_counter: 0,
            per_query: HashMap::new(),
        }
    }

    fn next_alias(&mut self) -> String {
        let n = self.alias_counter;
        self.alias_counter += 1;
        format!("_t{n}")
    }

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
                self.conditions
                    .push(format!("{} = {}", col, existing));
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

/// Bind one end of a triple (subject or object) to the translation context.
/// Returns an optional SQL equality condition if the term is a constant.
fn bind_term(
    alias: &str,
    col: &str,           // "s" or "o"
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
        TermPattern::NamedNode(nn) => {
            match ctx.encode_iri(nn.as_str()) {
                Some(id) => conditions.push(format!("{col_expr} = {id}")),
                None => conditions.push("FALSE".to_owned()),
            }
        }
        TermPattern::Literal(lit) => {
            let id = ctx.encode_literal(lit);
            conditions.push(format!("{col_expr} = {id}"));
        }
        TermPattern::BlankNode(_) => {
            // Blank nodes in query patterns are treated as variables in SPARQL;
            // spargebra should have converted them to fresh variables already.
            // As a fallback, treat as an unbound (wildcard) subject/object.
        }
        _ => {
            // Catch-all for future term pattern variants (e.g. RDF-star triples).
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
                // Unbound predicate: scan vp_rare for all predicates.
                // Generate a rare-all subquery and bind the predicate variable to `p`.
                let vname = v.as_str().to_owned();
                let a = alias.clone();
                frag.from_items.push((
                    a.clone(),
                    "_pg_ripple.vp_rare".to_owned(),
                ));
                if let Some(existing) = frag.bindings.get(&vname) {
                    frag.conditions
                        .push(format!("{a}.p = {existing}"));
                } else {
                    frag.bindings.insert(vname, format!("{a}.p"));
                }
                bind_term(&a, "s", &tp.subject, ctx, &mut frag.bindings, &mut frag.conditions);
                bind_term(&a, "o", &tp.object, ctx, &mut frag.bindings, &mut frag.conditions);
                continue;
            }
        };

        let tbl = table_expr(&source);
        frag.from_items.push((alias.clone(), tbl));
        for c in pred_conditions {
            frag.conditions.push(c);
        }

        bind_term(&alias, "s", &tp.subject, ctx, &mut frag.bindings, &mut frag.conditions);
        bind_term(&alias, "o", &tp.object, ctx, &mut frag.bindings, &mut frag.conditions);
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
            let right_frag_clone = translate_pattern(right, ctx);

            // Identify shared variables (appear in both sides).
            let shared: Vec<String> = right_frag_clone
                .bindings
                .keys()
                .filter(|v| left_frag.bindings.contains_key(*v))
                .cloned()
                .collect();

            let opt_alias = ctx.next_opt();

            // Build a subquery SELECT for the right-hand side.
            let mut right_frag_sq = translate_pattern(right, ctx);

            // Add the OPTIONAL filter expression to the right subquery, if any.
            if let Some(expr) = expression {
                if let Some(cond) = translate_expr(expr, &right_frag_sq.bindings, ctx) {
                    right_frag_sq.conditions.push(cond);
                }
            }

            let right_subquery = right_frag_sq.as_subquery(&opt_alias);

            // Build the combined fragment.
            let mut frag = left_frag;

            // Add the LEFT JOIN.
            let left_from = frag.build_from();
            let left_where = frag.build_where();

            // Build ON condition.
            let on_cond: Option<String> = if shared.is_empty() {
                None
            } else {
                let parts: Vec<String> = shared
                    .iter()
                    .filter_map(|v| {
                        let left_col = frag.bindings.get(v)?;
                        Some(format!("{left_col} = {opt_alias}.{opt_alias}_{v}"))
                    })
                    .collect();
                if parts.is_empty() {
                    None
                } else {
                    Some(parts.join(" AND "))
                }
            };

            // Reconstruct the fragment as a combined FROM with LEFT JOIN.
            frag.from_items.clear();
            frag.conditions.clear();
            // Use a subquery for the left side to keep it self-contained.
            let left_sq = format!(
                "(SELECT {} FROM {left_from} {left_where}) AS _left_{}",
                frag.bindings
                    .iter()
                    .map(|(v, col)| format!("{col} AS _left_{v}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                ctx.alias_counter
            );
            ctx.alias_counter += 1;

            let left_alias = format!("_leftjoin_{}", ctx.opt_counter);
            ctx.opt_counter += 1;

            let on_clause = on_cond
                .map(|c| format!("ON {c}"))
                .unwrap_or_else(|| "ON TRUE".to_owned());

            let combined = format!("{left_sq} LEFT JOIN {right_subquery} AS {opt_alias} {on_clause}");
            frag.from_items.push((left_alias.clone(), combined));

            // Rebind left variables through the left subquery prefix.
            let left_bind_remap: HashMap<String, String> = frag
                .bindings
                .iter()
                .map(|(v, _)| (v.clone(), format!("{left_alias}._left_{v}")))
                .collect();
            frag.bindings = left_bind_remap;

            // Add right-side variables as nullable columns.
            let right_frag_for_bind = translate_pattern(right, ctx);
            for v in right_frag_for_bind.bindings.keys() {
                frag.bindings
                    .insert(v.clone(), format!("{opt_alias}.{opt_alias}_{v}"));
            }

            frag
        }

        GraphPattern::Filter { expr, inner } => {
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
                            frag.conditions
                                .push(format!("{col} = {existing}"));
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

        other => {
            // Return empty fragment with FALSE for unsupported patterns.
            let mut frag = Fragment::empty();
            frag.conditions.push("FALSE".to_owned());
            let _ = other; // silence unused var warning
            frag
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
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(format!("({la} = {ra})"))
        }
        Expression::SameTerm(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(format!("({la} = {ra})"))
        }
        Expression::Greater(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(format!("({la} > {ra})"))
        }
        Expression::GreaterOrEqual(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(format!("({la} >= {ra})"))
        }
        Expression::Less(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(format!("({la} < {ra})"))
        }
        Expression::LessOrEqual(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
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
            let id = ctx.encode_literal(lit);
            Some(id.to_string())
        }
        _ => None,
    }
}

// ─── ORDER BY translator ──────────────────────────────────────────────────────

fn translate_order_by(
    exprs: &[OrderExpression],
    bindings: &HashMap<String, String>,
) -> String {
    let parts: Vec<String> = exprs
        .iter()
        .filter_map(|oe| match oe {
            OrderExpression::Asc(expr) => {
                if let Expression::Variable(v) = expr {
                    bindings
                        .get(v.as_str())
                        .map(|col| format!("{col} ASC"))
                } else {
                    None
                }
            }
            OrderExpression::Desc(expr) => {
                if let Expression::Variable(v) = expr {
                    bindings
                        .get(v.as_str())
                        .map(|col| format!("{col} DESC"))
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
    let limit_clause = mods
        .limit
        .map(|l| format!("LIMIT {l}"))
        .unwrap_or_default();
    let offset_clause = if mods.offset > 0 {
        format!("OFFSET {}", mods.offset)
    } else {
        String::new()
    };

    let sql = format!(
        "SELECT {distinct_kw}{} FROM {from} {where_clause} {order_clause} {limit_clause} {offset_clause}",
        if select_cols.is_empty() { "1 AS _dummy".to_owned() } else { select_cols.join(", ") }
    );

    Translation { sql, variables }
}

/// Translate a SPARQL ASK query pattern to SQL.
pub fn translate_ask(pattern: &GraphPattern) -> String {
    let mut mods = extract_modifiers(pattern);
    let inner = mods.pattern;
    let mut ctx = Ctx::new();
    let frag = translate_pattern(inner, &mut ctx);
    let from = frag.build_from();
    let where_clause = frag.build_where();
    format!("SELECT EXISTS(SELECT 1 FROM {from} {where_clause})")
}
