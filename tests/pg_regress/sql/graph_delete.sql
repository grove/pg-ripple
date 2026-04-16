-- graph_delete.sql — pg_regress test for graph-aware deletion (v0.15.0)
--
-- Tests: delete_triple_from_graph, clear_graph

-- ── Setup: load data into named graphs ───────────────────────────────────────

SELECT pg_ripple.load_ntriples_into_graph(
    '<https://example.org/s1> <https://example.org/p1> <https://example.org/o1> .
<https://example.org/s2> <https://example.org/p1> <https://example.org/o2> .
<https://example.org/s3> <https://example.org/p2> <https://example.org/o3> .
',
    'https://example.org/gdelete'
) AS loaded;

SELECT pg_ripple.triple_count_in_graph('https://example.org/gdelete') AS before_delete;

-- ── delete_triple_from_graph ─────────────────────────────────────────────────

SELECT pg_ripple.delete_triple_from_graph(
    '<https://example.org/s1>',
    '<https://example.org/p1>',
    '<https://example.org/o1>',
    'https://example.org/gdelete'
) AS deleted_count;

SELECT pg_ripple.triple_count_in_graph('https://example.org/gdelete') AS after_delete_one;

-- Verify the deleted triple no longer appears
SELECT * FROM pg_ripple.find_triples_in_graph(
    '<https://example.org/s1>',
    '<https://example.org/p1>',
    '<https://example.org/o1>',
    'https://example.org/gdelete'
) AS deleted_check;

-- ── clear_graph ──────────────────────────────────────────────────────────────

-- Load a second graph for clear_graph testing
SELECT pg_ripple.load_ntriples_into_graph(
    '<https://example.org/a> <https://example.org/b> <https://example.org/c> .
',
    'https://example.org/gclear'
) AS loaded_gclear;

SELECT pg_ripple.triple_count_in_graph('https://example.org/gclear') AS gclear_before;

SELECT pg_ripple.clear_graph('https://example.org/gclear') AS cleared_count;

SELECT pg_ripple.triple_count_in_graph('https://example.org/gclear') AS gclear_after;

-- Verify clear_graph didn't affect gdelete
SELECT pg_ripple.triple_count_in_graph('https://example.org/gdelete') AS gdelete_intact;

-- Cleanup
SELECT pg_ripple.drop_graph('https://example.org/gdelete');
SELECT pg_ripple.drop_graph('https://example.org/gclear');
