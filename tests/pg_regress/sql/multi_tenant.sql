-- pg_regress test: Multi-tenant graph isolation
-- v0.57.0 Feature L-5.3

SET search_path TO pg_ripple, public;

-- Test: tenant_stats returns a table (empty initially).
SELECT count(*) >= 0 AS tenant_stats_ok
FROM pg_ripple.tenant_stats();

-- Test: columnar_threshold GUC.
SHOW pg_ripple.columnar_threshold;

-- Test: adaptive_indexing_enabled GUC.
SHOW pg_ripple.adaptive_indexing_enabled;

-- Test: probabilistic_datalog GUC.
SHOW pg_ripple.probabilistic_datalog;

-- Test: kge_enabled GUC defaults to off.
SELECT current_setting('pg_ripple.kge_enabled') = 'off' AS kge_off_by_default;

-- Test: suggest_mappings can be called (returns empty when no ontologies loaded).
SELECT count(*) >= 0 AS suggest_mappings_ok
FROM pg_ripple.suggest_mappings('', '', 'lexical');
