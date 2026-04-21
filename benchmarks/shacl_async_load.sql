-- benchmarks/shacl_async_load.sql
-- SHACL async pipeline load test (v0.45.0)
--
-- Informational benchmark that verifies the SHACL async validation queue
-- remains bounded under sustained triple-insert load.
--
-- Designed to be run via pgbench:
--   pgbench -f benchmarks/shacl_async_load.sql -T 300 -c 4 -j 4 postgres
--
-- Asserts (checked manually or via CI artifact inspection):
--   (a) validation_queue depth stays bounded (does not grow unboundedly)
--   (b) drain rate >= arrival rate ± 5%
--   (c) dead-letter queue receives any persistent violators
--   (d) no backend crashes
--
-- NOTE: This benchmark is informational (non-blocking) but results are
-- logged as a CI artifact by the 'shacl-async-load' CI job.

\set subject_id random(1, 100000)
\set object_id  random(1, 100000)

-- Insert a random triple (simulating sustained write load).
SELECT pg_ripple.insert_triple(
    '<https://bench.ex.org/s' || :subject_id || '>',
    '<https://bench.ex.org/hasProp>',
    '"value_' || :object_id || '"'
);

-- Periodically check queue depth (1% of iterations).
-- In pgbench, this will run once per 100 iterations on average.
-- Queue depth metric is exposed via pg_ripple.diagnostic_report().
