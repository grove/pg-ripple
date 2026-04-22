//! Join (inner join) translator with batch SERVICE detection.

use spargebra::algebra::GraphPattern;
use spargebra::term::NamedNodePattern;

use crate::sparql::federation;
use crate::sparql::sqlgen::{Ctx, Fragment};

pub(crate) fn translate_join(left: &GraphPattern, right: &GraphPattern, ctx: &mut Ctx) -> Fragment {
    // Batch SERVICE detection: combine two SERVICE clauses to same endpoint.
    if let (
        GraphPattern::Service {
            name: name_l,
            inner: inner_l,
            silent: silent_l,
        },
        GraphPattern::Service {
            name: name_r,
            inner: inner_r,
            silent: silent_r,
        },
    ) = (left, right)
        && let (NamedNodePattern::NamedNode(url_l), NamedNodePattern::NamedNode(url_r)) =
            (name_l, name_r)
    {
        let url_l_str = url_l.as_str();
        let url_r_str = url_r.as_str();
        if url_l_str == url_r_str {
            let vars_l = federation::collect_pattern_variables(inner_l);
            let vars_r = federation::collect_pattern_variables(inner_r);
            if vars_l.is_disjoint(&vars_r)
                && let Some(frag) = crate::sparql::translate::graph::translate_service_batched(
                    url_l_str,
                    inner_l,
                    inner_r,
                    *silent_l || *silent_r,
                    ctx,
                )
            {
                return frag;
            }
        }
    }
    // Standard join: merge fragments.
    let mut frag = crate::sparql::sqlgen::translate_pattern(left, ctx);
    let right_frag = crate::sparql::sqlgen::translate_pattern(right, ctx);
    frag.merge(right_frag);
    frag
}
