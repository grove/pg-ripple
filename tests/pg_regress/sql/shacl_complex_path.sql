-- shacl_complex_path.sql
-- Test SHACL complex sh:path validation (v0.51.0).
-- Covers the now-enabled property_path module (traverse_sh_path).
SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- Setup: test graph
SELECT pg_ripple.insert_triple(
    '<http://shacl.example.org/Alice>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://shacl.example.org/Person>'
) > 0 AS alice_type_inserted;

SELECT pg_ripple.insert_triple(
    '<http://shacl.example.org/Alice>',
    '<http://shacl.example.org/name>',
    '"Alice"'
) > 0 AS alice_name_inserted;

-- Test 1: Load a SHACL shape with a direct path
SELECT pg_ripple.load_shacl(
    '@prefix sh: <http://www.w3.org/ns/shacl#> .
     @prefix ex: <http://shacl.example.org/> .

     ex:PersonShape a sh:NodeShape ;
       sh:targetClass ex:Person ;
       sh:property [
         sh:path ex:name ;
         sh:minCount 1 ;
       ] .'
) >= 1 AS shapes_loaded;

-- Test 2: Validation of a conformant node returns no violations
SELECT jsonb_array_length(pg_ripple.validate()->'violations') AS violation_count;

-- Test 3: Add a node that violates minCount
SELECT pg_ripple.insert_triple(
    '<http://shacl.example.org/NoName>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://shacl.example.org/Person>'
) > 0 AS noname_type_inserted;

-- NoName has no ex:name -- should produce a minCount violation.
SELECT jsonb_array_length(pg_ripple.validate()->'violations') >= 1 AS has_violations;

-- Test 4: Drop the violating node and re-validate
SELECT pg_ripple.sparql_update($$
    DELETE WHERE { <http://shacl.example.org/NoName> ?p ?o . }
$$) >= 0 AS noname_cleaned;

SELECT jsonb_array_length(pg_ripple.validate()->'violations') AS violation_count_after_fix;

-- Cleanup
SELECT pg_ripple.sparql_update($$
    DELETE WHERE { <http://shacl.example.org/Alice> ?p ?o . }
$$) >= 0 AS alice_cleaned;

SELECT pg_ripple.drop_shape('http://shacl.example.org/PersonShape') >= 0 AS shape_dropped;
