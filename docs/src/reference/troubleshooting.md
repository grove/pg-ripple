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

