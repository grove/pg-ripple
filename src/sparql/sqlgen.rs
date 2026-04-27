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
//! - `Service` — SPARQL SERVICE (v0.16.0) → inline VALUES from remote endpoint

use std::collections::HashMap;

use pgrx::prelude::*;
use spargebra::algebra::{Expression, GraphPattern, OrderExpression};
use spargebra::term::{Literal, TermPattern};

use super::federation;
use super::property_path::{PathCtx, compile_path};
use crate::dictionary;
use crate::sparql::translate::filter::{extract_modifiers, translate_order_by};
use crate::sparql::translate::{bgp, distinct, filter, graph, group, join, left_join, union};

// ─── VP table resolution ─────────────────────────────────────────────────────

/// How a predicate's triples are physically stored.
pub(crate) enum VpSource {
    /// Dedicated table, e.g. `_pg_ripple.vp_1234`.
    Dedicated(String),
    /// Stored in the shared `vp_rare` table with predicate filter `p = {id}`.
    Rare(i64),
    /// Predicate never stored — table expression yields 0 rows.
    Empty,
}

/// Resolve how to access triples for `pred_id`.
pub(crate) fn vp_source(pred_id: i64) -> VpSource {
    // v0.38.0: use the backend-local predicate cache to avoid per-atom SPI.
    use crate::storage::catalog::PredicateCatalog as _;
    match crate::storage::catalog::PREDICATE_CACHE.resolve(pred_id) {
        Some(desc) if desc.dedicated => VpSource::Dedicated(format!("_pg_ripple.vp_{pred_id}")),
        Some(_) => VpSource::Rare(pred_id),
        None => VpSource::Empty,
    }
}

// ─── XSD canonical double format ──────────────────────────────────────────────

/// Convert a PostgreSQL numeric string to XSD 1.1 canonical double lexical form.
///
/// XSD canonical double: `["-"]m.nE["-"]e` where the mantissa has exactly one
/// digit before the decimal point and at least one digit after, and the exponent
/// is the minimal decimal integer.
/// Examples: "32100" → "3.21E4", "0.4" → "4.0E-1", "100" → "1.0E2".
///
/// Called from `pg_ripple.xsd_double_fmt()` pgrx wrapper in dict_api.rs.
pub fn xsd_double_fmt_impl(s: &str) -> String {
    let s = s.trim();
    let (neg, s) = if let Some(rest) = s.strip_prefix('-') {
        (true, rest)
    } else {
        (false, s)
    };
    let s = s.trim_start_matches('+');

    // Parse scientific notation if present (e.g. "1.0E2", "3.21E4", "2E-1")
    let (mantissa_str, exp_offset): (&str, i32) = if let Some(e_pos) = s.find(['E', 'e']) {
        let exp_part = &s[e_pos + 1..];
        let exp_val: i32 = exp_part.parse().unwrap_or(0);
        (&s[..e_pos], exp_val)
    } else {
        (s, 0)
    };

    // Find/split integer and fractional parts of mantissa
    let (int_part, frac_part) = if let Some(dot) = mantissa_str.find('.') {
        (&mantissa_str[..dot], &mantissa_str[dot + 1..])
    } else {
        (mantissa_str, "")
    };

    // Combine all digits (strip decimal point)
    let combined: String = format!("{int_part}{frac_part}");
    // decimal_pos = number of integer digits + exp_offset
    let decimal_pos = int_part.len() as i32 + exp_offset;

    // Find first non-zero digit
    let Some(first_nz) = combined.chars().position(|c| c != '0') else {
        return "0.0E0".to_string();
    };

    let exp = decimal_pos - (first_nz as i32) - 1;
    let significant = &combined[first_nz..];
    let trimmed = significant.trim_end_matches('0');
    let trimmed = if trimmed.is_empty() { "0" } else { trimmed };

    let mantissa = if trimmed.len() == 1 {
        format!("{trimmed}.0")
    } else {
        format!("{}.{}", &trimmed[..1], &trimmed[1..])
    };

    let sign = if neg { "-" } else { "" };
    format!("{sign}{mantissa}E{exp}")
}

/// Build a SQL table expression for one triple pattern (exposing `s`, `o`, `g`).
/// When `graph_filter` is `Some(gid)`, injects `WHERE g = {gid}` so that the
/// filter is baked into the leaf scan before any `LEFT JOIN` or CTE wrapper is built.
pub(crate) fn table_expr(src: &VpSource, graph_filter: Option<i64>, svc_excl: &str) -> String {
    match src {
        VpSource::Dedicated(name) => match graph_filter {
            None => {
                if svc_excl.is_empty() {
                    name.clone()
                } else {
                    format!("(SELECT s, o, g FROM {name} WHERE 1=1{svc_excl})")
                }
            }
            Some(gid) => format!("(SELECT s, o, g FROM {name} WHERE g = {gid})"),
        },
        VpSource::Rare(p) => match graph_filter {
            None => {
                if svc_excl.is_empty() {
                    format!("(SELECT s, o, g FROM _pg_ripple.vp_rare WHERE p = {p})")
                } else {
                    format!("(SELECT s, o, g FROM _pg_ripple.vp_rare WHERE p = {p}{svc_excl})")
                }
            }
            Some(gid) => {
                format!("(SELECT s, o, g FROM _pg_ripple.vp_rare WHERE p = {p} AND g = {gid})")
            }
        },
        VpSource::Empty => {
            "(SELECT NULL::bigint AS s, NULL::bigint AS o, NULL::bigint AS g LIMIT 0)".to_owned()
        }
    }
}

/// Build a UNION ALL subquery that covers every predicate — both dedicated VP
/// tables and `vp_rare`.  Each branch projects `(p, s, o, g)` so the caller
/// can bind the predicate variable.
///
/// When `graph_filter` is `Some(gid)`, injects `WHERE g = {gid}` into every
/// branch so the filter is baked in before any outer `LEFT JOIN` wrapper.
pub(crate) fn build_all_predicates_union(graph_filter: Option<i64>, svc_excl: &str) -> String {
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
                match graph_filter {
                    None => {
                        if svc_excl.is_empty() {
                            branches.push(format!(
                                "SELECT {pred_id}::bigint AS p, s, o, g FROM _pg_ripple.vp_{pred_id}"
                            ))
                        } else {
                            branches.push(format!(
                                "SELECT {pred_id}::bigint AS p, s, o, g FROM _pg_ripple.vp_{pred_id} WHERE 1=1{svc_excl}"
                            ))
                        }
                    }
                    Some(gid) => branches.push(format!(
                        "SELECT {pred_id}::bigint AS p, s, o, g FROM _pg_ripple.vp_{pred_id} WHERE g = {gid}"
                    )),
                }
            }
        }
    });

    // Always include vp_rare (it already has a `p` column).
    match graph_filter {
        None => {
            if svc_excl.is_empty() {
                branches.push("SELECT p, s, o, g FROM _pg_ripple.vp_rare".to_owned())
            } else {
                branches.push(format!(
                    "SELECT p, s, o, g FROM _pg_ripple.vp_rare WHERE 1=1{svc_excl}"
                ))
            }
        }
        Some(gid) => branches.push(format!(
            "SELECT p, s, o, g FROM _pg_ripple.vp_rare WHERE g = {gid}"
        )),
    }

    branches.join(" UNION ALL ")
}

// ─── Translation context ─────────────────────────────────────────────────────

/// Mutable state carried through recursive translation.
pub(crate) struct Ctx {
    pub(crate) alias_counter: u32,
    #[allow(dead_code)]
    pub(crate) opt_counter: u32,
    pub(crate) path_counter: u32,
    /// Per-query IRI/literal encoding cache — avoids repeated SPI look-ups.
    per_query: HashMap<String, Option<i64>>,
    /// Variables that hold raw SQL integers (COUNT, SUM, etc. aggregate outputs).
    /// FILTER constants compared against these must stay as raw SQL values,
    /// not be re-encoded as inline IDs.
    pub(crate) raw_numeric_vars: std::collections::HashSet<String>,
    /// Variables that hold raw SQL text (GROUP_CONCAT outputs, STRUUID results).
    /// FILTER comparisons on these must use the literal's lexical value as
    /// SQL text, not its dictionary-encoded i64 ID.
    pub(crate) raw_text_vars: std::collections::HashSet<String>,
    /// Variables that hold raw IRI text (UUID() results).
    /// Not encoded as dictionary IDs; ISIRI always true, string ops use text directly.
    pub(crate) raw_iri_vars: std::collections::HashSet<String>,
    /// Variables that hold raw SQL double (RAND() results).
    /// Needed so DATATYPE() can return xsd:double without a dict lookup.
    pub(crate) raw_double_vars: std::collections::HashSet<String>,
    /// Graph filter propagated by `GRAPH <G> { ... }` context (v0.40.0).
    ///
    /// When `Some(gid)`, every VP table scan emitted by `translate_bgp`,
    /// `table_expr`, `build_all_predicates_union`, and property paths injects
    /// `WHERE g = gid` directly into the leaf expression.  This ensures the
    /// filter is present *before* any `LEFT JOIN` or `WITH RECURSIVE` wrapper
    /// is built, so `OPTIONAL {}` and property paths inside `GRAPH {}` work
    /// correctly without relying on post-hoc alias lookups.
    pub(crate) graph_filter: Option<i64>,
    /// Set to `true` when translating inside `GRAPH ?g { ... }` (variable
    /// graph).  Property path compilation uses this flag to include a `g`
    /// column in CTE output so the GRAPH ?g handler can bind the variable and
    /// so sequence paths correctly restrict both hops to the same named graph.
    pub(crate) variable_graph: bool,
    /// Base IRI from the SPARQL BASE declaration (e.g. `BASE <http://example.org/>`).
    /// Used by `IRI()`/`URI()` to resolve relative IRI string arguments.
    pub(crate) base_iri: Option<String>,
    /// Dictionary IDs of named graphs used as SERVICE mock endpoints (v0.42.0).
    /// When non-empty, outer BGP scans (without a GRAPH clause) exclude these
    /// graphs so that endpoint data loaded into named graphs does not leak into
    /// the outer query.  The SERVICE inner patterns still scope to their graph
    /// via `ctx.graph_filter = Some(gid)`.
    service_graph_exclude: Vec<i64>,
    /// Set to `true` by `translate_bgp` when a cyclic BGP is detected and
    /// WCOJ optimisation is activated (v0.62.0).  When set, the query executor
    /// runs the WCOJ SET LOCAL preamble before the main query.
    pub(crate) wcoj_preamble: bool,
}

impl Ctx {
    pub(crate) fn new() -> Self {
        Self {
            alias_counter: 0,
            opt_counter: 0,
            path_counter: 0,
            per_query: HashMap::new(),
            raw_numeric_vars: std::collections::HashSet::new(),
            raw_text_vars: std::collections::HashSet::new(),
            raw_iri_vars: std::collections::HashSet::new(),
            raw_double_vars: std::collections::HashSet::new(),
            graph_filter: None,
            variable_graph: false,
            base_iri: None,
            service_graph_exclude: federation::get_service_graph_ids(),
            wcoj_preamble: false,
        }
    }

    /// Returns a SQL fragment like `" AND g NOT IN (gid1, gid2)"` to exclude
    /// service endpoint named graphs from outer BGP scans.  Returns an empty
    /// string when there are no service graphs registered or when the context
    /// already has an explicit graph filter (in which case `table_expr` applies
    /// `WHERE g = gid` and the exclude list is irrelevant).
    pub(crate) fn service_excl(&self) -> String {
        if self.service_graph_exclude.is_empty() || self.graph_filter.is_some() {
            return String::new();
        }
        let ids = self
            .service_graph_exclude
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!(" AND g NOT IN ({ids})")
    }

    pub(crate) fn next_alias(&mut self) -> String {
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
    pub(crate) fn encode_iri(&mut self, iri: &str) -> Option<i64> {
        if let Some(cached) = self.per_query.get(iri) {
            return *cached;
        }
        let id = dictionary::lookup_iri(iri);
        self.per_query.insert(iri.to_owned(), id);
        id
    }

    /// Encode a `spargebra::Literal` to a dictionary id (may insert).
    pub(crate) fn encode_literal(&mut self, lit: &Literal) -> i64 {
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

    /// Translate an expression to a SQL value (dictionary ID or raw numeric).
    /// Used by expr.rs when resolving function arguments.
    #[allow(dead_code)]
    pub(crate) fn translate_value(
        &mut self,
        expr: &Expression,
        bindings: &HashMap<String, String>,
    ) -> Option<String> {
        filter::translate_expr_value(expr, bindings, self)
    }

    /// Translate an expression to a SQL boolean.
    /// Used by expr.rs when resolving IF conditions.
    #[allow(dead_code)]
    pub(crate) fn translate_filter(
        &mut self,
        expr: &Expression,
        bindings: &HashMap<String, String>,
    ) -> Option<String> {
        filter::translate_expr(expr, bindings, self)
    }

    /// Check whether a variable holds a raw IRI text (UUID() result).
    pub(crate) fn is_raw_iri_var(&self, v: &str) -> bool {
        self.raw_iri_vars.contains(v)
    }

    /// Check whether a variable holds a raw double (RAND() result).
    pub(crate) fn is_raw_double_var(&self, v: &str) -> bool {
        self.raw_double_vars.contains(v)
    }

    /// Check whether a variable holds raw text (GROUP_CONCAT / STRUUID result).
    pub(crate) fn is_raw_text_var(&self, v: &str) -> bool {
        self.raw_text_vars.contains(v)
    }
}

// ─── Fragment ─────────────────────────────────────────────────────────────────

/// A SQL query fragment accumulating table joins, conditions, and variable bindings.
pub(crate) struct Fragment {
    /// FROM clause items: (alias, table expression).
    pub(crate) from_items: Vec<(String, String)>,
    /// WHERE conditions (logical AND).
    pub(crate) conditions: Vec<String>,
    /// SPARQL variable name → SQL column or expression.
    pub(crate) bindings: HashMap<String, String>,
}

impl Fragment {
    pub(crate) fn empty() -> Self {
        Self {
            from_items: vec![],
            conditions: vec![],
            bindings: HashMap::new(),
        }
    }

    /// Return a fragment that produces exactly zero rows (for SILENT error cases).
    pub(crate) fn zero_rows() -> Self {
        Self {
            from_items: vec![("_zero".to_owned(), "(SELECT 1 LIMIT 0)".to_owned())],
            conditions: vec![],
            bindings: HashMap::new(),
        }
    }

    /// Merge `other` into `self`, adding equality conditions for shared variables.
    pub(crate) fn merge(&mut self, other: Fragment) {
        for (alias, tbl) in other.from_items {
            self.from_items.push((alias, tbl));
        }
        for cond in other.conditions {
            self.conditions.push(cond);
        }
        for (var, col) in other.bindings {
            if let Some(existing) = self.bindings.get(&var).cloned() {
                // Variable already bound in both sides.
                // Use SPARQL-compatible null-safe join: if the existing binding is
                // NULL (unbound from an OPTIONAL), the other side's value fills in.
                // This matches SPARQL semantics: unbound variables are compatible
                // with any binding from the other side (e.g. VALUES after OPTIONAL).
                self.conditions
                    .push(format!("({existing} IS NULL OR {existing} = {col})"));
                // Update binding to prefer the non-NULL value.
                self.bindings
                    .insert(var, format!("COALESCE({existing}, {col})"));
            } else {
                self.bindings.insert(var, col);
            }
        }
    }

    pub(crate) fn build_from(&self) -> String {
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

    pub(crate) fn build_where(&self) -> String {
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

// ─── Core graph-pattern dispatcher ───────────────────────────────────────────

pub(crate) fn translate_pattern(pattern: &GraphPattern, ctx: &mut Ctx) -> Fragment {
    match pattern {
        GraphPattern::Bgp { patterns } => bgp::translate_bgp(patterns, ctx),

        GraphPattern::Join { left, right } => join::translate_join(left, right, ctx),

        GraphPattern::LeftJoin {
            left,
            right,
            expression,
        } => left_join::translate_left_join(left, right, expression.as_ref(), ctx),

        GraphPattern::Filter { expr, inner } => {
            // Special case: Filter wrapping Group → HAVING clause.
            if let GraphPattern::Group {
                inner: group_inner,
                variables,
                aggregates,
            } = inner.as_ref()
            {
                return group::translate_group(group_inner, variables, aggregates, Some(expr), ctx);
            }
            let mut frag = translate_pattern(inner, ctx);
            // SPARQL 1.1 §18.6: filter error → false.
            match filter::translate_expr(expr, &frag.bindings, ctx) {
                Some(cond) => frag.conditions.push(cond),
                None => frag.conditions.push("FALSE".to_owned()),
            }
            frag
        }

        GraphPattern::Graph { name, inner } => graph::translate_graph(name, inner, ctx),

        // Modifiers are peeled off by translate_select — these are fall-throughs
        // for when they appear in nested positions.
        GraphPattern::Project { inner, variables } => {
            let mut frag = translate_pattern(inner, ctx);
            let var_set: std::collections::HashSet<String> =
                variables.iter().map(|v| v.as_str().to_owned()).collect();
            frag.bindings.retain(|v, _| var_set.contains(v));
            frag
        }

        GraphPattern::Distinct { inner } | GraphPattern::Reduced { inner } => {
            // v0.38.0: SHACL hints — if ALL predicates have sh:maxCount ≤ 1,
            // the result is already distinct.  Wired to keep the code path alive.
            let _ = bgp::shacl_bgp_all_max_count_1(inner);
            translate_pattern(inner, ctx)
        }

        GraphPattern::Slice { .. } => distinct::translate_slice(pattern, ctx),

        GraphPattern::OrderBy { inner, .. } => translate_pattern(inner, ctx),

        // ── Property path (p+, p*, p?, p/q, p|q, ^p, !(p)) ─────────────────
        GraphPattern::Path {
            subject,
            path,
            object,
        } => {
            let max_depth = crate::MAX_PATH_DEPTH.get();
            let mut path_ctx = PathCtx::new(ctx.path_counter);
            let s_const = bgp::ground_term_sql_for_path(subject, ctx);
            let o_const = bgp::ground_term_sql_for_path(object, ctx);
            let include_g = ctx.variable_graph;
            let path_sql = compile_path(
                path,
                s_const.as_deref(),
                o_const.as_deref(),
                &mut path_ctx,
                max_depth,
                ctx.graph_filter,
                include_g,
            );
            ctx.path_counter = path_ctx.counter;

            let alias = ctx.next_alias();
            let mut frag = Fragment::empty();
            frag.from_items.push((alias.clone(), path_sql));

            if let TermPattern::Variable(v) = subject {
                let vname = v.as_str().to_owned();
                let col = format!("{alias}.s");
                if let Some(existing) = frag.bindings.get(&vname) {
                    frag.conditions.push(format!("{col} = {existing}"));
                } else {
                    frag.bindings.insert(vname, col);
                }
            }

            if let TermPattern::Variable(v) = object {
                let vname = v.as_str().to_owned();
                let col = format!("{alias}.o");
                if let Some(existing) = frag.bindings.get(&vname) {
                    frag.conditions.push(format!("{col} = {existing}"));
                } else {
                    frag.bindings.insert(vname, col);
                }
            }

            frag
        }

        GraphPattern::Union { left, right } => union::translate_union(left, right, ctx),
        GraphPattern::Minus { left, right } => union::translate_minus(left, right, ctx),

        GraphPattern::Group {
            inner,
            variables,
            aggregates,
        } => group::translate_group(inner, variables, aggregates, None, ctx),

        GraphPattern::Extend {
            inner,
            variable,
            expression,
        } => filter::translate_extend(inner, variable, expression, ctx),

        GraphPattern::Values {
            variables,
            bindings,
        } => filter::translate_values(variables, bindings, ctx),

        GraphPattern::Service {
            name,
            inner,
            silent,
        } => graph::translate_service(name, inner, *silent, ctx),
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
    /// Variables that hold raw SQL text (GROUP_CONCAT / STRUUID outputs).
    /// These must be read as TEXT columns (not i64) and emitted as JSON strings.
    pub raw_text_vars: std::collections::HashSet<String>,
    /// Variables that hold raw IRI text (UUID() outputs).
    /// Must be read as TEXT columns and emitted as `<iri>` IRI format.
    pub raw_iri_vars: std::collections::HashSet<String>,
    /// Variables that hold raw double (RAND() outputs).
    /// Must be read as FLOAT8 columns and emitted as `"val"^^xsd:double` format.
    pub raw_double_vars: std::collections::HashSet<String>,
    /// True when TopN push-down (v0.46.0) was applied: `ORDER BY … LIMIT N`
    /// was emitted directly in SQL rather than post-decode truncation.
    pub topn_applied: bool,
    /// True when the WCOJ planner (v0.62.0) detected a cyclic BGP and
    /// activated the Leapfrog-Triejoin execution path.  The query executor
    /// must run `wcoj_session_preamble()` before executing the SQL.
    pub wcoj_preamble: bool,
}

/// Translate a SPARQL SELECT query pattern to SQL.
pub fn translate_select(pattern: &GraphPattern, base_iri: Option<&str>) -> Translation {
    let mut mods = extract_modifiers(pattern);
    let mut ctx = Ctx::new();
    ctx.base_iri = base_iri.map(|s| s.to_owned());
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

    // When SELECT DISTINCT is combined with ORDER BY on a non-projected variable,
    // PostgreSQL rejects the query ("ORDER BY expressions must appear in select list").
    // Per SPARQL 1.1 §15, such ordering is implementation-defined.
    // Drop any ORDER BY expressions that reference non-projected variables so the
    // query remains valid SQL.
    let order_clause = if mods.distinct && !mods.order_exprs.is_empty() {
        let projected: std::collections::HashSet<&str> =
            variables.iter().map(|v| v.as_str()).collect();
        let safe_exprs: Vec<_> = mods
            .order_exprs
            .iter()
            .filter(|oe| {
                let var = match oe {
                    OrderExpression::Asc(Expression::Variable(v))
                    | OrderExpression::Desc(Expression::Variable(v)) => Some(v.as_str()),
                    _ => None,
                };
                // Keep the expression only if it refers to a projected variable (or
                // is not a simple variable reference, e.g. a complex expression).
                var.is_none_or(|v| projected.contains(v))
            })
            .cloned()
            .collect();
        if safe_exprs.is_empty() {
            String::new()
        } else {
            let s = translate_order_by(&safe_exprs, &frag.bindings);
            if s.is_empty() {
                String::new()
            } else {
                format!("ORDER BY {s}")
            }
        }
    } else {
        mods.order_by.unwrap_or_default()
    };
    let limit_clause = mods.limit.map(|l| format!("LIMIT {l}")).unwrap_or_default();
    let offset_clause = if mods.offset > 0 {
        format!("OFFSET {}", mods.offset)
    } else {
        String::new()
    };

    // ── v0.46.0 TopN push-down ────────────────────────────────────────────────
    // When ORDER BY + LIMIT is present (no OFFSET, no DISTINCT) and the GUC is
    // enabled, the LIMIT clause is already embedded directly in the SQL above.
    // `sparql_explain()` surfaces whether the optimisation was applied via the
    // `topn_applied` key.  No structural change needed here — the limit_clause
    // is already emitted after order_clause in the format! below.
    // The `topn_applied` flag is set in the Translation struct for explain.
    let topn_applied = crate::TOPN_PUSHDOWN.get()
        && mods.limit.is_some()
        && !mods.distinct
        && mods.offset == 0
        && !order_clause.is_empty();

    let sql = format!(
        "SELECT {distinct_kw}{} FROM {from} {where_clause} {order_clause} {limit_clause} {offset_clause}",
        if select_cols.is_empty() {
            "1 AS _dummy".to_owned()
        } else {
            select_cols.join(", ")
        }
    );

    // v0.62.0: if a cyclic BGP was detected during translation, wrap the SQL
    // with the WCOJ materialized-CTE hint so the planner uses sort-merge joins.
    let wcoj_preamble = ctx.wcoj_preamble;
    let sql = if wcoj_preamble {
        crate::sparql::wcoj::apply_wcoj_hints(&sql)
    } else {
        sql
    };

    Translation {
        sql,
        variables,
        raw_numeric_vars: ctx.raw_numeric_vars,
        raw_text_vars: ctx.raw_text_vars,
        raw_iri_vars: ctx.raw_iri_vars,
        raw_double_vars: ctx.raw_double_vars,
        topn_applied,
        wcoj_preamble,
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
