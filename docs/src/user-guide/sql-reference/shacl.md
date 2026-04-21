# SHACL Validation

pg_ripple v0.7.0 adds **SHACL Core** — the W3C standard for expressing data quality rules over RDF graphs. Rules are loaded from Turtle, stored in the database, and can be enforced inline at insert time or evaluated on demand.

---

## Quick Start

```sql
-- 1. Load shapes from Turtle
SELECT pg_ripple.load_shacl($SHACL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <https://example.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:PersonShape
    a sh:NodeShape ;
    sh:targetClass ex:Person ;
    sh:property [
        sh:path ex:name ;
        sh:minCount 1 ;
        sh:datatype xsd:string ;
    ] ;
    sh:property [
        sh:path ex:email ;
        sh:maxCount 1 ;
    ] .
$SHACL$);

-- 2. Validate the default graph
SELECT pg_ripple.validate();

-- 3. Enable inline rejection of violations
SET pg_ripple.shacl_mode = 'sync';
```

---

## Functions

### `load_shacl(data TEXT) → INTEGER`

Parse `data` (Turtle-formatted SHACL shapes) and store every shape in `_pg_ripple.shacl_shapes`. Returns the count of shapes loaded. Raises an error on Turtle syntax failures so no partial state is committed.

**Supported shape types:**
- `sh:NodeShape` — targets a class or specific nodes
- `sh:PropertyShape` — constraints on a predicate path

**Supported constraints (v0.7.0 Core):**

| Constraint | Description |
|---|---|
| `sh:minCount` | Minimum number of value nodes per focus node |
| `sh:maxCount` | Maximum number of value nodes per focus node |
| `sh:datatype` | Required datatype IRI for value nodes |
| `sh:in (...)` | Allowed value set (Turtle list) |
| `sh:pattern "regex"` | Regex match on lexical form |
| `sh:class` | Required `rdf:type` for value nodes |
| `sh:node` | Nested shape reference — value nodes must conform to the referenced shape |
| `sh:or (...)` | Value/focus node must conform to at least one listed shape (v0.8.0) |
| `sh:and (...)` | Value/focus node must conform to all listed shapes (v0.8.0) |
| `sh:not` | Value/focus node must NOT conform to the referenced shape (v0.8.0) |
| `sh:qualifiedValueShape` | Combined with `sh:qualifiedMinCount`/`sh:qualifiedMaxCount` (v0.8.0) |
| `sh:equals <path>` | The value set for the declared path must equal the value set for `<path>` (v0.45.0) |
| `sh:disjoint <path>` | The value set for the declared path must be disjoint from the value set for `<path>` (v0.45.0) |

**Supported target declarations:**

| Declaration | Description |
|---|---|
| `sh:targetClass` | All instances (`rdf:type` members) of a class |
| `sh:targetNode` | One or more specific nodes |
| `sh:targetSubjectsOf` | All subjects of a given predicate |
| `sh:targetObjectsOf` | All objects of a given predicate |

```sql
-- Returns the number of shapes loaded
SELECT pg_ripple.load_shacl('
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <https://example.org/> .

ex:ThingShape
    a sh:NodeShape ;
    sh:targetClass ex:Thing ;
    sh:property [
        sh:path ex:name ;
        sh:minCount 1 ;
    ] .
');
```

---

### `validate(graph TEXT DEFAULT NULL) → JSONB`

Run a full offline SHACL validation report against all active shapes.

| `graph` value | Scope |
|---|---|
| `NULL` (default) | Default graph (id 0) |
| `''` (empty string) | Default graph |
| `'*'` | All named graphs |
| `'<https://example.org/g1>'` | Specific named graph |

**Return value** — a JSONB object with two keys:

```json
{
  "conforms": false,
  "violations": [
    {
      "focusNode": "https://example.org/alice",
      "shapeIRI":  "https://example.org/PersonShape",
      "path":      "https://example.org/email",
      "constraint": "sh:maxCount",
      "message":   "expected at most 1 value(s) for <https://example.org/email>, found 2",
      "severity":  "Violation"
    }
  ]
}
```

```sql
-- Check if the default graph conforms
SELECT (pg_ripple.validate() ->> 'conforms')::boolean AS ok;

-- Count violations
SELECT jsonb_array_length(pg_ripple.validate() -> 'violations') AS violation_count;

-- Validate a named graph
SELECT pg_ripple.validate('<https://example.org/my-graph>');
```

---

### `list_shapes() → TABLE(shape_iri TEXT, active BOOLEAN)`

Return all shapes in the shapes catalog.

```sql
SELECT * FROM pg_ripple.list_shapes();
```

---

### `drop_shape(shape_uri TEXT) → INTEGER`

Remove a shape by its IRI. Returns 1 if found and removed, 0 if not found.

```sql
SELECT pg_ripple.drop_shape('https://example.org/PersonShape');
```

---

## Validation Modes (`pg_ripple.shacl_mode`)

| Mode | Behaviour |
|---|---|
| `off` (default) | No SHACL enforcement. Shapes are stored but not used at insert time. |
| `sync` | Violations are detected inline during `insert_triple()`. The insert is rejected with an error message; no partial data is written. |
| `async` | (v0.8.0) Triples are queued in `_pg_ripple.validation_queue` for background validation. Violations are moved to `_pg_ripple.dead_letter_queue`. |

```sql
-- Enable inline enforcement
SET pg_ripple.shacl_mode = 'sync';

-- This will raise an error if the shape's sh:maxCount is exceeded:
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<https://example.org/email>',
    '"alice3@example.org"'
);
-- ERROR:  SHACL violation: <https://example.org/alice> sh:maxCount 1 for
--         <https://example.org/email>: found 1 existing value(s), limit is 1

-- Restore default
RESET pg_ripple.shacl_mode;
```

> **Latency note**: `sync` mode executes per-shape validator plans for every `insert_triple()` call when `shacl_mode = 'sync'`. For high-throughput ingestion, use `off` (validate after load with `validate()`) or configure `async` mode (v0.8.0).

---

## Example: Full Workflow

```sql
-- 1. Load shapes
SELECT pg_ripple.load_shacl($SHACL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <https://example.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:EmployeeShape
    a sh:NodeShape ;
    sh:targetClass ex:Employee ;
    sh:property [
        sh:path ex:employeeId ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:datatype xsd:integer ;
    ] ;
    sh:property [
        sh:path ex:department ;
        sh:minCount 1 ;
        sh:in ( ex:Engineering ex:Sales ex:HR ) ;
    ] .
$SHACL$);

-- 2. Load data
SELECT pg_ripple.load_ntriples($NQ$
<https://example.org/emp1> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/Employee> .
<https://example.org/emp1> <https://example.org/employeeId> "42"^^<http://www.w3.org/2001/XMLSchema#integer> .
<https://example.org/emp1> <https://example.org/department> <https://example.org/Engineering> .
$NQ$);

-- 3. Validate
SELECT pg_ripple.validate();
-- {"conforms": true, "violations": []}

-- 4. Confirm shapes loaded
SELECT * FROM pg_ripple.list_shapes();
```

---

## Internal Tables

| Table | Description |
|---|---|
| `_pg_ripple.shacl_shapes` | Shape catalog: `shape_iri`, `shape_json` (JSONB IR), `active`, timestamps |
| `_pg_ripple.validation_queue` | Async validation inbox (populated when `shacl_mode = 'async'`) |
| `_pg_ripple.dead_letter_queue` | Triples rejected by async validation with violation report |

---

## Limitations (v0.7.0)

- Property paths beyond direct predicates (e.g., `sh:inversePath`, `sh:alternativePath`) are not supported.
- `sh:minCount` is only checked by `validate()`, not during `insert_triple()` in sync mode (absence cannot be detected on a single insert).

---

## Async Validation Pipeline (v0.8.0)

When `pg_ripple.shacl_mode = 'async'`, violations are **not** raised inline. Instead, every triple inserted via `insert_triple()` is recorded in `_pg_ripple.validation_queue`. A background worker (the merge worker) drains the queue in batches, running full SHACL validation against each triple. Triples that violate any active shape are written to `_pg_ripple.dead_letter_queue` with a structured violation report.

### Management functions

| Function | Returns | Description |
|---|---|---|
| `process_validation_queue(batch_size BIGINT DEFAULT 1000)` | `BIGINT` | Process up to `batch_size` items; returns count processed |
| `validation_queue_length()` | `BIGINT` | Number of items pending in the queue |
| `dead_letter_count()` | `BIGINT` | Number of violations recorded |
| `dead_letter_queue()` | `JSONB` | All dead-letter entries as a JSON array |
| `drain_dead_letter_queue()` | `BIGINT` | Delete all dead-letter entries; returns count deleted |

```sql
-- Enable async mode
SET pg_ripple.shacl_mode = 'async';

-- Inserts never raise errors — violations are queued
SELECT pg_ripple.insert_triple(
    '<https://example.org/thing>',
    '<https://example.org/value>',
    '"wrong-type"'
);

-- Check queue depth
SELECT pg_ripple.validation_queue_length();

-- Manually drain the queue (background worker also does this)
SELECT pg_ripple.process_validation_queue();

-- Inspect violations
SELECT pg_ripple.dead_letter_queue();

-- Clear after review
SELECT pg_ripple.drain_dead_letter_queue();
```

### When to use async mode

- **High-throughput ingestion** where inline SHACL overhead is unacceptable
- **Batch imports** — validate after load by calling `process_validation_queue()` once
- **Non-blocking pipelines** — violations are flagged asynchronously without interrupting writers

> **Note**: The background worker processes the queue automatically; `process_validation_queue()` is a manual override useful in testing and one-shot pipelines.

---

## Complex Shape Constraints (v0.8.0)

### `sh:or` — at least one shape must match

```turtle
ex:AgentShape
    a sh:NodeShape ;
    sh:targetClass ex:Agent ;
    sh:or (ex:PersonShape ex:OrganizationShape) .
```

The focus node must conform to **at least one** of the listed shapes. Evaluated both in `validate()` and `sync` mode.

### `sh:and` — all shapes must match

```turtle
ex:VerifiedPersonShape
    a sh:NodeShape ;
    sh:targetClass ex:VerifiedPerson ;
    sh:and (ex:PersonShape ex:VerifiedShape) .
```

The focus node must conform to **every** listed shape.

### `sh:not` — shape must not match

```turtle
ex:ActiveEntityShape
    a sh:NodeShape ;
    sh:targetClass ex:ActiveEntity ;
    sh:not ex:BannedEntityShape .
```

The focus node must **not** conform to the referenced shape.

### `sh:node` — nested shape for value nodes

```turtle
ex:CompanyShape
    a sh:NodeShape ;
    sh:targetClass ex:Company ;
    sh:property [
        sh:path ex:headquarterAddress ;
        sh:node ex:AddressShape ;
    ] .
```

Each value node along the path must conform to `ex:AddressShape`. Recursion is depth-limited at 32 levels.

### `sh:qualifiedValueShape` — qualified cardinality

```turtle
ex:EmployerShape
    a sh:NodeShape ;
    sh:targetClass ex:Employer ;
    sh:property [
        sh:path ex:officeAddress ;
        sh:qualifiedValueShape ex:USAddressShape ;
        sh:qualifiedMinCount 1 ;
        sh:qualifiedMaxCount 3 ;
    ] .
```

At least `sh:qualifiedMinCount` (and at most `sh:qualifiedMaxCount`) value nodes along the path must conform to the qualified shape.

---

## Limitations (v0.8.0)

- Property paths beyond direct predicates (e.g., `sh:inversePath`, `sh:alternativePath`) are not supported.
- `sh:minCount` is only checked by `validate()`, not during `insert_triple()` in sync mode.
- `sh:qualifiedMinCount` is only checked by `validate()` in sync mode (absence cannot be detected on a single insert).
