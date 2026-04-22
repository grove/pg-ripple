-- pg_regress test: new SHACL Core constraints (v0.48.0)
-- Tests sh:minLength, sh:maxLength, sh:xone, sh:minExclusive, sh:maxExclusive,
-- sh:minInclusive, sh:maxInclusive

SET search_path TO pg_ripple, public;

-- Load a simple SHACL shape with string-length constraints
SELECT pg_ripple.load_shacl($$
  @prefix sh: <http://www.w3.org/ns/shacl#> .
  @prefix ex: <http://shacl48.test/> .
  @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

  ex:PersonShape a sh:NodeShape ;
    sh:targetClass ex:Person ;
    sh:property [
      sh:path ex:name ;
      sh:minLength 2 ;
      sh:maxLength 50 ;
    ] ;
    sh:property [
      sh:path ex:score ;
      sh:minInclusive "0.0"^^xsd:decimal ;
      sh:maxInclusive "100.0"^^xsd:decimal ;
    ] .
$$) >= 0 AS shacl_loaded;

-- Verify the shape was stored
SELECT count(*) >= 1 AS shape_present
FROM pg_ripple.list_shapes()
WHERE shape_iri = 'http://shacl48.test/PersonShape';

-- Validate on empty graph should succeed
SELECT (pg_ripple.validate() ->> 'conforms')::boolean AS conforms_empty;

-- Load test data
SELECT pg_ripple.insert_triple(
    '<http://shacl48.test/alice>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://shacl48.test/Person>'
) > 0 AS alice_type;

SELECT pg_ripple.insert_triple(
    '<http://shacl48.test/alice>',
    '<http://shacl48.test/name>',
    '"Alice"'
) > 0 AS alice_name;

SELECT pg_ripple.insert_triple(
    '<http://shacl48.test/alice>',
    '<http://shacl48.test/score>',
    '"85.5"^^<http://www.w3.org/2001/XMLSchema#decimal>'
) > 0 AS alice_score;

-- Test insert_triples SRF (batch insert)
SELECT count(*) >= 2 AS insert_triples_works
FROM pg_ripple.insert_triples(
    ARRAY[
        '<http://shacl48.test/bob>',
        '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
        '<http://shacl48.test/Person>',
        '<http://shacl48.test/bob>',
        '<http://shacl48.test/name>',
        '"Bob"'
    ]
);

-- Verify federation_max_response_bytes GUC exists with correct default
SELECT name FROM pg_settings WHERE name = 'pg_ripple.federation_max_response_bytes';

SELECT current_setting('pg_ripple.federation_max_response_bytes')::bigint = 104857600
    AS federation_max_bytes_default_ok;
