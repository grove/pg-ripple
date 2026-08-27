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

pub use aggregate::compile_aggregate_rule;
pub use emit_cte::compile_on_demand_cte;
pub use shacl_rules::compile_constraint_check;
pub use sql::compile_rule_delta_variants_to;

use crate::datalog::{ArithOp, Atom, BodyLiteral, Rule, StringBuiltin, Term};
use pgrx::datum::DatumWithOid;

mod aggregate;
mod emit_cte;
mod emit_union;

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
        emit_union::compile_nonrecursive_rule(rule, head_pred, &head_g_expr, target)
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
        emit_union::compile_nonrecursive_rule(rule, head_pred, &head_g_expr, &target)
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

// Non-recursive rules are emitted by emit_union::compile_nonrecursive_rule.
// Optimizations are implemented by emit_union::compile_nonrecursive_rule.
// 1. Cost-based body atom reordering.
// 2. Anti-join negation.
// 3. Predicate-filter pushdown.
// v0.122.0 H17-02: helpers extracted to helpers.rs
pub(super) mod helpers;
pub(crate) use helpers::{
    arith_op_sql, build_join_cond, build_not_exists_conds, compare_op_sql, render_comparison_term,
};
