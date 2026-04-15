-- insert_throughput.sql — 1M-triple insert throughput benchmark for pg_ripple.
--
-- Generates 1,000,000 N-Triples across 10 predicates (100K triples each)
-- and measures wall-clock load time via pg_ripple.load_ntriples().
--
-- Usage:
--   psql -d postgres -f benchmarks/insert_throughput.sql
--
-- Or via justfile:
--   just bench-insert
--
-- Targets (per ROADMAP.md §7):
--   Pre-HTAP (v0.1.0–v0.5.1):  >30,000 triples/sec
--   Post-HTAP (v0.6.0+):       >100,000 triples/sec
--
-- Output:
--   - Total triples loaded
--   - Wall-clock elapsed time (seconds)
--   - Throughput (triples/sec)
--   - Point-query latency (single BGP)

SET search_path TO pg_ripple, public;

-- ── Clean slate (drop + recreate extension) ──────────────────────────────────
\echo '=== pg_ripple Insert Throughput Benchmark (1M triples) ==='
\echo ''
\echo 'WARNING: This will DROP and re-CREATE the pg_ripple extension.'
\echo ''

DROP EXTENSION IF EXISTS pg_ripple CASCADE;
CREATE EXTENSION pg_ripple;

-- ── Phase 1: Generate and load 1M triples ────────────────────────────────────
\echo 'Phase 1: Generating and loading 1,000,000 triples (10 predicates × 100,000 subjects)...'

\timing on

DO $$
DECLARE
    pred      INT;
    batch     INT;
    nt        TEXT;
    i         INT;
    batch_sz  INT := 10000;  -- triples per load_ntriples() call
    total     BIGINT := 0;
    t_start   TIMESTAMPTZ;
    t_end     TIMESTAMPTZ;
    elapsed   NUMERIC;
    throughput NUMERIC;
BEGIN
    t_start := clock_timestamp();

    -- 10 predicates × 100,000 subjects = 1,000,000 triples
    FOR pred IN 1..10 LOOP
        FOR batch IN 0..9 LOOP
            nt := '';
            FOR i IN (batch * batch_sz + 1)..(batch * batch_sz + batch_sz) LOOP
                nt := nt ||
                    '<http://bench.example.org/S' || i || '> ' ||
                    '<http://bench.example.org/P' || pred || '> ' ||
                    '<http://bench.example.org/O' || pred || '_' || i || '> .' || E'\n';
            END LOOP;
            PERFORM pg_ripple.load_ntriples(nt);
            total := total + batch_sz;
        END LOOP;
    END LOOP;

    t_end := clock_timestamp();
    elapsed := EXTRACT(EPOCH FROM (t_end - t_start));
    throughput := total / elapsed;

    RAISE NOTICE '──────────────────────────────────────────────';
    RAISE NOTICE 'BENCHMARK RESULTS: Insert Throughput';
    RAISE NOTICE '──────────────────────────────────────────────';
    RAISE NOTICE 'Total triples loaded:  %', total;
    RAISE NOTICE 'Elapsed time:          % seconds', round(elapsed, 3);
    RAISE NOTICE 'Throughput:            % triples/sec', round(throughput, 0);
    RAISE NOTICE '──────────────────────────────────────────────';
END $$;

\timing off

-- Verify triple count
SELECT pg_ripple.triple_count() AS total_triples;

-- ── Phase 2: Point-query latency ─────────────────────────────────────────────
\echo ''
\echo 'Phase 2: Point-query latency (single BGP, warm cache)...'

-- Warm up: run a few queries to prime the dictionary cache
SELECT count(*) FROM pg_ripple.find_triples(
    '<http://bench.example.org/S1>', '<http://bench.example.org/P1>', NULL
);
SELECT count(*) FROM pg_ripple.find_triples(
    '<http://bench.example.org/S500>', '<http://bench.example.org/P5>', NULL
);

\timing on

-- Point query: specific subject + predicate
SELECT count(*) AS point_query_result FROM pg_ripple.find_triples(
    '<http://bench.example.org/S42>', '<http://bench.example.org/P3>', NULL
);

-- Pattern query: all triples for a subject (star pattern)
SELECT count(*) AS star_query_result FROM pg_ripple.find_triples(
    '<http://bench.example.org/S100>', NULL, NULL
);

-- SPARQL BGP query
SELECT count(*) AS sparql_bgp_result FROM pg_ripple.sparql($$
    SELECT ?o WHERE {
        <http://bench.example.org/S999> <http://bench.example.org/P7> ?o .
    }
$$);

-- SPARQL star pattern (multi-predicate join)
SELECT count(*) AS sparql_star_result FROM pg_ripple.sparql($$
    SELECT ?o1 ?o2 WHERE {
        <http://bench.example.org/S500> <http://bench.example.org/P1> ?o1 .
        <http://bench.example.org/S500> <http://bench.example.org/P2> ?o2 .
    }
$$);

\timing off

\echo ''
\echo '=== Benchmark complete ==='
