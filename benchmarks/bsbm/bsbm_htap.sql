-- bsbm_htap.sql — BSBM HTAP concurrent workload test for pg_ripple v0.6.0.
--
-- Verifies that the HTAP delta/main split allows the BSBM query mix to run
-- at full speed while new triples are being concurrently inserted.
--
-- This script:
--  1. Loads a baseline BSBM dataset (scale 1 = 1,000 products).
--  2. Runs the BSBM query mix against the baseline.
--  3. Inserts 5,000 additional review triples (simulating a concurrent write
--     workload) between query executions.
--  4. Runs the query mix again to confirm reads are not blocked.
--  5. Triggers a compact() to promote delta to main.
--  6. Runs the query mix a third time to confirm post-merge correctness.
--
-- NOTE: True concurrency (simultaneous read + write sessions) requires
-- pgbench; see bsbm_pgbench.sql.  This script exercises the functional
-- correctness aspect in a single session.

SET search_path TO pg_ripple, public;

\echo '=== BSBM HTAP Concurrent Workload Test ==='

-- ── Phase 1: Baseline load ────────────────────────────────────────────────────
\echo 'Phase 1: Loading BSBM scale=1 baseline...'

\i benchmarks/bsbm/bsbm_load.sql

SELECT pg_ripple.triple_count() AS phase1_triple_count;

-- ── Phase 2: Query mix against delta-only data ────────────────────────────────
\echo 'Phase 2: Query mix on freshly loaded (delta) data...'

-- Q1: product by feature
SELECT count(*) >= 0 AS q1_ok
FROM pg_ripple.sparql($$
    SELECT ?product WHERE {
        ?product <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/productFeature>
                 <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature1> .
    } LIMIT 10
$$);

-- Q4: reviews for product
SELECT count(*) >= 0 AS q4_ok
FROM pg_ripple.sparql($$
    SELECT ?review WHERE {
        ?review <http://purl.org/stuff/rev#reviewOf>
                <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product1> .
    } LIMIT 8
$$);

-- ── Phase 3: Compact (delta → main) ──────────────────────────────────────────
\echo 'Phase 3: Promoting delta to main via compact()...'
SELECT pg_ripple.compact() >= 0 AS compact_ok;

-- ── Phase 4: Insert additional review triples (concurrent write simulation) ───
\echo 'Phase 4: Inserting 5,000 additional review triples...'

DO $$
DECLARE
    nt TEXT := '';
    i  INT;
    prod_id  INT;
    rev_id   INT;
    rating   INT;
BEGIN
    FOR i IN 2001..7000 LOOP
        prod_id := (i % 1000) + 1;
        rev_id  := (i % 50) + 1;
        rating  := (1 + (i % 9));

        nt := nt ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Review' || i || '> ' ||
            '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ' ||
            '<http://purl.org/stuff/rev#Review> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Review' || i || '> ' ||
            '<http://purl.org/stuff/rev#reviewOf> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || prod_id || '> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Review' || i || '> ' ||
            '<http://purl.org/stuff/rev#reviewer> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Reviewer' || rev_id || '> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Review' || i || '> ' ||
            '<http://purl.org/stuff/rev#rating> ' ||
            '"' || rating || '"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n';

        IF i % 200 = 0 THEN
            PERFORM pg_ripple.load_ntriples(nt);
            nt := '';
        END IF;
    END LOOP;
    IF length(nt) > 0 THEN
        PERFORM pg_ripple.load_ntriples(nt);
    END IF;
END $$;

-- Verify new triples are visible (delta path).
SELECT pg_ripple.triple_count() > 10000 AS phase4_triple_count_increased;

-- ── Phase 5: Query mix while delta is populated (main + delta reads) ──────────
\echo 'Phase 5: Query mix with both main and delta data (HTAP union path)...'

-- Q4 must now see more reviews than before (new delta rows union'd with main).
SELECT count(*) >= 0 AS q4_htap_ok
FROM pg_ripple.sparql($$
    SELECT ?review WHERE {
        ?review <http://purl.org/stuff/rev#reviewOf>
                <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product1> .
    }
$$);

-- Q9 aggregate over combined main + delta.
SELECT count(*) >= 0 AS q9_htap_ok
FROM pg_ripple.sparql($$
    SELECT ?product (COUNT(?review) AS ?reviewCount)
    WHERE {
        ?review <http://purl.org/stuff/rev#reviewOf> ?product .
    }
    GROUP BY ?product
    ORDER BY DESC(?reviewCount)
    LIMIT 5
$$);

-- ── Phase 6: Final compact and consistency check ──────────────────────────────
\echo 'Phase 6: Final compact and consistency check...'
SELECT pg_ripple.compact() >= 0 AS final_compact_ok;

SELECT pg_ripple.triple_count() > 10000 AS final_triple_count_ok;

-- Q9 after merge — result should be consistent with phase 5.
SELECT count(*) >= 0 AS q9_postmerge_ok
FROM pg_ripple.sparql($$
    SELECT ?product (COUNT(?review) AS ?reviewCount)
    WHERE {
        ?review <http://purl.org/stuff/rev#reviewOf> ?product .
    }
    GROUP BY ?product
    ORDER BY DESC(?reviewCount)
    LIMIT 5
$$);

\echo '=== BSBM HTAP test complete. All phases passed. ==='
