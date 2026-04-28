//! GROUP BY / Aggregate translator.

use std::collections::HashMap;

use spargebra::algebra::{AggregateExpression, AggregateFunction, Expression, GraphPattern};

use crate::sparql::sqlgen::{Ctx, Fragment};
use crate::sparql::translate::filter::{sanitize_sql_ident, translate_expr, translate_expr_value};

pub(crate) fn translate_group(
    inner: &GraphPattern,
    group_vars: &[spargebra::term::Variable],
    aggregates: &[(spargebra::term::Variable, AggregateExpression)],
    having: Option<&Expression>,
    ctx: &mut Ctx,
) -> Fragment {
    let inner_frag = crate::sparql::sqlgen::translate_pattern(inner, ctx);

    let variable_graph_g: Option<String> = if ctx.variable_graph {
        inner_frag
            .from_items
            .first()
            .map(|(alias, _)| format!("{alias}.g"))
    } else {
        None
    };

    let mut inner_select_parts: Vec<String> = inner_frag
        .bindings
        .iter()
        .map(|(v, col)| format!("{col} AS _gi_{}", sanitize_sql_ident(v)))
        .collect();
    if let Some(ref gcol) = variable_graph_g {
        inner_select_parts.push(format!("{gcol} AS _gi__g"));
    }
    let inner_select = if inner_select_parts.is_empty() {
        "1 AS _gi_dummy".to_owned()
    } else {
        inner_select_parts.join(", ")
    };
    let inner_sql = format!(
        "SELECT {inner_select} FROM {} {}",
        inner_frag.build_from(),
        inner_frag.build_where()
    );

    let inner_alias: HashMap<String, String> = inner_frag
        .bindings
        .keys()
        .map(|v| (v.clone(), format!("_gi_{}", sanitize_sql_ident(v))))
        .collect();

    let group_cols: Vec<(String, String)> = group_vars
        .iter()
        .filter_map(|v| {
            inner_alias
                .get(v.as_str())
                .map(|alias| (v.as_str().to_owned(), alias.clone()))
        })
        .collect();

    let mut select_parts: Vec<String> = group_cols
        .iter()
        .map(|(v, alias)| format!("{alias} AS _g_{}", sanitize_sql_ident(v)))
        .collect();
    if variable_graph_g.is_some() {
        select_parts.push("_gi__g AS g".to_string());
    }

    let mut agg_bindings: Vec<(String, String)> = Vec::new();
    let mut text_agg_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut encoded_agg_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut raw_agg_for_having: Vec<(String, String)> = Vec::new();
    for (agg_var, agg_expr) in aggregates {
        let (encoded_sql, raw_sql) = translate_aggregate(agg_expr, &inner_alias, ctx);
        let vname = agg_var.as_str().to_owned();
        let is_group_concat = matches!(
            agg_expr,
            AggregateExpression::FunctionCall {
                name: AggregateFunction::GroupConcat { .. },
                ..
            }
        );
        let is_encoded = matches!(
            agg_expr,
            AggregateExpression::FunctionCall {
                name: AggregateFunction::Sum
                    | AggregateFunction::Avg
                    | AggregateFunction::Min
                    | AggregateFunction::Max
                    | AggregateFunction::Sample,
                ..
            }
        ) || matches!(
            agg_expr,
            AggregateExpression::FunctionCall { name: AggregateFunction::Custom(n), .. }
            if matches!(n.as_str(), "urn:arq:median" | "urn:arq:mode")
        );
        if is_group_concat {
            text_agg_vars.insert(vname.clone());
        }
        if is_encoded {
            encoded_agg_vars.insert(vname.clone());
        }
        select_parts.push(format!(
            "{encoded_sql} AS _g_{}",
            sanitize_sql_ident(&vname)
        ));
        agg_bindings.push((vname.clone(), encoded_sql));
        raw_agg_for_having.push((vname, raw_sql));
    }

    let group_by_clause = if group_cols.is_empty() && variable_graph_g.is_none() {
        String::new()
    } else {
        let mut gb_cols: Vec<String> = group_cols
            .iter()
            .map(|(_, alias)| alias.to_string())
            .collect();
        if variable_graph_g.is_some() {
            gb_cols.push("_gi__g".to_string());
        }
        format!("GROUP BY {}", gb_cols.join(", "))
    };

    let having_clause = if let Some(having_expr) = having {
        let mut having_bindings = inner_alias.clone();
        for (vname, raw_sql) in &raw_agg_for_having {
            having_bindings.insert(vname.clone(), raw_sql.clone());
        }
        for (vname, _) in &raw_agg_for_having {
            ctx.raw_numeric_vars.insert(vname.clone());
        }
        let result = translate_expr(having_expr, &having_bindings, ctx)
            .map(|c| format!("HAVING {c}"))
            .unwrap_or_default();
        for (vname, _) in &raw_agg_for_having {
            ctx.raw_numeric_vars.remove(vname.as_str());
        }
        result
    } else {
        String::new()
    };

    let select_list = if select_parts.is_empty() {
        "COUNT(*) AS _g__count".to_owned()
    } else {
        select_parts.join(", ")
    };

    let group_sql = if variable_graph_g.is_some() && group_cols.is_empty() {
        let inner_agg = format!(
            "SELECT {select_list} FROM ({inner_sql}) AS _grp_inner {group_by_clause} {having_clause}"
        );
        let inner_agg_alias = ctx.next_alias();
        let outer_cols: Vec<String> = agg_bindings
            .iter()
            .map(|(vname, _)| {
                let col = format!("{inner_agg_alias}._g_{}", sanitize_sql_ident(vname));
                if encoded_agg_vars.contains(vname) {
                    format!("{col} AS _g_{}", sanitize_sql_ident(vname))
                } else {
                    format!("COALESCE({col}, 0) AS _g_{}", sanitize_sql_ident(vname))
                }
            })
            .collect();
        let outer_select = if outer_cols.is_empty() {
            "0 AS _g__count".to_owned()
        } else {
            outer_cols.join(", ")
        };
        format!(
            "(SELECT ng.graph_id AS g, {outer_select} \
             FROM _pg_ripple.named_graphs ng \
             LEFT JOIN ({inner_agg}) AS {inner_agg_alias} \
             ON {inner_agg_alias}.g = ng.graph_id \
             WHERE ng.graph_id <> 0)"
        )
    } else {
        format!(
            "(SELECT {select_list} FROM ({inner_sql}) AS _grp_inner {group_by_clause} {having_clause})"
        )
    };

    let alias = ctx.next_alias();
    let mut frag = Fragment::empty();
    frag.from_items.push((alias.clone(), group_sql));

    for (v, _) in &group_cols {
        frag.bindings
            .insert(v.clone(), format!("{alias}._g_{}", sanitize_sql_ident(v)));
    }
    for (vname, _) in &agg_bindings {
        frag.bindings.insert(
            vname.clone(),
            format!("{alias}._g_{}", sanitize_sql_ident(vname)),
        );
        if text_agg_vars.contains(vname) {
            ctx.raw_text_vars.insert(vname.clone());
        } else if !encoded_agg_vars.contains(vname) {
            ctx.raw_numeric_vars.insert(vname.clone());
        }
    }

    frag
}

fn translate_aggregate(
    agg: &AggregateExpression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> (String, String) {
    match agg {
        AggregateExpression::CountSolutions { distinct } => {
            let s = if *distinct {
                if bindings.is_empty() {
                    "COUNT(*)".to_owned()
                } else {
                    let cols: Vec<String> = bindings
                        .values()
                        .map(|col| format!("COALESCE({col}::text, '')"))
                        .collect();
                    let concat = cols.join(" || '|' || ");
                    format!("COUNT(DISTINCT ({concat}))")
                }
            } else {
                "COUNT(*)".to_owned()
            };
            (s.clone(), s)
        }
        AggregateExpression::FunctionCall {
            name,
            expr,
            distinct,
        } => {
            let distinct_kw = if *distinct { "DISTINCT " } else { "" };
            let arg = translate_agg_expr(expr, bindings, ctx).unwrap_or_else(|| "NULL".to_owned());
            match name {
                AggregateFunction::Count => {
                    // CITUS-HLL-01 (v0.68.0): when approx_distinct=on and hll is available,
                    // use hll_add_agg(hll_hash_bigint(x)) for scalable COUNT(DISTINCT).
                    let s = if *distinct
                        && crate::gucs::storage::APPROX_DISTINCT.get()
                        && citus_hll_available()
                    {
                        format!("hll_cardinality(hll_add_agg(hll_hash_bigint({arg})))::bigint")
                    } else {
                        format!("COUNT({distinct_kw}{arg})")
                    };
                    (s.clone(), s)
                }
                AggregateFunction::Sum => {
                    let raw_having = rdf_decoded_agg("SUM", distinct_kw, &arg);
                    let enc = rdf_numeric_agg("SUM", distinct_kw, &arg, false);
                    (enc, raw_having)
                }
                AggregateFunction::Avg => {
                    let raw_having = rdf_decoded_agg("AVG", distinct_kw, &arg);
                    let enc = rdf_numeric_agg("AVG", distinct_kw, &arg, true);
                    (enc, raw_having)
                }
                AggregateFunction::Min => {
                    let raw_having = rdf_decoded_agg("MIN", "", &arg);
                    let enc = rdf_minmax_agg(&arg, "ASC");
                    (enc, raw_having)
                }
                AggregateFunction::Max => {
                    let raw_having = rdf_decoded_agg("MAX", "", &arg);
                    let enc = rdf_minmax_agg(&arg, "DESC");
                    (enc, raw_having)
                }
                AggregateFunction::GroupConcat { separator } => {
                    let sep = separator.as_deref().unwrap_or(" ");
                    let decode_expr = format!(
                        "CASE WHEN {arg} < 0 THEN \
                         (({arg} & 72057594037927935::bigint) - 36028797018963968::bigint)::text \
                         ELSE (SELECT d.value FROM _pg_ripple.dictionary d WHERE d.id = {arg} LIMIT 1) \
                         END"
                    );
                    let s = if *distinct {
                        format!(
                            "STRING_AGG(DISTINCT ({decode_expr})::text, {sep_lit} ORDER BY ({decode_expr}))",
                            sep_lit = quote_sql_string(sep)
                        )
                    } else {
                        format!(
                            "STRING_AGG(({decode_expr})::text, {sep_lit} ORDER BY {arg})",
                            sep_lit = quote_sql_string(sep)
                        )
                    };
                    (s.clone(), s)
                }
                AggregateFunction::Sample => {
                    let s = format!("MIN({arg})");
                    (s.clone(), s)
                }
                AggregateFunction::Custom(iri) => match iri.as_str() {
                    "urn:arq:median" => {
                        let decode = format!(
                            "CASE WHEN ({arg}) IS NULL THEN NULL \
                             WHEN ({arg}) < 0 THEN \
                               ((({arg}) & 72057594037927935::bigint) - \
                               36028797018963968::bigint)::numeric \
                             ELSE (SELECT CASE WHEN d.datatype IN (\
                               'http://www.w3.org/2001/XMLSchema#decimal',\
                               'http://www.w3.org/2001/XMLSchema#double',\
                               'http://www.w3.org/2001/XMLSchema#float',\
                               'http://www.w3.org/2001/XMLSchema#integer') \
                               THEN d.value::numeric ELSE NULL END \
                               FROM _pg_ripple.dictionary d WHERE d.id = ({arg}) LIMIT 1) \
                             END"
                        );
                        let s = format!(
                            "pg_ripple.encode_typed_literal(\
                             trim_scale(PERCENTILE_CONT(0.5::numeric) \
                             WITHIN GROUP (ORDER BY {decode}))::text,\
                             'http://www.w3.org/2001/XMLSchema#decimal')"
                        );
                        (s.clone(), s)
                    }
                    "urn:arq:mode" => {
                        let s = format!("MODE() WITHIN GROUP (ORDER BY {arg})");
                        (s.clone(), s)
                    }
                    _ => {
                        let s = format!("MIN({arg})");
                        (s.clone(), s)
                    }
                },
            }
        }
    }
}

fn rdf_decoded_agg(agg_fn: &str, distinct_kw: &str, arg: &str) -> String {
    let decode = format!(
        "CASE WHEN ({arg}) IS NULL THEN NULL \
         WHEN ({arg}) < 0 THEN \
           ((({arg}) & 72057594037927935::bigint) - 36028797018963968::bigint)::numeric \
         ELSE (SELECT CASE WHEN d.datatype IN (\
           'http://www.w3.org/2001/XMLSchema#decimal',\
           'http://www.w3.org/2001/XMLSchema#double',\
           'http://www.w3.org/2001/XMLSchema#float',\
           'http://www.w3.org/2001/XMLSchema#integer') \
           THEN d.value::numeric ELSE NULL END \
           FROM _pg_ripple.dictionary d WHERE d.id = ({arg}) LIMIT 1) END"
    );
    format!("{agg_fn}({distinct_kw}{decode})")
}

fn rdf_numeric_agg(agg_fn: &str, distinct_kw: &str, arg: &str, is_avg: bool) -> String {
    let decode = format!(
        "CASE WHEN ({arg}) IS NULL THEN NULL \
         WHEN ({arg}) < 0 THEN \
           ((({arg}) & 72057594037927935::bigint) - 36028797018963968::bigint)::numeric \
         ELSE (SELECT CASE WHEN d.datatype IN (\
           'http://www.w3.org/2001/XMLSchema#decimal',\
           'http://www.w3.org/2001/XMLSchema#double',\
           'http://www.w3.org/2001/XMLSchema#float',\
           'http://www.w3.org/2001/XMLSchema#integer') \
           THEN d.value::numeric ELSE NULL END \
           FROM _pg_ripple.dictionary d WHERE d.id = ({arg}) LIMIT 1) END"
    );
    let tc = format!(
        "CASE WHEN ({arg}) IS NULL OR ({arg}) < 0 THEN 0 \
         ELSE COALESCE((SELECT CASE \
           WHEN d.datatype IN ('http://www.w3.org/2001/XMLSchema#double',\
                               'http://www.w3.org/2001/XMLSchema#float') THEN 2 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#integer' THEN 0 \
           ELSE 1 END FROM _pg_ripple.dictionary d WHERE d.id = ({arg}) LIMIT 1), 0) END"
    );
    if is_avg {
        format!(
            "CASE WHEN BOOL_OR(({arg}) IS NOT NULL AND ({decode}) IS NULL) THEN NULL \
               WHEN {agg_fn}({distinct_kw}{decode}) IS NULL \
               THEN pg_ripple.encode_typed_literal('0', 'http://www.w3.org/2001/XMLSchema#integer') \
               ELSE pg_ripple.encode_typed_literal(\
                 CASE COALESCE(MAX({tc}), 0) \
                 WHEN 2 THEN pg_ripple.xsd_double_fmt({agg_fn}({distinct_kw}{decode})::text) \
                 ELSE trim_scale({agg_fn}({distinct_kw}{decode}))::text \
                 END, \
                 CASE COALESCE(MAX({tc}), 0) \
                 WHEN 2 THEN 'http://www.w3.org/2001/XMLSchema#double' \
                 ELSE 'http://www.w3.org/2001/XMLSchema#decimal' \
                 END) END"
        )
    } else {
        format!(
            "CASE WHEN BOOL_OR(({arg}) IS NOT NULL AND ({decode}) IS NULL) THEN NULL \
               ELSE pg_ripple.encode_typed_literal(\
               CASE COALESCE(MAX({tc}), 0) \
               WHEN 2 THEN pg_ripple.xsd_double_fmt(SUM({distinct_kw}{decode})::text) \
               WHEN 1 THEN trim_scale(SUM({distinct_kw}{decode}))::text \
               ELSE SUM({distinct_kw}{decode})::bigint::text \
               END, \
               CASE COALESCE(MAX({tc}), 0) \
               WHEN 2 THEN 'http://www.w3.org/2001/XMLSchema#double' \
               WHEN 1 THEN 'http://www.w3.org/2001/XMLSchema#decimal' \
               ELSE 'http://www.w3.org/2001/XMLSchema#integer' \
               END) END"
        )
    }
}

fn rdf_minmax_agg(arg: &str, order: &str) -> String {
    let decode = format!(
        "CASE WHEN ({arg}) < 0 THEN \
           ((({arg}) & 72057594037927935::bigint) - 36028797018963968::bigint)::numeric \
         ELSE (SELECT CASE WHEN d.datatype IN (\
           'http://www.w3.org/2001/XMLSchema#decimal',\
           'http://www.w3.org/2001/XMLSchema#double',\
           'http://www.w3.org/2001/XMLSchema#float',\
           'http://www.w3.org/2001/XMLSchema#integer') \
           THEN d.value::numeric ELSE NULL END \
           FROM _pg_ripple.dictionary d WHERE d.id = ({arg}) LIMIT 1) END"
    );
    format!(
        "CASE WHEN BOOL_OR(({arg}) IS NOT NULL AND ({decode}) IS NULL) THEN NULL \
         ELSE (array_agg(({arg}) ORDER BY ({decode}) {order} NULLS LAST) \
          FILTER (WHERE ({arg}) IS NOT NULL AND ({decode}) IS NOT NULL))[1] \
         END"
    )
}

fn translate_agg_expr(
    expr: &Expression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<String> {
    translate_expr_value(expr, bindings, ctx)
}

pub(crate) fn quote_sql_string(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

// ─── Citus HLL helper (v0.68.0 CITUS-HLL-01) ─────────────────────────────────

/// Return `true` if the `hll` PostgreSQL extension is installed and accessible.
///
/// Used by the aggregate translator to decide whether to use
/// `hll_add_agg(hll_hash_bigint(x))` for approximate COUNT(DISTINCT) when
/// `pg_ripple.approx_distinct = on`.
pub(crate) fn citus_hll_available() -> bool {
    let result = pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS ( \
             SELECT 1 FROM pg_extension WHERE extname = 'hll' \
         )",
    );
    matches!(result, Ok(Some(true)))
}
