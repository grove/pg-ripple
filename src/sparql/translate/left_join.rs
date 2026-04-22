//! LeftJoin (OPTIONAL) translator.

use spargebra::algebra::{Expression, GraphPattern};

use crate::sparql::sqlgen::{Ctx, Fragment};
use crate::sparql::translate::bgp::shacl_right_is_mandatory;
use crate::sparql::translate::filter::{sanitize_sql_ident, translate_expr};

pub(crate) fn translate_left_join(
    left: &GraphPattern,
    right: &GraphPattern,
    expression: Option<&Expression>,
    ctx: &mut Ctx,
) -> Fragment {
    let left_frag = crate::sparql::sqlgen::translate_pattern(left, ctx);
    let mut right_frag = crate::sparql::sqlgen::translate_pattern(right, ctx);

    if let Some(expr) = expression
        && let Some(cond) = translate_expr(expr, &right_frag.bindings, ctx) {
            right_frag.conditions.push(cond);
        }

    let shared_vars: Vec<String> = left_frag
        .bindings
        .keys()
        .filter(|v| right_frag.bindings.contains_key(*v))
        .cloned()
        .collect();

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
