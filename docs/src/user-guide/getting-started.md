# Getting Started

This guide walks you through five minutes of hands-on pg_ripple usage: create the extension, insert some triples, bulk-load a Turtle file, and run a SPARQL query.

## 1. Create the extension

```sql
CREATE EXTENSION pg_ripple;
```

## 2. Insert individual triples

```sql
-- IRI subject, IRI predicate, IRI object (N-Triples notation)
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
);

-- Literal object
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<https://example.org/name>',
    '"Alice"'
);

-- Language-tagged literal
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<https://example.org/bio>',
    '"Linked data enthusiast"@en'
);

-- Typed literal
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<https://example.org/age>',
    '"30"^^<http://www.w3.org/2001/XMLSchema#integer>'
);
```

## 3. Run a SPARQL query

```sql
SELECT *
FROM pg_ripple.sparql('
  SELECT ?name ?age WHERE {
    ?person <https://example.org/name> ?name .
    OPTIONAL { ?person <https://example.org/age> ?age }
  }
');
```

`sparql()` returns a relational table with one column per projected variable, all typed `TEXT`.

## 4. Bulk-load N-Triples

For larger datasets use `load_ntriples()`:

```sql
SELECT pg_ripple.load_ntriples('
<https://example.org/bob>   <https://example.org/name>  "Bob" .
<https://example.org/bob>   <https://example.org/knows> <https://example.org/carol> .
<https://example.org/carol> <https://example.org/name>  "Carol" .
');
-- Returns the number of triples loaded
```

To load from a file:

```sql
SELECT pg_ripple.load_ntriples_file('/data/dataset.nt');
```

For Turtle files:

```sql
SELECT pg_ripple.load_turtle('/data/dataset.ttl');
```

## 5. Count and find triples

```sql
-- Total triples
SELECT pg_ripple.triple_count();

-- Find triples by subject (NULL is a wildcard)
SELECT * FROM pg_ripple.find_triples('<https://example.org/alice>', NULL, NULL);

-- Find all outgoing edges from alice
SELECT subject, predicate, object
FROM pg_ripple.find_triples('<https://example.org/alice>', NULL, NULL);
```

## 6. Register a prefix

Prefixes shorten N-Triples-style terms but are not required. They are used by the
export functions.

```sql
SELECT pg_ripple.register_prefix('ex', 'https://example.org/');
```

## 7. ASK and EXPLAIN

```sql
-- Boolean query
SELECT pg_ripple.sparql_ask('ASK { <https://example.org/alice> <https://example.org/knows> ?x }');

-- Inspect the generated SQL
SELECT pg_ripple.sparql_explain(
    'SELECT ?name WHERE { ?p <https://example.org/name> ?name }',
    false  -- false = text format
);
```

## Next steps

- [SQL Reference](sql-reference/index.md) — full API documentation
- [Configuration](configuration.md) — GUC parameters
- [Playground](playground.md) — Docker sandbox: try pg_ripple without installing anything
- [SPARQL Patterns](best-practices/sparql-patterns.md) — writing efficient queries
