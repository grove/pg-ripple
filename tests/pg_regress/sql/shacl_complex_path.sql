-- shacl_complex_path.sql
-- Test SHACL property-shape validation (v0.51.0).
-- Exercises the property-path value counting introduced in v0.51.0.
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
                     1          ]                      1          ]           ti                     1          ]                EC                     1          ]                      1          ]am                     1          iol                     1          ]     trip                     1          ]g/                     1          ]  999/                   #type>',
    '<http://shacl.exampl    '<http://shacl.exampl    '<http:/ed;

-- NoName has no ex:name - should produce a minCount violation.
SELECT (pg_ripple.validate() ->> 'conforms')::boolean = false AS has_violation;

-- Test 4: Re-- Test 4: Re-- Test 4: Re-- Test 4:ate
-- Test 4: Re-- Test 4: Re-- Test 4:   -- Test 4: RE { -- Test 4: Re-- Test 4: Re--Nam-- Test 4: Re-- Test 4: Re-- Test 4:   -- Test EC-- Test 4: Re-- Test 4: Re> '-- Test 4: Re-- Test 4: Re-- Test 4:   -- Test 
----------------------------------------------/sh-----------------------------------AS-------------------------------e.sparql_update($$
    DELETE WHERE { <http://shacl.example.org/Alice> ?p ?o . }
$$) >= 0 AS alice_cleaned;
