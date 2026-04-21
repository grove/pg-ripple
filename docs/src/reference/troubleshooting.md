# Troubleshooting

## SPARQL query returns 0 rows

**Symptom**: A SPARQL query returns no results even though you expect data to be there.

**Common causes**:

1. **Wrong IRI prefix** — IRIs are case-sensitive and must match exactly. `<https://example.org/Alice>` and `<https://example.org/alice>` are different resources.

   Debug: query the dictionary directly.
   ```sql
   SELECT id, value FROM _pg_ripple.dictionary
   WHERE value LIKE '%alice%';
   ```

2. **Unregistered IRI** — the IRI you used in the query was never loaded into the dictionary. Check `triple_count()` and `find_triples()` to confirm data is present.

3. **Case mismatch in literals** — `"Alice"` and `"alice"` are different literals.

4. **Wrong namespace** — verify the exact predicate IRI used at load time.

**General approach**: use `sparql_explain()` to see the generated SQL, then run it directly to understand what the planner is doing.

```sql
SELECT pg_ripple.sparql_explain(
    'SELECT ?name WHERE { ?p <https://example.org/name> ?name }',
    false
);
```

---

## Property path hangs or times out

**Symptom**: A query using `+` or `*` paths runs forever or is killed by `statement_timeout`.

**Cause**: An unbounded path query on a dense graph can generate millions of recursive CTE iterations before the depth guard fires.

**Fix**:

```sql
-- Cap recursion depth for the current session
SET pg_ripple.max_path_depth = 10;

-- Or set a statement timeout
SET statement_timeout = '5s';
```

**Also check** whether the graph contains a cycle. The `CYCLE` clause in PG18 prevents infinite loops, but very wide fan-out before a cycle is detected still generates many rows.

---

## Aggregate returns unexpected results

**Symptom**: `COUNT(?x)` returns a larger number than expected.

**Cause**: SPARQL aggregate functions count all solution bindings, including duplicates, unless `DISTINCT` is used.

```sparql
-- Counts all bindings, including duplicates
SELECT (COUNT(?x) AS ?n) WHERE { ?s <ex:p> ?x }

-- Counts distinct values only
SELECT (COUNT(DISTINCT ?x) AS ?n) WHERE { ?s <ex:p> ?x }
```

**Also**: if data was loaded multiple times (e.g. in tests), duplicate triples will inflate counts. pg_ripple VP tables do not enforce uniqueness constraints.

---

## load_ntriples returns fewer triples than expected

**Symptom**: `load_ntriples()` returns a count smaller than the number of lines in your file.

**Cause**: Lines with syntax errors are silently skipped by the parser. Blank lines and comment lines (starting with `#`) are also not counted.

**Debug**: check the PostgreSQL server log for parse warnings, or validate the file with an external tool such as Apache Jena `riot`:

```bash
riot --validate input.nt
```

---

## find_triples returns `f` for found_literal

**Symptom**: A triple with a literal object was inserted but `find_triples(..., '"Alice"', ...)` returns no results.

**Cause**: The literal might have been inserted with a language tag or type annotation that you are not including in the search term.

```sql
-- Insert with type
SELECT pg_ripple.insert_triple('<ex:p>', '<ex:name>', '"Alice"^^<xsd:string>');

-- Search must include the type
SELECT * FROM pg_ripple.find_triples(NULL, '<ex:name>', '"Alice"^^<http://www.w3.org/2001/XMLSchema#string>');
```

---

## extension "pg_ripple" has no update path

**Symptom**: `ALTER EXTENSION pg_ripple UPDATE` fails with:
```
ERROR: extension "pg_ripple" has no update path from version "X" to version "Y"
```

**Cause**: The migration script `sql/pg_ripple--X--Y.sql` is missing from the extension directory.

**Fix**: reinstall the extension from the target version's source tree:

```bash
cargo pgrx install --pg-config $(which pg_config)
```

Then retry `ALTER EXTENSION pg_ripple UPDATE`.

---

## Merge worker not running (v0.6.0)

**Symptom**: `SELECT pg_ripple.stats() -> 'merge_worker_pid'` returns `0`.

**Cause**: Either `shared_preload_libraries` is not set, or the worker crashed and was not restarted by the postmaster.

**Fix**:

1. Verify `shared_preload_libraries`:
   ```sql
   SHOW shared_preload_libraries;
   -- Should include 'pg_ripple'
   ```

2. If not set, add it to `postgresql.conf` and restart PostgreSQL:
   ```ini
   shared_preload_libraries = 'pg_ripple'
   ```

3. Set `pg_ripple.worker_database` to match the database where the extension is installed:
   ```ini
   pg_ripple.worker_database = 'mydb'
   ```

4. Check PostgreSQL server logs for crash messages from the worker:
   ```bash
   grep "pg_ripple" $PGDATA/log/postgresql.log | tail -20
   ```

---

## Delta rows not being merged (v0.6.0)

**Symptom**: `"unmerged_delta_rows"` from `stats()` grows continuously without decreasing, even after inserting above the `merge_threshold`.

**Possible causes and fixes**:

1. **Merge threshold not reached**: check that `pg_ripple.merge_threshold` ≤ the current `unmerged_delta_rows`.

2. **Worker is behind on its poll cycle**: lower `merge_interval_secs` or call `SELECT pg_ripple.compact()` to force an immediate merge.

3. **Lock contention**: the merge worker holds a brief exclusive lock during the table swap. If many long-running transactions are open, the swap may be blocked. Monitor with:
   ```sql
   SELECT * FROM pg_locks WHERE relation IN (
       SELECT oid FROM pg_class WHERE relname LIKE '%_delta'
   );
   ```

4. **Watchdog warning**: if `pg_ripple.merge_watchdog_timeout` seconds pass without a successful merge, a `WARNING: pg_ripple merge worker idle for N seconds` message appears in the server log. This is the first sign of a stuck worker.

---

## stats() shows unmerged_delta_rows = -1 (v0.6.0)

**Cause**: pg_ripple was loaded via `CREATE EXTENSION` only, not via `shared_preload_libraries`. The shared-memory atomics were never initialised.

**Fix**: see [Pre-Deployment Checklist](../user-guide/pre-deployment.md) to add pg_ripple to `shared_preload_libraries` and restart PostgreSQL.

---

## CDC notifications not firing (v0.6.0)

**Symptom**: After calling `pg_ripple.subscribe(...)` and `LISTEN my_channel`, no notifications arrive when triples are inserted.

**Possible causes**:

1. **Wrong predicate IRI in the subscription**: the `pattern` argument must exactly match the predicate IRI (with angle brackets, e.g. `'<https://schema.org/name>'`). Use `'*'` to subscribe to all predicates.

2. **Notifications fire on commit**: PostgreSQL `NOTIFY` notifications are delivered after the inserting transaction commits. If you are testing inside a transaction that has not committed, no notification will fire.

3. **Different database**: `LISTEN` and `NOTIFY` are database-scoped. The listener must be in the same database as the inserting session.

4. **Subscription not persisted across restarts**: subscriptions are stored in `_pg_ripple.cdc_subscriptions`. After a server restart, resubscribe:
   ```sql
   SELECT pg_ripple.subscribe('<https://schema.org/name>', 'my_channel');
   ```

---

## Insert rejected by SHACL (v0.7.0)

**Symptom**: An `INSERT` via `insert_triple()` fails with an error like:

```
ERROR:  SHACL violation: <https://example.org/alice> sh:maxCount 1 for
        <https://example.org/email>: found 1 existing value(s), limit is 1
```

**Cause**: `pg_ripple.shacl_mode = 'sync'` is set and the inserted triple would violate an active shape's constraint.

**How to read the violation report**:
- **Focus node** (`<https://example.org/alice>`): the subject of the rejected triple.
- **Constraint** (`sh:maxCount 1`): the violated constraint.
- **Path** (`<https://example.org/email>`): the predicate that triggered the check.
- **Message**: "found N existing value(s), limit is M".

**Resolutions**:

1. Delete the conflicting existing triple first, then insert the new value.
2. Temporarily set `pg_ripple.shacl_mode = 'off'` to load data that violates current shapes, then validate after with `SELECT pg_ripple.validate()`.
3. Use `pg_ripple.shacl_mode = 'async'` for high-throughput inserts — violations are queued rather than rejected.

---

## Async violations not appearing (v0.8.0)

**Symptom**: `pg_ripple.shacl_mode = 'async'` is set and data was inserted, but `dead_letter_count()` returns 0 even though violations are expected.

**Possible causes**:

1. **Queue not yet processed**: The background merge worker drains the queue periodically. Call `SELECT pg_ripple.process_validation_queue()` to drain manually.
2. **No active shapes**: `SELECT count(*) FROM pg_ripple.list_shapes() WHERE active` must be > 0.
3. **Extension not loaded via shared_preload_libraries**: The background worker only runs when the extension is loaded at startup. Without `shared_preload_libraries = 'pg_ripple'`, the worker does not start and the queue accumulates. Use `process_validation_queue()` to drain manually.

```sql
-- Manual drain
SELECT pg_ripple.process_validation_queue();

-- Check for violations
SELECT pg_ripple.dead_letter_count();
SELECT pg_ripple.dead_letter_queue();
```

---

## Dead-letter queue backlog (v0.8.0)

**Symptom**: `dead_letter_count()` returns a large number and keeps growing.

**Causes**:

- Data is being inserted that violates shapes, and `shacl_mode = 'async'` is set.
- The background worker is processing the queue but the violation rate exceeds the drain rate.

**Resolution**:

1. Review violations: `SELECT pg_ripple.dead_letter_queue()`.
2. Fix the upstream data source that is producing violating triples.
3. After fixing, clear the queue: `SELECT pg_ripple.drain_dead_letter_queue()`.
4. If the backlog is too large to process in one call, batch-drain: call `process_validation_queue(1000)` in a loop.

---

## Shape parsing failure (v0.7.0+)

**Symptom**: `load_shacl()` raises an error like:

```
ERROR:  SHACL shape parsing failed: unknown prefix 'my' in token 'my:Foo'
```

**Causes and resolutions**:

1. **Unknown prefix**: The parser only recognises built-in prefixes (`sh:`, `rdf:`, `rdfs:`, `xsd:`, `owl:`). Declare all custom prefixes with `@prefix` directives at the top of the Turtle document.
2. **Missing `sh:path`**: Every `sh:property [...]` block must include `sh:path <predicate>`.
3. **No shapes found**: If the Turtle data contains no `sh:NodeShape` or `sh:PropertyShape` declarations, `load_shacl()` returns 0 and logs a warning — it does not raise an error.

---

## Slow query diagnosis (v0.13.0)

**Symptom**: A SPARQL query is slower than expected and you want to understand why.

**Step 1 — get the generated SQL and plan:**

```sql
-- Get the generated SQL and EXPLAIN output
SELECT pg_ripple.sparql_explain($$
  SELECT ?s ?name WHERE { ?s <https://schema.org/name> ?name }
$$, true);
```

The second argument `true` runs `EXPLAIN ANALYZE`, which shows actual row counts and execution times. The first argument `false` runs `EXPLAIN` only (no actual execution).

**Step 2 — check plan cache efficiency:**

```sql
SELECT pg_ripple.plan_cache_stats();
-- {"hits": 1234, "misses": 56, "size": 48, "capacity": 256, "hit_rate": 0.9567}
```

A low `hit_rate` (< 0.5) suggests queries are not being reused. Common causes:
- Each invocation uses different literal values rather than variables — consider restructuring to use `VALUES` or bindings
- Cache is too small: increase `pg_ripple.plan_cache_size`

**Step 3 — check if BGP reordering is helping:**

```sql
SET pg_ripple.bgp_reorder = off;
EXPLAIN ANALYZE ...;  -- baseline

SET pg_ripple.bgp_reorder = on;
EXPLAIN ANALYZE ...;  -- with reordering
```

If the plan is the same, run `ANALYZE _pg_ripple.vp_rare` and any promoted VP tables so the reordering optimizer has current statistics.

**Step 4 — check statistics freshness:**

```sql
-- See when each VP table was last analyzed
SELECT relname, last_analyze, last_autoanalyze, n_live_tup
FROM pg_stat_user_tables
WHERE relname LIKE 'vp_%'
ORDER BY last_analyze NULLS FIRST;

-- Force fresh statistics
ANALYZE;
```

**Step 5 — check parallel execution:**

```sql
-- See if parallel workers are being used
SHOW max_parallel_workers_per_gather;
SHOW enable_parallel_hash;

-- For multi-join SPARQL queries, set a lower threshold:
SET pg_ripple.parallel_query_min_joins = 2;
```

---

## plan_cache_stats() shows 0 hits (v0.13.0)

**Symptom**: `plan_cache_stats()` returns `"hits": 0` even after running the same SPARQL query multiple times.

**Causes**:

1. **Cache was reset**: `plan_cache_reset()` was called, or the backend restarted (cache is per-backend).
2. **bgp_reorder changed**: if `pg_ripple.bgp_reorder` was toggled between queries, the cache key differs and a cache hit cannot occur. Set a consistent value per session.
3. **Cache disabled**: if `pg_ripple.plan_cache_size = 0`, the cache is disabled. Set to a positive value.

```sql
SHOW pg_ripple.plan_cache_size;
SET pg_ripple.plan_cache_size = 256;  -- enable if it was 0
```

---

## Vector Federation Timeouts (v0.28.0)

**Symptom**: Calls to `pg_ripple.hybrid_search()` with a registered external endpoint fail with a timeout error.

**Diagnosis**: The external vector service is either unreachable or slow. pg_ripple makes a synchronous HTTP call to the registered endpoint; if it does not respond within `pg_ripple.vector_federation_timeout_ms` milliseconds, the call fails.

**Fix**:

1. Check endpoint connectivity:
   ```sql
   SELECT url, enabled FROM _pg_ripple.vector_endpoints;
   ```

2. Increase the timeout:
   ```sql
   SET pg_ripple.vector_federation_timeout_ms = 30000;
   ```

3. If the endpoint is permanently unavailable, disable it:
   ```sql
   UPDATE _pg_ripple.vector_endpoints SET enabled = false WHERE url = 'https://my-endpoint/';
   ```

---

## SSRF Errors on Vector Endpoint Registration

**Symptom**: `pg_ripple.register_vector_endpoint()` registers a URL, but the federation call fails when that URL points to an internal service not meant to be accessed from PostgreSQL.

**Prevention**: Use network-level controls (AWS security groups, Kubernetes NetworkPolicy, or firewall rules) to restrict which external hosts your PostgreSQL server can reach. pg_ripple does not perform SSRF validation at registration time — network policies are the correct enforcement layer.

---

## Endpoint Unreachable After Registration

**Symptom**: A vector endpoint was registered with `register_vector_endpoint()`, but `hybrid_search()` returns zero results with a `PT607` warning.

**Diagnosis**:
- The endpoint URL may have changed.
- The endpoint service may be down.
- The `enabled` column in `_pg_ripple.vector_endpoints` may have been set to `false`.

**Fix**:

```sql
-- Re-register with the correct URL
SELECT pg_ripple.register_vector_endpoint('https://new-url/', 'qdrant');

-- Or re-enable a disabled endpoint
UPDATE _pg_ripple.vector_endpoints SET enabled = true WHERE url = 'https://my-endpoint/';
```


---

## Rare-Predicate Promotion Stuck or Inconsistent (v0.45.0)

**Symptom**: After a PostgreSQL crash during a large batch insert that crossed the rare-predicate promotion threshold, `_pg_ripple.vp_rare` contains rows for a predicate that also has a promoted VP table — or vice versa.

**Diagnosis**:

```sql
SELECT * FROM pg_ripple.diagnostic_report();
-- Look for predicates with 'promotion_state' != 'complete' or 'none'
```

A valid state is either:
- **Promoted**: VP table exists, `_pg_ripple.vp_rare` has zero rows for that predicate, `predicates.derived = true`
- **Not promoted**: No VP table, `_pg_ripple.vp_rare` has rows, `predicates.derived = false`

**Fix**: If the state is hybrid (partial promotion), the safest recovery is to roll back the promotion manually:

```sql
-- 1. Drop the partial VP table (if it exists and is empty or inconsistent)
DROP TABLE IF EXISTS _pg_ripple."vp_<predicate_id>";

-- 2. Ensure vp_rare still has the rows (they should not have been deleted)
SELECT count(*) FROM _pg_ripple.vp_rare WHERE p = <predicate_id>;

-- 3. Reset promotion flag
UPDATE _pg_ripple.predicates SET derived = false WHERE id = <predicate_id>;
```

Then re-run the insert batch. The promotion will be re-attempted cleanly.

---

## Inference Aborted Mid-Fixpoint (v0.45.0)

**Symptom**: A call to `infer()` or `infer_wfs()` was killed (e.g., `pg_cancel_backend()`, `pg_terminate_backend()`, or server crash) while the fixpoint was running. You suspect partially-derived facts may remain.

**Diagnosis**:

```sql
-- Check for any inferred triples from your rule set:
SELECT count(*) FROM _pg_ripple.vp_rare WHERE p IN (
    SELECT id FROM _pg_ripple.predicates WHERE rule_set = 'your_rule_set'
);
```

**Expected outcome**: pg_ripple's inference engine accumulates derived facts in PostgreSQL TEMP tables during the fixpoint. TEMP tables are automatically dropped (and all writes rolled back) when the session ends abnormally. Therefore, a killed inference session leaves **zero partial facts** in persistent storage.

If you find facts that appear to be from a failed inference run, they are more likely from a previously-completed run. Use `drop_rules('rule_set')` to remove the rule set and all its derived facts, then re-run `load_rules()` and `infer()` from scratch.
