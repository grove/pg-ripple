# SPARQL Update

pg_ripple supports the full [SPARQL 1.1 Update](https://www.w3.org/TR/sparql11-update/) specification via the `sparql_update()` function. All update operations — `INSERT DATA`, `DELETE DATA`, `DELETE/INSERT WHERE`, `LOAD`, `CLEAR`, `DROP`, and `CREATE` — are available as of v0.12.0.

## sparql_update

```sql
pg_ripple.sparql_update(query TEXT) RETURNS BIGINT
```

Executes a SPARQL Update statement and returns the number of triples affected (inserted + deleted).

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

Returns `0` if the triple does not exist (no error).

### DELETE/INSERT WHERE

Pattern-based updates find matching triples via a WHERE clause and delete and/or insert triples for each match. This is the SPARQL equivalent of SQL `UPDATE`.

```sql
-- Replace all "draft" status values with "published":
SELECT pg_ripple.sparql_update('
    DELETE { ?s <https://example.org/status> <https://example.org/draft> }
    INSERT { ?s <https://example.org/status> <https://example.org/published> }
    WHERE  { ?s <https://example.org/status> <https://example.org/draft> }
');

-- Add an email placeholder for every person who lacks one:
SELECT pg_ripple.sparql_update('
    INSERT { ?person <https://schema.org/email> "no-reply@example.org" }
    WHERE  {
        ?person a <https://schema.org/Person> .
        FILTER NOT EXISTS { ?person <https://schema.org/email> ?e }
    }
');
```

The WHERE clause is compiled through the existing SPARQL→SQL engine. All bound variables in the WHERE clause are available in the DELETE and INSERT templates. The DELETE phase runs before the INSERT phase for each bound row. The entire operation is transactional.

Return value: `(number of triples deleted) + (number of triples inserted)`.

### Named graphs

All forms support named graphs:

```sql
SELECT pg_ripple.sparql_update('
    INSERT DATA {
        GRAPH <https://example.org/graph1> {
            <https://example.org/alice> <https://example.org/memberOf> <https://example.org/org1>
        }
    }
');

-- Pattern-based update in a named graph:
SELECT pg_ripple.sparql_update('
    DELETE { GRAPH <https://example.org/graph1> { ?s ?p ?o } }
    WHERE  { GRAPH <https://example.org/graph1> { ?s ?p ?o } }
');
```

The default graph has ID `0`. Named graphs are created implicitly when the first triple is inserted.

## Graph management

### LOAD

Fetches an RDF document from a URL via HTTP(S) and inserts all triples into the store.

```sql
-- Load into the default graph:
SELECT pg_ripple.sparql_update(
    'LOAD <https://www.w3.org/People/Berners-Lee/card.rdf>'
);

-- Load into a named graph:
SELECT pg_ripple.sparql_update(
    'LOAD <https://example.org/data.ttl> INTO GRAPH <https://example.org/mygraph>'
);

-- Ignore errors (e.g. network failures):
SELECT pg_ripple.sparql_update(
    'LOAD SILENT <https://example.org/data.nt>'
);
```

Format detection: Turtle if the `Content-Type` contains `turtle` or the URL ends in `.ttl`; RDF/XML if `rdf+xml` or `.rdf`/`.owl`; otherwise N-Triples. Named graphs inside TriG are not split out — the destination graph overrides.

### CLEAR

Deletes all triples from the target graph(s) without removing the graph name from the dictionary.

```sql
-- Clear a specific named graph:
SELECT pg_ripple.sparql_update(
    'CLEAR GRAPH <https://example.org/mygraph>'
);

-- Clear the default graph:
SELECT pg_ripple.sparql_update('CLEAR DEFAULT');

-- Clear all named graphs (default graph untouched):
SELECT pg_ripple.sparql_update('CLEAR NAMED');

-- Clear everything:
SELECT pg_ripple.sparql_update('CLEAR ALL');

-- Ignore if the graph does not exist:
SELECT pg_ripple.sparql_update(
    'CLEAR SILENT GRAPH <https://example.org/nonexistent>'
);
```

### DROP

Like CLEAR, but also deregisters the graph. In pg_ripple, the graph IRI remains in the dictionary (deregistering is a no-op beyond clearing triples), so DROP and CLEAR are functionally equivalent.

```sql
SELECT pg_ripple.sparql_update(
    'DROP GRAPH <https://example.org/mygraph>'
);
SELECT pg_ripple.sparql_update('DROP ALL');
SELECT pg_ripple.sparql_update(
    'DROP SILENT GRAPH <https://example.org/nonexistent>'
);
```

### CREATE

Registers a named graph in the dictionary. Since pg_ripple creates graphs implicitly on first insert, `CREATE GRAPH` is rarely needed but is supported for SPARQL compliance.

```sql
SELECT pg_ripple.sparql_update(
    'CREATE GRAPH <https://example.org/newgraph>'
);
-- No-op if the graph already exists:
SELECT pg_ripple.sparql_update(
    'CREATE SILENT GRAPH <https://example.org/newgraph>'
);
```

## Return value

`sparql_update()` returns the total count of triples affected:

| Statement | Return value |
|---|---|
| `INSERT DATA { t1 . t2 . t3 }` | 3 if all were new |
| `DELETE DATA { t1 }` | 1 if found, 0 if not found |
| `DELETE { … } INSERT { … } WHERE { … }` | deletes + inserts |
| `CLEAR GRAPH <g>` | triples removed |
| `DROP GRAPH <g>` | triples removed |
| `CREATE GRAPH <g>` | 0 (no triples touched) |
| `LOAD <url>` | triples inserted |

## Compared to insert_triple / delete_triple

| | `insert_triple` | `sparql_update` |
|---|---|---|
| Input format | N-Triples strings (subject, predicate, object) | SPARQL text |
| Multiple triples | One call per triple | Multiple triples per statement |
| Pattern-based | No | Yes (DELETE/INSERT WHERE) |
| Graph management | No | Yes (CLEAR/DROP/CREATE/LOAD) |
| Use case | Programmatic insertion from SQL | SPARQL-based tools and standards-compliant clients |

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
