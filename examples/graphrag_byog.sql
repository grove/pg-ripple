-- graphrag_byog.sql — End-to-end GraphRAG BYOG walkthrough
--
-- This example demonstrates using pg_ripple as the knowledge graph backend
-- for Microsoft GraphRAG's Bring Your Own Graph (BYOG) feature.
--
-- Steps:
--   1. Create a named graph and register the gr: prefix
--   2. Load the GraphRAG ontology
--   3. Insert sample entities and relationships as Turtle
--   4. Load Datalog enrichment rules and run inference
--   5. Validate the graph with SHACL shapes
--   6. Query the enriched graph via SPARQL
--   7. Export entities and relationships to Parquet for BYOG
--
-- Prerequisites:
--   CREATE EXTENSION pg_ripple;
--
-- Usage:
--   psql -d mydb -f examples/graphrag_byog.sql
--
-- BYOG settings.yaml snippet (after export):
--   entity_table_path: /tmp/graphrag_byog/entities.parquet
--   relationship_table_path: /tmp/graphrag_byog/relationships.parquet
--   text_unit_table_path: /tmp/graphrag_byog/text_units.parquet

SET search_path TO pg_ripple, public;

-- ── Step 1: Create named graph and register gr: prefix ───────────────────────

SELECT pg_ripple.create_graph('https://byog.example/kg') > 0 AS graph_created;

SELECT pg_ripple.register_prefix('gr', 'https://graphrag.org/ns/') AS prefix_registered;

-- ── Step 2: Load GraphRAG ontology ───────────────────────────────────────────
-- In production: replace with pg_read_file('/path/to/graphrag_ontology.ttl')
-- Here we load a minimal inline subset for the walkthrough.

SELECT pg_ripple.load_turtle($ONT$
@prefix gr:   <https://graphrag.org/ns/> .
@prefix rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .

gr:Entity      a owl:Class .
gr:Relationship a owl:Class .
gr:TextUnit    a owl:Class .
gr:title       a owl:DatatypeProperty ; rdfs:domain gr:Entity ; rdfs:range xsd:string .
gr:type        a owl:DatatypeProperty ; rdfs:domain gr:Entity ; rdfs:range xsd:string .
gr:description a owl:DatatypeProperty .
gr:frequency   a owl:DatatypeProperty ; rdfs:range xsd:integer .
gr:degree      a owl:DatatypeProperty ; rdfs:range xsd:integer .
gr:source      a owl:ObjectProperty   ; rdfs:domain gr:Relationship ; rdfs:range gr:Entity .
gr:target      a owl:ObjectProperty   ; rdfs:domain gr:Relationship ; rdfs:range gr:Entity .
gr:weight      a owl:DatatypeProperty ; rdfs:domain gr:Relationship ; rdfs:range xsd:float .
gr:manages     a owl:ObjectProperty .
gr:coworker    a owl:SymmetricProperty .
gr:collaborates a owl:SymmetricProperty .
gr:indirectReport a owl:TransitiveProperty .
$ONT$) > 0 AS ontology_loaded;

-- ── Step 3: Insert sample entities and relationships ─────────────────────────

SELECT pg_ripple.load_trig($DATA$
@prefix gr:  <https://graphrag.org/ns/> .
@prefix ex:  <https://byog.example/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

GRAPH <https://byog.example/kg> {
    # Entities
    ex:alice
        a gr:Entity ;
        gr:title "Alice"^^xsd:string ;
        gr:type "person"^^xsd:string ;
        gr:description "Lead engineer at Acme Corp."^^xsd:string ;
        gr:frequency "12"^^xsd:integer ;
        gr:degree "4"^^xsd:integer .

    ex:bob
        a gr:Entity ;
        gr:title "Bob"^^xsd:string ;
        gr:type "person"^^xsd:string ;
        gr:description "Senior developer at Acme Corp."^^xsd:string ;
        gr:frequency "8"^^xsd:integer ;
        gr:degree "3"^^xsd:integer .

    ex:acme
        a gr:Entity ;
        gr:title "Acme Corp"^^xsd:string ;
        gr:type "organization"^^xsd:string ;
        gr:description "A technology company."^^xsd:string ;
        gr:frequency "20"^^xsd:integer ;
        gr:degree "6"^^xsd:integer .

    ex:globex
        a gr:Entity ;
        gr:title "Globex"^^xsd:string ;
        gr:type "organization"^^xsd:string ;
        gr:description "A competitor organization."^^xsd:string ;
        gr:frequency "5"^^xsd:integer ;
        gr:degree "2"^^xsd:integer .

    # Relationships
    ex:rel_alice_acme
        a gr:Relationship ;
        gr:source ex:alice ;
        gr:target ex:acme ;
        gr:description "Alice works at Acme Corp."^^xsd:string ;
        gr:weight "0.95"^^xsd:float .

    ex:rel_bob_acme
        a gr:Relationship ;
        gr:source ex:bob ;
        gr:target ex:acme ;
        gr:description "Bob works at Acme Corp."^^xsd:string ;
        gr:weight "0.90"^^xsd:float .

    ex:rel_alice_globex
        a gr:Relationship ;
        gr:source ex:alice ;
        gr:target ex:globex ;
        gr:description "Alice has dealings with Globex."^^xsd:string ;
        gr:weight "0.60"^^xsd:float .

    # Management hierarchy (for indirectReport inference)
    ex:alice gr:manages ex:bob .

    # Text units
    ex:tu1
        a gr:TextUnit ;
        gr:text "Alice and Bob are engineers at Acme Corp."^^xsd:string ;
        gr:tokenCount "10"^^xsd:integer ;
        gr:mentionsEntity ex:alice ;
        gr:mentionsEntity ex:bob ;
        gr:mentionsEntity ex:acme .
}
$DATA$) > 0 AS sample_data_loaded;

-- Verify entity count
SELECT count(*) AS entity_count
FROM pg_ripple.sparql(
    'SELECT ?e WHERE { GRAPH <https://byog.example/kg> { ?e a <https://graphrag.org/ns/Entity> } }'
);

-- ── Step 4: Load Datalog enrichment rules and run inference ──────────────────

SELECT pg_ripple.load_rules($RULES$
# coworker: entities that both have relationships targeting the same org
?a <https://graphrag.org/ns/coworker> ?b :- ?rel1 <https://graphrag.org/ns/source> ?a, ?rel2 <https://graphrag.org/ns/source> ?b, ?rel1 <https://graphrag.org/ns/target> ?org, ?rel2 <https://graphrag.org/ns/target> ?org .

# collaborates: entities mentioned in the same text unit
?a <https://graphrag.org/ns/collaborates> ?b :- ?tu <https://graphrag.org/ns/mentionsEntity> ?a, ?tu <https://graphrag.org/ns/mentionsEntity> ?b .

# indirectReport: transitive management (base case)
?leader <https://graphrag.org/ns/indirectReport> ?sub :- ?leader <https://graphrag.org/ns/manages> ?sub .

# indirectReport: transitive management (recursive case)
?leader <https://graphrag.org/ns/indirectReport> ?sub2 :- ?leader <https://graphrag.org/ns/indirectReport> ?mid, ?mid <https://graphrag.org/ns/manages> ?sub2 .
$RULES$, 'graphrag_enrichment') >= 0 AS rules_loaded;

SELECT pg_ripple.infer('graphrag_enrichment') AS derived_triples;

-- Verify that coworker triple was derived: alice and bob both work at acme
SELECT count(*) >= 1 AS coworker_derived
FROM pg_ripple.find_triples(
    '<https://byog.example/alice>',
    '<https://graphrag.org/ns/coworker>',
    '<https://byog.example/bob>'
);

-- ── Step 5: Validate the graph with SHACL shapes ─────────────────────────────

SELECT pg_ripple.load_shacl($SHACL$
@prefix gr:  <https://graphrag.org/ns/> .
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

gr:EntityShape
    a sh:NodeShape ;
    sh:targetClass gr:Entity ;
    sh:property [
        sh:path gr:title ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:datatype xsd:string ;
    ] ;
    sh:property [
        sh:path gr:type ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
    ] ;
    sh:property [
        sh:path gr:description ;
        sh:minCount 1 ;
    ] .
$SHACL$) > 0 AS shapes_loaded;

SELECT (pg_ripple.validate() ->> 'conforms')::boolean AS graph_conforms;

-- ── Step 6: Query enriched graph via SPARQL ───────────────────────────────────

-- Find all coworker relationships derived by Datalog
SELECT r.result ->> 'a' AS person_a,
       r.result ->> 'b' AS person_b
FROM pg_ripple.sparql(
    'SELECT ?a ?b WHERE {
       ?a <https://graphrag.org/ns/coworker> ?b .
       FILTER (?a != ?b)
     }
     ORDER BY ?a ?b'
) r(result jsonb)
LIMIT 10;

-- Find entities and their types
SELECT r.result ->> 'entity' AS entity,
       r.result ->> 'title'  AS title,
       r.result ->> 'type'   AS type
FROM pg_ripple.sparql(
    'SELECT ?entity ?title ?type WHERE {
       GRAPH <https://byog.example/kg> {
         ?entity a <https://graphrag.org/ns/Entity> .
         ?entity <https://graphrag.org/ns/title> ?title .
         ?entity <https://graphrag.org/ns/type>  ?type .
       }
     }
     ORDER BY ?title'
) r(result jsonb);

-- ── Step 7: Export to Parquet for BYOG ───────────────────────────────────────
-- This writes Parquet files to /tmp/graphrag_byog/ (requires superuser).
-- Pass --output-dir /tmp/graphrag_byog to scripts/graphrag_export.py instead
-- if you prefer the Python CLI.

SELECT pg_ripple.export_graphrag_entities(
    'https://byog.example/kg',
    '/tmp/graphrag_byog_entities.parquet'
) AS entities_exported;

SELECT pg_ripple.export_graphrag_relationships(
    'https://byog.example/kg',
    '/tmp/graphrag_byog_relationships.parquet'
) AS relationships_exported;

SELECT pg_ripple.export_graphrag_text_units(
    'https://byog.example/kg',
    '/tmp/graphrag_byog_text_units.parquet'
) AS text_units_exported;

-- ── Cleanup ───────────────────────────────────────────────────────────────────
SELECT pg_ripple.drop_rules('graphrag_enrichment') >= 0 AS rules_dropped;
SELECT pg_ripple.drop_graph('https://byog.example/kg') >= 0 AS graph_dropped;
