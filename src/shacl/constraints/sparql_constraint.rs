//! `sh:SPARQLConstraintComponent` — SHACL-SPARQL custom constraint checker (v0.53.0).
//!
//! Executes a user-authored SPARQL SELECT query as a validation rule.
//! The query must use `$this` as the focus-node variable.  Any non-empty
//! result set constitutes a constraint violation.
//!
//! ## W3C SHACL-SPARQL specification
//!
//! <https://www.w3.org/TR/shacl-af/#sparql-constraints>

use super::{ConstraintArgs, Violation};

/// Check a `sh:SPARQLConstraintComponent` constraint.
///
/// The `sparql_query` must be a complete SPARQL SELECT query.  The focus-node
/// IRI is substituted for the `$this` variable before execution.
///
/// Any rows returned by the query are decoded into `Violation` structs.
pub(crate) fn check_sparql_constraint(
    sparql_query: &str,
    message: Option<&str>,
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let focus_iri =
        crate::dictionary::decode(args.focus).unwrap_or_else(|| format!("_id_{}", args.focus));

    // Substitute `$this` with the encoded focus-node IRI.
    let bound_query = sparql_query.replace("$this", &format!("<{focus_iri}>"));

    // Execute the bound SPARQL query via the SPARQL engine.
    // sparql() returns Vec<pgrx::JsonB>; use catch_unwind since pgrx errors use longjmp.
    let rows = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::sparql::sparql(&bound_query)
    }));

    let rows = match rows {
        Ok(r) => r,
        Err(_) => {
            pgrx::warning!(
                "SHACL-SPARQL constraint query execution failed (PT481); \
                 shape={}, focus={}",
                args.shape_iri,
                focus_iri
            );
            return;
        }
    };

    if rows.is_empty() {
        // Query returned no results — constraint satisfied.
        return;
    }

    // Any returned rows constitute a violation.
    for row in &rows {
        let value_str = row
            .0
            .as_object()
            .and_then(|o| o.values().next())
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned());

        violations.push(Violation {
            focus_node: focus_iri.clone(),
            shape_iri: args.shape_iri.to_owned(),
            path: Some(args.path_iri.to_owned()),
            constraint: "sh:SPARQLConstraintComponent".to_owned(),
            message: message
                .map(|m| m.to_owned())
                .unwrap_or_else(|| format!("SHACL-SPARQL constraint violated for <{}>", focus_iri)),
            severity: "Violation".to_owned(),
            sh_value: value_str,
            sh_source_constraint_component: Some(
                "http://www.w3.org/ns/shacl#SPARQLConstraintComponent".to_owned(),
            ),
        });
    }
}
