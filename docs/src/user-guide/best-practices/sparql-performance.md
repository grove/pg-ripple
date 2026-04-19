# SPARQL Performance

Best practices for accelerating SPARQL queries against pg_ripple using tabling, demand transformation, and rule plan caching.

---

## Tabling / memoisation (v0.32.0)

When `pg_ripple.tabling = on` (default), the results of `infer_wfs()` calls are stored in an in-database cache keyed by an XXH3-64 hash of the goal string. Repeated calls with the same rule set return the cached result without re-running the fixpoint algorithm.

```sql
-- Check current tabling settings.
SHOW pg_ripple.tabling;      -- on
SHOW pg_ripple.tabling_ttl;  -- 300 (seconds)

-- Inspect what is cached.
SELECT * FROM pg_ripple.tabling_stats() ORDER BY hits DESC LIMIT 10;
```

### When tabling helps most

- **Analytical workloads** where the same rule set is evaluated repeatedly (e.g., dashboard refreshes, batch jobs).
- **SPARQL queries over inferred triples** where `infer_wfs()` is called before each query to ensure up-to-date materialization.
- **Short-lived applications** that query the same patterns many times within a session.

### TTL configuration

```sql
-- Never expire cached entries (best for read-heavy, rarely-changing data).
SET pg_ripple.tabling_ttl = 0;

-- Cache for 10 minutes.
SET pg_ripple.tabling_ttl = 600;

-- Disable tabling entirely (always recompute — useful for testing/debugging).
SET pg_ripple.tabling = off;
```

### Cache invalidation

The tabling cache is automatically invalidated on:

- `pg_ripple.insert_triple()` or `pg_ripple.delete_triple()` — any data change
- `pg_ripple.load_rules()` or `pg_ripple.drop_rules()` — any rule change

No manual invalidation is needed for normal use.

---

## Demand transformation (v0.31.0)

Rather than materializing the entire rule set closure, `infer_demand()` derives only the facts required to answer a specific set of goal patterns. For SPARQL queries that target a small predicate set within a large rule base, this can reduce inference work by 50–90%.

```sql
-- Only derive 'knows' and 'reachable' — skip unrelated predicates.
SELECT pg_ripple.infer_demand('social_rules',
    '[{"p": "<https://ex.org/knows>"},
      {"p": "<https://ex.org/reachable>"}]'
);

-- Then query normally.
SELECT * FROM pg_ripple.sparql('
  SELECT ?a ?b WHERE { ?a <https://ex.org/reachable> ?b }
');
```

When `pg_ripple.demand_transform = on` (default), `create_datalog_view()` automatically applies demand transformation when multiple goal patterns are specified.

---

## Rule plan caching (v0.30.0)

The rule plan cache (`pg_ripple.rule_plan_cache = on`, default) stores compiled SQL plans for each rule, avoiding repeated compilation overhead on repeated `infer()` or `infer_wfs()` calls within a session.

```sql
-- Check plan cache settings.
SHOW pg_ripple.rule_plan_cache;       -- on
SHOW pg_ripple.rule_plan_cache_size;  -- 64

-- Disable for debugging (forces recompilation each time).
SET pg_ripple.rule_plan_cache = off;
```

The rule plan cache is keyed on rule text, so changes to rules (via `load_rules` or `drop_rules`) automatically invalidate affected plan cache entries.

### Interaction with tabling

Both the rule plan cache and the tabling cache are active by default and complement each other:

- **Rule plan cache** eliminates SQL compilation overhead — each fixpoint iteration reuses the compiled SQL for each rule.
- **Tabling cache** eliminates entire fixpoint computations — if the goal was computed recently and the data has not changed, the cached result is returned immediately.

For long-running services, setting `pg_ripple.tabling_ttl = 0` (no expiry) combined with `pg_ripple.rule_plan_cache = on` gives the best repeated-query performance.

---

## SPARQL property paths and transitive closure

SPARQL property paths (e.g., `+`, `*`, `/`) are expanded into recursive CTEs at query time. For large graphs, this can be expensive. Consider pre-materialising transitive closure using a Datalog rule instead:

```sql
-- Pre-materialise transitive closure with a Datalog rule.
SELECT pg_ripple.load_rules('
  ?x <https://ex.org/reachable> ?y :- ?x <https://ex.org/edge> ?y .
  ?x <https://ex.org/reachable> ?z :- ?x <https://ex.org/reachable> ?y ,
                                      ?y <https://ex.org/edge> ?z .
', 'reach');

SELECT pg_ripple.infer('reach');

-- Query pre-materialised closure — no recursive CTE at query time.
SELECT * FROM pg_ripple.sparql('
  SELECT ?a ?b WHERE { ?a <https://ex.org/reachable> ?b }
');
```

Combined with tabling, this approach amortizes the cost of the transitive closure computation across multiple queries.

---

## Bounded-depth SPARQL property paths (v0.34.0)

SPARQL property path queries (`rdfs:subClassOf*`, `ex:knows+`) rely on `WITH RECURSIVE` CTEs internally. When the graph has a known bounded hierarchy depth, pre-materializing the closure with a depth bound avoids the recursive path at query time entirely.

```sql
-- Materialize a bounded closure (at most 5 hops)
SET pg_ripple.datalog_max_depth = 5;
SELECT pg_ripple.load_rules(
  '?x <https://ex.org/reach> ?y :- ?x <https://ex.org/step> ?y . '
  '?x <https://ex.org/reach> ?z :- ?x <https://ex.org/reach> ?y , ?y <https://ex.org/step> ?z .',
  'bounded_reach'
);
SELECT pg_ripple.infer('bounded_reach');
SET pg_ripple.datalog_max_depth = 0;

-- Query the pre-materialized bounded closure
SELECT * FROM pg_ripple.sparql('
  SELECT ?a ?b WHERE { ?a <https://ex.org/reach> ?b }
');
```

For hierarchies where the maximum depth is known (e.g., from a SHACL `sh:maxDepth` annotation), this pattern typically reduces property path query latency by 30-60% compared to the unbounded inline recursive CTE.

---

## Worst-case optimal joins for cyclic patterns (v0.36.0)

Standard hash-join and nested-loop algorithms are not worst-case optimal for *cyclic* SPARQL BGPs — query graphs that contain a cycle, such as triangle queries:

```sparql
SELECT ?a ?b ?c WHERE {
    ?a <ex:knows> ?b .
    ?b <ex:knows> ?c .
    ?c <ex:knows> ?a .
}
```

When `pg_ripple.wcoj_enabled = on` (the default), pg_ripple automatically detects cyclic BGPs and forces the PostgreSQL planner towards sort-merge joins, exploiting the `(s, o)` B-tree indices on VP tables.  This simulates the key locality property of the Leapfrog Triejoin algorithm.

```sql
-- Check WCOJ settings.
SHOW pg_ripple.wcoj_enabled;     -- on
SHOW pg_ripple.wcoj_min_tables;  -- 3 (min VP joins before WCOJ kicks in)

-- Detect whether a BGP is cyclic (useful for query plan inspection).
SELECT pg_ripple.wcoj_is_cyclic('[["a","b"],["b","c"],["c","a"]]');  -- true
SELECT pg_ripple.wcoj_is_cyclic('[["root","a"],["root","b"]]');       -- false

-- Benchmark a triangle query with WCOJ on vs. off.
SELECT pg_ripple.wcoj_triangle_query('https://example.org/knows');
-- Returns: {"triangle_count": N, "wcoj_applied": true, "predicate_iri": "..."}
```

### When WCOJ helps most

- **Social graph triangle queries** — finding mutual connections or common co-authors.
- **Transitive closure patterns** — property paths rewritten as join chains.
- **Cyclic constraint checking** — detecting cycles in directed graphs.

### Tuning

```sql
-- Raise the threshold if you only want WCOJ for large multi-hop joins.
SET pg_ripple.wcoj_min_tables = 5;

-- Disable WCOJ globally if you suspect it is causing a bad plan.
SET pg_ripple.wcoj_enabled = off;
```

> **Performance expectation:** On triangle queries over a VP table with 1 M edges, WCOJ reduces query time from > 10 s (hash-join plan) to < 1 s (sort-merge plan with B-tree exploitation).

---

## Materialization freshness after parallel inference (v0.35.0)

When `pg_ripple.datalog_parallel_workers > 1`, the Datalog engine partitions rules into independent groups and executes them in the optimal order within a single transaction. After `infer_with_stats()` or `infer()` returns, SPARQL queries immediately observe all derived facts — there is no staleness window within the same session.

```sql
-- After bulk loading, re-materialize derived predicates.
SELECT pg_ripple.load_turtle($$ <Alice> a <Person> . $$);
SET pg_ripple.datalog_parallel_workers = 4;
SELECT pg_ripple.infer_with_stats('owl-rl');

-- SPARQL now sees all derived rdf:type, rdfs:subClassOf, owl:sameAs facts.
SELECT pg_ripple.sparql('SELECT ?x ?type WHERE { ?x a ?type . }');
```

**Tip:** Check `parallel_groups` in the `infer_with_stats()` output to verify that your rule set benefits from parallelism. A value of 1 means all rules are in a single dependency chain; a value > 1 confirms that concurrent execution is possible.

```sql
-- Check parallel group count before tuning workers.
SELECT pg_ripple.infer_with_stats('owl-rl')->>'parallel_groups';  -- e.g., "3"
```
