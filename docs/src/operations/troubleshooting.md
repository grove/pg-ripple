# Troubleshooting

A runbook of common issues, their causes, and step-by-step resolutions. Each entry follows the pattern: **Symptom → Cause → Diagnostic → Fix**.

---

## 1. SPARQL Query Returns Zero Rows

**Symptom**: A SPARQL query that should return results returns an empty set.

**Cause**: The most common cause is querying with unencoded IRIs that don't match the dictionary, or querying the wrong graph.

**Diagnostic**:

```sql
-- Check that triples exist
SELECT pg_ripple.stats();

-- Verify the IRI is in the dictionary
SELECT id FROM _pg_ripple.dictionary WHERE value = 'http://example.org/MyResource';

-- Check the default graph vs named graphs
SELECT pg_ripple.sparql($$
  SELECT ?g (COUNT(*) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } } GROUP BY ?g
$$);
```

**Fix**: Ensure the query uses the exact IRI as stored (case-sensitive, no trailing slash differences). If data was loaded into a named graph, use `GRAPH` or `FROM` clauses.

---

## 2. Merge Worker Not Running

**Symptom**: `pg_ripple.stats()` shows `merge_worker_pid: 0`. Delta rows accumulate.

**Cause**: `pg_ripple` is not in `shared_preload_libraries`, or `worker_database` points to the wrong database.

**Diagnostic**:

```sql
SHOW shared_preload_libraries;
SHOW pg_ripple.worker_database;
```

**Fix**:

```ini
# postgresql.conf
shared_preload_libraries = 'pg_ripple'
pg_ripple.worker_database = 'mydb'
```

Restart PostgreSQL. Verify with:

```sql
SELECT (pg_ripple.stats()->>'merge_worker_pid')::int;
```

---

## 3. Slow Queries — Unbounded Property Paths

**Symptom**: Queries with `*` or `+` property paths take minutes or never complete.

**Cause**: Property path queries compile to `WITH RECURSIVE` CTEs. On large, highly-connected graphs, recursion explores an enormous search space.

**Diagnostic**:

```sql
SHOW pg_ripple.max_path_depth;
-- Check the generated SQL
SET pg_ripple.plan_cache_size = 0;  -- disable cache to see fresh plans
EXPLAIN (ANALYZE, BUFFERS) <generated SQL from logs>;
```

**Fix**: Limit recursion depth:

```sql
SET pg_ripple.max_path_depth = 10;
```

Or rewrite the query to use a bounded path (`{1,5}`) instead of `*`/`+`.

---

## 4. SHACL Validation Not Triggering

**Symptom**: Data that violates SHACL shapes is inserted without errors.

**Cause**: SHACL enforcement is asynchronous by default, or the shapes are not loaded.

**Diagnostic**:

```sql
-- Check loaded shapes
SELECT pg_ripple.sparql($$
  SELECT ?shape WHERE { ?shape a <http://www.w3.org/ns/shacl#NodeShape> }
$$);

-- Check enforce mode
SHOW pg_ripple.enforce_constraints;
```

**Fix**: Set enforcement mode to `'error'` for synchronous validation:

```sql
SET pg_ripple.enforce_constraints = 'error';
```

Reload shapes if needed:

```sql
SELECT pg_ripple.load_shapes('<shapes-graph-iri>');
```

---

## 5. Datalog Inference Produces No Results

**Symptom**: `pg_ripple.infer()` or `pg_ripple.infer_goal()` returns zero new triples.

**Cause**: Rules are not loaded, inference mode is `'off'`, or the rule atoms don't match any data.

**Diagnostic**:

```sql
SHOW pg_ripple.inference_mode;

-- List loaded rule sets
SELECT pg_ripple.list_rule_sets();

-- Test with a simple rule
SELECT pg_ripple.load_rules('test', $$
  :Parent(?x, ?z) :- :Parent(?x, ?y), :Parent(?y, ?z).
$$);
SELECT pg_ripple.infer('test');
```

**Fix**: Ensure `inference_mode` is `'on_demand'` or `'materialized'`, rules are loaded, and the predicates in rule atoms match your data's actual IRIs exactly.

---

## 6. Shared Memory Errors on Startup

**Symptom**: PostgreSQL fails to start with `could not create shared memory segment` or pg_ripple logs `insufficient shared memory`.

**Cause**: `pg_ripple.dictionary_cache_size` is too large for the system's shared memory limits.

**Diagnostic**:

```bash
# Check system shared memory limits
sysctl kern.sysv.shmmax  # macOS
sysctl kernel.shmmax      # Linux
```

**Fix**: Either reduce `dictionary_cache_size` or increase the OS shared memory limit:

```bash
# Linux
sudo sysctl -w kernel.shmmax=17179869184  # 16GB
sudo sysctl -w kernel.shmall=4194304

# macOS
sudo sysctl -w kern.sysv.shmmax=17179869184
```

```admonish tip title="Docker environments"
In Docker, set `--shm-size=2g` (or larger) in your `docker run` command.
```

---

## 7. High Dictionary Cache Eviction Pressure

**Symptom**: `encode_cache_evictions` in `pg_ripple.stats()` is high; cache hit rate drops below 90%.

**Cause**: The working set of IRIs/literals exceeds the cache capacity.

**Diagnostic**:

```sql
SELECT
  s->>'encode_cache_capacity' AS capacity,
  s->>'encode_cache_utilization_pct' AS util_pct,
  s->>'encode_cache_evictions' AS evictions,
  ROUND(
    (s->>'encode_cache_hits')::numeric /
    NULLIF((s->>'encode_cache_hits')::numeric + (s->>'encode_cache_misses')::numeric, 0),
    4
  ) AS hit_rate
FROM pg_ripple.stats() s;
```

**Fix**: Increase `dictionary_cache_size` in `postgresql.conf` and restart:

```ini
pg_ripple.dictionary_cache_size = 131072  -- double the default
```

---

## 8. Federation Query Timeout

**Symptom**: Queries with `SERVICE` clauses hang or return a timeout error.

**Cause**: The remote SPARQL endpoint is unreachable, slow, or returning an unexpected format.

**Diagnostic**:

```bash
# Test the remote endpoint directly
curl -s -H "Accept: application/sparql-results+json" \
  "https://remote.example.org/sparql?query=SELECT+*+WHERE+{?s+?p+?o}+LIMIT+1"
```

**Fix**:

- Verify network connectivity to the remote endpoint
- Increase the federation timeout:

```sql
SET pg_ripple.federation_timeout = 60;  -- seconds
```

- Check that the remote endpoint supports the required result format (SPARQL JSON Results)

---

## 9. pg_ripple_http Not Responding

**Symptom**: The HTTP SPARQL endpoint returns connection refused or 502 errors.

**Cause**: The `pg_ripple_http` companion service is not running, or it cannot connect to PostgreSQL.

**Diagnostic**:

```bash
# Check if the process is running
ps aux | grep pg_ripple_http

# Check the service logs
journalctl -u pg_ripple_http --since "10 minutes ago"

# Test the PostgreSQL connection directly
psql -h localhost -p 5432 -U pg_ripple_http -d mydb -c "SELECT 1"
```

**Fix**:

- Start or restart the service
- Verify the connection string in the `pg_ripple_http` configuration
- Check that `pg_hba.conf` allows connections from the HTTP service

---

## 10. VP Table Bloat

**Symptom**: Disk usage grows faster than expected; `pg_size_pretty(pg_total_relation_size('_pg_ripple.vp_12345'))` is much larger than the triple count suggests.

**Cause**: Frequent deletes and re-inserts without merge cycles, or autovacuum not keeping up.

**Diagnostic**:

```sql
-- Check dead tuples
SELECT relname, n_dead_tup, n_live_tup,
       last_autovacuum, last_autoanalyze
FROM pg_stat_user_tables
WHERE schemaname = '_pg_ripple'
ORDER BY n_dead_tup DESC
LIMIT 10;
```

**Fix**:

```sql
-- Force a vacuum on the bloated table
VACUUM (VERBOSE) _pg_ripple.vp_12345_main;

-- Reclaim space aggressively
VACUUM (FULL) _pg_ripple.vp_12345_main;
```

Tune autovacuum for VP tables:

```sql
ALTER TABLE _pg_ripple.vp_12345_delta
  SET (autovacuum_vacuum_scale_factor = 0.01);
```

---

## 11. Bulk Load Slower Than Expected

**Symptom**: `pg_ripple.load_turtle()` or `pg_ripple.load_ntriples()` runs much slower than the documented 50K–200K triples/sec.

**Cause**: Small batch sizes, synchronous commit overhead, or insufficient `work_mem`.

**Diagnostic**:

```sql
SHOW synchronous_commit;
SHOW work_mem;
SHOW maintenance_work_mem;
```

**Fix**:

```sql
-- Disable synchronous commit for bulk loads
SET synchronous_commit = off;

-- Increase work memory
SET work_mem = '256MB';
SET maintenance_work_mem = '2GB';

-- Use the batch loading functions
SELECT pg_ripple.load_turtle_file('/path/to/data.ttl');
```

```admonish warning title="synchronous_commit = off"
Disabling synchronous commit risks losing the last few transactions on a crash. Only use this for bulk loads that can be re-run.
```

---

## 12. RDF-Star Parse Error

**Symptom**: Loading RDF-star data fails with `unexpected token` or `invalid quoted triple`.

**Cause**: The input file uses RDF-star syntax (`<<>>`) but the parser is not in RDF-star mode, or the syntax is malformed.

**Diagnostic**: Check the file around the reported line number for syntax issues. Common problems:

- Nested `<<>>` without proper whitespace
- Missing datatype on literal objects inside quoted triples
- Using Turtle-star syntax in N-Triples files (or vice versa)

**Fix**: Verify the file uses the correct format. For Turtle-star:

```turtle
<<:Alice :knows :Bob>> :since "2024"^^xsd:gYear .
```

For N-Triples-star, every term must be fully qualified — no prefixes.

---

## 13. SHACL Validation Queue Backlog

**Symptom**: `pg_ripple.validation_queue_depth()` returns a large number; validation results are delayed.

**Cause**: High write throughput is generating validations faster than the async validator can process them.

**Diagnostic**:

```sql
SELECT pg_ripple.validation_queue_depth();
SELECT pg_ripple.stats();
```

**Fix**:

- Increase the validation worker's processing capacity (if applicable)
- Temporarily switch to synchronous validation during low-traffic periods:

```sql
SET pg_ripple.enforce_constraints = 'error';
```

- Reduce write batch sizes to give the validator time to catch up

---

## 14. Plan Cache Thrashing

**Symptom**: SPARQL query latency is inconsistent. The first execution of a query pattern is slow, but subsequent runs are fast — then it becomes slow again.

**Cause**: The plan cache (`pg_ripple.plan_cache_size`) is too small for the number of distinct query patterns. Plans are evicted and recompiled repeatedly.

**Diagnostic**:

```sql
SHOW pg_ripple.plan_cache_size;

-- Estimate distinct query patterns in your workload
-- (application-level logging required)
```

**Fix**:

```sql
-- Increase the plan cache
SET pg_ripple.plan_cache_size = 1024;
```

If the number of distinct patterns exceeds any reasonable cache size, consider parameterizing queries to reduce pattern diversity.

---

## 15. "relation _pg_ripple.vp_XXXXX does not exist"

**Symptom**: SPARQL queries fail with a "relation does not exist" error for a specific VP table.

**Cause**: The predicates catalog references a VP table that was dropped or never created. This can happen after an incomplete migration or manual DDL.

**Diagnostic**:

```sql
-- Check the predicates catalog
SELECT id, table_oid, triple_count
FROM _pg_ripple.predicates
WHERE id = XXXXX;

-- Verify the table exists
SELECT oid FROM pg_class WHERE oid = (
  SELECT table_oid FROM _pg_ripple.predicates WHERE id = XXXXX
);
```

**Fix**:

```sql
-- Rebuild the VP table for the predicate
SELECT pg_ripple.reindex_predicate(XXXXX);
```

If the data is lost, the predicate entry should be removed:

```sql
DELETE FROM _pg_ripple.predicates WHERE id = XXXXX;
```

```admonish danger title="Manual catalog edits"
Directly modifying `_pg_ripple.predicates` bypasses integrity checks. Only do this as a last resort after confirming the VP table is genuinely missing.
```

---

## 16. "permission denied for schema _pg_ripple"

**Symptom**: Non-superuser connections get permission errors when running SPARQL queries.

**Cause**: The user does not have `USAGE` on `_pg_ripple` and `pg_ripple` schemas.

**Fix**:

```sql
GRANT USAGE ON SCHEMA pg_ripple TO myuser;
GRANT USAGE ON SCHEMA _pg_ripple TO myuser;
GRANT SELECT ON ALL TABLES IN SCHEMA _pg_ripple TO myuser;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA pg_ripple TO myuser;
```

---

## General Diagnostic Commands

A quick-reference set of commands for any troubleshooting session:

```sql
-- Extension health
SELECT pg_ripple.canary();
SELECT pg_ripple.stats();

-- PostgreSQL activity
SELECT pid, state, query, wait_event_type, wait_event
FROM pg_stat_activity
WHERE datname = current_database();

-- Lock contention
SELECT * FROM pg_locks WHERE NOT granted;

-- Table sizes in _pg_ripple
SELECT relname, pg_size_pretty(pg_total_relation_size(oid))
FROM pg_class
WHERE relnamespace = '_pg_ripple'::regnamespace
ORDER BY pg_total_relation_size(oid) DESC
LIMIT 20;

-- GUC settings
SELECT name, setting, source
FROM pg_settings
WHERE name LIKE 'pg_ripple.%'
ORDER BY name;
```
