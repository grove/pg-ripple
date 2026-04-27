# Cookbook: Ontology Mapping and Alignment

**Goal.** You import data from an external dataset (Wikidata, schema.org, an industry vocabulary) and want it to align with your internal ontology — same concepts, different IRIs. Manual mapping is tedious; KGE-driven candidate generation plus SHACL gates plus owl:sameAs canonicalization makes the job tractable.

**Why pg_ripple.** Composes the same building blocks as record linkage, but applied to *classes* and *properties* instead of *individuals*.

**Time to first result.** ~25 minutes.

---

## The challenge

Your internal ontology calls a person `intkb:Employee`. Wikidata calls them `wd:Q5` (human). Schema.org calls them `schema:Person`. None of these is wrong — they describe overlapping but not identical concepts. You want SPARQL queries to *transparently* see them as the same class for retrieval, but you also want to *retain* the source-specific axioms.

---

## Step 1 — Load all three vocabularies

```sql
-- Internal ontology.
SELECT pg_ripple.load_turtle_into_graph('https://example.org/ontologies/intkb', $TTL$
@prefix intkb: <https://intkb.example/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .

intkb:Employee  rdfs:label "Employee" ;
                rdfs:comment "An employee of the organisation" .
intkb:Department rdfs:label "Department" .
intkb:reports   rdfs:label "reports to" .
$TTL$);

-- Schema.org subset.
SELECT pg_ripple.load_turtle_file_into_graph(
    'https://example.org/ontologies/schemaorg', '/data/schemaorg-people.ttl');

-- Wikidata subset.
SELECT pg_ripple.load_turtle_file_into_graph(
    'https://example.org/ontologies/wikidata',  '/data/wikidata-people.ttl');
```

## Step 2 — Train KGE across all three

```sql
SET pg_ripple.kge_enabled = on;
SELECT pg_ripple.kge_train(model := 'RotatE', dimensions := 200, epochs := 200);
```

RotatE is preferred for ontology alignment because it captures inverse and symmetric patterns common in `owl:inverseOf` axioms.

## Step 3 — Generate candidate alignments

```sql
SELECT * FROM pg_ripple.find_alignments(
    source_graph := 'https://example.org/ontologies/intkb',
    target_graph := 'https://example.org/ontologies/schemaorg',
    threshold    := 0.85
)
ORDER BY similarity DESC;

-- Same against Wikidata.
SELECT * FROM pg_ripple.find_alignments(
    source_graph := 'https://example.org/ontologies/intkb',
    target_graph := 'https://example.org/ontologies/wikidata',
    threshold    := 0.85
)
ORDER BY similarity DESC;
```

## Step 4 — Gate with structural SHACL

You only want to align *classes* with *classes* and *properties* with *properties*. A SHACL shape blocks cross-kind alignment errors:

```sql
SELECT pg_ripple.load_shacl($TTL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

# An owl:equivalentClass link must point Class → Class.
[] a sh:NodeShape ;
   sh:targetSubjectsOf owl:equivalentClass ;
   sh:property [ sh:path rdf:type ; sh:hasValue owl:Class ] .

# An owl:equivalentProperty link must point Property → Property.
[] a sh:NodeShape ;
   sh:targetSubjectsOf owl:equivalentProperty ;
   sh:property [ sh:path rdf:type ;
                 sh:in ( owl:ObjectProperty owl:DatatypeProperty owl:AnnotationProperty ) ] .
$TTL$);

ALTER SYSTEM SET pg_ripple.shacl_mode = 'sync';
SELECT pg_reload_conf();
```

## Step 5 — Apply alignments

For *exact* equivalences, use `owl:equivalentClass` / `owl:equivalentProperty`. For *broader* relationships, use `rdfs:subClassOf` / `rdfs:subPropertyOf`. The OWL 2 RL rule set will then propagate inferences in both directions.

```sql
-- High-confidence exact equivalences.
SELECT pg_ripple.insert_triple(
    s1, '<http://www.w3.org/2002/07/owl#equivalentClass>', s2
)
FROM pg_ripple.find_alignments(
    'https://example.org/ontologies/intkb',
    'https://example.org/ontologies/schemaorg', 0.95);

-- Mid-confidence: subclass instead of equivalent.
SELECT pg_ripple.insert_triple(
    s1, '<http://www.w3.org/2000/01/rdf-schema#subClassOf>', s2
)
FROM pg_ripple.find_alignments(
    'https://example.org/ontologies/intkb',
    'https://example.org/ontologies/schemaorg', 0.85)
WHERE similarity < 0.95;
```

## Step 6 — Materialise the inference

```sql
SELECT pg_ripple.load_rules_builtin('owl-rl');
SELECT pg_ripple.infer('owl-rl');
```

After this, a SPARQL query for `?p a schema:Person` returns every entity that is `intkb:Employee` (and vice versa for the equivalent classes) with no application-level glue.

---

## Choosing equivalence vs subclass

| Confidence | Suggested axiom |
|---|---|
| ≥ 0.95 | `owl:equivalentClass` / `owl:equivalentProperty` |
| 0.85 – 0.95 | `rdfs:subClassOf` (one-way generalisation) |
| < 0.85 | Reject; surface to a human ontologist |

A bidirectional `owl:equivalentClass` is a strong claim — both classes have *exactly* the same instances. If the source vocabularies disagree on edge cases (e.g. *minor employee* vs *human*), the weaker `rdfs:subClassOf` is safer.

---

## See also

- [Knowledge-Graph Embeddings](../features/knowledge-graph-embeddings.md)
- [OWL 2 Profiles](../features/owl-profiles.md)
- [Record Linkage](../features/record-linkage.md) — the *individual* counterpart to ontology alignment.
