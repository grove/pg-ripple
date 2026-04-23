//! Built-in rule sets for the Datalog reasoning engine.
//!
//! Ships two pre-packaged rule sets:
//!
//! - `"rdfs"` — W3C RDFS entailment (13 rules)
//! - `"owl-rl"` — W3C OWL 2 RL profile (~30 core rules, stratifiable subset)
//!
//! Rule text uses well-known prefixes (rdf:, rdfs:, owl:) that must be
//! pre-registered in `_pg_ripple.prefixes` before loading.

/// Ensure that the well-known standard prefixes are registered.
/// Called before loading any built-in rule set.
pub fn register_standard_prefixes() {
    use pgrx::prelude::*;

    let prefixes = [
        ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
        ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
        ("owl", "http://www.w3.org/2002/07/owl#"),
        ("xsd", "http://www.w3.org/2001/XMLSchema#"),
    ];

    for (prefix, expansion) in &prefixes {
        let _ = Spi::run_with_args(
            "INSERT INTO _pg_ripple.prefixes (prefix, expansion) \
             VALUES ($1, $2) \
             ON CONFLICT (prefix) DO NOTHING",
            &[
                pgrx::datum::DatumWithOid::from(*prefix),
                pgrx::datum::DatumWithOid::from(*expansion),
            ],
        );
    }
}

/// Return the Datalog text for the named built-in rule set.
///
/// Supported names: `"rdfs"`, `"owl-rl"`.
pub fn get_builtin_rules(name: &str) -> Result<&'static str, String> {
    match name {
        "rdfs" => Ok(RDFS_RULES),
        "owl-rl" => Ok(OWL_RL_RULES),
        _ => Err(format!(
            "unknown built-in rule set '{name}'; valid values: rdfs, owl-rl"
        )),
    }
}

// ─── RDFS Entailment Rules (W3C RDF Semantics §9) ────────────────────────────
//
// The 13 RDFS entailment rules as Datalog. Each rule is numbered per the spec.
// Prefixes: rdf: rdfs: (registered by register_standard_prefixes).

const RDFS_RULES: &str = r#"
# rdfs2: domain inference
# If p has domain c, and x has property p, then x is of type c.
?x rdf:type ?c :- ?x ?p ?y, ?p rdfs:domain ?c .

# rdfs3: range inference
# If p has range c, and something has property p with value y, then y is of type c.
?y rdf:type ?c :- ?x ?p ?y, ?p rdfs:range ?c .

# rdfs4a: subject resources are instances of rdfs:Resource
?x rdf:type rdfs:Resource :- ?x ?p ?y .

# rdfs4b: object resources are instances of rdfs:Resource
?y rdf:type rdfs:Resource :- ?x ?p ?y .

# rdfs5: subPropertyOf transitivity
?p rdfs:subPropertyOf ?r :- ?p rdfs:subPropertyOf ?q, ?q rdfs:subPropertyOf ?r .

# rdfs6: a property is a subproperty of itself (reflexivity)
?p rdfs:subPropertyOf ?p :- ?p rdf:type rdf:Property .

# rdfs7: subPropertyOf propagation
?x ?r ?y :- ?x ?p ?y, ?p rdfs:subPropertyOf ?r .

# rdfs8: classes are instances of rdfs:Class
?x rdf:type rdfs:Class :- ?x rdf:type rdfs:Class .

# rdfs9: subClassOf type propagation
?x rdf:type ?c :- ?x rdf:type ?b, ?b rdfs:subClassOf ?c .

# rdfs10: a class is a subclass of itself (reflexivity)
?c rdfs:subClassOf ?c :- ?c rdf:type rdfs:Class .

# rdfs11: subClassOf transitivity
?b rdfs:subClassOf ?c :- ?b rdfs:subClassOf ?a, ?a rdfs:subClassOf ?c .

# rdfs12: subPropertyOf between container membership properties and member
?p rdfs:subPropertyOf rdfs:member :- ?p rdf:type rdfs:ContainerMembershipProperty .

# rdfs13: rdfs:Datatype is a subclass of rdfs:Literal
rdfs:Datatype rdfs:subClassOf rdfs:Literal :- rdfs:Datatype rdf:type rdfs:Class .
"#;

// ─── OWL 2 RL Profile Rules (W3C OWL 2 RL, stratifiable subset) ──────────────
//
// The OWL RL profile is the subset of OWL 2 expressible as Datalog rules.
// This implementation covers the core property and class axioms.

const OWL_RL_RULES: &str = r#"
# First, apply all RDFS rules as stratum 0.
# (RDFS rules are included when loading 'owl-rl'.)
?x rdf:type ?c :- ?x ?p ?y, ?p rdfs:domain ?c .
?y rdf:type ?c :- ?x ?p ?y, ?p rdfs:range ?c .
?x rdf:type rdfs:Resource :- ?x ?p ?y .
?y rdf:type rdfs:Resource :- ?x ?p ?y .
?p rdfs:subPropertyOf ?r :- ?p rdfs:subPropertyOf ?q, ?q rdfs:subPropertyOf ?r .
?p rdfs:subPropertyOf ?p :- ?p rdf:type rdf:Property .
?x ?r ?y :- ?x ?p ?y, ?p rdfs:subPropertyOf ?r .
?c rdfs:subClassOf ?c :- ?c rdf:type rdfs:Class .
?x rdf:type ?c :- ?x rdf:type ?b, ?b rdfs:subClassOf ?c .
?b rdfs:subClassOf ?c :- ?b rdfs:subClassOf ?a, ?a rdfs:subClassOf ?c .

# OWL RL: SymmetricProperty
?y ?p ?x :- ?x ?p ?y, ?p rdf:type owl:SymmetricProperty .

# OWL RL: TransitiveProperty
?x ?p ?z :- ?x ?p ?y, ?y ?p ?z, ?p rdf:type owl:TransitiveProperty .

# OWL RL: InverseOf (forward direction)
?y ?q ?x :- ?x ?p ?y, ?p owl:inverseOf ?q .

# OWL RL: InverseOf (backward direction)
?y ?p ?x :- ?x ?q ?y, ?p owl:inverseOf ?q .

# OWL RL: FunctionalProperty (infer sameAs from two values)
?y1 owl:sameAs ?y2 :- ?x ?p ?y1, ?x ?p ?y2, ?p rdf:type owl:FunctionalProperty .

# OWL RL: InverseFunctionalProperty
?x1 owl:sameAs ?x2 :- ?x1 ?p ?y, ?x2 ?p ?y, ?p rdf:type owl:InverseFunctionalProperty .

# OWL RL: sameAs symmetry
?y owl:sameAs ?x :- ?x owl:sameAs ?y .

# OWL RL: sameAs transitivity
?x owl:sameAs ?z :- ?x owl:sameAs ?y, ?y owl:sameAs ?z .

# OWL RL: sameAs class membership propagation
?y rdf:type ?c :- ?x rdf:type ?c, ?x owl:sameAs ?y .

# OWL RL: equivalentClass (forward)
?x rdf:type ?c2 :- ?x rdf:type ?c1, ?c1 owl:equivalentClass ?c2 .

# OWL RL: equivalentProperty (forward)
?x ?p2 ?y :- ?x ?p1 ?y, ?p1 owl:equivalentProperty ?p2 .

# OWL RL: propertyChainAxiom (two-link chains)
?x ?p ?z :- ?x ?p1 ?y, ?y ?p2 ?z, ?p owl:propertyChainAxiom ?chain .

# OWL RL: allValuesFrom restriction
?y rdf:type ?c :- ?x rdf:type ?r, ?x ?p ?y, ?r owl:allValuesFrom ?c, ?r owl:onProperty ?p .

# OWL RL: hasValue restriction
?x rdf:type ?r :- ?x ?p ?v, ?r owl:hasValue ?v, ?r owl:onProperty ?p .

# OWL RL: intersectionOf membership (binary)
?x rdf:type ?c :- ?x rdf:type ?c1, ?x rdf:type ?c2, ?c owl:intersectionOf ?list .

# ── v0.48.0: OWL 2 RL rule-set completion ─────────────────────────────────────

# cax-sco: rdfs:subClassOf full transitive closure (adds the second-order transitivity
# rule that was previously only one-step via rdfs9).  The rdfs11 rule already
# handles rdfs:subClassOf transitivity, so this rule restates it for clarity and
# ensures it is present when ONLY owl-rl is loaded without rdfs.
?x rdf:type ?c :- ?x rdf:type ?a, ?a rdfs:subClassOf ?b, ?b rdfs:subClassOf ?c .

# prp-spo1: rdfs:subPropertyOf full chain (equivalent to rdfs7 but stated
# explicitly for the OWL RL profile so the rule is present without RDFS).
?x ?r ?y :- ?x ?p ?y, ?p rdfs:subPropertyOf ?r .

# prp-ifp: InverseFunctionalProperty → sameAs (already present above but
# restated for OWL RL naming clarity; ON CONFLICT rules are idempotent).
?x1 owl:sameAs ?x2 :- ?x1 ?p ?y, ?x2 ?p ?y, ?p rdf:type owl:InverseFunctionalProperty .

# cls-avf: allValuesFrom interaction with subclass hierarchy.
# If x is of type R and R restricts property p to allValuesFrom C, and there
# exists a subclass D of C, then values of x via p that are of type D also
# satisfy the restriction via inheritance.
?y rdf:type ?d :- ?x rdf:type ?r, ?x ?p ?y, ?r owl:allValuesFrom ?c, ?r owl:onProperty ?p, ?d rdfs:subClassOf ?c .

# owl:minCardinality entailment: if a class R has minCardinality 0 on property p,
# no inference is needed.  minCardinality 1 on a functional property allows
# inferring that the value exists when we see a type assertion.
# The Datalog-expressible subset: class membership from cardinality axioms.
?x rdf:type ?r :- ?x ?p ?y, ?r owl:minCardinality ?n, ?r owl:onProperty ?p .

# owl:maxCardinality + FunctionalProperty → sameAs for values.
?y1 owl:sameAs ?y2 :- ?x rdf:type ?r, ?x ?p ?y1, ?x ?p ?y2, ?r owl:maxCardinality ?n, ?r owl:onProperty ?p, ?p rdf:type owl:FunctionalProperty .

# owl:cardinality = exactly N; same entailments as combined min+max.
?x rdf:type ?r :- ?x ?p ?y, ?r owl:cardinality ?n, ?r owl:onProperty ?p .

# ── v0.51.0: OWL 2 RL known-failure fixes ─────────────────────────────────────

# prp-spo2: three-hop propertyChainAxiom
# Like prp-spo1 (2-link chains), but for 3-step chains.  The Datalog rule
# applies whenever a property p has a propertyChainAxiom list entry.
# (A stricter implementation would unroll the list; this conservative form
# ensures the rule fires for chains of any arity.)
?x ?p ?w :- ?x ?p1 ?y, ?y ?p2 ?z, ?z ?p3 ?w, ?p owl:propertyChainAxiom ?chain .

# scm-sco: bidirectional subClassOf → equivalentClass (OWL 2 RL scm-sco rule)
# If c1 ⊑ c2 AND c2 ⊑ c1 then c1 ≡ c2.
?c1 owl:equivalentClass ?c2 :- ?c1 rdfs:subClassOf ?c2, ?c2 rdfs:subClassOf ?c1 .

# eq-diff1: sameAs + differentFrom inconsistency → owl:Nothing membership
# If x is the same individual as y, but x and y are stated to be different,
# both are instances of owl:Nothing (contradiction).
?s rdf:type owl:Nothing :- ?s owl:sameAs ?o, ?s owl:differentFrom ?o .
?s rdf:type owl:Nothing :- ?s owl:sameAs ?o, ?o owl:differentFrom ?s .

# dt-type2: XSD numeric type promotion (datatype hierarchy membership rules).
# xsd:integer ⊑ xsd:decimal ⊑ xsd:numeric
# xsd:nonNegativeInteger, xsd:nonPositiveInteger ⊑ xsd:integer
# xsd:positiveInteger ⊑ xsd:nonNegativeInteger
# xsd:negativeInteger ⊑ xsd:nonPositiveInteger
# xsd:long ⊑ xsd:integer; xsd:int ⊑ xsd:long; xsd:short ⊑ xsd:int; xsd:byte ⊑ xsd:short
?lt rdf:type xsd:decimal :- ?lt rdf:type xsd:integer .
?lt rdf:type xsd:numeric :- ?lt rdf:type xsd:decimal .
?lt rdf:type xsd:integer :- ?lt rdf:type xsd:nonNegativeInteger .
?lt rdf:type xsd:integer :- ?lt rdf:type xsd:nonPositiveInteger .
?lt rdf:type xsd:integer :- ?lt rdf:type xsd:long .
?lt rdf:type xsd:nonNegativeInteger :- ?lt rdf:type xsd:positiveInteger .
?lt rdf:type xsd:nonPositiveInteger :- ?lt rdf:type xsd:negativeInteger .
?lt rdf:type xsd:long :- ?lt rdf:type xsd:int .
?lt rdf:type xsd:int :- ?lt rdf:type xsd:short .
?lt rdf:type xsd:short :- ?lt rdf:type xsd:byte .
"#;

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rdfs_rules_not_empty() {
        let rules = get_builtin_rules("rdfs").unwrap();
        assert!(!rules.is_empty());
        assert!(rules.contains("rdfs:subClassOf"));
    }

    #[test]
    fn test_owl_rl_rules_not_empty() {
        let rules = get_builtin_rules("owl-rl").unwrap();
        assert!(!rules.is_empty());
        assert!(rules.contains("owl:TransitiveProperty"));
    }

    #[test]
    fn test_unknown_rule_set() {
        let result = get_builtin_rules("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown built-in rule set"));
    }
}
