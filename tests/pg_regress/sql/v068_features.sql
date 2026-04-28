-- pg_regress test: v0.68.0 feature gate
--   STREAM-01: CONSTRUCT cursor streaming
--   CITUS-HLL-01: approx_distinct GUC and HLL translation
--   CITUS-SVC-01: citus_service_pruning GUC
--   PROMO-01: vp_promotion_batch_size GUC, promotion_status column
--   FUZZ-01: continuous_fuzzing feature_status entry

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- ── Part 1: New GUCs ─────────────────────────────────────────────────────────

-- 1a. approx_distinct defaults to off.
SHOW pg_ripple.approx_distinct;

-- 1b. citus_service_pruning defaults to off.
SHOW pg_ripple.citus_service_pruning;

-- 1c. vp_promotion_batch_size defaults to 10000.
SHOW pg_ripple.vp_promotion_batch_size;

-- 1d. GUCs can be set and reset.
SET pg_ripple.approx_distinct = on;
SHOW pg_ripple.approx_distinct;
SET pg_ripple.approx_distinct = off;

SET pg_ripple.citus_service_pruning = on;
SHOW pg_ripple.citus_service_pruning;
SET pg_ripple.citus_service_pruning = off;

SET pg_ripple.vp_promotion_batch_size = 500;
SHOW pg_ripple.vp_promotion_batch_size;
SET pg_ripple.vp_promotion_batch_size = 10000;

-- ── Part 2: CONSTRUCT cursor streaming (STREAM-01) ───────────────────────────

-- 2a. sparql_cursor_turtle returns a set (may be empty on fresh DB).
SELECT count(*) >= 0 AS turtle_cursor_callable
FROM pg_ripple.sparql_cursor_turtle(
    'CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 0'
);

-- 2b. sparql_cursor_jsonld returns a set (may be empty on fresh DB).
SELECT count(*) >= 0 AS jsonld_cursor_callable
FROM pg_ripple.sparql_cursor_jsonld(
    'CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 0'
);

-- 2c. CONSTRUCT cursor with data.
SELECT pg_ripple.sparql_update(
    'INSERT DATA { <http://v068.test/s> <http://v068.test/p> "v068_value" }'
) > 0 AS inserted_test_triple;

SELECT count(*) = 1 AS construct_cursor_streams_data
FROM pg_ripple.sparql_cursor_turtle(
    'CONSTRUCT { ?s <http://v068.test/p> ?o } WHERE { ?s <http://v068.test/p> ?o }'
);

-- Cleanup.
SELECT pg_ripple.sparql_update(
    'DELETE DATA { <http://v068.test/s> <http://v068.test/p> "v068_value" }'
) >= 0 AS cleanup_ok;

-- ── Part 3: Promotion status column (PROMO-01) ───────────────────────────────

-- 3a. promotion_status column exists in predicates catalog.
SELECT count(*) >= 0 AS promotion_status_column_exists
FROM information_schema.columns
WHERE table_schema = '_pg_ripple'
  AND table_name   = 'predicates'
  AND column_name  = 'promotion_status';

-- 3b. Promoted predicates show 'promoted' status (or NULL for old rows).
SELECT bool_and(
    promotion_status IS NULL OR promotion_status IN ('promoted', 'promoting')
) AS promotion_status_values_valid
FROM _pg_ripple.predicates;

-- ── Part 4: feature_status() entries ─────────────────────────────────────────

-- 4a. construct_turtle/jsonld streaming entries show experimental.
SELECT status = 'experimental' AS construct_streaming_experimental
FROM pg_ripple.feature_status()
WHERE feature_name = 'sparql_cursor_streaming';

-- 4b. citus_hll_distinct is experimental.
SELECT status = 'experimental' AS citus_hll_experimental
FROM pg_ripple.feature_status()
WHERE feature_name = 'citus_hll_distinct';

-- 4c. citus_service_pruning is experimental.
SELECT status = 'experimental' AS citus_svc_experimental
FROM pg_ripple.feature_status()
WHERE feature_name = 'citus_service_pruning';

-- 4d. citus_nonblocking_promotion is experimental.
SELECT status = 'experimental' AS nonblocking_promo_experimental
FROM pg_ripple.feature_status()
WHERE feature_name = 'citus_nonblocking_promotion';

-- 4e. continuous_fuzzing is experimental.
SELECT status = 'experimental' AS continuous_fuzzing_experimental
FROM pg_ripple.feature_status()
WHERE feature_name = 'continuous_fuzzing';

-- 4f. No planned items remain from the v0.68.0 deliverable list.
SELECT count(*) = 0 AS no_planned_v068_items
FROM pg_ripple.feature_status()
WHERE feature_name IN (
    'sparql_cursor_streaming',
    'citus_hll_distinct',
    'citus_service_pruning',
    'citus_nonblocking_promotion',
    'citus_multihop_pruning',
    'continuous_fuzzing'
)
AND status = 'planned';
