[← Back to Blog Index](README.md)

# RDF-star: Making Statements About Statements

## Metadata, provenance, and temporal annotations without the reification mess

---

"Alice knows Bob." That's a triple. Simple.

"Alice knows Bob, according to the HR system." Now you need metadata about the triple itself. Who said it? When? How confident are we?

In classic RDF, the only way to do this is reification — a pattern so verbose that it turns one triple into four:

```turtle
ex:stmt1 rdf:type rdf:Statement .
ex:stmt1 rdf:subject ex:alice .
ex:stmt1 rdf:predicate foaf:knows .
ex:stmt1 rdf:object ex:bob .
ex:stmt1 ex:source ex:hr_system .
ex:stmt1 ex:assertedOn "2024-01-15"^^xsd:date .
```

Six triples to say what should be one triple plus two annotations. The reified statement isn't connected to the original triple — it's a separate description that *refers to* the triple without being it. Queries become awkward, storage bloats 4×, and nobody actually uses it.

RDF-star fixes this.

---

## What RDF-star Is

RDF-star (also written RDF*) extends RDF to allow triples as subjects or objects of other triples:

```turtle
<< ex:alice foaf:knows ex:bob >> ex:source ex:hr_system .
<< ex:alice foaf:knows ex:bob >> ex:assertedOn "2024-01-15"^^xsd:date .
```

The `<< ... >>` syntax creates a *quoted triple* — a triple that can be referenced by other triples. Two triples instead of six. The quoted triple *is* the statement; the outer triples annotate it.

SPARQL-star extends SPARQL with the same syntax:

```sparql
SELECT ?source ?date WHERE {
  << ex:alice foaf:knows ?friend >> ex:source ?source ;
                                     ex:assertedOn ?date .
}
```

This finds all friends of Alice, along with the source and assertion date for each `knows` relationship.

---

## How pg_ripple Stores RDF-star

RDF-star quoted triples are tricky to store because they're *nested* — a triple can contain other triples, which can contain other triples, recursively.

pg_ripple stores quoted triples in the dictionary (since v0.4.0). A quoted triple `<< ex:alice foaf:knows ex:bob >>` is hashed and assigned a dictionary ID, just like any IRI or literal. The dictionary entry stores the three component IDs (subject, predicate, object) of the quoted triple:

```sql
_pg_ripple.dictionary (
  id     BIGINT PRIMARY KEY,
  value  TEXT,        -- serialized form for display
  kind   SMALLINT,    -- 3 = quoted triple
  qt_s   BIGINT,      -- subject component ID
  qt_p   BIGINT,      -- predicate component ID
  qt_o   BIGINT       -- object component ID
)
```

The outer triples that annotate the quoted triple use its dictionary ID as a normal subject or object in VP tables:

```
-- << ex:alice foaf:knows ex:bob >> has dictionary ID 9928371

-- In vp_{ex:source}:
s = 9928371, o = <ID of ex:hr_system>

-- In vp_{ex:assertedOn}:
s = 9928371, o = <ID of "2024-01-15"^^xsd:date>
```

The VP tables don't change at all. Quoted triples are just another kind of dictionary entry. This is the beauty of dictionary encoding — the storage layer doesn't need to know whether an ID represents an IRI, a literal, or a quoted triple.

---

## Querying RDF-star in pg_ripple

SPARQL-star queries work naturally:

```sql
SELECT * FROM pg_ripple.sparql('
  SELECT ?person ?friend ?source ?confidence WHERE {
    << ?person foaf:knows ?friend >> ex:source ?source ;
                                      ex:confidence ?confidence .
    FILTER(?confidence > 0.8)
  }
');
```

The translation pipeline handles the quoted triple pattern by:

1. Joining the annotation VP tables (ex:source, ex:confidence) to get the quoted triple IDs.
2. Looking up the quoted triple components in the dictionary (qt_s, qt_p, qt_o).
3. Binding `?person` to qt_s and `?friend` to qt_o.
4. Applying the filter on `?confidence`.
5. Decoding the final results.

The key optimization: if the quoted triple pattern specifies a fixed predicate (`foaf:knows`), the dictionary lookup can filter on `qt_p` first, drastically reducing the candidate set.

---

## Use Cases

### Provenance

"Where did this fact come from?"

```turtle
<< ex:patient42 ex:diagnosis ex:diabetes >> ex:source ex:lab_report_123 ;
                                             ex:diagnosedBy ex:dr_smith ;
                                             ex:diagnosedOn "2024-03-15"^^xsd:date .
```

Query: "Find all diagnoses made by Dr. Smith that are based on lab reports."

```sparql
SELECT ?patient ?diagnosis ?report WHERE {
  << ?patient ex:diagnosis ?diagnosis >> ex:diagnosedBy ex:dr_smith ;
                                         ex:source ?report .
  ?report rdf:type ex:LabReport .
}
```

### Temporal Annotations

"When was this relationship true?"

```turtle
<< ex:alice ex:worksAt ex:acme >> ex:from "2020-01-01"^^xsd:date ;
                                   ex:to "2023-06-30"^^xsd:date .
<< ex:alice ex:worksAt ex:globex >> ex:from "2023-07-01"^^xsd:date .
```

Query: "Where did Alice work as of March 2022?"

```sparql
SELECT ?company WHERE {
  << ex:alice ex:worksAt ?company >> ex:from ?start .
  OPTIONAL { << ex:alice ex:worksAt ?company >> ex:to ?end . }
  FILTER(?start <= "2022-03-01"^^xsd:date)
  FILTER(!BOUND(?end) || ?end > "2022-03-01"^^xsd:date)
}
```

### Confidence and Certainty

"How sure are we about this?"

```turtle
<< ex:companyA ex:competitor ex:companyB >> ex:confidence 0.92 ;
                                            ex:method "nlp_extraction" ;
                                            ex:extractedFrom ex:article_789 .
```

Query: "Find high-confidence competitor relationships extracted by NLP."

```sparql
SELECT ?company1 ?company2 ?confidence WHERE {
  << ?company1 ex:competitor ?company2 >> ex:confidence ?confidence ;
                                          ex:method "nlp_extraction" .
  FILTER(?confidence > 0.85)
}
ORDER BY DESC(?confidence)
```

### Access Control

"Who is allowed to see this fact?"

```turtle
<< ex:project42 ex:budget 5000000 >> ex:accessLevel "confidential" ;
                                      ex:allowedRoles "finance", "executive" .
```

Combined with PostgreSQL's row-level security, this enables per-triple access control — a feature that most triplestores don't support.

---

## Why Reification Is Dead

The old reification pattern has three fatal problems:

1. **Verbosity.** Every annotated triple requires 4+ reification triples. A graph with 1 million annotated facts needs 4+ million reification triples. Storage and query performance suffer proportionally.

2. **Disconnection.** The reified statement is a *description* of the triple, not the triple itself. If you query for `ex:alice foaf:knows ex:bob`, you get the triple. If you query for reified statements about the same relationship, you get a separate set of results. There's no built-in connection between the two.

3. **Query complexity.** Finding "Alice's friends with provenance" requires joining the triple pattern with the reification pattern:

   ```sparql
   -- With reification (ugly)
   SELECT ?friend ?source WHERE {
     ex:alice foaf:knows ?friend .
     ?stmt rdf:type rdf:Statement ;
           rdf:subject ex:alice ;
           rdf:predicate foaf:knows ;
           rdf:object ?friend ;
           ex:source ?source .
   }

   -- With RDF-star (clean)
   SELECT ?friend ?source WHERE {
     << ex:alice foaf:knows ?friend >> ex:source ?source .
   }
   ```

   The RDF-star version is shorter, clearer, and faster (fewer joins).

---

## Interaction with Other pg_ripple Features

### SHACL

SHACL shapes can constrain annotations on quoted triples:

```turtle
ex:AnnotatedFactShape a sh:NodeShape ;
  sh:targetObjectsOf rdf:type ;
  sh:property [
    sh:path ex:source ;
    sh:minCount 1 ;  # Every fact must have a source
  ] .
```

### Datalog

Datalog rules can derive annotations:

```
-- If a fact is asserted by two independent sources, mark it as verified
verified(Stmt) :- source(Stmt, S1), source(Stmt, S2), S1 != S2.
```

### JSON-LD

JSON-LD framing can embed annotations in the output:

```json
{
  "@context": { "knows": "foaf:knows" },
  "knows": {
    "@id": "ex:bob",
    "source": "ex:hr_system",
    "assertedOn": "2024-01-15"
  }
}
```

---

## The Storage Efficiency

Compared to reification, RDF-star in pg_ripple stores:

| Pattern | Triples stored | Dictionary entries |
|---------|---------------|-------------------|
| One fact, two annotations (reification) | 6 | ~10 |
| One fact, two annotations (RDF-star) | 2 | ~7 (including quoted triple) |

The 3× reduction in triple count translates directly to smaller VP tables, fewer index entries, and faster queries.

For datasets with heavy annotation (provenance tracking, temporal data, multi-source integration), the savings compound. A graph with 1 million facts, each with 3 annotations, stores 3 million triples with RDF-star versus 7 million with reification. That's a meaningful difference in storage cost and query performance.

RDF-star is the way to attach metadata to RDF facts. pg_ripple has supported it since v0.4.0. If you're still using reification, stop.
