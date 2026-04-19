# §2.4 Validating Data Quality

## What and Why

Storing knowledge is only half the battle — you also need to ensure it is **correct**.
SHACL (Shapes Constraint Language) is the W3C standard for declaring and validating
constraints on RDF data. It answers questions like:

- Does every paper have at least one author?
- Are all email addresses syntactically valid?
- Does every person have exactly one name?
- Are date values well-formed?

pg_ripple integrates SHACL validation directly into the database engine. You can validate
on demand, enforce constraints synchronously on every insert, or queue triples for
asynchronous background validation with violations routed to a dead-letter queue.

```admonish note
SHACL is to RDF what CHECK constraints and triggers are to relational databases — but
SHACL shapes are declarative, composable, and standardized across all RDF systems.
```

---

## How It Works

### The SHACL Model

A SHACL **shape** declares constraints on a set of **focus nodes** (entities matching a
target pattern). Each shape contains one or more **property shapes** that constrain the
values of a specific predicate.

```
NodeShape (target: instances of bibo:AcademicArticle)
  └─ PropertyShape (path: dct:title)
       ├─ sh:minCount 1     ← every paper must have at least one title
       ├─ sh:maxCount 1     ← at most one title
       └─ sh:datatype xsd:string  ← title must be a string
```

### Validation Modes

| Mode | GUC setting | Behavior |
|---|---|---|
| **Off** (default) | `pg_ripple.shacl_mode = 'off'` | No automatic validation |
| **Sync** | `pg_ripple.shacl_mode = 'sync'` | Every `insert_triple()` is validated before commit; violations raise an ERROR |
| **Async** | `pg_ripple.shacl_mode = 'async'` | Triples are inserted immediately; a background worker validates and routes violations to the dead-letter queue |

### Supported Constraints

| Constraint | Description |
|---|---|
| `sh:minCount` | Minimum number of values |
| `sh:maxCount` | Maximum number of values |
| `sh:datatype` | Value must have a specific XSD datatype |
| `sh:class` | Value must be an instance of a class |
| `sh:in` | Value must be from an enumerated set |
| `sh:pattern` | Value must match a regex |
| `sh:node` | Value must conform to another shape |
| `sh:or` | Value must conform to at least one of several shapes |
| `sh:and` | Value must conform to all listed shapes |
| `sh:not` | Value must NOT conform to a shape |
| `sh:qualifiedValueShape` | Qualified cardinality constraints |
| `sh:hasValue` | At least one value must equal the given term |
| `sh:nodeKind` | Value must be IRI, blank node, or literal |
| `sh:languageIn` | Language tag must be in the allowed list |
| `sh:uniqueLang` | No duplicate language tags |
| `sh:lessThan` / `sh:greaterThan` | Comparative constraints between properties |
| `sh:closed` | Reject unknown predicates |

---

## Worked Examples

### Loading Simple Shapes

Define shapes for the bibliographic dataset:

```sql
SELECT pg_ripple.load_shacl('
@prefix sh:     <http://www.w3.org/ns/shacl#> .
@prefix xsd:    <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:     <https://example.org/> .
@prefix dct:    <http://purl.org/dc/terms/> .
@prefix bibo:   <http://purl.org/ontology/bibo/> .
@prefix foaf:   <http://xmlns.com/foaf/0.1/> .
@prefix schema: <https://schema.org/> .

ex:PaperShape a sh:NodeShape ;
    sh:targetClass bibo:AcademicArticle ;
    sh:property [
        sh:path dct:title ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:datatype xsd:string ;
    ] ;
    sh:property [
        sh:path dct:creator ;
        sh:minCount 1 ;
        sh:class foaf:Person ;
    ] .

ex:PersonShape a sh:NodeShape ;
    sh:targetClass foaf:Person ;
    sh:property [
        sh:path foaf:name ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:datatype xsd:string ;
    ] ;
    sh:property [
        sh:path schema:affiliation ;
        sh:maxCount 1 ;
        sh:nodeKind sh:IRI ;
    ] .
');
-- Returns: 2 (number of shapes loaded)
```

### Running Validation

Validate the default graph against all active shapes:

```sql
SELECT pg_ripple.validate();
```

The result is a JSONB validation report:

```json
{
  "conforms": false,
  "violations": [
    {
      "focusNode": "<https://example.org/paper/99>",
      "shapeIRI": "<https://example.org/PaperShape>",
      "path": "<http://purl.org/dc/terms/creator>",
      "constraint": "sh:class",
      "message": "value <https://example.org/person/carol> is not an instance of <http://xmlns.com/foaf/0.1/Person>",
      "severity": "sh:Violation"
    }
  ]
}
```

Validate a specific named graph:

```sql
SELECT pg_ripple.validate('https://example.org/graph/pubmed');
```

Validate all graphs at once:

```sql
SELECT pg_ripple.validate('*');
```

### Synchronous Validation

Enable sync mode so invalid triples are rejected at insert time:

```sql
SET pg_ripple.shacl_mode = 'sync';

-- This succeeds (paper has a title)
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/700>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://purl.org/ontology/bibo/AcademicArticle>'
);

-- This would fail if the shape requires dct:title and the paper doesn't have one yet
-- (sync validation checks per-triple, not transactionally)
```

```admonish warning
Synchronous validation adds overhead to every `insert_triple()` call. Use it for
low-volume, high-integrity scenarios. For bulk loads, use `'off'` mode and validate
after loading.
```

### Asynchronous Validation with Dead-Letter Queue

Enable async mode for high-throughput pipelines:

```sql
SET pg_ripple.shacl_mode = 'async';

-- Triples are inserted immediately; validation happens in the background
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/800>',
    '<http://purl.org/dc/terms/title>',
    '"A New Paper"'
);

-- Check the validation queue length
SELECT pg_ripple.validation_queue_length();

-- Manually process the queue (normally handled by background worker)
SELECT pg_ripple.process_validation_queue(1000);

-- Check for violations
SELECT pg_ripple.dead_letter_count();

-- View the full dead-letter queue
SELECT pg_ripple.dead_letter_queue();
```

### Complex Shapes

**Disjunctive constraints (sh:or)**:

```sql
SELECT pg_ripple.load_shacl('
@prefix sh:   <http://www.w3.org/ns/shacl#> .
@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:   <https://example.org/> .
@prefix dct:  <http://purl.org/dc/terms/> .

ex:DateShape a sh:NodeShape ;
    sh:targetSubjectsOf dct:date ;
    sh:property [
        sh:path dct:date ;
        sh:or (
            [ sh:datatype xsd:date ]
            [ sh:datatype xsd:gYear ]
            [ sh:datatype xsd:dateTime ]
        ) ;
    ] .
');
```

**Closed shapes (reject unknown predicates)**:

```sql
SELECT pg_ripple.load_shacl('
@prefix sh:     <http://www.w3.org/ns/shacl#> .
@prefix ex:     <https://example.org/> .
@prefix dct:    <http://purl.org/dc/terms/> .
@prefix bibo:   <http://purl.org/ontology/bibo/> .
@prefix schema: <https://schema.org/> .

ex:StrictPaperShape a sh:NodeShape ;
    sh:targetClass bibo:AcademicArticle ;
    sh:closed true ;
    sh:ignoredProperties (
        <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>
    ) ;
    sh:property [
        sh:path dct:title ;
        sh:minCount 1 ;
    ] ;
    sh:property [
        sh:path dct:creator ;
        sh:minCount 1 ;
    ] ;
    sh:property [
        sh:path dct:date ;
        sh:maxCount 1 ;
    ] ;
    sh:property [
        sh:path schema:keywords ;
    ] ;
    sh:property [
        sh:path bibo:cites ;
    ] ;
    sh:property [
        sh:path bibo:citedBy ;
    ] .
');
```

**Qualified cardinality**:

```sql
SELECT pg_ripple.load_shacl('
@prefix sh:     <http://www.w3.org/ns/shacl#> .
@prefix ex:     <https://example.org/> .
@prefix dct:    <http://purl.org/dc/terms/> .
@prefix bibo:   <http://purl.org/ontology/bibo/> .
@prefix foaf:   <http://xmlns.com/foaf/0.1/> .

ex:CollabPaperShape a sh:NodeShape ;
    sh:targetClass bibo:AcademicArticle ;
    sh:property [
        sh:path dct:creator ;
        sh:qualifiedValueShape [
            sh:class foaf:Person ;
        ] ;
        sh:qualifiedMinCount 2 ;
    ] .
');
```

### Managing Shapes

```sql
-- List all loaded shapes
SELECT * FROM pg_ripple.list_shapes();

-- Deactivate a shape without deleting it
SELECT pg_ripple.disable_rule_set('custom');

-- Remove a shape entirely
SELECT pg_ripple.drop_shape('https://example.org/StrictPaperShape');
```

### SHACL DAG Monitors

For real-time violation detection, enable DAG monitors (requires pg_trickle):

```sql
-- Load shapes first
SELECT pg_ripple.load_shacl('...');

-- Enable per-shape violation stream tables
SELECT pg_ripple.enable_shacl_dag_monitors();

-- View the live violation summary
SELECT * FROM _pg_ripple.violation_summary_dag;

-- List active monitors
SELECT * FROM pg_ripple.list_shacl_dag_monitors();

-- Disable when no longer needed
SELECT pg_ripple.disable_shacl_dag_monitors();
```

---

## Common Patterns

### Pattern: Validate After Bulk Load

The most common workflow — load first, validate second:

```sql
-- Turn off validation during load
SET pg_ripple.shacl_mode = 'off';

-- Load data
SELECT pg_ripple.load_turtle_file('/data/papers.ttl');

-- Load shapes
SELECT pg_ripple.load_shacl('...');

-- Validate
SELECT pg_ripple.validate();
```

### Pattern: Data Quality Dashboard

Use the dead-letter queue as a data quality monitor:

```sql
-- Enable async validation
SET pg_ripple.shacl_mode = 'async';

-- Periodically check violation counts
SELECT pg_ripple.dead_letter_count();

-- Get violation details
SELECT pg_ripple.dead_letter_queue();

-- With pg_trickle: enable violation summary stream table
SELECT pg_ripple.enable_shacl_monitors();
SELECT * FROM _pg_ripple.violation_summary;
```

### Pattern: Embedding Completeness Check

Ensure all entities have vector embeddings (see [§2.7](../features/ai-retrieval-graph-rag.md)):

```sql
SELECT pg_ripple.load_shacl('
@prefix sh:   <http://www.w3.org/ns/shacl#> .
@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:   <https://example.org/> .
@prefix pg:   <urn:pg_ripple:> .
@prefix bibo: <http://purl.org/ontology/bibo/> .

ex:EmbeddingCompletenessShape a sh:NodeShape ;
    sh:targetClass bibo:AcademicArticle ;
    sh:property [
        sh:path pg:hasEmbedding ;
        sh:minCount 1 ;
        sh:hasValue "true"^^xsd:boolean ;
    ] .
');

-- Add embedding triples for entities that have been embedded
SELECT pg_ripple.add_embedding_triples();

-- Check completeness
SELECT pg_ripple.validate();
```

### Pattern: Multi-Language Support

Ensure labels exist in required languages:

```sql
SELECT pg_ripple.load_shacl('
@prefix sh:   <http://www.w3.org/ns/shacl#> .
@prefix ex:   <https://example.org/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

ex:LabelShape a sh:NodeShape ;
    sh:targetSubjectsOf rdfs:label ;
    sh:property [
        sh:path rdfs:label ;
        sh:languageIn ( "en" "de" "fr" ) ;
        sh:uniqueLang true ;
    ] .
');
```

---

## Performance and Trade-offs

| Validation mode | Overhead | Data integrity | Use case |
|---|---|---|---|
| `off` | None | Manual check with `validate()` | Bulk loads, development |
| `sync` | High (per-triple check) | Immediate rejection | Low-volume critical data |
| `async` | Low (background worker) | Eventual (violations in DLQ) | High-throughput pipelines |

- **Shape count**: validation time scales linearly with the number of active shapes and
  focus nodes. Deactivate shapes you do not need.
- **DAG monitors**: per-shape stream tables are `IMMEDIATE` mode — violations are detected
  within the same transaction. But pg_trickle must be installed.
- **Dead-letter queue**: grows without bound. Periodically review and clean it:
  ```sql
  -- Remove violations older than 30 days
  DELETE FROM _pg_ripple.dead_letter_queue
  WHERE detected_at < NOW() - INTERVAL '30 days';
  ```

```admonish tip
Shapes with `sh:maxCount 1` allow the SPARQL query engine to omit `DISTINCT` on that
predicate's joins. Shapes with `sh:minCount 1` allow downgrading `LEFT JOIN` to
`INNER JOIN`. Declaring accurate shapes improves both data quality and query performance.
```

---

## Gotchas and Debugging

### Shape Loading Errors

If `load_shacl()` returns 0, the Turtle may have syntax errors. Check for:

- Missing `@prefix` declarations
- Unclosed brackets in blank node property lists
- Missing semicolons between property shapes

### Sync Mode and Transaction Boundaries

Sync validation checks individual triples, not entire transactions. A paper might pass
the `dct:creator` check (because the triple being inserted is the author link) but fail
the `dct:title` check because the title has not been inserted yet in the same transaction.

Solution: insert all triples for an entity, then validate explicitly:

```sql
SET pg_ripple.shacl_mode = 'off';

-- Insert all triples for the entity
SELECT pg_ripple.insert_triple('<https://example.org/paper/900>', '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>', '<http://purl.org/ontology/bibo/AcademicArticle>');
SELECT pg_ripple.insert_triple('<https://example.org/paper/900>', '<http://purl.org/dc/terms/title>', '"My Paper"');
SELECT pg_ripple.insert_triple('<https://example.org/paper/900>', '<http://purl.org/dc/terms/creator>', '<https://example.org/person/alice>');

-- Then validate
SELECT pg_ripple.validate();
```

### Viewing Shape Definitions

```sql
-- List all shapes and their active status
SELECT * FROM pg_ripple.list_shapes();
```

### Validation Report Interpretation

The validation report JSONB has two top-level keys:

- `conforms`: `true` if no violations were found
- `violations`: array of violation objects, each with `focusNode`, `shapeIRI`, `path`, `constraint`, `message`, and `severity`

```sql
-- Extract just the violation messages
SELECT v->>'message'
FROM jsonb_array_elements(
    (SELECT pg_ripple.validate()::jsonb -> 'violations')
) AS v;
```

---

## Next Steps

- **[§2.5 Reasoning and Inference](../features/reasoning-and-inference.md)** — derive new facts from rules; SHACL shapes interact with inference.
- **[§2.6 Exporting and Sharing](../features/exporting-and-sharing.md)** — SHACL quality enforcement for GraphRAG exports.
- **[§2.7 AI Retrieval and GraphRAG](../features/ai-retrieval-graph-rag.md)** — embedding completeness shapes.
