# SPARQL Queries

pg_ripple executes SPARQL 1.1 queries by compiling them to SQL that runs natively inside the PostgreSQL engine. The generated SQL is visible via `sparql_explain()`.

## sparql

```sql
pg_ripple.sparql(query TEXT) RETURNS TABLE(…)
```

Executes a SPARQL SELECT query and returns results as a relational table. Each projected variable becomes a `TEXT` column. Values are returned in N-Triples notation for IRIs and blank nodes, and literal notation for literals.

```sql
SELECT name, age
FROM pg_ripple.sparql('
  SELECT ?name ?age WHERE {
    ?person <https://example.org/name> ?name .
    OPTIONAL { ?person <https://example.org/age> ?age }
  }
');
```

> **Note**: The column names in the returned table match the SPARQL variable names without the `?` prefix.

---

## sparql_ask

```sql
pg_ripple.sparql_ask(query TEXT) RETURNS BOOLEAN
```

Executes a SPARQL ASK query and returns `true` if at least one solution exists.

```sql
SELECT pg_ripple.sparql_ask(
    'ASK { <https://example.org/alice> <https://example.org/knows> ?x }'
);
```

---

## sparql_explain

```sql
pg_ripple.sparql_explain(query TEXT, verbose BOOLEAN DEFAULT FALSE) RETURNS TEXT
```

Returns the SQL generated for a SPARQL query without executing it. Useful for debugging and performance analysis.

```sql
SELECT pg_ripple.sparql_explain(
    'SELECT ?name WHERE { ?p <https://example.org/name> ?name }',
    false
);
```

---

## Supported SPARQL 1.1 features

### SELECT

```sparql
SELECT ?s ?p WHERE { ?s ?p <https://example.org/bob> }
SELECT DISTINCT ?s WHERE { ?s <ex:knows> ?o }
SELECT ?s WHERE { ?s <ex:knows> ?o } ORDER BY ?s LIMIT 10 OFFSET 5
```

### FILTER

```sparql
SELECT ?name WHERE {
  ?p <ex:name> ?name .
  FILTER(STRLEN(?name) > 3)
}

SELECT ?age WHERE {
  ?p <ex:age> ?age .
  FILTER(?age >= 18 && ?age < 65)
}
```

### OPTIONAL (LeftJoin)

```sparql
SELECT ?person ?name ?email WHERE {
  ?person <ex:worksAt> <ex:acme> .
  OPTIONAL { ?person <ex:name> ?name }
  OPTIONAL { ?person <ex:email> ?email }
}
```

### UNION / MINUS

```sparql
-- UNION
SELECT ?contact WHERE {
  { ?alice <ex:knows> ?contact } UNION { ?bob <ex:knows> ?contact }
}

-- MINUS (anti-join)
SELECT ?person WHERE {
  ?person <ex:worksAt> ?company .
  MINUS { ?person <ex:worksAt> <ex:acme> }
}
```

### Aggregates and GROUP BY

```sparql
SELECT ?company (COUNT(?person) AS ?headcount) WHERE {
  ?person <ex:worksAt> ?company
} GROUP BY ?company HAVING (COUNT(?person) >= 2)
```

Supported aggregate functions: `COUNT`, `SUM`, `AVG`, `MIN`, `MAX`, `GROUP_CONCAT`.

### Subqueries

```sparql
SELECT ?company ?headcount WHERE {
  {
    SELECT ?company (COUNT(?p) AS ?headcount) WHERE {
      ?p <ex:worksAt> ?company
    } GROUP BY ?company
  }
  FILTER(?headcount >= 2)
}
```

### BIND / VALUES

```sparql
-- BIND
SELECT ?person ?label WHERE {
  ?person <ex:worksAt> ?company .
  BIND(<ex:employee> AS ?label)
}

-- VALUES (inline data)
SELECT ?person ?company WHERE {
  VALUES ?person { <ex:alice> <ex:bob> }
  ?person <ex:worksAt> ?company
}
```

### Property paths

```sparql
-- OneOrMore (+): transitive closure
SELECT ?target WHERE { <ex:alice> <ex:knows>+ ?target }

-- ZeroOrMore (*): transitive closure including identity
SELECT ?target WHERE { <ex:alice> <ex:follows>* ?target }

-- ZeroOrOne (?): direct or identity
SELECT ?target WHERE { <ex:alice> <ex:follows>? ?target }

-- Sequence (/)
SELECT ?target WHERE { <ex:alice> <ex:knows>/<ex:knows> ?target }

-- Alternative (|)
SELECT ?target WHERE { <ex:alice> (<ex:knows>|<ex:follows>) ?target }

-- Inverse (^)
SELECT ?who WHERE { ?who ^<ex:knows> <ex:bob> }
```

Property paths compile to PostgreSQL `WITH RECURSIVE` CTEs with the PG18 `CYCLE` clause for hash-based cycle detection. See [max_path_depth](../configuration.md#max_path_depth) to limit traversal depth.

### Named graphs

```sparql
SELECT ?s ?p ?o WHERE {
  GRAPH <https://example.org/graph1> { ?s ?p ?o }
}
```

### ASK

```sparql
ASK { <ex:alice> <ex:knows> <ex:bob> }
```

---

## Plan cache

Compiled SPARQL→SQL plans are cached per-backend in an LRU cache (configurable via [`pg_ripple.plan_cache_size`](../configuration.md#plan_cache_size)). Repeated identical queries skip recompilation.

The cache key includes the query text and the current value of `pg_ripple.max_path_depth`. Changing the GUC invalidates cached path query plans.
