-- Datalog Aggregation Benchmark (v0.30.0)
-- Degree-centrality via COUNT aggregate rule on a synthetic social graph.
--
-- Usage:
--   psql -d <your_db> -f benchmarks/datalog_agg.sql
--
-- Requires pg_ripple ≥ 0.30.0.

-- ── Configuration ─────────────────────────────────────────────────────────────
-- Number of synthetic person nodes (creates ~N*log(N) edges for a scale-free graph).
-- Adjust NODE_COUNT for larger experiments.
\set NODE_COUNT 200

-- ── Setup ──────────────────────────────────────────────────────────────────────

\timing on

-- Drop any previous run's tables.
DROP TABLE IF EXISTS _bench_persons CASCADE;

-- Create person nodes:  person_0 … person_N-1.
CREATE TEMP TABLE _bench_persons AS
    SELECT i, format('<https://bench.example/person/%s>', i) AS iri
    FROM generate_series(0, :NODE_COUNT - 1) AS i;

-- Insert foaf:knows edges: Barabási–Albert-style preferential attachment
-- (simplified: each new node connects to the sqrt(i) nodes with smallest id).
DO $$
DECLARE
    n   integer := :NODE_COUNT;
    src text;
    dst text;
    i   integer;
    j   integer;
BEGIN
    FOR i IN 1 .. n - 1 LOOP
        src := format('<https://bench.example/person/%s>', i);
        FOR j IN 0 .. greatest(0, floor(sqrt(i))::int - 1) LOOP
            dst := format('<https://bench.example/person/%s>', j);
            PERFORM pg_ripple.insert_triple(src, '<https://xmlns.com/foaf/0.1/knows>', dst);
        END LOOP;
    END LOOP;
END $$;

SELECT pg_ripple.triple_count() AS total_triples_after_insert;

-- ── Rule setup ────────────────────────────────────────────────────────────────

-- Degree-centrality rule: count each person's out-degree.
SELECT pg_ripple.drop_rules('bench_degree') >= 0;

SELECT pg_ripple.load_rules(
    '?x <https://bench.example/outDegree> ?n :- COUNT(?y WHERE ?x <https://xmlns.com/foaf/0.1/knows> ?y) = ?n .',
    'bench_degree'
) AS rules_loaded;

-- ── Benchmark: infer_agg() ────────────────────────────────────────────────────

\echo 'Warm-up run (cold cache):'
SELECT pg_ripple.infer_agg('bench_degree') AS first_run;

\echo 'Second run (warm cache – plan served from cache):'
SELECT pg_ripple.infer_agg('bench_degree') AS second_run;

\echo 'Third run (warm cache):'
SELECT pg_ripple.infer_agg('bench_degree') AS third_run;

-- ── Cache stats ───────────────────────────────────────────────────────────────

\echo 'Plan cache statistics:'
SELECT * FROM pg_ripple.rule_plan_cache_stats() WHERE rule_set = 'bench_degree';

-- ── Spot-check results ────────────────────────────────────────────────────────

-- Person 0 should have the highest in-degree (everyone connects to node 0).
-- We query out-degree here (node 1 connects to node 0, node 2 to nodes 0 and 1, etc.)
\echo 'Sample degree values:'
SELECT s_iri, o_iri
FROM pg_ripple.find_triples(NULL, '<https://bench.example/outDegree>', NULL)
LIMIT 10;

-- ── Cleanup ───────────────────────────────────────────────────────────────────

SELECT pg_ripple.drop_rules('bench_degree') >= 0;

\timing off
\echo 'Benchmark complete.'
