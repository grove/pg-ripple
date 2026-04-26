# Citus Horizontal Sharding

pg_ripple v0.58.0 introduces native support for [Citus](https://www.citusdata.com/)
horizontal sharding of VP tables.  When enabled, each VP delta table is
distributed across Citus worker nodes using the triple's **subject ID** (`s`)
as the distribution column, co-locating star-pattern triples on the same shard.

## Requirements

- **Citus 12+** installed in the same PostgreSQL 18 cluster.
- **pg_ripple ≥ 0.58.0**.
- **pg-trickle ≥ 0.32.0** (for CDC / pg-trickle compatibility mode).

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
| `pg_ripple.vp_promoted` | When a VP table is distributed | `{"predicate_id":N,"pid":PID}` |

pg-trickle subscribes to `pg_ripple.merge_end` to resume CDC apply after a
merge cycle completes.

## Limitations (v0.58.0)

- The `_pg_ripple.statement_id_timeline` table is coordinator-local; SID
  recording is done via the VP delta trigger which fires on the coordinator
  before shard routing.
- `pg_ripple.enable_citus_sharding()` must be called once after Citus is
  activated.  It is idempotent (safe to call multiple times).
- `vp_{id}_main` and `vp_{id}_tombstones` are not distributed; only the delta
  table is.  The merge worker reads from main and delta and writes back to main
  via the coordinator.
- Cross-shard SPARQL federation is not automatically optimised in v0.58.0.
  A future release will add shard-pruning for bound subject patterns.
