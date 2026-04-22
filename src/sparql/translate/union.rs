//! UNION and MINUS translators.

use spargebra::algebra::GraphPattern;

use crate::sparql::sqlgen::{Ctx, Fragment};

/// Translate UNION to SQL UNION ALL of two subqueries.
pub(crate) fn translate_union(
    left: &GraphPattern,
    right: &GraphPattern,
    ctx: &mut Ctx,
) -> Fragment {
    let left_frag = crate::sparql::sqlgen::translate_pattern(left, ctx);
    let right_frag = crate::sparql::sqlgen::translate_pattern(right, ctx);

    let include_g = ctx.variable_graph;

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
    let union_subquery = format!("(({left_sql}) UNION ALL ({right_sql}))");

    let mut frag = Fragment::empty();
    frag.from_items.push((alias.clone(), union_subquery));
    for v in &all_vars {
        frag.bindings.insert(v.clone(), format!("{alias}._u_{v}"));
    }
    frag
}

/// Translate MINUS to SQL NOT EXISTS with SPARQL-correct null-aware compatibility.
pub(crate) fn translate_minus(
    left: &GraphPattern,
    right: &GraphPattern,
    ctx: &mut Ctx,
) -> Fragment {
    let left_frag = crate::sparql::sqlgen::translate_pattern(left, ctx);
    let right_frag = crate::sparql::sqlgen::translate_pattern(right, ctx);

    let mut shared_vars: Vec<String> = left_frag
        .bindings
        .keys()
        .filter(|v| right_frag.bindings.contains_key(*v))
        .cloned()
        .collect();
    shared_vars.sort();

    let alias = ctx.next_alias();

    if shared_vars.is_empty() {
        return left_frag;
    }

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

    let any_bound: String = shared_vars
        .iter()
        .map(|v| format!("(_lminus._m_{v} IS NOT NULL AND _rminus._m_{v} IS NOT NULL)"))
        .collect::<Vec<_>>()
        .join(" OR ");
    let all_compatible: String = shared_vars.iter()
        .map(|v| format!("(_lminus._m_{v} IS NULL OR _rminus._m_{v} IS NULL OR _lminus._m_{v} = _rminus._m_{v})"))
        .collect::<Vec<_>>().join(" AND ");

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
