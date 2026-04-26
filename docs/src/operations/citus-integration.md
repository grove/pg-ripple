# Citus + pg_ripple + pg-trickle: End-to-End Integration Guide

> **Version**: v0.59.0 (CITUS-15)
>
> This guide covers deploying pg_ripple with Citus horizontal sharding and
> pg-trickle CDC replication in a multi-worker environment.  It assumes you
> have already read the [Citus Sharding](citus-sharding.md) page.

---

## Overview

The v0.58.0 + v0.59.0 releases complete the Citus sharding story:

| Feature | Version | Description |
|---------|---------|-------------|
| VP table distribution | v0.58.0 | `enable_citus_sharding()` distributes VP delta tables |
| Merge fence advisory lock | v0.58.0 | Prevents split-brain during rebalancing |
| SPARQL shard-pruning | v0.59.0 | Bound-subject queries target one shard (10–100×) |
| Rebalance NOTIFY | v0.59.0 | `merge_start`/`merge_end` signals for pg-trickle |
| `explain_sparql` Citus section | v0.59.0 | Verify pruning with `EXPLAIN` |
| `citus_rebalance_progress()` | v0.59.0 | Observe live rebalance status |

---

## Prerequisites

1. **Citus 12+** installed on coordinator and all workers.
2. **pg_ripple 0.59.0** installed on the coordinator.
3. **pg-trickle 0.34.0+** (optional, for CDC streaming) with `handle_vp_promoted` 
   wiring configured.

---

## Step 1: Install pg_ripple and Citus on the coordinator

```sql
-- On the coordinator node:
CREATE EXTENSION citus;
CREATE EXTENSION pg_ripple;

-- Verify Citus is detected:
SELECT pg_ripple.citus_available();  -- returns true
```

## Step 2: Configure sharding GUCs

```sql
ALTER SYSTEM SET pg_ripple.citus_sharding_enabled = on;
-- Enable pg-trickle co-location compatibility (prevents cross-shard deletes):
ALTER SYSTEM SET pg_ripple.citus_trickle_compat = on;
SELECT pg_reload_conf();
```

## Step 3: Distribute VP tables

After loading your initial data, distribute all VP tables:

```sql
SELECT predicate_id, table_name, status
FROM pg_ripple.enable_citus_sharding();
```

This performs for each VP delta table:
1. `ALTER TABLE … REPLICA IDENTITY FULL` (required for pg-trickle logical replication)
2. `create_distributed_table(…, 's', colocate_with => 'none')` (or `'default'`)
3. `pg_notify('pg_ripple.vp_promoted', …)` — pg-trickle creates per-shard slots

## Step 4: Verify shard-pruning (v0.59.0)

After loading some data with a known subject IRI, verify shard-pruning works:

```sql
SELECT pg_ripple.explain_sparql(
    'SELECT ?p ?o WHERE { <http://example.org/Alice> ?p ?o }',
    false,      -- analyze
    true        -- citus (new in v0.59.0)
) -> 'citus';
```

Expected output when pruning succeeds:

```json
{
  "available": true,
  "pruned_to_shard": 102008,
  "worker": "worker1:5432",
  "full_fanout_avoided": true,
  "estimated_rows_per_shard": 47
}
```

When `full_fanout_avoided` is `false`, check that:
- The subject IRI exists in the dictionary: `SELECT pg_ripple.encode_term('<http://example.org/Alice>', 0)`.
- The VP delta table is distributed: `SELECT logicalrelid FROM pg_dist_partition WHERE logicalrelid::text LIKE '%delta%'`.

## Step 5: Rebalancing workers

When adding Citus workers, use the pg_ripple-aware rebalancer:

```sql
-- Monitor progress (returns empty when no rebalance is running):
SELECT * FROM pg_ripple.citus_rebalance_progress();

-- Trigger a rebalance:
SELECT pg_ripple.citus_rebalance();
```

`citus_rebalance()` emits `pg_ripple.merge_start` before acquiring the fence
lock and `pg_ripple.merge_end` after releasing it.  pg-trickle listens on these
channels and suspends per-worker slot polling until the rebalance completes,
preventing duplicate CDC delivery.

Listen for the signals in a monitoring session:

```sql
LISTEN "pg_ripple.merge_start";
LISTEN "pg_ripple.merge_end";
-- (run pg_ripple.citus_rebalance() in another session, then check notifications)
```

---

## GUC Reference

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.citus_sharding_enabled` | `off` | Enable Citus shard distribution for VP tables |
| `pg_ripple.citus_trickle_compat` | `off` | Use `colocate_with => 'none'` for pg-trickle CDC |
| `pg_ripple.merge_fence_timeout_ms` | `0` | Max ms to wait for merge fence (0 = block indefinitely) |

---

## Troubleshooting

### `PT536`: Citus extension is not installed

`pg_ripple.enable_citus_sharding()` or `citus_rebalance()` raised PT536.

**Fix**: `CREATE EXTENSION citus;` on the coordinator before calling these functions.

### `full_fanout_avoided: false` in explain output

The subject IRI is not in the dictionary (no triples loaded for that subject yet),
or the VP table is not distributed.

**Fix**: Load at least one triple for the subject, then re-run `enable_citus_sharding()`.

### pg-trickle stops after rebalance

Check that your pg-trickle version is ≥ 0.34.0, which processes `pg_ripple.merge_start`
/ `merge_end` notifications and resumes slot polling automatically.

---

*See also: [Citus Sharding](citus-sharding.md), [High Availability](high-availability.md)*
