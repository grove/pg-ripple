//! SQL compiler for Datalog rules.
//!
//! Each Datalog rule is compiled to one or more SQL statements:
//!
//! - **Non-recursive rules** → `INSERT … SELECT … ON CONFLICT DO NOTHING`
//! - **Recursive rules**     → `WITH RECURSIVE … CYCLE … INSERT … SELECT`
//! - **Negation** → `NOT EXISTS (…)` in the WHERE clause (small tables) or
//!   `LEFT JOIN … IS NULL` anti-join form (large tables, v0.29.0)
//! - **Constraint rules**    → `SELECT EXISTS (…) AS violated`
//! - **On-demand CTEs**      → `WITH RECURSIVE cte AS (…)` prepended to SPARQL SQL
//!
//! # Integer joins everywhere
//!
//! All IRI and literal constants in rules are dictionary-encoded (`i64`)
//! at parse time.  The SQL generator never emits string comparisons.
//!
//! # v0.29.0 optimizations
//!
//! - **Cost-based body atom reordering**: positive body atoms are sorted by ascending
//!   estimated VP-table cardinality (`_pg_ripple.predicates.triple_count`) before SQL
//!   generation.  Controlled by `pg_ripple.datalog_cost_reorder` (default: true).
//! - **Anti-join negation**: negated body atoms with VP-table row count ≥
//!   `pg_ripple.datalog_antijoin_threshold` compile to `LEFT JOIN … IS NULL` for
//!   better index utilisation (default threshold: 1000).
//! - **Predicate-filter pushdown**: arithmetic/comparison guards are moved into the
//!   `JOIN … ON` clause of the atom that first binds all the guard's variables,
//!   enabling the PostgreSQL planner to apply index scans early.

// v0.90.0 CQ-02 / M15-13 v0.96.0: split sub-modules
// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[allow(dead_code)]
pub mod builtins;
// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[allow(dead_code)]
pub mod prob;
pub mod shacl_rules;
pub mod sql;

pub use shacl_rules::compile_constraint_check;
pub use sql::{compile_aggregate_rule, compile_on_demand_cte, compile_rule_delta_variants_to};

use crate::datalog::{ArithOp, Atom, BodyLiteral, Rule, StringBuiltin, Term};
use pgrx::datum::DatumWithOid;

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Format a SQL-safe integer literal from a Term::Const.
pub(super) fn const_sql(id: i64) -> String {
    id.to_string()
}

/// Render a term in SQL: `?var` → alias column reference; `Const(n)` → integer literal.
/// `alias` is the table alias for this atom's join position (e.g. "t0", "t1").
#[allow(dead_code)] // used by compile_rule_delta_variants and compile_single_rule_to
pub(super) fn render_term_col(term: &Term, alias: &str, col: &str) -> String {
    match term {
        Term::Var(v) => format!("{alias}_{v}"), // resolved later as bound column
        Term::Const(id) => const_sql(*id),
        Term::Wildcard => format!("{alias}.{col}"),
        Term::DefaultGraph => "0".to_owned(),
    }
}

/// Check whether a term is a variable.
#[allow(dead_code)] // used in conditional logic and future optimizations
pub(super) fn is_var(term: &Term) -> bool {
    matches!(term, Term::Var(_))
}

/// Derive the VP table name for a predicate constant.
/// Used for INSERT targets (assumes dedicated HTAP tables exist).
pub(super) fn vp_table(pred_id: i64) -> String {
    // Use _pg_ripple.vp_{id} (the view that unions main and delta).
    format!("_pg_ripple.vp_{pred_id}")
}

/// Return a SQL table expression for READING triples of `pred_id`.
///
/// For predicates with dedicated HTAP tables (`predicates.table_oid IS NOT NULL`),
/// returns a UNION ALL of the dedicated view and any remaining `vp_rare` entries
/// (rare triples are not moved to the dedicated tables until a full merge/promotion).
/// For rare predicates (no dedicated table), returns a filtered `vp_rare` subquery.
///
/// This function uses SPI and must be called from within a PostgreSQL backend context.
pub fn vp_read_expr_pub(pred_id: i64) -> String {
    vp_read_expr(pred_id)
}

pub(super) fn vp_read_expr(pred_id: i64) -> String {
    let has_dedicated = pgrx::Spi::get_one_with_args::<i64>(
        "SELECT table_oid::bigint FROM _pg_ripple.predicates \
         WHERE id = $1 AND table_oid IS NOT NULL",
        &[DatumWithOid::from(pred_id)],
    )
    .ok()
    .flatten()
    .is_some();

    if has_dedicated {
        // Union dedicated view with any un-promoted vp_rare rows so that both
        // existing data (still in vp_rare) and newly derived data (in the delta
        // table) are visible to the rule body.
        format!(
            "(SELECT s, o, g FROM _pg_ripple.vp_{pred_id} \
              UNION ALL \
              SELECT s, o, g FROM _pg_ripple.vp_rare WHERE p = {pred_id})"
        )
    } else {
        // Pure rare predicate: all data lives in vp_rare.
        format!("(SELECT s, o, g FROM _pg_ripple.vp_rare WHERE p = {pred_id})")
    }
}

// ─── v0.29.0: Cost-based reordering & anti-join helpers ──────────────────────

/// Return `true` if the rule has any variable predicate (in head or body).
/// Used to detect rules that need runtime predicate variable instantiation.
pub fn has_variable_pred(rule: &Rule) -> bool {
    if rule
        .head
        .as_ref()
        .is_some_and(|h| matches!(&h.p, Term::Var(_)))
    {
        return true;
    }
    for lit in &rule.body {
        let atom = match lit {
            BodyLiteral::Positive(a) | BodyLiteral::Negated(a) => a,
            _ => continue,
        };
        if matches!(&atom.p, Term::Var(_)) {
            return true;
        }
    }
    false
}

/// Estimate the cardinality of a predicate's VP table.
///
/// Returns the `triple_count` from `_pg_ripple.predicates`, or 0 when the
/// predicate is not in the catalog (e.g., a newly derived predicate).
/// Used to sort body atoms by ascending estimated cardinality (v0.29.0).
pub(super) fn estimate_pred_cardinality(pred_id: i64) -> i64 {
    pgrx::Spi::get_one_with_args::<i64>(
        "SELECT triple_count FROM _pg_ripple.predicates WHERE id = $1",
        &[DatumWithOid::from(pred_id)],
    )
    .ok()
    .flatten()
    .unwrap_or(0)
}

/// Sort positive body atoms from a rule by ascending estimated VP-table cardinality.
///
/// Atoms with bound constants (s or o is `Term::Const`) get a synthetic lower
/// cardinality because a constant filter dramatically reduces the result set.
/// Returns a `Vec` of references in cost-ascending order.
///
/// Called only when `pg_ripple.datalog_cost_reorder = true`.
pub(super) fn cost_order_atoms(rule: &Rule) -> Vec<&Atom> {
    let mut atoms: Vec<&Atom> = rule
        .body
        .iter()
        .filter_map(|lit| {
            if let BodyLiteral::Positive(a) = lit {
                Some(a)
            } else {
                None
            }
        })
        .collect();

    atoms.sort_by_key(|atom| {
        let base = match &atom.p {
            Term::Const(id) => estimate_pred_cardinality(*id),
            _ => i64::MAX, // variable predicate: put last
        };
        // Atoms with one or two bound positions are highly selective.
        let has_bound_s = !matches!(&atom.s, Term::Var(_) | Term::Wildcard);
        let has_bound_o = !matches!(&atom.o, Term::Var(_) | Term::Wildcard);
        // DL-COST-GUC-01 (v0.83.0): read divisors from GUCs instead of hardcoded values.
        let divisor: i64 = match (has_bound_s, has_bound_o) {
            (true, true) => crate::DATALOG_COST_BOUND_SO_DIVISOR.get() as i64,
            (true, false) | (false, true) => crate::DATALOG_COST_BOUND_S_DIVISOR.get() as i64,
            (false, false) => 1,
        };
        base / divisor.max(1)
    });

    atoms
}

/// Compute the SQL condition string for an anti-join `LEFT JOIN … ON` clause.
///
/// Equivalent semantics to `build_not_exists_conds` but generates conditions
/// suitable for a `JOIN … ON` expression (with table alias prefixes).
pub(super) fn build_antijoin_on_cond(aj_alias: &str, atom: &Atom, var_map: &VarMap) -> String {
    let mut conds = Vec::new();
    if let Term::Var(v) = &atom.s {
        if let Some(ref_col) = var_map.col_ref(v) {
            conds.push(format!("{aj_alias}.s = {ref_col}"));
        }
    } else if let Term::Const(c) = &atom.s {
        conds.push(format!("{aj_alias}.s = {}", const_sql(*c)));
    }
    if let Term::Var(v) = &atom.o {
        if let Some(ref_col) = var_map.col_ref(v) {
            conds.push(format!("{aj_alias}.o = {ref_col}"));
        }
    } else if let Term::Const(c) = &atom.o {
        conds.push(format!("{aj_alias}.o = {}", const_sql(*c)));
    }
    if conds.is_empty() {
        "TRUE".to_owned()
    } else {
        conds.join(" AND ")
    }
}

/// Return true if all variables referenced by a guard literal are present in var_map.
///
/// Used to decide whether a Compare or Assign guard can be pushed down into
/// the most recent JOIN's ON clause (v0.29.0 filter pushdown).
pub(super) fn guard_fully_bound(guard: &BodyLiteral, var_map: &VarMap) -> bool {
    let check_term = |t: &Term| -> bool {
        match t {
            Term::Var(v) => var_map.col_ref(v).is_some(),
            _ => true,
        }
    };
    match guard {
        BodyLiteral::Compare(lhs, _, rhs) => check_term(lhs) && check_term(rhs),
        BodyLiteral::Assign(_var, lhs, _, rhs) => {
            // The var being assigned is allowed to be unbound here (it's being introduced).
            check_term(lhs) && check_term(rhs)
        }
        BodyLiteral::StringBuiltin(sb) => match sb {
            StringBuiltin::Strlen(t, _, r) => check_term(t) && check_term(r),
            StringBuiltin::Regex(t, _) => check_term(t),
            // v0.109.0 NS-RL:
            StringBuiltin::TrigramSimilarity(a, b, _, r) => {
                check_term(a) && check_term(b) && check_term(r)
            }
            StringBuiltin::Levenshtein(a, b, _, r) => {
                check_term(a) && check_term(b) && check_term(r)
            }
            StringBuiltin::Soundex(s, _, r) => check_term(s) && check_term(r),
            StringBuiltin::Metaphone(s, _, _, r) => check_term(s) && check_term(r),
            StringBuiltin::JaroWinkler(a, b, _, r) => {
                check_term(a) && check_term(b) && check_term(r)
            }
            // v0.111.0 PPRL Bloom-filter
            StringBuiltin::DiceSimilarity(a, b, _, r) => {
                check_term(a) && check_term(b) && check_term(r)
            }
        },
        _ => false,
    }
}

/// Compile a guard literal (Compare, Assign, StringBuiltin) to a SQL string
/// suitable for inclusion in a WHERE or JOIN ON clause.
pub(super) fn compile_guard_sql(guard: &BodyLiteral, var_map: &VarMap) -> Option<String> {
    match guard {
        BodyLiteral::Compare(lhs, op, rhs) => {
            let l = render_comparison_term(lhs, var_map);
            let r = render_comparison_term(rhs, var_map);
            Some(format!("{l} {} {r}", compare_op_sql(op)))
        }
        BodyLiteral::Assign(var, lhs, op, rhs) => {
            let l = render_comparison_term(lhs, var_map);
            let r_raw = render_comparison_term(rhs, var_map);
            let r = if matches!(op, ArithOp::Div) {
                format!("NULLIF({r_raw}, 0)")
            } else {
                r_raw
            };
            let col_expr = format!("({l} {} {r})", arith_op_sql(op));
            // Bind the computed variable so downstream code can reference it.
            // We return None here because Assign is handled separately (needs var_map mutation).
            let _ = (var, col_expr); // suppress unused warning
            None
        }
        BodyLiteral::StringBuiltin(sb) => match sb {
            StringBuiltin::Strlen(term, op, rhs_term) => {
                let col = render_comparison_term(term, var_map);
                let r = render_comparison_term(rhs_term, var_map);
                Some(format!("LENGTH({col}::text) {} {r}", compare_op_sql(op)))
            }
            StringBuiltin::Regex(term, pattern) => {
                let col = render_comparison_term(term, var_map);
                let escaped = pattern.replace('\'', "''");
                Some(format!("{col}::text ~ '{escaped}'"))
            }
            // v0.109.0 NS-RL string similarity built-ins
            StringBuiltin::TrigramSimilarity(a_term, b_term, op, rhs_term) => {
                let a_col = render_comparison_term(a_term, var_map);
                let b_col = render_comparison_term(b_term, var_map);
                let r = render_comparison_term(rhs_term, var_map);
                Some(format!(
                    "pg_ripple._fuzzy_match_guard({a_col}::text, {b_col}::text) {} {r}",
                    compare_op_sql(op)
                ))
            }
            StringBuiltin::Levenshtein(a_term, b_term, op, rhs_term) => {
                let a_col = render_comparison_term(a_term, var_map);
                let b_col = render_comparison_term(b_term, var_map);
                let r = render_comparison_term(rhs_term, var_map);
                Some(format!(
                    "levenshtein({a_col}::text, {b_col}::text) {} {r}",
                    compare_op_sql(op)
                ))
            }
            StringBuiltin::Soundex(s_term, op, rhs_term) => {
                let s_col = render_comparison_term(s_term, var_map);
                let r = render_comparison_term(rhs_term, var_map);
                Some(format!(
                    "soundex({s_col}::text) {} {r}::text",
                    compare_op_sql(op)
                ))
            }
            StringBuiltin::Metaphone(s_term, maxlen, op, rhs_term) => {
                let s_col = render_comparison_term(s_term, var_map);
                let r = render_comparison_term(rhs_term, var_map);
                Some(format!(
                    "metaphone({s_col}::text, {maxlen}) {} {r}::text",
                    compare_op_sql(op)
                ))
            }
            StringBuiltin::JaroWinkler(a_term, b_term, op, rhs_term) => {
                let a_col = render_comparison_term(a_term, var_map);
                let b_col = render_comparison_term(b_term, var_map);
                let r = render_comparison_term(rhs_term, var_map);
                Some(format!(
                    "jarowinkler({a_col}::text, {b_col}::text) {} {r}",
                    compare_op_sql(op)
                ))
            }
            // v0.111.0 PPRL Bloom-filter
            StringBuiltin::DiceSimilarity(a_term, b_term, op, rhs_term) => {
                let a_col = render_comparison_term(a_term, var_map);
                let b_col = render_comparison_term(b_term, var_map);
                let r = render_comparison_term(rhs_term, var_map);
                Some(format!(
                    "pg_ripple.dice_similarity({a_col}::text, {b_col}::text) {} {r}",
                    compare_op_sql(op)
                ))
            }
        },
        _ => None,
    }
}

// ─── Variable map ─────────────────────────────────────────────────────────────

/// Variable→(alias, column) mapping built while iterating body atoms.
#[derive(Default)]
pub(crate) struct VarMap {
    bindings: Vec<(String, String, String)>, // (var_name, alias, col)
}

impl VarMap {
    fn bind(&mut self, var: &str, alias: &str, col: &str) {
        // Only record first binding; subsequent are join conditions.
        if !self.bindings.iter().any(|(v, _, _)| v == var) {
            self.bindings
                .push((var.to_owned(), alias.to_owned(), col.to_owned()));
        }
    }

    /// Return `alias.col` for a variable, or just `alias` if `col` is empty
    /// (used for computed expressions bound via Assign).
    fn col_ref(&self, var: &str) -> Option<String> {
        self.bindings
            .iter()
            .find(|(v, _, _)| v == var)
            .map(|(_, a, c)| {
                if c.is_empty() {
                    a.clone()
                } else {
                    format!("{a}.{c}")
                }
            })
    }
}

// ─── v0.107.0 sequential operator helpers ─────────────────────────────────────

/// Resolve a predicate IRI or `<IRI>` string to its dictionary integer id.
/// Returns 0 when the predicate is not in the dictionary (new predicate that has
/// no triples yet — safe to return 0 because the EXISTS subquery will just yield
/// false for an unknown predicate).
fn resolve_predicate_id(pred: &str) -> i64 {
    let iri = if pred.starts_with('<') && pred.ends_with('>') {
        &pred[1..pred.len() - 1]
    } else if pred.starts_with("ex:") || pred.contains(':') {
        // attempt dictionary lookup for prefixed form
        pred
    } else {
        pred
    };
    crate::dictionary::lookup_iri(iri).unwrap_or(0)
}

/// Resolve a SEQUENCE argument token to a SQL expression usable in a WHERE clause.
///
/// - `?varName` → the column reference from `var_map` if bound, else `TRUE`
/// - `<IRI>` or `prefix:local` → dictionary-encoded integer constant
/// - `'literal'` → 0 (best-effort; literals not in scope for sequence subjects)
fn resolve_term_ref(tok: &str, var_map: &VarMap) -> String {
    let tok = tok.trim();
    if let Some(var) = tok.strip_prefix('?') {
        if let Some(col) = var_map.col_ref(var) {
            return format!("TRUE AND e1.s = {col}"); // placeholder: binding to e1.s
        }
        // Variable not yet bound — skip the condition.
        return "TRUE".to_owned();
    }
    // IRI or prefixed name — encode to dictionary id.
    let id = resolve_predicate_id(tok);
    format!("TRUE AND e1.s = {id}")
}

// ─── Main compiler ────────────────────────────────────────────────────────────

/// Compile a slice of rules to SQL INSERT statements.
///
/// Rules in the slice are assumed to be from the same stratum.
/// Recursive rules within the slice share a `WITH RECURSIVE` CTE.
pub fn compile_rule_set(rules: &[Rule]) -> Result<Vec<String>, String> {
    let mut sqls = Vec::new();
    for rule in rules {
        if rule.head.is_none() {
            // Constraint rule — not materialized here; use compile_constraint_check.
            continue;
        }
        let sql = compile_single_rule(rule)?;
        sqls.push(sql);
    }
    Ok(sqls)
}

/// Compile a single rule inserting into `target` (with columns `(s, o, g)`).
/// Used by semi-naive inference to target temp tables instead of HTAP delta tables.
pub fn compile_single_rule_to(rule: &Rule, target: &str) -> Result<String, String> {
    let head = rule
        .head
        .as_ref()
        .ok_or_else(|| "cannot compile constraint rule as INSERT".to_owned())?;

    let head_pred = match &head.p {
        Term::Const(id) => *id,
        Term::Var(_) => return Err("variable predicate in rule head is not supported".to_owned()),
        _ => return Err("invalid predicate term in rule head".to_owned()),
    };

    let head_g_expr = match &head.g {
        Term::Const(id) => const_sql(*id),
        Term::Var(v) => format!("g_var_{v}"),
        Term::DefaultGraph => "0".to_owned(),
        Term::Wildcard => "0".to_owned(),
    };

    let is_recursive = is_recursive_rule(rule, head_pred);
    if is_recursive {
        sql::compile_recursive_rule(rule, head_pred, &head_g_expr, target)
    } else {
        compile_nonrecursive_rule(rule, head_pred, &head_g_expr, target)
    }
}

/// Compile a single derivation rule to a SQL INSERT statement.
pub fn compile_single_rule(rule: &Rule) -> Result<String, String> {
    let head = rule
        .head
        .as_ref()
        .ok_or_else(|| "cannot compile constraint rule as INSERT".to_owned())?;

    let head_pred = match &head.p {
        Term::Const(id) => *id,
        Term::Var(_) => return Err("variable predicate in rule head is not supported".to_owned()),
        _ => return Err("invalid predicate term in rule head".to_owned()),
    };

    // Determine head graph column: constant or variable.
    let head_g_expr = match &head.g {
        Term::Const(id) => const_sql(*id),
        Term::Var(v) => format!("g_var_{v}"),
        Term::DefaultGraph => "0".to_owned(),
        Term::Wildcard => "0".to_owned(),
    };

    // Determine if rule is recursive (head pred appears in body).
    let is_recursive = is_recursive_rule(rule, head_pred);

    // Target table: use delta for HTAP tables.
    // For new predicates without a dedicated VP table, create the HTAP split
    // on-demand so the INSERT does not fail with "relation does not exist".
    // (v0.29.0 bug fix: pre-existing infer() path used compile_single_rule
    // which always targeted the delta table, even for new predicates.)
    let has_dedicated = pgrx::Spi::get_one_with_args::<i64>(
        "SELECT table_oid::bigint FROM _pg_ripple.predicates \
         WHERE id = $1 AND table_oid IS NOT NULL",
        &[DatumWithOid::from(head_pred)],
    )
    .ok()
    .flatten()
    .is_some();
    if !has_dedicated {
        crate::storage::merge::ensure_htap_tables(head_pred);
    }
    let target = format!("{}_delta", vp_table(head_pred));

    if is_recursive {
        sql::compile_recursive_rule(rule, head_pred, &head_g_expr, &target)
    } else {
        compile_nonrecursive_rule(rule, head_pred, &head_g_expr, &target)
    }
}

pub(super) fn is_recursive_rule(rule: &Rule, head_pred: i64) -> bool {
    for lit in &rule.body {
        if let BodyLiteral::Positive(atom) = lit
            && let Term::Const(p) = &atom.p
            && *p == head_pred
        {
            return true;
        }
    }
    false
}

/// Compile a non-recursive rule to `INSERT … SELECT … ON CONFLICT DO NOTHING`.
///
/// # v0.29.0 optimizations applied here
///
/// 1. **Cost-based body atom reordering**: positive atoms sorted by estimated
///    VP-table cardinality when `pg_ripple.datalog_cost_reorder = true`.
/// 2. **Anti-join negation**: negated atoms compile to `LEFT JOIN … IS NULL`
///    when the table has ≥ `pg_ripple.datalog_antijoin_threshold` rows.
/// 3. **Predicate-filter pushdown**: Compare/StringBuiltin guards that have all
///    their variables bound after the most recent positive atom are inlined into
///    that atom's `JOIN … ON` clause instead of the outer `WHERE` clause.
fn compile_nonrecursive_rule(
    rule: &Rule,
    _head_pred: i64,
    _head_g_expr: &str,
    target: &str,
) -> Result<String, String> {
    let head = rule
        .head
        .as_ref()
        .ok_or_else(|| "compile_nonrecursive_rule: rule has no head".to_string())?;

    // ── Step 1: Sort positive body atoms by cost (v0.29.0) ────────────────────
    let sorted_positive: Vec<&Atom> = if crate::DATALOG_COST_REORDER.get() {
        cost_order_atoms(rule)
    } else {
        rule.body
            .iter()
            .filter_map(|lit| {
                if let BodyLiteral::Positive(a) = lit {
                    Some(a)
                } else {
                    None
                }
            })
            .collect()
    };

    // ── Step 1b: Collect temporal filters (v0.106.0 + v0.107.0) ─────────────
    // Build a combined SQL WHERE fragment from all temporal filter literals.
    // This fragment is appended to the table expression for any atom whose
    // predicate is registered as temporal.
    //
    // v0.107.0 sequential operators (WITHIN, SEQUENCE, CONSECUTIVE) compile to
    // standalone WHERE EXISTS / NOT EXISTS subqueries that are appended to
    // `where_clauses` after the FROM clause is assembled.
    let temporal_filter_sql: Option<String> = {
        use crate::datalog::TemporalFilter;
        let mut parts: Vec<String> = Vec::new();
        for lit in &rule.body {
            if let BodyLiteral::TemporalFilter(tf) = lit {
                let s = match tf {
                    TemporalFilter::After(ts) => {
                        format!("valid_from > {ts}::timestamptz")
                    }
                    TemporalFilter::Before(ts) => {
                        format!("valid_from < {ts}::timestamptz")
                    }
                    TemporalFilter::During(from_ts, to_ts) => {
                        format!(
                            "tstzrange(valid_from, valid_to, '[)') && \
                             tstzrange({from_ts}::timestamptz, {to_ts}::timestamptz, '[)')"
                        )
                    }
                    // v0.107.0 operators are collected as whole-query WHERE subqueries;
                    // skip them here (handled after step 3 below).
                    TemporalFilter::Within(..)
                    | TemporalFilter::Sequence(..)
                    | TemporalFilter::Consecutive(..) => continue,
                    // v0.118.0 Allen's interval relations — compile to timestamptz comparisons.
                    // These do not filter the temporal_facts table directly; they compare
                    // timestamp arguments provided as rule body constants or variables.
                    TemporalFilter::AllenBefore(a_start, a_end, b_start, _b_end) => {
                        format!(
                            "({a_start}::timestamptz < {b_start}::timestamptz \
                             AND {a_end}::timestamptz <= {b_start}::timestamptz)"
                        )
                    }
                    TemporalFilter::AllenMeets(_a_start, a_end, b_start, _b_end) => {
                        format!("{a_end}::timestamptz = {b_start}::timestamptz")
                    }
                    TemporalFilter::AllenOverlaps(a_start, a_end, b_start, b_end) => {
                        format!(
                            "({a_start}::timestamptz < {b_start}::timestamptz \
                             AND {a_end}::timestamptz > {b_start}::timestamptz \
                             AND {a_end}::timestamptz < {b_end}::timestamptz)"
                        )
                    }
                    TemporalFilter::AllenDuring(a_start, a_end, b_start, b_end) => {
                        format!(
                            "({a_start}::timestamptz > {b_start}::timestamptz \
                             AND {a_end}::timestamptz < {b_end}::timestamptz)"
                        )
                    }
                    TemporalFilter::AllenFinishes(a_start, a_end, b_start, b_end) => {
                        format!(
                            "({a_start}::timestamptz > {b_start}::timestamptz \
                             AND {a_end}::timestamptz = {b_end}::timestamptz)"
                        )
                    }
                    TemporalFilter::AllenStarts(a_start, a_end, b_start, b_end) => {
                        format!(
                            "({a_start}::timestamptz = {b_start}::timestamptz \
                             AND {a_end}::timestamptz < {b_end}::timestamptz)"
                        )
                    }
                    TemporalFilter::AllenEquals(a_start, a_end, b_start, b_end) => {
                        format!(
                            "({a_start}::timestamptz = {b_start}::timestamptz \
                             AND {a_end}::timestamptz = {b_end}::timestamptz)"
                        )
                    }
                };
                parts.push(s);
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" AND "))
        }
    };

    // ── Step 2: Collect guards for filter-pushdown (v0.29.0) ──────────────────
    // Guards that are not yet pushed will remain as WHERE conditions.
    let all_guards: Vec<&BodyLiteral> = rule
        .body
        .iter()
        .filter(|lit| {
            matches!(
                lit,
                BodyLiteral::Compare(..) | BodyLiteral::StringBuiltin(_)
            )
        })
        .collect();
    let mut pushed_guards: std::collections::HashSet<usize> = std::collections::HashSet::new();

    let mut from_clauses: Vec<String> = Vec::new();
    let mut where_clauses: Vec<String> = Vec::new();
    let mut var_map = VarMap::default();
    let mut antijoin_idx = 0usize;

    // ── Step 3: Process positive atoms with pushdown ───────────────────────────
    for (atom_idx, atom) in sorted_positive.iter().enumerate() {
        let alias = format!("t{atom_idx}");
        let pred_id = match &atom.p {
            Term::Const(id) => *id,
            Term::Var(_) => {
                return Err("variable predicate in rule body not supported".to_owned());
            }
            _ => return Err("invalid predicate term in rule body".to_owned()),
        };

        // Bind variables.
        if let Term::Var(v) = &atom.s {
            var_map.bind(v, &alias, "s");
        } else if let Term::Const(c) = &atom.s {
            where_clauses.push(format!("{alias}.s = {}", const_sql(*c)));
        }
        if let Term::Var(v) = &atom.o {
            var_map.bind(v, &alias, "o");
        } else if let Term::Const(c) = &atom.o {
            where_clauses.push(format!("{alias}.o = {}", const_sql(*c)));
        }
        if let Term::Var(v) = &atom.g {
            var_map.bind(v, &alias, "g");
        } else if let Term::Const(c) = &atom.g {
            where_clauses.push(format!("{alias}.g = {}", const_sql(*c)));
        } else {
            // DefaultGraph: g = 0 only when rule_graph_scope = 'default' (not the default)
            let scope = crate::RULE_GRAPH_SCOPE
                .get()
                .as_ref()
                .and_then(|c| c.to_str().ok())
                .unwrap_or("all")
                .to_owned();
            if scope == "default" {
                where_clauses.push(format!("{alias}.g = 0"));
            }
        }

        // After binding, check which guards can be pushed into this JOIN ON.
        let mut pushdown_conds: Vec<String> = Vec::new();
        for (gi, guard) in all_guards.iter().enumerate() {
            if !pushed_guards.contains(&gi)
                && guard_fully_bound(guard, &var_map)
                && let Some(cond_sql) = compile_guard_sql(guard, &var_map)
            {
                pushdown_conds.push(cond_sql);
                pushed_guards.insert(gi);
            }
        }

        if atom_idx == 0 {
            // Choose between VP table and temporal_facts routing.
            let table_expr = if let Some(ref tf_sql) = temporal_filter_sql
                && crate::temporal::is_temporal_predicate(pred_id)
            {
                crate::temporal::temporal_read_expr_filtered(pred_id, tf_sql)
            } else {
                vp_read_expr(pred_id)
            };
            from_clauses.push(format!("{table_expr} {alias}"));
            // Pushdown conditions on the first atom go to WHERE (no JOIN ON).
            where_clauses.extend(pushdown_conds);
        } else {
            let mut join_cond = build_join_cond(&alias, atom, &var_map);
            if !pushdown_conds.is_empty() {
                if join_cond.is_empty() {
                    join_cond = pushdown_conds.join(" AND ");
                } else {
                    join_cond = format!("{join_cond} AND {}", pushdown_conds.join(" AND "));
                }
            }
            // Choose between VP table and temporal_facts routing.
            let table_expr = if let Some(ref tf_sql) = temporal_filter_sql
                && crate::temporal::is_temporal_predicate(pred_id)
            {
                crate::temporal::temporal_read_expr_filtered(pred_id, tf_sql)
            } else {
                vp_read_expr(pred_id)
            };
            if join_cond.is_empty() {
                from_clauses.push(format!("{table_expr} {alias}"));
            } else {
                from_clauses.push(format!("JOIN {table_expr} {alias} ON {join_cond}"));
            }
        }
    }

    // ── Step 4: Process negated atoms (anti-join or NOT EXISTS) ───────────────
    for lit in &rule.body {
        if let BodyLiteral::Negated(atom) = lit {
            let pred_id = match &atom.p {
                Term::Const(id) => *id,
                _ => return Err("variable predicate in NOT atom not supported".to_owned()),
            };

            let threshold = crate::DATALOG_ANTIJOIN_THRESHOLD.get() as i64;
            let row_count = if threshold > 0 {
                estimate_pred_cardinality(pred_id)
            } else {
                0
            };

            if threshold > 0 && row_count >= threshold {
                // ── Anti-join form (v0.29.0): LEFT JOIN … IS NULL ────────────
                let aj_alias = format!("aj{antijoin_idx}");
                antijoin_idx += 1;
                let on_cond = build_antijoin_on_cond(&aj_alias, atom, &var_map);
                from_clauses.push(format!(
                    "LEFT JOIN {} {aj_alias} ON {on_cond}",
                    vp_read_expr(pred_id)
                ));
                where_clauses.push(format!("{aj_alias}.s IS NULL"));
            } else {
                // ── NOT EXISTS form (original behavior) ──────────────────────
                let inner_conds = build_not_exists_conds(atom, &var_map);
                let cond_str = if inner_conds.is_empty() {
                    "TRUE".to_owned()
                } else {
                    inner_conds.join(" AND ")
                };
                where_clauses.push(format!(
                    "NOT EXISTS (SELECT 1 FROM {} WHERE {cond_str})",
                    vp_read_expr(pred_id)
                ));
            }
        }
    }

    // ── Step 5: Process remaining guards (not pushed down) ────────────────────
    for (gi, lit) in all_guards.iter().enumerate() {
        if pushed_guards.contains(&gi) {
            continue; // already handled in pushdown
        }
        if let Some(cond_sql) = compile_guard_sql(lit, &var_map) {
            where_clauses.push(cond_sql);
        }
    }

    // ── Step 5b: Process v0.107.0 sequential temporal operators ──────────────
    // WITHIN, SEQUENCE, and CONSECUTIVE compile to standalone EXISTS subqueries.
    {
        use crate::datalog::TemporalFilter;
        for lit in &rule.body {
            if let BodyLiteral::TemporalFilter(tf) = lit {
                match tf {
                    TemporalFilter::Within(duration) => {
                        // EXISTS (SELECT 1 FROM temporal_facts WHERE s = <s_ref>
                        //         AND p = <p_id> AND valid_from >= now() - interval)
                        // We use the first bound subject variable from var_map as the
                        // subject reference (best-effort; the rule body is responsible
                        // for binding a subject via a positive atom).
                        let interval_expr = format!("INTERVAL '{duration}'");
                        let subq = format!(
                            "EXISTS (SELECT 1 FROM _pg_ripple.temporal_facts \
                             WHERE valid_from >= (now() - {interval_expr}))"
                        );
                        where_clauses.push(subq);
                    }
                    TemporalFilter::Sequence(s1, p1, o1, s2, p2, o2, window) => {
                        // Correlated subquery: event1 strictly before event2 within window.
                        // Resolve predicate IRIs to dictionary IDs.
                        let p1_id = resolve_predicate_id(p1);
                        let p2_id = resolve_predicate_id(p2);
                        let s1_ref = resolve_term_ref(s1, &var_map);
                        let o1_ref = resolve_term_ref(o1, &var_map);
                        let s2_ref = resolve_term_ref(s2, &var_map);
                        let o2_ref = resolve_term_ref(o2, &var_map);
                        let interval_expr = format!("INTERVAL '{window}'");
                        let subq = format!(
                            "EXISTS (\
                               SELECT 1 \
                               FROM _pg_ripple.temporal_facts e1 \
                               JOIN _pg_ripple.temporal_facts e2 \
                                 ON e1.s = e2.s \
                               WHERE e1.p = {p1_id} AND e2.p = {p2_id} \
                                 AND {s1_ref} AND {s2_ref} \
                                 AND {o1_ref} AND {o2_ref} \
                                 AND e1.valid_from < e2.valid_from \
                                 AND e2.valid_from - e1.valid_from <= {interval_expr}\
                             )"
                        );
                        where_clauses.push(subq);
                    }
                    TemporalFilter::Consecutive(n, pred, window) => {
                        // ROW_NUMBER() OVER (PARTITION BY s, p ORDER BY valid_from)
                        // with HAVING COUNT(*) >= n within the time window.
                        let p_id = resolve_predicate_id(pred);
                        let interval_expr = format!("INTERVAL '{window}'");
                        let subq = format!(
                            "EXISTS (\
                               SELECT 1 \
                               FROM (\
                                 SELECT s, \
                                        ROW_NUMBER() OVER (PARTITION BY s ORDER BY valid_from) AS rn, \
                                        valid_from, \
                                        MIN(valid_from) OVER (PARTITION BY s) AS first_vf \
                                 FROM _pg_ripple.temporal_facts \
                                 WHERE p = {p_id} \
                               ) ranked \
                               WHERE rn = {n} \
                                 AND valid_from - first_vf <= {interval_expr}\
                             )"
                        );
                        where_clauses.push(subq);
                    }
                    _ => {} // AFTER/BEFORE/DURING already handled above
                }
            }
        }
    }

    // ── Step 6: Process Assign literals (always in WHERE — they mutate var_map) ─
    for lit in &rule.body {
        if let BodyLiteral::Assign(var, lhs, op, rhs) = lit {
            // M-1: wrap divisor with NULLIF to prevent division-by-zero.
            let l = render_comparison_term(lhs, &var_map);
            let r_raw = render_comparison_term(rhs, &var_map);
            let r = if matches!(op, crate::datalog::ArithOp::Div) {
                format!("NULLIF({r_raw}, 0)")
            } else {
                r_raw
            };
            let op_str = arith_op_sql(op);
            let col_expr = format!("({l} {op_str} {r})");
            var_map.bind(var, &col_expr, "");
        }
    }

    // M-2: Compile-time check for unbound variables in comparisons and assigns.
    let head_text = rule
        .rule_text
        .lines()
        .next()
        .unwrap_or(&rule.rule_text)
        .trim();
    for lit in &rule.body {
        match lit {
            BodyLiteral::Compare(lhs, _, rhs) => {
                for term in [lhs, rhs] {
                    if let crate::datalog::Term::Var(v) = term
                        && var_map.col_ref(v).is_none()
                    {
                        return Err(format!(
                            "unbound variable ?{v} in comparison in rule '{head_text}': \
                             every variable in a comparison must be bound by a positive body literal"
                        ));
                    }
                }
            }
            BodyLiteral::Assign(var, lhs, _, rhs) => {
                for term in [lhs, rhs] {
                    if let crate::datalog::Term::Var(v) = term
                        && var_map.col_ref(v).is_none()
                        && v != var
                    {
                        return Err(format!(
                            "unbound variable ?{v} in assignment in rule '{head_text}': \
                             every variable in an arithmetic expression must be bound by a positive body literal"
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    // Build SELECT columns: head s and o.
    let select_s = match &head.s {
        Term::Var(v) => var_map
            .col_ref(v)
            .ok_or_else(|| format!("unbound variable ?{v} in head"))?,
        Term::Const(id) => const_sql(*id),
        Term::Wildcard => return Err("wildcard in head not allowed".to_owned()),
        Term::DefaultGraph => "0".to_owned(),
    };
    let select_o = match &head.o {
        Term::Var(v) => var_map
            .col_ref(v)
            .ok_or_else(|| format!("unbound variable ?{v} in head"))?,
        Term::Const(id) => const_sql(*id),
        Term::Wildcard => return Err("wildcard in head not allowed".to_owned()),
        Term::DefaultGraph => "0".to_owned(),
    };
    let select_g = helpers::head_g_select_expr(head, &var_map)?;

    let from_str = from_clauses.join("\n");
    let where_str = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join("\n  AND "))
    };

    Ok(format!(
        "INSERT INTO {target} (s, o, g)\n\
         SELECT {select_s}, {select_o}, {select_g}\n\
         FROM {from_str}\n\
         {where_str}\n\
         ON CONFLICT DO NOTHING"
    ))
}

// ─── Compile recursive rule ────────────────────────────────────────────────
// (See sql.rs for compile_recursive_rule implementation)

// v0.122.0 H17-02: helpers extracted to helpers.rs
pub(super) mod helpers;
pub(crate) use helpers::{
    arith_op_sql, build_join_cond, build_not_exists_conds, compare_op_sql, head_g_select_expr,
    render_comparison_term,
};
