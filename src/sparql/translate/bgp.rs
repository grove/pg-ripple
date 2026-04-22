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
        TermPattern::Triple(_) => match ground_term_id(term, ctx) {
            Some(id) => conditions.push(format!("{col_expr} = {id}")),
            None => {
                pgrx::warning!(
                    "SPARQL-star: variable inside quoted triple pattern is not yet supported; \
                         pattern treated as no-match"
                );
                conditions.push("FALSE".to_owned());
            }
        },
    }
}

// ─── Core BGP translator ─────────────────────────────────────────────────────

pub(crate) fn translate_bgp(
    patterns: &[spargebra::term::TriplePattern],
    ctx: &mut Ctx,
) -> Fragment {
    let reordered = crate::sparql::optimizer::reorder_bgp(patterns, &mut |iri| ctx.encode_iri(iri));
    let patterns = reordered.as_slice();

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
