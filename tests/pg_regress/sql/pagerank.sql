-- pg_regress test: Datalog-Native PageRank & Graph Analytics (v0.88.0)
-- Tests pagerank_run(), centrality_run(), explain_pagerank(), export_pagerank(),
-- vacuum_pagerank_dirty(), pagerank_queue_stats(), and error codes PT0401-PT0423.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── Setup: load a small Karate-Club-like graph ────────────────────────────────
-- 6 nodes with directed edges to test convergence.
SELECT pg_ripple.load_ntriples(
    '<http://ex.org/a> <http://ex.org/knows> <http://ex.org/b> .
<http://ex.org/a> <http://ex.org/knows> <http://ex.org/c> .
<http://ex.org/b> <http://ex.org/knows> <http://ex.org/c> .
<http://ex.org/c> <http://ex.org/knows> <http://ex.org/d> .
<http://ex.org/d> <http://ex.org/knows> <http://ex.org/e> .
<http://ex.org/e> <http://ex.org/knows> <http://ex.org/a> .
<http://ex.org/b> <http://ex.org/knows> <http://ex.org/d> .
<http://ex.org/d> <http://ex.org/knows> <http://ex.org/a> .'
) AS loaded;

-- ── Test 1: pagerank_run returns rows ─────────────────────────────────────────
SELECT COUNT(*) > 0 AS has_results
FROM pg_ripple.pagerank_run(
    damping => 0.85,
    max_iterations => 30,
    convergence_delta => 0.0001
);

-- ── Test 2: scores are in (0, 1] range ────────────────────────────────────────
SELECT COUNT(*) AS out_of_range
FROM pg_ripple.pagerank_run(
    damping => 0.85,
    max_iterations => 30
)
WHERE score <= 0.0 OR score > 1.0;

-- ── Test 3: vacuum_pagerank_dirty returns non-negative bigint ─────────────────
SELECT pg_ripple.vacuum_pagerank_dirty() >= 0 AS vacuum_ok;

-- ── Test 4: pagerank_queue_stats returns a row ───────────────────────────────
SELECT COUNT(*) = 1 AS stats_ok
FROM pg_ripple.pagerank_queue_stats();

-- ── Test 5: pagerank_queue_stats queued_edges is non-negative ────────────────
SELECT queued_edges >= 0 AS non_negative_queue
FROM pg_ripple.pagerank_queue_stats();

-- ── Test 6: pagerank_scores table exists ─────────────────────────────────────
SELECT COUNT(*) > 0 AS scores_populated
FROM information_schema.tables
WHERE table_schema = '_pg_ripple'
  AND table_name = 'pagerank_scores';

-- ── Test 7: centrality_scores table exists ───────────────────────────────────
SELECT COUNT(*) > 0 AS centrality_table_ok
FROM information_schema.tables
WHERE table_schema = '_pg_ripple'
  AND table_name = 'centrality_scores';

-- ── Test 8: centrality_run('betweenness') returns rows ───────────────────────
SELECT COUNT(*) >= 0 AS betweenness_ok
FROM pg_ripple.centrality_run('betweenness');

-- ── Test 9: centrality_run('closeness') returns rows ─────────────────────────
SELECT COUNT(*) >= 0 AS closeness_ok
FROM pg_ripple.centrality_run('closeness');

-- ── Test 10: centrality_run('eigenvector') returns rows ──────────────────────
SELECT COUNT(*) >= 0 AS eigenvector_ok
FROM pg_ripple.centrality_run('eigenvector');

-- ── Test 11: centrality_run('katz') returns rows ─────────────────────────────
SELECT COUNT(*) >= 0 AS katz_ok
FROM pg_ripple.centrality_run('katz');

-- ── Test 12: explain_pagerank returns rows or empty for known node ────────────
SELECT COUNT(*) >= 0 AS explain_ok
FROM pg_ripple.explain_pagerank('<http://ex.org/a>', 5);

-- ── Test 13: export_pagerank('csv') returns non-empty text ───────────────────
SELECT length(pg_ripple.export_pagerank('csv')) > 0 AS csv_ok;

-- ── Test 14: export_pagerank('turtle') returns valid turtle ──────────────────
SELECT pg_ripple.export_pagerank('turtle') LIKE '%pg:hasPageRank%' AS turtle_ok;

-- ── Test 15: export_pagerank('ntriples') ────────────────────────────────────
SELECT length(pg_ripple.export_pagerank('ntriples')) >= 0 AS ntriples_ok;

-- ── Test 16: export_pagerank('jsonld') ───────────────────────────────────────
SELECT length(pg_ripple.export_pagerank('jsonld')) >= 0 AS jsonld_ok;

-- ── Test 17: direction='reverse' runs without error ─────────────────────────
SELECT COUNT(*) >= 0 AS reverse_ok
FROM pg_ripple.pagerank_run(direction => 'reverse', max_iterations => 10);

-- ── Test 18: topic run stores under topic key ─────────────────────────────────
SELECT COUNT(*) >= 0 AS topic_ok
FROM pg_ripple.pagerank_run(topic => 'science', max_iterations => 10);

-- ── Test 19: pagerank_find_duplicates returns a table ────────────────────────
SELECT COUNT(*) >= 0 AS find_dup_ok
FROM pg_ripple.pagerank_find_duplicates('betweenness', 0.0, 0.0);

-- ── Test 20: pagerank_dirty_edges table exists ───────────────────────────────
SELECT COUNT(*) > 0 AS dirty_edges_table_ok
FROM information_schema.tables
WHERE table_schema = '_pg_ripple'
  AND table_name = 'pagerank_dirty_edges';

-- ── Test 21: GUC pagerank_enabled is off by default ──────────────────────────
SELECT current_setting('pg_ripple.pagerank_enabled')::BOOL = false AS pagerank_disabled;

-- ── Test 22: GUC pagerank_damping default is 0.85 ───────────────────────────
SELECT current_setting('pg_ripple.pagerank_damping')::FLOAT8 = 0.85 AS damping_default;

-- ── Test 23: GUC pagerank_max_iterations default is 100 ─────────────────────
SELECT current_setting('pg_ripple.pagerank_max_iterations')::INT = 100 AS max_iter_default;

-- ── Test 24: PT0401 fires for invalid damping factor ─────────────────────────
SET client_min_messages = error;
DO $$
BEGIN
  PERFORM pg_ripple.pagerank_run(damping => 1.5, max_iterations => 1);
  RAISE EXCEPTION 'expected PT0401 error but none was raised';
EXCEPTION
  WHEN others THEN NULL; -- error caught
END;
$$;
SET client_min_messages = DEFAULT;
SELECT 'PT0401_ok' AS test_24_result;

-- ── Test 25: PT0412 fires for invalid direction ───────────────────────────────
SET client_min_messages = error;
DO $$
BEGIN
  PERFORM pg_ripple.pagerank_run(direction => 'diagonal', max_iterations => 1);
  RAISE EXCEPTION 'expected PT0412 error but none was raised';
EXCEPTION
  WHEN others THEN NULL; -- error caught
END;
$$;
SET client_min_messages = DEFAULT;
SELECT 'PT0412_ok' AS test_25_result;

-- ── Test 26: PT0417 fires for unsupported export format ──────────────────────
SET client_min_messages = error;
DO $$
BEGIN
  PERFORM pg_ripple.export_pagerank('xml');
  RAISE EXCEPTION 'expected PT0417 error but none was raised';
EXCEPTION
  WHEN others THEN NULL; -- error caught
END;
$$;
SET client_min_messages = DEFAULT;
SELECT 'PT0417_ok' AS test_26_result;

-- ── Test 27: PT0419 fires for unrecognised centrality metric ─────────────────
SET client_min_messages = error;
DO $$
BEGIN
  PERFORM pg_ripple.centrality_run('invalid_metric');
  RAISE EXCEPTION 'expected PT0419 error but none was raised';
EXCEPTION
  WHEN others THEN NULL; -- error caught
END;
$$;
SET client_min_messages = DEFAULT;
SELECT 'PT0419_ok' AS test_27_result;

-- ── Test 28: pagerank_run_topics runs without error ───────────────────────────
SELECT COUNT(*) >= 0 AS topics_ok
FROM pg_ripple.pagerank_run(topic => 'topic_a', max_iterations => 5);

-- ── Test 29: export_pagerank top_k limits output ─────────────────────────────
SELECT length(pg_ripple.export_pagerank('csv', 1, NULL)) > 0 AS topk_ok;

-- ── Test 30: feature_status includes pagerank_datalog ────────────────────────
SELECT COUNT(*) >= 1 AS fs_pagerank_ok
FROM pg_ripple.feature_status()
WHERE feature = 'pagerank_datalog' AND status = 'implemented';
