-- bsbm_pgbench.sql — pgbench custom script for pg_ripple BSBM HTAP workload.
--
-- Simulates concurrent BSBM reads and writes to benchmark the HTAP
-- delta/main split under sustained load.
--
-- Usage (after loading baseline data with bsbm_load.sql):
--   pgbench -f benchmarks/bsbm/bsbm_pgbench.sql -c 8 -j 4 -T 60 <dbname>
--
-- Targets (pg_ripple v0.6.0 HTAP):
--   - Bulk insert throughput:  >100,000 triples/sec
--   - Q1 query latency:         <10 ms
--   - Q4 query latency:         <20 ms
--   - No write-read conflicts (transactions must complete without errors)

\set product_id random(1, 1000)
\set feature_id random(1, 5)
\set review_id  random(100000, 999999)
\set prod_rev   random(1, 1000)
\set reviewer_id random(1, 50)
\set rating     random(1, 10)
\set op         random(1, 4)

-- Mix of read queries (75%) and write inserts (25%).
BEGIN;

SELECT CASE :op
    -- Q1: Products by feature (25% of transactions)
    WHEN 1 THEN (
        SELECT count(*) FROM pg_ripple.sparql(
            'SELECT ?p WHERE { ?p <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/productFeature> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature' || :feature_id || '> . } LIMIT 10'
        )
    )
    -- Q4: Reviews for a product (25% of transactions)
    WHEN 2 THEN (
        SELECT count(*) FROM pg_ripple.sparql(
            'SELECT ?r WHERE { ?r <http://purl.org/stuff/rev#reviewOf> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || :product_id || '> . } LIMIT 8'
        )
    )
    -- Q5: Products from same vendor (25% of transactions — multi-hop join)
    WHEN 3 THEN (
        SELECT count(*) FROM pg_ripple.sparql(
            'SELECT ?p WHERE { ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || :product_id || '> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/producer> ?v . ' ||
            '?p <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/producer> ?v . } LIMIT 10'
        )
    )
    -- Write: Insert a new review triple (25% of transactions)
    ELSE (
        SELECT pg_ripple.load_ntriples(
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ReviewBench' || :review_id || '> ' ||
            '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ' ||
            '<http://purl.org/stuff/rev#Review> .' || chr(10) ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ReviewBench' || :review_id || '> ' ||
            '<http://purl.org/stuff/rev#reviewOf> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || :prod_rev || '> .' || chr(10) ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ReviewBench' || :review_id || '> ' ||
            '<http://purl.org/stuff/rev#rating> ' ||
            '"' || :rating || '"^^<http://www.w3.org/2001/XMLSchema#integer> .'
        )
    )
END;

COMMIT;
