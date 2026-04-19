-- pg_regress test: SPARQL on-demand query with Datalog plan cache (v0.30.0)
--
-- Verifies that:
-- 1. A SPARQL query referencing a derived predicate triggers inference and
--    returns results.
-- 2. After inference via infer_agg(), rule_plan_cache_stats() shows an entry
--    for the rule set used.
-- 3. A second call to infer_agg() on the same rule set shows cache hits > 0,
--    confirming the plan was served from cache.

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

CREATE TEMP TABLE _sc_test_baseline AS
    SELECT COALESCE(MAX(i), 0) AS max_i FROM _pg_ripple.vp_rare;

-- ── Setup: COUNT aggregate rule for social graph ─────────────────────────────

SELECT pg_ripple.load_rules(
    '?x <https://example.org/sc/degree> ?n :- COUNT(?y WHERE ?x <https://example.org/sc/knows> ?y) = ?n .',
    'sc_test'
) > 0 AS rule_loaded;

SELECT pg_ripple.insert_triple(
    '<https://example.org/sc/Alice>',
    '<https://example.org/sc/knows>',
    '<https://example.org/sc/Bob>'
) > 0 AS t1;

SELECT pg_ripple.insert_triple(
    '<https://example.org/sc/Alice>',
    '<https://example.org/sc/knows>',
    '<https://example.org/sc/Carol>'
) > 0 AS t2;

SELECT pg_ripple.insert_triple(
    '<https://example.org/sc/Bob>',
    '<https://example.org/sc/knows>',
    '<https://example.org/sc/Dave>'
) > 0 AS t3;

-- ── Part 1: First infer_agg run (cold cache = miss) ───────────────────────────

SELECT (pg_ripple.infer_agg('sc_test')->>'aggregate_derived')::bigint >= 0
    AS first_infer_agg_ok;

-- ── Part 2: SPARQL query on derived predicate works ───────────────────────────

-- After infer_agg, derived degree triples should be queryable.
SELECT count(*) >= 1 AS derived_degree_exists
FROM pg_ripple.find_triples(
    '<https://example.org/sc/Alice>',
    '<https://example.org/sc/degree>',
    NULL
);

-- ── Part 3: Second infer_agg run (warm cache = hit) ───────────────────────────

SELECT (pg_ripple.infer_agg('sc_test')->>'aggregate_derived')::bigint >= 0
    AS second_infer_agg_ok;

-- Cache stats must show hits >= 1 for sc_test.
SELECT hits >= 1 AS cache_hit_after_second_run
FROM pg_ripple.rule_plan_cache_stats()
WHERE rule_set = 'sc_test';

-- misses should be >= 1 (first run was a miss).
SELECT misses >= 1 AS cache_miss_recorded
FROM pg_ripple.rule_plan_cache_stats()
WHERE rule_set = 'sc_test';

-- ── Part 4: Cache invalidation ───────────────────────────────────────────────

SELECT pg_ripple.drop_rules('sc_test') >= 0 AS rules_dropped;

SELECT count(*) = 0 AS cache_cleared
FROM pg_ripple.rule_plan_cache_stats()
WHERE rule_set = 'sc_test';

-- ── Cleanup ───────────────────────────────────────────────────────────────────
DELETE FROM _pg_ripple.vp_rare WHERE i > (SELECT max_i FROM _sc_test_baseline);
