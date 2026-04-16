//! SQL compiler for Datalog rules.
//!
//! Each Datalog rule is compiled to one or more SQL statements:
//!
//! - **Non-recursive rules** → `INSERT … SELECT … ON CONFLICT DO NOTHING`
//! - **Recursive rules**     → `WITH RECURSIVE … CYCLE … INSERT … SELECT`
//! - **Negation**            → `NOT EXISTS (…)` in the WHERE clause
//! - **Constraint rules**    → `SELECT EXISTS (…) AS violated`
//! - **On-demand CTEs**      → `WITH RECURSIVE cte AS (…)` prepended to SPARQL SQL
//!
//! # Integer joins everywhere
//!
//! All IRI and literal constants in rules are dictionary-encoded (`i64`)
//! at parse time.  The SQL generator never emits string comparisons.

use crate::datalog::{ArithOp, Atom, BodyLiteral, CompareOp, Rule, StringBuiltin, Term};

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Format a SQL-safe integer literal from a Term::Const.
fn const_sql(id: i64) -> String {
    id.to_string()
}

/// Render a term in SQL: `?var` → alias column reference; `Const(n)` → integer literal.
/// `alias` is the table alias for this atom's join position (e.g. "t0", "t1").
fn render_term_col(term: &Term, alias: &str, col: &str) -> String {
    match term {
        Term::Var(v) => format!("{alias}_{v}"), // resolved later as bound column
        Term::Const(id) => const_sql(*id),
        Term::Wildcard => format!("{alias}.{col}"),
        Term::DefaultGraph => "0".to_owned(),
    }
}

/// Check whether a term is a variable.
fn is_var(term: &Term) -> bool {
    matches!(term, Term::Var(_))
}

/// Derive the VP table name for a predicate constant.
/// Derived predicates use the same look-up path as base VP tables.
fn vp_table(pred_id: i64) -> String {
    // Use _pg_ripple.vp_{id} (the view that unions main and delta).
    format!("_pg_ripple.vp_{pred_id}")
}

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

    /// Return `alias.col` for a variable.
    fn col_ref(&self, var: &str) -> Option<String> {
        self.bindings
            .iter()
            .find(|(v, _, _)| v == var)
            .map(|(_, a, c)| format!("{a}.{c}"))
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

/// Compile a single derivation rule to a SQL INSERT statement.
pub fn compile_single_rule(rule: &Rule) -> Result<String, String> {
    let head = rule
        .head
        .as_ref()
        .ok_or_else(|| "cannot compile constraint rule as INSERT".to_owned())?;

    let head_pred = match &head.p {
        Term::Const(id) => *id,
        Term::Var(_) => {
            return Err("variable predicate in rule head is not supported".to_owned())
        }
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

    // Target table: use delta for HTAP tables, or vp_rare for rare predicates.
    let target = format!("{}_delta", vp_table(head_pred));

    if is_recursive {
        compile_recursive_rule(rule, head_pred, &head_g_expr, &target)
    } else {
        compile_nonrecursive_rule(rule, head_pred, &head_g_expr, &target)
    }
}

fn is_recursive_rule(rule: &Rule, head_pred: i64) -> bool {
    for lit in &rule.body {
        if let BodyLiteral::Positive(atom) = lit {
            if let Term::Const(p) = &atom.p {
                if *p == head_pred {
                    return true;
                }
            }
        }
    }
    false
}

/// Compile a non-recursive rule to `INSERT … SELECT … ON CONFLICT DO NOTHING`.
fn compile_nonrecursive_rule(
    rule: &Rule,
    head_pred: i64,
    head_g_expr: &str,
    target: &str,
) -> Result<String, String> {
    let head = rule.head.as_ref().unwrap();

    let mut from_clauses: Vec<String> = Vec::new();
    let mut join_conditions: Vec<String> = Vec::new();
    let mut where_clauses: Vec<String> = Vec::new();
    let mut var_map = VarMap::default();
    let mut atom_idx = 0usize;

    // Process positive body atoms.
    for lit in &rule.body {
        match lit {
            BodyLiteral::Positive(atom) => {
                let alias = format!("t{atom_idx}");
                let pred_id = match &atom.p {
                    Term::Const(id) => *id,
                    Term::Var(_) => {
                        return Err("variable predicate in rule body not supported".to_owned())
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
                    // DefaultGraph: g = 0 (unless rule_graph_scope = 'all')
                    let scope = crate::RULE_GRAPH_SCOPE
                        .get()
                        .as_ref()
                        .and_then(|c| c.to_str().ok())
                        .unwrap_or("default")
                        .to_owned();
                    if scope == "default" {
                        where_clauses.push(format!("{alias}.g = 0"));
                    }
                }

                if atom_idx == 0 {
                    from_clauses.push(format!("{} {alias}", vp_table(pred_id)));
                } else {
                    // Emit as JOIN with equality conditions for shared variables.
                    let join_cond = build_join_cond(&alias, atom, &var_map);
                    if join_cond.is_empty() {
                        from_clauses.push(format!("{} {alias}", vp_table(pred_id)));
                    } else {
                        from_clauses.push(format!(
                            "JOIN {} {alias} ON {}",
                            vp_table(pred_id),
                            join_cond
                        ));
                    }
                }
                atom_idx += 1;
            }
            BodyLiteral::Negated(atom) => {
                // NOT EXISTS subquery.
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
                    vp_table(pred_id)
                ));
            }
            BodyLiteral::Compare(lhs, op, rhs) => {
                let l = render_comparison_term(lhs, &var_map);
                let r = render_comparison_term(rhs, &var_map);
                let op_str = compare_op_sql(op);
                where_clauses.push(format!("{l} {op_str} {r}"));
            }
            BodyLiteral::Assign(var, lhs, op, rhs) => {
                // Arithmetic assign: computed as SELECT expression.
                let l = render_comparison_term(lhs, &var_map);
                let r = render_comparison_term(rhs, &var_map);
                let op_str = arith_op_sql(op);
                // This needs to appear as a column in SELECT; handled separately.
                let _ = (var, l, r, op_str); // placeholder
            }
            BodyLiteral::StringBuiltin(builtin) => {
                match builtin {
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
                }
            }
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
    head_g_expr: &str,
    target: &str,
) -> Result<String, String> {
    let head = rule.head.as_ref().unwrap();

    // CTE name for the recursive derived predicate.
    let cte_name = format!("derived_{head_pred}");

    // For simple transitive closure: find the base atom and recursive atom.
    let mut base_atoms: Vec<&Atom> = Vec::new();
    let mut rec_atom: Option<&Atom> = None;

    for lit in &rule.body {
        if let BodyLiteral::Positive(atom) = lit {
            if let Term::Const(p) = &atom.p {
                if *p == head_pred {
                    rec_atom = Some(atom);
                } else {
                    base_atoms.push(atom);
                }
            } else {
                base_atoms.push(atom);
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
                .unwrap_or("default")
                .to_owned();
            let g_filter = if scope == "default" { "WHERE g = 0" } else { "" };
            base_selects.push(format!(
                "SELECT s, o, g FROM {} {g_filter}",
                vp_table(*p)
            ));
        }
    }

    let base_sql = if base_selects.is_empty() {
        format!("SELECT s, o, g FROM {}", vp_table(head_pred))
    } else {
        base_selects.join("\nUNION\n")
    };

    // Recursive step.
    let rec_sql = if let Some(rec) = rec_atom {
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
        let cycle_cols = if has_graph_var {
            "s, o, g"
        } else {
            "s, o"
        };

        format!(
            "SELECT base.s, r.o, base.g\n\
             FROM {} base\n\
             JOIN {cte_name} r ON r.s = base.o {join_g}",
            vp_table(base_pred)
        )
    } else {
        // Fallback: direct recursion on CTE.
        format!(
            "SELECT base.s, r.o, base.g\n\
             FROM {} base\n\
             JOIN {cte_name} r ON r.s = base.o",
            vp_table(head_pred)
        )
    };

    let has_graph_var = matches!(&head.g, Term::Var(_));
    let cycle_cols = if has_graph_var { "s, o, g" } else { "s, o" };

    let select_s = match &head.s {
        Term::Var(v) => {
            if v == "x" || v == "s" {
                format!("{cte_name}.s")
            } else {
                format!("{cte_name}.s")
            }
        }
        Term::Const(id) => const_sql(*id),
        _ => format!("{cte_name}.s"),
    };
    let select_o = match &head.o {
        Term::Var(v) => {
            if v == "z" || v == "o" {
                format!("{cte_name}.o")
            } else {
                format!("{cte_name}.o")
            }
        }
        Term::Const(id) => const_sql(*id),
        _ => format!("{cte_name}.o"),
    };

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
                    from_clauses.push(format!("{} {alias}", vp_table(pred_id)));
                } else {
                    let join_cond = build_join_cond(&alias, atom, &var_map);
                    if join_cond.is_empty() {
                        from_clauses.push(format!("{} {alias}", vp_table(pred_id)));
                    } else {
                        from_clauses.push(format!(
                            "JOIN {} {alias} ON {}",
                            vp_table(pred_id),
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
    Ok(format!(
        "WITH {cte_name}(s, o, g) AS (\n{union_body}\n)"
    ))
}

fn compile_recursive_cte_fragment(
    rule: &Rule,
    head_pred: i64,
    cte_name: &str,
) -> Result<String, String> {
    let head = rule.head.as_ref().unwrap();

    // Base case: non-recursive body atoms.
    let mut base_selects: Vec<String> = Vec::new();
    for lit in &rule.body {
        if let BodyLiteral::Positive(atom) = lit {
            if let Term::Const(p) = &atom.p {
                if *p != head_pred {
                    base_selects.push(format!("SELECT s, o, g FROM {}", vp_table(*p)));
                }
            }
        }
    }

    let base_sql = if base_selects.is_empty() {
        format!("SELECT s, o, g FROM {}", vp_table(head_pred))
    } else {
        base_selects.join("\nUNION\n")
    };

    // Find the base source predicate for recursive step.
    let base_pred = rule
        .body
        .iter()
        .find_map(|lit| {
            if let BodyLiteral::Positive(atom) = lit {
                if let Term::Const(p) = &atom.p {
                    if *p != head_pred {
                        return Some(*p);
                    }
                }
            }
            None
        })
        .unwrap_or(head_pred);

    let has_graph_var = matches!(&head.g, Term::Var(_));
    let join_g = if has_graph_var { "AND r.g = base.g" } else { "" };
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
        vp_table(base_pred)
    ))
}

fn compile_select_arm(rule: &Rule, head: &Atom) -> Result<String, String> {
    let mut from_clauses: Vec<String> = Vec::new();
    let mut where_clauses: Vec<String> = Vec::new();
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
                from_clauses.push(format!("{} {alias}", vp_table(pred_id)));
            } else {
                let join_cond = build_join_cond(&alias, atom, &var_map);
                if join_cond.is_empty() {
                    from_clauses.push(format!("{} {alias}", vp_table(pred_id)));
                } else {
                    from_clauses.push(format!(
                        "JOIN {} {alias} ON {}",
                        vp_table(pred_id),
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
            .unwrap_or("default")
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
        Term::Var(v) => var_map.col_ref(v).unwrap_or_else(|| format!("NULL /* unbound ?{v} */")),
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
        ArithOp::Div => "/",
    }
}
