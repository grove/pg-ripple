# Update Patterns

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
