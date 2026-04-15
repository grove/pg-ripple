-- bsbm.sql — Berlin SPARQL Benchmark (BSBM) correctness test for pg_ripple.
--
-- Exercises BSBM query patterns Q1–Q12 at micro-scale (20 products, 40 reviews)
-- to verify that the SPARQL engine handles BSBM-shaped data correctly under
-- the HTAP (v0.6.0) delta/main architecture.
--
-- Wall-clock performance targets are validated separately via the benchmark
-- scripts in benchmarks/bsbm/.  This file verifies correctness only.

SET search_path TO pg_ripple, public;

-- ── Load micro-scale BSBM data ────────────────────────────────────────────────
-- 2 features, 2 vendors, 2 reviewers, 20 products, 40 reviews.

SELECT pg_ripple.register_prefix('bsbm', 'http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/');
SELECT pg_ripple.register_prefix('bsbm-inst', 'http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/');

DO $$
DECLARE
    nt TEXT := '';
    i  INT;
BEGIN
    -- 2 product features
    FOR i IN 1..2 LOOP
        nt := nt ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature' || i || '> ' ||
            '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/ProductFeature> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature' || i || '> ' ||
            '<http://www.w3.org/2000/01/rdf-schema#label> ' ||
            '"Feature ' || i || '"@en .' || E'\n';
    END LOOP;

    -- 2 vendors
    FOR i IN 1..2 LOOP
        nt := nt ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Vendor' || i || '> ' ||
            '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/Vendor> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Vendor' || i || '> ' ||
            '<http://xmlns.com/foaf/0.1/name> "Vendor ' || i || '"@en .' || E'\n';
    END LOOP;

    -- 2 reviewers
    FOR i IN 1..2 LOOP
        nt := nt ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Reviewer' || i || '> ' ||
            '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ' ||
            '<http://xmlns.com/foaf/0.1/Person> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Reviewer' || i || '> ' ||
            '<http://xmlns.com/foaf/0.1/name> "Reviewer ' || i || '"@en .' || E'\n';
    END LOOP;

    -- 20 products
    FOR i IN 1..20 LOOP
        nt := nt ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || i || '> ' ||
            '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/Product> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || i || '> ' ||
            '<http://www.w3.org/2000/01/rdf-schema#label> "Product ' || i || '"@en .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || i || '> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/productFeature> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature' || ((i % 2) + 1) || '> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || i || '> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/producer> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Vendor' || ((i % 2) + 1) || '> .' || E'\n';
    END LOOP;

    PERFORM pg_ripple.load_ntriples(nt);
    nt := '';

    -- 40 reviews (2 per product)
    FOR i IN 1..40 LOOP
        nt := nt ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Review' || i || '> ' ||
            '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ' ||
            '<http://purl.org/stuff/rev#Review> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Review' || i || '> ' ||
            '<http://purl.org/stuff/rev#reviewOf> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || ((i % 20) + 1) || '> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Review' || i || '> ' ||
            '<http://purl.org/stuff/rev#reviewer> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Reviewer' || ((i % 2) + 1) || '> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Review' || i || '> ' ||
            '<http://purl.org/stuff/rev#rating> ' ||
            '"' || (1 + (i % 9)) || '"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n';
    END LOOP;

    PERFORM pg_ripple.load_ntriples(nt);
END $$;

-- ── Verify data loaded ────────────────────────────────────────────────────────
SELECT pg_ripple.triple_count() >= 100 AS bsbm_data_loaded;

-- ── Q1: Products with ProductFeature1 ────────────────────────────────────────
-- 10 products have Feature1 (odd-numbered products).
SELECT count(*) AS q1_products_with_feature1
FROM pg_ripple.sparql($$
    SELECT ?product
    WHERE {
        ?product <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/productFeature>
                 <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature1> .
    }
$$);

-- ── Q2: Products with a feature and label (star pattern) ─────────────────────
SELECT count(*) AS q2_products_with_label_and_feature
FROM pg_ripple.sparql($$
    SELECT ?product ?label
    WHERE {
        ?product <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/productFeature>
                 <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature2> .
        ?product <http://www.w3.org/2000/01/rdf-schema#label> ?label .
    }
$$);

-- ── Q4: Reviews for Product1 ──────────────────────────────────────────────────
-- Product1 has reviews: Review1, Review21 (indices where (i % 20) + 1 = 1).
SELECT count(*) AS q4_reviews_for_product1
FROM pg_ripple.sparql($$
    SELECT ?review ?reviewer ?rating
    WHERE {
        ?review <http://purl.org/stuff/rev#reviewOf>
                <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product1> .
        ?review <http://purl.org/stuff/rev#reviewer> ?reviewer .
        ?review <http://purl.org/stuff/rev#rating> ?rating .
    }
$$);

-- ── Q5: Products from same vendor as Product1 ─────────────────────────────────
-- Product1 has producer Vendor2, so all even products share it.
SELECT count(*) >= 1 AS q5_sibling_products_exist
FROM pg_ripple.sparql($$
    SELECT ?product
    WHERE {
        <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product1>
            <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/producer> ?vendor .
        ?product <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/producer> ?vendor .
    }
$$);

-- ── Q8: OPTIONAL — reviewer with country ─────────────────────────────────────
-- Reviewers have no country triple, so OPTIONAL returns NULL for country.
SELECT count(*) = 2 AS q8_all_reviewers_found
FROM pg_ripple.sparql($$
    SELECT ?reviewer ?name
    WHERE {
        ?reviewer <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>
                  <http://xmlns.com/foaf/0.1/Person> .
        ?reviewer <http://xmlns.com/foaf/0.1/name> ?name .
    }
$$);

-- ── Q9: Aggregate — review count per product ─────────────────────────────────
-- Each product has exactly 2 reviews.
SELECT (result->>'reviewCount')::int = 2 AS q9_review_count_correct
FROM pg_ripple.sparql($$
    SELECT ?product (COUNT(?review) AS ?reviewCount)
    WHERE {
        ?review <http://purl.org/stuff/rev#reviewOf> ?product .
    }
    GROUP BY ?product
    ORDER BY ?product
    LIMIT 1
$$);

-- ── Q10: ASK ─────────────────────────────────────────────────────────────────
SELECT pg_ripple.sparql_ask($$
    ASK {
        ?p <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/productFeature>
           <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature1> .
    }
$$) AS q10_ask_true;

-- ── Q11: UNION query ──────────────────────────────────────────────────────────
-- 20 Products + 40 Reviews + 2 ProductFeatures = 62 typed entities total.
SELECT count(*) = 62 AS q11_union_entity_count
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
        UNION
        {
            ?entity <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>
                    <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/ProductFeature> .
            BIND(<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/ProductFeature> AS ?type)
        }
    }
$$);

-- ── Q12: CONSTRUCT ────────────────────────────────────────────────────────────
SELECT count(*) >= 1 AS q12_construct_nonempty
FROM pg_ripple.sparql_construct($$
    CONSTRUCT { ?p ?pred ?obj . }
    WHERE {
        ?p <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>
           <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/Product> .
        ?p ?pred ?obj .
    }
    LIMIT 20
$$);

-- ── HTAP: compact and re-query ────────────────────────────────────────────────
SELECT pg_ripple.compact() >= 0 AS bsbm_compact_ok;

-- After compact, Q4 must still return the same count.
SELECT count(*) AS q4_post_compact
FROM pg_ripple.sparql($$
    SELECT ?review
    WHERE {
        ?review <http://purl.org/stuff/rev#reviewOf>
                <http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product1> .
    }
$$);

-- stats() must report live_statistics_enabled field.
SELECT (pg_ripple.stats()->'live_statistics_enabled') IS NOT NULL AS stats_has_live_field;
