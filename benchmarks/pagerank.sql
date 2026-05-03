-- PageRank & Graph Analytics benchmarks (v0.88.0 PR-BENCH-01).
--
-- Synthetic scale-free graph benchmark: 1M edges, power-law degree distribution.
-- Uses pgbench via: pgbench -f benchmarks/pagerank.sql -n -T 60 dbname
--
-- Pre-requisite: load synthetic data once (run outside pgbench):
--   SELECT pg_ripple.load_ntriples($ntriples_text$
--     <http://e/0> <http://p/links> <http://e/1> .
--     ...
--   $ntriples_text$, 'http://g/pagerank_bench');

-- B01: Full PageRank, default parameters
\set damping 0.85
\set max_iter 100
\set topic_id :client_id
SELECT COUNT(*)
FROM pg_ripple.pagerank_run(
    damping            => :damping,
    max_iterations     => :max_iter,
    convergence_delta  => 0.0001,
    topic              => 'bench_' || :topic_id::TEXT
);

-- B02: Top-10 lookup (post-run read path)
SELECT node_iri, score
FROM pg_ripple.pagerank_run(topic => 'bench_' || :topic_id::TEXT)
ORDER BY score DESC
LIMIT 10;

-- B03: Centrality — betweenness (approximate)
SELECT COUNT(*) FROM pg_ripple.centrality_run('betweenness');

-- B04: Centrality — PageRank-normalized
SELECT COUNT(*) FROM pg_ripple.centrality_run('pagerank');

-- B05: IVM vacuum
SELECT pg_ripple.vacuum_pagerank_dirty();

-- B06: Queue stats (read-only)
SELECT * FROM pg_ripple.pagerank_queue_stats();

-- B07: Export CSV top-10k
SELECT LENGTH(body)
FROM (
  SELECT pg_ripple.export_pagerank('csv', 10000) AS body
) q;

-- B08: Explain tree for a high-centrality node (magic-sets partial evaluation)
SELECT COUNT(*) FROM pg_ripple.explain_pagerank('<http://e/0>', 5);

-- B09: Duplicate detection (centrality + fuzzy)
SELECT COUNT(*) FROM pg_ripple.pagerank_find_duplicates('betweenness', 0.05, 0.80);

-- B10: Topic-scoped run (personalized / topic-sensitive PageRank)
SELECT COUNT(*)
FROM pg_ripple.pagerank_run(
    damping            => 0.85,
    topic              => 'science'
);
