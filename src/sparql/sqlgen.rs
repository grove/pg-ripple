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
use spargebra::algebra::{
    AggregateExpression, AggregateFunction, Expression, Function, GraphPattern, OrderExpression,
};
use spargebra::term::{GroundTerm, Literal, NamedNodePattern, TermPattern};

use super::expr;
use super::federation;
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
fn table_expr(src: &VpSource, graph_filter: Option<i64>, svc_excl: &str) -> String {
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
fn build_all_predicates_union(graph_filter: Option<i64>, svc_excl: &str) -> String {
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
pub(super) struct Ctx {
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
    /// Variables that hold raw SQL text (GROUP_CONCAT outputs, STRUUID results).
    /// FILTER comparisons on these must use the literal's lexical value as
    /// SQL text, not its dictionary-encoded i64 ID.
    raw_text_vars: std::collections::HashSet<String>,
    /// Variables that hold raw IRI text (UUID() results).
    /// Not encoded as dictionary IDs; ISIRI always true, string ops use text directly.
    raw_iri_vars: std::collections::HashSet<String>,
    /// Variables that hold raw SQL double (RAND() results).
    /// Needed so DATATYPE() can return xsd:double without a dict lookup.
    raw_double_vars: std::collections::HashSet<String>,
    /// Graph filter propagated by `GRAPH <G> { ... }` context (v0.40.0).
    ///
    /// When `Some(gid)`, every VP table scan emitted by `translate_bgp`,
    /// `table_expr`, `build_all_predicates_union`, and property paths injects
    /// `WHERE g = gid` directly into the leaf expression.  This ensures the
    /// filter is present *before* any `LEFT JOIN` or `WITH RECURSIVE` wrapper
    /// is built, so `OPTIONAL {}` and property paths inside `GRAPH {}` work
    /// correctly without relying on post-hoc alias lookups.
    pub(super) graph_filter: Option<i64>,
    /// Set to `true` when translating inside `GRAPH ?g { ... }` (variable
    /// graph).  Property path compilation uses this flag to include a `g`
    /// column in CTE output so the GRAPH ?g handler can bind the variable and
    /// so sequence paths correctly restrict both hops to the same named graph.
    pub(super) variable_graph: bool,
    /// Base IRI from the SPARQL BASE declaration (e.g. `BASE <http://example.org/>`).
    /// Used by `IRI()`/`URI()` to resolve relative IRI string arguments.
    pub(super) base_iri: Option<String>,
    /// Dictionary IDs of named graphs used as SERVICE mock endpoints (v0.42.0).
    /// When non-empty, outer BGP scans (without a GRAPH clause) exclude these
    /// graphs so that endpoint data loaded into named graphs does not leak into
    /// the outer query.  The SERVICE inner patterns still scope to their graph
    /// via `ctx.graph_filter = Some(gid)`.
    service_graph_exclude: Vec<i64>,
}

impl Ctx {
    fn new() -> Self {
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
        }
    }

    /// Returns a SQL fragment like `" AND g NOT IN (gid1, gid2)"` to exclude
    /// service endpoint named graphs from outer BGP scans.  Returns an empty
    /// string when there are no service graphs registered or when the context
    /// already has an explicit graph filter (in which case `table_expr` applies
    /// `WHERE g = gid` and the exclude list is irrelevant).
    fn service_excl(&self) -> String {
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
    pub(super) fn encode_iri(&mut self, iri: &str) -> Option<i64> {
        if let Some(cached) = self.per_query.get(iri) {
            return *cached;
        }
        let id = dictionary::lookup_iri(iri);
        self.per_query.insert(iri.to_owned(), id);
        id
    }

    /// Encode a `spargebra::Literal` to a dictionary id (may insert).
    pub(super) fn encode_literal(&mut self, lit: &Literal) -> i64 {
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
    pub(super) fn translate_value(
        &mut self,
        expr: &Expression,
        bindings: &HashMap<String, String>,
    ) -> Option<String> {
        translate_expr_value(expr, bindings, self)
    }

    /// Translate an expression to a SQL boolean.
    /// Used by expr.rs when resolving IF conditions.
    #[allow(dead_code)]
    pub(super) fn translate_filter(
        &mut self,
        expr: &Expression,
        bindings: &HashMap<String, String>,
    ) -> Option<String> {
        translate_expr(expr, bindings, self)
    }

    /// Check whether a variable holds a raw IRI text (UUID() result).
    pub(super) fn is_raw_iri_var(&self, v: &str) -> bool {
        self.raw_iri_vars.contains(v)
    }

    /// Check whether a variable holds a raw double (RAND() result).
    pub(super) fn is_raw_double_var(&self, v: &str) -> bool {
        self.raw_double_vars.contains(v)
    }

    /// Check whether a variable holds raw text (GROUP_CONCAT / STRUUID result).
    pub(super) fn is_raw_text_var(&self, v: &str) -> bool {
        self.raw_text_vars.contains(v)
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

    /// Return a fragment that produces exactly zero rows (for SILENT error cases).
    fn zero_rows() -> Self {
        Self {
            from_items: vec![("_zero".to_owned(), "(SELECT 1 LIMIT 0)".to_owned())],
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

/// Like `ground_term_id` but for property path endpoints — returns a SQL expression string.
/// When the IRI is not in the dictionary (e.g. empty dataset), falls back to
/// `pg_ripple.encode_term(iri, 0::int2)` so that zero-length paths can still match.
fn ground_term_sql_for_path(term: &TermPattern, ctx: &mut Ctx) -> Option<String> {
    match term {
        TermPattern::NamedNode(nn) => {
            if let Some(id) = ctx.encode_iri(nn.as_str()) {
                Some(id.to_string())
            } else {
                // IRI not in dictionary yet — encode dynamically.
                let iri = nn.as_str().replace('\'', "''");
                Some(format!("pg_ripple.encode_term('{iri}', 0::int2)"))
            }
        }
        TermPattern::Literal(lit) => Some(ctx.encode_literal(lit).to_string()),
        TermPattern::Triple(_inner) => {
            // Quoted triples: try static encoding only.
            ground_term_id(term, ctx).map(|id| id.to_string())
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
            // Blank node IDs may contain ':' (e.g. `_:f6891...`) which is invalid
            // in unquoted SQL identifiers.  Sanitize to alphanumeric + '_' only.
            let vname = sanitize_sql_ident(&format!("_bn_{}", bnode));
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
    // v0.13.0: reorder patterns by estimated selectivity for minimum intermediate results.
    let reordered = super::optimizer::reorder_bgp(patterns, &mut |iri| ctx.encode_iri(iri));
    let patterns = reordered.as_slice();

    let mut frag = Fragment::empty();

    // Self-join elimination: detect duplicate triple patterns, only scan once.
    // v0.21.0: use a structural (s_term, p_term, o_term) key instead of the
    // Debug-string representation, so only truly identical patterns are collapsed.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for tp in patterns {
        // Build a canonical key from the Display representation of each term part.
        // spargebra's term types implement Display with consistent output.
        let key = format!("{}\x00{}\x00{}", tp.subject, tp.predicate, tp.object);
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
                // v0.40.0: pass graph_filter so the union branches include WHERE g = gid.
                let vname = v.as_str().to_owned();
                let a = alias.clone();
                let union_subquery =
                    build_all_predicates_union(ctx.graph_filter, &ctx.service_excl());
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

        // v0.40.0: pass ctx.graph_filter so graph filters are baked into leaf scans.
        // v0.42.0: also pass service_excl to exclude service-endpoint named graphs from outer scans.
        let tbl = table_expr(&source, ctx.graph_filter, &ctx.service_excl());
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

/// v0.38.0: Check if the right side of a SPARQL OPTIONAL is guaranteed
/// non-empty by a SHACL shape hint (`sh:minCount ≥ 1`).
///
/// Returns `true` only when the right pattern is a single-atom BGP with a
/// named predicate that has a `min_count_1` hint in `_pg_ripple.shape_hints`.
/// In that case the optimizer may safely downgrade LEFT JOIN → INNER JOIN.
fn shacl_right_is_mandatory(pattern: &GraphPattern) -> bool {
    // Only consider single-atom BGPs with named predicates.
    let GraphPattern::Bgp { patterns } = pattern else {
        return false;
    };
    if patterns.len() != 1 {
        return false;
    }
    let spargebra::term::NamedNodePattern::NamedNode(nn) = &patterns[0].predicate else {
        return false;
    };
    // Look up predicate in dictionary (read-only; returns None if unknown).
    let Some(pred_id) = crate::dictionary::lookup_iri(nn.as_str()) else {
        return false;
    };
    crate::shacl::hints::has_min_count_1(pred_id)
}

/// v0.38.0: Check if ALL predicates in a BGP pattern have a `max_count_1`
/// SHACL hint (sh:maxCount ≤ 1). When true, the SPARQL engine could safely
/// suppress the outer DISTINCT since each focus node has at most one value.
fn shacl_bgp_all_max_count_1(pattern: &GraphPattern) -> bool {
    let GraphPattern::Bgp { patterns } = pattern else {
        return false;
    };
    if patterns.is_empty() {
        return false;
    }
    patterns.iter().all(|tp| {
        let spargebra::term::NamedNodePattern::NamedNode(nn) = &tp.predicate else {
            return false;
        };
        let Some(pred_id) = crate::dictionary::lookup_iri(nn.as_str()) else {
            return false;
        };
        crate::shacl::hints::has_max_count_1(pred_id)
    })
}

fn translate_pattern(pattern: &GraphPattern, ctx: &mut Ctx) -> Fragment {
    match pattern {
        GraphPattern::Bgp { patterns } => translate_bgp(patterns, ctx),

        GraphPattern::Join { left, right } => {
            // ── Batch SERVICE detection (v0.19.0) ────────────────────────────
            // When both children are SERVICE clauses targeting the same registered
            // endpoint and their inner patterns share no variables, combine them
            // into a single UNION query to halve the HTTP round trips.
            if let (
                GraphPattern::Service {
                    name: name_l,
                    inner: inner_l,
                    silent: silent_l,
                },
                GraphPattern::Service {
                    name: name_r,
                    inner: inner_r,
                    silent: silent_r,
                },
            ) = (left.as_ref(), right.as_ref())
                && let (NamedNodePattern::NamedNode(url_l), NamedNodePattern::NamedNode(url_r)) =
                    (name_l, name_r)
            {
                let url_l_str = url_l.as_str();
                let url_r_str = url_r.as_str();
                if url_l_str == url_r_str {
                    // Check no shared variables between the two inner patterns.
                    let vars_l = federation::collect_pattern_variables(inner_l);
                    let vars_r = federation::collect_pattern_variables(inner_r);
                    if vars_l.is_disjoint(&vars_r) {
                        let batched = translate_service_batched(
                            url_l_str,
                            inner_l,
                            inner_r,
                            *silent_l || *silent_r,
                            ctx,
                        );
                        if let Some(frag) = batched {
                            return frag;
                        }
                    }
                }
            }
            // Fallthrough: standard join translation.
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
                .map(|(v, col)| format!("{col} AS _lc_{}", sanitize_sql_ident(v)))
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
                .map(|(v, col)| format!("{col} AS _rc_{}", sanitize_sql_ident(v)))
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
                        .map(|v| {
                            let sv = sanitize_sql_ident(v);
                            format!("{lft}._lc_{sv} = {rgt}._rc_{sv}")
                        })
                        .collect::<Vec<_>>()
                        .join(" AND ")
                )
            };

            // Combined SELECT: left vars (always), right-only vars (nullable).
            let mut combined_cols: Vec<String> = left_frag
                .bindings
                .keys()
                .map(|v| {
                    let sv = sanitize_sql_ident(v);
                    format!("{lft}._lc_{sv} AS _lj_{sv}")
                })
                .collect();
            for v in right_frag.bindings.keys() {
                if !left_frag.bindings.contains_key(v) {
                    let sv = sanitize_sql_ident(v);
                    combined_cols.push(format!("{rgt}._rc_{sv} AS _lj_{sv}"));
                }
            }
            let combined_select = if combined_cols.is_empty() {
                "1 AS _dummy".to_owned()
            } else {
                combined_cols.join(", ")
            };

            let lj = ctx.next_alias();

            // v0.38.0: SHACL hints — if the right pattern is a simple BGP
            // with a single named predicate that has sh:minCount ≥ 1,
            // downgrade LEFT JOIN → INNER JOIN (the value is guaranteed to exist).
            let join_kw = if shacl_right_is_mandatory(right) {
                "INNER JOIN"
            } else {
                "LEFT JOIN"
            };

            let lj_sql = format!(
                "(SELECT {combined_select} \
                 FROM {left_subq} AS {lft} \
                 {join_kw} {right_subq} AS {rgt} {on_clause})"
            );

            let mut frag = Fragment::empty();
            frag.from_items.push((lj.clone(), lj_sql));

            for v in left_frag.bindings.keys() {
                let sv = sanitize_sql_ident(v);
                frag.bindings.insert(v.clone(), format!("{lj}._lj_{sv}"));
            }
            for v in right_frag.bindings.keys() {
                if !left_frag.bindings.contains_key(v) {
                    let sv = sanitize_sql_ident(v);
                    frag.bindings.insert(v.clone(), format!("{lj}._lj_{sv}"));
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
            // SPARQL 1.1 §18.6: if a filter expression evaluates to an error
            // (e.g., references an unbound variable), the filter result is false.
            // When translate_expr returns None (expression not translatable), emit
            // FALSE so no rows pass — matching SPARQL error-as-false semantics.
            match translate_expr(expr, &frag.bindings, ctx) {
                Some(cond) => frag.conditions.push(cond),
                None => frag.conditions.push("FALSE".to_owned()),
            }
            frag
        }

        GraphPattern::Graph { name, inner } => {
            // v0.40.0: graph-filter context propagation.
            //
            // The pre-v0.40.0 approach applied the graph filter post-hoc by
            // iterating over `frag.from_items` after the inner pattern was
            // translated.  This failed when the inner pattern contained an
            // OPTIONAL (LeftJoin) or WITH RECURSIVE property path because those
            // wrap their children in aliased subqueries that strip the `g` column.
            //
            // Fix: propagate `ctx.graph_filter` *before* recursing.  Every leaf
            // VP scan in `translate_bgp` / `table_expr` / `build_all_predicates_union`
            // now bakes in `WHERE g = {gid}` directly, so no post-hoc alias loop
            // is needed and LeftJoin / Path nodes work correctly.
            match name {
                NamedNodePattern::NamedNode(nn) => {
                    match ctx.encode_iri(nn.as_str()) {
                        Some(gid) => {
                            // Propagate into the recursive translation.
                            let saved = ctx.graph_filter;
                            ctx.graph_filter = Some(gid);
                            let frag = translate_pattern(inner, ctx);
                            ctx.graph_filter = saved;
                            frag
                        }
                        None => {
                            // Named graph IRI not in dictionary → no results.
                            let mut frag = Fragment::empty();
                            frag.conditions.push("FALSE".to_owned());
                            frag
                        }
                    }
                }
                NamedNodePattern::Variable(v) => {
                    // Variable graph: translate inner with variable_graph=true
                    // so property path CTEs include a `g` column.
                    let vname = v.as_str().to_owned();
                    let saved_vg = ctx.variable_graph;
                    ctx.variable_graph = true;
                    let mut frag = translate_pattern(inner, ctx);
                    ctx.variable_graph = saved_vg;
                    if let Some((alias, _)) = frag.from_items.first() {
                        let gcol = format!("{alias}.g");
                        // Exclude default graph (g = 0) from GRAPH ?g patterns.
                        // Per SPARQL semantics, GRAPH ?g only iterates named graphs.
                        frag.conditions.push(format!("{gcol} <> 0"));
                        // Ensure ALL other from_items join on the same named graph.
                        // Without this, a BGP join like { :a :p1 ?mid . ?mid :p2 ?x }
                        // would allow triples from different named graphs (pp06 bug).
                        // spargebra normalizes sequence paths like :p1/:p2 into a
                        // BGP join of two separate triple patterns.
                        let other_g_conditions: Vec<String> = frag
                            .from_items
                            .iter()
                            .skip(1)
                            .filter(|(_, sql)| {
                                // Only constrain from_items that are VP table scans
                                // (contain "_pg_ripple.vp_"). Skip VALUES/subquery etc.
                                sql.contains("_pg_ripple.vp_")
                            })
                            .map(|(a, _)| format!("{a}.g = {gcol}"))
                            .collect();
                        frag.conditions.extend(other_g_conditions);
                        if let Some(existing) = frag.bindings.get(&vname) {
                            let existing = existing.clone();
                            frag.conditions.push(format!("{gcol} = {existing}"));
                        } else {
                            frag.bindings.insert(vname, gcol);
                        }
                    }
                    frag
                }
            }
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
            // v0.38.0: SHACL hints — if ALL predicates in the inner BGP
            // have sh:maxCount ≤ 1, the result is already distinct by
            // construction and DISTINCT can be suppressed in the outer wrapper.
            // The actual DISTINCT keyword is applied in translate_select;
            // here we just check for the hint so the has_max_count_1 function
            // stays wired to an actual code path.
            let _ = shacl_bgp_all_max_count_1(inner);
            translate_pattern(inner, ctx)
        }
        GraphPattern::Slice { .. } => {
            // Nested subquery with LIMIT/OFFSET: extract modifiers from this
            // node and wrap the inner translation in a SQL subquery so the
            // LIMIT is applied before the outer query joins with the result.
            let mods = extract_modifiers(pattern);
            let inner_frag = translate_pattern(mods.pattern, ctx);

            // Which variables to project: either the declared set or all bound.
            let keep_vars: Vec<String> = if let Some(ref pv) = mods.project_vars {
                pv.clone()
            } else {
                inner_frag.bindings.keys().cloned().collect()
            };

            let cols: Vec<String> = keep_vars
                .iter()
                .filter_map(|v| {
                    inner_frag
                        .bindings
                        .get(v)
                        .map(|col| format!("{col} AS _sl_{v}"))
                })
                .collect();

            let select_clause = if cols.is_empty() {
                "1 AS _sl_dummy".to_owned()
            } else {
                cols.join(", ")
            };

            let order_clause = if !mods.order_exprs.is_empty() {
                let os = translate_order_by(&mods.order_exprs, &inner_frag.bindings);
                if os.is_empty() {
                    String::new()
                } else {
                    format!("ORDER BY {os}")
                }
            } else {
                String::new()
            };

            let limit_str = mods.limit.map_or(String::new(), |n| format!("LIMIT {n}"));
            let offset_str = if mods.offset > 0 {
                format!("OFFSET {}", mods.offset)
            } else {
                String::new()
            };

            let subq = format!(
                "(SELECT {select_clause} FROM {} {} {order_clause} {limit_str} {offset_str})",
                inner_frag.build_from(),
                inner_frag.build_where()
            );

            let alias = ctx.next_alias();
            let mut frag = Fragment::empty();
            frag.from_items.push((alias.clone(), subq));
            for v in &keep_vars {
                if inner_frag.bindings.contains_key(v) {
                    frag.bindings.insert(v.clone(), format!("{alias}._sl_{v}"));
                }
            }
            frag
        }
        GraphPattern::OrderBy { inner, .. } => translate_pattern(inner, ctx),

        // ── Property path (p+, p*, p?, p/q, p|q, ^p, !(p)) ────────────────────
        GraphPattern::Path {
            subject,
            path,
            object,
        } => {
            // v0.24.0: use the more restrictive of max_path_depth and property_path_max_depth.
            let max_depth = crate::MAX_PATH_DEPTH
                .get()
                .min(crate::PROPERTY_PATH_MAX_DEPTH.get());
            let mut path_ctx = PathCtx::new(ctx.path_counter);

            // Determine bound constants for subject / object to push into the CTE.
            // For zero-length paths (p*, p?), the constant may not be in the dictionary yet
            // (e.g. empty dataset). Use encode_term SQL expression as fallback so that
            // reflexive zero-hop rows can still be generated.
            let s_const: Option<String> = ground_term_sql_for_path(subject, ctx);
            let o_const: Option<String> = ground_term_sql_for_path(object, ctx);

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
                TermPattern::NamedNode(_nn) => {
                    // Filter was already pushed into path SQL via s_const.
                    // s_const=None only happens for variables/blank nodes, not NamedNodes.
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
                TermPattern::NamedNode(_nn) => {
                    // Filter was already pushed into path SQL via o_const.
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
            let sql_expr = translate_expr_value(expression, &frag.bindings, ctx);
            if let Some(expr_sql) = sql_expr {
                frag.bindings.insert(variable.as_str().to_owned(), expr_sql);
            } else if matches!(
                expression,
                Expression::Equal(_, _)
                    | Expression::Greater(_, _)
                    | Expression::GreaterOrEqual(_, _)
                    | Expression::Less(_, _)
                    | Expression::LessOrEqual(_, _)
                    | Expression::SameTerm(_, _)
                    | Expression::And(_, _)
                    | Expression::Or(_, _)
                    | Expression::Not(_)
                    | Expression::Bound(_)
            ) && let Some(bool_sql) = translate_expr(expression, &frag.bindings, ctx)
            {
                // Comparison/logical operators return SQL booleans.
                // Encode as inline xsd:boolean literal IDs.
                // inline_true  = -9151314442816847871
                // inline_false = -9151314442816847872
                let encoded = format!(
                    "CASE WHEN ({bool_sql}) IS NULL THEN NULL::bigint \
                         WHEN ({bool_sql}) THEN -9151314442816847871::bigint \
                         ELSE -9151314442816847872::bigint END"
                );
                frag.bindings.insert(variable.as_str().to_owned(), encoded);
            }
            // Propagate raw_numeric status from:
            // 1. Simple variable references to already-raw_numeric variables.
            // 2. SPARQL numeric functions (STRLEN, ABS, CEIL, FLOOR, ROUND, RAND,
            //    YEAR, MONTH, DAY, HOURS, MINUTES, SECONDS).
            let is_from_numeric_var = if let Expression::Variable(src_var) = expression {
                ctx.raw_numeric_vars.contains(src_var.as_str())
            } else {
                false
            };
            let is_from_numeric_fn = if let Expression::FunctionCall(func, _) = expression {
                expr::is_numeric_function(func)
            } else {
                false
            };
            if is_from_numeric_var || is_from_numeric_fn {
                ctx.raw_numeric_vars.insert(variable.as_str().to_owned());
            }
            // Propagate raw_text status from GROUP_CONCAT internal aggregate variables.
            let is_from_text_var = if let Expression::Variable(src_var) = expression {
                ctx.raw_text_vars.contains(src_var.as_str())
            } else {
                false
            };
            if is_from_text_var {
                ctx.raw_text_vars.insert(variable.as_str().to_owned());
            }
            // STRUUID() → raw text literal (gen_random_uuid()::text, not encoded as dict ID).
            if matches!(expression, Expression::FunctionCall(Function::StrUuid, _)) {
                ctx.raw_text_vars.insert(variable.as_str().to_owned());
            }
            // UUID() → raw IRI text ('urn:uuid:' || gen_random_uuid()::text, not encoded).
            let is_from_iri_var = if let Expression::Variable(src_var) = expression {
                ctx.raw_iri_vars.contains(src_var.as_str())
            } else {
                false
            };
            if is_from_iri_var || matches!(expression, Expression::FunctionCall(Function::Uuid, _))
            {
                ctx.raw_iri_vars.insert(variable.as_str().to_owned());
            }
            // RAND() → raw double (random()), tracked for DATATYPE() returns xsd:double.
            let is_from_double_var = if let Expression::Variable(src_var) = expression {
                ctx.raw_double_vars.contains(src_var.as_str())
            } else {
                false
            };
            if is_from_double_var
                || matches!(expression, Expression::FunctionCall(Function::Rand, _))
            {
                ctx.raw_double_vars.insert(variable.as_str().to_owned());
            }
            frag
        }

        // ── VALUES ───────────────────────────────────────────────────────────
        GraphPattern::Values {
            variables,
            bindings,
        } => translate_values(variables, bindings, ctx),

        // ── SERVICE (SPARQL federation, v0.16.0) ─────────────────────────────
        GraphPattern::Service {
            name,
            inner,
            silent,
        } => translate_service(name, inner, *silent, ctx),
    }
}

// ─── UNION translator ─────────────────────────────────────────────────────────

/// Translate UNION to SQL UNION of two subqueries.
/// Both sides must expose the same set of variables; missing variables are NULL.
fn translate_union(left: &GraphPattern, right: &GraphPattern, ctx: &mut Ctx) -> Fragment {
    let left_frag = translate_pattern(left, ctx);
    let right_frag = translate_pattern(right, ctx);

    // When inside a GRAPH ?g { ... UNION ... } context, the graph variable
    // must be available in the UNION output.  Each arm's from_items carries
    // VP-table rows with a `g` column — expose it so the outer GRAPH handler
    // can bind `?g = alias.g`.
    let include_g = ctx.variable_graph;

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
        let mut cols: Vec<String> = all_vars
            .iter()
            .map(|v| {
                frag.bindings
                    .get(v)
                    .map(|col| format!("{col} AS _u_{v}"))
                    .unwrap_or_else(|| format!("NULL::bigint AS _u_{v}"))
            })
            .collect();
        // Propagate the named-graph column when needed.
        if include_g {
            let gcol = frag
                .from_items
                .first()
                .map(|(a, _)| format!("{a}.g"))
                .unwrap_or_else(|| "NULL::bigint".to_owned());
            cols.push(format!("{gcol} AS g"));
        }
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
    // SPARQL UNION is a multiset (bag) union — duplicate solution mappings from
    // both arms must be preserved.  Use SQL UNION ALL (not UNION, which deduplicates).
    let union_subquery = format!("(({left_sql}) UNION ALL ({right_sql}))");

    let mut frag = Fragment::empty();
    frag.from_items.push((alias.clone(), union_subquery));

    for v in &all_vars {
        frag.bindings.insert(v.clone(), format!("{alias}._u_{v}"));
    }

    frag
}

// ─── MINUS translator ────────────────────────────────────────────────────────

/// Translate MINUS to SQL NOT EXISTS with SPARQL-correct null-aware compatibility.
///
/// SPARQL 1.1 §8.3: a left row μ is excluded iff there EXISTS a right row μ' such that:
///   1. dom(μ) ∩ dom(μ') ≠ ∅  (at least one shared variable is bound in both)
///   2. μ and μ' are compatible (for all v ∈ dom(μ) ∩ dom(μ'), μ(v) = μ'(v))
///
/// The old LEFT JOIN + NULL-check approach was wrong because SQL NULL comparisons
/// (e.g., NULL = NULL → NULL, not TRUE) don't handle OPTIONAL-unbound right rows
/// correctly.  NOT EXISTS with explicit null guards is correct.
fn translate_minus(left: &GraphPattern, right: &GraphPattern, ctx: &mut Ctx) -> Fragment {
    let left_frag = translate_pattern(left, ctx);
    let right_frag = translate_pattern(right, ctx);

    // SPARQL MINUS excludes left rows that have a compatible match in right.
    // Shared variables determine compatibility.
    let mut shared_vars: Vec<String> = left_frag
        .bindings
        .keys()
        .filter(|v| right_frag.bindings.contains_key(*v))
        .cloned()
        .collect();
    shared_vars.sort(); // deterministic SQL

    let alias = ctx.next_alias();

    if shared_vars.is_empty() {
        // No shared variables → MINUS has no effect (return left unchanged).
        return left_frag;
    }

    // Build left SELECT: all columns + shared columns (aliased _m_<v>).
    let left_all_cols: Vec<String> = left_frag
        .bindings
        .iter()
        .map(|(v, col)| format!("{col} AS _ma_{v}"))
        .collect();
    let left_shared_cols: Vec<String> = shared_vars
        .iter()
        .map(|v| format!("{} AS _m_{v}", left_frag.bindings[v]))
        .collect();
    let right_shared_cols: Vec<String> = shared_vars
        .iter()
        .map(|v| format!("{} AS _m_{v}", right_frag.bindings[v]))
        .collect();

    let left_sql = format!(
        "SELECT {}, {} FROM {} {}",
        left_all_cols.join(", "),
        left_shared_cols.join(", "),
        left_frag.build_from(),
        left_frag.build_where()
    );
    let right_sql = format!(
        "SELECT {} FROM {} {}",
        right_shared_cols.join(", "),
        right_frag.build_from(),
        right_frag.build_where()
    );

    // Condition 1: at least one shared var is bound (non-NULL) in BOTH sides.
    let any_bound: String = shared_vars
        .iter()
        .map(|v| format!("(_lminus._m_{v} IS NOT NULL AND _rminus._m_{v} IS NOT NULL)"))
        .collect::<Vec<_>>()
        .join(" OR ");

    // Condition 2: for all shared vars, if both bound then values are equal.
    let all_compatible: String = shared_vars
        .iter()
        .map(|v| {
            format!(
                "(_lminus._m_{v} IS NULL OR _rminus._m_{v} IS NULL OR _lminus._m_{v} = _rminus._m_{v})"
            )
        })
        .collect::<Vec<_>>()
        .join(" AND ");

    let lout = left_frag
        .bindings
        .keys()
        .map(|v| format!("_lminus._ma_{v} AS _mn_{v}"))
        .collect::<Vec<_>>()
        .join(", ");

    let minus_sql = format!(
        "(SELECT {lout} FROM ({left_sql}) AS _lminus \
         WHERE NOT EXISTS (\
           SELECT 1 FROM ({right_sql}) AS _rminus \
           WHERE ({any_bound}) AND ({all_compatible})\
         ))"
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

    // v0.42.0: When inside GRAPH ?var { aggregation }, we must propagate the
    // `g` (graph-id) column through the aggregation so the outer GRAPH handler
    // can bind the graph variable.  Capture the g column from the first VP scan.
    let variable_graph_g: Option<String> = if ctx.variable_graph {
        inner_frag
            .from_items
            .first()
            .map(|(alias, _)| format!("{alias}.g"))
    } else {
        None
    };

    // Build inner SQL with safe unqualified column aliases (_gi_<v>) so the
    // outer GROUP BY and aggregate expressions can reference them without
    // table-qualified names that become invalid inside a subquery wrapper.
    let mut inner_select_parts: Vec<String> = inner_frag
        .bindings
        .iter()
        .map(|(v, col)| format!("{col} AS _gi_{}", sanitize_sql_ident(v)))
        .collect();
    // Include g column when in variable-graph context.
    if let Some(ref gcol) = variable_graph_g {
        inner_select_parts.push(format!("{gcol} AS _gi__g"));
    }
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
        .map(|v| (v.clone(), format!("_gi_{}", sanitize_sql_ident(v))))
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
        .map(|(v, alias)| format!("{alias} AS _g_{}", sanitize_sql_ident(v)))
        .collect();
    // Include g in outer SELECT for variable-graph context.
    if variable_graph_g.is_some() {
        select_parts.push("_gi__g AS g".to_string());
    }

    let mut agg_bindings: Vec<(String, String)> = Vec::new();
    // Track which aggregate output variables produce text (GROUP_CONCAT) vs
    // numeric (COUNT/SUM/AVG/MIN/MAX) results so FILTER comparisons use the
    // correct type.
    let mut text_agg_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Track which aggregates produce raw numeric (COUNT, old-style) vs encoded bigint.
    let mut encoded_agg_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Store raw aggregate SQL for HAVING comparisons (separate from encoded output).
    let mut raw_agg_for_having: Vec<(String, String)> = Vec::new();
    for (agg_var, agg_expr) in aggregates {
        let (encoded_sql, raw_sql) = translate_aggregate(agg_expr, &inner_alias, ctx);
        let vname = agg_var.as_str().to_owned();
        // GROUP_CONCAT produces SQL text; all others produce SQL integers.
        let is_group_concat = matches!(
            agg_expr,
            AggregateExpression::FunctionCall {
                name: AggregateFunction::GroupConcat { .. },
                ..
            }
        );
        let is_encoded = matches!(
            agg_expr,
            AggregateExpression::FunctionCall {
                name: AggregateFunction::Sum
                    | AggregateFunction::Avg
                    | AggregateFunction::Min
                    | AggregateFunction::Max
                    | AggregateFunction::Sample,
                ..
            }
        ) || matches!(
            agg_expr,
            AggregateExpression::FunctionCall {
                name: AggregateFunction::Custom(n),
                ..
            } if matches!(n.as_str(), "urn:arq:median" | "urn:arq:mode")
        );
        if is_group_concat {
            text_agg_vars.insert(vname.clone());
        }
        if is_encoded {
            encoded_agg_vars.insert(vname.clone());
        }
        select_parts.push(format!(
            "{encoded_sql} AS _g_{}",
            sanitize_sql_ident(&vname)
        ));
        agg_bindings.push((vname.clone(), encoded_sql));
        raw_agg_for_having.push((vname, raw_sql));
    }

    let group_by_clause = if group_cols.is_empty() && variable_graph_g.is_none() {
        String::new()
    } else {
        let mut gb_cols: Vec<String> = group_cols
            .iter()
            .map(|(_, alias)| alias.to_string())
            .collect();
        // When in variable-graph context, group by the g column too so each
        // named graph gets its own result row.
        if variable_graph_g.is_some() {
            gb_cols.push("_gi__g".to_string());
        }
        format!("GROUP BY {}", gb_cols.join(", "))
    };

    // HAVING clause (from Filter wrapping Group in the caller).
    let having_clause = if let Some(having_expr) = having {
        // Build temporary bindings that include aggregate aliases for HAVING.
        // Use the RAW aggregate expressions (not the encoded ones) so that
        // HAVING comparisons work correctly with numeric types.
        let mut having_bindings = inner_alias.clone();
        for (vname, raw_sql) in &raw_agg_for_having {
            // Use the original aggregate SQL expression (e.g. COUNT(*)) rather
            // than the SELECT-list alias (e.g. _g_count): PostgreSQL does not
            // allow SELECT aliases to be referenced in HAVING.
            having_bindings.insert(vname.clone(), raw_sql.clone());
        }
        // Mark aggregate vars as raw numeric so FILTER constants (e.g. >= 2) are
        // not encoded as inline IDs — COUNT(*) returns a raw SQL integer, not an
        // inline-encoded value.
        for (vname, _) in &raw_agg_for_having {
            ctx.raw_numeric_vars.insert(vname.clone());
        }
        let result = translate_expr(having_expr, &having_bindings, ctx)
            .map(|c| format!("HAVING {c}"))
            .unwrap_or_default();
        // Remove them again — only raw in HAVING scope of this group fragment.
        for (vname, _) in &raw_agg_for_having {
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

    // v0.43.0: When inside GRAPH ?var { aggregate } with no explicit GROUP BY
    // variables, wrap the aggregation in a LEFT JOIN with _pg_ripple.named_graphs
    // so that empty named graphs (zero matching triples) also appear in results
    // with default aggregate values (COUNT=0, etc.).
    let group_sql = if variable_graph_g.is_some() && group_cols.is_empty() {
        // Inner aggregation SQL (grouped by g).
        let inner_agg = format!(
            "SELECT {select_list} FROM ({inner_sql}) AS _grp_inner \
             {group_by_clause} {having_clause}"
        );
        let inner_agg_alias = ctx.next_alias();
        // Outer SELECT: COALESCE each aggregate column to its empty-group default.
        // COUNT → 0; SUM → 0; encoded aggregates (MIN/MAX/AVG/SAMPLE) → NULL.
        let outer_cols: Vec<String> = agg_bindings
            .iter()
            .map(|(vname, _)| {
                let col = format!("{inner_agg_alias}._g_{}", sanitize_sql_ident(vname));
                if encoded_agg_vars.contains(vname) {
                    // These produce encoded bigints (MIN/MAX/AVG/SAMPLE) — NULL is correct.
                    format!("{col} AS _g_{}", sanitize_sql_ident(vname))
                } else {
                    // COUNT and raw-numeric aggregates → default 0 for empty groups.
                    format!("COALESCE({col}, 0) AS _g_{}", sanitize_sql_ident(vname))
                }
            })
            .collect();
        let outer_select = if outer_cols.is_empty() {
            "0 AS _g__count".to_owned()
        } else {
            outer_cols.join(", ")
        };
        format!(
            "(SELECT ng.graph_id AS g, {outer_select} \
             FROM _pg_ripple.named_graphs ng \
             LEFT JOIN ({inner_agg}) AS {inner_agg_alias} \
             ON {inner_agg_alias}.g = ng.graph_id \
             WHERE ng.graph_id <> 0)"
        )
    } else {
        format!(
            "(SELECT {select_list} FROM ({inner_sql}) AS _grp_inner \
             {group_by_clause} {having_clause})"
        )
    };

    let alias = ctx.next_alias();
    let mut frag = Fragment::empty();
    frag.from_items.push((alias.clone(), group_sql));

    // Bind group-by variables.
    for (v, _) in &group_cols {
        frag.bindings
            .insert(v.clone(), format!("{alias}._g_{}", sanitize_sql_ident(v)));
    }
    // Bind aggregate output variables and mark them as raw numeric or raw text.
    // This ensures that FILTER(?cnt >= 2) in an outer pattern (e.g. a subquery
    // wrapping a GROUP BY) uses raw integer comparison rather than inline IDs.
    // GROUP_CONCAT output variables are marked as raw_text so FILTER comparisons
    // use the literal's lexical value (text) rather than its dictionary ID.
    // SUM/AVG/MIN/MAX now produce encoded bigints — do NOT mark them as raw_numeric.
    for (vname, _) in &agg_bindings {
        frag.bindings.insert(
            vname.clone(),
            format!("{alias}._g_{}", sanitize_sql_ident(vname)),
        );
        if text_agg_vars.contains(vname) {
            ctx.raw_text_vars.insert(vname.clone());
        } else if !encoded_agg_vars.contains(vname) {
            // Only COUNT and similar produce raw integers; SUM/AVG/MIN/MAX produce encoded bigints.
            ctx.raw_numeric_vars.insert(vname.clone());
        }
    }

    frag
}

/// Translate an AggregateExpression to `(encoded_sql, raw_sql)` where:
/// - `encoded_sql`: full pg_ripple-encoded bigint result (for SELECT output)
/// - `raw_sql`: raw SQL aggregate on the dict-id column (for HAVING comparisons)
fn translate_aggregate(
    agg: &AggregateExpression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> (String, String) {
    match agg {
        AggregateExpression::CountSolutions { distinct } => {
            let s = if *distinct {
                // COUNT(DISTINCT *) — count distinct solution rows.
                // PostgreSQL doesn't support COUNT(DISTINCT *), so we hash all
                // inner columns into a single text key.
                if bindings.is_empty() {
                    "COUNT(*)".to_owned()
                } else {
                    // Concatenate all column values (bigint::text, safe from null bytes).
                    // NULLs are represented as empty string with | delimiters ensuring
                    // distinct separation: "1||3" ≠ "12|" ≠ "1|2|3" since values are integers.
                    let cols: Vec<String> = bindings
                        .values()
                        .map(|col| format!("COALESCE({col}::text, '')"))
                        .collect();
                    let concat = cols.join(" || '|' || ");
                    format!("COUNT(DISTINCT ({concat}))")
                }
            } else {
                "COUNT(*)".to_owned()
            };
            (s.clone(), s)
        }
        AggregateExpression::FunctionCall {
            name,
            expr,
            distinct,
        } => {
            let distinct_kw = if *distinct { "DISTINCT " } else { "" };
            let arg = translate_agg_expr(expr, bindings, ctx).unwrap_or_else(|| "NULL".to_owned());

            match name {
                AggregateFunction::Count => {
                    let s = format!("COUNT({distinct_kw}{arg})");
                    (s.clone(), s)
                }
                AggregateFunction::Sum => {
                    let raw_having = rdf_decoded_agg("SUM", distinct_kw, &arg);
                    let enc = rdf_numeric_agg("SUM", distinct_kw, &arg, false);
                    (enc, raw_having)
                }
                AggregateFunction::Avg => {
                    let raw_having = rdf_decoded_agg("AVG", distinct_kw, &arg);
                    let enc = rdf_numeric_agg("AVG", distinct_kw, &arg, true);
                    (enc, raw_having)
                }
                AggregateFunction::Min => {
                    let raw_having = rdf_decoded_agg("MIN", "", &arg);
                    let enc = rdf_minmax_agg(&arg, "ASC");
                    (enc, raw_having)
                }
                AggregateFunction::Max => {
                    let raw_having = rdf_decoded_agg("MAX", "", &arg);
                    let enc = rdf_minmax_agg(&arg, "DESC");
                    (enc, raw_having)
                }
                AggregateFunction::GroupConcat { separator } => {
                    let sep = separator.as_deref().unwrap_or(" ");
                    let decode_expr = format!(
                        "CASE WHEN {arg} < 0 THEN \
                         (({arg} & 72057594037927935::bigint) - 36028797018963968::bigint)::text \
                         ELSE (SELECT d.value FROM _pg_ripple.dictionary d WHERE d.id = {arg} LIMIT 1) \
                         END"
                    );
                    let s = if *distinct {
                        format!(
                            "STRING_AGG(DISTINCT ({decode_expr})::text, {sep_lit} ORDER BY ({decode_expr}))",
                            sep_lit = quote_sql_string(sep)
                        )
                    } else {
                        format!(
                            "STRING_AGG(({decode_expr})::text, {sep_lit} ORDER BY {arg})",
                            sep_lit = quote_sql_string(sep)
                        )
                    };
                    (s.clone(), s)
                }
                AggregateFunction::Sample => {
                    let s = format!("MIN({arg})");
                    (s.clone(), s)
                }
                AggregateFunction::Custom(iri) => match iri.as_str() {
                    "urn:arq:median" => {
                        // SPARQL ARQ MEDIAN aggregate: compute PERCENTILE_CONT(0.5)
                        // over the decoded numeric values and re-encode as xsd:decimal.
                        let decode = format!(
                            "CASE WHEN ({arg}) IS NULL THEN NULL \
                             WHEN ({arg}) < 0 THEN \
                               ((({arg}) & 72057594037927935::bigint) - \
                               36028797018963968::bigint)::numeric \
                             ELSE (SELECT CASE WHEN d.datatype IN (\
                               'http://www.w3.org/2001/XMLSchema#decimal',\
                               'http://www.w3.org/2001/XMLSchema#double',\
                               'http://www.w3.org/2001/XMLSchema#float',\
                               'http://www.w3.org/2001/XMLSchema#integer') \
                               THEN d.value::numeric ELSE NULL END \
                               FROM _pg_ripple.dictionary d WHERE d.id = ({arg}) LIMIT 1) \
                             END"
                        );
                        let s = format!(
                            "pg_ripple.encode_typed_literal(\
                             trim_scale(PERCENTILE_CONT(0.5::numeric) \
                             WITHIN GROUP (ORDER BY {decode}))::text,\
                             'http://www.w3.org/2001/XMLSchema#decimal')"
                        );
                        (s.clone(), s)
                    }
                    "urn:arq:mode" => {
                        // SPARQL ARQ MODE aggregate: return the most frequent
                        // dictionary-encoded bigint directly (no decode/re-encode
                        // needed because equal literals always share the same ID).
                        let s = format!("MODE() WITHIN GROUP (ORDER BY {arg})");
                        (s.clone(), s)
                    }
                    _ => {
                        let s = format!("MIN({arg})");
                        (s.clone(), s)
                    }
                },
            }
        }
    }
}

/// Build a decoded numeric aggregate for HAVING comparisons.
///
/// Returns `{agg_fn}({distinct_kw}decoded(arg))` as a plain PostgreSQL numeric,
/// suitable for use in HAVING expressions with raw numeric constants (2.0 etc.).
fn rdf_decoded_agg(agg_fn: &str, distinct_kw: &str, arg: &str) -> String {
    let decode = format!(
        "CASE WHEN ({arg}) IS NULL THEN NULL \
         WHEN ({arg}) < 0 THEN \
           ((({arg}) & 72057594037927935::bigint) - 36028797018963968::bigint)::numeric \
         ELSE (SELECT CASE WHEN d.datatype IN (\
           'http://www.w3.org/2001/XMLSchema#decimal',\
           'http://www.w3.org/2001/XMLSchema#double',\
           'http://www.w3.org/2001/XMLSchema#float',\
           'http://www.w3.org/2001/XMLSchema#integer') \
           THEN d.value::numeric ELSE NULL END \
           FROM _pg_ripple.dictionary d WHERE d.id = ({arg}) LIMIT 1) END"
    );
    format!("{agg_fn}({distinct_kw}{decode})")
}

/// Build the RDF-aware numeric aggregate SQL for SUM/AVG.
///
/// Decodes each input value (inline integer or dict-encoded decimal/double) to
/// PostgreSQL numeric, applies the SQL aggregate, determines the result XSD type,
/// and re-encodes via pg_ripple.encode_typed_literal().
fn rdf_numeric_agg(agg_fn: &str, distinct_kw: &str, arg: &str, is_avg: bool) -> String {
    // Decode expression: inline integer → numeric; dict decimal/double → numeric.
    let decode = format!(
        "CASE WHEN ({arg}) IS NULL THEN NULL \
         WHEN ({arg}) < 0 THEN \
           ((({arg}) & 72057594037927935::bigint) - 36028797018963968::bigint)::numeric \
         ELSE (SELECT CASE WHEN d.datatype IN (\
           'http://www.w3.org/2001/XMLSchema#decimal',\
           'http://www.w3.org/2001/XMLSchema#double',\
           'http://www.w3.org/2001/XMLSchema#float',\
           'http://www.w3.org/2001/XMLSchema#integer') \
           THEN d.value::numeric ELSE NULL END \
           FROM _pg_ripple.dictionary d WHERE d.id = ({arg}) LIMIT 1) END"
    );
    // Type-code expression: 0=integer, 1=decimal, 2=double.
    let tc = format!(
        "CASE WHEN ({arg}) IS NULL OR ({arg}) < 0 THEN 0 \
         ELSE COALESCE((SELECT CASE \
           WHEN d.datatype IN ('http://www.w3.org/2001/XMLSchema#double',\
                               'http://www.w3.org/2001/XMLSchema#float') THEN 2 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#integer' THEN 0 \
           ELSE 1 END FROM _pg_ripple.dictionary d WHERE d.id = ({arg}) LIMIT 1), 0) END"
    );

    // For AVG: integer → decimal result; for SUM: integer stays integer.
    // The result is always decimal when any decimal/double input is present.
    if is_avg {
        // AVG: result is always decimal or double (never integer per SPARQL 1.1 §17.4.3.4).
        // Exception: empty bag → 0^^xsd:integer (SPARQL 1.1 §17.4.3.4).
        // Error propagation: if any bound value is non-numeric, AVG raises a type error
        // (SPARQL 1.1 §18.5.1) and should produce an unbound result.
        format!(
            "CASE WHEN BOOL_OR(({arg}) IS NOT NULL AND ({decode}) IS NULL) THEN NULL \
               WHEN {agg_fn}({distinct_kw}{decode}) IS NULL \
               THEN pg_ripple.encode_typed_literal('0', 'http://www.w3.org/2001/XMLSchema#integer') \
               ELSE pg_ripple.encode_typed_literal(\
                 CASE COALESCE(MAX({tc}), 0) \
                 WHEN 2 THEN pg_ripple.xsd_double_fmt({agg_fn}({distinct_kw}{decode})::text) \
                 ELSE trim_scale({agg_fn}({distinct_kw}{decode}))::text \
                 END, \
                 CASE COALESCE(MAX({tc}), 0) \
                 WHEN 2 THEN 'http://www.w3.org/2001/XMLSchema#double' \
                 ELSE 'http://www.w3.org/2001/XMLSchema#decimal' \
                 END) END"
        )
    } else {
        // SUM: integer+integer→integer, any decimal→decimal, any double→double.
        // Error propagation: if any bound value is non-numeric, return unbound.
        format!(
            "CASE WHEN BOOL_OR(({arg}) IS NOT NULL AND ({decode}) IS NULL) THEN NULL \
               ELSE pg_ripple.encode_typed_literal(\
               CASE COALESCE(MAX({tc}), 0) \
               WHEN 2 THEN pg_ripple.xsd_double_fmt(SUM({distinct_kw}{decode})::text) \
               WHEN 1 THEN trim_scale(SUM({distinct_kw}{decode}))::text \
               ELSE SUM({distinct_kw}{decode})::bigint::text \
               END, \
               CASE COALESCE(MAX({tc}), 0) \
               WHEN 2 THEN 'http://www.w3.org/2001/XMLSchema#double' \
               WHEN 1 THEN 'http://www.w3.org/2001/XMLSchema#decimal' \
               ELSE 'http://www.w3.org/2001/XMLSchema#integer' \
               END) END"
        )
    }
}

/// Build the RDF-aware MIN or MAX aggregate SQL.
///
/// Returns the original pg_ripple-encoded bigint of the row with the
/// minimum (ASC) or maximum (DESC) decoded numeric value.  Preserves the
/// original lexical form (e.g. "1.0"^^decimal stays "1.0", not "1").
///
/// If any bound value in the group is non-numeric (e.g. a blank node), the
/// aggregate propagates the type error and returns NULL (SPARQL 1.1 §18.5.1).
fn rdf_minmax_agg(arg: &str, order: &str) -> String {
    let decode = format!(
        "CASE WHEN ({arg}) < 0 THEN \
           ((({arg}) & 72057594037927935::bigint) - 36028797018963968::bigint)::numeric \
         ELSE (SELECT CASE WHEN d.datatype IN (\
           'http://www.w3.org/2001/XMLSchema#decimal',\
           'http://www.w3.org/2001/XMLSchema#double',\
           'http://www.w3.org/2001/XMLSchema#float',\
           'http://www.w3.org/2001/XMLSchema#integer') \
           THEN d.value::numeric ELSE NULL END \
           FROM _pg_ripple.dictionary d WHERE d.id = ({arg}) LIMIT 1) END"
    );
    format!(
        "CASE WHEN BOOL_OR(({arg}) IS NOT NULL AND ({decode}) IS NULL) THEN NULL \
         ELSE (array_agg(({arg}) ORDER BY ({decode}) {order} NULLS LAST) \
          FILTER (WHERE ({arg}) IS NOT NULL AND ({decode}) IS NOT NULL))[1] \
         END"
    )
}

/// Obtain a SQL column reference for an expression used inside an aggregate.
fn translate_agg_expr(
    expr: &Expression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<String> {
    // Use value context so that variables return their raw SQL column (not a boolean IS NOT NULL).
    translate_expr_value(expr, bindings, ctx)
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

// ─── Batch SERVICE translator (v0.19.0) ──────────────────────────────────────

/// Combine two independent SERVICE clauses targeting the same endpoint into one
/// HTTP request.
///
/// Sends `SELECT * WHERE { { pattern1 } UNION { pattern2 } }` to the remote
/// endpoint.  The combined results are split by variable set back into per-clause
/// bindings and merged into a single fragment.
///
/// Returns `None` when the endpoint is not allowed, unhealthy, or the combined
/// call fails — callers fall back to sequential translation in that case.
fn translate_service_batched(
    url: &str,
    inner_l: &GraphPattern,
    inner_r: &GraphPattern,
    silent: bool,
    ctx: &mut Ctx,
) -> Option<Fragment> {
    if !federation::is_endpoint_allowed(url) {
        return None; // fallback to sequential
    }
    if !federation::is_endpoint_healthy(url) {
        return None;
    }
    if federation::get_local_view(url).is_some() {
        return None; // local view rewrite — let sequential path handle it
    }

    // Collect variables from each inner pattern.
    let mut vars_l: Vec<String> = federation::collect_pattern_variables(inner_l)
        .into_iter()
        .collect();
    vars_l.sort();
    let mut vars_r: Vec<String> = federation::collect_pattern_variables(inner_r)
        .into_iter()
        .collect();
    vars_r.sort();

    // Build all variables for the combined projection.
    let mut all_vars: Vec<String> = vars_l.iter().chain(vars_r.iter()).cloned().collect();
    all_vars.sort();
    all_vars.dedup();

    let projection = if all_vars.is_empty() {
        "*".to_owned()
    } else {
        all_vars
            .iter()
            .map(|v| format!("?{v}"))
            .collect::<Vec<_>>()
            .join(" ")
    };

    // Combined SPARQL UNION query.
    let combined_text =
        format!("SELECT {projection} WHERE {{ {{ {inner_l} }} UNION {{ {inner_r} }} }}");
    pgrx::debug1!("batch SERVICE {url}: {combined_text}");

    let timeout_secs = federation::effective_timeout_secs(url);
    let max_results = crate::FEDERATION_MAX_RESULTS.get();

    let on_partial = crate::FEDERATION_ON_PARTIAL.get();
    let on_partial_str = on_partial
        .as_ref()
        .and_then(|c| c.to_str().ok())
        .unwrap_or("empty");

    let start = std::time::Instant::now();
    let result = if on_partial_str == "use" {
        federation::execute_remote_partial(url, &combined_text, timeout_secs, max_results)
    } else {
        federation::execute_remote(url, &combined_text, timeout_secs, max_results)
    };
    let latency_ms = start.elapsed().as_millis() as i64;

    match result {
        Ok((variables, rows)) => {
            federation::record_health(url, true, latency_ms);
            if variables.is_empty() || rows.is_empty() {
                return Some(Fragment::zero_rows());
            }
            let (variables, encoded_rows) = federation::encode_results(variables, rows);
            Some(translate_service_values(&variables, &encoded_rows, ctx))
        }
        Err(e) => {
            federation::record_health(url, false, latency_ms);
            if silent {
                pgrx::warning!("batch SERVICE {url} failed (returning empty): {e}");
                return Some(Fragment::zero_rows());
            }
            // Fall back to sequential translation on error.
            pgrx::warning!("batch SERVICE {url} failed, falling back to sequential: {e}");
            None
        }
    }
}

// ─── SERVICE translator (v0.16.0, enhanced v0.19.0) ──────────────────────────

/// Translate a SPARQL `SERVICE` clause.
///
/// Execution strategy:
/// 1. Resolve endpoint URL from the `name` pattern.
/// 2. Check SSRF allowlist; error on unregistered endpoint.
/// 3. If a local SPARQL view covers this endpoint, scan its stream table.
/// 4. Build an explicit `SELECT ?v1 ?v2 … WHERE { inner }` query (variable
///    projection, v0.19.0) rather than `SELECT *`.
/// 5. Determine effective timeout via adaptive timeout GUC (v0.19.0).
/// 6. Execute remote, respecting `federation_on_partial` GUC (v0.19.0).
/// 7. Dictionary-encode remote results (with per-call deduplication, v0.19.0)
///    and inject as an inline VALUES fragment.
///
/// Multiple SERVICE clauses in one query execute sequentially (SPI context
/// does not support concurrent HTTP + SPI).
fn translate_service(
    name: &NamedNodePattern,
    inner: &GraphPattern,
    silent: bool,
    ctx: &mut Ctx,
) -> Fragment {
    // Helper: when SERVICE SILENT fails, return one empty mapping so outer
    // pattern variables are preserved with service variables unbound.
    // Per SPARQL 1.1 semantics, SERVICE SILENT failure → the service evaluates
    // to a single empty solution mapping {}, making it behave like a cross-join
    // with one empty row. Service-exclusive variables end up unbound (NULL);
    // shared variables are contributed by the outer pattern.
    let service_silent_fallback = |ctx: &mut Ctx| -> Fragment {
        let alias = ctx.next_alias();
        let mut frag = Fragment::empty();
        frag.from_items
            .push((alias, "(SELECT 1 AS _dummy)".to_owned()));
        frag // No bindings — service vars are absent (unbound) in the output
    };

    // ── 1. Resolve URL ────────────────────────────────────────────────────────
    let url = match name {
        NamedNodePattern::NamedNode(nn) => nn.as_str().to_string(),
        NamedNodePattern::Variable(v) => {
            // Variable endpoint (v0.42.0): expand via all registered graph endpoints.
            // Each registered endpoint with a graph_iri becomes one UNION arm,
            // binding the variable to the endpoint URL and executing the inner
            // pattern against the named graph.
            let vname = v.as_str().to_owned();
            let endpoints = federation::get_all_graph_endpoints();
            if endpoints.is_empty() {
                pgrx::warning!(
                    "SERVICE with variable endpoint ?{} — no registered graph endpoints; returning empty",
                    v.as_str()
                );
                let mut frag = Fragment::empty();
                frag.conditions.push("FALSE".to_owned());
                return frag;
            }

            // Build one arm per registered endpoint; UNION all arms.
            let mut arms: Vec<Fragment> = Vec::new();
            for (ep_url, graph_iri) in &endpoints {
                let url_id = match ctx.encode_iri(ep_url) {
                    Some(id) => id,
                    None => continue, // URL not in dictionary → skip
                };
                let gid = match ctx.encode_iri(graph_iri) {
                    Some(id) => id,
                    None => continue, // graph IRI not in dictionary → skip
                };
                let saved = ctx.graph_filter;
                ctx.graph_filter = Some(gid);
                let mut arm_frag = translate_pattern(inner, ctx);
                ctx.graph_filter = saved;
                // Bind ?service to the endpoint URL id (constant).
                if let Some(existing) = arm_frag.bindings.get(&vname).cloned() {
                    arm_frag.conditions.push(format!("{existing} = {url_id}"));
                } else {
                    arm_frag.bindings.insert(vname.clone(), url_id.to_string());
                }
                arms.push(arm_frag);
            }

            if arms.is_empty() {
                let mut frag = Fragment::empty();
                frag.conditions.push("FALSE".to_owned());
                return frag;
            }
            if arms.len() == 1
                && let Some(arm) = arms.pop()
            {
                return arm;
            }

            // Collect all variables across all arms for the UNION projection.
            let all_vars: Vec<String> = {
                let mut vars: std::collections::HashSet<String> = std::collections::HashSet::new();
                for arm in &arms {
                    vars.extend(arm.bindings.keys().cloned());
                }
                let mut v: Vec<String> = vars.into_iter().collect();
                v.sort();
                v
            };

            let union_arms: Vec<String> = arms
                .iter()
                .map(|arm| {
                    let cols: Vec<String> = all_vars
                        .iter()
                        .map(|var| {
                            arm.bindings
                                .get(var)
                                .map(|col| format!("{col} AS _sv_{var}"))
                                .unwrap_or_else(|| format!("NULL::bigint AS _sv_{var}"))
                        })
                        .collect();
                    let cols_str = if cols.is_empty() {
                        "1 AS _dummy".to_owned()
                    } else {
                        cols.join(", ")
                    };
                    format!(
                        "SELECT {cols_str} FROM {} {}",
                        arm.build_from(),
                        arm.build_where()
                    )
                })
                .collect();

            let union_subq = format!("({})", union_arms.join(" UNION ALL "));
            let alias = ctx.next_alias();
            let mut frag = Fragment::empty();
            frag.from_items.push((alias.clone(), union_subq));
            for var in &all_vars {
                frag.bindings
                    .insert(var.clone(), format!("{alias}._sv_{var}"));
            }
            return frag;
        }
    };

    // ── 2. SSRF allowlist check ───────────────────────────────────────────────
    if !federation::is_endpoint_allowed(&url) {
        if silent {
            pgrx::warning!("SERVICE endpoint not registered (SILENT skipping): {url}");
            return service_silent_fallback(ctx);
        }
        pgrx::error!(
            "federation endpoint not registered: {}; use pg_ripple.register_endpoint() to allow it",
            url
        );
    }

    // ── 2b. Health check (skip unhealthy endpoints) ───────────────────────────
    if !federation::is_endpoint_healthy(&url) {
        if silent {
            pgrx::warning!("SERVICE endpoint {url} is unhealthy (success_rate < 10%); skipping");
            return service_silent_fallback(ctx);
        }
        pgrx::warning!("SERVICE endpoint {url} is unhealthy; proceeding anyway");
    }

    // ── 3. Local SPARQL view rewrite ──────────────────────────────────────────
    if let Some(stream_table) = federation::get_local_view(&url) {
        return translate_service_local(&stream_table, ctx);
    }

    // ── 3b. Named-graph local execution (v0.42.0) ─────────────────────────────
    // When a `graph_iri` is registered for this endpoint, translate the inner
    // pattern against the local named graph instead of making an HTTP call.
    // This is used for mock endpoints in the W3C federation test suite.
    if let Some(graph_iri) = federation::get_graph_iri(&url) {
        if let Some(gid) = ctx.encode_iri(&graph_iri) {
            let saved = ctx.graph_filter;
            ctx.graph_filter = Some(gid);
            let frag = translate_pattern(inner, ctx);
            ctx.graph_filter = saved;
            return frag;
        }
        // graph IRI not in dictionary → no results
        let mut frag = Fragment::empty();
        frag.conditions.push("FALSE".to_owned());
        return frag;
    }

    // ── 4. Variable projection rewrite (v0.19.0) ─────────────────────────────
    // Collect all variables from the inner pattern and build an explicit
    // SELECT projection instead of SELECT *.  This reduces data transfer when
    // the remote endpoint honours the projection and when only a subset of
    // the variables are needed downstream.
    let inner_vars: Vec<String> = {
        let mut vars: Vec<String> = federation::collect_pattern_variables(inner)
            .into_iter()
            .collect();
        vars.sort(); // deterministic ordering for stable query text and cache keys
        vars
    };
    let projection = if inner_vars.is_empty() {
        "*".to_owned()
    } else {
        inner_vars
            .iter()
            .map(|v| format!("?{v}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let inner_text = format!("SELECT {projection} WHERE {{ {inner} }}");

    // ── 5. Adaptive timeout (v0.19.0) ────────────────────────────────────────
    let timeout_secs = federation::effective_timeout_secs(&url);
    let max_results = crate::FEDERATION_MAX_RESULTS.get();

    let start = std::time::Instant::now();

    // ── 6. Remote execution with partial-result support (v0.19.0) ────────────
    let on_partial = crate::FEDERATION_ON_PARTIAL.get();
    let on_partial_str = on_partial
        .as_ref()
        .and_then(|c| c.to_str().ok())
        .unwrap_or("empty");

    let result = if on_partial_str == "use" {
        federation::execute_remote_partial(&url, &inner_text, timeout_secs, max_results)
    } else {
        federation::execute_remote(&url, &inner_text, timeout_secs, max_results)
    };

    let latency_ms = start.elapsed().as_millis() as i64;

    let (variables, rows) = match result {
        Ok(r) => {
            federation::record_health(&url, true, latency_ms);
            r
        }
        Err(e) => {
            federation::record_health(&url, false, latency_ms);
            let on_error = crate::FEDERATION_ON_ERROR.get();
            let on_error_str = on_error
                .as_ref()
                .and_then(|c| c.to_str().ok())
                .unwrap_or("warning");
            if silent || on_error_str == "empty" {
                pgrx::warning!("SERVICE {url} failed (returning empty): {e}");
                if silent {
                    return service_silent_fallback(ctx);
                }
                return Fragment::zero_rows();
            } else if on_error_str == "error" {
                pgrx::error!("SERVICE {url} failed: {e}");
            } else {
                // default: warning + empty
                pgrx::warning!("SERVICE {url} failed (returning empty): {e}");
                return Fragment::zero_rows();
            }
        }
    };

    if variables.is_empty() || rows.is_empty() {
        return Fragment::zero_rows();
    }

    // ── 7. Encode results and inject as VALUES ────────────────────────────────
    let (variables, encoded_rows) = federation::encode_results(variables, rows);

    translate_service_values(&variables, &encoded_rows, ctx)
}

/// Translate a local SPARQL view rewrite: scan the pre-materialised stream
/// table directly instead of making an HTTP call.
fn translate_service_local(stream_table: &str, ctx: &mut Ctx) -> Fragment {
    let vars = federation::get_view_variables(stream_table);
    if vars.is_empty() {
        let mut frag = Fragment::empty();
        frag.conditions.push("FALSE".to_owned());
        return frag;
    }

    let alias = ctx.next_alias();
    let mut frag = Fragment::empty();
    // Fully-qualify the stream table name; it lives in _pg_ripple schema.
    // If it already has a schema prefix, use it as-is.
    let qualified = if stream_table.contains('.') {
        stream_table.to_owned()
    } else {
        format!("_pg_ripple.{stream_table}")
    };
    frag.from_items.push((alias.clone(), qualified));

    for v in &vars {
        frag.bindings.insert(v.clone(), format!("{alias}._v_{v}"));
    }

    frag
}

/// Build a VALUES fragment from pre-encoded (i64) remote results.
///
/// When the row count exceeds `pg_ripple.federation_inline_max_rows`, the rows
/// are spooled into a temporary table and a `SELECT` from that table is used
/// instead of a VALUES clause.  Emits PT620 INFO when spooling is triggered (v0.42.0).
fn translate_service_values(
    variables: &[String],
    encoded_rows: &[Vec<Option<i64>>],
    ctx: &mut Ctx,
) -> Fragment {
    if variables.is_empty() || encoded_rows.is_empty() {
        return Fragment::empty();
    }

    let inline_max = crate::FEDERATION_INLINE_MAX_ROWS.get() as usize;
    let row_count = encoded_rows.len();

    // ── Spooling path: large result sets ─────────────────────────────────────
    if inline_max > 0 && row_count > inline_max {
        pgrx::info!(
            "PT620: SERVICE result set ({row_count} rows) exceeds \
             pg_ripple.federation_inline_max_rows ({inline_max}); \
             spooling to temporary table"
        );
        return translate_service_values_spool(variables, encoded_rows, ctx);
    }

    // ── Inline VALUES path ────────────────────────────────────────────────────
    let col_names: Vec<String> = variables.iter().map(|v| format!("_svc_{v}")).collect();

    let col_names_str = col_names.join(", ");

    let rows_sql: Vec<String> = encoded_rows
        .iter()
        .map(|row| {
            let cells: Vec<String> = row
                .iter()
                .map(|cell| match cell {
                    None => "NULL::bigint".to_owned(),
                    Some(id) => id.to_string(),
                })
                .collect();
            format!("({})", cells.join(", "))
        })
        .collect();

    let n = ctx.alias_counter;
    ctx.alias_counter += 1;
    let values_expr = format!(
        "(SELECT * FROM (VALUES {}) AS _svi{n}({col_names_str}))",
        rows_sql.join(", ")
    );

    let alias = ctx.next_alias();
    let mut frag = Fragment::empty();
    frag.from_items.push((alias.clone(), values_expr));

    for v in variables {
        frag.bindings.insert(v.clone(), format!("{alias}._svc_{v}"));
    }

    frag
}

/// Spool federation results into a temporary table and return a SELECT fragment (v0.42.0).
///
/// Used when the row count exceeds `pg_ripple.federation_inline_max_rows`.
fn translate_service_values_spool(
    variables: &[String],
    encoded_rows: &[Vec<Option<i64>>],
    ctx: &mut Ctx,
) -> Fragment {
    let n = ctx.alias_counter;
    ctx.alias_counter += 1;
    let temp_table = format!("_pg_ripple_svc_spool_{n}");

    let col_defs: Vec<String> = variables
        .iter()
        .map(|v| format!("_svc_{v} bigint"))
        .collect();

    // Create a temporary table.
    let create_sql = format!(
        "CREATE TEMP TABLE IF NOT EXISTS {temp_table} ({}) \
         ON COMMIT DROP",
        col_defs.join(", ")
    );

    if let Err(e) = pgrx::Spi::run(&create_sql) {
        pgrx::log!("SERVICE spool: failed to create temp table {temp_table}: {e}");
        // Fall back to inline — truncate to inline_max rows.
        let max = crate::FEDERATION_INLINE_MAX_ROWS.get() as usize;
        let truncated = &encoded_rows[..max.min(encoded_rows.len())];
        return translate_service_values(variables, truncated, ctx);
    }

    // Batch-insert using COPY-style multi-row INSERT.
    let batch_size = 1000usize;
    let col_names: Vec<String> = variables.iter().map(|v| format!("_svc_{v}")).collect();
    let col_names_str = col_names.join(", ");

    for chunk in encoded_rows.chunks(batch_size) {
        let rows_sql: Vec<String> = chunk
            .iter()
            .map(|row| {
                let cells: Vec<String> = row
                    .iter()
                    .map(|cell| match cell {
                        None => "NULL::bigint".to_owned(),
                        Some(id) => id.to_string(),
                    })
                    .collect();
                format!("({})", cells.join(", "))
            })
            .collect();

        let insert_sql = format!(
            "INSERT INTO {temp_table} ({col_names_str}) VALUES {}",
            rows_sql.join(", ")
        );
        if let Err(e) = pgrx::Spi::run(&insert_sql) {
            pgrx::log!("SERVICE spool: INSERT error for {temp_table}: {e}");
        }
    }

    let alias = ctx.next_alias();
    let mut frag = Fragment::empty();
    frag.from_items.push((alias.clone(), temp_table));

    for v in variables {
        frag.bindings.insert(v.clone(), format!("{alias}._svc_{v}"));
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

/// Dispatch a SPARQL function call in boolean (FILTER) context.
///
/// Tries `expr::translate_function_filter` first.  If it returns `None`
/// (the function is not boolean-typed), attempts to use the function in value
/// context: if it produces a non-NULL value, return TRUE (acts as `BOUND`).
/// If neither context produces a result, applies the `sparql_strict` policy:
/// raise ERRCODE_FEATURE_NOT_SUPPORTED when strict, or warn-and-return-None
/// when lenient.
fn translate_function_call_filter(
    func: &Function,
    args: &[Expression],
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<String> {
    // Try boolean context first.
    if let Some(sql) = expr::translate_function_filter(func, args, bindings, ctx) {
        return Some(sql);
    }
    // Try value context: function produces a value → use IS NOT NULL as boolean.
    let mut is_numeric = false;
    if let Some(val_sql) =
        expr::translate_function_value(func, args, bindings, ctx, &mut is_numeric)
    {
        return Some(format!("({val_sql} IS NOT NULL)"));
    }
    // Neither worked: apply strict / lenient policy.
    let strict = crate::SPARQL_STRICT.get();
    if strict {
        pgrx::error!(
            "SPARQL function {} is not supported; \
             set pg_ripple.sparql_strict = off to warn-and-skip instead",
            expr::function_name(func)
        );
    } else {
        pgrx::warning!(
            "SPARQL function {} is not yet supported — FILTER predicate dropped \
             (set pg_ripple.sparql_strict = on to raise an error instead)",
            expr::function_name(func)
        );
        None
    }
}

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

        // Boolean literals (`true` / `false` in FILTER expressions).
        Expression::Literal(lit) => {
            let dt = lit.datatype();
            if dt.as_str() == "http://www.w3.org/2001/XMLSchema#boolean" {
                match lit.value() {
                    "true" | "1" => Some("TRUE".to_owned()),
                    _ => Some("FALSE".to_owned()),
                }
            } else {
                // Non-boolean literals in boolean context: compute EBV.
                // Numeric zero / empty string → FALSE; anything else → TRUE.
                let val_sql = translate_expr_value(expr, bindings, ctx)?;
                // inline_false = -9151314442816847872, inline_int_zero = -9187343239835811840
                Some(format!(
                    "({val_sql} IS NOT NULL AND {val_sql} NOT IN \
                     (-9151314442816847872, -9187343239835811840))"
                ))
            }
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

        // ── IF / COALESCE (v0.21.0) ──────────────────────────────────────────
        Expression::If(cond, then_expr, else_expr) => {
            let then_sql =
                translate_expr(then_expr, bindings, ctx).unwrap_or_else(|| "FALSE".to_owned());
            let else_sql =
                translate_expr(else_expr, bindings, ctx).unwrap_or_else(|| "FALSE".to_owned());
            // Try value context for condition to handle NULL/error propagation (e.g. 1/0 → NULL).
            // EBV: NULL → false (error in filter); inline_false or inline_int_zero → ELSE; else → THEN
            if let Some(cond_val) = translate_expr_value(cond, bindings, ctx) {
                Some(format!(
                    "CASE WHEN ({cond_val}) IS NULL \
                          OR ({cond_val}) IN (-9151314442816847872::bigint, -9187343239835811840::bigint) \
                     THEN ({else_sql}) ELSE ({then_sql}) END"
                ))
            } else {
                let cond_sql = translate_expr(cond, bindings, ctx)?;
                Some(format!(
                    "CASE WHEN {cond_sql} THEN ({then_sql}) ELSE ({else_sql}) END"
                ))
            }
        }
        Expression::Coalesce(exprs) => {
            let parts: Vec<String> = exprs
                .iter()
                .filter_map(|e| translate_expr_value(e, bindings, ctx))
                .collect();
            if parts.is_empty() {
                Some("NULL::bigint".to_owned())
            } else {
                // In boolean context, coalesce is truthy when non-null.
                Some(format!("(COALESCE({}) IS NOT NULL)", parts.join(", ")))
            }
        }

        // ── Arithmetic expressions ────────────────────────────────────────────
        Expression::Add(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_arith("+", &la, &ra))
        }
        Expression::Subtract(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_arith("-", &la, &ra))
        }
        Expression::Multiply(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_arith("*", &la, &ra))
        }
        Expression::Divide(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_divide(&la, &ra))
        }
        Expression::UnaryPlus(inner) => translate_expr_value(inner, bindings, ctx),
        Expression::UnaryMinus(inner) => {
            let sql = translate_expr_value(inner, bindings, ctx)?;
            Some(format!("(-({sql}))"))
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

        // ── String filter functions ───────────────────────────────────────────
        // Variables hold dictionary IDs; decode to text via a correlated subquery.
        // Literals use their raw lexical value as a SQL string.
        Expression::FunctionCall(Function::Contains, args) if args.len() >= 2 => {
            translate_function_call_filter(&Function::Contains, args, bindings, ctx)
        }

        Expression::FunctionCall(Function::StrStarts, args) if args.len() >= 2 => {
            translate_function_call_filter(&Function::StrStarts, args, bindings, ctx)
        }

        Expression::FunctionCall(Function::StrEnds, args) if args.len() >= 2 => {
            translate_function_call_filter(&Function::StrEnds, args, bindings, ctx)
        }

        Expression::FunctionCall(Function::Regex, args) if args.len() >= 2 => {
            translate_function_call_filter(&Function::Regex, args, bindings, ctx)
        }

        // ── SPARQL 1.1 built-in functions (v0.21.0) ─────────────────────────
        // All function calls first try the FILTER boolean context dispatcher.
        // If it returns None (function not applicable in boolean context), it
        // falls through to the EXISTS / NOT EXISTS handler below.
        Expression::FunctionCall(func, args) => {
            translate_function_call_filter(func, args, bindings, ctx)
        }

        // ── EXISTS / NOT EXISTS ───────────────────────────────────────────────
        // NOT EXISTS is Expression::Not(Expression::Exists(...)), handled via
        // the existing Not arm which recursively calls translate_expr.
        Expression::Exists(pattern) => {
            let inner_frag = translate_pattern(pattern, ctx);

            // Correlate inner variables against outer bindings.
            let mut all_conditions = inner_frag.conditions.clone();
            for (var, inner_col) in &inner_frag.bindings {
                if let Some(outer_col) = bindings.get(var.as_str()) {
                    all_conditions.push(format!("{inner_col} = {outer_col}"));
                }
            }

            let where_clause = if all_conditions.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", all_conditions.join(" AND "))
            };

            let from_clause = inner_frag.build_from();
            Some(format!(
                "(EXISTS (SELECT 1 FROM {from_clause} {where_clause}))"
            ))
        }

        // Unsupported expressions: raise a structured error when sparql_strict
        // is on (default), or silently drop (warn only) when off.
        // Never silently drop: either raise or warn, but never corrupt data by
        // omitting a filter predicate without any indication.
        _ => {
            let strict = crate::SPARQL_STRICT.get();
            if strict {
                pgrx::error!(
                    "unsupported SPARQL expression type in FILTER; \
                     set pg_ripple.sparql_strict = off to warn-and-skip instead"
                );
            } else {
                pgrx::warning!(
                    "unsupported SPARQL expression in FILTER — predicate dropped \
                     (set pg_ripple.sparql_strict = on to raise an error instead)"
                );
                None
            }
        }
    }
}

/// Returns a SQL text expression for `expr`.
///
/// Variables hold dictionary IDs — decoded via a correlated subquery against
/// `_pg_ripple.dictionary`.  Literals use their raw lexical value as a SQL
/// string constant.  Returns `None` for expressions that cannot be decoded to
/// text (e.g. complex sub-expressions).
#[allow(dead_code)]
fn expr_as_text_sql(expr: &Expression, bindings: &HashMap<String, String>) -> Option<String> {
    match expr {
        Expression::Variable(v) => {
            let col = bindings.get(v.as_str())?;
            Some(format!(
                "(SELECT _dict.value FROM _pg_ripple.dictionary _dict WHERE _dict.id = {col})"
            ))
        }
        Expression::Literal(lit) => {
            let val = lit.value();
            let escaped = val.replace('\'', "''");
            Some(format!("'{escaped}'"))
        }
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
            // Try inline (dictionary lookup at translation time).
            if let Some(id) = ctx.encode_iri(nn.as_str()) {
                return Some(id.to_string());
            }
            // IRI not yet in dictionary; embed a runtime lookup so BIND/IF/COALESCE
            // can reference IRIs that are inserted in the same transaction.
            let iri = nn.as_str().replace('\'', "''");
            Some(format!(
                "(SELECT d.id FROM _pg_ripple.dictionary d WHERE d.value = '{iri}' AND d.kind = 0 LIMIT 1)"
            ))
        }
        Expression::Literal(lit) => {
            // use inline encoding (or dict if out of range / unsupported type)
            let id = ctx.encode_literal(lit);
            Some(id.to_string())
        }
        // ── IF / COALESCE (v0.21.0) ──────────────────────────────────────────
        Expression::If(cond, then_expr, else_expr) => {
            let then_sql = translate_expr_value(then_expr, bindings, ctx)?;
            let else_sql = translate_expr_value(else_expr, bindings, ctx)
                .unwrap_or_else(|| "NULL::bigint".to_owned());
            // EBV constants: inline_false = -9151314442816847872, inline_int_zero = -9187343239835811840

            // Boolean predicate functions (isNumeric, isBlank, isIri, etc.) return SQL
            // booleans from translate_expr, not encoded bigints. Use them directly.
            let is_bool_pred = matches!(
                cond.as_ref(),
                Expression::FunctionCall(
                    spargebra::algebra::Function::IsBlank
                        | spargebra::algebra::Function::IsIri
                        | spargebra::algebra::Function::IsLiteral
                        | spargebra::algebra::Function::IsNumeric,
                    _
                )
            );
            if is_bool_pred && let Some(cond_sql) = translate_expr(cond, bindings, ctx) {
                return Some(format!(
                    "CASE WHEN ({cond_sql}) THEN ({then_sql}) ELSE ({else_sql}) END"
                ));
            }

            // For comparison operators (Less, LessOrEqual, etc.) and other boolean ops,
            // translate_expr returns a SQL boolean. Use it directly if it succeeds.
            // For variables and arithmetic, fall through to EBV check.
            match cond.as_ref() {
                Expression::Variable(_)
                | Expression::Add(_, _)
                | Expression::Subtract(_, _)
                | Expression::Multiply(_, _)
                | Expression::Divide(_, _)
                | Expression::UnaryMinus(_)
                | Expression::UnaryPlus(_) => {
                    // These return encoded bigints — use EBV check.
                    translate_expr_value(cond, bindings, ctx).map(|cond_val| format!(
                            "CASE WHEN ({cond_val}) IS NULL THEN NULL::bigint \
                             WHEN ({cond_val}) IN (-9151314442816847872::bigint, -9187343239835811840::bigint) THEN ({else_sql}) \
                             ELSE ({then_sql}) END"
                        ))
                }
                _ => {
                    // Comparisons, logical ops, etc. return SQL booleans.
                    if let Some(cond_sql) = translate_expr(cond, bindings, ctx) {
                        Some(format!(
                            "CASE WHEN ({cond_sql}) THEN ({then_sql}) ELSE ({else_sql}) END"
                        ))
                    } else {
                        translate_expr_value(cond, bindings, ctx).map(|cond_val| format!(
                            "CASE WHEN ({cond_val}) IS NULL THEN NULL::bigint \
                             WHEN ({cond_val}) IN (-9151314442816847872::bigint, -9187343239835811840::bigint) THEN ({else_sql}) \
                             ELSE ({then_sql}) END"
                        ))
                    }
                }
            }
        }
        Expression::Coalesce(exprs) => {
            let parts: Vec<String> = exprs
                .iter()
                .filter_map(|e| translate_expr_value(e, bindings, ctx))
                .collect();
            if parts.is_empty() {
                Some("NULL::bigint".to_owned())
            } else {
                Some(format!("COALESCE({})", parts.join(", ")))
            }
        }
        // ── SPARQL 1.1 built-in functions (v0.21.0) ──────────────────────────
        Expression::FunctionCall(func, args) => {
            let mut is_numeric = false;
            let result =
                expr::translate_function_value(func, args, bindings, ctx, &mut is_numeric)?;
            // NOTE: is_numeric flag is only used in the Extend pattern handler.
            // Here we just return the SQL expression; the Extend handler will
            // also call is_numeric_function() directly.
            Some(result)
        }
        // ── Inline-integer arithmetic (v0.42.0) ──────────────────────────────
        // SPARQL arithmetic on xsd:integer values encoded as inline i64s.
        // The encoding is: id = INLINE_FLAG | ((value + INTEGER_OFFSET) & VALUE_MASK)
        //   INLINE_FLAG   = -9223372036854775808  (bit 63)
        //   INTEGER_OFFSET = 36028797018963968    (1 << 55)
        //   VALUE_MASK    = 72057594037927935     (bits 55-0)
        // Extraction: (id & VALUE_MASK) - INTEGER_OFFSET
        // Packing:    INLINE_FLAG | ((val + INTEGER_OFFSET) & VALUE_MASK)
        Expression::Add(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_arith("+", &la, &ra))
        }
        Expression::Subtract(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_arith("-", &la, &ra))
        }
        Expression::Multiply(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_arith("*", &la, &ra))
        }
        Expression::Divide(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_divide(&la, &ra))
        }
        Expression::UnaryPlus(inner) => translate_expr_value(inner, bindings, ctx),
        Expression::UnaryMinus(inner) => {
            let sql = translate_expr_value(inner, bindings, ctx)?;
            Some(inline_int_negate(&sql))
        }
        _ => None,
    }
}

// Like `translate_expr_value`, but always returns raw numeric SQL values for
// numeric literals — used when the comparison context is a raw aggregate
// output (COUNT, SUM, etc.) rather than a stored inline-encoded triple value.
// ─── Inline-integer arithmetic helpers ───────────────────────────────────────

/// Sanitize a SPARQL variable name for use as a SQL column alias.
///
/// SPARQL blank-node variables can contain colons (e.g. `_bn__:f6891676...`)
/// which are not valid in unquoted SQL identifiers. Replace every character
/// that is not alphanumeric or underscore with an underscore.
fn sanitize_sql_ident(v: &str) -> String {
    v.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Extract the integer value from an inline-encoded i64 SQL expression.
///
/// Inline encoding: id = INLINE_FLAG | ((value + INTEGER_OFFSET) & VALUE_MASK)
///   VALUE_MASK     = (1 << 56) - 1 = 72057594037927935
///   INTEGER_OFFSET = 1 << 55       = 36028797018963968
/// Extraction: (id & VALUE_MASK) - INTEGER_OFFSET
fn inline_int_extract(sql: &str) -> String {
    format!("(({sql} & 72057594037927935::bigint) - 36028797018963968::bigint)")
}

/// Re-pack an extracted SQL integer back into the inline-encoding format.
///
/// Packing: INLINE_FLAG | ((val + INTEGER_OFFSET) & VALUE_MASK)
///   INLINE_FLAG = INT64_MIN = -9223372036854775808 (bit 63 set, as signed i64)
///
/// Note: we cannot write (-9223372036854775808::bigint) directly in PostgreSQL
/// because the parser tries to cast the positive literal first, which overflows.
/// Instead, `~(9223372036854775807::bigint)` = ~INT64_MAX = INT64_MIN.
fn inline_int_pack(sql: &str) -> String {
    format!(
        "((~(9223372036854775807::bigint)) | \
         (({sql} + 36028797018963968::bigint) & 72057594037927935::bigint))"
    )
}

/// Generate SQL for binary arithmetic on two inline-encoded integer expressions.
///
/// Both `la` and `ra` are SQL expressions that evaluate to inline-encoded i64
/// values. The result is also an inline-encoded i64.
///
/// If either operand is a dictionary ID (non-negative, bit 63 = 0), the result
/// is NULL — propagating a SPARQL type error as an unbound value.
#[allow(dead_code)]
fn inline_int_arith(op: &str, la: &str, ra: &str) -> String {
    let extract_a = inline_int_extract(la);
    let extract_b = inline_int_extract(ra);
    // Guard: dict IDs (positive bigint) are not inline integers; return NULL (type error).
    format!(
        "CASE WHEN ({la}) >= 0 OR ({ra}) >= 0 THEN NULL::bigint \
         ELSE {packed} END",
        packed = inline_int_pack(&format!("({extract_a} {op} {extract_b})")),
    )
}

/// Generate SQL for division on two inline-encoded integer expressions.
///
/// Returns NULL when the denominator is zero (SPARQL div-by-zero error semantics)
/// or when either operand is not an inline-encoded integer (dict ID, type error).
#[allow(dead_code)]
fn inline_int_divide(la: &str, ra: &str) -> String {
    let extract_a = inline_int_extract(la);
    let extract_b = inline_int_extract(ra);
    // SPARQL 1.1: integer / integer = xsd:decimal (not integer).
    // Guard: dict IDs (positive, bit63=0) → NULL (type error).
    // Denominator zero → NULLIF → NULL propagated through encode_typed_literal (STRICT).
    // Format: strip trailing zeros, always keep at least one decimal digit (e.g. '2.0').
    let xsd_decimal = "http://www.w3.org/2001/XMLSchema#decimal";
    // Pre-compute division as a named intermediate to avoid triple repetition.
    let div = format!("({extract_a}::numeric / NULLIF({extract_b}, 0)::numeric)");
    format!(
        "CASE WHEN ({la}) >= 0 OR ({ra}) >= 0 THEN NULL::bigint \
         ELSE (SELECT pg_ripple.encode_typed_literal( \
                   CASE WHEN _dv LIKE '%.%' THEN _dv ELSE _dv || '.0' END, \
                   '{xsd_decimal}' \
               ) FROM (SELECT trim_scale({div})::text AS _dv) _divtmp) \
         END",
    )
}

/// Generate SQL for unary negation of an inline-encoded integer expression.
fn inline_int_negate(sql: &str) -> String {
    let extract = inline_int_extract(sql);
    format!(
        "CASE WHEN ({sql}) >= 0 THEN NULL::bigint \
         ELSE {packed} END",
        packed = inline_int_pack(&format!("(-({extract}))")),
    )
}

/// RDF-aware binary arithmetic for +, -, * on numeric values.
///
/// Handles all SPARQL numeric type combinations:
/// - integer OP integer → integer (inline)
/// - integer OP decimal / decimal OP anything → decimal (dict-encoded)
/// - anything OP double / double OP anything → double (dict-encoded)
/// - Non-numeric operands → NULL (type error).
fn rdf_numeric_arith(op: &str, la: &str, ra: &str) -> String {
    let extract_a = inline_int_extract(la);
    let extract_b = inline_int_extract(ra);

    // Decode helper: inline int → numeric; dict numeric → numeric (via SPI to
    // see freshly inserted rows from encode_typed_literal); else → NULL.
    let decode_a = format!(
        "CASE WHEN ({la}) IS NULL THEN NULL \
         WHEN ({la}) < 0 THEN ({extract_a})::numeric \
         ELSE pg_ripple.decode_numeric_spi(({la})) END"
    );
    let decode_b = format!(
        "CASE WHEN ({ra}) IS NULL THEN NULL \
         WHEN ({ra}) < 0 THEN ({extract_b})::numeric \
         ELSE pg_ripple.decode_numeric_spi(({ra})) END"
    );

    // Type code: 0=integer (inline), 1=decimal, 2=double.
    let tc_a = format!(
        "CASE WHEN ({la}) < 0 THEN 0 \
         ELSE COALESCE((SELECT CASE \
           WHEN d.datatype IN ('http://www.w3.org/2001/XMLSchema#double',\
                               'http://www.w3.org/2001/XMLSchema#float') THEN 2 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#integer' THEN 0 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#decimal' THEN 1 \
           ELSE -1 END FROM _pg_ripple.dictionary d WHERE d.id = ({la}) LIMIT 1), -1) END"
    );
    let tc_b = format!(
        "CASE WHEN ({ra}) < 0 THEN 0 \
         ELSE COALESCE((SELECT CASE \
           WHEN d.datatype IN ('http://www.w3.org/2001/XMLSchema#double',\
                               'http://www.w3.org/2001/XMLSchema#float') THEN 2 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#integer' THEN 0 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#decimal' THEN 1 \
           ELSE -1 END FROM _pg_ripple.dictionary d WHERE d.id = ({ra}) LIMIT 1), -1) END"
    );

    let xsd_int = "http://www.w3.org/2001/XMLSchema#integer";
    let xsd_dec = "http://www.w3.org/2001/XMLSchema#decimal";
    let xsd_dbl = "http://www.w3.org/2001/XMLSchema#double";

    // Fast path: both inline integers → stay inline.
    let fast_int = format!(
        "CASE WHEN ({la}) >= 0 OR ({ra}) >= 0 THEN NULL::bigint \
         ELSE {packed} END",
        packed = inline_int_pack(&format!("(({extract_a}) {op} ({extract_b}))")),
    );

    format!(
        "CASE WHEN ({la}) IS NULL OR ({ra}) IS NULL THEN NULL::bigint \
         WHEN ({la}) < 0 AND ({ra}) < 0 THEN ({fast_int}) \
         ELSE (SELECT pg_ripple.encode_typed_literal( \
                   CASE \
                     WHEN _tc < 0 THEN NULL \
                     WHEN _tc >= 2 THEN pg_ripple.xsd_double_fmt(_result::float8::text) \
                     WHEN _tc = 1 THEN CASE WHEN _result LIKE '%.%' THEN trim_scale(_result::numeric)::text ELSE _result || '.0' END \
                     ELSE _result \
                   END, \
                   CASE \
                     WHEN _tc < 0 THEN 'http://www.w3.org/2001/XMLSchema#error' \
                     WHEN _tc >= 2 THEN '{xsd_dbl}' \
                     WHEN _tc = 1 THEN '{xsd_dec}' \
                     ELSE '{xsd_int}' \
                   END \
               ) \
               FROM (SELECT \
                   GREATEST(({tc_a}), ({tc_b})) AS _tc, \
                   (({decode_a}) {op} ({decode_b}))::text AS _result \
               ) _arith \
               WHERE _tc >= 0 AND _result IS NOT NULL) \
         END"
    )
}

/// RDF-aware division for SPARQL integer/integer → decimal.
fn rdf_numeric_divide(la: &str, ra: &str) -> String {
    let extract_a = inline_int_extract(la);
    let extract_b = inline_int_extract(ra);

    let decode_a = format!(
        "CASE WHEN ({la}) IS NULL THEN NULL \
         WHEN ({la}) < 0 THEN ({extract_a})::numeric \
         ELSE pg_ripple.decode_numeric_spi(({la})) END"
    );
    let decode_b = format!(
        "CASE WHEN ({ra}) IS NULL THEN NULL \
         WHEN ({ra}) < 0 THEN ({extract_b})::numeric \
         ELSE pg_ripple.decode_numeric_spi(({ra})) END"
    );

    let tc_a = format!(
        "CASE WHEN ({la}) < 0 THEN 0 \
         ELSE COALESCE((SELECT CASE \
           WHEN d.datatype IN ('http://www.w3.org/2001/XMLSchema#double',\
                               'http://www.w3.org/2001/XMLSchema#float') THEN 2 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#integer' THEN 0 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#decimal' THEN 1 \
           ELSE -1 END FROM _pg_ripple.dictionary d WHERE d.id = ({la}) LIMIT 1), -1) END"
    );
    let tc_b = format!(
        "CASE WHEN ({ra}) < 0 THEN 0 \
         ELSE COALESCE((SELECT CASE \
           WHEN d.datatype IN ('http://www.w3.org/2001/XMLSchema#double',\
                               'http://www.w3.org/2001/XMLSchema#float') THEN 2 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#integer' THEN 0 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#decimal' THEN 1 \
           ELSE -1 END FROM _pg_ripple.dictionary d WHERE d.id = ({ra}) LIMIT 1), -1) END"
    );

    let xsd_dec = "http://www.w3.org/2001/XMLSchema#decimal";
    let xsd_dbl = "http://www.w3.org/2001/XMLSchema#double";

    format!(
        "CASE WHEN ({la}) IS NULL OR ({ra}) IS NULL THEN NULL::bigint \
         ELSE (SELECT pg_ripple.encode_typed_literal( \
                   CASE \
                     WHEN _tc < 0 OR _denominator IS NULL OR _denominator = 0 THEN NULL \
                     WHEN _tc >= 2 THEN pg_ripple.xsd_double_fmt((_numerator / _denominator)::float8::text) \
                     ELSE CASE WHEN _result LIKE '%.%' THEN trim_scale(_result::numeric)::text \
                               ELSE _result || '.0' END \
                   END, \
                   CASE \
                     WHEN _tc >= 2 THEN '{xsd_dbl}' \
                     ELSE '{xsd_dec}' \
                   END \
               ) \
               FROM (SELECT \
                   GREATEST(({tc_a}), ({tc_b}), 1) AS _tc, \
                   ({decode_a}) AS _numerator, \
                   ({decode_b}) AS _denominator, \
                   CASE WHEN ({decode_b}) != 0 \
                        THEN trim_scale(({decode_a}) / NULLIF({decode_b}, 0))::text \
                        ELSE NULL END AS _result \
               ) _div \
               WHERE _tc >= 0) \
         END"
    )
}

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
        // Numeric function calls (ABS, CEIL, FLOOR, ROUND, STRLEN, YEAR, etc.)
        // produce raw SQL numeric values — return the SQL expression directly.
        Expression::FunctionCall(func, args) => {
            let mut is_numeric = false;
            let sql = expr::translate_function_value(func, args, bindings, ctx, &mut is_numeric)?;
            if is_numeric { Some(sql) } else { None }
        }
        _ => None,
    }
}

/// Determine whether an expression is a raw-numeric variable (aggregate output).
fn expr_is_raw_numeric(expr: &Expression, ctx: &Ctx) -> bool {
    match expr {
        Expression::Variable(v) => {
            ctx.raw_numeric_vars.contains(v.as_str()) || ctx.raw_double_vars.contains(v.as_str())
        }
        // Function calls that produce raw SQL numeric output (ABS, CEIL, FLOOR, etc.)
        // are never inline-encoded, so comparisons must use raw numeric values.
        Expression::FunctionCall(func, _) => expr::is_numeric_function(func),
        _ => false,
    }
}

/// Determine whether an expression is a raw-text variable (GROUP_CONCAT output).
fn expr_is_raw_text(expr: &Expression, ctx: &Ctx) -> bool {
    if let Expression::Variable(v) = expr {
        ctx.raw_text_vars.contains(v.as_str())
    } else {
        false
    }
}

/// Return the lexical string form of a literal for text comparisons.
fn literal_lexical_value(lit: &Literal) -> String {
    // Return as a SQL quoted string literal using the lexical value.
    let val = lit.value().replace('\'', "''");
    format!("'{val}'")
}

/// Translate both sides of a comparison, using raw encoding for numeric
/// literals when either side is a raw-numeric aggregate variable.
fn translate_comparison_sides(
    a: &Expression,
    b: &Expression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<(String, String)> {
    // Case 1: one side is a raw-text variable (GROUP_CONCAT result).
    // Compare the other side using its lexical string value.
    if expr_is_raw_text(a, ctx) {
        let la = translate_expr_value(a, bindings, ctx)?;
        let ra = match b {
            Expression::Literal(lit) => literal_lexical_value(lit),
            _ => return None, // unsupported: text var vs non-literal
        };
        return Some((la, ra));
    }
    if expr_is_raw_text(b, ctx) {
        let la = match a {
            Expression::Literal(lit) => literal_lexical_value(lit),
            _ => return None,
        };
        let ra = translate_expr_value(b, bindings, ctx)?;
        return Some((la, ra));
    }
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
                    // SPARQL 1.1 §15.1: unbound variables sort last in ASC order.
                    bindings
                        .get(v.as_str())
                        .map(|col| format!("{col} ASC NULLS LAST"))
                } else {
                    None
                }
            }
            OrderExpression::Desc(expr) => {
                if let Expression::Variable(v) = expr {
                    // SPARQL 1.1 §15.1: unbound variables sort first in DESC order.
                    bindings
                        .get(v.as_str())
                        .map(|col| format!("{col} DESC NULLS FIRST"))
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

    Translation {
        sql,
        variables,
        raw_numeric_vars: ctx.raw_numeric_vars,
        raw_text_vars: ctx.raw_text_vars,
        raw_iri_vars: ctx.raw_iri_vars,
        raw_double_vars: ctx.raw_double_vars,
        topn_applied,
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
