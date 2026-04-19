//! sh:datatype, sh:class, and sh:nodeKind constraint checkers.

use super::{
    ConstraintArgs, Violation, get_value_ids, value_has_datatype, value_has_node_kind,
    value_has_rdf_type,
};

/// Check `sh:datatype iri` — every value must have the specified RDF datatype.
pub(crate) fn check_datatype(dt_iri: &str, args: &ConstraintArgs, violations: &mut Vec<Violation>) {
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    for v_id in value_ids {
        if !value_has_datatype(v_id, dt_iri) {
            let focus_iri = crate::dictionary::decode(args.focus)
                .unwrap_or_else(|| format!("_id_{}", args.focus));
            violations.push(Violation {
                focus_node: focus_iri,
                shape_iri: args.shape_iri.to_owned(),
                path: Some(args.path_iri.to_owned()),
                constraint: "sh:datatype".to_owned(),
                message: format!("value node id {v_id} does not have datatype <{dt_iri}>"),
                severity: "Violation".to_owned(),
            });
        }
    }
}

/// Check `sh:class class_iri` — every value must be an instance of the given class.
pub(crate) fn check_class(class_iri: &str, args: &ConstraintArgs, violations: &mut Vec<Violation>) {
    let class_id = match crate::dictionary::lookup_iri(class_iri) {
        Some(id) => id,
        None => return,
    };
    let rdf_type_id =
        match crate::dictionary::lookup_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type") {
            Some(id) => id,
            None => return,
        };
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    for v_id in value_ids {
        if !value_has_rdf_type(v_id, rdf_type_id, class_id) {
            let focus_iri = crate::dictionary::decode(args.focus)
                .unwrap_or_else(|| format!("_id_{}", args.focus));
            violations.push(Violation {
                focus_node: focus_iri,
                shape_iri: args.shape_iri.to_owned(),
                path: Some(args.path_iri.to_owned()),
                constraint: "sh:class".to_owned(),
                message: format!("value node id {v_id} is not an instance of <{class_iri}>"),
                severity: "Violation".to_owned(),
            });
        }
    }
}

/// Check `sh:nodeKind kind_iri` — every value must have the specified node kind.
pub(crate) fn check_node_kind(
    kind_iri: &str,
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    for v_id in value_ids {
        if !value_has_node_kind(v_id, kind_iri) {
            let focus_iri = crate::dictionary::decode(args.focus)
                .unwrap_or_else(|| format!("_id_{}", args.focus));
            violations.push(Violation {
                focus_node: focus_iri,
                shape_iri: args.shape_iri.to_owned(),
                path: Some(args.path_iri.to_owned()),
                constraint: "sh:nodeKind".to_owned(),
                message: format!("value node id {v_id} does not have node kind <{kind_iri}>"),
                severity: "Violation".to_owned(),
            });
        }
    }
}
