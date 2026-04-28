[← Back to Blog Index](README.md)

# EXPLAIN SPARQL: Inside the Query Plan

## EXPLAIN ANALYZE for graph queries — see what PostgreSQL actually does with your SPARQL

---

Your SPARQL query takes 3 seconds. Is it the join order? A missing index? A VP table scan that should be an index lookup? A federation call that's timing out?

In SQL, you'd run `EXPLAIN ANALYZE`. In pg_ripple, you run `explain_sparql()`.

---

## The Function

```sql
SELECT pg_ripple.explain_sparql(
  '
  SELECT ?name ?email WHERE {
    ?person rdf:type foaf:Person .
    ?person foaf:name ?name .
    ?person foaf:mbox ?email .
    FILTER(CONTAINS(?name, "Alice"))
  }
  LIMIT 10
  ',
  analyze => true
);
```

Returns a JSONB document with four sections:

```json
{
  "algebra": { ... },
  "sql": "WITH star AS ( ... ) SELECT ...",
  "plan": { ... },
  "statistics": {
    "planning_time_ms": 0.8,
    "execution_time_ms": 12.3,
    "rows_returned": 3,
    "dictionary_decodes": 6,
    "vp_tables_scanned": 3,
    "plan_cache_hit": false
  }
}
```

---

## Section 1: The Algebra Tree

The `algebra` section shows the parsed and optimized SPARQL algebra — the intermediate representation before SQL generation:

```json
{
  "algebra": {
    "type": "Slice",
    "limit": 10,
    "child": {
      "type": "Project",
      "variables": ["?name", "?email"],
      "child": {
        "type": "Filter",
        "expression": "CONTAINS(?name, 'Alice')",
        "child": {
          "type": "StarJoin",
          "subject": "?person",
          "patterns": [
            {"predicate": "rdf:type", "object": "foaf:Person", "table": "vp_291"},
            {"predicate": "foaf:name", "object": "?name", "table": "vp_42"},
            {"predicate": "foaf:mbox", "object": "?email", "table": "vp_87"}
          ],
          "driving_table": "vp_291",
          "estimated_rows": 12000
        }
      }
    }
  }
}
```

This tells you:
- The optimizer detected a star pattern and chose `vp_291` (rdf:type) as the driving table.
- The filter was pushed down.
- Estimated cardinality is 12,000 rows before the filter.

If you see a `NestedLoopJoin` where you expected a `StarJoin`, or a `SequentialScan` where you expected an `IndexLookup`, the algebra section tells you where the optimizer's plan diverges from your expectations.

---

## Section 2: The Generated SQL

The `sql` section shows the exact SQL that pg_ripple sends to PostgreSQL via SPI:

```sql
WITH star AS (
  SELECT t1.s AS person, t2.o AS name_id, t3.o AS email_id
  FROM _pg_ripple.vp_291 t1
  JOIN _pg_ripple.vp_42  t2 ON t2.s = t1.s
  JOIN _pg_ripple.vp_87  t3 ON t3.s = t1.s
  WHERE t1.o = 1847293
)
SELECT d1.value AS name, d2.value AS email
FROM star
JOIN _pg_ripple.dictionary d1 ON d1.id = star.name_id
JOIN _pg_ripple.dictionary d2 ON d2.id = star.email_id
WHERE d1.value LIKE '%Alice%'
LIMIT 10
```

You can copy this SQL, paste it into `psql`, and run `EXPLAIN ANALYZE` on it directly. This is useful when you want to experiment with hints, set `work_mem`, or compare plan choices.

---

## Section 3: The PostgreSQL Plan

When `analyze => true`, pg_ripple runs `EXPLAIN ANALYZE` on the generated SQL and includes the PostgreSQL query plan:

```json
{
  "plan": {
    "Node Type": "Limit",
    "Actual Rows": 3,
    "Actual Total Time": 11.5,
    "Plans": [
      {
        "Node Type": "Nested Loop",
        "Join Type": "Inner",
        "Actual Rows": 3,
        "Plans": [
          {
            "Node Type": "Index Scan",
            "Index Name": "vp_291_s_o_idx",
            "Actual Rows": 12000
          }
        ]
      }
    ]
  }
}
```

This is the same output you'd get from `EXPLAIN (ANALYZE, FORMAT JSON)` in PostgreSQL, but embedded in the SPARQL explain output. You can see exactly which indexes were used, which join methods were chosen, and where time was spent.

---

## Section 4: Statistics

The `statistics` section aggregates key metrics:

- **planning_time_ms:** Time to translate SPARQL to SQL (algebra optimization + SQL generation).
- **execution_time_ms:** Time to execute the SQL + decode results.
- **rows_returned:** Final result count.
- **dictionary_decodes:** Number of dictionary lookups for result decoding.
- **vp_tables_scanned:** Number of VP tables touched.
- **plan_cache_hit:** Whether the SQL plan was served from pg_ripple's plan cache.

The plan cache is worth noting: pg_ripple caches the SQL plan for frequently executed SPARQL queries (keyed by the normalized algebra tree). A cache hit means the SPARQL-to-SQL translation is skipped entirely, saving 0.5–2ms per query.

---

## Explain for Datalog

`explain_datalog()` provides similar introspection for Datalog inference:

```sql
SELECT pg_ripple.explain_datalog(
  ruleset => 'owl2rl',
  analyze => true
);
```

Returns:

```json
{
  "strata": [
    {
      "stratum": 0,
      "rules": ["rdfs_subClassOf_trans", "rdf_type_propagation"],
      "iterations": 7,
      "new_facts_per_iteration": [12000, 3400, 890, 210, 48, 7, 0],
      "execution_time_ms": 450,
      "parallel_workers": 2
    },
    {
      "stratum": 1,
      "rules": ["owl_sameAs_closure"],
      "iterations": 3,
      "execution_time_ms": 120,
      "parallel_workers": 1
    }
  ],
  "total_new_facts": 16555,
  "total_time_ms": 570
}
```

This shows:
- How many strata the rule set was decomposed into.
- How many iterations each stratum took to reach fixpoint.
- The convergence curve (new facts per iteration — you want this to decrease rapidly).
- Whether parallel workers were used.

If a stratum takes 50 iterations to converge, you might have a rule cycle that's producing too many intermediate facts. If one stratum dominates the total time, its rules are the optimization target.

---

## Practical Debugging

### Slow Star Pattern

```
explain_sparql shows:
  StarJoin driving table: vp_42 (foaf:name), estimated_rows: 5,000,000

Problem: foaf:name is the least selective predicate.
Fix: The optimizer should use vp_291 (rdf:type) filtered to a specific class.
```

Check if the predicate catalog has accurate statistics:

```sql
SELECT * FROM _pg_ripple.predicates ORDER BY triple_count DESC LIMIT 10;
```

If `triple_count` is stale, run `ANALYZE` on the VP tables.

### Missing Index Scan

```
PostgreSQL plan shows: Seq Scan on vp_42 (rows: 5,000,000)

Expected: Index Scan on vp_42_s_o_idx
```

This usually means the query has a variable in the subject position and the optimizer estimated that a sequential scan is cheaper than an index scan. Check `shared_buffers` — if the table fits in memory, a sequential scan can indeed be faster.

### Federation Timeout

```
statistics: { execution_time_ms: 30000, federation_calls: 1 }

The SERVICE clause is the bottleneck.
```

Check the federation pool configuration and the remote endpoint's health. Consider caching the result.

---

## When to Use Explain

- **Before deploying a new SPARQL query to production.** Check that the plan is sensible.
- **When a query suddenly gets slow.** Table statistics may have changed, causing a plan regression.
- **When comparing Datalog rule sets.** Different rule formulations produce different convergence profiles. Explain reveals which is more efficient.
- **When debugging federation.** The explain output shows which SERVICE calls were made and how long each took.

`explain_sparql()` is the debugger for your knowledge graph queries. Use it the same way you'd use `EXPLAIN ANALYZE` for SQL — early, often, and before blaming the database.
