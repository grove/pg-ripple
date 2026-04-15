-- pg_regress test: v0.9.0 serialization — RDF/XML, Turtle export, JSON-LD export
-- Namespace: https://serial.test/

DO $$
BEGIN
    DELETE FROM _pg_ripple.vp_rare
    WHERE p IN (
        SELECT id FROM _pg_ripple.dictionary
        WHERE value LIKE 'https://serial.test/%'
    );
END $$;

-- ── RDF/XML load ──────────────────────────────────────────────────────────────

SELECT pg_ripple.load_rdfxml(
    '<?xml version="1.0"?>' ||
    '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"' ||
    '         xmlns:ex="https://serial.test/">' ||
    '  <rdf:Description rdf:about="https://serial.test/alice">' ||
    '    <ex:name>Alice</ex:name>' ||
    '    <ex:knows rdf:resource="https://serial.test/bob"/>' ||
    '  </rdf:Description>' ||
    '  <rdf:Description rdf:about="https://serial.test/bob">' ||
    '    <ex:name>Bob</ex:name>' ||
    '  </rdf:Description>' ||
    '</rdf:RDF>'
) = 4 AS rdfxml_loaded_four_triples;

-- Verify the triples were loaded
SELECT count(*) >= 1 AS alice_name_found
FROM pg_ripple.find_triples('<https://serial.test/alice>', '<https://serial.test/name>', NULL);

SELECT count(*) >= 1 AS alice_knows_bob_found
FROM pg_ripple.find_triples('<https://serial.test/alice>', '<https://serial.test/knows>', '<https://serial.test/bob>');

-- ── Turtle export ─────────────────────────────────────────────────────────────

-- export_turtle returns TEXT and should mention alice
SELECT pg_ripple.export_turtle(NULL) LIKE '%serial.test/alice%' AS turtle_contains_alice;

-- export_turtle output ends with a triple dot
SELECT pg_ripple.export_turtle(NULL) LIKE '% .' AS turtle_has_triple_dot;

-- ── JSON-LD export ────────────────────────────────────────────────────────────

-- export_jsonld returns JSONB array
SELECT jsonb_typeof(pg_ripple.export_jsonld(NULL)) = 'array' AS jsonld_is_array;

-- JSON-LD result contains @id entries
SELECT count(*) >= 1 AS jsonld_has_id_entries
FROM jsonb_array_elements(pg_ripple.export_jsonld(NULL)) AS elem
WHERE elem ? '@id';

-- JSON-LD result contains alice
SELECT count(*) >= 1 AS jsonld_contains_alice
FROM jsonb_array_elements(pg_ripple.export_jsonld(NULL)) AS elem
WHERE elem->>'@id' = 'https://serial.test/alice';

-- ── Streaming Turtle export ───────────────────────────────────────────────────

SELECT count(*) >= 1 AS turtle_stream_has_lines
FROM pg_ripple.export_turtle_stream(NULL);

SELECT count(*) >= 1 AS turtle_stream_has_alice
FROM pg_ripple.export_turtle_stream(NULL)
WHERE line LIKE '%serial.test/alice%';

-- ── Streaming JSON-LD export ──────────────────────────────────────────────────

SELECT count(*) >= 1 AS jsonld_stream_has_lines
FROM pg_ripple.export_jsonld_stream(NULL);

-- ── Round-trip: load RDF/XML → export Turtle → verify content ─────────────────

-- After loading the RDF/XML data above, export_turtle should contain bob's name
SELECT pg_ripple.export_turtle(NULL) LIKE '%serial.test/bob%' AS turtle_round_trip_ok;
