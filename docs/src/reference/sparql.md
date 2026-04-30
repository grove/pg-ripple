# SPARQL Reference

This page is the reference for pg_ripple's SPARQL 1.1 query and update engine.

## Overview

pg_ripple implements SPARQL 1.1 Query Language and SPARQL 1.1 Update as native
PostgreSQL SQL functions. All SPARQL execution is performed inside the
extension via the `spargebra` parser, an algebra optimizer (`sparopt`), and a
translation layer that converts SPARQL algebra to PostgreSQL SQL executed
through SPI. Results are decoded back through the dictionary to return RDF
terms as text.

## Status

```sql
SELECT feature_name, status FROM pg_ripple.feature_status()
WHERE feature_name LIKE 'sparql%';
```

## SQL Functions

| Function | Description |
|---|---|
| `pg_ripple.sparql(query TEXT) → SETOF record` | Execute a SPARQL SELECT query |
| `pg_ripple.sparql_update(update TEXT) → void` | Execute SPARQL 1.1 Update (INSERT DATA, DELETE DATA, DELETE/INSERT WHERE, CLEAR, DROP, COPY, MOVE, ADD) |
| `pg_ripple.sparql_construct(query TEXT) → TEXT` | Execute SPARQL CONSTRUCT, return Turtle |
| `pg_ripple.sparql_describe(iri TEXT) → TEXT` | Execute SPARQL DESCRIBE, return Turtle |
| `pg_ripple.sparql_ask(query TEXT) → BOOLEAN` | Execute SPARQL ASK query |
| `pg_ripple.explain_sparql(query TEXT, analyze BOOLEAN) → TEXT` | Return JSON explain plan for a SPARQL query |
| `pg_ripple.sparql_cursor(query TEXT, page_size INT) → TEXT` | Open a server-side cursor for large result sets |
| `pg_ripple.sparql_cursor_next(cursor_id TEXT, page_size INT) → SETOF record` | Fetch next page from cursor |
| `pg_ripple.sparql_cursor_close(cursor_id TEXT) → void` | Close cursor and release resources |
| `pg_ripple.sparql_cursor_turtle(query TEXT, page_size INT) → TEXT` | Open CONSTRUCT cursor returning Turtle pages |
| `pg_ripple.sparql_cursor_jsonld(query TEXT, page_size INT) → TEXT` | Open CONSTRUCT cursor returning JSON-LD pages |
| `pg_ripple.subscribe_sparql(id TEXT, query TEXT, graph_iri TEXT) → void` | Register a live subscription |
| `pg_ripple.unsubscribe_sparql(id TEXT) → void` | Remove a live subscription |
| `pg_ripple.list_sparql_subscriptions() → SETOF record` | List active subscriptions |

## SPARQL 1.1 Feature Coverage

pg_ripple supports the full SPARQL 1.1 specification:

- **SELECT** with projection, DISTINCT, REDUCED, LIMIT, OFFSET, ORDER BY
- **CONSTRUCT** with graph patterns and template triples
- **DESCRIBE** returning a CBD (Concise Bounded Description)
- **ASK** returning boolean
- **Graph patterns**: BGP, OPTIONAL, UNION, MINUS, GRAPH, SERVICE, FILTER, BIND, VALUES
- **Property paths**: `|`, `/`, `^`, `?`, `*`, `+`, `!`, `{n}`, `{n,}`, `{n,m}`
- **Aggregate functions**: COUNT, SUM, MIN, MAX, AVG, GROUP_CONCAT, SAMPLE
- **Built-in functions**: All ~50+ SPARQL 1.1 scalar functions
- **Subqueries**: nested SELECT patterns
- **SPARQL Update**: all 10 update forms

## RDF-star Support

Triple-quoted patterns `<<s p o>>` in both subject and object positions are
supported. The dictionary stores RDF-star terms as encoded triples (hash of the
quoted triple's subject, predicate, and object encoded together).

## Performance Notes

- Integer joins: all SPARQL-to-SQL translation encodes bound terms to `BIGINT`
  before generating SQL; no string comparisons occur inside VP table queries.
- Filter pushdown: FILTER constants are encoded at translation time.
- Self-join elimination: star patterns on the same subject are collapsed into
  single-scan plans.
- The plan cache (`_pg_ripple.plan_cache`) stores compiled SQL for reuse
  across repeated queries.

## Related Pages

- [SPARQL Query SQL Reference](../user-guide/sql-reference/sparql-query.md)
- [SPARQL Update SQL Reference](../user-guide/sql-reference/sparql-update.md)
- [SPARQL Compliance Matrix](sparql-compliance.md)
- [Plan Cache](plan-cache.md)
- [Query Optimization](query-optimization.md)
- [Feature Status Taxonomy](feature-status-taxonomy.md)
