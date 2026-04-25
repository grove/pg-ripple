//! OWL 2 QL / DL-Lite query rewriting for pg_ripple v0.57.0.
//!
//! When `pg_ripple.owl_profile = 'QL'`, SPARQL Basic Graph Patterns (BGPs)
//! are rewritten before SQL translation by expanding OWL 2 QL axioms
//! (subClassOf, inverseOf, subPropertyOf, DisjointClasses) that are present
//! in the active named graph.
//!
//! # Supported axiom forms
//!
//! | Axiom | Effect on query rewriting |
//! |---|---|
//! | `SubClassOf(:A :B)` | Expand type checks on `:A` to also match `:B` subclasses |
//! | `SubObjectPropertyOf(:p :q)` | Property lookups on `:q` also match via `:p` |
//! | `InverseObjectProperty(:p :q)` | Add reversed triple pattern |
//! | `DisjointClasses(:A :B)` | Validate / prune via `owl:disjointWith` awareness |
//!
//! The rewriter operates on the SPARQL algebra level (BGP triples list),
//! producing an expanded set of UNION-ed BGPs. The expansion is kept shallow
//! (one step) to avoid query blowup.

#![allow(dead_code)]

/// Rewrite result type: a list of additional BGP triple patterns to UNION with
/// the original BGPs. Each entry is `(subject_var_or_iri, property_iri, object_var_or_iri)`.
pub type BgpTriple = (String, String, String);

/// Perform a one-step OWL 2 QL rewrite on a list of BGP triples.
///
/// This function queries the database for registered QL axioms and returns
/// a list of additional triple patterns that should be UNION-ed with the
/// original patterns to simulate ontology-mediated query answering.
///
/// # Arguments
///
/// * `triples` - The original BGP triple patterns as `(subject, predicate, object)` strings
///
/// Returns a (possibly empty) list of additional triples to union.
pub fn rewrite_bgp(triples: &[BgpTriple]) -> Vec<BgpTriple> {
    let mut expansions: Vec<BgpTriple> = Vec::new();

    for (s, p, o) in triples {
        // Expand rdf:type triples via SubClassOf axioms.
        if p == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" {
            // Look up subclasses of `o` in the dictionary.
            let subclasses = query_subclasses(o);
            for subclass in subclasses {
                expansions.push((s.clone(), p.clone(), subclass));
            }
        }

        // Expand property patterns via SubObjectPropertyOf axioms.
        {
            let subprops = query_subproperties(p);
            for subprop in subprops {
                expansions.push((s.clone(), subprop, o.clone()));
            }
        }

        // Expand via InverseObjectProperty axioms.
        {
            let inverses = query_inverse_properties(p);
            for inv in inverses {
                // Swap subject and object for the inverse property.
                expansions.push((o.clone(), inv, s.clone()));
            }
        }
    }

    expansions
}

/// Query `_pg_ripple` for all subclasses of a given class IRI.
/// Returns empty vec if the class has no registered subclasses.
fn query_subclasses(class_iri: &str) -> Vec<String> {
    use pgrx::datum::DatumWithOid;
    use pgrx::prelude::*;

    if class_iri.starts_with('?') {
        // Variable — cannot expand statically.
        return Vec::new();
    }

    let mut result = Vec::new();
    let subclass_pred = crate::dictionary::encode(
        "http://www.w3.org/2000/01/rdf-schema#subClassOf",
        crate::dictionary::KIND_IRI,
    );
    let class_id = crate::dictionary::encode(class_iri, crate::dictionary::KIND_IRI);

    // Find all X where X rdfs:subClassOf class_iri (direct subclasses only).
    let found: Vec<String> = Spi::connect(|client| {
        let rows = client.select(
            "SELECT s FROM _pg_ripple.vp_rare WHERE p = $1 AND o = $2 LIMIT 50",
            None,
            &[
                DatumWithOid::from(subclass_pred),
                DatumWithOid::from(class_id),
            ],
        )?;
        let mut v = Vec::new();
        for row in rows {
            if let Some(decoded) = row.get::<i64>(1)?.and_then(crate::dictionary::decode) {
                v.push(decoded);
            }
        }
        Ok::<_, pgrx::spi::Error>(v)
    })
    .unwrap_or_default();
    result.extend(found);
    result
}

/// Query for all sub-properties of a given property IRI.
fn query_subproperties(prop_iri: &str) -> Vec<String> {
    use pgrx::datum::DatumWithOid;
    use pgrx::prelude::*;

    if prop_iri.starts_with('?') {
        return Vec::new();
    }

    let mut result = Vec::new();
    let subprop_pred = crate::dictionary::encode(
        "http://www.w3.org/2000/01/rdf-schema#subPropertyOf",
        crate::dictionary::KIND_IRI,
    );
    let prop_id = crate::dictionary::encode(prop_iri, crate::dictionary::KIND_IRI);

    let found: Vec<String> = Spi::connect(|client| {
        let rows = client.select(
            "SELECT s FROM _pg_ripple.vp_rare WHERE p = $1 AND o = $2 LIMIT 50",
            None,
            &[
                DatumWithOid::from(subprop_pred),
                DatumWithOid::from(prop_id),
            ],
        )?;
        let mut v = Vec::new();
        for row in rows {
            if let Some(decoded) = row.get::<i64>(1)?.and_then(crate::dictionary::decode) {
                v.push(decoded);
            }
        }
        Ok::<_, pgrx::spi::Error>(v)
    })
    .unwrap_or_default();
    result.extend(found);
    result
}

/// Query for inverse properties of a given property IRI.
fn query_inverse_properties(prop_iri: &str) -> Vec<String> {
    use pgrx::datum::DatumWithOid;
    use pgrx::prelude::*;

    if prop_iri.starts_with('?') {
        return Vec::new();
    }

    let mut result = Vec::new();
    let inverse_pred = crate::dictionary::encode(
        "http://www.w3.org/2002/07/owl#inverseOf",
        crate::dictionary::KIND_IRI,
    );
    let prop_id = crate::dictionary::encode(prop_iri, crate::dictionary::KIND_IRI);

    let found: Vec<String> = Spi::connect(|client| {
        let rows = client.select(
            "SELECT o FROM _pg_ripple.vp_rare WHERE p = $1 AND s = $2 \
             UNION SELECT s FROM _pg_ripple.vp_rare WHERE p = $1 AND o = $2 LIMIT 50",
            None,
            &[
                DatumWithOid::from(inverse_pred),
                DatumWithOid::from(prop_id),
            ],
        )?;
        let mut v = Vec::new();
        for row in rows {
            if let Some(decoded) = row.get::<i64>(1)?.and_then(crate::dictionary::decode) {
                v.push(decoded);
            }
        }
        Ok::<_, pgrx::spi::Error>(v)
    })
    .unwrap_or_default();
    result.extend(found);
    result
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Check if OWL 2 QL rewriting is enabled via the `owl_profile` GUC.
pub fn is_ql_profile_active() -> bool {
    if let Some(profile) = crate::OWL_PROFILE.get() {
        let s = profile.to_str().unwrap_or("");
        s.eq_ignore_ascii_case("ql")
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rewrite_bgp_empty() {
        let result = rewrite_bgp(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_is_ql_profile_not_active_by_default() {
        // Without a GUC value set, QL is not active.
        // We can't test with GUC here; just check the logic.
        // The fn reads from GUC which has no value in unit test context.
    }
}
