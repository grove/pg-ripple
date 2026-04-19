-- Shared bibliographic fixture dataset for documentation examples
-- Reused across all feature deep-dive chapters and tutorials
--
-- Domain: papers, authors, institutions, topics, citations, embeddings
-- Loaded once per docs test run

-- Ensure extension is available
CREATE EXTENSION IF NOT EXISTS pg_ripple;

-- Register common prefixes
SELECT pg_ripple.register_prefix('ex', 'http://example.org/');
SELECT pg_ripple.register_prefix('foaf', 'http://xmlns.com/foaf/0.1/');
SELECT pg_ripple.register_prefix('dc', 'http://purl.org/dc/elements/1.1/');
SELECT pg_ripple.register_prefix('dcterms', 'http://purl.org/dc/terms/');
SELECT pg_ripple.register_prefix('schema', 'http://schema.org/');
SELECT pg_ripple.register_prefix('rdfs', 'http://www.w3.org/2000/01/rdf-schema#');
SELECT pg_ripple.register_prefix('rdf', 'http://www.w3.org/1999/02/22-rdf-syntax-ns#');
SELECT pg_ripple.register_prefix('xsd', 'http://www.w3.org/2001/XMLSchema#');
SELECT pg_ripple.register_prefix('owl', 'http://www.w3.org/2002/07/owl#');
SELECT pg_ripple.register_prefix('skos', 'http://www.w3.org/2004/02/skos/core#');
SELECT pg_ripple.register_prefix('bib', 'http://example.org/bib/');
SELECT pg_ripple.register_prefix('sh', 'http://www.w3.org/ns/shacl#');

-- Load bibliographic dataset
SELECT pg_ripple.load_turtle('
@prefix ex:      <http://example.org/> .
@prefix foaf:    <http://xmlns.com/foaf/0.1/> .
@prefix dc:      <http://purl.org/dc/elements/1.1/> .
@prefix dcterms: <http://purl.org/dc/terms/> .
@prefix schema:  <http://schema.org/> .
@prefix rdfs:    <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf:     <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:     <http://www.w3.org/2001/XMLSchema#> .
@prefix owl:     <http://www.w3.org/2002/07/owl#> .
@prefix skos:    <http://www.w3.org/2004/02/skos/core#> .
@prefix bib:     <http://example.org/bib/> .

# --- Institutions ---
bib:mit       a schema:Organization ;
              schema:name "Massachusetts Institute of Technology" ;
              schema:alternateName "MIT" .

bib:stanford  a schema:Organization ;
              schema:name "Stanford University" .

bib:oxford    a schema:Organization ;
              schema:name "University of Oxford" .

bib:eth       a schema:Organization ;
              schema:name "ETH Zurich" .

# --- Authors ---
bib:alice     a foaf:Person ;
              foaf:name "Alice Chen" ;
              schema:affiliation bib:mit ;
              schema:email "alice@mit.edu" .

bib:bob       a foaf:Person ;
              foaf:name "Bob Smith" ;
              schema:affiliation bib:stanford .

bib:carol     a foaf:Person ;
              foaf:name "Carol Martinez" ;
              schema:affiliation bib:oxford .

bib:dave      a foaf:Person ;
              foaf:name "Dave Johnson" ;
              schema:affiliation bib:eth .

bib:eve       a foaf:Person ;
              foaf:name "Eve Williams" ;
              schema:affiliation bib:mit .

# --- Topics ---
bib:kg        a skos:Concept ;
              skos:prefLabel "Knowledge Graphs" .

bib:sparql    a skos:Concept ;
              skos:prefLabel "SPARQL" ;
              skos:broader bib:kg .

bib:rdf       a skos:Concept ;
              skos:prefLabel "RDF" ;
              skos:broader bib:kg .

bib:ml        a skos:Concept ;
              skos:prefLabel "Machine Learning" .

bib:nlp       a skos:Concept ;
              skos:prefLabel "Natural Language Processing" ;
              skos:broader bib:ml .

bib:rag       a skos:Concept ;
              skos:prefLabel "Retrieval-Augmented Generation" ;
              skos:broader bib:ml ;
              skos:related bib:kg .

# --- Papers ---
bib:paper1    a schema:ScholarlyArticle ;
              dc:title "Knowledge Graphs in Practice" ;
              dc:creator bib:alice ;
              dc:creator bib:bob ;
              dcterms:issued "2024-01-15"^^xsd:date ;
              schema:about bib:kg ;
              schema:about bib:rdf ;
              dcterms:abstract "A comprehensive survey of knowledge graph adoption in industry." .

bib:paper2    a schema:ScholarlyArticle ;
              dc:title "Efficient SPARQL Query Processing" ;
              dc:creator bib:bob ;
              dc:creator bib:carol ;
              dcterms:issued "2024-03-22"^^xsd:date ;
              schema:about bib:sparql ;
              dcterms:abstract "Novel optimization techniques for SPARQL query engines." .

bib:paper3    a schema:ScholarlyArticle ;
              dc:title "Graph-Enhanced Retrieval for LLMs" ;
              dc:creator bib:alice ;
              dc:creator bib:dave ;
              dcterms:issued "2024-06-10"^^xsd:date ;
              schema:about bib:rag ;
              schema:about bib:kg ;
              dcterms:abstract "Combining knowledge graphs with vector retrieval for better LLM responses." .

bib:paper4    a schema:ScholarlyArticle ;
              dc:title "SHACL Validation at Scale" ;
              dc:creator bib:carol ;
              dc:creator bib:eve ;
              dcterms:issued "2024-09-05"^^xsd:date ;
              schema:about bib:kg ;
              dcterms:abstract "Scalable approaches to RDF data quality validation." .

bib:paper5    a schema:ScholarlyArticle ;
              dc:title "Datalog Reasoning over RDF" ;
              dc:creator bib:dave ;
              dcterms:issued "2023-11-20"^^xsd:date ;
              schema:about bib:kg ;
              dcterms:abstract "Applying Datalog inference to large-scale RDF datasets." .

bib:paper6    a schema:ScholarlyArticle ;
              dc:title "Neural Entity Resolution" ;
              dc:creator bib:eve ;
              dc:creator bib:alice ;
              dcterms:issued "2024-12-01"^^xsd:date ;
              schema:about bib:ml ;
              schema:about bib:kg ;
              dcterms:abstract "Using neural networks for entity matching in knowledge graphs." .

# --- Citations ---
bib:paper2    dcterms:references bib:paper1 .
bib:paper3    dcterms:references bib:paper1 .
bib:paper3    dcterms:references bib:paper2 .
bib:paper4    dcterms:references bib:paper1 .
bib:paper4    dcterms:references bib:paper2 .
bib:paper6    dcterms:references bib:paper3 .
bib:paper6    dcterms:references bib:paper1 .

# --- Co-authorship (explicit)
bib:alice     foaf:knows bib:bob .
bib:alice     foaf:knows bib:dave .
bib:alice     foaf:knows bib:eve .
bib:bob       foaf:knows bib:alice .
bib:bob       foaf:knows bib:carol .
bib:carol     foaf:knows bib:bob .
bib:carol     foaf:knows bib:eve .
bib:dave      foaf:knows bib:alice .
bib:eve       foaf:knows bib:alice .
bib:eve       foaf:knows bib:carol .
');
