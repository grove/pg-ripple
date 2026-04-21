//! sh:equals and sh:disjoint relational constraint checkers (v0.45.0).
//!
//! Both constraints compare the *set* of value-node IDs for the focus node's
//! declared path with the set of value-node IDs for an *other* path.
//!
//! ## sh:equals
//! For every focus node n:
//!   values(path) == values(other_path)
//! Implemented as two NOT-EXISTS checks (one per direction).
//!
//! ## sh:disjoint
//! For every focus node n:
//!   values(path) ∩ values(other_path) == ∅
//! Implemented as an EXISTS check for any shared value ID.

use super::{ConstraintArgs, Violation, get_value_ids};

/// Check `sh:equals other_path_iri` — the value set for the focus node's
/// declared path must be identical to the value set for `other_path_iri`.
pub(crate) fn check_equals(
    other_path_iri: &str,
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let other_pred_id = match crate::dictionary::lookup_iri(other_path_iri) {
        Some(id) => id,
        None => {
            // Other path not in dictionary → other set is empty.
            // Our set is non-empty only if we have values; if we do, that is a
            // violation (the sets are not equal).
            let my_values = get_value_ids(args.focus, args.path_id, args.graph_id);
            if !my_values.is_empty() {
                let focus_iri = crate::shacl::decode_id_safe(args.focus);
                violations.push(Violation {
                    focus_node: focus_iri,
                    shape_iri: args.shape_iri.to_owned(),
                    path: Some(args.path_iri.to_owned()),
                    constraint: "sh:equals".to_owned(),
                    message: format!(
                        "value set for <{}> is not equal to value set for <{other_path_iri}>: \
                         other path has no values",
                        args.path_iri
                    ),
                    severity: "Violation".to_owned(),
                });
            }
            return;
        }
    };

    let my_values: std::collections::HashSet<i64> =
        get_value_ids(args.focus, args.path_id, args.graph_id)
            .into_iter()
            .collect();
    let other_values: std::collections::HashSet<i64> =
        get_value_ids(args.focus, other_pred_id, args.graph_id)
            .into_iter()
            .collect();

    if my_values != other_values {
        let focus_iri = crate::shacl::decode_id_safe(args.focus);
        // Describe the symmetric difference to aid debugging.
        let only_mine: Vec<i64> = my_values.difference(&other_values).copied().collect();
        let only_other: Vec<i64> = other_values.difference(&my_values).copied().collect();
        violations.push(Violation {
            focus_node: focus_iri,
            shape_iri: args.shape_iri.to_owned(),
            path: Some(args.path_iri.to_owned()),
            constraint: "sh:equals".to_owned(),
            message: format!(
                "value set for <{}> != value set for <{other_path_iri}>: \
                 only-in-path={only_mine:?}, only-in-other={only_other:?}",
                args.path_iri
            ),
            severity: "Violation".to_owned(),
        });
    }
}

/// Check `sh:disjoint other_path_iri` — the value set for the focus node's
/// declared path must share no common values with `other_path_iri`.
pub(crate) fn check_disjoint(
    other_path_iri: &str,
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let other_pred_id = match crate::dictionary::lookup_iri(other_path_iri) {
        Some(id) => id,
        None => {
            // Other path not in dictionary → other set is empty → disjoint trivially.
            return;
        }
    };

    let my_values: std::collections::HashSet<i64> =
        get_value_ids(args.focus, args.path_id, args.graph_id)
            .into_iter()
            .collect();
    let other_values: std::collections::HashSet<i64> =
        get_value_ids(args.focus, other_pred_id, args.graph_id)
            .into_iter()
            .collect();

    let shared: Vec<i64> = my_values.intersection(&other_values).copied().collect();
    if !shared.is_empty() {
        let focus_iri = crate::shacl::decode_id_safe(args.focus);
        violations.push(Violation {
            focus_node: focus_iri,
            shape_iri: args.shape_iri.to_owned(),
            path: Some(args.path_iri.to_owned()),
            constraint: "sh:disjoint".to_owned(),
            message: format!(
                "value set for <{}> is not disjoint with <{other_path_iri}>: \
                 shared value ids={shared:?}",
                args.path_iri
            ),
            severity: "Violation".to_owned(),
        });
    }
}
