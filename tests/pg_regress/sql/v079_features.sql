-- pg_regress test: v0.79.0 feature_status() completeness (WCOJ-LFTI-01f, SHACL-SPARQL-01f)
--
-- Verifies that after v0.79.0, all feature_status() rows show 'implemented'
-- (except optional/external dependencies that intentionally remain in
-- 'experimental', 'degraded', or 'planned').

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── All core rows must be implemented ────────────────────────────────────────

-- The wcoj row must be 'implemented' (was 'planner_hint' before v0.79.0).
SELECT status AS wcoj_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'wcoj';

-- The shacl_sparql_rule row must be 'implemented' (was 'planned' before v0.79.0).
SELECT status AS shacl_sparql_rule_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'shacl_sparql_rule';

-- No core feature rows should still be in 'planned' status after v0.79.0.
-- (Optional/external features may remain in 'experimental' or 'degraded'.)
SELECT count(*) AS planned_core_features
FROM pg_ripple.feature_status()
WHERE status = 'planned'
  AND feature_name NOT IN (
      -- Intentionally planned-only external features
      'citus_service_pruning',
      -- sparql_12 is waiting for upstream spargebra SPARQL 1.2 grammar support
      'sparql_12'
  );

-- shacl_sparql_constraint must remain implemented.
SELECT status AS shacl_constraint_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'shacl_sparql_constraint';

-- sparql_select and sparql_update must be implemented.
SELECT feature_name, status
FROM pg_ripple.feature_status()
WHERE feature_name IN ('sparql_select', 'sparql_update')
ORDER BY feature_name;

-- construct_writeback must be implemented.
SELECT status AS construct_writeback_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'construct_writeback';

-- datalog_inference must be implemented.
SELECT status AS datalog_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'datalog_inference';

-- The total number of feature rows must be > 10 (sanity check).
SELECT count(*) > 10 AS has_enough_features
FROM pg_ripple.feature_status();
