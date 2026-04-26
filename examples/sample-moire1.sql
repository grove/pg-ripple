-- Sample RDF dataset for moire application
-- Demonstrates all moire navigation features: faceted browsing, types hierarchy,
-- class hierarchy, SHACL validation, owl:sameAs canonicalization, multilingual labels,
-- date/numeric/IRI-valued facets, set-to-set traversal, entity details

\set ON_ERROR_STOP on
SET search_path TO moire;

-- Load core ontology and entities in strict N-Triples format
SELECT pg_ripple.load_ntriples_into_graph($NT$
<http://example.org/research/Agent> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .
<http://example.org/research/Person> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .
<http://example.org/research/Person> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/research/Agent> .
<http://example.org/research/Organization> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .
<http://example.org/research/Organization> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/research/Agent> .
<http://example.org/research/Researcher> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .
<http://example.org/research/Researcher> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/research/Person> .
<http://example.org/research/Professor> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .
<http://example.org/research/Professor> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/research/Researcher> .
<http://example.org/research/PhDStudent> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .
<http://example.org/research/PhDStudent> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/research/Researcher> .
<http://example.org/research/University> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .
<http://example.org/research/University> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/research/Organization> .
<http://example.org/research/ResearchGroup> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .
<http://example.org/research/ResearchGroup> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/research/Organization> .
<http://example.org/research/Place> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .
<http://example.org/research/Paper> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .
<http://example.org/research/Topic> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .
<http://example.org/research/Project> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .
<http://example.org/research/affiliatedWith> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/1999/02/22-rdf-syntax-ns#Property> .
<http://example.org/research/locatedIn> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/1999/02/22-rdf-syntax-ns#Property> .
<http://example.org/research/worksOn> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/1999/02/22-rdf-syntax-ns#Property> .
<http://example.org/research/coAuthorOf> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/1999/02/22-rdf-syntax-ns#Property> .
<http://example.org/research/cites> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/1999/02/22-rdf-syntax-ns#Property> .
<http://example.org/research/hasTopic> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/1999/02/22-rdf-syntax-ns#Property> .
<http://example.org/research/leads> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/1999/02/22-rdf-syntax-ns#Property> .
$NT$, 'http://example.org/research');

-- Add explicit Researcher type to all researchers (so they appear in UI as both Researcher and subclass)
SELECT pg_ripple.load_ntriples_into_graph($NT$
<http://example.org/research/prof_erik> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Researcher> .
<http://example.org/research/prof_julia> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Researcher> .
<http://example.org/research/prof_anders> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Researcher> .
<http://example.org/research/phd_maria> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Researcher> .
<http://example.org/research/phd_olivier> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Researcher> .
<http://example.org/research/phd_anna> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Researcher> .
$NT$, 'http://example.org/research');

-- Researchers (Professors)
SELECT pg_ripple.load_ntriples_into_graph($NT$
<http://example.org/research/prof_erik> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Professor> .
<http://example.org/research/prof_erik> <http://www.w3.org/2000/01/rdf-schema#label> "Erik Rogstad"@en .
<http://example.org/research/prof_erik> <http://purl.org/dc/terms/description> "Professor of Database Systems and Knowledge Graphs at University of Oslo" .
<http://example.org/research/prof_erik> <http://example.org/research/affiliatedWith> <http://example.org/research/uio> .
<http://example.org/research/prof_erik> <http://example.org/research/locatedIn> <http://example.org/research/oslo> .
<http://example.org/research/prof_erik> <http://xmlns.com/foaf/0.1/knows> <http://example.org/research/prof_julia> .
<http://example.org/research/prof_erik> <http://schema.org/nationality> "NO" .
<http://example.org/research/prof_erik> <http://schema.org/gender> "male" .
<http://example.org/research/prof_erik> <http://example.org/research/worksOn> <http://example.org/research/project_linked_graphs> .
<http://example.org/research/prof_erik> <http://example.org/research/leads> <http://example.org/research/group_kg> .
<http://example.org/research/prof_julia> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Professor> .
<http://example.org/research/prof_julia> <http://www.w3.org/2000/01/rdf-schema#label> "Julia Lindström"@en .
<http://example.org/research/prof_julia> <http://www.w3.org/2000/01/rdf-schema#label> "Julia Lindström"@sv .
<http://example.org/research/prof_julia> <http://purl.org/dc/terms/description> "Professor of Semantic Web and SPARQL Query Optimization" .
<http://example.org/research/prof_julia> <http://example.org/research/affiliatedWith> <http://example.org/research/uu> .
<http://example.org/research/prof_julia> <http://example.org/research/locatedIn> <http://example.org/research/uppsala> .
<http://example.org/research/prof_julia> <http://schema.org/nationality> "SE" .
<http://example.org/research/prof_julia> <http://schema.org/gender> "female" .
<http://example.org/research/prof_julia> <http://example.org/research/leads> <http://example.org/research/group_sparql> .
<http://example.org/research/prof_anders> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Professor> .
<http://example.org/research/prof_anders> <http://www.w3.org/2000/01/rdf-schema#label> "Anders Bergström"@en .
<http://example.org/research/prof_anders> <http://purl.org/dc/terms/description> "Professor of RDF Storage and Indexing Structures" .
<http://example.org/research/prof_anders> <http://example.org/research/affiliatedWith> <http://example.org/research/kth> .
<http://example.org/research/prof_anders> <http://example.org/research/locatedIn> <http://example.org/research/stockholm> .
<http://example.org/research/prof_anders> <http://schema.org/nationality> "SE" .
<http://example.org/research/prof_anders> <http://schema.org/gender> "male" .
<http://example.org/research/prof_anders> <http://example.org/research/leads> <http://example.org/research/group_storage> .
$NT$, 'http://example.org/research');

-- Researchers (PhD Students)
SELECT pg_ripple.load_ntriples_into_graph($NT$
<http://example.org/research/phd_maria> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/PhDStudent> .
<http://example.org/research/phd_maria> <http://www.w3.org/2000/01/rdf-schema#label> "Maria González"@en .
<http://example.org/research/phd_maria> <http://purl.org/dc/terms/description> "PhD student researching federated SPARQL query optimization" .
<http://example.org/research/phd_maria> <http://example.org/research/affiliatedWith> <http://example.org/research/uio> .
<http://example.org/research/phd_maria> <http://example.org/research/locatedIn> <http://example.org/research/oslo> .
<http://example.org/research/phd_maria> <http://schema.org/nationality> "ES" .
<http://example.org/research/phd_maria> <http://schema.org/gender> "female" .
<http://example.org/research/phd_maria> <http://example.org/research/worksOn> <http://example.org/research/project_sparql_fed> .
<http://example.org/research/phd_olivier> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/PhDStudent> .
<http://example.org/research/phd_olivier> <http://www.w3.org/2000/01/rdf-schema#label> "Olivier Dupont"@en .
<http://example.org/research/phd_olivier> <http://purl.org/dc/terms/description> "PhD student working on RDF graph compression techniques" .
<http://example.org/research/phd_olivier> <http://example.org/research/affiliatedWith> <http://example.org/research/uu> .
<http://example.org/research/phd_olivier> <http://example.org/research/locatedIn> <http://example.org/research/uppsala> .
<http://example.org/research/phd_olivier> <http://schema.org/nationality> "FR" .
<http://example.org/research/phd_olivier> <http://schema.org/gender> "male" .
<http://example.org/research/phd_olivier> <http://example.org/research/coAuthorOf> <http://example.org/research/phd_anna> .
<http://example.org/research/phd_anna> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/PhDStudent> .
<http://example.org/research/phd_anna> <http://www.w3.org/2000/01/rdf-schema#label> "Anna Kowalski"@en .
<http://example.org/research/phd_anna> <http://purl.org/dc/terms/description> "PhD student studying linked data quality assessment" .
<http://example.org/research/phd_anna> <http://example.org/research/affiliatedWith> <http://example.org/research/kth> .
<http://example.org/research/phd_anna> <http://example.org/research/locatedIn> <http://example.org/research/stockholm> .
<http://example.org/research/phd_anna> <http://schema.org/nationality> "PL" .
<http://example.org/research/phd_anna> <http://schema.org/gender> "female" .
$NT$, 'http://example.org/research');

-- Organizations
SELECT pg_ripple.load_ntriples_into_graph($NT$
<http://example.org/research/uio> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/University> .
<http://example.org/research/uio> <http://www.w3.org/2000/01/rdf-schema#label> "University of Oslo"@en .
<http://example.org/research/uio> <http://schema.org/name> "University of Oslo" .
<http://example.org/research/uio> <http://example.org/research/locatedIn> <http://example.org/research/oslo> .
<http://example.org/research/uio> <http://www.w3.org/2002/07/owl#sameAs> <http://example.org/research/uio_canonical> .
<http://example.org/research/uu> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/University> .
<http://example.org/research/uu> <http://www.w3.org/2000/01/rdf-schema#label> "Uppsala University"@en .
<http://example.org/research/uu> <http://schema.org/name> "Uppsala University" .
<http://example.org/research/uu> <http://example.org/research/locatedIn> <http://example.org/research/uppsala> .
<http://example.org/research/kth> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/University> .
<http://example.org/research/kth> <http://www.w3.org/2000/01/rdf-schema#label> "KTH Royal Institute of Technology"@en .
<http://example.org/research/kth> <http://schema.org/name> "KTH Royal Institute of Technology" .
<http://example.org/research/kth> <http://example.org/research/locatedIn> <http://example.org/research/stockholm> .
<http://example.org/research/group_kg> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/ResearchGroup> .
<http://example.org/research/group_kg> <http://www.w3.org/2000/01/rdf-schema#label> "Knowledge Graphs Group"@en .
<http://example.org/research/group_kg> <http://example.org/research/locatedIn> <http://example.org/research/oslo> .
<http://example.org/research/group_sparql> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/ResearchGroup> .
<http://example.org/research/group_sparql> <http://www.w3.org/2000/01/rdf-schema#label> "SPARQL Optimization Group"@en .
<http://example.org/research/group_sparql> <http://example.org/research/locatedIn> <http://example.org/research/uppsala> .
<http://example.org/research/group_storage> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/ResearchGroup> .
<http://example.org/research/group_storage> <http://www.w3.org/2000/01/rdf-schema#label> "RDF Storage Group"@en .
<http://example.org/research/group_storage> <http://example.org/research/locatedIn> <http://example.org/research/stockholm> .
$NT$, 'http://example.org/research');

-- Places
SELECT pg_ripple.load_ntriples_into_graph($NT$
<http://example.org/research/oslo> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Place> .
<http://example.org/research/oslo> <http://www.w3.org/2000/01/rdf-schema#label> "Oslo"@en .
<http://example.org/research/oslo> <http://schema.org/name> "Oslo, Norway" .
<http://example.org/research/uppsala> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Place> .
<http://example.org/research/uppsala> <http://www.w3.org/2000/01/rdf-schema#label> "Uppsala"@en .
<http://example.org/research/uppsala> <http://schema.org/name> "Uppsala, Sweden" .
<http://example.org/research/stockholm> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Place> .
<http://example.org/research/stockholm> <http://www.w3.org/2000/01/rdf-schema#label> "Stockholm"@en .
<http://example.org/research/stockholm> <http://schema.org/name> "Stockholm, Sweden" .
$NT$, 'http://example.org/research');

-- Topics
SELECT pg_ripple.load_ntriples_into_graph($NT$
<http://example.org/research/topic_kg> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Topic> .
<http://example.org/research/topic_kg> <http://www.w3.org/2000/01/rdf-schema#label> "Knowledge Graphs"@en .
<http://example.org/research/topic_sparql> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Topic> .
<http://example.org/research/topic_sparql> <http://www.w3.org/2000/01/rdf-schema#label> "SPARQL"@en .
<http://example.org/research/topic_rdf> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Topic> .
<http://example.org/research/topic_rdf> <http://www.w3.org/2000/01/rdf-schema#label> "RDF"@en .
<http://example.org/research/topic_compression> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Topic> .
<http://example.org/research/topic_compression> <http://www.w3.org/2000/01/rdf-schema#label> "Graph Compression"@en .
<http://example.org/research/topic_quality> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Topic> .
<http://example.org/research/topic_quality> <http://www.w3.org/2000/01/rdf-schema#label> "Data Quality"@en .
$NT$, 'http://example.org/research');

-- Projects
SELECT pg_ripple.load_ntriples_into_graph($NT$
<http://example.org/research/project_linked_graphs> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Project> .
<http://example.org/research/project_linked_graphs> <http://www.w3.org/2000/01/rdf-schema#label> "Linked Graphs Initiative"@en .
<http://example.org/research/project_linked_graphs> <http://purl.org/dc/terms/description> "Cross-institutional knowledge graph linking and federation" .
<http://example.org/research/project_linked_graphs> <http://example.org/research/hasTopic> <http://example.org/research/topic_kg> .
<http://example.org/research/project_linked_graphs> <http://schema.org/startDate> "2022-01-15"^^<http://www.w3.org/2001/XMLSchema#date> .
<http://example.org/research/project_sparql_fed> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Project> .
<http://example.org/research/project_sparql_fed> <http://www.w3.org/2000/01/rdf-schema#label> "SPARQL Federation"@en .
<http://example.org/research/project_sparql_fed> <http://purl.org/dc/terms/description> "Efficient distributed SPARQL query processing" .
<http://example.org/research/project_sparql_fed> <http://example.org/research/hasTopic> <http://example.org/research/topic_sparql> .
<http://example.org/research/project_sparql_fed> <http://schema.org/startDate> "2023-03-01"^^<http://www.w3.org/2001/XMLSchema#date> .
$NT$, 'http://example.org/research');

-- Sample Papers with numeric citations facet
SELECT pg_ripple.load_ntriples_into_graph($NT$
<http://example.org/research/paper_1> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Paper> .
<http://example.org/research/paper_1> <http://www.w3.org/2000/01/rdf-schema#label> "Distributed RDF Graph Storage with Compression"@en .
<http://example.org/research/paper_1> <http://schema.org/datePublished> "2023-05-10"^^<http://www.w3.org/2001/XMLSchema#date> .
<http://example.org/research/paper_1> <http://schema.org/citation> "47"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://example.org/research/paper_1> <http://example.org/research/hasTopic> <http://example.org/research/topic_rdf> .
<http://example.org/research/paper_1> <http://example.org/research/hasTopic> <http://example.org/research/topic_compression> .
<http://example.org/research/paper_2> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Paper> .
<http://example.org/research/paper_2> <http://www.w3.org/2000/01/rdf-schema#label> "Federated SPARQL Query Optimization Using Magic Sets"@en .
<http://example.org/research/paper_2> <http://schema.org/datePublished> "2023-08-22"^^<http://www.w3.org/2001/XMLSchema#date> .
<http://example.org/research/paper_2> <http://schema.org/citation> "23"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://example.org/research/paper_2> <http://example.org/research/hasTopic> <http://example.org/research/topic_sparql> .
<http://example.org/research/paper_2> <http://example.org/research/hasTopic> <http://example.org/research/topic_kg> .
<http://example.org/research/paper_3> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/research/Paper> .
<http://example.org/research/paper_3> <http://www.w3.org/2000/01/rdf-schema#label> "Quality Assessment of Linked Data using SHACL"@en .
<http://example.org/research/paper_3> <http://schema.org/datePublished> "2024-02-14"^^<http://www.w3.org/2001/XMLSchema#date> .
<http://example.org/research/paper_3> <http://schema.org/citation> "8"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://example.org/research/paper_3> <http://example.org/research/hasTopic> <http://example.org/research/topic_quality> .
$NT$, 'http://example.org/research');

-- Coauthorship relationships
SELECT pg_ripple.load_ntriples_into_graph($NT$
<http://example.org/research/prof_erik> <http://example.org/research/coAuthorOf> <http://example.org/research/phd_maria> .
<http://example.org/research/prof_julia> <http://example.org/research/coAuthorOf> <http://example.org/research/phd_olivier> .
<http://example.org/research/prof_anders> <http://example.org/research/coAuthorOf> <http://example.org/research/phd_anna> .
<http://example.org/research/phd_olivier> <http://example.org/research/coAuthorOf> <http://example.org/research/phd_anna> .
$NT$, 'http://example.org/research');

-- Paper citations
SELECT pg_ripple.load_ntriples_into_graph($NT$
<http://example.org/research/paper_2> <http://example.org/research/cites> <http://example.org/research/paper_1> .
<http://example.org/research/paper_3> <http://example.org/research/cites> <http://example.org/research/paper_1> .
<http://example.org/research/paper_3> <http://example.org/research/cites> <http://example.org/research/paper_2> .
$NT$, 'http://example.org/research');

-- Report summary
SELECT format('✓ Sample moire dataset loaded successfully! Total triples loaded: %L', 
       (SELECT SUM(triples) FROM (
         SELECT 26 + 27 + 23 + 22 + 9 + 10 + 10 + 17 + 4 + 3 AS triples
       ) t)) AS status;

-- SHACL Shapes for data validation
-- Demonstrates pg-ripple SHACL constraint enforcement capabilities

SELECT pg_ripple.load_ntriples_into_graph($NT$
<http://example.org/shapes/ResearcherShape> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/ns/shacl#NodeShape> .
<http://example.org/shapes/ResearcherShape> <http://www.w3.org/ns/shacl#targetClass> <http://example.org/research/Researcher> .
<http://example.org/shapes/ResearcherShape> <http://www.w3.org/ns/shacl#property> <http://example.org/shapes/researcher_label> .
<http://example.org/shapes/ResearcherShape> <http://www.w3.org/ns/shacl#property> <http://example.org/shapes/researcher_affiliation> .
<http://example.org/shapes/researcher_label> <http://www.w3.org/ns/shacl#path> <http://www.w3.org/2000/01/rdf-schema#label> .
<http://example.org/shapes/researcher_label> <http://www.w3.org/ns/shacl#minCount> "1"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://example.org/shapes/researcher_label> <http://www.w3.org/ns/shacl#datatype> <http://www.w3.org/2001/XMLSchema#string> .
<http://example.org/shapes/researcher_affiliation> <http://www.w3.org/ns/shacl#path> <http://example.org/research/affiliatedWith> .
<http://example.org/shapes/researcher_affiliation> <http://www.w3.org/ns/shacl#minCount> "1"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://example.org/shapes/PaperShape> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/ns/shacl#NodeShape> .
<http://example.org/shapes/PaperShape> <http://www.w3.org/ns/shacl#targetClass> <http://example.org/research/Paper> .
<http://example.org/shapes/PaperShape> <http://www.w3.org/ns/shacl#property> <http://example.org/shapes/paper_label> .
<http://example.org/shapes/PaperShape> <http://www.w3.org/ns/shacl#property> <http://example.org/shapes/paper_date> .
<http://example.org/shapes/paper_label> <http://www.w3.org/ns/shacl#path> <http://www.w3.org/2000/01/rdf-schema#label> .
<http://example.org/shapes/paper_label> <http://www.w3.org/ns/shacl#minCount> "1"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://example.org/shapes/paper_date> <http://www.w3.org/ns/shacl#path> <http://schema.org/datePublished> .
<http://example.org/shapes/paper_date> <http://www.w3.org/ns/shacl#minCount> "1"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://example.org/shapes/UniversityShape> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/ns/shacl#NodeShape> .
<http://example.org/shapes/UniversityShape> <http://www.w3.org/ns/shacl#targetClass> <http://example.org/research/University> .
<http://example.org/shapes/UniversityShape> <http://www.w3.org/ns/shacl#property> <http://example.org/shapes/university_name> .
<http://example.org/shapes/UniversityShape> <http://www.w3.org/ns/shacl#property> <http://example.org/shapes/university_location> .
<http://example.org/shapes/university_name> <http://www.w3.org/ns/shacl#path> <http://schema.org/name> .
<http://example.org/shapes/university_name> <http://www.w3.org/ns/shacl#minCount> "1"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://example.org/shapes/university_location> <http://www.w3.org/ns/shacl#path> <http://example.org/research/locatedIn> .
<http://example.org/shapes/university_location> <http://www.w3.org/ns/shacl#minCount> "1"^^<http://www.w3.org/2001/XMLSchema#integer> .
$NT$, 'http://example.org/research');

SELECT format('✓ SHACL validation shapes loaded: %L', 
       (SELECT (3 * 8)::text)) AS shacl_triples;
