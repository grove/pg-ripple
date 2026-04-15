# SPARQL Update

pg_ripple supports a subset of [SPARQL 1.1 Update](https://www.w3.org/TR/sparql11-update/) via the `sparql_update()` function. In v0.5.1, `INSERT DATA` and `DELETE DATA` are fully supported. Pattern-based updates (`DELETE/INSERT WHERE`) are planned for v0.12.0.

## sparql_update

```sql
pg_ripple.sparql_update(query TEXT) RETURNS BIGINT
```

Executes a SPARQL Update statement and returns the number of triples affected.

### INSERT DATA

Adds one or more ground triples (no variables) to the store:

```sql
-- Insert a single triple
SELECT pg_ripple.sparql_update(
    'INSERT DATA { <https://example.org/alice> <https://example.org/age> "30"^^<http://www.w3.org/2001/XMLSchema#integer> }'
);

-- Insert multiple triples in one statement
SELECT pg_ripple.sparql_update('
    INSERT DATA {
        <https://example.org/alice> <https://example.org/knows> <https://example.org/bob> .
        <https://example.org/bob>   <https://example.org/name>  "Bob"
    }
');
```

All subjects, predicates, and objects are dictionary-encoded before insertion. Typed literals that qualify for inline encoding (`xsd:integer`, `xsd:boolean`, `xsd:date`, `xsd:dateTime`) are stored as bit-packed IDs rather than dictionary rows.

### DELETE DATA

Removes exact-match triples from the store:

```sql
SELECT pg_ripple.sparql_update(
    'DELETE DATA { <https://example.org/alice> <https://example.org/knows> <https://example.org/bob> }'
);
```

Returns `0` if the triple does not exist (no error). DELETE DATA is an atomic set operation — all triples in the statement are deleted or none are (if any are missing, the count reflects only the triples actually removed).

### Named graphs

Both forms support named graphs:

```sql
SELECT pg_ripple.sparql_update('
    INSERT DATA {
        GRAPH <https://example.org/graph1> {
            <https://example.org/alice> <https://example.org/memberOf> <https://example.org/org1>
        }
    }
');
```

The default graph has ID `0`. Named graphs are created implicitly when the first triple is inserted.

## Return value

`sparql_update()` returns the count of triples inserted or deleted:

| Statement | Return value |
|---|---|
| `INSERT DATA { t1 . t2 . t3 }` | 3 if all were new; fewer if some already existed |
| `DELETE DATA { t1 }` | 1 if found, 0 if not found |

## Compared to insert_triple / delete_triple

| | `insert_triple` | `sparql_update` |
|---|---|---|
| Input format | N-Triples strings (subject, predicate, object) | SPARQL text |
| Missing arguments | `NULL` used for absent components | Not applicable — all parts required for INSERT/DELETE DATA |
| Multiple triples | One call per triple | Multiple triples per statement |
| Use case | Programmatic insertion from SQL or application code | SPARQL-based tools and standards-compliant clients |

## Unsupported forms (planned for later releases)

The following SPARQL Update forms are **not yet supported** in v0.5.1:

- `DELETE/INSERT WHERE { … }` — pattern-based updates (v0.12.0)
- `LOAD <url>` (v0.12.0)
- `CLEAR GRAPH <g>` (v0.12.0)
- `DROP GRAPH <g>` (v0.12.0)
- `CREATE GRAPH <g>` (v0.12.0)
- Update sequences (`; UPDATE1 ; UPDATE2`) (v0.12.0)
