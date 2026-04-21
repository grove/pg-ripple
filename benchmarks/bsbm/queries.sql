-- BSBM (Berlin SPARQL Benchmark) Explore Queries for pg_ripple.
-- Scale: 1M triples (product dataset).
-- These 12 queries are the standard BSBM explore query mix.
--
-- Run via: cargo test --test bsbm_suite
-- Or directly against the database:
--   psql -f benchmarks/bsbm/queries.sql

-- Q1: Find products matching a set of feature requirements.
-- Finds products with specific product features within a price range.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
  PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
  SELECT DISTINCT ?product ?label WHERE {
    ?product a bsbm:Product .
    ?product rdfs:label ?label .
    ?product bsbm:productFeature ?feature1 .
    ?product bsbm:productPropertyNumeric1 ?p1 .
    FILTER (?p1 > 10)
  } ORDER BY ?label LIMIT 10
$SPARQL$);

-- Q2: Retrieve basic information about a specific product.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
  SELECT ?label ?comment ?producer ?productFeature ?propertyTextual1 WHERE {
    <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/dataFromProducer1/Product1>
      rdfs:label ?label ;
      rdfs:comment ?comment ;
      bsbm:producer ?producer ;
      bsbm:productFeature ?productFeature ;
      bsbm:productPropertyTextual1 ?propertyTextual1 .
  }
$SPARQL$);

-- Q3: Find products that satisfy a more complex set of constraints.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
  SELECT ?product ?label WHERE {
    ?product a bsbm:Product .
    ?product rdfs:label ?label .
    ?product bsbm:productFeature ?feature1 .
    ?product bsbm:productFeature ?feature2 .
    ?product bsbm:productPropertyNumeric1 ?p1 .
    ?product bsbm:productPropertyNumeric3 ?p3 .
    FILTER (?p1 > 10 && ?p3 < 100)
  } ORDER BY ?label LIMIT 10
$SPARQL$);

-- Q4: Find products matching two different sets of constraints.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
  SELECT DISTINCT ?product ?label WHERE {
    {
      ?product a bsbm:Product .
      ?product rdfs:label ?label .
      ?product bsbm:productFeature ?feature1 .
      ?product bsbm:productPropertyNumeric1 ?p1 .
      FILTER (?p1 > 50)
    } UNION {
      ?product a bsbm:Product .
      ?product rdfs:label ?label .
      ?product bsbm:productFeature ?feature2 .
      ?product bsbm:productPropertyNumeric2 ?p2 .
      FILTER (?p2 > 50)
    }
  } ORDER BY ?label LIMIT 10
$SPARQL$);

-- Q5: Find products that are similar to a given product.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
  SELECT DISTINCT ?product ?productLabel WHERE {
    ?product a bsbm:Product .
    ?product rdfs:label ?productLabel .
    ?product bsbm:productFeature ?feature1 .
    ?product bsbm:productFeature ?feature2 .
    ?product bsbm:productPropertyNumeric1 ?p1 .
    FILTER (?product != <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/dataFromProducer1/Product1>)
    FILTER (?p1 > 5)
  } ORDER BY ?productLabel LIMIT 5
$SPARQL$);

-- Q6: Find products meeting a text search condition.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
  SELECT ?product ?label ?comment WHERE {
    ?product a bsbm:Product .
    ?product rdfs:label ?label .
    ?product rdfs:comment ?comment .
    FILTER regex(?label, "^Product", "i")
  } LIMIT 10
$SPARQL$);

-- Q7: Statistical query: count products per product type.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
  SELECT ?productType (COUNT(?product) AS ?count) WHERE {
    ?product a ?productType .
    ?productType a bsbm:ProductType .
  } GROUP BY ?productType ORDER BY DESC(?count) LIMIT 10
$SPARQL$);

-- Q8: Find products with reviews.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
  PREFIX rev: <http://purl.org/stuff/rev#>
  SELECT DISTINCT ?product ?productLabel ?offer ?price ?review ?reviewDate WHERE {
    ?product rdfs:label ?productLabel .
    ?offer bsbm:product ?product .
    ?offer bsbm:price ?price .
    ?review bsbm:reviewFor ?product .
    ?review dc:date ?reviewDate .
    FILTER (?price < 100)
  } ORDER BY ?reviewDate DESC LIMIT 10
$SPARQL$);

-- Q9: Get all information about a specific review.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX rev: <http://purl.org/stuff/rev#>
  DESCRIBE <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/dataFromRatingSite1/Review1>
$SPARQL$);

-- Q10: Get offers for a specific product.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
  SELECT DISTINCT ?offer ?price WHERE {
    ?offer bsbm:product
      <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/dataFromProducer1/Product1> .
    ?offer bsbm:price ?price .
    ?offer bsbm:vendor ?vendor .
    ?vendor bsbm:country <http://downlode.org/rdf/iso-3166/countries#US> .
  } ORDER BY ?price LIMIT 10
$SPARQL$);

-- Q11: Get all products offered by a specific vendor.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
  SELECT ?product ?productLabel ?price WHERE {
    ?offer bsbm:vendor
      <http://www4.wiwiss.fu-berlin.de-bizer/bsbm/v01/instances/dataFromVendor1/Vendor1> .
    ?offer bsbm:product ?product .
    ?offer bsbm:price ?price .
    ?product rdfs:label ?productLabel .
  } ORDER BY ?price LIMIT 10
$SPARQL$);

-- Q12: Export all data about a specific product as a subgraph.
SELECT pg_ripple.sparql_query($SPARQL$
  PREFIX bsbm: <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/>
  CONSTRUCT {
    ?product ?p ?o .
  } WHERE {
    ?product a bsbm:Product .
    ?product ?p ?o .
    FILTER (?product = <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/dataFromProducer1/Product1>)
  }
$SPARQL$);
