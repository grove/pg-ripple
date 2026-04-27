//! Basic Graph Pattern translator.

use std::collections::HashMap;

use spargebra::algebra::GraphPattern;
use spargebra::term::{NamedNodePattern, TermPattern};

use crate::dictionary;
use crate::sparql::sqlgen::{
    Ctx, Fragment, VpSource, build_all_predicates_union, table_expr, vp_source,
};
use crate::sparql::translate::filter::sanitize_sql_ident;

// ─── TermPattern → SQL column ─────────────────────────────────────────────────

pub(crate) fn ground_term_id(term: &TermPattern, ctx: &mut Ctx) -> Option<i64> {
    match term {
        TermPattern::NamedNode(nn) => ctx.encode_iri(nn.as_str()),
        TermPattern::Literal(lit) => Some(ctx.encode_literal(lit)),
        TermPattern::Triple(inner) => {
            let s_id = ground_term_id(&inner.subject, ctx)?;
            let p_id = match &inner.predicate {
                NamedNodePattern::NamedNode(nn) => ctx.encode_iri(nn.as_str())?,
                NamedNodePattern::Variable(_) => return None,
            };
            let o_id = ground_term_id(&inner.object, ctx)?;
            dictionary::lookup_quoted_triple(s_id, p_id, o_id)
        }
        TermPattern::Variable(_) | TermPattern::BlankNode(_) => None,
    }
}

pub(crate) fn ground_term_sql_for_path(term: &TermPattern, ctx: &mut Ctx) -> Option<String> {
    match term {
        TermPattern::NamedNode(nn) => {
            if let Some(id) = ctx.encode_iri(nn.as_str()) {
                Some(id.to_string())
            } else {
                let iri = nn.as_str().replace('\'', "''");
                Some(format!("pg_ripple.encode_term('{iri}', 0::int2)"))
            }
        }
        TermPattern::Literal(lit) => Some(ctx.encode_literal(lit).to_string()),
        TermPattern::Triple(_inner) => ground_term_id(term, ctx).map(|id| id.to_string()),
        TermPattern::Variable(_) | TermPattern::BlankNode(_) => None,
    }
}

pub(crate) fn bind_term(
    alias: &str,
    col: &str,
    term: &TermPattern,
    ctx: &mut Ctx,
    bindings: &mut HashMap<String, String>,
    conditions: &mut Vec<String>,
) {
    let col_expr = format!("{alias}.{col}");
    match term {
        TermPattern::Variable(v) => {
            let vname = v.as_str().to_owned();
            if let Some(existing) = bindings.get(&vname) {
                conditions.push(format!("{col_expr} = {existing}"));
            } else {
                bindings.insert(vname, col_expr);
            }
        }
        TermPattern::NamedNode(nn) => match ctx.encode_iri(nn.as_str()) {
            Some(id) => conditions.push(format!("{col_expr} = {id}")),
            None => conditions.push("FALSE".to_owned()),
        },
        TermPattern::Literal(lit) => {
            let id = ctx.encode_literal(lit);
            conditions.push(format!("{col_expr} = {id}"));
        }
        TermPattern::BlankNode(bnode) => {
            let vname = sanitize_sql_ident(&format!("_bn_{}", bnode));
            if let Some(existing) = bindings.get(&vname) {
                conditions.push(format!("{col_expr} = {existing}"));
            } else {
                bindings.insert(vname, col_expr);
            }
        }
        TermPattern::Triple(inner) => {
            // v0.48.0: SPARQL-star variable-inside-quoted-triple support.
            // If all components are ground, look up the quoted-triple ID directly.
            // Otherwise, generate a JOIN against the dictionary on qt_s/qt_p/qt_o columns
            // to bind the variable components and constrain col_expr to matching IDs.
            match ground_term_id(term, ctx) {
                Some(id) => conditions.push(format!("{col_expr} = {id}")),
                None => {
                    // At least one component is a variable. We need a subquery:
                    //   col_expr IN (SELECT id FROM _pg_ripple.dictionary WHERE kind = 5
                    //                AND qt_s = <s_val> AND qt_p = <p_val> AND qt_o = <o_val>)
                    // where ground components become literal equality conditions
                    // and variable components are bound via dictionary column projections.
                    let dict_alias = sanitize_sql_ident(&format!("_qt_dict_{col}"));

                    // Collect per-component SQL expressions for the subquery conditions.
                    let mut qt_conds: Vec<String> = Vec::new();
                    let mut qt_bindings: Vec<(String, String)> = Vec::new(); // (var_name, col_expr)

                    // Helper: turn a TermPattern component into either a literal i64 or
                    // a "bind variable to dict column" pair.
                    let process_qt_component =
                        |tp: &TermPattern,
                         dict_col: &str,
                         ctx: &mut Ctx,
                         conds: &mut Vec<String>,
                         bnd: &mut Vec<(String, String)>| {
                            match tp {
                                TermPattern::NamedNode(nn) => match ctx.encode_iri(nn.as_str()) {
                                    Some(id) => conds.push(format!("{dict_col} = {id}")),
                                    None => conds.push("FALSE".to_owned()),
                                },
                                TermPattern::Literal(lit) => {
                                    let id = ctx.encode_literal(lit);
                                    conds.push(format!("{dict_col} = {id}"));
                                }
                                TermPattern::Variable(v) => {
                                    bnd.push((v.as_str().to_owned(), dict_col.to_owned()));
                                }
                                TermPattern::BlankNode(bn) => {
                                    let vn = sanitize_sql_ident(&format!("_bn_{bn}"));
                                    bnd.push((vn, dict_col.to_owned()));
                                }
                                TermPattern::Triple(_) => {
                                    // Nested quoted triples: not supported here, treat as no-match.
                                    conds.push("FALSE".to_owned());
                                }
                            }
                        };

                    // Subject component.
                    {
                        let mut conds_tmp = Vec::new();
                        let mut bnd_tmp = Vec::new();
                        let s_col = "qt_s".to_owned();
                        process_qt_component(
                            &inner.subject,
                            &s_col,
                            ctx,
                            &mut conds_tmp,
                            &mut bnd_tmp,
                        );
                        qt_conds.extend(conds_tmp);
                        qt_bindings.extend(bnd_tmp);
                    }

                    // Predicate component (NamedNodePattern, not TermPattern).
                    let p_col = "qt_p".to_owned();
                    match &inner.predicate {
                        spargebra::term::NamedNodePattern::NamedNode(nn) => {
                            match ctx.encode_iri(nn.as_str()) {
                                Some(id) => qt_conds.push(format!("{p_col} = {id}")),
                                None => qt_conds.push("FALSE".to_owned()),
                            }
                        }
                        spargebra::term::NamedNodePattern::Variable(v) => {
                            qt_bindings.push((v.as_str().to_owned(), p_col.clone()));
                        }
                    }

                    // Object component.
                    {
                        let mut conds_tmp = Vec::new();
                        let mut bnd_tmp = Vec::new();
                        let o_col = "qt_o".to_owned();
                        process_qt_component(
                            &inner.object,
                            &o_col,
                            ctx,
                            &mut conds_tmp,
                            &mut bnd_tmp,
                        );
                        qt_conds.extend(conds_tmp);
                        qt_bindings.extend(bnd_tmp);
                    }

                    // Build the JOIN condition: col_expr = dict.id AND kind = 5 AND <qt conditions>.
                    let kind_cond = format!("{dict_alias}.kind = 5");
                    let all_conds: Vec<String> =
                        std::iter::once(kind_cond).chain(qt_conds).collect();
                    let where_clause = all_conds.join(" AND ");

                    // Add the dictionary table to FROM with a JOIN condition.
                    // We add it as: JOIN _pg_ripple.dictionary AS {dict_alias}
                    //               ON {col_expr} = {dict_alias}.id AND {where_clause}
                    let _join_expr = format!("_pg_ripple.dictionary AS {dict_alias}");
                    // Add the FROM item and conditions via the Fragment.
                    // We use the bindings map to add the join + bind variables.
                    // Emit: {col_expr} = {dict_alias}.id
                    conditions.push(format!("{col_expr} = {dict_alias}.id"));
                    conditions.push(where_clause);

                    // Register the dictionary alias as a FROM item.
                    // We add it into the frag directly via bindings/conditions only;
                    // the actual JOIN is emitted as a CROSS JOIN with WHERE conditions
                    // (equivalent, but simpler to generate correctly here).
                    // Note: `frag` is not accessible here; use a dummy approach:
                    // wrap the conditions to create the right filter.
                    // Instead, add the table reference by pushing to a special slot.
                    // Since we can't access frag directly from bind_term, we embed the
                    // dictionary subquery inline via col_expr IN (SELECT id FROM ...).
                    // Undo the conditions we just pushed and do a proper IN subquery.
                    conditions.pop();
                    conditions.pop();

                    // Build the IN subquery approach instead.
                    let subquery_conds: Vec<String> = std::iter::once("kind = 5".to_owned())
                        .chain(all_conds[1..].iter().cloned())
                        .collect();
                    let subquery_where = subquery_conds.join(" AND ");

                    // For variable bindings: we can't bind from a subquery directly without
                    // adding the dictionary table to FROM. Instead, add an EXISTS clause
                    // that constrains the col_expr.
                    // For variable components (qt_s, qt_p, qt_o) where the variable needs
                    // to be *bound*, we need to add the dictionary table to the FROM list.
                    // We do this by recording the FROM entry in bindings under a special key
                    // and resolving it in translate_bgp.

                    // Simple approach: emit col_expr IN (SELECT id FROM dictionary WHERE ...)
                    // and for each variable component, add a separate FROM entry + binding.
                    let in_subq = format!(
                        "{col_expr} IN (SELECT id FROM _pg_ripple.dictionary WHERE {subquery_where})"
                    );
                    conditions.push(in_subq);

                    // Bind variables by adding the dict table as a lateral join.
                    // We push to bindings map: _qt_alias = "d.qt_s" etc.
                    // The fragment will have these bindings available for SELECT generation.
                    for (vname, dict_col) in &qt_bindings {
                        // dict_col is like "_qt_dict_o.qt_s" — we need it resolvable.
                        // Re-derive the dict_col as a subquery:
                        // SELECT qt_s FROM dictionary WHERE id = col_expr
                        // and bind the variable to that.
                        let derived_col = format!(
                            "(SELECT {dict_col_base} FROM _pg_ripple.dictionary WHERE id = {col_expr})",
                            dict_col_base = dict_col.split('.').next_back().unwrap_or(dict_col),
                            col_expr = col_expr
                        );
                        if !bindings.contains_key(vname.as_str()) {
                            bindings.insert(vname.clone(), derived_col);
                        }
                    }
                }
            }
        }
    }
}

// ─── Core BGP translator ─────────────────────────────────────────────────────

pub(crate) fn translate_bgp(
    patterns: &[spargebra::term::TriplePattern],
    ctx: &mut Ctx,
) -> Fragment {
    let reordered = crate::sparql::optimizer::reorder_bgp(patterns, &mut |iri| ctx.encode_iri(iri));
    let patterns = reordered.as_slice();

    // v0.62.0: WCOJ planner integration — analyse the BGP for cyclic patterns
    // before translating. If cyclic, set ctx.wcoj_preamble so the executor
    // runs the Leapfrog-Triejoin SET LOCAL preamble before the query.
    if patterns.len() >= 3 {
        let pattern_vars: Vec<Vec<String>> = patterns
            .iter()
            .map(|tp| {
                let mut vars: Vec<String> = Vec::new();
                if let spargebra::term::TermPattern::Variable(v) = &tp.subject {
                    vars.push(v.as_str().to_owned());
                }
                if let spargebra::term::NamedNodePattern::Variable(v) = &tp.predicate {
                    vars.push(v.as_str().to_owned());
                }
                if let spargebra::term::TermPattern::Variable(v) = &tp.object {
                    vars.push(v.as_str().to_owned());
                }
                vars
            })
            .collect();
        let analysis = crate::sparql::wcoj::analyse_bgp(&pattern_vars);
        if analysis.use_wcoj {
            ctx.wcoj_preamble = true;
        }
    }

    let mut frag = Fragment::empty();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for tp in patterns {
        let key = format!("{}\x00{}\x00{}", tp.subject, tp.predicate, tp.object);
        if !seen.insert(key) {
            continue;
        }

        let alias = ctx.next_alias();

        let (pred_conditions, source) = match &tp.predicate {
            NamedNodePattern::NamedNode(nn) => match ctx.encode_iri(nn.as_str()) {
                None => (vec![], VpSource::Empty),
                Some(id) => (vec![], vp_source(id)),
            },
            NamedNodePattern::Variable(v) => {
                let vname = v.as_str().to_owned();
                let a = alias.clone();
                let union_subquery =
                    build_all_predicates_union(ctx.graph_filter, &ctx.service_excl());
                frag.from_items
                    .push((a.clone(), format!("({union_subquery})")));
                if let Some(existing) = frag.bindings.get(&vname) {
                    frag.conditions.push(format!("{a}.p = {existing}"));
                } else {
                    frag.bindings.insert(vname, format!("{a}.p"));
                }
                bind_term(
                    &a,
                    "s",
                    &tp.subject,
                    ctx,
                    &mut frag.bindings,
                    &mut frag.conditions,
                );
                bind_term(
                    &a,
                    "o",
                    &tp.object,
                    ctx,
                    &mut frag.bindings,
                    &mut frag.conditions,
                );
                continue;
            }
        };

        let tbl = table_expr(&source, ctx.graph_filter, &ctx.service_excl());
        frag.from_items.push((alias.clone(), tbl));
        for c in pred_conditions {
            frag.conditions.push(c);
        }
        bind_term(
            &alias,
            "s",
            &tp.subject,
            ctx,
            &mut frag.bindings,
            &mut frag.conditions,
        );
        bind_term(
            &alias,
            "o",
            &tp.object,
            ctx,
            &mut frag.bindings,
            &mut frag.conditions,
        );
    }

    frag
}

/// Check if the right side of OPTIONAL is guaranteed non-empty (sh:minCount >= 1).
pub(crate) fn shacl_right_is_mandatory(pattern: &GraphPattern) -> bool {
    let GraphPattern::Bgp { patterns } = pattern else {
        return false;
    };
    if patterns.len() != 1 {
        return false;
    }
    let spargebra::term::NamedNodePattern::NamedNode(nn) = &patterns[0].predicate else {
        return false;
    };
    let Some(pred_id) = crate::dictionary::lookup_iri(nn.as_str()) else {
        return false;
    };
    crate::shacl::hints::has_min_count_1(pred_id)
}

/// Check if ALL predicates in a BGP have sh:maxCount <= 1.
pub(crate) fn shacl_bgp_all_max_count_1(pattern: &GraphPattern) -> bool {
    let GraphPattern::Bgp { patterns } = pattern else {
        return false;
    };
    if patterns.is_empty() {
        return false;
    }
    patterns.iter().all(|tp| {
        let spargebra::term::NamedNodePattern::NamedNode(nn) = &tp.predicate else {
            return false;
        };
        let Some(pred_id) = crate::dictionary::lookup_iri(nn.as_str()) else {
            return false;
        };
        crate::shacl::hints::has_max_count_1(pred_id)
    })
}
