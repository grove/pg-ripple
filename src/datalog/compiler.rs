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

use crate::datalog::{
    AggFunc, AggregateLiteral, ArithOp, Atom, BodyLiteral, CompareOp, Rule, StringBuiltin, Term,
};
use pgrx::datum::DatumWithOid;

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Format a SQL-safe integer literal from a Term::Const.
fn const_sql(id: i64) -> String {
    id.to_string()
}

/// Render a term in SQL: `?var` → alias column reference; `Const(n)` → integer literal.
/// `alias` is the table alias for this atom's join position (e.g. "t0", "t1").
#[allow(dead_code)] // used by compile_rule_delta_variants and compile_single_rule_to
fn render_term_col(term: &Term, alias: &str, col: &str) -> String {
    match term {
        Term::Var(v) => format!("{alias}_{v}"), // resolved later as bound column
        Term::Const(id) => const_sql(*id),
        Term::Wildcard => format!("{alias}.{col}"),
        Term::DefaultGraph => "0".to_owned(),
    }
}

/// Check whether a term is a variable.
#[allow(dead_code)] // used in conditional logic and future optimizations
fn is_var(term: &Term) -> bool {
    matches!(term, Term::Var(_))
}

/// Derive the VP table name for a predicate constant.
/// Used for INSERT targets (assumes dedicated HTAP tables exist).
fn vp_table(pred_id: i64) -> String {
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

fn vp_read_expr(pred_id: i64) -> String {
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
fn estimate_pred_cardinality(pred_id: i64) -> i64 {
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
fn cost_order_atoms(rule: &Rule) -> Vec<&Atom> {
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
fn build_antijoin_on_cond(aj_alias: &str, atom: &Atom, var_map: &VarMap) -> String {
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
fn guard_fully_bound(guard: &BodyLiteral, var_map: &VarMap) -> bool {
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
        },
        _ => false,
    }
}

/// Compile a guard literal (Compare, Assign, StringBuiltin) to a SQL string
/// suitable for inclusion in a WHERE or JOIN ON clause.
fn compile_guard_sql(guard: &BodyLiteral, var_map: &VarMap) -> Option<String> {
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
        },
        _ => None,
    }
}

// ─── Variable map ─────────────────────────────────────────────────────────────

/// Variable→(alias, column) mapping built while iterating body atoms.
#[derive(Default)]
struct VarMap {
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
        compile_recursive_rule(rule, head_pred, &head_g_expr, target)
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
        compile_recursive_rule(rule, head_pred, &head_g_expr, &target)
    } else {
        compile_nonrecursive_rule(rule, head_pred, &head_g_expr, &target)
    }
}

fn is_recursive_rule(rule: &Rule, head_pred: i64) -> bool {
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
    head_g_expr: &str,
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
            from_clauses.push(format!("{} {alias}", vp_read_expr(pred_id)));
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
            if join_cond.is_empty() {
                from_clauses.push(format!("{} {alias}", vp_read_expr(pred_id)));
            } else {
                from_clauses.push(format!(
                    "JOIN {} {alias} ON {}",
                    vp_read_expr(pred_id),
                    join_cond
                ));
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

    let from_str = from_clauses.join("\n");
    let where_str = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join("\n  AND "))
    };

    Ok(format!(
        "INSERT INTO {target} (s, o, g)\n\
         SELECT {select_s}, {select_o}, {head_g_expr}\n\
         FROM {from_str}\n\
         {where_str}\n\
         ON CONFLICT DO NOTHING"
    ))
}

/// Compile a recursive rule to a `WITH RECURSIVE … INSERT … SELECT`.
fn compile_recursive_rule(
    rule: &Rule,
    head_pred: i64,
    _head_g_expr: &str,
    target: &str,
) -> Result<String, String> {
    let head = rule
        .head
        .as_ref()
        .ok_or_else(|| "compile_linear_recursive_rule: rule has no head".to_string())?;

    // v0.34.0: bounded-depth termination — read GUC at compile time.
    let max_depth = crate::DATALOG_MAX_DEPTH.get();

    // CTE name for the recursive derived predicate.
    let cte_name = format!("derived_{head_pred}");

    // For simple transitive closure: find the base atom and recursive atom.
    let mut base_atoms: Vec<&Atom> = Vec::new();
    let mut rec_atom: Option<&Atom> = None;

    for lit in &rule.body {
        if let BodyLiteral::Positive(atom) = lit {
            match &atom.p {
                Term::Var(_) => {
                    // Variable predicate in recursive rule body — not supported.
                    // Return Err so the caller can try the variable-predicate runtime path.
                    return Err(
                        "variable predicate in recursive rule body not supported".to_owned()
                    );
                }
                Term::Const(p) if *p == head_pred => {
                    rec_atom = Some(atom);
                }
                Term::Const(_) => {
                    base_atoms.push(atom);
                }
                _ => {
                    base_atoms.push(atom);
                }
            }
        }
    }

    // Base case: base_atoms only.
    let mut base_selects: Vec<String> = Vec::new();
    for base_atom in &base_atoms {
        if let Term::Const(p) = &base_atom.p {
            let scope = crate::RULE_GRAPH_SCOPE
                .get()
                .as_ref()
                .and_then(|c| c.to_str().ok())
                .unwrap_or("all")
                .to_owned();
            let g_filter = if scope == "default" {
                "WHERE g = 0"
            } else {
                ""
            };
            base_selects.push(format!(
                "SELECT s, o, g FROM {} {g_filter}",
                vp_read_expr(*p)
            ));
        }
    }

    // Build base SQL.  When there are multiple base predicates we must NOT
    // join them with a bare UNION at the top level, because PostgreSQL's
    // CYCLE clause requires the left side of the outer UNION to be a plain
    // SELECT — a SetOperationStmt on the left triggers "the left side of the
    // UNION must be a SELECT".  Wrap in a subquery instead.
    let base_sql = if base_selects.is_empty() {
        // Seed from the current VP table.  Use alias _vp so PostgreSQL
        // accepts the derived-table expression.
        format!("SELECT s, o, g FROM {} _vp", vp_read_expr(head_pred))
    } else if base_selects.len() == 1 {
        base_selects[0].clone()
    } else {
        // Wrap multiple base predicates in a single derived table so the
        // left side of the outer UNION remains a plain SELECT.
        format!(
            "SELECT s, o, g FROM (\n  {}\n) _base",
            base_selects.join("\n  UNION ALL\n  ")
        )
    };

    // Recursive step.
    let rec_sql = if let Some(_rec) = rec_atom {
        let base_pred = base_atoms
            .first()
            .and_then(|a| {
                if let Term::Const(p) = &a.p {
                    Some(*p)
                } else {
                    None
                }
            })
            .unwrap_or(head_pred);

        let has_graph_var = matches!(&head.g, Term::Var(_));
        let join_g = if has_graph_var {
            "AND r.g = base.g"
        } else {
            ""
        };
        format!(
            "SELECT base.s, r.o, base.g\n\
             FROM {} base\n\
             JOIN {cte_name} r ON r.s = base.o {join_g}",
            vp_read_expr(base_pred)
        )
    } else {
        // Fallback: direct recursion on CTE.
        format!(
            "SELECT base.s, r.o, base.g\n\
             FROM {} base\n\
             JOIN {cte_name} r ON r.s = base.o",
            vp_read_expr(head_pred)
        )
    };

    let has_graph_var = matches!(&head.g, Term::Var(_));
    let cycle_cols = if has_graph_var { "s, o, g" } else { "s, o" };

    let select_s = match &head.s {
        Term::Var(_) => format!("{cte_name}.s"),
        Term::Const(id) => const_sql(*id),
        _ => format!("{cte_name}.s"),
    };
    let select_o = match &head.o {
        Term::Var(_) => format!("{cte_name}.o"),
        Term::Const(id) => const_sql(*id),
        _ => format!("{cte_name}.o"),
    };

    // v0.34.0: bounded-depth termination using a depth counter column.
    if max_depth > 0 {
        // Inject a depth column into both base and recursive cases.
        //
        // base_sql has the form: SELECT s, o, g FROM ...
        // We inject ", 0 AS depth" into the SELECT list.
        let base_sql_depth = base_sql.replacen("SELECT s, o, g", "SELECT s, o, g, 0 AS depth", 1);
        // For multi-union base (UNION of multiple base predicates), replace all occurrences.
        let base_sql_depth =
            base_sql_depth.replace("SELECT s, o, g FROM", "SELECT s, o, g, 0 AS depth FROM");

        // rec_sql has the form:
        //   SELECT base.s, r.o, base.g\nFROM vp_X base\nJOIN {cte_name} r ON r.s = base.o
        // We inject "r.depth + 1 AS depth" into the SELECT list and add WHERE r.depth < max_depth.
        let rec_sql_depth = rec_sql.replacen("base.g", "base.g, r.depth + 1 AS depth", 1);
        let rec_sql_depth = format!("{rec_sql_depth}\nWHERE r.depth < {max_depth}");

        Ok(format!(
            "WITH RECURSIVE {cte_name}(s, o, g, depth) AS (\n\
                 {base_sql_depth}\n\
               UNION\n\
                 {rec_sql_depth}\n\
             )\n\
             CYCLE {cycle_cols} SET is_cycle USING cycle_path\n\
             INSERT INTO {target} (s, o, g)\n\
             SELECT {select_s}, {select_o}, {cte_name}.g\n\
             FROM {cte_name}\n\
             WHERE NOT is_cycle\n\
             ON CONFLICT DO NOTHING"
        ))
    } else {
        Ok(format!(
            "WITH RECURSIVE {cte_name}(s, o, g) AS (\n\
                 {base_sql}\n\
               UNION\n\
                 {rec_sql}\n\
             )\n\
             CYCLE {cycle_cols} SET is_cycle USING cycle_path\n\
             INSERT INTO {target} (s, o, g)\n\
             SELECT {select_s}, {select_o}, {cte_name}.g\n\
             FROM {cte_name}\n\
             WHERE NOT is_cycle\n\
             ON CONFLICT DO NOTHING"
        ))
    }
}

// ─── Constraint check compiler ────────────────────────────────────────────────

/// Compile a constraint rule (empty head) to a `SELECT EXISTS (…) AS violated`.
pub fn compile_constraint_check(rule: &Rule) -> Result<String, String> {
    if rule.head.is_some() {
        return Err("not a constraint rule".to_owned());
    }

    let mut from_clauses: Vec<String> = Vec::new();
    let mut where_clauses: Vec<String> = Vec::new();
    let mut var_map = VarMap::default();
    let mut atom_idx = 0usize;

    for lit in &rule.body {
        match lit {
            BodyLiteral::Positive(atom) => {
                let alias = format!("t{atom_idx}");
                let pred_id = match &atom.p {
                    Term::Const(id) => *id,
                    _ => return Err("variable predicate in constraint body".to_owned()),
                };

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

                if atom_idx == 0 {
                    from_clauses.push(format!("{} {alias}", vp_read_expr(pred_id)));
                } else {
                    let join_cond = build_join_cond(&alias, atom, &var_map);
                    if join_cond.is_empty() {
                        from_clauses.push(format!("{} {alias}", vp_read_expr(pred_id)));
                    } else {
                        from_clauses.push(format!(
                            "JOIN {} {alias} ON {}",
                            vp_read_expr(pred_id),
                            join_cond
                        ));
                    }
                }
                atom_idx += 1;
            }
            BodyLiteral::Compare(lhs, op, rhs) => {
                let l = render_comparison_term(lhs, &var_map);
                let r = render_comparison_term(rhs, &var_map);
                let op_str = compare_op_sql(op);
                where_clauses.push(format!("{l} {op_str} {r}"));
            }
            _ => {}
        }
    }

    if from_clauses.is_empty() {
        return Ok("SELECT FALSE AS violated".to_owned());
    }

    let from_str = from_clauses.join("\n");
    let where_str = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    Ok(format!(
        "SELECT EXISTS (\n\
             SELECT 1 FROM {from_str}\n\
             {where_str}\n\
         ) AS violated"
    ))
}

// ─── On-demand CTE compiler ──────────────────────────────────────────────────

/// Compile an on-demand CTE for a derived predicate.
///
/// The returned string is a `WITH RECURSIVE cte_name(s, o, g) AS (…)` fragment
/// that can be prepended to a SPARQL→SQL query.
pub fn compile_on_demand_cte(rules: &[Rule], pred_id: i64) -> Result<String, String> {
    let cte_name = format!("derived_{pred_id}");
    let mut selects: Vec<String> = Vec::new();

    for rule in rules {
        let Some(head) = &rule.head else { continue };
        let head_pred = match &head.p {
            Term::Const(id) => *id,
            _ => continue,
        };
        if head_pred != pred_id {
            continue;
        }

        let is_recursive = is_recursive_rule(rule, head_pred);
        if is_recursive {
            // Return the full recursive CTE.
            return compile_recursive_cte_fragment(rule, pred_id, &cte_name);
        }

        // Non-recursive: build one SELECT arm.
        let select = compile_select_arm(rule, head)?;
        selects.push(select);
    }

    if selects.is_empty() {
        return Err(format!("no rules found for predicate {pred_id}"));
    }

    let union_body = selects.join("\nUNION ALL\n");
    Ok(format!("WITH {cte_name}(s, o, g) AS (\n{union_body}\n)"))
}

fn compile_recursive_cte_fragment(
    rule: &Rule,
    head_pred: i64,
    cte_name: &str,
) -> Result<String, String> {
    let head = rule
        .head
        .as_ref()
        .ok_or_else(|| "compile_recursive_cte_fragment: rule has no head".to_string())?;

    // Base case: non-recursive body atoms.
    let mut base_selects: Vec<String> = Vec::new();
    for lit in &rule.body {
        if let BodyLiteral::Positive(atom) = lit
            && let Term::Const(p) = &atom.p
            && *p != head_pred
        {
            base_selects.push(format!("SELECT s, o, g FROM {}", vp_read_expr(*p)));
        }
    }

    let base_sql = if base_selects.is_empty() {
        format!("SELECT s, o, g FROM {} _vp", vp_read_expr(head_pred))
    } else if base_selects.len() == 1 {
        base_selects[0].clone()
    } else {
        format!(
            "SELECT s, o, g FROM (\n  {}\n) _base",
            base_selects.join("\n  UNION ALL\n  ")
        )
    };

    // Find the base source predicate for recursive step.
    let base_pred = rule
        .body
        .iter()
        .find_map(|lit| {
            if let BodyLiteral::Positive(atom) = lit
                && let Term::Const(p) = &atom.p
                && *p != head_pred
            {
                return Some(*p);
            }
            None
        })
        .unwrap_or(head_pred);

    let has_graph_var = matches!(&head.g, Term::Var(_));
    let join_g = if has_graph_var {
        "AND r.g = base.g"
    } else {
        ""
    };
    let cycle_cols = if has_graph_var { "s, o, g" } else { "s, o" };

    Ok(format!(
        "WITH RECURSIVE {cte_name}(s, o, g) AS (\n\
             {base_sql}\n\
           UNION\n\
             SELECT base.s, r.o, base.g\n\
             FROM {} base\n\
             JOIN {cte_name} r ON r.s = base.o {join_g}\n\
         )\n\
         CYCLE {cycle_cols} SET is_cycle USING cycle_path",
        vp_read_expr(base_pred)
    ))
}

fn compile_select_arm(rule: &Rule, head: &Atom) -> Result<String, String> {
    let mut from_clauses: Vec<String> = Vec::new();
    let where_clauses: Vec<String> = Vec::new();
    let mut var_map = VarMap::default();
    let mut atom_idx = 0usize;

    for lit in &rule.body {
        if let BodyLiteral::Positive(atom) = lit {
            let alias = format!("t{atom_idx}");
            let pred_id = match &atom.p {
                Term::Const(id) => *id,
                _ => continue,
            };

            if let Term::Var(v) = &atom.s {
                var_map.bind(v, &alias, "s");
            }
            if let Term::Var(v) = &atom.o {
                var_map.bind(v, &alias, "o");
            }
            if let Term::Var(v) = &atom.g {
                var_map.bind(v, &alias, "g");
            }

            if atom_idx == 0 {
                from_clauses.push(format!("{} {alias}", vp_read_expr(pred_id)));
            } else {
                let join_cond = build_join_cond(&alias, atom, &var_map);
                if join_cond.is_empty() {
                    from_clauses.push(format!("{} {alias}", vp_read_expr(pred_id)));
                } else {
                    from_clauses.push(format!(
                        "JOIN {} {alias} ON {}",
                        vp_read_expr(pred_id),
                        join_cond
                    ));
                }
            }
            atom_idx += 1;
        }
    }

    let select_s = match &head.s {
        Term::Var(v) => var_map
            .col_ref(v)
            .ok_or_else(|| format!("unbound variable ?{v} in head"))?,
        Term::Const(id) => const_sql(*id),
        _ => return Err("invalid head subject term".to_owned()),
    };
    let select_o = match &head.o {
        Term::Var(v) => var_map
            .col_ref(v)
            .ok_or_else(|| format!("unbound variable ?{v} in head"))?,
        Term::Const(id) => const_sql(*id),
        _ => return Err("invalid head object term".to_owned()),
    };
    let select_g = match &head.g {
        Term::Var(v) => var_map.col_ref(v).unwrap_or_else(|| "0".to_owned()),
        Term::Const(id) => const_sql(*id),
        Term::DefaultGraph => "0".to_owned(),
        Term::Wildcard => "0".to_owned(),
    };

    let from_str = from_clauses.join("\n");
    let where_str = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("\nWHERE {}", where_clauses.join(" AND "))
    };

    Ok(format!(
        "SELECT {select_s} AS s, {select_o} AS o, {select_g} AS g\nFROM {from_str}{where_str}"
    ))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn build_join_cond(alias: &str, atom: &Atom, var_map: &VarMap) -> String {
    let mut conds = Vec::new();

    if let Term::Var(v) = &atom.s {
        if let Some(ref_col) = var_map.col_ref(v) {
            conds.push(format!("{alias}.s = {ref_col}"));
        }
    } else if let Term::Const(c) = &atom.s {
        conds.push(format!("{alias}.s = {}", const_sql(*c)));
    }
    if let Term::Var(v) = &atom.o {
        if let Some(ref_col) = var_map.col_ref(v) {
            conds.push(format!("{alias}.o = {ref_col}"));
        }
    } else if let Term::Const(c) = &atom.o {
        conds.push(format!("{alias}.o = {}", const_sql(*c)));
    }
    if let Term::Var(v) = &atom.g {
        if let Some(ref_col) = var_map.col_ref(v) {
            conds.push(format!("{alias}.g = {ref_col}"));
        }
    } else if let Term::Const(c) = &atom.g {
        conds.push(format!("{alias}.g = {}", const_sql(*c)));
    } else {
        let scope = crate::RULE_GRAPH_SCOPE
            .get()
            .as_ref()
            .and_then(|c| c.to_str().ok())
            .unwrap_or("all")
            .to_owned();
        if scope == "default" {
            conds.push(format!("{alias}.g = 0"));
        }
    }
    conds.join(" AND ")
}

fn build_not_exists_conds(atom: &Atom, var_map: &VarMap) -> Vec<String> {
    let mut conds = Vec::new();
    if let Term::Var(v) = &atom.s {
        if let Some(ref_col) = var_map.col_ref(v) {
            conds.push(format!("s = {ref_col}"));
        }
    } else if let Term::Const(c) = &atom.s {
        conds.push(format!("s = {}", const_sql(*c)));
    }
    if let Term::Var(v) = &atom.o {
        if let Some(ref_col) = var_map.col_ref(v) {
            conds.push(format!("o = {ref_col}"));
        }
    } else if let Term::Const(c) = &atom.o {
        conds.push(format!("o = {}", const_sql(*c)));
    }
    conds
}

fn render_comparison_term(term: &Term, var_map: &VarMap) -> String {
    match term {
        Term::Var(v) => var_map
            .col_ref(v)
            .unwrap_or_else(|| format!("NULL /* unbound ?{v} */")),
        Term::Const(id) => const_sql(*id),
        Term::Wildcard => "NULL".to_owned(),
        Term::DefaultGraph => "0".to_owned(),
    }
}

fn compare_op_sql(op: &CompareOp) -> &'static str {
    match op {
        CompareOp::Gt => ">",
        CompareOp::Gte => ">=",
        CompareOp::Lt => "<",
        CompareOp::Lte => "<=",
        CompareOp::Eq => "=",
        CompareOp::Neq => "<>",
    }
}

fn arith_op_sql(op: &ArithOp) -> &'static str {
    match op {
        ArithOp::Add => "+",
        ArithOp::Sub => "-",
        ArithOp::Mul => "*",
        // Division uses NULLIF to prevent division-by-zero errors (M-1).
        // The caller is responsible for wrapping the RHS with NULLIF.
        ArithOp::Div => "/",
    }
}

// ─── Semi-naive delta variant compilation ────────────────────────────────────

/// Compile all semi-naive delta variants of a non-recursive rule.
///
/// For each body atom at position `i` that uses a derived predicate, generates
/// one INSERT variant where:
///   - atom `i` reads from the **delta** table (`delta_name(pred_id)`)
///   - all preceding atoms read from the **full** VP table
///   - all following atoms read from the **full** VP table
///
/// This implements the standard semi-naive evaluation principle:
/// only consider tuples that include at least one row from the delta
/// of the previous iteration, avoiding redundant recomputation.
///
/// Like `compile_rule_delta_variants_to` but uses the default VP delta table targets.
#[allow(dead_code)] // public API; compile_rule_delta_variants_to is the canonical form
pub fn compile_rule_delta_variants(
    rule: &Rule,
    derived_pred_ids: &std::collections::HashSet<i64>,
    delta_table_name: &dyn Fn(i64) -> String,
) -> Result<Vec<String>, String> {
    compile_rule_delta_variants_to(rule, derived_pred_ids, delta_table_name, None)
}

/// Like `compile_rule_delta_variants` but inserts into tables named by `target_fn`
/// instead of the default `_pg_ripple.vp_{pred_id}_delta`. Used by semi-naive
/// inference when targeting temp tables.
pub fn compile_rule_delta_variants_to(
    rule: &Rule,
    derived_pred_ids: &std::collections::HashSet<i64>,
    delta_table_name: &dyn Fn(i64) -> String,
    target_fn: Option<&dyn Fn(i64) -> String>,
) -> Result<Vec<String>, String> {
    let head = match &rule.head {
        Some(h) => h,
        None => return Ok(vec![]), // constraint rules have no head
    };

    let head_pred = match &head.p {
        Term::Const(id) => *id,
        _ => return Err("variable predicate in rule head is not supported".to_owned()),
    };

    let head_g_expr = match &head.g {
        Term::Const(id) => const_sql(*id),
        Term::Var(v) => format!("g_var_{v}"),
        Term::DefaultGraph => "0".to_owned(),
        Term::Wildcard => "0".to_owned(),
    };

    let target = if let Some(tf) = target_fn {
        tf(head_pred)
    } else {
        format!("{}_delta", vp_table(head_pred))
    };

    // Collect body atoms that reference derived predicates with their positions.
    let positive_body: Vec<&Atom> = rule
        .body
        .iter()
        .filter_map(|lit| {
            if let BodyLiteral::Positive(atom) = lit {
                Some(atom)
            } else {
                None
            }
        })
        .collect();

    let mut variants: Vec<String> = Vec::new();

    // For each positive body atom position that uses a derived predicate,
    // generate one semi-naive variant.
    for (delta_pos, atom) in positive_body.iter().enumerate() {
        let pred_id = match &atom.p {
            Term::Const(id) => *id,
            _ => continue,
        };
        if !derived_pred_ids.contains(&pred_id) {
            continue; // not a derived predicate → skip
        }

        // Generate the variant: compile with atom at delta_pos using delta table.
        let sql = compile_rule_with_one_delta_atom(
            rule,
            head_pred,
            &head_g_expr,
            &target,
            delta_pos,
            delta_table_name,
        )?;
        variants.push(sql);
    }

    Ok(variants)
}

/// Compile a single rule variant where the body atom at `delta_atom_pos` (counting
/// only positive atoms) uses `delta_table_name(pred_id)` instead of the full VP table.
fn compile_rule_with_one_delta_atom(
    rule: &Rule,
    _head_pred: i64,
    head_g_expr: &str,
    target: &str,
    delta_atom_pos: usize,
    delta_table_name: &dyn Fn(i64) -> String,
) -> Result<String, String> {
    let head = rule
        .head
        .as_ref()
        .ok_or_else(|| "compile_rule_delta_variants_to: rule has no head".to_string())?;

    let mut from_clauses: Vec<String> = Vec::new();
    let mut where_clauses: Vec<String> = Vec::new();
    let mut var_map = VarMap::default();
    let mut pos_atom_idx = 0usize; // index among positive atoms
    let mut alias_idx = 0usize;

    for lit in &rule.body {
        match lit {
            BodyLiteral::Positive(atom) => {
                let alias = format!("t{alias_idx}");
                alias_idx += 1;

                let pred_id = match &atom.p {
                    Term::Const(id) => *id,
                    _ => {
                        return Err("variable predicate in body not supported".to_owned());
                    }
                };

                // Use delta table for this atom if it is the chosen delta position.
                let tbl = if pos_atom_idx == delta_atom_pos {
                    delta_table_name(pred_id)
                } else {
                    vp_read_expr(pred_id)
                };
                pos_atom_idx += 1;

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

                if alias_idx == 1 {
                    from_clauses.push(format!("{tbl} {alias}"));
                } else {
                    let join_cond = build_join_cond(&alias, atom, &var_map);
                    if join_cond.is_empty() {
                        from_clauses.push(format!("{tbl} {alias}"));
                    } else {
                        from_clauses.push(format!("JOIN {tbl} {alias} ON {join_cond}"));
                    }
                }
            }
            BodyLiteral::Negated(atom) => {
                let pred_id = match &atom.p {
                    Term::Const(id) => *id,
                    _ => return Err("variable predicate in NOT atom not supported".to_owned()),
                };
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
            BodyLiteral::Compare(lhs, op, rhs) => {
                let l = render_comparison_term(lhs, &var_map);
                let r = render_comparison_term(rhs, &var_map);
                let op_str = compare_op_sql(op);
                where_clauses.push(format!("{l} {op_str} {r}"));
            }
            BodyLiteral::Assign(var, lhs, op, rhs) => {
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
            BodyLiteral::StringBuiltin(builtin) => match builtin {
                StringBuiltin::Strlen(term, op, rhs_term) => {
                    let col = render_comparison_term(term, &var_map);
                    let r = render_comparison_term(rhs_term, &var_map);
                    let op_str = compare_op_sql(op);
                    where_clauses.push(format!("LENGTH({col}::text) {op_str} {r}"));
                }
                StringBuiltin::Regex(term, pattern) => {
                    let col = render_comparison_term(term, &var_map);
                    let escaped = pattern.replace('\'', "''");
                    where_clauses.push(format!("{col}::text ~ '{escaped}'"));
                }
            },
            // Aggregate literals are handled by compile_aggregate_rule, not here.
            BodyLiteral::Aggregate(_) => {}
        }
    }

    let select_s = match &head.s {
        Term::Var(v) => var_map
            .col_ref(v)
            .ok_or_else(|| format!("unbound variable ?{v} in head"))?,
        Term::Const(id) => const_sql(*id),
        _ => return Err("wildcard/invalid in head not allowed".to_owned()),
    };
    let select_o = match &head.o {
        Term::Var(v) => var_map
            .col_ref(v)
            .ok_or_else(|| format!("unbound variable ?{v} in head"))?,
        Term::Const(id) => const_sql(*id),
        _ => return Err("wildcard/invalid in head not allowed".to_owned()),
    };

    let from_str = from_clauses.join("\n");
    let where_str = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join("\n  AND "))
    };

    Ok(format!(
        "INSERT INTO {target} (s, o, g)\n\
         SELECT {select_s}, {select_o}, {head_g_expr}\n\
         FROM {from_str}\n\
         {where_str}\n\
         ON CONFLICT DO NOTHING"
    ))
}

// ─── v0.30.0: Aggregate rule compilation ─────────────────────────────────────

/// Compile an aggregate rule to a GROUP BY SQL INSERT statement (v0.30.0).
///
/// Aggregate rules have the form:
/// ```text
/// ?x pred ?n :- COUNT(?y WHERE ?x bodyPred ?y) = ?n .
/// ```
/// See `crate::datalog::AggregateLiteral` for the IR.
pub fn compile_aggregate_rule(rule: &Rule, target: &str) -> Result<String, String> {
    let head = rule
        .head
        .as_ref()
        .ok_or_else(|| "aggregate rule must have a head".to_owned())?;

    let agg_lit: &AggregateLiteral = rule
        .body
        .iter()
        .find_map(|lit| {
            if let BodyLiteral::Aggregate(a) = lit {
                Some(a)
            } else {
                None
            }
        })
        .ok_or_else(|| "no aggregate literal found in rule body".to_owned())?;

    let pred_id = match &agg_lit.atom.p {
        Term::Const(id) => *id,
        _ => return Err("aggregate atom predicate must be a constant".to_owned()),
    };

    let agg_func_sql = match agg_lit.func {
        AggFunc::Count => "COUNT",
        AggFunc::Sum => "SUM",
        AggFunc::Min => "MIN",
        AggFunc::Max => "MAX",
        AggFunc::Avg => "AVG",
    };

    let (agg_col, group_col) = match (&agg_lit.atom.s, &agg_lit.atom.o) {
        (Term::Var(s_var), Term::Var(o_var)) => {
            if s_var == &agg_lit.agg_var {
                ("s", "o")
            } else if o_var == &agg_lit.agg_var {
                ("o", "s")
            } else {
                return Err(format!(
                    "agg_var '{}' not found in atom s or o positions",
                    agg_lit.agg_var
                ));
            }
        }
        (Term::Const(_), Term::Var(o_var)) => {
            if o_var == &agg_lit.agg_var {
                ("o", "s")
            } else {
                return Err(format!(
                    "agg_var '{}' not in atom (s=const, o={o_var})",
                    agg_lit.agg_var
                ));
            }
        }
        (Term::Var(s_var), Term::Const(_)) => {
            if s_var == &agg_lit.agg_var {
                ("s", "o")
            } else {
                return Err(format!(
                    "agg_var '{}' not in atom (s={s_var}, o=const)",
                    agg_lit.agg_var
                ));
            }
        }
        _ => {
            return Err("aggregate atom must have at least one variable".to_owned());
        }
    };

    let result_in_head_s = matches!(&head.s, Term::Var(v) if *v == agg_lit.result_var);
    let result_in_head_o = matches!(&head.o, Term::Var(v) if *v == agg_lit.result_var);

    if !result_in_head_s && !result_in_head_o {
        return Err(format!(
            "result_var '{}' not found in head subject or object",
            agg_lit.result_var
        ));
    }

    let agg_expr =
        format!("pg_ripple.encode_term({agg_func_sql}(t0.{agg_col})::text, 2::smallint)");
    let group_expr = format!("t0.{group_col}");

    let (insert_s, insert_o) = if result_in_head_o {
        (group_expr, agg_expr)
    } else {
        (agg_expr, group_expr)
    };

    let scope = crate::RULE_GRAPH_SCOPE
        .get()
        .as_ref()
        .and_then(|c| c.to_str().ok())
        .unwrap_or("all")
        .to_owned();

    let mut where_parts: Vec<String> = Vec::new();
    if scope == "default" {
        where_parts.push("t0.g = 0".to_owned());
    }
    if let Term::Const(c) = &agg_lit.atom.s {
        where_parts.push(format!("t0.s = {}", const_sql(*c)));
    }
    if let Term::Const(c) = &agg_lit.atom.o {
        where_parts.push(format!("t0.o = {}", const_sql(*c)));
    }

    let where_str = if where_parts.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_parts.join(" AND "))
    };

    Ok(format!(
        "INSERT INTO {target} (s, o, g)\n\
         SELECT {insert_s}, {insert_o}, 0\n\
         FROM {source} t0\n\
         {where_str}\n\
         GROUP BY t0.{group_col}\n\
         ON CONFLICT DO NOTHING",
        source = vp_read_expr(pred_id)
    ))
}
