-- examples/citus_rebalance_with_trickle.sql
-- v0.60.0 K7-3: Runnable walkthrough of a Citus shard rebalance while
-- pg_trickle CDC subscriptions remain active.
--
-- Prerequisites:
--   1. PostgreSQL 18 with Citus extension installed
--   2. pg_ripple v0.59.0+ installed on all Citus nodes
--   3. pg_trickle installed (for the pause/resume CDC section)
--   4. At least two Citus worker nodes configured
--
-- pg_ripple integrates with Citus via the following mechanism:
--   - INSERT/UPDATE/DELETE on VP tables use Citus routing to reach the
--     correct shard based on the hash of the subject IRI (s column).
--   - The merge worker runs on the coordinator and handles all shards.
--   - Shard rebalancing emits a NOTIFY on 'pg_ripple_citus_rebalance'
--     so pg_trickle can pause CDC delivery during the rebalance window.
--
-- This example shows the recommended procedure for zero-downtime rebalancing.

-- ─── Step 1: Check current shard distribution ────────────────────────────────

-- Before rebalancing, inspect the current shard placement.
-- Run this on the Citus coordinator.
SELECT
    logicalrelid AS table_name,
    shardid,
    nodename,
    nodeport,
    shardlength
FROM citus_shards
WHERE logicalrelid::text LIKE '_pg_ripple.vp_%'
ORDER BY logicalrelid, shardid;

-- Check current VP table triple distribution across shards.
SELECT
    p.id AS pred_id,
    d.iri AS predicate_iri,
    p.triple_count
FROM _pg_ripple.predicates p
JOIN _pg_ripple.dictionary d ON d.id = p.id
ORDER BY p.triple_count DESC
LIMIT 20;

-- ─── Step 2: Pause pg_trickle CDC (if installed) ─────────────────────────────
-- If you have pg_trickle active, pause CDC delivery before rebalancing to
-- prevent delivery failures during shard moves.
--
-- NOTE: This step is only needed if pg_trickle is installed and active.
-- pg_ripple's internal merge worker handles rebalance pausing automatically
-- via the NOTIFY 'pg_ripple_citus_rebalance' signal.

-- SELECT pg_trickle.pause_all_subscriptions();

-- ─── Step 3: Enable pg_ripple maintenance mode on coordinator ────────────────
-- This prevents the merge worker from starting a new cycle during rebalancing.
SET pg_ripple.maintenance_mode = 'on';

-- ─── Step 4: Run the Citus rebalance ─────────────────────────────────────────
-- Rebalance VP table shards across all available worker nodes.
-- The drain_only mode first moves shards away from nodes being decommissioned.
-- The default strategy ('by_disk_size') distributes based on data volume.

-- Rebalance all pg_ripple VP tables.
SELECT citus_rebalance_start(
    relation_name_filter := '_pg_ripple.vp_',
    threshold            := 0.1,         -- allow ±10% imbalance
    max_shard_moves      := 20,           -- process up to 20 shards per run
    excluded_shard_list  := ARRAY[]::bigint[],
    drain_only           := false,
    rebalance_strategy   := 'by_disk_size'
);

-- Monitor progress (poll until complete).
-- In production, run this in a loop with pg_sleep(5) between iterations.
SELECT
    rebalance_progress,
    total_shard_moves,
    completed_shard_moves,
    remaining_shard_moves
FROM citus_rebalance_status();

-- Wait for completion synchronously (blocks until done).
SELECT citus_rebalance_wait();

-- ─── Step 5: Verify rebalance result ─────────────────────────────────────────
-- Confirm shards are now evenly distributed.
SELECT
    nodename,
    count(*)          AS shard_count,
    sum(shardlength)  AS total_bytes
FROM citus_shards
WHERE logicalrelid::text LIKE '_pg_ripple.vp_%'
GROUP BY nodename
ORDER BY nodename;

-- ─── Step 6: Resume pg_ripple and pg_trickle ─────────────────────────────────
-- Re-enable the merge worker.
SET pg_ripple.maintenance_mode = 'off';

-- Trigger an immediate merge cycle to catch up on any delta inserts that
-- accumulated during the rebalance window.
SELECT pg_ripple.merge_all();

-- Resume pg_trickle CDC delivery (if paused in Step 2).
-- SELECT pg_trickle.resume_all_subscriptions();

-- ─── Step 7: Verify SPARQL query still works ─────────────────────────────────
-- Run a test query to confirm the VP views are still accessible.
SELECT pg_ripple.sparql($$
    SELECT (COUNT(*) AS ?count)
    WHERE { ?s ?p ?o }
$$);

-- ─── Notes ───────────────────────────────────────────────────────────────────
-- • pg_ripple's shard pruning is based on the XXH3-128 hash of the subject IRI
--   (stored in the s column). After rebalancing, the same query will route to
--   the correct shard automatically.
-- • The VP view definition does not change during rebalance — only the
--   underlying shard placements change.
-- • For large deployments (>10 nodes), consider using drain_only := true first
--   to decommission specific nodes before adding new ones.
-- • The pg_ripple_http companion service does NOT need to be restarted after
--   a Citus rebalance — connection pooling is managed by the coordinator.
