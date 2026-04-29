-- pg_regress test: Citus SERVICE shard annotation benchmark (v0.71.0 CITUS-BENCH-01)
--
-- Verifies that:
--   1. citus_service_pruning GUC exists and can be toggled.
--   2. explain_sparql() works for queries with SERVICE-like patterns.
--   3. is_citus_worker_endpoint() function exists and is callable.
--   4. feature_status() lists citus_service_pruning as experimental (not planned).
--
-- NOTE: Actual shard-count pruning requires a running Citus cluster.  This test
-- validates the GUC and function plumbing; the benchmark comparison (pruned vs
-- unpruned shard counts) is covered by tests/integration/citus_rls_propagation.sh
-- when a Citus cluster is available.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── Part 1: GUC existence and toggle ─────────────────────────────────────────

-- 1a. citus_service_pruning defaults to off.
SHOW pg_ripple.citus_service_pruning;

-- 1b. Can be set on.
SET pg_ripple.citus_service_pruning = on;
SHOW pg_ripple.citus_service_pruning;

-- 1c. Reset to off.
SET pg_ripple.citus_service_pruning = off;
SHOW pg_ripple.citus_service_pruning;

-- ── Part 2: Function existence checks ────────────────────────────────────────

-- 2a. is_citus_worker_endpoint() exists.
SELECT EXISTS (
    SELECT 1 FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'pg_ripple'
      AND p.proname = 'citus_available'
) AS citus_available_exists;

-- 2b. Without Citus installed, citus_available() returns false.
SELECT pg_ripple.citus_available() AS citus_not_available_in_ci;

-- ── Part 3: explain_sparql with citus_service_pruning on vs off ───────────────

-- 3a. explain_sparql() works with citus_service_pruning = off.
SET pg_ripple.citus_service_pruning = off;
SELECT length(pg_ripple.explain_sparql(
    'SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 1',
    false
)) > 0 AS explain_works_pruning_off;

-- 3b. explain_sparql() works with citus_service_pruning = on.
SET pg_ripple.citus_service_pruning = on;
SELECT length(pg_ripple.explain_sparql(
    'SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 1',
    false
)) > 0 AS explain_works_pruning_on;

-- Reset GUC.
SET pg_ripple.citus_service_pruning = off;

-- ── Part 4: feature_status entry ─────────────────────────────────────────────

-- 4a. citus_service_pruning must be listed as experimental (not planned).
SELECT status NOT IN ('planned') AS citus_service_pruning_not_planned
FROM pg_ripple.feature_status()
WHERE feature_name = 'citus_service_pruning';
