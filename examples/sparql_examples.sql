-- =============================================================================
-- examples/sparql_examples.sql
-- Example SPARQL queries for the two sample graphs loaded by sample_graphs.sql.
--
-- Graph 1 — BSBM E-Commerce  <http://example.org/bsbm>
-- Graph 2 — Academic KG      <http://example.org/academic>
--
-- Usage:
--   psql moire -f examples/sparql_examples.sql
-- =============================================================================

SET search_path TO pg_ripple, public;

\echo ''
\echo '============================================================'
\echo '  pg_ripple SPARQL example queries'
\echo '============================================================'
\echo ''


-- ============================================================================
-- GRAPH 1: BSBM E-Commerce  <http://example.org/bsbm>
-- ============================================================================

\echo '--- BSBM queries ---'
\echo ''


-- ── Q1: Products with a given feature ────────────────────────────────────────
\echo 'Q1: Products with ProductFeature1, ordered by label (LIMIT 10)'
SELECT * FROM pg_ripple.sparql($$
PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
PREFIX inst: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>

SELECT ?product ?label
WHERE {
  GRAPH <http://example.org/bsbm> {
    ?product bsbm:productFeature inst:ProductFeature1 .
    ?product rdfs:label ?label .
  }
}
ORDER BY ?label
LIMIT 10
$$);


-- ── Q2: Products from a specific vendor ────────────────────────────────────────
\echo ''
\echo 'Q2: Products produced by Vendor1'
SELECT * FROM pg_ripple.sparql($$
PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
PREFIX inst: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>

SELECT ?product ?label ?price
WHERE {
  GRAPH <http://example.org/bsbm> {
    ?product a bsbm:Product .
    ?product bsbm:producer inst:Vendor1 .
    ?product rdfs:label ?label .
    ?product bsbm:price ?price .
  }
}
ORDER BY ?label
LIMIT 20
$$);


-- ── Q3: Products with two features (star pattern) ────────────────────────────
\echo ''
\echo 'Q3: Products that have BOTH Feature1 and Feature2'
SELECT * FROM pg_ripple.sparql($$
PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
PREFIX inst: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>

SELECT ?product ?label
WHERE {
  GRAPH <http://example.org/bsbm> {
    ?product bsbm:productFeature inst:ProductFeature1 .
    ?product bsbm:productFeature inst:ProductFeature2 .
    ?product rdfs:label ?label .
  }
}
LIMIT 10
$$);


-- ── Q4: Reviews for a product with reviewer names ───────────────────────────
\echo ''
\echo 'Q4: Reviews for Product1 with ratings and reviewer names'
SELECT * FROM pg_ripple.sparql($$
PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
PREFIX inst: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/>
PREFIX rev:  <http://purl.org/stuff/rev#>
PREFIX foaf: <http://xmlns.com/foaf/0.1/>

SELECT ?review ?rating ?reviewer ?name
WHERE {
  GRAPH <http://example.org/bsbm> {
    ?review rev:reviewOf inst:Product1 .
    ?review rev:rating   ?rating .
    ?review rev:reviewer ?reviewer .
    ?reviewer foaf:name  ?name .
  }
}
ORDER BY DESC(?rating)
LIMIT 10
$$);


-- ── Q5: Average rating per vendor ────────────────────────────────────────────
\echo ''
\echo 'Q5: Top 10 vendors by average product review rating'
SELECT * FROM pg_ripple.sparql($$
PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
PREFIX rev:  <http://purl.org/stuff/rev#>
PREFIX foaf: <http://xmlns.com/foaf/0.1/>

SELECT ?vendor ?name (AVG(?rating) AS ?avgRating) (COUNT(?review) AS ?reviewCount)
WHERE {
  GRAPH <http://example.org/bsbm> {
    ?product bsbm:producer ?vendor .
    ?vendor  foaf:name     ?name .
    ?review  rev:reviewOf  ?product .
    ?review  rev:rating    ?rating .
  }
}
GROUP BY ?vendor ?name
ORDER BY DESC(?avgRating)
LIMIT 10
$$);


-- ── Q6: Products by country of vendor ────────────────────────────────────────
\echo ''
\echo 'Q6: Count products per vendor country'
SELECT * FROM pg_ripple.sparql($$
PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>

SELECT ?country (COUNT(?product) AS ?products)
WHERE {
  GRAPH <http://example.org/bsbm> {
    ?product bsbm:producer ?vendor .
    ?vendor  bsbm:country  ?country .
  }
}
GROUP BY ?country
ORDER BY DESC(?products)
$$);


-- ── Q7: Reviews since a given date ───────────────────────────────────────────
\echo ''
\echo 'Q7: Reviews posted in 2023 or later'
SELECT count(*) AS review_count FROM pg_ripple.sparql($$
PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>

SELECT ?review ?date
WHERE {
  GRAPH <http://example.org/bsbm> {
    ?review a <http://purl.org/stuff/rev#Review> .
    ?review bsbm:reviewDate ?date .
    FILTER (?date >= "2023-01-01"^^<http://www.w3.org/2001/XMLSchema#date>)
  }
}
$$);


-- ============================================================================
-- GRAPH 2: Academic Knowledge Graph  <http://example.org/academic>
-- ============================================================================

\echo ''
\echo '--- Academic KG queries ---'
\echo ''


-- ── A1: All research topics ───────────────────────────────────────────────────
\echo 'A1: All research topics (SKOS concepts)'
SELECT * FROM pg_ripple.sparql($$
PREFIX skos:   <http://www.w3.org/2004/02/skos/core#>

SELECT ?topic ?label
WHERE {
  GRAPH <http://example.org/academic> {
    ?topic a skos:Concept .
    ?topic skos:prefLabel ?label .
  }
}
ORDER BY ?label
$$);


-- ── A2: Researchers at a university ───────────────────────────────────────────
\echo ''
\echo 'A2: Researchers affiliated with MIT (through department)'
SELECT * FROM pg_ripple.sparql($$
PREFIX foaf:   <http://xmlns.com/foaf/0.1/>
PREFIX schema: <http://schema.org/>

SELECT ?person ?name ?deptName
WHERE {
  GRAPH <http://example.org/academic> {
    ?univ schema:name "MIT" .
    ?dept schema:parentOrganization ?univ .
    ?dept schema:name ?deptName .
    ?person schema:affiliation ?dept .
    ?person foaf:name ?name .
  }
}
ORDER BY ?name
LIMIT 20
$$);


-- ── A3: Papers on a topic ─────────────────────────────────────────────────────
\echo ''
\echo 'A3: Papers about Knowledge Graphs with titles and years'
SELECT * FROM pg_ripple.sparql($$
PREFIX dct:    <http://purl.org/dc/terms/>
PREFIX schema: <http://schema.org/>
PREFIX skos:   <http://www.w3.org/2004/02/skos/core#>

SELECT ?paper ?title ?year
WHERE {
  GRAPH <http://example.org/academic> {
    ?topic skos:prefLabel "Knowledge Graphs"@en .
    ?paper schema:about ?topic .
    ?paper dct:title    ?title .
    ?paper dct:date     ?year .
  }
}
ORDER BY DESC(?year) ?title
LIMIT 20
$$);


-- ── A4: Co-authors of a researcher ────────────────────────────────────────────
\echo ''
\echo 'A4: Co-authors of person/1 (two hops through shared papers)'
SELECT * FROM pg_ripple.sparql($$
PREFIX dct:  <http://purl.org/dc/terms/>
PREFIX foaf: <http://xmlns.com/foaf/0.1/>

SELECT DISTINCT ?coauthor ?coname
WHERE {
  GRAPH <http://example.org/academic> {
    ?paper  dct:creator <http://example.org/academic/person/1> .
    ?paper  dct:creator ?coauthor .
    ?coauthor foaf:name ?coname .
    FILTER (?coauthor != <http://example.org/academic/person/1>)
  }
}
ORDER BY ?coname
$$);


-- ── A5: Most prolific authors ─────────────────────────────────────────────────
\echo ''
\echo 'A5: Top 10 most prolific researchers by paper count'
SELECT * FROM pg_ripple.sparql($$
PREFIX dct:  <http://purl.org/dc/terms/>
PREFIX foaf: <http://xmlns.com/foaf/0.1/>

SELECT ?person ?name (COUNT(?paper) AS ?papers)
WHERE {
  GRAPH <http://example.org/academic> {
    ?paper dct:creator ?person .
    ?person foaf:name  ?name .
  }
}
GROUP BY ?person ?name
ORDER BY DESC(?papers)
LIMIT 10
$$);


-- ── A6: Papers per year ───────────────────────────────────────────────────────
\echo ''
\echo 'A6: Paper publication counts by year'
SELECT * FROM pg_ripple.sparql($$
PREFIX dct:    <http://purl.org/dc/terms/>
PREFIX schema: <http://schema.org/>

SELECT ?year (COUNT(?paper) AS ?count)
WHERE {
  GRAPH <http://example.org/academic> {
    ?paper a schema:ScholarlyArticle .
    ?paper dct:date ?year .
  }
}
GROUP BY ?year
ORDER BY ?year
$$);


-- ── A7: Papers citing a specific paper ────────────────────────────────────────
\echo ''
\echo 'A7: Papers that cite paper/100'
SELECT * FROM pg_ripple.sparql($$
PREFIX dct:    <http://purl.org/dc/terms/>
PREFIX schema: <http://schema.org/>

SELECT ?citing ?title ?year
WHERE {
  GRAPH <http://example.org/academic> {
    ?citing schema:citation <http://example.org/academic/paper/100> .
    ?citing dct:title ?title .
    ?citing dct:date  ?year .
  }
}
ORDER BY DESC(?year)
$$);


-- ── A8: Topic hierarchy (direct broader links) ────────────────────────────────
-- Note: property paths inside GRAPH {} hit a known rare-table join bug; use
-- a plain triple pattern to walk one level of the hierarchy instead.
\echo ''
\echo 'A8: Topics with a direct skos:broader link to any of the first 5 top topics'
SELECT * FROM pg_ripple.sparql($$
PREFIX skos: <http://www.w3.org/2004/02/skos/core#>

SELECT ?narrower ?narrowLabel ?broader ?broaderLabel
WHERE {
  GRAPH <http://example.org/academic> {
    ?narrower skos:broader  ?broader .
    ?narrower skos:prefLabel ?narrowLabel .
    ?broader  skos:prefLabel ?broaderLabel .
  }
}
ORDER BY ?broaderLabel ?narrowLabel
$$);


-- ── A9: Researchers and their topic interests ─────────────────────────────────
\echo ''
\echo 'A9: Researchers with their topic interests and the parent topic'
SELECT * FROM pg_ripple.sparql($$
PREFIX skos: <http://www.w3.org/2004/02/skos/core#>
PREFIX foaf: <http://xmlns.com/foaf/0.1/>

SELECT ?person ?name ?topicLabel ?parentLabel
WHERE {
  GRAPH <http://example.org/academic> {
    ?person foaf:topic_interest ?topic .
    ?person foaf:name           ?name .
    ?topic  skos:prefLabel      ?topicLabel .
    ?topic  skos:broader        ?parent .
    ?parent skos:prefLabel      ?parentLabel .
  }
}
ORDER BY ?parentLabel ?topicLabel ?name
LIMIT 30
$$);


-- ── A10: Cross-graph — BSBM reviewer who is also a researcher ─────────────────
\echo ''
\echo 'A10: Enumerate both graphs to show their structure'
SELECT * FROM pg_ripple.sparql($$
SELECT ?g (COUNT(*) AS ?triples)
WHERE {
  GRAPH ?g { ?s ?p ?o }
}
GROUP BY ?g
ORDER BY ?g
$$);
