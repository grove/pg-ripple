# Production Readiness Checklist

Use this checklist before deploying pg_ripple to production. Each item links to the relevant documentation for details.

---

## PostgreSQL Configuration

- [ ] **PostgreSQL 18** installed — pg_ripple requires PostgreSQL 18.x
- [ ] **`shared_preload_libraries`** includes `'pg_ripple'` — required for the background merge worker and shared-memory dictionary cache ([Configuration](configuration.md))
- [ ] **`pg_ripple.worker_database`** set to your target database — the merge worker connects to this database ([Merge Workers](merge-workers.md))
- [ ] **Shared memory** sized correctly — `pg_ripple.dictionary_cache_size` determines shared memory usage; check OS limits ([Troubleshooting §6](troubleshooting.md))
- [ ] **PostgreSQL restarted** after `shared_preload_libraries` changes

## Security

- [ ] **Row-Level Security (RLS)** enabled on named graphs if multi-tenant — `pg_ripple.enable_graph_rls()` + role grants ([Security](security.md), [Multi-Tenant Graphs](../features/multi-tenant-graphs.md))
- [ ] **Federation SSRF protection** configured — `pg_ripple.federation_allow_private = off` (default) prevents SERVICE queries to private IPs ([GUC Reference](../reference/guc-reference.md))
- [ ] **`pg_ripple_http` auth token** set — `PG_RIPPLE_HTTP_AUTH_TOKEN` environment variable for Bearer token authentication ([HTTP API Reference](../reference/http-api.md))
- [ ] **TLS termination** configured — use a reverse proxy (nginx, Caddy) for HTTPS; pg_ripple_http does not handle TLS directly
- [ ] **`pg_hba.conf`** restricts connections to the pg_ripple_http service account
- [ ] **Embedding API keys** not stored in `postgresql.conf` — use `ALTER SYSTEM` or inject via session `SET` ([GUC Reference](../reference/guc-reference.md))

## Performance

- [ ] **Merge workers** tuned — `pg_ripple.merge_workers` = 2–4 for workloads with many predicates ([Merge Workers](merge-workers.md))
- [ ] **Dictionary cache** sized to working set — monitor `encode_cache_evictions` via `pg_ripple.stats()`, target > 90% hit rate ([Troubleshooting §7](troubleshooting.md))
- [ ] **Autovacuum** tuned for VP tables — consider `autovacuum_vacuum_scale_factor = 0.01` on high-churn delta tables ([Performance](performance.md))
- [ ] **`work_mem`** adequate for SPARQL-generated SQL — 64–256 MB for large queries
- [ ] **Property path depth** bounded — `pg_ripple.max_path_depth` prevents runaway recursion (default: 10) ([Troubleshooting §3](troubleshooting.md))

## Monitoring

- [ ] **Prometheus metrics** configured — `pg_ripple_http` exposes `/metrics` endpoint ([Monitoring](monitoring.md))
- [ ] **Key metrics monitored**:
  - `pg_ripple_triple_count` — total stored triples
  - `pg_ripple_merge_worker_lag` — merge backlog
  - `pg_ripple_dictionary_cache_hit_rate` — encoding efficiency
  - `pg_ripple_sparql_query_duration_seconds` — query latency
- [ ] **Health check** configured — `GET /health` and `GET /health/ready` for load balancer probes
- [ ] **Log-based alerting** on PT-series error codes — see [Error Catalog](../reference/error-catalog.md)

## Backup and Recovery

- [ ] **`pg_dump`** tested — pg_ripple stores all data in standard PostgreSQL tables; `pg_dump`/`pg_restore` works without special steps ([Backup](backup-recovery.md))
- [ ] **WAL archiving** enabled for point-in-time recovery
- [ ] **Backup schedule** documented and tested for restore

## v0.133.0 resilience qualification runbook

Run the qualification commands from the repository root. The required matrix
uses a disposable PostgreSQL 18 instance and reuses the established
crash-recovery tests. It is deterministic, bounded, and stops on the first
failure.

```bash
# CI-safe: no PostgreSQL connection, cluster, or external mutation required.
bash tests/resilience/fault_matrix.sh --validate
find tests -type f -name '*.sh' -print0 | xargs -0 -n1 bash -n
python3 scripts/check_docs_links.py
python3 scripts/check_docs_summary.py
```

```bash
# Required crash, restart, and logical-restore matrix. Use only a disposable
# cluster because the legacy crash tests intentionally drop their test DBs.
export PGDATA=/path/to/disposable/pgdata
export PGHOST=/path/to/disposable/socket
export PGPORT=28818
export PGUSER=postgres
export PGDATABASE=postgres
export PG_RIPPLE_RESILIENCE_ALLOW_DESTRUCTIVE=1
export PG_RIPPLE_RESILIENCE_RUN_ID=v0133
bash tests/resilience/fault_matrix.sh --all
```

The driver runs these cases in this fixed order:

| Scenario | Command | Recovery assertion |
|---|---|---|
| `load-sigkill-restart` | `bash tests/resilience/fault_matrix.sh --scenario load-sigkill-restart` | Dictionary and post-crash load remain usable |
| `merge-sigkill-restart` | `bash tests/resilience/fault_matrix.sh --scenario merge-sigkill-restart` | Main/delta/tombstone state is queryable after restart |
| `promotion-sigkill-restart` | `bash tests/resilience/fault_matrix.sh --scenario promotion-sigkill-restart` | Promotion recovery leaves no stuck catalog state |
| `writeback-sigkill-restart` | `bash tests/resilience/fault_matrix.sh --scenario writeback-sigkill-restart` | A writeback view can be materialized again |
| `inference-sigkill-restart` | `bash tests/resilience/fault_matrix.sh --scenario inference-sigkill-restart` | Inference can be rerun without a corrupt derivation state |
| `upgrade-sigkill-restart` | `bash tests/resilience/fault_matrix.sh --scenario upgrade-sigkill-restart` | Failed upgrade rolls back; SIGKILL/restart preserves data |
| `logical-dump-restore` | `bash tests/resilience/fault_matrix.sh --scenario logical-dump-restore` | `pg_dump`/`pg_restore` preserves triples and health |

If the installed candidate was built with the test-only fault-injection
feature, set `PG_RIPPLE_FAULT_INJECTION` before starting PostgreSQL to select a
deterministic hook, for example
`merge_phase_start=terminate`; a termination hook must be run only on the
disposable instance.

### Physical backup and PITR

The physical script is opt-in because it starts restored PostgreSQL clusters
and uses the configured primary as a backup source. It fails on backup,
restore, archive, or health errors; it skips only when explicitly not enabled.

```bash
export PG_RIPPLE_RUN_PHYSICAL=1
bash tests/resilience/fault_matrix.sh --scenario physical-backup-restore-pitr

# Add these only when the primary has archive_mode=on and archive_command
# writes to the supplied directory.
export PG_RIPPLE_RUN_PITR=1
export PG_RIPPLE_WAL_ARCHIVE_DIR=/path/to/wal-archive
bash tests/resilience/fault_matrix.sh --scenario physical-backup-restore-pitr
```

### Primary/standby promotion

Promotion is a topology mutation and requires fencing the old primary first.
The harness requires local data-directory paths so it can perform that fence;
it does not attempt to rejoin or overwrite either cluster after promotion.

```bash
export PG_RIPPLE_RUN_HA=1
export PG_RIPPLE_RESILIENCE_ALLOW_TOPOLOGY_CHANGE=1
export PG_RIPPLE_PRIMARY_URL='postgresql://postgres@primary/postgres'
export PG_RIPPLE_STANDBY_URL='postgresql://postgres@standby/postgres'
export PG_RIPPLE_PRIMARY_DATA=/path/to/primary/pgdata
export PG_RIPPLE_STANDBY_DATA=/path/to/standby/pgdata
bash tests/resilience/fault_matrix.sh --scenario primary-standby-promotion
```

### Read-replica write safety

This check is read-only against the replica except for an intentionally
rejected `insert_triple` call. A connection failure is a test failure, not a
read-only pass.

```bash
export PG_RIPPLE_RUN_HA=1
export PG_RIPPLE_READ_REPLICA_URL='postgresql://postgres@replica/postgres'
bash tests/resilience/fault_matrix.sh --scenario read-replica-write-safety
```

### Resource pressure

The pressure check changes session-local `statement_timeout`, `work_mem`, and
`temp_file_limit`, then verifies a bounded property path and a cancellation
diagnostic. Row count and timeout are configurable but capped by the script.

```bash
export PG_RIPPLE_RUN_RESOURCE_PRESSURE=1
export PG_RIPPLE_RESOURCE_ROWS=2000
export PG_RIPPLE_RESOURCE_TIMEOUT_MS=1500
export PG_RIPPLE_RESOURCE_WORK_MEM=1MB
bash tests/resilience/fault_matrix.sh --scenario resource-pressure
```

Required scenarios must exit zero. Physical/PITR and HA commands are
environment-gated: `SKIP` is valid only when their opt-in variable is absent;
once enabled, missing prerequisites or failed assertions are failures.

## Upgrade Path

- [ ] **Migration scripts** verified — `ALTER EXTENSION pg_ripple UPDATE` applies sequential migration scripts ([Upgrading](upgrading.md))
- [ ] **Compatibility matrix** checked — if using `pg_ripple_http`, verify version compatibility ([Compatibility](compatibility.md))
- [ ] **Test in staging** before production upgrade — run `cargo pgrx regress` or the pg_regress suite against the new version

## Optional Components

- [ ] **pgvector** installed if using vector/hybrid search — `CREATE EXTENSION vector` ([Vector Search](../features/vector-and-hybrid-search.md))
- [ ] **pg_trickle** installed if using live views; **pg_tide** installed if using the CDC bridge or relay outboxes — ([CDC Operations](cdc.md))
- [ ] **PostGIS** installed if using GeoSPARQL — ([GeoSPARQL](../features/geospatial.md))

## Smoke Test

After deployment, verify the extension is working:

```sql
-- Check extension version
SELECT pg_ripple.build_info();

-- Verify merge worker is running
SELECT (pg_ripple.stats()->>'merge_worker_pid')::int > 0 AS merge_worker_alive;

-- Verify dictionary cache is active
SELECT pg_ripple.stats()->>'encode_cache_capacity' AS cache_capacity;

-- Load a test triple and query it
SELECT pg_ripple.insert_triple(
    'http://example.org/test',
    'http://example.org/status',
    '"production-ready"'
);
SELECT * FROM pg_ripple.sparql($$
    SELECT ?status WHERE {
        <http://example.org/test> <http://example.org/status> ?status
    }
$$);
```
