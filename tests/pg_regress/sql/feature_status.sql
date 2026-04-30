-- pg_regress test: feature_status() catalog (v0.64.0, TRUTH-01)
-- Verifies that pg_ripple.feature_status() returns honest status values
-- for all partial features identified in PLAN_OVERALL_ASSESSMENT_8.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- The function must return a table with the required columns.
SELECT count(*) > 0 AS has_rows FROM pg_ripple.feature_status();

-- All returned status values must be from the approved taxonomy.
SELECT count(*) AS invalid_statuses
FROM pg_ripple.feature_status()
WHERE status NOT IN (
    'implemented',
    'experimental',
    'planner_hint',
    'manual_refresh',
    'stub',
    'degraded',
    'planned'
);

-- arrow_flight is experimental (HMAC signing + Arrow IPC added in v0.66.0).
SELECT status AS arrow_flight_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'arrow_flight';

-- wcoj must be implemented (v0.79.0 WCOJ-LFTI-01 delivers the true Leapfrog Triejoin executor).
SELECT status AS wcoj_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'wcoj';

-- shacl_sparql_rule must be implemented (v0.79.0 SHACL-SPARQL-01).
SELECT status AS shacl_rule_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'shacl_sparql_rule';

-- construct_writeback must be implemented (v0.65.0 closes the delta maintenance gap).
SELECT status AS construct_writeback_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'construct_writeback';

-- citus_service_pruning must be planned (not wired into translator yet).
SELECT status NOT IN ('implemented') AS citus_pruning_not_implemented
FROM pg_ripple.feature_status()
WHERE feature_name = 'citus_service_pruning';

-- sparql_select and sparql_update must be implemented.
SELECT feature_name, status
FROM pg_ripple.feature_status()
WHERE feature_name IN ('sparql_select', 'sparql_update')
ORDER BY feature_name;

-- shacl_sparql_constraint must be implemented (distinct from shacl_sparql_rule).
SELECT status AS shacl_constraint_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'shacl_sparql_constraint';

-- Every row must have a non-null feature_name and status.
SELECT count(*) AS null_feature_names
FROM pg_ripple.feature_status()
WHERE feature_name IS NULL OR status IS NULL;
