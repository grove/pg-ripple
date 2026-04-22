//! Slice (LIMIT/OFFSET with nested subquery) translator.

use spargebra::algebra::GraphPattern;

use crate::sparql::sqlgen::{Ctx, Fragment};
use crate::sparql::translate::filter::{extract_modifiers, translate_order_by};

/// Translate a Slice node (nested LIMIT/OFFSET subquery).
pub(crate) fn translate_slice(pattern: &GraphPattern, ctx: &mut Ctx) -> Fragment {
    let mods = extract_modifiers(pattern);
    let inner_frag = crate::sparql::sqlgen::translate_pattern(mods.pattern, ctx);

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
