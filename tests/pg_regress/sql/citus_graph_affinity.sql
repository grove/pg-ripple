-- pg_regress test: Citus graph shard affinity (v0.61.0 CITUS-21)
-- Tests set_graph_shard_affinity() and clear_graph_shard_affinity().

SET search_path TO pg_ripple, public;

-- Ensure the graph_shard_affinity table exists (created by migration / _PG_INIT).
SELECT EXISTS (
    SELECT 1 FROM pg_catalog.pg_class c
    JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname = '_pg_ripple' AND c.relname = 'graph_shard_affinity'
) AS affinity_table_exists;

-- set_graph_shard_affinity should run without error.
SELECT pg_ripple.set_graph_shard_affinity(
    'https://example.org/graph1',
    1
) AS affinity_set;

-- Verify the row was inserted.
SELECT count(*) = 1 AS affinity_recorded
FROM _pg_ripple.graph_shard_affinity
WHERE shard_id = 1;

-- clear_graph_shard_affinity should remove the row.
SELECT pg_ripple.clear_graph_shard_affinity(
    'https://example.org/graph1'
) AS affinity_cleared;

-- Verify the row is gone.
SELECT count(*) = 0 AS affinity_removed
FROM _pg_ripple.graph_shard_affinity
WHERE shard_id = 1;
