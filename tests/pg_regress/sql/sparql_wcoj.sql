-- pg_regress test: WCOJ Leapfrog Triejoin for cyclic SPARQL patterns (v0.36.0)
--
-- Tests:
-- 1. New GUCs exist with correct defaults.
-- 2. GUCs can be set and read back.
-- 3. wcoj_is_cyclic() correctly identifies cyclic vs. acyclic patterns.
-- 4. Triangle query returns correct results with WCOJ enabled and disabled.
-- 5. wcoj_triangle_query() returns JSONB with expected fields.

-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Part 1: GUC defaults ─────────────────────────────────────────────────────

-- 1a. wcoj_enabled default = true.
SHOW pg_ripple.wcoj_enabled;

-- 1b. wcoj_min_tables default = 3.
SHOW pg_ripple.wcoj_min_tables;

-- 1c. wcoj_enabled can be disabled.
SET pg_ripple.wcoj_enabled = false;
SHOW pg_ripple.wcoj_enabled;

-- 1d. Restore to true.
SET pg_ripple.wcoj_enabled = true;
SHOW pg_ripple.wcoj_enabled;

-- 1e. wcoj_min_tables can be set to 2.
SET pg_ripple.wcoj_min_tables = 2;
SHOW pg_ripple.wcoj_min_tables;

-- Restore defaults.
SET pg_ripple.wcoj_min_tables = 3;

-- ── Part 2: Cyclic pattern detection ─────────────────────────────────────────

-- 2a. Triangle pattern is cyclic: ?a-?b, ?b-?c, ?c-?a.
SELECT pg_ripple.wcoj_is_cyclic('[["a","b"],["b","c"],["c","a"]]') AS triangle_is_cyclic;

-- 2b. Star pattern is NOT cyclic: ?root-?a, ?root-?b, ?root-?c.
SELECT pg_ripple.wcoj_is_cyclic('[["root","a"],["root","b"],["root","c"]]') AS star_not_cyclic;

-- 2c. Linear chain is NOT cyclic: ?a-?b, ?b-?c.
SELECT pg_ripple.wcoj_is_cyclic('[["a","b"],["b","c"]]') AS chain_not_cyclic;

-- 2d. 4-cycle is cyclic: ?a-?b, ?b-?c, ?c-?d, ?d-?a.
SELECT pg_ripple.wcoj_is_cyclic('[["a","b"],["b","c"],["c","d"],["d","a"]]') AS square_is_cyclic;

-- 2e. Empty pattern is NOT cyclic.
SELECT pg_ripple.wcoj_is_cyclic('[]') AS empty_not_cyclic;

-- 2f. Single pattern is NOT cyclic.
SELECT pg_ripple.wcoj_is_cyclic('[["a","b"]]') AS single_not_cyclic;

-- ── Part 3: Triangle query with data ─────────────────────────────────────────

-- Insert a social graph with known triangles.
-- Triangle: alice -> bob -> carol -> alice
SELECT pg_ripple.insert_triple(
    '<https://wcoj.test/alice>',
    '<https://wcoj.test/knows>',
    '<https://wcoj.test/bob>'
) IS NOT NULL AS alice_knows_bob;

SELECT pg_ripple.insert_triple(
    '<https://wcoj.test/bob>',
    '<https://wcoj.test/knows>',
    '<https://wcoj.test/carol>'
) IS NOT NULL AS bob_knows_carol;

SELECT pg_ripple.insert_triple(
    '<https://wcoj.test/carol>',
    '<https://wcoj.test/knows>',
    '<https://wcoj.test/alice>'
) IS NOT NULL AS carol_knows_alice;

-- Additional edges (not completing a triangle by themselves).
SELECT pg_ripple.insert_triple(
    '<https://wcoj.test/alice>',
    '<https://wcoj.test/knows>',
    '<https://wcoj.test/dave>'
) IS NOT NULL AS alice_knows_dave;

SELECT pg_ripple.insert_triple(
    '<https://wcoj.test/dave>',
    '<https://wcoj.test/knows>',
    '<https://wcoj.test/eve>'
) IS NOT NULL AS dave_knows_eve;

-- 3a. Triangle count with WCOJ enabled.
SET pg_ripple.wcoj_enabled = true;
SELECT (pg_ripple.wcoj_triangle_query('https://wcoj.test/knows')->>'triangle_count')::int AS triangle_count_wcoj_on;
SELECT (pg_ripple.wcoj_triangle_query('https://wcoj.test/knows')->>'wcoj_applied')::boolean AS wcoj_was_applied;

-- 3b. Triangle count with WCOJ disabled — must match WCOJ result.
SET pg_ripple.wcoj_enabled = false;
SELECT (pg_ripple.wcoj_triangle_query('https://wcoj.test/knows')->>'triangle_count')::int AS triangle_count_wcoj_off;
SELECT (pg_ripple.wcoj_triangle_query('https://wcoj.test/knows')->>'wcoj_applied')::boolean AS wcoj_was_not_applied;

-- Restore default.
SET pg_ripple.wcoj_enabled = true;

-- 3c. Non-existent predicate returns 0 triangles.
SELECT (pg_ripple.wcoj_triangle_query('https://wcoj.test/nonexistent')->>'triangle_count')::int AS no_triangles;

-- ── Part 4: SPARQL triangle query via the standard engine ────────────────────

-- Verify the triangle via SPARQL SELECT (independent of WCOJ).
SELECT count(*) AS sparql_triangle_count
FROM pg_ripple.sparql(
    'SELECT ?a ?b ?c WHERE { '
    '<https://wcoj.test/alice> <https://wcoj.test/knows> <https://wcoj.test/bob> . '
    '<https://wcoj.test/bob> <https://wcoj.test/knows> <https://wcoj.test/carol> . '
    '<https://wcoj.test/carol> <https://wcoj.test/knows> <https://wcoj.test/alice> . '
    'BIND(<https://wcoj.test/alice> AS ?a) '
    'BIND(<https://wcoj.test/bob> AS ?b) '
    'BIND(<https://wcoj.test/carol> AS ?c) '
    '}'
);

-- ── Part 5: GUC range validation ─────────────────────────────────────────────

-- 5a. wcoj_min_tables minimum is 2.
SET pg_ripple.wcoj_min_tables = 2;
SHOW pg_ripple.wcoj_min_tables;

-- 5b. wcoj_min_tables can be set high.
SET pg_ripple.wcoj_min_tables = 10;
SHOW pg_ripple.wcoj_min_tables;

-- Restore.
SET pg_ripple.wcoj_min_tables = 3;
SHOW pg_ripple.wcoj_min_tables;
