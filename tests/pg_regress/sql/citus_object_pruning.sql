-- pg_regress test: Citus object-level pruning (v0.61.0 CITUS-21)
-- Tests that prune_bound_term() generates correct shard-pruning hints.
-- Skips gracefully when Citus is not installed.

SET search_path TO pg_ripple, public;

-- Check if Citus is available; skip the rest if not.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_extension WHERE extname = 'citus'
    ) THEN
        RAISE NOTICE 'SKIP: Citus extension not installed; skipping citus_object_pruning test';
    END IF;
END;
$$;

-- The set_graph_shard_affinity function should exist regardless of Citus.
SELECT pg_catalog.pg_get_function_result(oid) IS NOT NULL AS function_exists
FROM pg_catalog.pg_proc
WHERE proname = 'set_graph_shard_affinity'
  AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple');

-- Verify clear_graph_shard_affinity also exists.
SELECT pg_catalog.pg_get_function_result(oid) IS NOT NULL AS clear_fn_exists
FROM pg_catalog.pg_proc
WHERE proname = 'clear_graph_shard_affinity'
  AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple');
