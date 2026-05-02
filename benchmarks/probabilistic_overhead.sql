-- pg_ripple benchmark: probabilistic Datalog overhead (v0.87.0 CONF-PERF-01a)
--
-- Measures the additional cost of probabilistic Datalog when @weight
-- annotations are present. Run with:
--   pgbench -f benchmarks/probabilistic_overhead.sql -c 4 -j 2 -T 30 pg_ripple_test
--
-- Prerequisites:
--   CREATE EXTENSION IF NOT EXISTS pg_ripple;
--   SELECT pg_ripple.load_rules('bench_prob', '
--     parent(X, Y) :- father(X, Y). @weight(0.9)
--     parent(X, Y) :- mother(X, Y). @weight(0.85)
--     ancestor(X, Z) :- parent(X, Z). @weight(0.9)
--   ');

\set VERBOSITY terse

-- Toggle probabilistic mode for the benchmark
SET pg_ripple.probabilistic_datalog = on;

-- Run a single-stratum inference cycle with probabilistic scoring.
SELECT pg_ripple.run_inference('bench_prob');

-- Reset
SET pg_ripple.probabilistic_datalog = off;
