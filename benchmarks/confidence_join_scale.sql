-- pg_ripple benchmark: confidence JOIN scaling (v0.87.0 CONF-PERF-01b)
--
-- Measures the cost of pg:confidence() SPARQL function at scale by injecting
-- a large number of confidence rows and running a SPARQL SELECT with a
-- confidence filter.
--
-- Run with:
--   pgbench -f benchmarks/confidence_join_scale.sql -c 4 -j 2 -T 60 pg_ripple_test

\set VERBOSITY terse

-- SPARQL SELECT with confidence filter (should use confidence_stmt_idx)
SELECT pg_ripple.sparql(
  'PREFIX pg: <http://pg-ripple.org/functions/>
   SELECT ?s ?p ?o ?conf WHERE {
     ?s ?p ?o .
     BIND(pg:confidence(?s, ?p, ?o) AS ?conf)
     FILTER(?conf > 0.5)
   } LIMIT 1000'
);
