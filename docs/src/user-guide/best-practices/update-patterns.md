# Update Patterns

This page covers best practices for writing data to pg_ripple — when to use `INSERT DATA` / `DELETE DATA` vs `DELETE/INSERT WHERE`, how to manage named graphs, and how to write idempotent update scripts.

## Choosing the right write API

| Scenario | Recommended API |
|---|---|
| Loading a large file (> ~1 000 triples) | `load_ntriples()` / `load_turtle()` |
| Inserting a single known triple from SQL | `insert_triple()` |
| Inserting triples from a SPARQL-capable client | `sparql_update()` with INSERT DATA |
| Removing an exact triple | `delete_triple()` or DELETE DATA |
| Pattern-based updates (find-and-replace) | `DELETE/INSERT WHERE` |
| Clearing a named graph | `CLEAR GRAPH <g>` |
| Loading remote RDF data | `LOAD <url>` |

---

## INSERT DATA vs bulk load

`INSERT DATA` and bulk load (`load_ntriples`) both result in identical on-disk storage, but their performance profiles differ:

| | `sparql_update` (INSERT DATA) | `load_ntriples` |
|---|---|---|
| Per-triple overhead | Medium (SPL + dictionary lookup per term) | Low (batched dictionary ops) |
| Transaction boundary | One PG transaction per call | One PG transaction per call |
| Typical throughput | ~1 000–5 000 triples/sec | ~50 000–200 000 triples/sec |
| Use case | Small, targeted writes | Bulk ingestion |

For initial data loads, always use `load_ntriples` or `load_turtle`. Reserve `sparql_update` / `INSERT DATA` for incremental updates.

---

## Pattern-based updates (DELETE/INSERT WHERE)

`DELETE/INSERT WHERE` is the SPARQL equivalent of SQL `UPDATE`. It matches triples using a WHERE clause, then deletes and/or inserts triples for each match. The WHERE clause is compiled through the same SPARQL→SQL engine as SELECT queries.

### Rename a property value

```sql
-- Change all "draft" status values to "published":
SELECT pg_ripple.sparql_update('
    DELETE { ?s <https://example.org/status> <https://example.org/draft> }
    INSERT { ?s <https://example.org/status> <https://example.org/published> }
    WHERE  { ?s <https://example.org/status> <https://example.org/draft> }
');
```

Return value: `(# deleted) + (# inserted)`. For `N` matching subjects, this returns `2N`.

### Add a property conditionally

```sql
-- For every person lacking an email, insert a placeholder:
SELECT pg_ripple.sparql_update('
    INSERT { ?person <https://schema.org/email> "no-reply@example.org" }
    WHERE  {
        ?person a <https://schema.org/Person> .
        FILTER NOT EXISTS { ?person <https://schema.org/email> ?e }
    }
');
```

### Delete by pattern only

You can omit `INSERT` to delete only:

```sql
SELECT pg_ripple.sparql_update('
    DELETE { ?s <https://example.org/temp> ?o }
    WHERE  { ?s <https://example.org/temp> ?o }
');
```

Or omit `DELETE` to insert only:

```sql
SELECT pg_ripple.sparql_update('
    INSERT { ?s <https://example.org/indexed> "true"^^<http://www.w3.org/2001/XMLSchema#boolean> }
    WHERE  { ?s <https://example.org/name> ?name }
');
```

### Performance note

For each WHERE binding, the DELETE phase and INSERT phase run individually. For large result sets (thousands of bindings), consider batching via the `load_*` APIs or using a single `INSERT DATA` with pre-computed data.

---

## Graph lifecycle management

### Creating and populating a named graph

Named graphs are created implicitly when the first triple is inserted. `CREATE GRAPH` is useful for SPARQL compliance or to pre-register a graph IRI in the dictionary before any triples arrive.

```sql
-- Explicit creation (optional):
SELECT pg_ripple.sparql_update(
    'CREATE GRAPH <https://example.org/mygraph>'
);

-- Implicit creation via INSERT:
SELECT pg_ripple.sparql_update('
    INSERT DATA {
        GRAPH <https://example.org/mygraph> {
            <https://example.org/a> <https://example.org/b> <https://example.org/c>
        }
    }
');
```

### CLEAR vs DROP

Both operations delete all triples from a graph. The difference is conceptual — DROP "removes" the graph while CLEAR keeps it as an empty container. In pg_ripple, both behave identically on storage (the graph IRI remains in the dictionary either way).

```sql
-- Remove all triples, keep the graph:
SELECT pg_ripple.sparql_update(
    'CLEAR GRAPH <https://example.org/mygraph>'
);

-- Remove all triples and the graph:
SELECT pg_ripple.sparql_update(
    'DROP GRAPH <https://example.org/mygraph>'
);
```

### Clearing multiple graphs at once

```sql
-- Clear the default graph only:
SELECT pg_ripple.sparql_update('CLEAR DEFAULT');

-- Clear all named graphs (default graph untouched):
SELECT pg_ripple.sparql_update('CLEAR NAMED');

-- Clear everything (default + all named):
SELECT pg_ripple.sparql_update('CLEAR ALL');
```

### SILENT modifier

Adding `SILENT` suppresses errors (e.g., if a graph does not exist):

```sql
SELECT pg_ripple.sparql_update(
    'DROP SILENT GRAPH <https://example.org/nonexistent>'
);
SELECT pg_ripple.sparql_update(
    'CLEAR SILENT GRAPH <https://example.org/nonexistent>'
);
```

---

## Loading remote RDF data (LOAD)

`LOAD <url>` fetches a remote RDF document via HTTP(S) and inserts all triples.

```sql
-- Load into the default graph:
SELECT pg_ripple.sparql_update(
    'LOAD <https://www.w3.org/People/Berners-Lee/card.rdf>'
);

-- Load into a named graph:
SELECT pg_ripple.sparql_update(
    'LOAD <https://example.org/data.ttl> INTO GRAPH <https://example.org/remote>'
);

-- Ignore HTTP errors:
SELECT pg_ripple.sparql_update(
    'LOAD SILENT <https://example.org/maybe-missing.nt>'
);
```

Format is detected from `Content-Type` or URL extension:
- `text/turtle` / `.ttl` → Turtle
- `application/rdf+xml` / `.rdf` / `.owl` → RDF/XML
- Everything else → N-Triples

For large remote files, prefer the file-load APIs (`load_ntriples`, `load_turtle`) after fetching the file separately — `LOAD` buffers the entire response in memory before parsing.

---

## Idempotent insert patterns

Because VP tables use `ON CONFLICT DO NOTHING`, inserting an already-existing triple is safe — the SID is returned for the existing row and `sparql_update()` counts it as 1 affected triple.

To write idempotent SQL migration scripts:

```sql
-- Safe to run multiple times
SELECT pg_ripple.sparql_update('
    INSERT DATA {
        <https://example.org/config> <https://example.org/version>
            "2"^^<http://www.w3.org/2001/XMLSchema#integer>
    }
');
```

---

## Atomic replace (delete + insert)

Use `DELETE/INSERT WHERE` for atomic property replacement — it runs both phases in a single operation:

```sql
-- Atomic rename — no window where the property is absent:
SELECT pg_ripple.sparql_update('
    DELETE { <https://example.org/alice> <https://schema.org/name> "Alice Smith" }
    INSERT { <https://example.org/alice> <https://schema.org/name> "Alice Jones" }
    WHERE  { <https://example.org/alice> <https://schema.org/name> "Alice Smith" }
');
```

For cases where the old value is not known in advance:

```sql
SELECT pg_ripple.sparql_update('
    DELETE { <https://example.org/alice> <https://schema.org/name> ?old }
    INSERT { <https://example.org/alice> <https://schema.org/name> "Alice Jones" }
    WHERE  { <https://example.org/alice> <https://schema.org/name> ?old }
');
```

---

## Using inline-encoded types for efficient range queries

For numeric or date predicates that will be compared in SPARQL FILTERs, use typed literals with an inline-compatible type:

| Use this type | Instead of |
|---|---|
| `"42"^^xsd:integer` | `"42"` (plain string) |
| `"2024-01-01"^^xsd:date` | `"2024-01-01"` (plain string) |
| `"true"^^xsd:boolean` | `"true"` (plain string) |

With inline-encoded types, FILTER comparisons like `FILTER(?age > 30)` compile to `WHERE o > <inline_id>` — a simple integer comparison on the VP table column with no dictionary join.

```sql
-- Good: uses inline encoding for age
SELECT pg_ripple.sparql_update('
    INSERT DATA {
        <https://example.org/alice> <https://example.org/age>
            "30"^^<http://www.w3.org/2001/XMLSchema#integer>
    }
');
```

This page covers best practices for writing data to pg_ripple — when to use `INSERT DATA` / `DELETE DATA`, when to use the lower-level `insert_triple` / `delete_triple` functions, and how to write idempotent update scripts.

## Choosing the right write API

| Scenario | Recommended API |
|---|---|
| Loading a large file (> ~1 000 triples) | `load_ntriples()` / `load_turtle()` |
| Inserting a single known triple from SQL | `insert_triple()` |
| Inserting triples from a SPARQL-capable client | `sparql_update()` with INSERT DATA |
| Removing an exact triple | `delete_triple()` or DELETE DATA |
| Pattern-based updates (find-and-replace) | DELETE/INSERT WHERE (v0.12.0) |

---

## INSERT DATA vs bulk load

`INSERT DATA` and bulk load (`load_ntriples`) both result in identical on-disk storage, but their performance profiles differ:

| | `sparql_update` (INSERT DATA) | `load_ntriples` |
|---|---|---|
| Per-triple overhead | Medium (SPL + dictionary lookup per term) | Low (batched dictionary ops) |
| Transaction boundary | One PG transaction per call | One PG transaction per call |
| Typical throughput | ~1 000–5 000 triples/sec | ~50 000–200 000 triples/sec |
| Use case | Small, targeted writes | Bulk ingestion |

For initial data loads, always use `load_ntriples` or `load_turtle`. Reserve `sparql_update` / `INSERT DATA` for incremental updates.

---

## Idempotent insert patterns

Because `vp_rare` and dedicated VP tables use `ON CONFLICT DO NOTHING`, inserting an already-existing triple is safe — `insert_triple()` returns the existing SID and `sparql_update()` counts it as 1 affected triple regardless.

To write idempotent SQL migration scripts:

```sql
-- Safe to run multiple times
SELECT pg_ripple.sparql_update('
    INSERT DATA {
        <https://example.org/config> <https://example.org/version> "2"^^<http://www.w3.org/2001/XMLSchema#integer>
    }
');
```

To implement a "set if not present" pattern (only insert if the subject doesn't already have the predicate):

```sql
-- Insert only if alice does not already have an email
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_ripple.find_triples(
            '<https://example.org/alice>',
            '<https://schema.org/email>',
            NULL
        )
    ) THEN
        PERFORM pg_ripple.insert_triple(
            '<https://example.org/alice>',
            '<https://schema.org/email>',
            '"alice@example.org"'
        );
    END IF;
END $$;
```

---

## Atomic replace (delete + insert)

To atomically replace the value of a property:

```sql
BEGIN;

-- Remove old value(s)
SELECT pg_ripple.sparql_update(
    'DELETE DATA { <https://example.org/alice> <https://schema.org/name> "Alice Smith" }'
);

-- Insert new value
SELECT pg_ripple.sparql_update(
    'INSERT DATA { <https://example.org/alice> <https://schema.org/name> "Alice Jones" }'
);

COMMIT;
```

> **Tip**: When replacing a value, wrap the delete and insert in a single `BEGIN / COMMIT` block so readers never see the intermediate state where the property is absent.

---

## Using inline-encoded types for efficient range queries

For numeric or date predicates that will be compared in SPARQL FILTERs, use typed literals with an inline-compatible type:

| Use this type | Instead of |
|---|---|
| `"42"^^xsd:integer` | `"42"` (plain string) |
| `"2024-01-01"^^xsd:date` | `"2024-01-01"` (plain string) |
| `"true"^^xsd:boolean` | `"true"` (plain string) |

With inline-encoded types, FILTER comparisons like `FILTER(?age > 30)` compile to `WHERE o > <inline_id>` — a simple integer comparison on the VP table column with no dictionary join.

```sql
-- Good: uses inline encoding for age
SELECT pg_ripple.sparql_update('
    INSERT DATA {
        <https://example.org/alice> <https://example.org/age>
            "30"^^<http://www.w3.org/2001/XMLSchema#integer>
    }
');

-- Less efficient: stored as plain string; FILTER comparisons require dict join
SELECT pg_ripple.sparql_update('
    INSERT DATA {
        <https://example.org/alice> <https://example.org/age> "30"
    }
');
```

---

## Batch deletes

To delete all triples for a subject in a single SQL call (faster than DELETE DATA per-triple):

```sql
-- Delete all triples where alice is the subject
SELECT pg_ripple.delete_triple(s, p, o)
FROM pg_ripple.find_triples('<https://example.org/alice>', NULL, NULL)
AS t(s TEXT, p TEXT, o TEXT, g TEXT, i BIGINT);
```

For named-graph isolation, filter by graph ID:

```sql
SELECT pg_ripple.delete_triple(s, p, o)
FROM pg_ripple.find_triples(NULL, NULL, NULL)
WHERE g = pg_ripple.graph_id('<https://example.org/draft-graph>');
```
