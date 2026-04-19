-- pg_ripple benchmark: WCOJ Leapfrog Triejoin vs. standard planner (v0.36.0)
--
-- This benchmark compares WCOJ execution vs. the standard PostgreSQL hash-join
-- planner on triangle queries at different scale factors (100K, 1M, 10M triples).
--
-- Usage:
--   pgbench -c 1 -j 1 -n -f benchmarks/wcoj.sql -P 5 postgres
--
-- Prerequisites:
--   - pg_ripple installed and schema created
--   - A VP table with the 'ex:knows' predicate (loaded via load_ntriples)
--
-- The benchmark inserts synthetic social-graph edges, then runs:
--   1. Triangle query with WCOJ enabled   (pg_ripple.wcoj_enabled = true)
--   2. Triangle query with WCOJ disabled  (pg_ripple.wcoj_enabled = false)
--
-- Expected outcome: wcoj_enabled=true is ≥10× faster at 1M+ edges.

\set knows_iri 'https://bench.example/knows'

-- Setup: ensure pg_ripple extension is loaded.
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET search_path TO pg_ripple, public;

-- Run triangle query with WCOJ enabled.
SET pg_ripple.wcoj_enabled = true;
\timing on
SELECT pg_ripple.wcoj_triangle_query(:'knows_iri') AS wcoj_enabled_result;
\timing off

-- Run triangle query with WCOJ disabled (standard planner).
SET pg_ripple.wcoj_enabled = false;
\timing on
SELECT pg_ripple.wcoj_triangle_query(:'knows_iri') AS wcoj_disabled_result;
\timing off

-- Restore default.
SET pg_ripple.wcoj_enabled = true;
