//! sh:node, sh:or, sh:and, sh:not, sh:xone, sh:qualifiedValueShape constraint checkers.

use super::{ConstraintArgs, Violation, get_value_ids, node_conforms_to_shape};

/// Check `sh:node shape_iri` -- every value must conform to the referenced shape.
pub(crate) fn check_node(
    ref_shape_iri: &str,
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    for v_id in value_ids {
        if !node_conforms_to_shape(v_id, ref_shape_iri, args.graph_id, args.all_shapes) {
            let focus_iri = crate::dictionary::decode(args.focus)
                .unwrap_or_else(|| format!("_id_{}", args.focus));
            violations.push(Violation {
                focus_node: focus_iri,
                shape_iri: args.shape_iri.to_owned(),
                path: Some(args.path_iri.to_owned()),
                constraint: "sh:node".to_owned(),
                message: format!(
                    "value id {} does not conform to shape <{ref_shape_iri}>",
                    v_id
                ),
                severity: "Violation".to_owned(),
                sh_value: None,
                sh_source_constraint_component: None,
            });
        }
    }
}

/// Check `sh:or [shape_iris]` -- every value must conform to at least one shape.
pub(crate) fn check_or(
    shape_iris: &[String],
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    for v_id in value_ids {
        let passes = shape_iris
            .iter()
            .any(|si| node_conforms_to_shape(v_id, si, args.graph_id, args.all_shapes));
        if !passes {
            let focus_iri = crate::dictionary::decode(args.focus)
                .unwrap_or_else(|| format!("_id_{}", args.focus));
            violations.push(Violation {
                focus_node: focus_iri,
                shape_iri: args.shape_iri.to_owned(),
                path: Some(args.path_iri.to_owned()),
                constraint: "sh:or".to_owned(),
                message: format!(
                    "value id {} does not conform to any of {:?}",
                    v_id, shape_iris
                ),
                severity: "Violation".to_owned(),
                sh_value: None,
                sh_source_constraint_component: None,
            });
        }
    }
}

/// Check `sh:and [shape_iris]` -- every value must conform to all shapes.
pub(crate) fn check_and(
    shape_iris: &[String],
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    for v_id in value_ids {
        for si in shape_iris {
            if !node_conforms_to_shape(v_id, si, args.graph_id, args.all_shapes) {
                let focus_iri = crate::dictionary::decode(args.focus)
                    .unwrap_or_else(|| format!("_id_{}", args.focus));
                violations.push(Violation {
                    focus_node: focus_iri,
                    shape_iri: args.shape_iri.to_owned(),
                    path: Some(args.path_iri.to_owned()),
                    constraint: "sh:and".to_owned(),
                    message: format!("value id {v_id} does not conform to shape <{si}>"),
                    severity: "Violation".to_owned(),
                    sh_value: None,
                    sh_source_constraint_component: None,
                });
            }
        }
    }
}

/// Check `sh:not shape_iri` -- every value must NOT conform to the shape.
pub(crate) fn check_not(
    ref_shape_iri: &str,
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    for v_id in value_ids {
        if node_conforms_to_shape(v_id, ref_shape_iri, args.graph_id, args.all_shapes) {
            let focus_iri = crate::dictionary::decode(args.focus)
                .unwrap_or_else(|| format!("_id_{}", args.focus));
            violations.push(Violation {
                focus_node: focus_iri,
                shape_iri: args.shape_iri.to_owned(),
                path: Some(args.path_iri.to_owned()),
                constraint: "sh:not".to_owned(),
                message: format!(
                    "value id {v_id} unexpectedly conforms to negated shape <{ref_shape_iri}>"
                ),
                severity: "Violation".to_owned(),
                sh_value: None,
                sh_source_constraint_component: None,
            });
        }
    }
}

/// Check `sh:xone [shape_iris]` — every value must conform to *exactly one* of the given shapes.
pub(crate) fn check_xone(
    shape_iris: &[String],
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    for v_id in value_ids {
        let matching: usize = shape_iris
            .iter()
            .filter(|si| node_conforms_to_shape(v_id, si, args.graph_id, args.all_shapes))
            .count();
        if matching != 1 {
            let focus_iri = crate::dictionary::decode(args.focus)
                .unwrap_or_else(|| format!("_id_{}", args.focus));
            violations.push(Violation {
                focus_node: focus_iri,
                shape_iri: args.shape_iri.to_owned(),
                path: Some(args.path_iri.to_owned()),
                constraint: "sh:xone".to_owned(),
                message: format!(
                    "value id {v_id} conforms to {matching} of {:?}, expected exactly 1",
                    shape_iris
                ),
                severity: "Violation".to_owned(),
                sh_value: None,
                sh_source_constraint_component: None,
            });
        }
    }
}

pub(crate) fn check_qualified(
    qvs_shape_iri: &str,
    min_count: Option<i64>,
    max_count: Option<i64>,
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    let qualifying: i64 = value_ids
        .iter()
        .filter(|&&v| node_conforms_to_shape(v, qvs_shape_iri, args.graph_id, args.all_shapes))
        .count() as i64;

    if let Some(min) = min_count
        && qualifying < min
    {
        let focus_iri =
            crate::dictionary::decode(args.focus).unwrap_or_else(|| format!("_id_{}", args.focus));
        violations.push(Violation {
            focus_node: focus_iri,
            shape_iri: args.shape_iri.to_owned(),
            path: Some(args.path_iri.to_owned()),
            constraint: "sh:qualifiedMinCount".to_owned(),
            message: format!(
                "expected at least {min} values conforming to <{qvs_shape_iri}>, found {qualifying}"
            ),
            severity: "Violation".to_owned(),
            sh_value: None,
            sh_source_constraint_component: None,
        });
    }
    if let Some(max) = max_count
        && qualifying > max
    {
        let focus_iri =
            crate::dictionary::decode(args.focus).unwrap_or_else(|| format!("_id_{}", args.focus));
        violations.push(Violation {
            focus_node: focus_iri,
            shape_iri: args.shape_iri.to_owned(),
            path: Some(args.path_iri.to_owned()),
            constraint: "sh:qualifiedMaxCount".to_owned(),
            message: format!(
                "expected at most {max} values conforming to <{qvs_shape_iri}>, found {qualifying}"
            ),
            severity: "Violation".to_owned(),
            sh_value: None,
            sh_source_constraint_component: None,
        });
    }
}
