-- pg_regress test: Citus horizontal sharding API (v0.58.0 + v0.59.0)
-- NOTE: These tests verify the Citus API functions exist and behave correctly
-- when Citus is NOT installed (the expected case in CI).  Functions must be
-- callable but return appropriate messages or empty sets, not crash.

-- Verify Citus API functions exist.
SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'citus_available'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS citus_available_fn_exists;

SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'enable_citus_sharding'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS enable_citus_sharding_fn_exists;

SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'citus_cluster_status'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS citus_cluster_status_fn_exists;

SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'citus_rebalance'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS citus_rebalance_fn_exists;

-- v0.59.0: Verify new API functions exist.
SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'citus_rebalance_progress'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS citus_rebalance_progress_fn_exists;

-- Verify explain_sparql 3-arg overload (text, bool, bool) exists.
SELECT EXISTS (
    SELECT 1 FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'pg_ripple'
      AND p.proname = 'explain_sparql'
      AND p.pronargs = 3
) AS explain_sparql_3arg_exists;

-- citus_available() should return false when Citus is not installed.
SELECT pg_ripple.citus_available() AS citus_available_result;

-- citus_cluster_status() should return empty set (not crash) when Citus absent.
SELECT count(*) = 0 AS empty_status_without_citus
FROM pg_ripple.citus_cluster_status();

-- v0.59.0: citus_rebalance_progress() must return empty set when Citus absent.
SELECT count(*) = 0 AS empty_progress_without_citus
FROM pg_ripple.citus_rebalance_progress();

-- Verify GUCs exist and have correct defaults.
SELECT current_setting('pg_ripple.citus_sharding_enabled') = 'off' AS sharding_guc_off;
SELECT current_setting('pg_ripple.citus_trickle_compat') = 'off' AS trickle_compat_guc_off;
SELECT current_setting('pg_ripple.merge_fence_timeout_ms') = '0' AS fence_timeout_zero;

-- Verify GUCs can be set.
SET pg_ripple.citus_sharding_enabled = off;
SET pg_ripple.citus_trickle_compat = off;
SET pg_ripple.merge_fence_timeout_ms = 0;
SELECT 'guc_set_ok' AS result;

-- Restore defaults.
RESET pg_ripple.citus_sharding_enabled;
RESET pg_ripple.citus_trickle_compat;
RESET pg_ripple.merge_fence_timeout_ms;
SELECT 'guc_reset_ok' AS result;

-- v0.59.0 (CITUS-12): explain_sparql 3-arg form is properly declared.
-- Calling it directly triggers PG18's EXPLAIN-in-session guard after prior
-- explain_sparql calls in the same pg_regress session.  Verify via pg_proc
-- that the function is declared VOLATILE (required for internal EXPLAIN calls).
SELECT p.provolatile = 'v' AS explain_sparql_3arg_is_volatile
FROM pg_proc p
JOIN pg_namespace n ON n.oid = p.pronamespace
WHERE n.nspname = 'pg_ripple'
  AND p.proname = 'explain_sparql'
  AND p.pronargs = 3;
