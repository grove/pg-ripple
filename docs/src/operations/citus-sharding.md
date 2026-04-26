# Citus Horizontal Sharding

pg_ripple v0.58.0 introduces native support for [Citus](https://www.citusdata.com/)
horizontal sharding of VP tables.  When enabled, each VP delta table is
distributed across Citus worker nodes using the triple's **subject ID** (`s`)
as the distribution column, co-locating star-pattern triples on the same shard.

## Requirements

- **Citus 12+** installed in the same PostgreSQL 18 cluster.
- **pg_ripple ≥ 0.58.0**.
- **pg-trickle ≥ 0.33.0** (for CDC / pg-trickle compatibility mode).

> **PT536** — Raised when a Citus API function (`enable_citus_sharding`, `citus_rebalance`, `enable_citus_sharding`) is called but the Citus extension is not installed. Install Citus or use `pg_ripple.citus_available()` to check before calling.

## Activation

1. Install Citus on all nodes and add it to `shared_preload_libraries`:

   ```sql
   -- postgresql.conf
   shared_preload_libraries = 'citus,pg_ripple'
   ```

2. Enable sharding in pg_ripple:

   ```sql
   -- Session or postgresql.conf
   SET pg_ripple.citus_sharding_enabled = on;
   ```

3. Distribute all existing VP tables in one call:

   ```sql
   SELECT * FROM pg_ripple.enable_citus_sharding();
   ```

   This converts the dictionary and predicates catalog to **reference tables**
   (replicated to every worker) and calls `create_distributed_table('vp_{id}_delta', 's')`
   for every promoted predicate.

## GUC Reference

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.citus_sharding_enabled` | `off` | Distribute new VP tables on promotion |
| `pg_ripple.citus_trickle_compat` | `off` | Use `colocate_with='none'` for pg-trickle CDC |
| `pg_ripple.merge_fence_timeout_ms` | `0` | Advisory fence timeout during rebalancing (0 = disabled) |

## pg-trickle Compatibility

When using pg-trickle to stream VP table changes to a downstream system:

1. Set `pg_ripple.citus_trickle_compat = on` before distributing tables.  This
   sets `colocate_with='none'`, preventing cross-shard tombstone deletes during
   CDC apply.

2. Set `pg_ripple.merge_fence_timeout_ms = 30000` (30 seconds) to pause merge
   workers during Citus rebalancing.  The merge worker will skip cycles while
   the rebalancer holds the advisory fence lock.

3. Configure pg-trickle with `st_placement = 'distributed'` on the stream table
   so that CDC apply uses DELETE+INSERT instead of UPDATE (safe for distributed
   tables).

## REPLICA IDENTITY

pg_ripple sets `REPLICA IDENTITY FULL` on every VP delta table **before**
calling `create_distributed_table()`.  This ensures that the logical
replication slot used by pg-trickle captures full row images from the very
first write, which is required for correct tombstone propagation.

## Shard Rebalancing

```sql
-- Trigger a blocking shard rebalance.
SELECT pg_ripple.citus_rebalance();

-- Check cluster status.
SELECT * FROM pg_ripple.citus_cluster_status();
```

The merge worker acquires advisory lock `0x5052_5000` before executing a merge
cycle.  During rebalancing, the rebalancer holds the same lock, causing the
merge worker to log a message and skip the cycle rather than racing the
rebalancer.

## Merge Worker Notifications

The merge worker emits PostgreSQL `NOTIFY` messages on two channels:

| Channel | When | Payload |
|---------|------|---------|
| `pg_ripple.merge_start` | Before a merge cycle with fence enabled | `{"worker":N,"pid":PID}` |
| `pg_ripple.merge_end` | After a successful merge cycle with fence | `{"worker":N,"pid":PID}` |
| `pg_ripple.vp_promoted` | When a VP table is distributed | `{"table":"_pg_ripple.vp_N_delta","shard_count":N,"shard_table_prefix":"_pg_ripple.vp_N_delta_","predicate_id":N}` |

These notifications are **best-effort observability hints**, not correctness
mechanisms.  Because the TRUNCATE+INSERT merge is executed inside a single
2PC transaction, each per-worker WAL decoder receives the delta TRUNCATE and
main INSERT as one atomic committed batch — there is no intermediate
inconsistent state visible to downstream consumers even without the notification.

pg-trickle v0.33.0 uses `pgtrickle.pgt_st_locks` (catalog-based mutual
exclusion) for cross-node refresh scheduling and does **not** rely on
`pg_ripple.merge_start` / `merge_end` for coordination.  Operators and
monitoring tools may LISTEN to these channels for operational visibility.

## Coordination with pg-trickle `pgt_st_locks`

pg-trickle v0.33.0 uses a catalog-based lock table (`pgtrickle.pgt_st_locks`)
for cross-node refresh scheduling.  Each distributed stream table refresh
acquires a lease with a configurable expiry before touching worker slots.

**Important:** the `pgt_st_locks` lease expiry must be **≥**
`pg_ripple.merge_fence_timeout_ms`.  If the lease expires while a pg_ripple
merge is still in progress, pg-trickle may resume slot polling against a
partially-merged delta table.  The recommended configuration:

```sql
-- pg_ripple: fence timeout (how long merge_worker waits before giving up)
SET pg_ripple.merge_fence_timeout_ms = 30000;   -- 30 seconds

-- pg-trickle: stream table refresh lease expiry must be at least as long
-- (set via the pgt_st_locks entry created at refresh time)
SET pg_trickle.citus_st_lock_lease_ms = 45000;  -- 45 seconds (≥ 30s fence)
```

Monitor both sides together:

```sql
SELECT
    r.predicate_id,
    r.cycle_duration_ms,
    c.stream_table,
    c.worker_frontier
FROM pg_ripple.merge_status()    AS r
JOIN pgtrickle.citus_status       AS c
  ON c.source_stable_name LIKE '_pg_ripple_vp_' || r.predicate_id || '_%';
```

## VP Promotion Notifications

When `enable_citus_sharding()` distributes a VP delta table it emits a
`pg_ripple.vp_promoted` notification.  Consumers (e.g., pg-trickle tooling,
monitoring scripts) can LISTEN for this channel from a regular backend session:

```sql
LISTEN "pg_ripple.vp_promoted";

-- Payload example (after promote):
-- {"table":"_pg_ripple.vp_42_delta","shard_count":32,
--  "shard_table_prefix":"_pg_ripple.vp_42_delta_","predicate_id":42}
```

The `shard_count` and `shard_table_prefix` fields allow listeners to enumerate
all physical shard tables (`{prefix}{shard_id}`) without querying
`pg_dist_shard`.

## Limitations (v0.58.0)

- **pg-trickle scheduler integration** (v0.33.0): When using pg-trickle for CDC on
  distributed VP tables, the scheduler does not yet automatically poll per-worker
  WAL slots. The infrastructure is in place (`handle_vp_promoted`, `pgt_st_locks`),
  but operators must manually invoke `LISTEN "pg_ripple.vp_promoted" +
  handle_vp_promoted()` or implement custom application logic. Automated scheduler
  integration is planned for a future release.

- The `_pg_ripple.statement_id_timeline` table is coordinator-local; SID
  recording is done via the VP delta trigger which fires on the coordinator
  before shard routing.
- `pg_ripple.enable_citus_sharding()` must be called once after Citus is
  activated.  It is idempotent (safe to call multiple times).
- `vp_{id}_main` is not distributed; only the delta and tombstones tables
  are.  The merge worker reads from main and delta and writes back to main
  via the coordinator.
- Cross-shard SPARQL federation is not automatically optimised in v0.58.0.
  A future release will add shard-pruning for bound subject patterns.
