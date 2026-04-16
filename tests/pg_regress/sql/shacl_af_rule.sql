-- pg_regress test: SHACL-AF sh:rule bridge

SET search_path TO pg_ripple, public;

-- Load SHACL data with sh:rule entries.
SELECT pg_ripple.load_shacl(
    E'@prefix sh: <http://www.w3.org/ns/shacl#> .\n'
    E'@prefix ex: <https://example.org/> .\n'
    E'@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n'
    E'ex:MyShape\n'
    E'    a sh:NodeShape ;\n'
    E'    sh:targetClass ex:Person ;\n'
    E'    sh:property [\n'
    E'        sh:path ex:name ;\n'
    E'        sh:minCount 1\n'
    E'    ]\n'
    E'.'
) >= 0 AS shacl_loaded;

-- Verify shape was stored.
SELECT count(*) >= 1 AS shape_present
FROM pg_ripple.list_shapes();

-- SHACL-AF bridge: load SHACL with sh:rule content.
-- The bridge detects sh:rule presence and notes it.
SELECT pg_ripple.load_shacl(
    E'@prefix sh: <http://www.w3.org/ns/shacl#> .\n'
    E'@prefix ex: <https://example.org/> .\n'
    E'ex:RuleShape\n'
    E'    a sh:NodeShape ;\n'
    E'    sh:targetClass ex:Entity ;\n'
    E'    sh:rule [\n'
    E'        rdf:type sh:TripleRule ;\n'
    E'        sh:subject ex:marker ;\n'
    E'        sh:predicate ex:hasRule ;\n'
    E'        sh:object ex:detected\n'
    E'    ]\n'
    E'.'
) >= 0 AS shacl_with_rule_loaded;

-- Cleanup shapes.
DO $$
DECLARE
    s RECORD;
BEGIN
    FOR s IN SELECT shape_iri FROM _pg_ripple.shacl_shapes LOOP
        PERFORM pg_ripple.drop_shape(s.shape_iri);
    END LOOP;
END $$;

SELECT count(*) = 0 AS shapes_cleaned
FROM pg_ripple.list_shapes();
