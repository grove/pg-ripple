[← Back to Blog Index](README.md)

# How SPARQL Becomes a PostgreSQL Query Plan

## The translation pipeline from graph patterns to relational algebra

---

SPARQL is a graph pattern matching language. PostgreSQL is a relational database with a cost-based query optimizer. Getting from one to the other is the core technical challenge of pg_ripple.

The naive approach — string-concatenate some SQL and hope for the best — produces query plans that are 10–100× slower than necessary. The careful approach — understanding what the PostgreSQL optimizer can and can't figure out on its own — produces plans that rival hand-written SQL.

This post walks through the translation pipeline: SPARQL text → parsed algebra → optimized algebra → SQL → SPI execution → decoded results. Each stage has specific responsibilities and specific places where performance is won or lost.

---

## Stage 1: Parse

pg_ripple uses `spargebra`, a Rust library that parses SPARQL 1.1 text into an algebraic representation. The algebra is a tree of operations: BGP (Basic Graph Pattern), Join, LeftJoin, Filter, Union, Minus, Group, OrderBy, Slice, Project.

```sparql
SELECT ?name ?email WHERE {
  ?person rdf:type foaf:Person .
  ?person foaf:name ?name .
  ?person foaf:mbox ?email .
  FILTER(CONTAINS(?name, "Alice"))
}
LIMIT 10
```

Parses to (simplified):

```
Slice(limit=10,
  Project([?name, ?email],
    Filter(CONTAINS(?name, "Alice"),
      BGP([
        (?person, rdf:type, foaf:Person),
        (?person, foaf:name, ?name),
        (?person, foaf:mbox, ?email)
      ])
    )
  )
)
```

The parser handles syntax. It doesn't optimize.

---

## Stage 2: Algebra Optimization

Before generating SQL, pg_ripple runs the `sparopt` optimizer over the algebra tree. This stage performs optimizations that are hard for PostgreSQL's SQL optimizer to discover on its own because they depend on knowledge of the VP storage model.

### Self-Join Elimination

The BGP above has three triple patterns with the same subject variable `?person`. In a naive translation, each pattern becomes a separate subquery against a different VP table, and they're joined on `?person`. That's three separate table scans joined by hash or merge join.

But pg_ripple knows something the SQL optimizer doesn't: all three VP tables are indexed on `(s, o)`. A star pattern — multiple predicates, same subject — can be planned as a single driving scan on the table with the best selectivity, with index lookups into the other tables.

The optimizer detects star patterns and rewrites the join tree to use the most selective table as the outer relation, with the others as inner index-lookup joins. For a star pattern with N predicates, this reduces the plan from N hash joins to 1 scan + (N-1) index lookups.

### Filter Pushdown

The `FILTER(CONTAINS(?name, "Alice"))` applies to `?name`, which is bound by the `foaf:name` pattern. The optimizer pushes this filter down to the `foaf:name` VP table scan, so rows that don't match are eliminated before the join rather than after.

Critically, the filter constant ("Alice") is encoded at translation time. The `CONTAINS` function is applied to the decoded string, but only for rows that survive the VP table joins. This is the encode-early, decode-late principle.

### Constant Encoding

Every IRI and literal constant in the query is encoded to its dictionary integer before SQL generation. `rdf:type`, `foaf:Person`, `foaf:name`, `foaf:mbox` are all resolved to their `BIGINT` IDs. The generated SQL never contains string literals in VP table predicates — only integers.

If a constant isn't in the dictionary, pg_ripple knows immediately that no triples exist for it and can short-circuit the entire query to an empty result.

---

## Stage 3: SQL Generation

The optimized algebra tree is compiled to SQL. Each algebra node maps to a SQL construct:

| SPARQL Algebra | SQL |
|---------------|-----|
| BGP (single pattern) | `SELECT s, o FROM vp_{pred_id} WHERE ...` |
| BGP (star pattern) | Multi-table join on subject |
| Join | `INNER JOIN` or subquery join |
| LeftJoin | `LEFT JOIN` with ON condition |
| Filter | `WHERE` clause (or `HAVING` for post-group) |
| Union | `UNION ALL` with deduplication if needed |
| Minus | `EXCEPT` or anti-join |
| Group + Aggregate | `GROUP BY` + aggregate functions |
| OrderBy | `ORDER BY` |
| Slice | `LIMIT` / `OFFSET` |
| Property Path (`+`, `*`) | `WITH RECURSIVE ... CYCLE` |
| SERVICE | Remote SPARQL endpoint call |

The critical rule: **table names are never string-concatenated.** The predicate catalog maps predicate IDs to table OIDs, and `format_ident!`-style quoting prevents SQL injection. This is a security requirement, not a style preference.

### Example Output

The SPARQL query above might compile to:

```sql
WITH star AS (
  SELECT t1.s AS person, t2.o AS name_id, t3.o AS email_id
  FROM _pg_ripple.vp_291 t1          -- rdf:type, filtered to foaf:Person
  JOIN _pg_ripple.vp_42  t2 ON t2.s = t1.s   -- foaf:name
  JOIN _pg_ripple.vp_87  t3 ON t3.s = t1.s   -- foaf:mbox
  WHERE t1.o = 1847293              -- foaf:Person (encoded)
)
SELECT d1.value AS name, d2.value AS email
FROM star
JOIN _pg_ripple.dictionary d1 ON d1.id = star.name_id
JOIN _pg_ripple.dictionary d2 ON d2.id = star.email_id
WHERE d1.value LIKE '%Alice%'
LIMIT 10;
```

Note:
- All VP table joins use integer columns.
- The `rdf:type = foaf:Person` filter is an integer equality (`t1.o = 1847293`).
- The `CONTAINS` filter is applied post-decode, but only on joined rows.
- Dictionary decodes happen at the end.

---

## Stage 4: Execution via SPI

The generated SQL is executed through PostgreSQL's Server Programming Interface (SPI). SPI is the standard mechanism for extensions to run SQL inside the server process. It uses the same query planner, the same executor, the same statistics, and the same transaction context as any client query.

This means:
- PostgreSQL's cost-based optimizer chooses the join order, join method, and access path.
- `EXPLAIN ANALYZE` works on the generated SQL (via `explain_sparql()`).
- Parallel query is available if the optimizer decides it's worthwhile.
- The query participates in the current transaction's snapshot — it sees committed data only, consistent with the calling transaction.

---

## Stage 5: Decode

The SPI result set contains integer IDs. The final stage decodes them back to strings by looking up each ID in the dictionary.

For large result sets, pg_ripple batches the decode: it collects all unique IDs from the result, does a single `SELECT id, value FROM dictionary WHERE id IN (...)` query, builds a local lookup map, and then maps the results. This avoids one dictionary query per row.

For streaming cursors (SPARQL cursors introduced in v0.40.0), the decode happens in chunks as results are fetched, keeping memory bounded.

---

## What the PostgreSQL Optimizer Handles Well

Once the SPARQL-to-SQL translation produces good SQL, the PostgreSQL optimizer is surprisingly effective:

- **Join ordering.** With accurate statistics on each VP table (which is just a regular table with `ANALYZE` statistics), the optimizer picks good join orders.
- **Index selection.** The dual `(s, o)` and `(o, s)` indexes on each VP table give the optimizer access-path choices that match typical SPARQL access patterns.
- **Parallel query.** For large scans (DESCRIBE over millions of triples), PostgreSQL can parallelize the VP table scans.
- **Hash joins.** For non-selective joins (like joining a large `rdf:type` table with a large `foaf:name` table), hash joins are efficient and the optimizer chooses them automatically.

## What It Doesn't Handle

- **VP table selection.** The optimizer doesn't know that `vp_42` is the `foaf:name` table. pg_ripple must resolve predicates to tables before generating SQL.
- **Cross-table cardinality estimation.** The optimizer doesn't know that subjects in `vp_42` and `vp_87` overlap significantly. pg_ripple's algebra optimizer compensates by choosing join order based on predicate statistics from the catalog.
- **Cyclic join detection.** Triangle patterns like "find mutual friends" require worst-case optimal joins (leapfrog triejoin). The standard optimizer doesn't know this pattern and will produce a bad plan. pg_ripple detects cycles in the algebra and compiles them to leapfrog triejoin directly.

The translation pipeline exists because a good SPARQL engine needs to know things about the data layout that a generic SQL optimizer can't know. By the time PostgreSQL sees the SQL, all the graph-specific decisions have already been made. PostgreSQL handles the relational execution — which is what it's best at.
