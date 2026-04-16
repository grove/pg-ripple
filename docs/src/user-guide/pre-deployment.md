# Pre-Deployment Checklist

This page covers the steps required before running pg_ripple in a production PostgreSQL 18 
environment.

## 1. Add pg_ripple to shared_preload_libraries

The HTAP merge worker and shared-memory counters require the extension to be loaded at 
PostgreSQL startup:

```ini
# postgresql.conf
shared_preload_libraries = 'pg_ripple'
```

> **Restart required.** `shared_preload_libraries` is read once at startup. After editing 
> `postgresql.conf`, restart PostgreSQL:
>
> ```bash
> pg_ctl restart -D $PGDATA
> # or via systemd:
> systemctl restart postgresql
> ```

Without this setting:
- The background merge worker does not start
- `stats()` returns `"unmerged_delta_rows": -1`
- The `ExecutorEnd` hook is not registered (no automatic latch pokes)

## 2. Create the Extension

```sql
-- Connect to the target database
\c mydb

-- Required because the pg_ripple schema begins with "pg_"
SET allow_system_table_mods = on;
CREATE EXTENSION pg_ripple;
```

Or use the superuser-level command:

```bash
psql -U postgres -d mydb \
  -c "SET allow_system_table_mods = on; CREATE EXTENSION pg_ripple;"
```

## 3. Configure the Worker Database

Set `pg_ripple.worker_database` to the name of the database where the extension is installed:

```ini
# postgresql.conf
pg_ripple.worker_database = 'mydb'
```

Reload the configuration (no restart needed for this parameter):

```sql
SELECT pg_reload_conf();
```

## 4. Verify the Merge Worker is Running

After restart and extension creation, confirm the worker is alive:

```sql
SELECT pg_ripple.stats();
-- "merge_worker_pid" should be a non-zero PID

-- Also confirm via pg_stat_activity
SELECT pid, application_name, state
FROM pg_stat_activity
WHERE application_name = 'pg_ripple merge worker';
```

## 5. Size Shared Memory

pg_ripple's shared-memory footprint is small (a few PgAtomic slots). The only 
`shared_memory`-related GUC is `pg_ripple.dictionary_cache_size`:

```ini
# postgresql.conf
# Increase if the dictionary lookup rate (from pg_stat_statements) is high
pg_ripple.dictionary_cache_size = 131072   # 128K entries (default: 64K)
```

Estimate memory requirement: each dictionary cache slot holds approximately 200 bytes, 
so 128K entries ≈ 25 MB per backend. Adjust `shared_buffers` and `work_mem` as usual 
for your PostgreSQL instance size.

## 6. Set HTAP Thresholds for Your Workload

Review the [Configuration](configuration.md#htap-parameters-v060) page and tune:

```ini
# postgresql.conf
# For write-heavy ETL workloads:
pg_ripple.merge_threshold        = 100000
pg_ripple.latch_trigger_threshold = 50000
pg_ripple.merge_interval_secs    = 30
pg_ripple.merge_retention_seconds = 30

# For real-time write workloads:
pg_ripple.merge_threshold        = 5000
pg_ripple.latch_trigger_threshold = 1000
pg_ripple.merge_interval_secs    = 5
```

## 8. SHACL Mode Selection (v0.7.0)

Choose the right SHACL enforcement strategy before going live:

| Mode | When to use | Latency impact |
|---|---|---|
| `off` (default) | Bulk ingestion pipelines, ETL, performance-critical paths | None |
| `sync` | Interactive inserts where violations must be rejected immediately | Low-to-medium (per shape) |
| `async` | (v0.8.0) High-throughput writes with post-hoc violation reporting | Minimal |

```ini
# postgresql.conf — only set shacl_mode if you want enforcement
# pg_ripple.shacl_mode = 'sync'   # uncomment to enable inline rejection
```

**Best practice**: Load SHACL shapes **before** bulk importing data:

```sql
-- 1. Load shapes
SELECT pg_ripple.load_shacl(pg_read_file('/etc/shapes/my-shapes.ttl'));

-- 2. (Optional) Set mode for ongoing inserts
SET pg_ripple.shacl_mode = 'sync';

-- 3. Load data
SELECT pg_ripple.load_turtle('...', '<https://example.org/my-graph>');

-- 4. Validate after bulk load (always useful regardless of mode)
SELECT pg_ripple.validate();
```

## 9. Ensure allow_system_table_mods in pg_regress / CI

pg_ripple's bootstrap SQL includes `SET LOCAL allow_system_table_mods = on;`. In CI:

```bash
# When running cargo pgrx regress, always pass:
cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"
```

## 10. Production Checklist Summary

| Item | Command / File |
|---|---|
| `shared_preload_libraries` set | `postgresql.conf` |
| PostgreSQL restarted | `pg_ctl restart` |
| Extension created | `CREATE EXTENSION pg_ripple` |
| Worker database set | `pg_ripple.worker_database` |
| Worker PID non-zero | `SELECT pg_ripple.stats()` |
| HTAP thresholds tuned | `postgresql.conf` |
| `ANALYZE` run after initial load | `ANALYZE _pg_ripple.vp_rare` |
| SHACL mode confirmed | `SHOW pg_ripple.shacl_mode` |
| Shapes loaded before bulk import | `SELECT pg_ripple.list_shapes()` |
| Monitoring configured | See [Administration](sql-reference/admin.md) |
| `pg_stat_statements` enabled | See §11 below |
| `work_mem` tuned for aggregates | See §12 below |

## 11. Enable pg_stat_statements (v0.13.0+)

pg_stat_statements records execution statistics for all SQL statements, including the generated SQL that pg_ripple emits for each SPARQL query. This lets you identify slow SPARQL queries by inspecting the underlying SQL.

```ini
# postgresql.conf
shared_preload_libraries = 'pg_ripple,pg_stat_statements'
pg_stat_statements.track = 'all'
pg_stat_statements.max = 10000
```

After enabling:
```sql
-- Which SPARQL-generated SQL queries are slowest?
SELECT
  left(query, 80)   AS query_prefix,
  calls,
  round(mean_exec_time::numeric, 2) AS mean_ms,
  round(total_exec_time::numeric, 2) AS total_ms
FROM pg_stat_statements
WHERE query LIKE '%_pg_ripple%'
ORDER BY total_exec_time DESC
LIMIT 20;
```

## 12. Tune work_mem for SPARQL Aggregates (v0.13.0+)

SPARQL aggregates (`COUNT`, `SUM`, `GROUP BY`) generate SQL with `GROUP BY` and hash aggregation. Large aggregate queries benefit from higher `work_mem`:

```ini
# postgresql.conf (global default)
work_mem = '64MB'          -- default is 4MB; too low for large SPARQL aggregates

# Or set per-session before running analytical SPARQL queries:
```

```sql
SET work_mem = '256MB';
SELECT * FROM pg_ripple.sparql($$
  SELECT ?type (COUNT(?s) AS ?count) WHERE {
    ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ?type .
  }
  GROUP BY ?type
  ORDER BY DESC(?count)
$$);
```

**Rule of thumb for `work_mem`:**
- Simple BGP queries: 16–32 MB is sufficient
- Aggregates over 1M+ triples: 128–512 MB
- Heavily concurrent deployments: keep the global default low (4–16 MB) and use per-session settings for analytical queries
