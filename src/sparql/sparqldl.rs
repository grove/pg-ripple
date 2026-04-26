//! SPARQL-DL axiom-level query routing (v0.58.0, Feature L-1.4).
//!
//! SPARQL-DL extends SPARQL with the ability to query OWL axioms (T-Box
//! knowledge) directly.  This module detects BGPs that use OWL vocabulary
//! predicates with unbound or partially-bound subject/object variables and
//! routes them to the in-memory T-Box index rather than generating VP table SQL.

#![allow(dead_code)]

use pgrx::prelude::*;

// ─── OWL predicate IRIs ───────────────────────────────────────────────────────

const OWL_SUB_CLASS_OF: &str = "http://www.w3.org/2002/07/owl#subClassOf";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";

/// OWL vocabulary predicates that trigger SPARQL-DL routing.
pub const SPARQL_DL_PREDICATES: &[&str] = &[
    OWL_SUB_CLASS_OF,
    OWL_EQUIVALENT_CLASS,
    OWL_DISJOINT_WITH,
    OWL_INVERSE_OF,
    OWL_SOME_VALUES_FROM,
    OWL_OBJECT_PROPERTY,
];

// ─── Query analysis ───────────────────────────────────────────────────────────

/// A single SPARQL-DL pattern extracted from a BGP.
#[derive(Debug, Clone)]
pub struct DlPattern {
    /// The OWL vocabulary predicate IRI (without angle brackets).
    pub predicate: String,
    /// Subject variable name or bound IRI.
    pub subject: DlTerm,
    /// Object variable name or bound IRI.
    pub object: DlTerm,
}

/// A term in a SPARQL-DL pattern — either a bound IRI or an unbound variable.
#[derive(Debug, Clone)]
pub enum DlTerm {
    /// An unbound variable (SPARQL `?name`).
    Variable(String),
    /// A bound IRI value (without angle brackets).
    BoundIri(String),
}

impl DlTerm {
    /// Return `true` if this term is an unbound variable.
    pub fn is_variable(&self) -> bool {
        matches!(self, DlTerm::Variable(_))
    }
}

/// Detect whether a predicate IRI string is an OWL SPARQL-DL predicate.
pub fn is_sparql_dl_predicate(predicate_iri: &str) -> bool {
    let iri = predicate_iri
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>');
    SPARQL_DL_PREDICATES.contains(&iri)
}

// ─── T-Box query execution ────────────────────────────────────────────────────

/// Execute a SPARQL-DL pattern against the T-Box (VP table data).
///
/// Returns rows of `(subject_iri, object_iri)` that satisfy the pattern.
/// The results are returned as a SQL `VALUES` clause string suitable for
/// injection into the query plan as a lateral join.
///
/// # Implementation note
///
/// Rather than maintaining a separate in-memory T-Box index, we query the VP
/// tables directly using the encoded predicate IDs.  This is semantically
/// equivalent to a T-Box query and avoids memory management complexity.
/// For performance-critical deployments, a future version could cache the
/// T-Box in shared memory (see plans/implementation_plan.md §L-1.4).
pub fn execute_dl_pattern(pattern: &DlPattern) -> Vec<(String, String)> {
    let pred_iri = &pattern.predicate;
    let pred_id = crate::dictionary::encode(pred_iri, crate::dictionary::KIND_IRI);

    // Build the query based on bound/unbound subject and object.
    // OWL axiom predicates are almost always in vp_rare (rarely promoted to their
    // own VP table).  We query vp_rare only.
    let base = "SELECT ds.value AS subj, do_.value AS obj \
         FROM _pg_ripple.vp_rare vr \
         JOIN _pg_ripple.dictionary ds ON ds.id = vr.s \
         JOIN _pg_ripple.dictionary do_ ON do_.id = vr.o";

    let sql = match (&pattern.subject, &pattern.object) {
        (DlTerm::Variable(_), DlTerm::Variable(_)) => {
            format!(
                "{base} \
                 WHERE vr.p = {pred_id} \
                 LIMIT 10000"
            )
        }
        (DlTerm::BoundIri(s_iri), DlTerm::Variable(_)) => {
            let s_id = crate::dictionary::encode(s_iri, crate::dictionary::KIND_IRI);
            format!(
                "{base} \
                 WHERE vr.p = {pred_id} AND vr.s = {s_id} \
                 LIMIT 1000"
            )
        }
        (DlTerm::Variable(_), DlTerm::BoundIri(o_iri)) => {
            let o_id = crate::dictionary::encode(o_iri, crate::dictionary::KIND_IRI);
            format!(
                "{base} \
                 WHERE vr.p = {pred_id} AND vr.o = {o_id} \
                 LIMIT 1000"
            )
        }
        (DlTerm::BoundIri(s_iri), DlTerm::BoundIri(o_iri)) => {
            let s_id = crate::dictionary::encode(s_iri, crate::dictionary::KIND_IRI);
            let o_id = crate::dictionary::encode(o_iri, crate::dictionary::KIND_IRI);
            format!(
                "{base} \
                 WHERE vr.p = {pred_id} AND vr.s = {s_id} AND vr.o = {o_id} \
                 LIMIT 1"
            )
        }
    };

    Spi::connect(|c| {
        c.select(&sql, None, &[])
            .map(|rows| {
                rows.filter_map(|row| {
                    let subj = row.get::<String>(1).ok().flatten().unwrap_or_default();
                    let obj = row.get::<String>(2).ok().flatten().unwrap_or_default();
                    if subj.is_empty() || obj.is_empty() {
                        None
                    } else {
                        Some((subj, obj))
                    }
                })
                .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    })
}

/// Format DL results as a SQL VALUES clause.
///
/// Returns something like:
/// ```sql
/// VALUES ('<http://ex.org/A>', '<http://ex.org/B>'), ...
/// ```
/// or `NULL` if there are no results.
pub fn dl_results_to_values(rows: &[(String, String)]) -> String {
    if rows.is_empty() {
        return "SELECT NULL::text AS s, NULL::text AS o WHERE false".to_string();
    }
    let vals: Vec<String> = rows
        .iter()
        .map(|(s, o)| {
            format!(
                "($$<{}>$$, $$<{}>$$)",
                s.trim_matches('<').trim_matches('>'),
                o.trim_matches('<').trim_matches('>')
            )
        })
        .collect();
    format!("VALUES {}", vals.join(", "))
}

// ─── SQL API ──────────────────────────────────────────────────────────────────

/// Execute a SPARQL-DL subclass query for the given class IRI.
///
/// Returns all classes that are `owl:subClassOf` the given `class_iri`
/// (direct subclasses only — transitivity not expanded here).
///
/// SQL API: `pg_ripple.sparql_dl_subclasses(class_iri TEXT)`
#[pg_extern(schema = "pg_ripple")]
pub fn sparql_dl_subclasses(
    class_iri: &str,
) -> TableIterator<'static, (name!(subclass_iri, String),)> {
    let pattern = DlPattern {
        predicate: OWL_SUB_CLASS_OF.to_string(),
        subject: DlTerm::Variable("sub".to_string()),
        object: DlTerm::BoundIri(class_iri.trim_matches('<').trim_matches('>').to_string()),
    };
    let rows = execute_dl_pattern(&pattern);
    TableIterator::new(rows.into_iter().map(|(s, _)| (s,)))
}

/// Execute a SPARQL-DL superclass query for the given class IRI.
///
/// Returns all classes that the given `class_iri` is `owl:subClassOf`.
#[pg_extern(schema = "pg_ripple")]
pub fn sparql_dl_superclasses(
    class_iri: &str,
) -> TableIterator<'static, (name!(superclass_iri, String),)> {
    let pattern = DlPattern {
        predicate: OWL_SUB_CLASS_OF.to_string(),
        subject: DlTerm::BoundIri(class_iri.trim_matches('<').trim_matches('>').to_string()),
        object: DlTerm::Variable("super".to_string()),
    };
    let rows = execute_dl_pattern(&pattern);
    TableIterator::new(rows.into_iter().map(|(_, o)| (o,)))
}
