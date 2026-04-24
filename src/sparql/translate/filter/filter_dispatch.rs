//! SPARQL filter pattern dispatch utilities — SQL identifier sanitizer,
//! modifier extraction, ORDER BY translator, VALUES handler, and BIND (Extend).
//!
//! See [`filter_expr`](super::filter_expr) for the expression compilation half.

use std::collections::HashMap;

use spargebra::algebra::{Expression, GraphPattern, OrderExpression};
use spargebra::term::{GroundTerm, Literal};

use crate::dictionary;
use crate::sparql::sqlgen::{Ctx, Fragment};

// ─── SQL identifier sanitizer ─────────────────────────────────────────────────

/// Sanitize a SPARQL variable name for use as a SQL column alias.
pub(crate) fn sanitize_sql_ident(v: &str) -> String {
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

// ─── Modifier extraction helpers ─────────────────────────────────────────────

pub(crate) struct Modifiers<'a> {
    pub(crate) pattern: &'a GraphPattern,
    pub(crate) project_vars: Option<Vec<String>>,
    pub(crate) distinct: bool,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: usize,
    pub(crate) order_by: Option<String>,
    pub(crate) order_exprs: Vec<OrderExpression>,
}

pub(crate) fn extract_modifiers(mut p: &GraphPattern) -> Modifiers<'_> {
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

// ─── ORDER BY translator ──────────────────────────────────────────────────────

pub(crate) fn translate_order_by(
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
                        .map(|col| format!("{col} ASC NULLS LAST"))
                } else {
                    None
                }
            }
            OrderExpression::Desc(expr) => {
                if let Expression::Variable(v) = expr {
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

// ─── VALUES translator ────────────────────────────────────────────────────────

pub(crate) fn translate_values(
    variables: &[spargebra::term::Variable],
    bindings: &[Vec<Option<GroundTerm>>],
    ctx: &mut Ctx,
) -> Fragment {
    if variables.is_empty() || bindings.is_empty() {
        let mut frag = Fragment::empty();
        frag.conditions.push("FALSE".to_owned());
        return frag;
    }

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

pub(crate) fn encode_ground_term(gt: &GroundTerm, ctx: &mut Ctx) -> i64 {
    match gt {
        GroundTerm::NamedNode(nn) => ctx.encode_iri(nn.as_str()).unwrap_or(0),
        GroundTerm::Literal(lit) => ctx.encode_literal(lit),
        GroundTerm::Triple(t) => {
            let s_id = ctx.encode_iri(t.subject.as_str()).unwrap_or(0);
            let p_id = ctx.encode_iri(t.predicate.as_str()).unwrap_or(0);
            let o_id = encode_ground_term(&t.object, ctx);
            dictionary::lookup_quoted_triple(s_id, p_id, o_id).unwrap_or(0)
        }
    }
}

// ─── Literal lexical helpers (used by filter_expr) ────────────────────────────

pub(crate) fn literal_lexical_value(lit: &Literal) -> String {
    let val = lit.value().replace('\'', "''");
    format!("'{val}'")
}
