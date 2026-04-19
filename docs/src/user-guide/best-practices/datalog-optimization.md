# Datalog Optimization

This guide helps you get the most out of the pg_ripple Datalog engine. v0.29.0 introduced several performance features that complement semi-naive evaluation: magic sets, cost-based join reordering, anti-join negation, predicate-filter pushdown, and delta-table indexing.

---

## Choosing between infer() and infer_goal()

| Use case | Recommended function |
|----------|---------------------|
| Materialize everything for a query workload | `infer_with_stats()` |
| One-off question with a specific target | `infer_goal()` |
| SPARQL VIEW backed by inference | `infer_with_stats()` once, then query VP tables |
| Large ontology, selective query, cold cache | `infer_goal()` |

`infer_goal()` shines when the goal pattern eliminates a large fraction of derivable facts. For example, asking "what types does Alice have?" needs only a tiny slice of an RDFS closure. On a 1M-triple dataset with a 5-level `rdfs:subClassOf` hierarchy, `infer_goal()` for a single entity can be 100× faster than full materialization.

Use `infer_with_stats()` when you want to pre-materialize the full closure and then serve many queries from VP storage without re-running inference.

---

## Reading infer_with_stats() output

```sql
SELECT pg_ripple.infer_with_stats('rdfs');
```

Example output:
```json
{
  "derived": 42,
  "iterations": 3,
  "eliminated_rules": []
}
```

| Field | What it tells you |
|-------|------------------|
| `derived` | Total new triples inserted; 0 means the fixpoint was already reached |
| `iterations` | Fixpoint depth; equals the longest derivation chain length |
| `eliminated_rules` | Rules removed by subsumption before evaluation; reduces SQL statements per iteration |

**High `iterations` value?** The engine needed many passes to saturate the relation. This is normal for deep transitive hierarchies. To investigate, increase logging with `SET pg_ripple.inference_mode = 'on_demand'` and look at `EXPLAIN ANALYZE` output for the generated semi-naive SQL (available via `list_rules()`).

**Non-empty `eliminated_rules`?** These rules were provably redundant given the other rules in the set. No action needed; this is a free optimization.

---

## Diagnosing slow fixpoint convergence

### Step 1: Check iteration count

```sql
SELECT pg_ripple.infer_with_stats('my-rules')->>'iterations';
```

More than 20 iterations usually means either:
- A deep recursive chain (expected for hierarchy data)
- A rule set with many cross-referencing rules (consider splitting into finer-grained rule sets)

### Step 2: Check cardinality of VP tables

```sql
SELECT relname, reltuples::bigint AS estimated_rows
FROM pg_class
WHERE relname LIKE 'vp_%'
ORDER BY reltuples DESC;
```

Large VP tables for the predicates in rule bodies slow down each iteration. Use `infer_goal()` to limit the scope.

### Step 3: Force ANALYZE on VP tables

The cost-based reordering (`pg_ripple.datalog_cost_reorder`) uses `pg_class.reltuples`. If these statistics are stale, the reordering may be suboptimal:

```sql
-- Refresh statistics on all VP tables
ANALYZE;
```

### Step 4: Check for delta-table index creation

```sql
-- Temporarily lower the threshold to index all delta tables
SET pg_ripple.delta_index_threshold = 1;
SELECT pg_ripple.infer_with_stats('my-rules');
```

If the index helps, you can lower the threshold permanently for your workload.

---

## Tuning GUCs by dataset size

| Dataset size | Recommended settings |
|-------------|----------------------|
| < 10K triples | Default settings work well |
| 10K–500K triples | `delta_index_threshold = 100`; `datalog_antijoin_threshold = 500` |
| 500K–10M triples | `delta_index_threshold = 50`; enable `magic_sets = true` for selective queries |
| > 10M triples | Use `infer_goal()` for selective queries; `magic_sets = true`; consider partitioned VP tables |

---

## Magic sets GUC

`pg_ripple.magic_sets` (default: `true`) controls whether `infer_goal()` applies the magic sets transformation or falls back to full bottom-up evaluation.

Set to `false` for debugging: both paths should return the same `matching` count; if they don't, there is a bug in the magic sets implementation — please report it.

```sql
-- Debug: compare magic sets vs full evaluation
SET pg_ripple.magic_sets = true;
SELECT pg_ripple.infer_goal('rdfs', '?x rdf:type foaf:Person') AS magic_result;

SET pg_ripple.magic_sets = false;
SELECT pg_ripple.infer_goal('rdfs', '?x rdf:type foaf:Person') AS full_result;
-- The "matching" field should be identical in both results
```

---

## Anti-join negation

For rules with `NOT` in the body, the compiler uses either:

- **NOT EXISTS** — preferred for small VP tables (good for the planner's nested-loop elimination)
- **LEFT JOIN … IS NULL** (anti-join) — preferred for large VP tables (allows hash anti-join and merge anti-join plans)

The threshold is controlled by `pg_ripple.datalog_antijoin_threshold` (default: 1000 rows).

```sql
-- Force anti-join for all negated atoms regardless of table size
SET pg_ripple.datalog_antijoin_threshold = 1;

-- Force NOT EXISTS for all negated atoms
SET pg_ripple.datalog_antijoin_threshold = 0;
```

In practice the default (1000) matches PostgreSQL's own planner heuristics for when hash anti-join becomes beneficial.

---

## Benchmark: magic sets vs full materialization

The included `benchmarks/magic_sets.sql` file demonstrates the performance difference between `infer()` and `infer_goal()` on an RDFS closure over a large class hierarchy.

To run:
```bash
cargo pgrx run pg18
# In psql:
\i benchmarks/magic_sets.sql
```

---

## Aggregate rules — stratification and performance (v0.30.0)

### Aggregation stratification

Aggregate literals (`COUNT`, `SUM`, `MIN`, `MAX`, `AVG`) add a **stratification constraint**: the aggregate rule must be evaluated after all the data it groups over is fully materialized. pg_ripple enforces this automatically via its SCC-based stratifier. If a cycle through aggregation is detected (e.g., a derived predicate P feeds an aggregate that produces another predicate Q which feeds P), the engine emits `WARNING PT510` and skips the aggregate rules.

**Avoid cycles through aggregation:**

```sql
-- ✗ BAD: foaf:knows is derived by rule 1, but rule 2 aggregates over foaf:knows.
--   If the aggregate result feeds back into foaf:knows, this is a PT510 violation.
SELECT pg_ripple.load_rules(
  '?x <foaf:knows> ?y :- ?x <ex:follows> ?y .
   ?x <ex:followCount> ?n :- COUNT(?y WHERE ?x <foaf:knows> ?y) = ?n .
   ?x <ex:follows> ?y :- ?x <ex:followCount> ?n , ?n > 1 .', -- cycle!
  'bad_set');

-- ✓ GOOD: Aggregate over base data only; result is a new predicate with no back-edge.
SELECT pg_ripple.load_rules(
  '?x <ex:followCount> ?n :- COUNT(?y WHERE ?x <ex:follows> ?y) = ?n .',
  'good_set');
```

### Performance tips for aggregate rules

1. **Run `infer_agg()` instead of `infer()`** for rule sets that contain aggregate literals. `infer()` silently skips aggregate literals; `infer_agg()` evaluates them.

2. **Plan cache hit ratio**: On a warm cache, the second and subsequent calls to `infer_agg()` skip compilation entirely. Check hit rates:

   ```sql
   SELECT * FROM pg_ripple.rule_plan_cache_stats();
   -- rule_set     | hits | misses | entries
   -- my_analytics |    9 |      1 |       1
   ```

   A hit rate < 90% may indicate that `load_rules()` is being called unnecessarily (each `load_rules()` invalidates the cache for that rule set).

3. **Use narrow predicates for the aggregate atom**: `COUNT(?y WHERE ?x <ex:knows> ?y)` scans the `ex:knows` VP table. Ensure that predicate has a B-tree index on `(s, o)`.

4. **Batch aggregate rules in a single rule set**: Multiple aggregate rules for the same rule set are compiled in a single `infer_agg()` call; splitting them into separate rule sets multiplies the number of GROUP BY queries.

---

## Rule plan cache tuning (v0.30.0)

The plan cache avoids re-compiling rule SQL on every `infer_agg()` call. Two GUCs control it:

| GUC | Default | Effect |
|-----|---------|--------|
| `pg_ripple.rule_plan_cache` | `true` | Master switch — set `false` to debug cache-related issues |
| `pg_ripple.rule_plan_cache_size` | `64` | Max rule sets cached; oldest entry evicted on overflow |

**Sizing guidelines:**

- If your application has fewer than 64 rule sets (typical), the default is fine.
- For > 64 rule sets, increase `rule_plan_cache_size` to avoid constant eviction:
  ```sql
  ALTER SYSTEM SET pg_ripple.rule_plan_cache_size = 256;
  SELECT pg_reload_conf();
  ```
- Memory cost is low: each cache entry stores a few SQL strings (~1–5 KB typical).

**Cache invalidation:**

The cache is automatically invalidated per rule set when:
- `pg_ripple.load_rules()` is called for that rule set (new rules may change compiled SQL)
- `pg_ripple.drop_rules()` is called for that rule set

The cache is **not** shared across backends (it is process-local). Each new backend connection starts with an empty cache, so the first `infer_agg()` call per backend always incurs a compile step.
