-- Example: Automated Ontology Mapping (v0.57.0)
-- Demonstrates pg_ripple.suggest_mappings() for cross-ontology class alignment.
--
-- Two ontologies are loaded into named graphs, then suggest_mappings() proposes
-- class alignments using lexical Jaccard similarity over rdfs:label values.

SET search_path TO pg_ripple, public;

-- Load two ontologies into separate named graphs.
SELECT pg_ripple.load_turtle(
    $$@prefix owl:  <http://www.w3.org/2002/07/owl#> .
    @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
    @prefix onto1: <http://example.org/onto1/> .

    onto1:Person a owl:Class ; rdfs:label "Person" .
    onto1:Organization a owl:Class ; rdfs:label "Organization" .
    onto1:Employee a owl:Class ; rdfs:label "Employee" ;
        rdfs:subClassOf onto1:Person .$$,
    'http://example.org/onto1'
);

SELECT pg_ripple.load_turtle(
    $$@prefix owl:  <http://www.w3.org/2002/07/owl#> .
    @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
    @prefix onto2: <http://example.org/onto2/> .

    onto2:Human a owl:Class ; rdfs:label "Human" .
    onto2:Company a owl:Class ; rdfs:label "Company" .
    onto2:Worker a owl:Class ; rdfs:label "Worker" ;
        rdfs:subClassOf onto2:Human .$$,
    'http://example.org/onto2'
);

-- Suggest mappings using lexical similarity.
SELECT source_class, target_class, confidence
FROM pg_ripple.suggest_mappings(
    'http://example.org/onto1',
    'http://example.org/onto2',
    'lexical'
)
ORDER BY confidence DESC;

-- With KGE embeddings (requires pg_ripple.kge_enabled = on).
-- SET pg_ripple.kge_enabled = on;
-- SELECT source_class, target_class, confidence
-- FROM pg_ripple.suggest_mappings(
--     'http://example.org/onto1',
--     'http://example.org/onto2',
--     'embedding'
-- )
-- ORDER BY confidence DESC;

-- Cleanup.
SELECT pg_ripple.drop_graph('http://example.org/onto1');
SELECT pg_ripple.drop_graph('http://example.org/onto2');
