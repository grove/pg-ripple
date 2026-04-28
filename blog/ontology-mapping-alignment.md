[← Back to Blog Index](README.md)

# Automated Ontology Mapping: Aligning Vocabularies Without Manual Labor

## When your data uses schema.org and your ontology uses FHIR

---

You receive patient data using schema.org vocabulary. Your clinical ontology uses HL7 FHIR. The concepts are the same — "Patient", "name", "birthDate" — but the IRIs are different.

Mapping schema.org to FHIR by hand means reading both specifications, finding correspondences, writing SPARQL CONSTRUCT queries or Datalog rules, and maintaining the mapping as both vocabularies evolve.

pg_ripple automates the initial alignment and gives you tools to refine it.

---

## The Alignment Function

```sql
SELECT * FROM pg_ripple.suggest_mappings(
  source_graph => 'http://example.org/schema_data',
  target_graph => 'http://example.org/fhir_ontology',
  method       => 'lexical',
  threshold    => 0.7
);
```

Returns:

| source_class | target_class | similarity | method |
|-------------|-------------|-----------|--------|
| schema:Patient | fhir:Patient | 0.95 | lexical |
| schema:MedicalCondition | fhir:Condition | 0.82 | lexical |
| schema:Drug | fhir:Medication | 0.71 | lexical |

The function compares `rdfs:label` values across the two graphs using Jaccard similarity on tokenized labels. "MedicalCondition" and "Condition" share enough tokens to score 0.82. "Drug" and "Medication" are different words for the same concept — they score lower with lexical matching.

---

## KGE-Based Alignment

When lexical similarity isn't enough (different languages, different naming conventions), KGE-based alignment uses knowledge graph embeddings:

```sql
SELECT * FROM pg_ripple.suggest_mappings(
  source_graph => 'http://example.org/schema_data',
  target_graph => 'http://example.org/fhir_ontology',
  method       => 'embedding',
  threshold    => 0.75
);
```

This computes cosine similarity between entity embeddings from the two graphs. Entities that occupy similar positions in their respective graph structures (similar neighbors, similar types) align even if their labels are completely different.

"Drug" and "Medication" might score 0.89 with embedding alignment because both have similar relationships to dosage, side effects, and administration routes in their respective ontologies.

---

## Pre-Built Vocabulary Templates

For common vocabulary pairs, pg_ripple ships pre-built mapping templates:

```sql
-- Schema.org → SAREF (IoT ontology)
SELECT pg_ripple.apply_vocabulary_template('schema_to_saref');

-- Schema.org → FHIR (healthcare)
SELECT pg_ripple.apply_vocabulary_template('schema_to_fhir');

-- Schema.org → PROV-O (provenance)
SELECT pg_ripple.apply_vocabulary_template('schema_to_provo');
```

Each template is a curated set of `owl:equivalentClass` and `owl:equivalentProperty` triples, loaded into the graph. Combined with Datalog's OWL RL rules, these equivalences propagate automatically: if `schema:Patient owl:equivalentClass fhir:Patient`, then any `rdf:type schema:Patient` triple also makes the entity a `fhir:Patient`.

---

## From Suggestions to Rules

The alignment suggestions aren't rules — they're candidates. Review and approve them:

```sql
-- Review suggestions
SELECT * FROM pg_ripple.suggest_mappings(...);

-- Accept a mapping: create an equivalence assertion
SELECT pg_ripple.sparql_update('
  INSERT DATA {
    schema:Patient owl:equivalentClass fhir:Patient .
    schema:MedicalCondition owl:equivalentClass fhir:Condition .
  }
');

-- Run inference to propagate the equivalences
SELECT pg_ripple.datalog_infer();
```

After inference, SPARQL queries against FHIR classes automatically include schema.org data:

```sparql
SELECT ?patient ?name WHERE {
  ?patient rdf:type fhir:Patient ;
           foaf:name ?name .
}
-- Returns patients originally typed as schema:Patient too
```

---

## Property Mapping

Class alignment is half the story. Properties need mapping too:

```sql
SELECT * FROM pg_ripple.suggest_mappings(
  source_graph => 'http://example.org/schema_data',
  target_graph => 'http://example.org/fhir_ontology',
  method       => 'lexical',
  entity_type  => 'property'
);
```

| source_property | target_property | similarity |
|----------------|----------------|-----------|
| schema:name | fhir:name | 0.97 |
| schema:birthDate | fhir:birthDate | 0.97 |
| schema:address | fhir:address | 0.97 |
| schema:identifier | fhir:identifier | 0.95 |

Accept and materialize:

```sql
SELECT pg_ripple.sparql_update('
  INSERT DATA {
    schema:name owl:equivalentProperty fhir:name .
    schema:birthDate owl:equivalentProperty fhir:birthDate .
  }
');
```

---

## The Integration Pipeline

The full ontology mapping workflow:

1. **Load both vocabularies** into separate named graphs.
2. **Run `suggest_mappings()`** with both lexical and embedding methods.
3. **Review candidates** — accept good mappings, reject false positives.
4. **Create equivalence assertions** for accepted mappings.
5. **Run Datalog inference** to propagate equivalences.
6. **Validate with SHACL** — check that the merged schema is consistent.
7. **Query across vocabularies** — SPARQL sees the unified view.

Steps 2–3 take minutes instead of the days that manual mapping requires. The suggestions are starting points, not final answers — domain expertise is still needed for ambiguous cases.

---

## When Mapping Breaks Down

- **1:N mappings.** schema.org's `schema:address` is a single string; FHIR's address is a structured object with street, city, postal code. These can't be mapped with simple equivalence assertions — you need SPARQL CONSTRUCT rules to restructure the data.
- **Semantic drift.** Two properties with the same name can mean different things in different ontologies. "status" in a healthcare context vs. "status" in a project management context. Automated alignment catches the name match but not the semantic mismatch.
- **Missing coverage.** The source vocabulary might have concepts that don't exist in the target. These can't be mapped — they need to be extended or accepted as gaps.

For these cases, the alignment function provides a starting point and the human fills in the gaps. That's still faster than doing the entire mapping by hand.
