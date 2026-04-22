//! sh:in, sh:hasValue, sh:lessThan, sh:greaterThan, sh:closed constraint checkers.

use super::{
    ConstraintArgs, Violation, compare_dictionary_values, encode_shacl_in_value, get_value_ids,
};

/// Check `sh:in [values]` — every value node must be one of the allowed values.
pub(crate) fn check_in(
    allowed_values: &[String],
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let allowed_ids: Vec<i64> = allowed_values
        .iter()
        .filter_map(|val| encode_shacl_in_value(val))
        .collect();
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    for v_id in value_ids {
        if !allowed_ids.contains(&v_id) {
            let focus_iri = crate::dictionary::decode(args.focus)
                .unwrap_or_else(|| format!("_id_{}", args.focus));
            violations.push(Violation {
                focus_node: focus_iri,
                shape_iri: args.shape_iri.to_owned(),
                path: Some(args.path_iri.to_owned()),
                constraint: "sh:in".to_owned(),
                message: format!("value node id {v_id} is not in the allowed value set"),
                severity: "Violation".to_owned(),
            });
        }
    }
}

/// Check `sh:hasValue value` — the focus node must have exactly the given value.
pub(crate) fn check_has_value(
    expected_val: &str,
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let expected_id = match encode_shacl_in_value(expected_val) {
        Some(id) => id,
        None => return, // Expected value not in dictionary — cannot match.
    };
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    if !value_ids.contains(&expected_id) {
        let focus_iri =
            crate::dictionary::decode(args.focus).unwrap_or_else(|| format!("_id_{}", args.focus));
        violations.push(Violation {
            focus_node: focus_iri,
            shape_iri: args.shape_iri.to_owned(),
            path: Some(args.path_iri.to_owned()),
            constraint: "sh:hasValue".to_owned(),
            message: format!("expected value '{expected_val}' not found"),
            severity: "Violation".to_owned(),
        });
    }
}

/// Check `sh:lessThan other_path` — every value must be less than the corresponding
/// value of `other_path` for the same focus node.
pub(crate) fn check_less_than(
    other_path_iri: &str,
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let other_pred_id = match crate::dictionary::lookup_iri(other_path_iri) {
        Some(id) => id,
        None => return,
    };
    let my_values = get_value_ids(args.focus, args.path_id, args.graph_id);
    let other_values = get_value_ids(args.focus, other_pred_id, args.graph_id);
    for v_id in &my_values {
        for o_id in &other_values {
            if compare_dictionary_values(*v_id, *o_id) != Some(std::cmp::Ordering::Less) {
                let focus_iri = crate::dictionary::decode(args.focus)
                    .unwrap_or_else(|| format!("_id_{}", args.focus));
                violations.push(Violation {
                    focus_node: focus_iri,
                    shape_iri: args.shape_iri.to_owned(),
                    path: Some(args.path_iri.to_owned()),
                    constraint: "sh:lessThan".to_owned(),
                    message: format!("value id {v_id} is not less than other-path value id {o_id}"),
                    severity: "Violation".to_owned(),
                });
            }
        }
    }
}

/// Check `sh:greaterThan other_path` — every value must be greater than the
/// corresponding value of `other_path` for the same focus node.
pub(crate) fn check_greater_than(
    other_path_iri: &str,
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let other_pred_id = match crate::dictionary::lookup_iri(other_path_iri) {
        Some(id) => id,
        None => return,
    };
    let my_values = get_value_ids(args.focus, args.path_id, args.graph_id);
    let other_values = get_value_ids(args.focus, other_pred_id, args.graph_id);
    for v_id in &my_values {
        for o_id in &other_values {
            if compare_dictionary_values(*v_id, *o_id) != Some(std::cmp::Ordering::Greater) {
                let focus_iri = crate::dictionary::decode(args.focus)
                    .unwrap_or_else(|| format!("_id_{}", args.focus));
                violations.push(Violation {
                    focus_node: focus_iri,
                    shape_iri: args.shape_iri.to_owned(),
                    path: Some(args.path_iri.to_owned()),
                    constraint: "sh:greaterThan".to_owned(),
                    message: format!(
                        "value id {v_id} is not greater than other-path value id {o_id}"
                    ),
                    severity: "Violation".to_owned(),
                });
            }
        }
    }
}

/// Check `sh:lessThanOrEquals other_path` — every value must be less than or equal to
/// every value of `other_path` for the same focus node.
pub(crate) fn check_less_than_or_equals(
    other_path_iri: &str,
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let other_pred_id = match crate::dictionary::lookup_iri(other_path_iri) {
        Some(id) => id,
        None => return,
    };
    let my_values = get_value_ids(args.focus, args.path_id, args.graph_id);
    let other_values = get_value_ids(args.focus, other_pred_id, args.graph_id);
    for v_id in &my_values {
        for o_id in &other_values {
            match compare_dictionary_values(*v_id, *o_id) {
                Some(std::cmp::Ordering::Less) | Some(std::cmp::Ordering::Equal) => {}
                _ => {
                    let focus_iri = crate::dictionary::decode(args.focus)
                        .unwrap_or_else(|| format!("_id_{}", args.focus));
                    violations.push(Violation {
                        focus_node: focus_iri,
                        shape_iri: args.shape_iri.to_owned(),
                        path: Some(args.path_iri.to_owned()),
                        constraint: "sh:lessThanOrEquals".to_owned(),
                        message: format!(
                            "value id {v_id} is not less than or equal to other-path value id {o_id}"
                        ),
                        severity: "Violation".to_owned(),
                    });
                }
            }
        }
    }
}

/// `sh:closed` — Verify that there are no disallowed predicates.
/// Currently a placeholder; full implementation requires enumerating all
/// declared paths in the shape and checking for unexpected predicates.
pub(crate) fn check_closed(_args: &ConstraintArgs, _violations: &mut Vec<Violation>) {
    // sh:closed enforcement is validated at the node level by run_validate().
    // The per-property dispatch skips this constraint because checking requires
    // the full set of declared property paths for the enclosing node shape,
    // which is not available in the per-property ConstraintArgs.
}
