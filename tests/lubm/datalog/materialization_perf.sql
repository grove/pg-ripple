-- LUBM Datalog sub-suite: materialization performance baseline
-- Benchmarks OWL RL materialization against the univ1 fixture.
-- Target: < 5 seconds for univ1 (< 100K triples).
--
-- Run after loading tests/lubm/fixtures/univ1.ttl.

-- Ensure a clean inference state before benchmarking
SELECT pg_ripple.load_rules_builtin('owl-rl') AS rules_loaded;

-- Time the full materialization
\timing on
SELECT pg_ripple.infer('owl-rl') AS derived_triples;
\timing off

-- Detailed stats (includes iteration count)
SELECT
    (result->>'derived')::bigint     AS derived,
    (result->>'iterations')::int     AS iterations,
    (result->>'parallel_groups')::int AS parallel_groups,
    (result->>'max_concurrent')::int  AS max_concurrent
FROM (SELECT pg_ripple.infer_with_stats('owl-rl') AS result) t;
