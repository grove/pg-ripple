-- Hybrid Search Benchmark (v0.28.0)
-- Measures hybrid search latency and throughput using pgbench.
--
-- Usage:
--   pgbench -h <host> -U <user> -d <db> -f benchmarks/hybrid_search.sql \
--           -T 60 -c 4 -j 4
--
-- Prerequisites:
--   1. pg_ripple installed with pgvector
--   2. At least 10,000 entities with embeddings in _pg_ripple.embeddings
--   3. An embedding API URL configured (or pre-loaded embeddings)
--
-- Target: < 50 ms P99 latency for top-10 hybrid search over 1M entities

-- ── pgbench script ────────────────────────────────────────────────────────────

\set query_text 'anti-inflammatory pain relief'
\set sparql_query 'SELECT ?entity WHERE { ?entity a <https://pharma.example/Drug> }'

-- Hybrid search: RRF fusion of SPARQL and vector results.
-- alpha = 0.5 (equal SPARQL/vector weight).
SELECT count(*) FROM pg_ripple.hybrid_search(
    :sparql_query,
    :query_text,
    10
);

-- Vector-only baseline.
SELECT count(*) FROM pg_ripple.similar_entities(
    :query_text,
    10
);

-- SPARQL-only baseline.
SELECT count(*) FROM pg_ripple.execute_sparql(
    'SELECT ?entity WHERE { ?entity a <https://pharma.example/Drug> } LIMIT 10'
);

-- RAG retrieval end-to-end.
SELECT count(*) FROM pg_ripple.rag_retrieve(
    :query_text,
    k := 10
);
