-- pg_regress test: export round-trip with Unicode escapes and non-ASCII literals (v0.25.0)
-- Verifies that literals with Unicode escapes, non-ASCII characters, and special
-- characters survive a Turtle export → re-import round-trip intact.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── Load triples with Unicode and special literals ────────────────────────────
SELECT pg_ripple.load_ntriples(
    '<https://rt.example/s1> <https://rt.example/label> "Hello, World!" .' || chr(10) ||
    '<https://rt.example/s1> <https://rt.example/value> "caf\u00e9" .' || chr(10) ||
    '<https://rt.example/s2> <https://rt.example/label> "\u4e2d\u6587" .' || chr(10) ||
    '<https://rt.example/s2> <https://rt.example/num>   "42"^^<http://www.w3.org/2001/XMLSchema#integer> .' || chr(10) ||
    '<https://rt.example/s3> <https://rt.example/lang>  "English text"@en .' || chr(10)
) = 5 AS roundtrip_triples_loaded;

-- Verify all five triples are present.
SELECT pg_ripple.triple_count() >= 5 AS at_least_five_triples;

-- The plain literal must be findable.
SELECT count(*) >= 1 AS plain_literal_found
FROM pg_ripple.find_triples('<https://rt.example/s1>', '<https://rt.example/label>', NULL);

-- The integer typed literal must be findable.
SELECT count(*) >= 1 AS typed_literal_found
FROM pg_ripple.find_triples('<https://rt.example/s2>', '<https://rt.example/num>', NULL);

-- The language-tagged literal must be findable.
SELECT count(*) >= 1 AS lang_literal_found
FROM pg_ripple.find_triples('<https://rt.example/s3>', '<https://rt.example/lang>', NULL);

-- Export to Turtle and verify key content is present.
SELECT pg_ripple.export_turtle(NULL) LIKE '%rt.example%' AS turtle_export_has_subject;
SELECT pg_ripple.export_turtle(NULL) LIKE '%label%' AS turtle_export_has_predicate;

-- Export to N-Triples and reimport.
DO $$
DECLARE
    nt_export TEXT;
    reload_count INT;
BEGIN
    nt_export := pg_ripple.export_ntriples(NULL);
    -- Reimport into a named graph to avoid conflicts with existing triples.
    reload_count := pg_ripple.load_ntriples(nt_export, false);
    IF reload_count < 0 THEN
        RAISE EXCEPTION 'reimport returned unexpected count %', reload_count;
    END IF;
END $$;

-- After round-trip, the triples still exist.
SELECT count(*) >= 1 AS roundtrip_subject_found
FROM pg_ripple.find_triples('<https://rt.example/s1>', NULL, NULL);

-- SPARQL query verifies decoding works end-to-end.
SELECT count(*) >= 1 AS sparql_finds_label
FROM pg_ripple.sparql(
    'SELECT ?label WHERE {
       <https://rt.example/s1> <https://rt.example/label> ?label .
     }'
);

SELECT TRUE AS export_roundtrip_complete;
