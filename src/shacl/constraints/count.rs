//! sh:minCount and sh:maxCount constraint checkers.

use super::{ConstraintArgs, Violation};

/// Check `sh:minCount n` — focus must have at least `n` values along the path.
pub(crate) fn check_min_count(n: i64, args: &ConstraintArgs, violations: &mut Vec<Violation>) {
    if args.count < n {
        let focus_iri =
            crate::dictionary::decode(args.focus).unwrap_or_else(|| format!("_id_{}", args.focus));
        violations.push(Violation {
            focus_node: focus_iri,
            shape_iri: args.shape_iri.to_owned(),
            path: Some(args.path_iri.to_owned()),
            constraint: "sh:minCount".to_owned(),
            message: format!(
                "expected at least {n} value(s) for <{}>, found {}",
                args.path_iri, args.count
            ),
            severity: "Violation".to_owned(),
            sh_value: None,
            sh_source_constraint_component: None,
        });
    }
}

/// Check `sh:maxCount n` — focus must have at most `n` values along the path.
pub(crate) fn check_max_count(n: i64, args: &ConstraintArgs, violations: &mut Vec<Violation>) {
    if args.count > n {
        let focus_iri =
            crate::dictionary::decode(args.focus).unwrap_or_else(|| format!("_id_{}", args.focus));
        violations.push(Violation {
            focus_node: focus_iri,
            shape_iri: args.shape_iri.to_owned(),
            path: Some(args.path_iri.to_owned()),
            constraint: "sh:maxCount".to_owned(),
            message: format!(
                "expected at most {n} value(s) for <{}>, found {}",
                args.path_iri, args.count
            ),
            severity: "Violation".to_owned(),
            sh_value: None,
            sh_source_constraint_component: None,
        });
    }
}
