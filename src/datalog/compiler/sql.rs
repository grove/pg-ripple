//! Datalog SQL compiler: recursive, on-demand CTE, semi-naive delta, aggregate rules (M15-13, v0.96.0).
//! Moved from datalog/compiler/mod.rs.

use crate::datalog::{AggFunc, AggregateLiteral, Atom, BodyLiteral, Rule, StringBuiltin, Term};

use super::{
    VarMap, arith_op_sql, build_join_cond, build_not_exists_conds, compare_op_sql, const_sql,
    head_g_select_expr, is_recursive_rule, render_comparison_term, vp_read_expr, vp_table,
};

pub(super) fn compile_recursive_rule(
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
    _head_g_expr: &str,
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
                // ── v0.109.0 NS-RL string similarity built-ins ────────────────
                StringBuiltin::TrigramSimilarity(a_term, b_term, op, rhs_term) => {
                    let a_col = render_comparison_term(a_term, &var_map);
                    let b_col = render_comparison_term(b_term, &var_map);
                    let r = render_comparison_term(rhs_term, &var_map);
                    let op_str = compare_op_sql(op);
                    where_clauses.push(format!(
                        "pg_ripple._fuzzy_match_guard({a_col}::text, {b_col}::text) {op_str} {r}"
                    ));
                }
                StringBuiltin::Levenshtein(a_term, b_term, op, rhs_term) => {
                    let a_col = render_comparison_term(a_term, &var_map);
                    let b_col = render_comparison_term(b_term, &var_map);
                    let r = render_comparison_term(rhs_term, &var_map);
                    let op_str = compare_op_sql(op);
                    where_clauses.push(format!(
                        "levenshtein({a_col}::text, {b_col}::text) {op_str} {r}"
                    ));
                }
                StringBuiltin::Soundex(s_term, op, rhs_term) => {
                    let s_col = render_comparison_term(s_term, &var_map);
                    let r = render_comparison_term(rhs_term, &var_map);
                    let op_str = compare_op_sql(op);
                    where_clauses.push(format!("soundex({s_col}::text) {op_str} {r}::text"));
                }
                StringBuiltin::Metaphone(s_term, maxlen, op, rhs_term) => {
                    let s_col = render_comparison_term(s_term, &var_map);
                    let r = render_comparison_term(rhs_term, &var_map);
                    let op_str = compare_op_sql(op);
                    where_clauses.push(format!(
                        "metaphone({s_col}::text, {maxlen}) {op_str} {r}::text"
                    ));
                }
                StringBuiltin::JaroWinkler(a_term, b_term, op, rhs_term) => {
                    let a_col = render_comparison_term(a_term, &var_map);
                    let b_col = render_comparison_term(b_term, &var_map);
                    let r = render_comparison_term(rhs_term, &var_map);
                    let op_str = compare_op_sql(op);
                    where_clauses.push(format!(
                        "jarowinkler({a_col}::text, {b_col}::text) {op_str} {r}"
                    ));
                }
                // ── v0.111.0 PPRL Bloom-filter ────────────────────────────────
                StringBuiltin::DiceSimilarity(a_term, b_term, op, rhs_term) => {
                    let a_col = render_comparison_term(a_term, &var_map);
                    let b_col = render_comparison_term(b_term, &var_map);
                    let r = render_comparison_term(rhs_term, &var_map);
                    let op_str = compare_op_sql(op);
                    where_clauses.push(format!(
                        "pg_ripple.dice_similarity({a_col}::text, {b_col}::text) {op_str} {r}"
                    ));
                }
            },
            // Aggregate literals are handled by compile_aggregate_rule, not here.
            BodyLiteral::Aggregate(_) => {}
            // v0.106.0: temporal filters are applied to the temporal_facts table expression;
            // they do not generate separate WHERE clauses in the delta variant compiler.
            BodyLiteral::TemporalFilter(_) => {}
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
    let select_g = head_g_select_expr(head, &var_map)?;

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
