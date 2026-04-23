-- shacl_complex_path.sql
-- Test SHACL complex sh:path validation (v0.51.0).
-- Covers the now-enabled property_path module (traverse_sh_path).
SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── Setup: test graph ─────────────────────────────────────────────────────────
SELECT pg_ripple.insert_triple(
    '<http://shacl.example.org/Alice>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://shacl.example.org/Person>'
) AS triple_id;

SELECT pg_ripple.insert_triple(
    '<http://shacl.example.org/Alice>',
    '<http://shacl.example.org/name>',
    '"Alice"'
) AS triple_id;

-- ── Test 1: Load a SHACL shape with a direct path ────────────────────────────
SELECT pg_ripple.load_shacl(
    '@prefix sh: <http://www.w3.org/ns/shacl#> .
     @prefix ex: <http://shacl.example.org/> .

     ex:PersonShape a sh:NodeShape ;
       sh:targetClass ex:Person ;
       sh:property [
         sh:path ex:name ;
         sh:minCount 1 ;
       ] .'
) AS shapes_loaded;

-- ── Test 2: Validation of a conformant node returns no violations ─────────────
SELECT jsonb_array_length(violations) AS violation_count
FROM pg_ripple.validate();

-- ── Test 3: Add a node that violates minCount ─────────────────────────────────
SELECT pg_ripple.insert_triple(
    '<http://shacl.example.org/NoName>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://shacl.example.org/Person>'
) AS triple_id;

-- NoName has no ex:name — should produce a minCount violation.
SELECT jsonb_array_length(violations) >= 1 AS has_violations
FROM pg_ripple.validate();

-- ── Test 4: Drop the violating node and re-validate ───────────────────────────
SELECT pg_ripple.drop_triples(
    subject   := '<http://shacl.example.org/NoName>',
    predicate := NULL,
    object    := NULL
) AS triples_dropped;

SELECT jsonb_array_length(violations) AS violation_count_after_fix
FROM pg_ripple.validate();

SELECT pg_ripple.insert_triple(
    '<http://example.org/Alice>',
    '<http://schema.org/name>',
    '"Alice"'
);
SELECT pg_ripple.insert_triple(
    '<http://example.org/Alice>',
    '<http://schema.org/knows>',
    '<http://example.org/Bob>'
);
SELECT pg_ripple.insert_triple(
    '<http://example.org/Bob>',
    '<http://schema.org/name>',
    '"Bob"'
);

-- ── Test 1: Simple direct path (sh:path is a predicate IRI) ───────────────────
-- Load a shape with a direct path and verify validation works.
SELECT pg_ripple.load_shacl(
    '@prefix sh: <http://www.w3.org/ns/shacl#> .
     @prefix schema: <http://schema.org/> .
     @prefix ex: <http://example.org/> .

     ex:PersonShape a sh:NodeShape ;
       sh:targetClass ex:Person ;
       sh:property [
         sh:path schema:name ;
         sh:minCount 1 ;
         sh:datatype <http://www.w3.org/2001/XMLSchema#string> ;
       ] .'
);

-- Validate: Alice has a schema:name, so should conform.
SELECT conforms FROM pg_ripple.validate()
WHERE conforms = true;

-- ── Test 2: sh:minCount on a path with values (passes) ───────────────────────
SELECT count(*) AS violations
FROM jsonb_array_elements(
    (SELECT (pg_ripple.validate()).violations)
) WHERE value->>'constraint' = 'sh:minCount';

-- ── Test 3: Add a node that violates minCount ─────────────────────────────────
SELECT pg_ripple.insert_triple(
    '<http://example.org/NoName>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://example.org/Person>'
);

-- NoName has no schema:name — should produce a minCount violation.
SELECT count(*) AS min_count_violations
FROM jsonb_array_elements(
    (SELECT (pg_ripple.validate()).violations)
) WHERE value->>'constraint' = 'sh:minCount';

-- ── Cleanup ───────────────────────────────────────────────────────────────────
SELECT pg_ripple.drop_triples(
    subject   := '<http://example.org/NoName>',
    predicate := NULL,
    object    := NULL
);
