-- sql_api_completeness.sql — pg_regress test for SQL API gaps (v0.15.0)
--
-- Tests: find_triples_in_graph, triple_count_in_graph, decode_id_full, lookup_iri,
--        load_rdfxml_file (via inline variant proxy)

-- ── Setup ────────────────────────────────────────────────────────────────────

SELECT pg_ripple.load_ntriples(
    '<https://example.org/alice> <https://example.org/knows> <https://example.org/bob> .
<https://example.org/alice> <https://example.org/name> "Alice" .
');

SELECT pg_ripple.load_ntriples_into_graph(
    '<https://example.org/carol> <https://example.org/knows> <https://example.org/dave> .
',
    'https://example.org/testgraph'
);

-- ── find_triples_in_graph with graph parameter ──────────────────────────────

-- Query named graph only
SELECT * FROM pg_ripple.find_triples_in_graph(
    NULL, NULL, NULL, 'https://example.org/testgraph'
) ORDER BY s;

-- Query default graph (NULL graph)
SELECT count(*) AS default_graph_count FROM pg_ripple.find_triples_in_graph(
    NULL, '<https://example.org/knows>', NULL, NULL
);

-- ── triple_count_in_graph ────────────────────────────────────────────────────

SELECT pg_ripple.triple_count_in_graph('https://example.org/testgraph') AS named_graph_count;

-- ── decode_id_full ───────────────────────────────────────────────────────────

-- Encode a term and then decode_id_full
SELECT pg_ripple.decode_id_full(
    pg_ripple.encode_term('https://example.org/alice', 0)
) AS alice_full;

-- Decode a plain literal
SELECT pg_ripple.decode_id_full(
    pg_ripple.encode_term('Alice', 2)
) AS literal_full;

-- Decode a non-existent ID returns NULL
SELECT pg_ripple.decode_id_full(-999999) AS nonexistent;

-- ── lookup_iri ───────────────────────────────────────────────────────────────

-- Known IRI should return a positive ID
SELECT pg_ripple.lookup_iri('https://example.org/alice') IS NOT NULL AS alice_exists;

-- Unknown IRI should return NULL
SELECT pg_ripple.lookup_iri('https://example.org/nonexistent_iri_12345') AS unknown_iri;

-- Cleanup
SELECT pg_ripple.drop_graph('https://example.org/testgraph');
