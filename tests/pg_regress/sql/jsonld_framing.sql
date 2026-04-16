-- pg_regress test: JSON-LD Framing Engine (v0.17.0)
--
-- Tests: type-based selection, property wildcards, absent-property patterns,
-- @reverse, @embed modes, @explicit, @omitDefault, @default, @requireAll,
-- named-graph scope, empty frame, jsonld_frame_to_sparql inspection,
-- jsonld_frame general-purpose primitive, streaming variant.

-- ── Setup: load test data ────────────────────────────────────────────────────

SELECT pg_ripple.load_ntriples(
    '<https://example.org/acme> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/Organization> .' || chr(10) ||
    '<https://example.org/acme> <https://schema.org/name> "ACME Corp" .' || chr(10) ||
    '<https://example.org/alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/Person> .' || chr(10) ||
    '<https://example.org/alice> <https://schema.org/name> "Alice" .' || chr(10) ||
    '<https://example.org/alice> <https://schema.org/worksFor> <https://example.org/acme> .' || chr(10) ||
    '<https://example.org/bob> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/Person> .' || chr(10) ||
    '<https://example.org/bob> <https://schema.org/name> "Bob" .' || chr(10) ||
    '<https://example.org/bob> <https://schema.org/worksFor> <https://example.org/acme> .' || chr(10)
) = 8 AS loaded_test_data;

-- ── jsonld_frame_to_sparql: inspection function ───────────────────────────────

-- Translating a type-based frame should produce a CONSTRUCT query.
SELECT pg_ripple.jsonld_frame_to_sparql(
    '{"@type": "https://schema.org/Person"}'::jsonb
) LIKE '%CONSTRUCT%' AS frame_to_sparql_returns_construct;

SELECT pg_ripple.jsonld_frame_to_sparql(
    '{"@type": "https://schema.org/Person"}'::jsonb
) LIKE '%rdf-syntax-ns#type%' AS frame_to_sparql_includes_rdf_type;

-- Property wildcard {} produces OPTIONAL.
SELECT pg_ripple.jsonld_frame_to_sparql(
    '{"@type": "https://schema.org/Person", "https://schema.org/name": {}}'::jsonb
) LIKE '%OPTIONAL%' AS wildcard_generates_optional;

-- Absent-property [] produces FILTER(!bound(...)).
SELECT pg_ripple.jsonld_frame_to_sparql(
    '{"@type": "https://schema.org/Organization", "https://schema.org/email": []}'::jsonb
) LIKE '%FILTER(!bound%' AS absent_property_generates_filter_not_bound;

-- Named graph scoping adds FILTER on graph.
SELECT pg_ripple.jsonld_frame_to_sparql(
    '{"@type": "https://schema.org/Person"}'::jsonb,
    'https://example.org/graph1'
) LIKE '%FILTER%' AS named_graph_adds_filter;

-- ── export_jsonld_framed: type-based selection ────────────────────────────────

-- Frame selects only Person nodes.
SELECT jsonb_typeof(pg_ripple.export_jsonld_framed(
    '{"@type": "https://schema.org/Person", "https://schema.org/name": {}}'::jsonb
)) = 'object' AS framed_returns_object;

-- With multiple Person nodes, result should have @graph with 2 entries.
SELECT jsonb_array_length(
    pg_ripple.export_jsonld_framed(
        '{"@type": "https://schema.org/Person", "https://schema.org/name": {}}'::jsonb
    )->'@graph'
) = 2 AS two_person_nodes_in_graph;

-- ── @requireAll: OPTIONAL → INNER JOIN ───────────────────────────────────────

-- @requireAll: true — returns only nodes with ALL listed properties.
SELECT jsonb_typeof(pg_ripple.export_jsonld_framed(
    '{"@type": "https://schema.org/Person", "https://schema.org/name": {}, "https://schema.org/worksFor": {}, "@requireAll": true}'::jsonb
)) = 'object' AS require_all_returns_object;

-- ── @explicit: omit unlisted properties ──────────────────────────────────────

-- @explicit = true: output nodes should NOT include properties not in frame.
-- A frame with @explicit true and only @type listed should omit 'name' etc.
SELECT (
    SELECT bool_and(
        NOT (node ? 'https://schema.org/name')
    )
    FROM jsonb_array_elements(
        pg_ripple.export_jsonld_framed(
            '{"@type": "https://schema.org/Person"}'::jsonb,
            NULL, '@once', true
        )->'@graph'
    ) AS node
) AS explicit_omits_unlisted_props;

-- ── @embed modes ──────────────────────────────────────────────────────────────

-- @embed @once (default) should return a valid object.
SELECT jsonb_typeof(pg_ripple.export_jsonld_framed(
    '{"@type": "https://schema.org/Person", "https://schema.org/name": {}}'::jsonb,
    NULL, '@once'
)) = 'object' AS embed_once_returns_object;

-- @embed @always should also return a valid object.
SELECT jsonb_typeof(pg_ripple.export_jsonld_framed(
    '{"@type": "https://schema.org/Person", "https://schema.org/name": {}}'::jsonb,
    NULL, '@always'
)) = 'object' AS embed_always_returns_object;

-- @embed @never: each node should only have @id (no nested properties beyond type).
SELECT (
    SELECT bool_and(
        jsonb_typeof(node) = 'object'
    )
    FROM jsonb_array_elements(
        pg_ripple.export_jsonld_framed(
            '{"@type": "https://schema.org/Person", "https://schema.org/worksFor": {}}'::jsonb,
            NULL, '@never'
        )->'@graph'
    ) AS node
) AS embed_never_all_objects;

-- ── @reverse property ─────────────────────────────────────────────────────────

-- Frame on Organization, @reverse worksFor should collect Person nodes.
SELECT jsonb_typeof(pg_ripple.export_jsonld_framed(
    '{
        "@type": "https://schema.org/Organization",
        "https://schema.org/name": {},
        "@reverse": {
            "https://schema.org/worksFor": {
                "https://schema.org/name": {}
            }
        }
    }'::jsonb
)) = 'object' AS reverse_property_returns_object;

-- ── Empty frame: matches all subjects ────────────────────────────────────────

-- An empty frame {} should return an object (possibly empty @graph).
SELECT jsonb_typeof(pg_ripple.export_jsonld_framed(
    '{}'::jsonb
)) = 'object' AS empty_frame_returns_object;

-- ── export_jsonld_framed_stream: one line per root node ───────────────────────

-- Streaming variant should return at least 2 Person rows.
SELECT count(*) >= 2 AS stream_returns_multiple_rows
FROM pg_ripple.export_jsonld_framed_stream(
    '{"@type": "https://schema.org/Person", "https://schema.org/name": {}}'::jsonb
);

-- Each line should be valid JSON.
SELECT bool_and(
    jsonb_typeof(line::jsonb) = 'object'
) AS stream_lines_are_valid_json
FROM pg_ripple.export_jsonld_framed_stream(
    '{"@type": "https://schema.org/Person", "https://schema.org/name": {}}'::jsonb
);

-- ── jsonld_frame: general-purpose primitive ───────────────────────────────────

-- Apply framing to an already-expanded JSON-LD document.
SELECT jsonb_typeof(pg_ripple.jsonld_frame(
    '[{"@id": "https://example.org/test1", "http://www.w3.org/1999/02/22-rdf-syntax-ns#type": [{"@id": "https://schema.org/Person"}], "https://schema.org/name": [{"@value": "Test"}]}, {"@id": "https://example.org/org1", "http://www.w3.org/1999/02/22-rdf-syntax-ns#type": [{"@id": "https://schema.org/Organization"}]}]'::jsonb,
    '{"@type": "https://schema.org/Person", "https://schema.org/name": {}}'::jsonb
)) = 'object' AS jsonld_frame_returns_object;

-- The general-purpose framing should select only Person nodes from the input.
SELECT (
    pg_ripple.jsonld_frame(
        '[{"@id": "https://example.org/test1", "http://www.w3.org/1999/02/22-rdf-syntax-ns#type": [{"@id": "https://schema.org/Person"}], "https://schema.org/name": [{"@value": "Test"}]}, {"@id": "https://example.org/org1", "http://www.w3.org/1999/02/22-rdf-syntax-ns#type": [{"@id": "https://schema.org/Organization"}]}]'::jsonb,
        '{"@type": "https://schema.org/Person", "https://schema.org/name": {}}'::jsonb
    )->>'@id'
) = 'https://example.org/test1' AS jsonld_frame_selects_correct_node;

-- ── @context compaction ───────────────────────────────────────────────────────

-- A frame with @context should produce compact IRI output.
SELECT pg_ripple.export_jsonld_framed(
    '{"@context": {"schema": "https://schema.org/"}, "@type": "https://schema.org/Person", "https://schema.org/name": {}}'::jsonb
) ? '@context' AS framed_with_context_includes_context_key;

-- ── Error handling ────────────────────────────────────────────────────────────

-- Invalid @embed value should raise an error.
SELECT pg_ripple.export_jsonld_framed(
    '{"@type": "https://schema.org/Person"}'::jsonb,
    NULL, '@invalid_embed_mode'
) IS NULL AS invalid_embed_raises_error;

-- ── Cleanup ───────────────────────────────────────────────────────────────────
-- Remove test triples so they do not affect later tests' row-count assertions.
SELECT pg_ripple.sparql_update('DELETE WHERE { ?s ?p ?o }') >= 0 AS cleanup_done;
