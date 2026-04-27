# OWL 2 Profiles — RL, EL, QL

OWL 2 is too expressive to evaluate in full inside a database — full OWL is undecidable for some queries. The W3C therefore standardised three **OWL 2 profiles** that trade expressiveness for tractability. pg_ripple ships built-in rule sets and query rewriters for all three.

| Profile | Best for | Tractability | Loaded with |
|---|---|---|---|
| **OWL 2 RL** | General-purpose enterprise reasoning, RDFS-on-steroids | Polynomial-time materialisation | `load_rules_builtin('owl-rl')` |
| **OWL 2 EL** | Large terminological hierarchies (medical ontologies, SNOMED) | Polynomial-time classification | `load_rules_builtin('owl-el')` |
| **OWL 2 QL** | Read-heavy access over an ontology, query rewriting | Sub-polynomial query answering | `load_rules_builtin('owl-ql')` |

A single GUC selects the active profile for new connections:

```sql
ALTER SYSTEM SET pg_ripple.owl_profile = 'rl';   -- 'rl' | 'el' | 'ql' | 'off'
SELECT pg_reload_conf();
```

Setting `owl_profile = 'off'` disables ontology rewriting — useful when you want to inspect raw triples without inference smoothing.

---

## OWL 2 RL — the workhorse

OWL 2 RL is the profile most users want. It covers:

- Class hierarchy: `rdfs:subClassOf`, `owl:equivalentClass`, `owl:disjointWith`
- Property hierarchy: `rdfs:subPropertyOf`, `owl:equivalentProperty`, `owl:propertyChainAxiom`
- Property characteristics: `owl:TransitiveProperty`, `owl:SymmetricProperty`, `owl:InverseOf`, `owl:FunctionalProperty`, `owl:InverseFunctionalProperty`
- Class constructors: `owl:unionOf`, `owl:intersectionOf`, `owl:hasValue`, `owl:someValuesFrom` (in restricted positions)
- `owl:sameAs` and `owl:differentFrom`

Run it with:

```sql
SELECT pg_ripple.load_rules_builtin('owl-rl');
SELECT pg_ripple.infer('owl-rl');
```

Performance: the OWL 2 RL rule set has ~80 rules. On a 10 M-triple graph with a typical 1:1 T-Box / A-Box ratio, a full materialisation takes seconds with parallel stratum evaluation enabled.

pg_ripple is **100 % conformant** with the W3C OWL 2 RL test suite — see [OWL 2 RL Conformance Results](../reference/owl2rl-results.md).

---

## OWL 2 EL — for large terminologies

EL was designed for ontologies with very large class hierarchies and few individuals — medical terminologies (SNOMED, NCIt), gene ontologies, product taxonomies. It supports:

- Class subsumption with existential restrictions: `owl:someValuesFrom`
- Property chains: e.g. `partOf ∘ partOf ⊑ partOf`
- `owl:hasSelf`
- Reflexive properties

EL classification (computing the full subclass hierarchy) is polynomial in the size of the ontology. pg_ripple's EL implementation uses a saturation-based algorithm and stores the closure in `_pg_ripple.el_classified`.

```sql
SELECT pg_ripple.load_rules_builtin('owl-el');
SELECT pg_ripple.infer('owl-el');

-- All classes that subsume :BacterialPneumonia.
SELECT * FROM pg_ripple.sparql('
    SELECT ?super WHERE { <https://example.org/BacterialPneumonia>
                          <http://www.w3.org/2000/01/rdf-schema#subClassOf>+ ?super }
');
```

---

## OWL 2 QL — for query rewriting

QL is the profile of choice when you want to answer SPARQL queries over an ontology *without* materialising inferences first. Instead of expanding the data, pg_ripple **rewrites the query** at translation time using the ontology axioms. This keeps the data store small and lets you change the ontology without re-materialising.

QL supports a deliberately small set of constructs — `rdfs:subClassOf`, `rdfs:subPropertyOf`, `owl:inverseOf`, `owl:someValuesFrom` (in object position only), `owl:disjointWith`. The trade-off is that everything is fast.

```sql
SELECT pg_ripple.load_rules_builtin('owl-ql');
SET pg_ripple.owl_profile = 'ql';

-- This SELECT is rewritten using subClassOf axioms before execution.
SELECT * FROM pg_ripple.sparql('
    SELECT ?animal WHERE { ?animal a <https://example.org/Mammal> }
');
```

If `Dog rdfs:subClassOf Mammal`, the rewriter expands `Mammal` into `(Mammal | Dog | Cat | …)` automatically — without inserting any new triples.

---

## SPARQL-DL — direct OWL axiom queries

When you want to query the *T-Box* itself (the ontology, not the data), pg_ripple exposes two SPARQL-DL helpers:

```sql
-- Direct subclasses of :Mammal.
SELECT * FROM pg_ripple.sparql_dl_subclasses('<https://example.org/Mammal>');

-- All superclasses of :Dog.
SELECT * FROM pg_ripple.sparql_dl_superclasses('<https://example.org/Dog>');
```

These route OWL vocabulary BGPs (`owl:subClassOf`, `owl:equivalentClass`, `owl:disjointWith`) directly to T-Box VP tables — no separate index required.

---

## Choosing a profile

| Question | Profile |
|---|---|
| I just want SPARQL queries to "see" RDFS+ inference | **RL** (default) |
| I have a million-class taxonomy and need fast classification | **EL** |
| I have an ontology and a tiny data store, and want to skip materialisation | **QL** |
| I'm not sure | **RL** |

You can also load multiple profiles' rule sets at once and run them under different rule-set names — they are independent.

---

## See also

- [Reasoning & Inference](reasoning-and-inference.md) — the Datalog engine that powers all three profiles.
- [OWL 2 RL Conformance Results](../reference/owl2rl-results.md)
- [SPARQL Compliance Matrix](../reference/sparql-compliance.md)
