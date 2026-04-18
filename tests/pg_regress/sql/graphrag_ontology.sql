-- pg_regress test: GraphRAG ontology loading and prefix registration (v0.26.0)
--
-- Covers:
--   1. register_prefix() for the gr: namespace
--   2. load_turtle() of the GraphRAG ontology
--   3. find_triples() to verify class triples are present
--   4. prefixes() to verify prefix was registered
--
-- Uses unique IRIs (<https://graphrag.org/ns/…>) isolated from other tests.
-- setup.sql already does DROP/CREATE EXTENSION before this file.

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET search_path TO pg_ripple, public;

-- ── 1. Register the gr: prefix ───────────────────────────────────────────────
SELECT pg_ripple.register_prefix('gr', 'https://graphrag.org/ns/') IS NOT NULL
    AS prefix_registered;

-- ── 2. Load a minimal GraphRAG ontology inline ───────────────────────────────
SELECT pg_ripple.load_turtle($ONT$
@prefix gr:   <https://graphrag.org/ns/> .
@prefix rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .

gr:Entity       a owl:Class ; rdfs:label "Entity" .
gr:Relationship a owl:Class ; rdfs:label "Relationship" .
gr:TextUnit     a owl:Class ; rdfs:label "Text Unit" .
gr:title        a owl:DatatypeProperty ; rdfs:domain gr:Entity ; rdfs:range xsd:string .
gr:type         a owl:DatatypeProperty ; rdfs:domain gr:Entity ; rdfs:range xsd:string .
gr:source       a owl:ObjectProperty ; rdfs:domain gr:Relationship ; rdfs:range gr:Entity .
gr:target       a owl:ObjectProperty ; rdfs:domain gr:Relationship ; rdfs:range gr:Entity .
gr:weight       a owl:DatatypeProperty ; rdfs:domain gr:Relationship ; rdfs:range xsd:float .
gr:text         a owl:DatatypeProperty ; rdfs:domain gr:TextUnit ; rdfs:range xsd:string .
gr:tokenCount   a owl:DatatypeProperty ; rdfs:domain gr:TextUnit ; rdfs:range xsd:integer .
$ONT$) > 0 AS ontology_triples_loaded;

-- ── 3. Verify Entity class triple is present ─────────────────────────────────
SELECT count(*) >= 1 AS entity_class_present
FROM pg_ripple.find_triples(
    '<https://graphrag.org/ns/Entity>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://www.w3.org/2002/07/owl#Class>'
);

-- ── 4. Verify Relationship class triple is present ───────────────────────────
SELECT count(*) >= 1 AS relationship_class_present
FROM pg_ripple.find_triples(
    '<https://graphrag.org/ns/Relationship>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://www.w3.org/2002/07/owl#Class>'
);

-- ── 5. Verify prefix is registered ───────────────────────────────────────────
SELECT count(*) >= 1 AS gr_prefix_registered
FROM pg_ripple.prefixes()
WHERE prefix = 'gr' AND expansion = 'https://graphrag.org/ns/';

-- ── Cleanup: remove all graphrag.test and graphrag.org/ns triples ─────────────
DO $$
BEGIN
    DELETE FROM _pg_ripple.vp_rare
    WHERE s IN (SELECT id FROM _pg_ripple.dictionary WHERE value LIKE 'https://graphrag.org/ns/%')
       OR p IN (SELECT id FROM _pg_ripple.dictionary WHERE value LIKE 'https://graphrag.org/ns/%')
       OR o IN (SELECT id FROM _pg_ripple.dictionary WHERE value LIKE 'https://graphrag.org/ns/%');
END $$;
SELECT count(*) = 0 AS ontology_cleaned
FROM pg_ripple.find_triples(
    '<https://graphrag.org/ns/Entity>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    NULL
);
