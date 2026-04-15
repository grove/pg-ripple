# SHACL Patterns

Practical patterns for defining and using SHACL shapes with pg_ripple.

---

## NodeShape vs PropertyShape

SHACL defines two kinds of shapes:

| Kind | When to use |
|---|---|
| **NodeShape** | Applies to a set of focus nodes (identified by `sh:targetClass`, `sh:targetNode`, etc.) |
| **PropertyShape** | Defines constraints on the values of a specific predicate, nested inside a NodeShape |

In pg_ripple's Turtle parser, a `sh:NodeShape` carries one or more `sh:property [...]` blocks, each describing a `PropertyShape` inline:

```turtle
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <https://example.org/> .

ex:ProductShape            # ← NodeShape
    a sh:NodeShape ;
    sh:targetClass ex:Product ;
    sh:property [          # ← inline PropertyShape
        sh:path ex:sku ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
    ] .
```

---

## `sh:datatype` — Enforcing Value Types

Use `sh:datatype` to require a specific XSD datatype for literal values:

```turtle
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <https://example.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:SensorShape
    a sh:NodeShape ;
    sh:targetClass ex:Sensor ;
    sh:property [
        sh:path ex:temperature ;
        sh:datatype xsd:decimal ;
    ] ;
    sh:property [
        sh:path ex:label ;
        sh:datatype xsd:string ;
    ] .
```

Insert the shape, then load data using the correct datatype suffix:

```sql
SELECT pg_ripple.load_ntriples('
<https://example.org/s1> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/Sensor> .
<https://example.org/s1> <https://example.org/temperature> "23.5"^^<http://www.w3.org/2001/XMLSchema#decimal> .
');

SELECT pg_ripple.validate();
-- {"conforms": true, ...}
```

A plain string for `ex:temperature` (e.g. `"23.5"` without `^^xsd:decimal`) will produce a `sh:datatype` violation.

---

## `sh:minCount` and `sh:maxCount`

These are the most common SHACL constraints and map naturally to cardinality checks.

| Pattern | Meaning |
|---|---|
| `sh:minCount 1` | Required field — every focus node must have at least one value |
| `sh:maxCount 1` | At most one value — useful for functional properties |
| `sh:minCount 1 ; sh:maxCount 1` | Exactly one value |

```turtle
ex:PersonShape
    a sh:NodeShape ;
    sh:targetClass ex:Person ;
    sh:property [
        sh:path ex:fullName ;
        sh:minCount 1 ;       -- required
        sh:maxCount 1 ;       -- unique per person
        sh:datatype xsd:string ;
    ] ;
    sh:property [
        sh:path ex:phoneNumber ;
        sh:maxCount 3 ;       -- at most 3 phone numbers
    ] .
```

> **Important**: `sh:minCount` is only checked by `pg_ripple.validate()`, not enforced during `insert_triple()` in sync mode. This is because a missing value cannot be detected from a single insert — it requires scanning all focus nodes after the fact.
>
> `sh:maxCount` **is** checked in sync mode, since exceeding the maximum can be detected as each new value arrives.

---

## Sync Mode: Latency Trade-offs

`pg_ripple.shacl_mode = 'sync'` runs SHACL validator plans on every `insert_triple()` call. This adds latency proportional to:

1. The number of active shapes
2. The selectivity of the target class (fewer focus nodes = faster)
3. The cost of counting existing value nodes for `sh:maxCount`

**Recommended for**: low-throughput, high-integrity workflows (master data, configuration graphs, knowledge bases).

**Not recommended for**: bulk data ingestion, high-frequency event streams, or when violations should be post-processed rather than rejected at insert time.

```sql
-- For bulk loads: keep shacl_mode off, validate after:
SET pg_ripple.shacl_mode = 'off';
SELECT pg_ripple.load_turtle($$ ... $$);
SELECT pg_ripple.validate();  -- check after the fact
```

---

## Calling `validate()` On Demand

`validate()` does a full pass over all focus nodes for every active shape. Use it:

- After a bulk load to detect any violations in the imported data
- As part of a scheduled data quality check
- Before publishing a named graph

```sql
-- Validate a specific named graph
SELECT pg_ripple.validate('<https://example.org/my-data>');

-- Validate all graphs
SELECT pg_ripple.validate('*');

-- Extract just the violations as a set
SELECT v
FROM jsonb_array_elements(
    pg_ripple.validate() -> 'violations'
) AS v;
```

---

## `sh:in` — Controlled Vocabulary

Use `sh:in` to restrict a property to a specific set of allowed values:

```turtle
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <https://example.org/> .

ex:OrderShape
    a sh:NodeShape ;
    sh:targetClass ex:Order ;
    sh:property [
        sh:path ex:status ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:in ( ex:pending ex:confirmed ex:shipped ex:delivered ex:cancelled ) ;
    ] .
```

---

## `sh:pattern` — Regex Constraints

Validate string values with a POSIX regular expression:

```turtle
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <https://example.org/> .

ex:ContactShape
    a sh:NodeShape ;
    sh:targetClass ex:Contact ;
    sh:property [
        sh:path ex:email ;
        sh:pattern "^[a-zA-Z0-9._%+\\-]+@[a-zA-Z0-9.\\-]+\\.[a-zA-Z]{2,}$" ;
    ] .
```

> **Note**: pg_ripple uses PostgreSQL's `~` operator for regex matching, which follows POSIX extended regex. Backslashes must be doubled in Turtle string literals.

---

## Managing Multiple Shapes

Load shapes from separate Turtle documents, one call per document:

```sql
-- Load Person shapes
SELECT pg_ripple.load_shacl(pg_read_file('/etc/shapes/person-shapes.ttl'));

-- Load Product shapes
SELECT pg_ripple.load_shacl(pg_read_file('/etc/shapes/product-shapes.ttl'));

-- List all active shapes
SELECT shape_iri, active FROM pg_ripple.list_shapes();

-- Deactivate a shape without deleting it (set active=false manually or drop it)
SELECT pg_ripple.drop_shape('https://example.org/OldPersonShape');
```

---

## Pre-deployment Checklist

Before running in production with SHACL:

1. Load all shapes **before** bulk importing data — this ensures violations are caught from the start.
2. For large existing datasets, run `SELECT pg_ripple.validate()` after loading shapes to identify pre-existing violations.
3. Choose `shacl_mode` based on throughput requirements: `off` for ETL pipelines, `sync` for interactive / low-volume inserts.
4. Index `ex:targetClass` predicates — `sh:targetClass` shapes perform a full scan of `rdf:type` triples to collect focus nodes. Ensure `rdf:type` has a dedicated VP table (it usually does after a few hundred triples).
