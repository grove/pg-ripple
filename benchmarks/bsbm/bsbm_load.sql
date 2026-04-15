-- bsbm_load.sql — Generate Berlin SPARQL Benchmark (BSBM) data for pg_ripple.
--
-- Loads a scale-factor-1 BSBM dataset (~1,000 products, ~10,000 triples).
-- Set :scale (psql variable) to a larger value for bigger datasets.
--
-- Usage:
--   psql -f benchmarks/bsbm/bsbm_load.sql
--   psql -v scale=10 -f benchmarks/bsbm/bsbm_load.sql
--
-- BSBM namespace prefixes (mirroring the official BSBM data generator):
--   bsbm:   http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/
--   bsbm-inst: http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/
--   dc:     http://purl.org/dc/elements/1.1/
--   foaf:   http://xmlns.com/foaf/0.1/
--   rev:    http://purl.org/stuff/rev#
--   xsd:    http://www.w3.org/2001/XMLSchema#

SET search_path TO pg_ripple, public;

-- ── Register standard BSBM prefixes ──────────────────────────────────────────
SELECT pg_ripple.register_prefix('bsbm', 'http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/');
SELECT pg_ripple.register_prefix('bsbm-inst', 'http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/');
SELECT pg_ripple.register_prefix('dc', 'http://purl.org/dc/elements/1.1/');
SELECT pg_ripple.register_prefix('foaf', 'http://xmlns.com/foaf/0.1/');
SELECT pg_ripple.register_prefix('rev', 'http://purl.org/stuff/rev#');
SELECT pg_ripple.register_prefix('rdfs', 'http://www.w3.org/2000/01/rdf-schema#');
SELECT pg_ripple.register_prefix('rdf', 'http://www.w3.org/1999/02/22-rdf-syntax-ns#');
SELECT pg_ripple.register_prefix('xsd', 'http://www.w3.org/2001/XMLSchema#');

-- ── Generate BSBM data ────────────────────────────────────────────────────────
-- Scale factor :scale (default 1 = 1,000 products).
-- Uses DO blocks to generate N-Triples strings and bulk-load them.

DO $$
DECLARE
    scale     INT := COALESCE(NULLIF('$BSBM_SCALE', ''), '1')::int;
    n_prod    INT := scale * 1000;   -- products
    n_feat    INT := scale * 5;      -- product features (classes)
    n_vendor  INT := GREATEST(1, scale * 10);  -- vendors
    n_review  INT := scale * 2000;   -- reviews (2× product count)
    n_rev     INT := GREATEST(1, scale * 50);  -- reviewers
    nt        TEXT := '';
    i         INT;
    feat_id   INT;
    vendor_id INT;
    reviewer_id INT;
    price     NUMERIC;
    rating    INT;
BEGIN
    -- ── Product Feature classes ───────────────────────────────────────────────
    FOR i IN 1..n_feat LOOP
        nt := nt ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature' || i || '> ' ||
            '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/ProductFeature> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature' || i || '> ' ||
            '<http://www.w3.org/2000/01/rdf-schema#label> ' ||
            '"Product Feature ' || i || '"@en .' || E'\n';
    END LOOP;

    -- Flush features.
    PERFORM pg_ripple.load_ntriples(nt);
    nt := '';

    -- ── Vendors ───────────────────────────────────────────────────────────────
    FOR i IN 1..n_vendor LOOP
        nt := nt ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Vendor' || i || '> ' ||
            '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/Vendor> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Vendor' || i || '> ' ||
            '<http://xmlns.com/foaf/0.1/name> ' ||
            '"Vendor ' || i || '"@en .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Vendor' || i || '> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/country> ' ||
            '"US" .' || E'\n';
    END LOOP;

    PERFORM pg_ripple.load_ntriples(nt);
    nt := '';

    -- ── Reviewers ─────────────────────────────────────────────────────────────
    FOR i IN 1..n_rev LOOP
        nt := nt ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Reviewer' || i || '> ' ||
            '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ' ||
            '<http://xmlns.com/foaf/0.1/Person> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Reviewer' || i || '> ' ||
            '<http://xmlns.com/foaf/0.1/name> ' ||
            '"Reviewer ' || i || '"@en .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Reviewer' || i || '> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/country> ' ||
            '"US" .' || E'\n';
    END LOOP;

    PERFORM pg_ripple.load_ntriples(nt);
    nt := '';

    -- ── Products (batched in groups of 100) ───────────────────────────────────
    FOR i IN 1..n_prod LOOP
        feat_id  := (i % n_feat) + 1;
        vendor_id := (i % n_vendor) + 1;
        price     := (100 + (random() * 900))::numeric(10,2);

        nt := nt ||
            -- rdf:type
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || i || '> ' ||
            '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/Product> .' || E'\n' ||
            -- rdfs:label
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || i || '> ' ||
            '<http://www.w3.org/2000/01/rdf-schema#label> ' ||
            '"Product ' || i || '"@en .' || E'\n' ||
            -- rdfs:comment
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || i || '> ' ||
            '<http://www.w3.org/2000/01/rdf-schema#comment> ' ||
            '"Description of product ' || i || '"@en .' || E'\n' ||
            -- bsbm:productFeature (primary)
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || i || '> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/productFeature> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature' || feat_id || '> .' || E'\n' ||
            -- bsbm:productFeature (secondary — ensures multi-feature star patterns)
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || i || '> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/productFeature> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/ProductFeature' || ((i % n_feat) + 1 + (i % 3)) % n_feat + 1 || '> .' || E'\n' ||
            -- bsbm:producer (vendor)
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || i || '> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/producer> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Vendor' || vendor_id || '> .' || E'\n' ||
            -- bsbm:price
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || i || '> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/price> ' ||
            '"' || price || '"^^<http://www.w3.org/2001/XMLSchema#decimal> .' || E'\n';

        -- Flush every 100 products.
        IF i % 100 = 0 THEN
            PERFORM pg_ripple.load_ntriples(nt);
            nt := '';
        END IF;
    END LOOP;

    -- Flush remaining products.
    IF length(nt) > 0 THEN
        PERFORM pg_ripple.load_ntriples(nt);
        nt := '';
    END IF;

    -- ── Reviews ───────────────────────────────────────────────────────────────
    FOR i IN 1..n_review LOOP
        feat_id     := (i % n_prod) + 1;       -- product reviewed
        reviewer_id := (i % n_rev) + 1;
        rating      := (1 + (random() * 9))::int;

        nt := nt ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Review' || i || '> ' ||
            '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ' ||
            '<http://purl.org/stuff/rev#Review> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Review' || i || '> ' ||
            '<http://purl.org/stuff/rev#reviewOf> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Product' || feat_id || '> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Review' || i || '> ' ||
            '<http://purl.org/stuff/rev#reviewer> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Reviewer' || reviewer_id || '> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Review' || i || '> ' ||
            '<http://purl.org/stuff/rev#rating> ' ||
            '"' || rating || '"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/Review' || i || '> ' ||
            '<http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/reviewDate> ' ||
            '"2024-01-01"^^<http://www.w3.org/2001/XMLSchema#date> .' || E'\n';

        -- Flush every 200 reviews.
        IF i % 200 = 0 THEN
            PERFORM pg_ripple.load_ntriples(nt);
            nt := '';
        END IF;
    END LOOP;

    IF length(nt) > 0 THEN
        PERFORM pg_ripple.load_ntriples(nt);
    END IF;

    RAISE NOTICE 'BSBM scale=% loaded: % products, % reviews, % vendors, % reviewers',
        scale, n_prod, n_review, n_vendor, n_rev;
END $$;

-- Report loaded triple count.
SELECT pg_ripple.triple_count() AS bsbm_total_triples;
