-- bsbm_queries.sql — Berlin SPARQL Benchmark (BSBM) query mix for pg_ripple.
--
-- BSBM query types Q1–Q12 translated to SPARQL and executed via
-- pg_ripple.sparql().  Load bsbm_load.sql first.
--
-- Usage:
--   psql -f benchmarks/bsbm/bsbm_queries.sql
--
-- Reference queries from:
--   http://wifo5-03.informatik.uni-mannheim.de/bizer/berlinsparqlbenchmark/

SET search_path TO pg_ripple, public;

\echo 'BSBM Query Mix — pg_ripple v0.6.0 HTAP'
\echo '======================================='

-- ── Q1: Find products matching a given feature ───────────────────────────────
-- Find up to 10 products that have a specific product feature, ordered by label.
\echo 'Q1: Find products by feature'
\timing on

SELECT count(*) AS q1_result_count
FROM pg_ripple.sparql($$
    SELECT ?product ?label
    WHERE {
        ?product <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/productFeature>
                 <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature1> .
        ?product <http://www.w3.org/2000/01/rdf-schema#label> ?label .
    }
    ORDER BY ?label
    LIMIT 10
$$);

\timing off

-- ── Q2: Find products with a feature in a price range ────────────────────────
\echo 'Q2: Products by feature and price range'
\timing on

SELECT count(*) AS q2_result_count
FROM pg_ripple.sparql($$
    SELECT ?product ?label ?price
    WHERE {
        ?product <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>
                 <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/Product> .
        ?product <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/productFeature>
                 <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature2> .
        ?product <http://www.w3.org/2000/01/rdf-schema#label> ?label .
        ?product <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/price> ?price .
    }
    LIMIT 10
$$);

\timing off

-- ── Q3: Find products with multiple features ──────────────────────────────────
-- Products that have both ProductFeature1 and ProductFeature2.
\echo 'Q3: Products with multiple features (star pattern)'
\timing on

SELECT count(*) AS q3_result_count
FROM pg_ripple.sparql($$
    SELECT ?product ?label
    WHERE {
        ?product <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/productFeature>
                 <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature1> .
        ?product <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/productFeature>
                 <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature2> .
        ?product <http://www.w3.org/2000/01/rdf-schema#label> ?label .
    }
    LIMIT 10
$$);

\timing off

-- ── Q4: Find reviews for a product ────────────────────────────────────────────
-- Find up to 8 reviews for Product1, with reviewer name and rating.
\echo 'Q4: Reviews for a product'
\timing on

SELECT count(*) AS q4_result_count
FROM pg_ripple.sparql($$
    SELECT ?review ?reviewer ?rating
    WHERE {
        ?review <http://purl.org/stuff/rev#reviewOf>
                <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product1> .
        ?review <http://purl.org/stuff/rev#reviewer> ?reviewer .
        ?review <http://purl.org/stuff/rev#rating> ?rating .
    }
    ORDER BY ?rating
    LIMIT 8
$$);

\timing off

-- ── Q5: Find products from same vendor as a given product ────────────────────
\echo 'Q5: Products from same vendor (two-hop join)'
\timing on

SELECT count(*) AS q5_result_count
FROM pg_ripple.sparql($$
    SELECT ?product ?label
    WHERE {
        <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product1>
            <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/producer> ?vendor .
        ?product <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/producer> ?vendor .
        ?product <http://www.w3.org/2000/01/rdf-schema#label> ?label .
    }
    LIMIT 10
$$);

\timing off

-- ── Q6: Find reviews by a specific reviewer ──────────────────────────────────
\echo 'Q6: Reviews by a reviewer'
\timing on

SELECT count(*) AS q6_result_count
FROM pg_ripple.sparql($$
    SELECT ?review ?product ?rating
    WHERE {
        ?review <http://purl.org/stuff/rev#reviewer>
                <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Reviewer1> .
        ?review <http://purl.org/stuff/rev#reviewOf> ?product .
        ?review <http://purl.org/stuff/rev#rating> ?rating .
    }
$$);

\timing off

-- ── Q7: Find all reviews for products from a given vendor ─────────────────────
\echo 'Q7: Reviews for vendor products (three-hop join)'
\timing on

SELECT count(*) AS q7_result_count
FROM pg_ripple.sparql($$
    SELECT ?review ?product ?rating
    WHERE {
        ?product <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/producer>
                 <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Vendor1> .
        ?review <http://purl.org/stuff/rev#reviewOf> ?product .
        ?review <http://purl.org/stuff/rev#rating> ?rating .
    }
    LIMIT 20
$$);

\timing off

-- ── Q8: Find reviewer details by name (OPTIONAL pattern) ────────────────────
\echo 'Q8: Reviewer details with OPTIONAL country'
\timing on

SELECT count(*) AS q8_result_count
FROM pg_ripple.sparql($$
    SELECT ?reviewer ?name ?country
    WHERE {
        ?reviewer <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>
                  <http://xmlns.com/foaf/0.1/Person> .
        ?reviewer <http://xmlns.com/foaf/0.1/name> ?name .
        OPTIONAL {
            ?reviewer <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/country> ?country .
        }
    }
    LIMIT 10
$$);

\timing off

-- ── Q9: COUNT reviews per product ─────────────────────────────────────────────
\echo 'Q9: Aggregate — review count per product (top 10)'
\timing on

SELECT count(*) AS q9_result_count
FROM pg_ripple.sparql($$
    SELECT ?product (COUNT(?review) AS ?reviewCount)
    WHERE {
        ?review <http://purl.org/stuff/rev#reviewOf> ?product .
    }
    GROUP BY ?product
    ORDER BY DESC(?reviewCount)
    LIMIT 10
$$);

\timing off

-- ── Q10: Find products by type (ASK) ──────────────────────────────────────────
\echo 'Q10: ASK — do any products have ProductFeature1?'
\timing on

SELECT pg_ripple.sparql_ask($$
    ASK {
        ?product <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/productFeature>
                 <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature1> .
    }
$$) AS q10_ask_result;

\timing off

-- ── Q11: UNION query — find all typed entities ────────────────────────────────
\echo 'Q11: UNION — products and reviews by type'
\timing on

SELECT count(*) AS q11_result_count
FROM pg_ripple.sparql($$
    SELECT ?entity ?type
    WHERE {
        {
            ?entity <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>
                    <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/Product> .
            BIND(<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/Product> AS ?type)
        }
        UNION
        {
            ?entity <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>
                    <http://purl.org/stuff/rev#Review> .
            BIND(<http://purl.org/stuff/rev#Review> AS ?type)
        }
    }
    LIMIT 20
$$);

\timing off

-- ── Q12: CONSTRUCT — export product triples ───────────────────────────────────
\echo 'Q12: CONSTRUCT — export Product1 triples'
\timing on

SELECT count(*) AS q12_constructed_triples
FROM pg_ripple.sparql_construct($$
    CONSTRUCT {
        ?product ?p ?o .
    }
    WHERE {
        ?product <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>
                 <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/Product> .
        ?product ?p ?o .
    }
    LIMIT 100
$$);

\timing off

\echo 'BSBM query mix complete.'
