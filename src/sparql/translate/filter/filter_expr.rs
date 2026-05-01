//! SPARQL expression compilation — translates SPARQL `Expression` AST nodes into
//! SQL fragments (filter predicates and value expressions).
//!
//! See [`filter_dispatch`](super::filter_dispatch) for the pattern dispatch utilities
//! (modifier extraction, ORDER BY, VALUES, BIND).

use std::collections::HashMap;

use spargebra::algebra::{Expression, Function, GraphPattern};

use super::filter_dispatch::literal_lexical_value;
use crate::sparql::expr;
use crate::sparql::sqlgen::{Ctx, Fragment};

// ─── BIND (Extend) translator ─────────────────────────────────────────────────

pub(crate) fn translate_extend(
    inner: &GraphPattern,
    variable: &spargebra::term::Variable,
    expression: &Expression,
    ctx: &mut Ctx,
) -> Fragment {
    let mut frag = crate::sparql::sqlgen::translate_pattern(inner, ctx);
    let sql_expr = translate_expr_value(expression, &frag.bindings, ctx);
    if let Some(expr_sql) = sql_expr {
        frag.bindings.insert(variable.as_str().to_owned(), expr_sql);
    } else if matches!(
        expression,
        Expression::Equal(_, _)
            | Expression::Greater(_, _)
            | Expression::GreaterOrEqual(_, _)
            | Expression::Less(_, _)
            | Expression::LessOrEqual(_, _)
            | Expression::SameTerm(_, _)
            | Expression::And(_, _)
            | Expression::Or(_, _)
            | Expression::Not(_)
            | Expression::Bound(_)
    ) && let Some(bool_sql) = translate_expr(expression, &frag.bindings, ctx)
    {
        let encoded = format!(
            "CASE WHEN ({bool_sql}) IS NULL THEN NULL::bigint \
                 WHEN ({bool_sql}) THEN -9151314442816847871::bigint \
                 ELSE -9151314442816847872::bigint END"
        );
        frag.bindings.insert(variable.as_str().to_owned(), encoded);
    }
    let is_from_numeric_var = if let Expression::Variable(src_var) = expression {
        ctx.raw_numeric_vars.contains(src_var.as_str())
    } else {
        false
    };
    let is_from_numeric_fn = if let Expression::FunctionCall(func, _) = expression {
        expr::is_numeric_function(func)
    } else {
        false
    };
    if is_from_numeric_var || is_from_numeric_fn {
        ctx.raw_numeric_vars.insert(variable.as_str().to_owned());
    }
    let is_from_text_var = if let Expression::Variable(src_var) = expression {
        ctx.raw_text_vars.contains(src_var.as_str())
    } else {
        false
    };
    if is_from_text_var {
        ctx.raw_text_vars.insert(variable.as_str().to_owned());
    }
    if matches!(expression, Expression::FunctionCall(Function::StrUuid, _)) {
        ctx.raw_text_vars.insert(variable.as_str().to_owned());
    }
    let is_from_iri_var = if let Expression::Variable(src_var) = expression {
        ctx.raw_iri_vars.contains(src_var.as_str())
    } else {
        false
    };
    if is_from_iri_var || matches!(expression, Expression::FunctionCall(Function::Uuid, _)) {
        ctx.raw_iri_vars.insert(variable.as_str().to_owned());
    }
    let is_from_double_var = if let Expression::Variable(src_var) = expression {
        ctx.raw_double_vars.contains(src_var.as_str())
    } else {
        false
    };
    if is_from_double_var || matches!(expression, Expression::FunctionCall(Function::Rand, _)) {
        ctx.raw_double_vars.insert(variable.as_str().to_owned());
    }
    frag
}

// ─── Expression translator ───────────────────────────────────────────────────

fn translate_function_call_filter(
    func: &Function,
    args: &[Expression],
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<String> {
    if let Some(sql) = expr::translate_function_filter(func, args, bindings, ctx) {
        return Some(sql);
    }
    let mut is_numeric = false;
    if let Some(val_sql) =
        expr::translate_function_value(func, args, bindings, ctx, &mut is_numeric)
    {
        return Some(format!("({val_sql} IS NOT NULL)"));
    }
    // FILTER-STRICT-01 (v0.81.0): honour both sparql_strict and strict_sparql_filters.
    // strict_sparql_filters specifically targets unknown built-in function names.
    let strict = crate::SPARQL_STRICT.get() || crate::STRICT_SPARQL_FILTERS.get();
    if strict {
        pgrx::error!(
            "SPARQL function {} is not supported (PT422); \
             set pg_ripple.sparql_strict = off and pg_ripple.strict_sparql_filters = off to warn-and-skip instead",
            expr::function_name(func)
        );
    } else {
        pgrx::warning!(
            "SPARQL function {} is not yet supported — FILTER predicate dropped \
             (set pg_ripple.sparql_strict = on or pg_ripple.strict_sparql_filters = on to raise an error instead)",
            expr::function_name(func)
        );
        None
    }
}

pub(crate) fn translate_expr(
    expr_in: &Expression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<String> {
    match expr_in {
        Expression::Variable(v) => {
            let col = bindings.get(v.as_str())?;
            Some(format!("({col} IS NOT NULL)"))
        }

        Expression::Literal(lit) => {
            let dt = lit.datatype();
            if dt.as_str() == "http://www.w3.org/2001/XMLSchema#boolean" {
                match lit.value() {
                    "true" | "1" => Some("TRUE".to_owned()),
                    _ => Some("FALSE".to_owned()),
                }
            } else {
                let val_sql = translate_expr_value(expr_in, bindings, ctx)?;
                Some(format!(
                    "({val_sql} IS NOT NULL AND {val_sql} NOT IN \
                     (-9151314442816847872, -9187343239835811840))"
                ))
            }
        }

        Expression::Equal(a, b) => {
            let (la, ra) = translate_comparison_sides(a, b, bindings, ctx)?;
            Some(format!("({la} = {ra})"))
        }
        Expression::SameTerm(a, b) => {
            let (la, ra) = translate_comparison_sides(a, b, bindings, ctx)?;
            Some(format!("({la} = {ra})"))
        }
        Expression::Greater(a, b) => {
            let (la, ra) = translate_comparison_sides(a, b, bindings, ctx)?;
            Some(format!("({la} > {ra})"))
        }
        Expression::GreaterOrEqual(a, b) => {
            let (la, ra) = translate_comparison_sides(a, b, bindings, ctx)?;
            Some(format!("({la} >= {ra})"))
        }
        Expression::Less(a, b) => {
            let (la, ra) = translate_comparison_sides(a, b, bindings, ctx)?;
            Some(format!("({la} < {ra})"))
        }
        Expression::LessOrEqual(a, b) => {
            let (la, ra) = translate_comparison_sides(a, b, bindings, ctx)?;
            Some(format!("({la} <= {ra})"))
        }

        Expression::And(a, b) => {
            let la = translate_expr(a, bindings, ctx)?;
            let ra = translate_expr(b, bindings, ctx)?;
            Some(format!("({la} AND {ra})"))
        }
        Expression::Or(a, b) => {
            let la = translate_expr(a, bindings, ctx)?;
            let ra = translate_expr(b, bindings, ctx)?;
            Some(format!("({la} OR {ra})"))
        }
        Expression::Not(inner) => {
            let c = translate_expr(inner, bindings, ctx)?;
            Some(format!("(NOT {c})"))
        }

        Expression::Bound(v) => {
            let col = bindings.get(v.as_str())?;
            Some(format!("({col} IS NOT NULL)"))
        }

        Expression::If(cond, then_expr, else_expr) => {
            let then_sql =
                translate_expr(then_expr, bindings, ctx).unwrap_or_else(|| "FALSE".to_owned());
            let else_sql =
                translate_expr(else_expr, bindings, ctx).unwrap_or_else(|| "FALSE".to_owned());
            if let Some(cond_val) = translate_expr_value(cond, bindings, ctx) {
                Some(format!(
                    "CASE WHEN ({cond_val}) IS NULL \
                          OR ({cond_val}) IN (-9151314442816847872::bigint, -9187343239835811840::bigint) \
                     THEN ({else_sql}) ELSE ({then_sql}) END"
                ))
            } else {
                let cond_sql = translate_expr(cond, bindings, ctx)?;
                Some(format!(
                    "CASE WHEN {cond_sql} THEN ({then_sql}) ELSE ({else_sql}) END"
                ))
            }
        }
        Expression::Coalesce(exprs) => {
            let parts: Vec<String> = exprs
                .iter()
                .filter_map(|e| translate_expr_value(e, bindings, ctx))
                .collect();
            if parts.is_empty() {
                Some("NULL::bigint".to_owned())
            } else {
                Some(format!("(COALESCE({}) IS NOT NULL)", parts.join(", ")))
            }
        }

        Expression::Add(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_arith("+", &la, &ra))
        }
        Expression::Subtract(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_arith("-", &la, &ra))
        }
        Expression::Multiply(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_arith("*", &la, &ra))
        }
        Expression::Divide(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_divide(&la, &ra))
        }
        Expression::UnaryPlus(inner) => translate_expr_value(inner, bindings, ctx),
        Expression::UnaryMinus(inner) => {
            let sql = translate_expr_value(inner, bindings, ctx)?;
            Some(format!("(-({sql}))"))
        }

        Expression::In(var, values) => {
            let col = translate_expr_value(var, bindings, ctx)?;
            let ids: Vec<_> = values
                .iter()
                .filter_map(|v| translate_expr_value(v, bindings, ctx))
                .collect();
            if ids.is_empty() {
                Some("FALSE".to_owned())
            } else {
                Some(format!("({col} IN ({}))", ids.join(", ")))
            }
        }

        Expression::FunctionCall(Function::Contains, args) if args.len() >= 2 => {
            translate_function_call_filter(&Function::Contains, args, bindings, ctx)
        }
        Expression::FunctionCall(Function::StrStarts, args) if args.len() >= 2 => {
            translate_function_call_filter(&Function::StrStarts, args, bindings, ctx)
        }
        Expression::FunctionCall(Function::StrEnds, args) if args.len() >= 2 => {
            translate_function_call_filter(&Function::StrEnds, args, bindings, ctx)
        }
        Expression::FunctionCall(Function::Regex, args) if args.len() >= 2 => {
            translate_function_call_filter(&Function::Regex, args, bindings, ctx)
        }
        Expression::FunctionCall(func, args) => {
            translate_function_call_filter(func, args, bindings, ctx)
        }

        Expression::Exists(pattern) => {
            let inner_frag = crate::sparql::sqlgen::translate_pattern(pattern, ctx);
            let mut all_conditions = inner_frag.conditions.clone();
            for (var, inner_col) in &inner_frag.bindings {
                if let Some(outer_col) = bindings.get(var.as_str()) {
                    all_conditions.push(format!("{inner_col} = {outer_col}"));
                }
            }
            let where_clause = if all_conditions.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", all_conditions.join(" AND "))
            };
            let from_clause = inner_frag.build_from();
            Some(format!(
                "(EXISTS (SELECT 1 FROM {from_clause} {where_clause}))"
            ))
        }

        _ => {
            let strict = crate::SPARQL_STRICT.get();
            if strict {
                pgrx::error!(
                    "unsupported SPARQL expression type in FILTER; \
                     set pg_ripple.sparql_strict = off to warn-and-skip instead"
                );
            } else {
                pgrx::warning!(
                    "unsupported SPARQL expression in FILTER — predicate dropped \
                     (set pg_ripple.sparql_strict = on to raise an error instead)"
                );
                None
            }
        }
    }
}

// v0.56.0 dead-code audit (A-6): expr_as_text_sql is a utility function
// used for text-comparison filters; suppress until wired into all filter paths.
#[allow(dead_code)]
pub(crate) fn expr_as_text_sql(
    expr_in: &Expression,
    bindings: &HashMap<String, String>,
) -> Option<String> {
    match expr_in {
        Expression::Variable(v) => {
            let col = bindings.get(v.as_str())?;
            Some(format!(
                "(SELECT _dict.value FROM _pg_ripple.dictionary _dict WHERE _dict.id = {col})"
            ))
        }
        Expression::Literal(lit) => {
            let val = lit.value();
            let escaped = val.replace('\'', "''");
            Some(format!("'{escaped}'"))
        }
        _ => None,
    }
}

pub(crate) fn translate_expr_value(
    expr_in: &Expression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<String> {
    match expr_in {
        Expression::Variable(v) => Some(bindings.get(v.as_str())?.clone()),
        Expression::NamedNode(nn) => {
            if let Some(id) = ctx.encode_iri(nn.as_str()) {
                return Some(id.to_string());
            }
            let iri = nn.as_str().replace('\'', "''");
            Some(format!(
                "(SELECT d.id FROM _pg_ripple.dictionary d WHERE d.value = '{iri}' AND d.kind = 0 LIMIT 1)"
            ))
        }
        Expression::Literal(lit) => {
            let id = ctx.encode_literal(lit);
            Some(id.to_string())
        }
        Expression::If(cond, then_expr, else_expr) => {
            let then_sql = translate_expr_value(then_expr, bindings, ctx)?;
            let else_sql = translate_expr_value(else_expr, bindings, ctx)
                .unwrap_or_else(|| "NULL::bigint".to_owned());
            let is_bool_pred = matches!(
                cond.as_ref(),
                Expression::FunctionCall(
                    spargebra::algebra::Function::IsBlank
                        | spargebra::algebra::Function::IsIri
                        | spargebra::algebra::Function::IsLiteral
                        | spargebra::algebra::Function::IsNumeric,
                    _
                )
            );
            if is_bool_pred && let Some(cond_sql) = translate_expr(cond, bindings, ctx) {
                return Some(format!(
                    "CASE WHEN ({cond_sql}) THEN ({then_sql}) ELSE ({else_sql}) END"
                ));
            }
            match cond.as_ref() {
                Expression::Variable(_)
                | Expression::Add(_, _)
                | Expression::Subtract(_, _)
                | Expression::Multiply(_, _)
                | Expression::Divide(_, _)
                | Expression::UnaryMinus(_)
                | Expression::UnaryPlus(_) => {
                    translate_expr_value(cond, bindings, ctx).map(|cond_val| format!(
                        "CASE WHEN ({cond_val}) IS NULL THEN NULL::bigint \
                         WHEN ({cond_val}) IN (-9151314442816847872::bigint, -9187343239835811840::bigint) THEN ({else_sql}) \
                         ELSE ({then_sql}) END"
                    ))
                }
                _ => {
                    if let Some(cond_sql) = translate_expr(cond, bindings, ctx) {
                        Some(format!(
                            "CASE WHEN ({cond_sql}) THEN ({then_sql}) ELSE ({else_sql}) END"
                        ))
                    } else {
                        translate_expr_value(cond, bindings, ctx).map(|cond_val| format!(
                            "CASE WHEN ({cond_val}) IS NULL THEN NULL::bigint \
                             WHEN ({cond_val}) IN (-9151314442816847872::bigint, -9187343239835811840::bigint) THEN ({else_sql}) \
                             ELSE ({then_sql}) END"
                        ))
                    }
                }
            }
        }
        Expression::Coalesce(exprs) => {
            let parts: Vec<String> = exprs
                .iter()
                .filter_map(|e| translate_expr_value(e, bindings, ctx))
                .collect();
            if parts.is_empty() {
                Some("NULL::bigint".to_owned())
            } else {
                Some(format!("COALESCE({})", parts.join(", ")))
            }
        }
        Expression::FunctionCall(func, args) => {
            let mut is_numeric = false;
            let result =
                expr::translate_function_value(func, args, bindings, ctx, &mut is_numeric)?;
            Some(result)
        }
        Expression::Add(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_arith("+", &la, &ra))
        }
        Expression::Subtract(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_arith("-", &la, &ra))
        }
        Expression::Multiply(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_arith("*", &la, &ra))
        }
        Expression::Divide(a, b) => {
            let la = translate_expr_value(a, bindings, ctx)?;
            let ra = translate_expr_value(b, bindings, ctx)?;
            Some(rdf_numeric_divide(&la, &ra))
        }
        Expression::UnaryPlus(inner) => translate_expr_value(inner, bindings, ctx),
        Expression::UnaryMinus(inner) => {
            let sql = translate_expr_value(inner, bindings, ctx)?;
            Some(inline_int_negate(&sql))
        }
        _ => None,
    }
}

// ─── Inline-integer arithmetic helpers ───────────────────────────────────────

fn inline_int_extract(sql: &str) -> String {
    format!("(({sql} & 72057594037927935::bigint) - 36028797018963968::bigint)")
}

fn inline_int_pack(sql: &str) -> String {
    format!(
        "((~(9223372036854775807::bigint)) | \
         (({sql} + 36028797018963968::bigint) & 72057594037927935::bigint))"
    )
}

// v0.56.0 dead-code audit (A-6): inline_int_arith and inline_int_divide were
// dead (never called). Removed. inline_int_negate below remains live.

fn inline_int_negate(sql: &str) -> String {
    let extract = inline_int_extract(sql);
    format!(
        "CASE WHEN ({sql}) >= 0 THEN NULL::bigint \
         ELSE {packed} END",
        packed = inline_int_pack(&format!("(-({extract}))")),
    )
}

fn rdf_numeric_arith(op: &str, la: &str, ra: &str) -> String {
    let extract_a = inline_int_extract(la);
    let extract_b = inline_int_extract(ra);

    let decode_a = format!(
        "CASE WHEN ({la}) IS NULL THEN NULL \
         WHEN ({la}) < 0 THEN ({extract_a})::numeric \
         ELSE pg_ripple.decode_numeric_spi(({la})) END"
    );
    let decode_b = format!(
        "CASE WHEN ({ra}) IS NULL THEN NULL \
         WHEN ({ra}) < 0 THEN ({extract_b})::numeric \
         ELSE pg_ripple.decode_numeric_spi(({ra})) END"
    );

    let tc_a = format!(
        "CASE WHEN ({la}) < 0 THEN 0 \
         ELSE COALESCE((SELECT CASE \
           WHEN d.datatype IN ('http://www.w3.org/2001/XMLSchema#double',\
                               'http://www.w3.org/2001/XMLSchema#float') THEN 2 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#integer' THEN 0 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#decimal' THEN 1 \
           ELSE -1 END FROM _pg_ripple.dictionary d WHERE d.id = ({la}) LIMIT 1), -1) END"
    );
    let tc_b = format!(
        "CASE WHEN ({ra}) < 0 THEN 0 \
         ELSE COALESCE((SELECT CASE \
           WHEN d.datatype IN ('http://www.w3.org/2001/XMLSchema#double',\
                               'http://www.w3.org/2001/XMLSchema#float') THEN 2 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#integer' THEN 0 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#decimal' THEN 1 \
           ELSE -1 END FROM _pg_ripple.dictionary d WHERE d.id = ({ra}) LIMIT 1), -1) END"
    );

    let xsd_int = "http://www.w3.org/2001/XMLSchema#integer";
    let xsd_dec = "http://www.w3.org/2001/XMLSchema#decimal";
    let xsd_dbl = "http://www.w3.org/2001/XMLSchema#double";

    let fast_int = format!(
        "CASE WHEN ({la}) >= 0 OR ({ra}) >= 0 THEN NULL::bigint \
         ELSE {packed} END",
        packed = inline_int_pack(&format!("(({extract_a}) {op} ({extract_b}))")),
    );

    format!(
        "CASE WHEN ({la}) IS NULL OR ({ra}) IS NULL THEN NULL::bigint \
         WHEN ({la}) < 0 AND ({ra}) < 0 THEN ({fast_int}) \
         ELSE (SELECT pg_ripple.encode_typed_literal( \
                   CASE \
                     WHEN _tc < 0 THEN NULL \
                     WHEN _tc >= 2 THEN pg_ripple.xsd_double_fmt(_result::float8::text) \
                     WHEN _tc = 1 THEN CASE WHEN _result LIKE '%.%' THEN trim_scale(_result::numeric)::text ELSE _result || '.0' END \
                     ELSE _result \
                   END, \
                   CASE \
                     WHEN _tc < 0 THEN 'http://www.w3.org/2001/XMLSchema#error' \
                     WHEN _tc >= 2 THEN '{xsd_dbl}' \
                     WHEN _tc = 1 THEN '{xsd_dec}' \
                     ELSE '{xsd_int}' \
                   END \
               ) \
               FROM (SELECT \
                   GREATEST(({tc_a}), ({tc_b})) AS _tc, \
                   (({decode_a}) {op} ({decode_b}))::text AS _result \
               ) _arith \
               WHERE _tc >= 0 AND _result IS NOT NULL) \
         END"
    )
}

fn rdf_numeric_divide(la: &str, ra: &str) -> String {
    let extract_a = inline_int_extract(la);
    let extract_b = inline_int_extract(ra);

    let decode_a = format!(
        "CASE WHEN ({la}) IS NULL THEN NULL \
         WHEN ({la}) < 0 THEN ({extract_a})::numeric \
         ELSE pg_ripple.decode_numeric_spi(({la})) END"
    );
    let decode_b = format!(
        "CASE WHEN ({ra}) IS NULL THEN NULL \
         WHEN ({ra}) < 0 THEN ({extract_b})::numeric \
         ELSE pg_ripple.decode_numeric_spi(({ra})) END"
    );

    let tc_a = format!(
        "CASE WHEN ({la}) < 0 THEN 0 \
         ELSE COALESCE((SELECT CASE \
           WHEN d.datatype IN ('http://www.w3.org/2001/XMLSchema#double',\
                               'http://www.w3.org/2001/XMLSchema#float') THEN 2 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#integer' THEN 0 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#decimal' THEN 1 \
           ELSE -1 END FROM _pg_ripple.dictionary d WHERE d.id = ({la}) LIMIT 1), -1) END"
    );
    let tc_b = format!(
        "CASE WHEN ({ra}) < 0 THEN 0 \
         ELSE COALESCE((SELECT CASE \
           WHEN d.datatype IN ('http://www.w3.org/2001/XMLSchema#double',\
                               'http://www.w3.org/2001/XMLSchema#float') THEN 2 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#integer' THEN 0 \
           WHEN d.datatype = 'http://www.w3.org/2001/XMLSchema#decimal' THEN 1 \
           ELSE -1 END FROM _pg_ripple.dictionary d WHERE d.id = ({ra}) LIMIT 1), -1) END"
    );

    let xsd_dec = "http://www.w3.org/2001/XMLSchema#decimal";
    let xsd_dbl = "http://www.w3.org/2001/XMLSchema#double";

    format!(
        "CASE WHEN ({la}) IS NULL OR ({ra}) IS NULL THEN NULL::bigint \
         ELSE (SELECT pg_ripple.encode_typed_literal( \
                   CASE \
                     WHEN _tc < 0 OR _denominator IS NULL OR _denominator = 0 THEN NULL \
                     WHEN _tc >= 2 THEN pg_ripple.xsd_double_fmt((_numerator / _denominator)::float8::text) \
                     ELSE CASE WHEN _result LIKE '%.%' THEN trim_scale(_result::numeric)::text \
                               ELSE _result || '.0' END \
                   END, \
                   CASE \
                     WHEN _tc >= 2 THEN '{xsd_dbl}' \
                     ELSE '{xsd_dec}' \
                   END \
               ) \
               FROM (SELECT \
                   GREATEST(({tc_a}), ({tc_b}), 1) AS _tc, \
                   ({decode_a}) AS _numerator, \
                   ({decode_b}) AS _denominator, \
                   CASE WHEN ({decode_b}) != 0 \
                        THEN trim_scale(({decode_a}) / NULLIF({decode_b}, 0))::text \
                        ELSE NULL END AS _result \
               ) _div \
               WHERE _tc >= 0) \
         END"
    )
}

pub(crate) fn translate_expr_value_raw(
    expr_in: &Expression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<String> {
    match expr_in {
        Expression::Variable(v) => Some(bindings.get(v.as_str())?.clone()),
        Expression::NamedNode(nn) => {
            let id = ctx.encode_iri(nn.as_str())?;
            Some(id.to_string())
        }
        Expression::Literal(lit) => {
            let dt = lit.datatype().as_str();
            if dt == "http://www.w3.org/2001/XMLSchema#integer"
                || dt == "http://www.w3.org/2001/XMLSchema#long"
                || dt == "http://www.w3.org/2001/XMLSchema#int"
                || dt == "http://www.w3.org/2001/XMLSchema#short"
                || dt == "http://www.w3.org/2001/XMLSchema#decimal"
                || dt == "http://www.w3.org/2001/XMLSchema#float"
                || dt == "http://www.w3.org/2001/XMLSchema#double"
            {
                Some(lit.value().to_owned())
            } else {
                let id = ctx.encode_literal(lit);
                Some(id.to_string())
            }
        }
        Expression::FunctionCall(func, args) => {
            let mut is_numeric = false;
            let sql = expr::translate_function_value(func, args, bindings, ctx, &mut is_numeric)?;
            if is_numeric { Some(sql) } else { None }
        }
        _ => None,
    }
}

pub(crate) fn expr_is_raw_numeric(expr_in: &Expression, ctx: &Ctx) -> bool {
    match expr_in {
        Expression::Variable(v) => {
            ctx.raw_numeric_vars.contains(v.as_str()) || ctx.raw_double_vars.contains(v.as_str())
        }
        Expression::FunctionCall(func, _) => expr::is_numeric_function(func),
        _ => false,
    }
}

pub(crate) fn expr_is_raw_text(expr_in: &Expression, ctx: &Ctx) -> bool {
    if let Expression::Variable(v) = expr_in {
        ctx.raw_text_vars.contains(v.as_str())
    } else {
        false
    }
}

pub(crate) fn translate_comparison_sides(
    a: &Expression,
    b: &Expression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<(String, String)> {
    if expr_is_raw_text(a, ctx) {
        let la = translate_expr_value(a, bindings, ctx)?;
        let ra = match b {
            Expression::Literal(lit) => literal_lexical_value(lit),
            _ => return None,
        };
        return Some((la, ra));
    }
    if expr_is_raw_text(b, ctx) {
        let la = match a {
            Expression::Literal(lit) => literal_lexical_value(lit),
            _ => return None,
        };
        let ra = translate_expr_value(b, bindings, ctx)?;
        return Some((la, ra));
    }
    if expr_is_raw_numeric(a, ctx) || expr_is_raw_numeric(b, ctx) {
        let la = translate_expr_value_raw(a, bindings, ctx)?;
        let ra = translate_expr_value_raw(b, bindings, ctx)?;
        Some((la, ra))
    } else {
        let la = translate_expr_value(a, bindings, ctx)?;
        let ra = translate_expr_value(b, bindings, ctx)?;
        Some((la, ra))
    }
}
