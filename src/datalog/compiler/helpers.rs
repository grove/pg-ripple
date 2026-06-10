//! SQL helper functions for the Datalog compiler (v0.122.0 H17-02 split).

use super::{VarMap, const_sql};
use crate::datalog::{ArithOp, Atom, CompareOp, Term};
// ─── Helpers ──────────────────────────────────────────────────────────────────

pub(crate) fn build_join_cond(alias: &str, atom: &Atom, var_map: &VarMap) -> String {
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

pub(crate) fn build_not_exists_conds(atom: &Atom, var_map: &VarMap) -> Vec<String> {
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

pub(crate) fn render_comparison_term(term: &Term, var_map: &VarMap) -> String {
    match term {
        Term::Var(v) => var_map
            .col_ref(v)
            .unwrap_or_else(|| format!("NULL /* unbound ?{v} */")),
        Term::Const(id) => const_sql(*id),
        Term::Wildcard => "NULL".to_owned(),
        Term::DefaultGraph => "0".to_owned(),
    }
}

pub(crate) fn compare_op_sql(op: &CompareOp) -> &'static str {
    match op {
        CompareOp::Gt => ">",
        CompareOp::Gte => ">=",
        CompareOp::Lt => "<",
        CompareOp::Lte => "<=",
        CompareOp::Eq => "=",
        CompareOp::Neq => "<>",
    }
}

pub(crate) fn arith_op_sql(op: &ArithOp) -> &'static str {
    match op {
        ArithOp::Add => "+",
        ArithOp::Sub => "-",
        ArithOp::Mul => "*",
        // Division uses NULLIF to prevent division-by-zero errors (M-1).
        // The caller is responsible for wrapping the RHS with NULLIF.
        ArithOp::Div => "/",
    }
}

/// SQL expression for the head graph column (constant, default graph, or bound ?g).
pub(crate) fn head_g_select_expr(head: &Atom, var_map: &VarMap) -> Result<String, String> {
    Ok(match &head.g {
        Term::Var(v) => var_map
            .col_ref(v)
            .ok_or_else(|| format!("unbound variable ?{v} in head graph"))?,
        Term::Const(id) => const_sql(*id),
        Term::DefaultGraph => "0".to_owned(),
        Term::Wildcard => "0".to_owned(),
    })
}
