-- =============================================================================
-- examples/sample_graphs.sql
-- Load two medium-sized example RDF graphs into pg_ripple for SPARQL demos.
--
-- Graph 1 — BSBM E-Commerce  <http://example.org/bsbm>
--           5 000 products · 100 vendors · 10 000 reviews  (~70 000 triples)
--
-- Graph 2 — Academic Knowledge Graph  <http://example.org/academic>
--           500 researchers · 2 000 papers · 5 000 citations  (~35 000 triples)
--
-- Usage:
--   psql moire -f examples/sample_graphs.sql
--
-- After loading, run example queries with:
--   psql moire -f examples/sparql_examples.sql
-- =============================================================================

SET search_path TO pg_ripple, public;

\echo ''
\echo '============================================================'
\echo '  pg_ripple sample graph loader'
\echo '============================================================'
\echo ''

-- ── Register common prefixes ──────────────────────────────────────────────────
SELECT pg_ripple.register_prefix('rdf',     'http://www.w3.org/1999/02/22-rdf-syntax-ns#');
SELECT pg_ripple.register_prefix('rdfs',    'http://www.w3.org/2000/01/rdf-schema#');
SELECT pg_ripple.register_prefix('xsd',     'http://www.w3.org/2001/XMLSchema#');
SELECT pg_ripple.register_prefix('owl',     'http://www.w3.org/2002/07/owl#');
SELECT pg_ripple.register_prefix('foaf',    'http://xmlns.com/foaf/0.1/');
SELECT pg_ripple.register_prefix('dc',      'http://purl.org/dc/elements/1.1/');
SELECT pg_ripple.register_prefix('dcterms', 'http://purl.org/dc/terms/');
SELECT pg_ripple.register_prefix('schema',  'http://schema.org/');
SELECT pg_ripple.register_prefix('skos',    'http://www.w3.org/2004/02/skos/core#');
SELECT pg_ripple.register_prefix('rev',     'http://purl.org/stuff/rev#');
SELECT pg_ripple.register_prefix('bsbm',    'http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/');
SELECT pg_ripple.register_prefix('ac',      'http://example.org/academic/');


-- =============================================================================
-- GRAPH 1: BSBM E-Commerce  <http://example.org/bsbm>
-- Products, vendors, reviewers, reviews.  Mirrors the Berlin SPARQL Benchmark.
-- =============================================================================

\echo 'Loading Graph 1: BSBM E-Commerce (5 000 products, 100 vendors, 10 000 reviews)...'

DO $$
DECLARE
    GRAPH     CONSTANT TEXT := 'http://example.org/bsbm';
    BASE      CONSTANT TEXT := 'http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/instances/';
    VOC       CONSTANT TEXT := 'http://www4.wiwiss.fu-berlin.de/bizer/bsbm/v01/vocabulary/';
    RDF_TYPE  CONSTANT TEXT := '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>';
    RDFS_LBL  CONSTANT TEXT := '<http://www.w3.org/2000/01/rdf-schema#label>';
    FOAF_NAME CONSTANT TEXT := '<http://xmlns.com/foaf/0.1/name>';
    XSD_DEC   CONSTANT TEXT := '^^<http://www.w3.org/2001/XMLSchema#decimal>';
    XSD_INT   CONSTANT TEXT := '^^<http://www.w3.org/2001/XMLSchema#integer>';
    XSD_DATE  CONSTANT TEXT := '^^<http://www.w3.org/2001/XMLSchema#date>';

    n_feat   INT := 25;
    n_vendor INT := 100;
    n_rev    INT := 250;
    n_prod   INT := 5000;
    n_review INT := 10000;

    countries TEXT[] := ARRAY['US','DE','FR','GB','JP','CA','AU','NL','SE','NO',
                               'IT','ES','BR','KR','IN'];
    nt        TEXT   := '';
    i         INT;
    j         INT;
    price     NUMERIC;
    rating    INT;
BEGIN
    -- ── Product Feature taxonomy ──────────────────────────────────────────────
    FOR i IN 1..n_feat LOOP
        nt := nt ||
            '<' || BASE || 'ProductFeature' || i || '> ' || RDF_TYPE ||
                ' <' || VOC || 'ProductFeature> .' || E'\n' ||
            '<' || BASE || 'ProductFeature' || i || '> ' || RDFS_LBL ||
                ' "Product Feature ' || i || '"@en .' || E'\n';
    END LOOP;
    PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
    nt := '';

    -- ── Vendors ───────────────────────────────────────────────────────────────
    FOR i IN 1..n_vendor LOOP
        nt := nt ||
            '<' || BASE || 'Vendor' || i || '> ' || RDF_TYPE ||
                ' <' || VOC || 'Vendor> .' || E'\n' ||
            '<' || BASE || 'Vendor' || i || '> ' || FOAF_NAME ||
                ' "Vendor ' || i || '"@en .' || E'\n' ||
            '<' || BASE || 'Vendor' || i || '> <' || VOC || 'country> "' ||
                countries[((i - 1) % 15) + 1] || '" .' || E'\n';
    END LOOP;
    PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
    nt := '';

    -- ── Reviewers ─────────────────────────────────────────────────────────────
    FOR i IN 1..n_rev LOOP
        nt := nt ||
            '<' || BASE || 'Reviewer' || i || '> ' || RDF_TYPE ||
                ' <http://xmlns.com/foaf/0.1/Person> .' || E'\n' ||
            '<' || BASE || 'Reviewer' || i || '> ' || FOAF_NAME ||
                ' "Reviewer ' || i || '"@en .' || E'\n' ||
            '<' || BASE || 'Reviewer' || i || '> <' || VOC || 'country> "' ||
                countries[((i - 1) % 15) + 1] || '" .' || E'\n';
    END LOOP;
    PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
    nt := '';

    -- ── Products (batched every 100) ──────────────────────────────────────────
    FOR i IN 1..n_prod LOOP
        price := round((100 + (random() * 900))::numeric, 2);
        j     := (i % n_feat) + 1;

        nt := nt ||
            '<' || BASE || 'Product' || i || '> ' || RDF_TYPE ||
                ' <' || VOC || 'Product> .' || E'\n' ||
            '<' || BASE || 'Product' || i || '> ' || RDFS_LBL ||
                ' "Product ' || i || '"@en .' || E'\n' ||
            '<' || BASE || 'Product' || i || '> <' || VOC || 'productFeature> ' ||
                '<' || BASE || 'ProductFeature' || j || '> .' || E'\n' ||
            '<' || BASE || 'Product' || i || '> <' || VOC || 'productFeature> ' ||
                '<' || BASE || 'ProductFeature' || ((j % n_feat) + 1) || '> .' || E'\n' ||
            '<' || BASE || 'Product' || i || '> <' || VOC || 'producer> ' ||
                '<' || BASE || 'Vendor' || ((i % n_vendor) + 1) || '> .' || E'\n' ||
            '<' || BASE || 'Product' || i || '> <' || VOC || 'price> "' ||
                price || '"' || XSD_DEC || ' .' || E'\n';

        IF i % 100 = 0 THEN
            PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
            nt := '';
        END IF;
    END LOOP;
    IF length(nt) > 0 THEN
        PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
        nt := '';
    END IF;

    -- ── Reviews (batched every 200) ───────────────────────────────────────────
    FOR i IN 1..n_review LOOP
        rating := 1 + floor(random() * 10)::int;

        nt := nt ||
            '<' || BASE || 'Review' || i || '> ' || RDF_TYPE ||
                ' <http://purl.org/stuff/rev#Review> .' || E'\n' ||
            '<' || BASE || 'Review' || i || '> <http://purl.org/stuff/rev#reviewOf> ' ||
                '<' || BASE || 'Product' || ((i % n_prod) + 1) || '> .' || E'\n' ||
            '<' || BASE || 'Review' || i || '> <http://purl.org/stuff/rev#reviewer> ' ||
                '<' || BASE || 'Reviewer' || ((i % n_rev) + 1) || '> .' || E'\n' ||
            '<' || BASE || 'Review' || i || '> <http://purl.org/stuff/rev#rating> "' ||
                rating || '"' || XSD_INT || ' .' || E'\n' ||
            '<' || BASE || 'Review' || i || '> <' || VOC || 'reviewDate> "' ||
                (2019 + (i % 5)) || '-' ||
                lpad(((i % 12) + 1)::text, 2, '0') || '-01"' || XSD_DATE || ' .' || E'\n';

        IF i % 200 = 0 THEN
            PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
            nt := '';
        END IF;
    END LOOP;
    IF length(nt) > 0 THEN
        PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
    END IF;

    RAISE NOTICE 'BSBM graph loaded: % products, % vendors, % reviewers, % reviews → <%>',
        n_prod, n_vendor, n_rev, n_review, GRAPH;
END $$;

\echo 'Graph 1 done.'
\echo ''


-- =============================================================================
-- GRAPH 2: Academic Knowledge Graph  <http://example.org/academic>
-- Universities, departments, researchers, papers, conferences, journals,
-- research topics, and citations.
-- =============================================================================

\echo 'Loading Graph 2: Academic KG (500 researchers, 2 000 papers, ~5 000 citations)...'

DO $$
DECLARE
    GRAPH     CONSTANT TEXT := 'http://example.org/academic';
    BASE      CONSTANT TEXT := 'http://example.org/academic/';
    RDF_TYPE  CONSTANT TEXT := '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>';
    XSD_YEAR  CONSTANT TEXT := '^^<http://www.w3.org/2001/XMLSchema#gYear>';
    XSD_DATE  CONSTANT TEXT := '^^<http://www.w3.org/2001/XMLSchema#date>';

    n_univ  INT := 20;
    n_dept  INT := 60;   -- 3 per university
    n_topic INT := 30;
    n_res   INT := 500;
    n_conf  INT := 50;
    n_jour  INT := 15;
    n_paper INT := 2000;
    n_cite  INT := 5000;

    univ_names TEXT[] := ARRAY[
        'MIT',
        'Stanford University',
        'University of Oxford',
        'University of Cambridge',
        'ETH Zurich',
        'Carnegie Mellon University',
        'UC Berkeley',
        'Princeton University',
        'Harvard University',
        'Imperial College London',
        'University of Toronto',
        'TU Munich',
        'TU Delft',
        'EPFL',
        'National University of Singapore',
        'Caltech',
        'University of Edinburgh',
        'Max Planck Institute for Informatics',
        'INRIA',
        'Tsinghua University'
    ];

    dept_types TEXT[] := ARRAY[
        'Computer Science',
        'Electrical Engineering',
        'Mathematics',
        'Information Systems',
        'Artificial Intelligence'
    ];

    topic_names TEXT[] := ARRAY[
        'Machine Learning',
        'Deep Learning',
        'Natural Language Processing',
        'Computer Vision',
        'Knowledge Graphs',
        'Database Systems',
        'Distributed Computing',
        'Cloud Computing',
        'Cryptography',
        'Formal Methods',
        'Algorithms',
        'Data Structures',
        'Computer Networks',
        'Operating Systems',
        'Programming Languages',
        'Software Engineering',
        'Human-Computer Interaction',
        'Bioinformatics',
        'Robotics',
        'Quantum Computing',
        'Information Retrieval',
        'Graph Theory',
        'Semantic Web',
        'Linked Data',
        'Ontology Engineering',
        'Federated Learning',
        'Graph Neural Networks',
        'Reinforcement Learning',
        'Question Answering',
        'Recommender Systems'
    ];

    first_names TEXT[] := ARRAY[
        'Alice','Bob','Carlos','Diana','Ethan','Fiona','George','Hannah',
        'Ivan','Julia','Kevin','Laura','Michael','Natasha','Oscar','Patricia',
        'Quentin','Rachel','Stefan','Tina','Umar','Vera','Wei','Xiao','Yuki','Zara'
    ];

    last_names TEXT[] := ARRAY[
        'Chen','Smith','Garcia','Brown','Mueller','Johnson','Lee','Kim',
        'Patel','Wagner','Tanaka','Williams','Fischer','Liu','Thompson',
        'Andersen','Petrov','Costa','Nakamura','Anderson','Martinez','Park',
        'Nguyen','Khan','Zhang','Hoffmann'
    ];

    conf_names TEXT[] := ARRAY[
        'ISWC','TheWebConf','SIGMOD','VLDB','KDD',
        'NeurIPS','ICML','ACL','EMNLP','ICLR',
        'CVPR','ICCV','AAAI','IJCAI','SIGIR',
        'RecSys','EDBT','PODS','ICDE','ICSE',
        'FSE','PLDI','SOSP','OSDI','NSDI',
        'CCS','WSDM','CoNLL','NAACL','EACL',
        'COLING','CHI','UIST','CSCW','ECIR',
        'HyperText','WebSci','CAiSE','ER','FOIS',
        'K-CAP','EKAW','ESWC','ECCV','ASPLOS',
        'USENIX Security','EuroPKI','INTERSPEECH','ICLP','SEKE'
    ];

    jour_names TEXT[] := ARRAY[
        'Journal of Web Semantics',
        'Semantic Web Journal',
        'IEEE TKDE',
        'VLDB Journal',
        'ACM TODS',
        'JMLR',
        'AI Magazine',
        'Communications of the ACM',
        'Information Systems',
        'Data and Knowledge Engineering',
        'Knowledge and Information Systems',
        'IEEE Transactions on Neural Networks',
        'Journal of AI Research',
        'Nature Machine Intelligence',
        'ACM Transactions on Computational Logic'
    ];

    nt  TEXT := '';
    i   INT;
    j   INT;
    k   INT;
    fn  TEXT;
    ln  TEXT;
    yr  INT;
BEGIN
    -- ── Universities ──────────────────────────────────────────────────────────
    FOR i IN 1..n_univ LOOP
        nt := nt ||
            '<' || BASE || 'univ/' || i || '> ' || RDF_TYPE ||
                ' <http://schema.org/EducationalOrganization> .' || E'\n' ||
            '<' || BASE || 'univ/' || i || '> <http://schema.org/name> "' ||
                univ_names[i] || '" .' || E'\n' ||
            '<' || BASE || 'univ/' || i || '> <http://www.w3.org/2000/01/rdf-schema#label> "' ||
                univ_names[i] || '"@en .' || E'\n';
    END LOOP;
    PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
    nt := '';

    -- ── Departments (3 per university) ────────────────────────────────────────
    FOR i IN 1..n_dept LOOP
        j := ((i - 1) / 3) + 1;     -- parent university (1..20)
        k := ((i - 1) % 5)  + 1;    -- department type   (1..5)

        nt := nt ||
            '<' || BASE || 'dept/' || i || '> ' || RDF_TYPE ||
                ' <http://schema.org/Organization> .' || E'\n' ||
            '<' || BASE || 'dept/' || i || '> <http://schema.org/name> "Dept. of ' ||
                dept_types[k] || ', ' || univ_names[j] || '" .' || E'\n' ||
            '<' || BASE || 'dept/' || i || '> <http://schema.org/parentOrganization> ' ||
                '<' || BASE || 'univ/' || j || '> .' || E'\n';
    END LOOP;
    PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
    nt := '';

    -- ── Research Topics (SKOS concept scheme with broader links) ──────────────
    FOR i IN 1..n_topic LOOP
        nt := nt ||
            '<' || BASE || 'topic/' || i || '> ' || RDF_TYPE ||
                ' <http://www.w3.org/2004/02/skos/core#Concept> .' || E'\n' ||
            '<' || BASE || 'topic/' || i ||
                '> <http://www.w3.org/2004/02/skos/core#prefLabel> "' ||
                topic_names[i] || '"@en .' || E'\n';

        -- Sub-topics 6..30 each get a broader link into one of the first 5 topics.
        IF i > 5 THEN
            nt := nt ||
                '<' || BASE || 'topic/' || i ||
                    '> <http://www.w3.org/2004/02/skos/core#broader> ' ||
                    '<' || BASE || 'topic/' || ((i % 5) + 1) || '> .' || E'\n';
        END IF;
    END LOOP;
    PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
    nt := '';

    -- ── Conferences ───────────────────────────────────────────────────────────
    FOR i IN 1..n_conf LOOP
        nt := nt ||
            '<' || BASE || 'conf/' || i || '> ' || RDF_TYPE ||
                ' <http://schema.org/AcademicEvent> .' || E'\n' ||
            '<' || BASE || 'conf/' || i || '> <http://schema.org/name> "' ||
                conf_names[i] || ' ' || (2018 + (i % 7)) || '" .' || E'\n' ||
            '<' || BASE || 'conf/' || i || '> <http://schema.org/startDate> "' ||
                (2018 + (i % 7)) || '-' ||
                lpad(((i % 12) + 1)::text, 2, '0') || '-01"' || XSD_DATE || ' .' || E'\n';
    END LOOP;
    PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
    nt := '';

    -- ── Journals ──────────────────────────────────────────────────────────────
    FOR i IN 1..n_jour LOOP
        nt := nt ||
            '<' || BASE || 'journal/' || i || '> ' || RDF_TYPE ||
                ' <http://schema.org/Periodical> .' || E'\n' ||
            '<' || BASE || 'journal/' || i || '> <http://schema.org/name> "' ||
                jour_names[i] || '" .' || E'\n';
    END LOOP;
    PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
    nt := '';

    -- ── Researchers ───────────────────────────────────────────────────────────
    FOR i IN 1..n_res LOOP
        fn := first_names[((i - 1) % array_length(first_names, 1)) + 1];
        ln := last_names [((i - 1) % array_length(last_names,  1)) + 1];
        j  := (i % n_dept)  + 1;   -- affiliated department
        k  := (i % n_topic) + 1;   -- primary research topic

        nt := nt ||
            '<' || BASE || 'person/' || i || '> ' || RDF_TYPE ||
                ' <http://xmlns.com/foaf/0.1/Person> .' || E'\n' ||
            '<' || BASE || 'person/' || i ||
                '> <http://xmlns.com/foaf/0.1/name> "' || fn || ' ' || ln || '" .' || E'\n' ||
            '<' || BASE || 'person/' || i ||
                '> <http://xmlns.com/foaf/0.1/mbox> ' ||
                '<mailto:' || lower(fn) || '.' || lower(ln) || i || '@example.org> .' || E'\n' ||
            '<' || BASE || 'person/' || i || '> <http://schema.org/affiliation> ' ||
                '<' || BASE || 'dept/' || j || '> .' || E'\n' ||
            '<' || BASE || 'person/' || i ||
                '> <http://xmlns.com/foaf/0.1/topic_interest> ' ||
                '<' || BASE || 'topic/' || k || '> .' || E'\n';

        IF i % 50 = 0 THEN
            PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
            nt := '';
        END IF;
    END LOOP;
    IF length(nt) > 0 THEN
        PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
        nt := '';
    END IF;

    -- ── Papers (batched every 100) ────────────────────────────────────────────
    FOR i IN 1..n_paper LOOP
        yr := 2017 + (i % 8);

        nt := nt ||
            '<' || BASE || 'paper/' || i || '> ' || RDF_TYPE ||
                ' <http://schema.org/ScholarlyArticle> .' || E'\n' ||
            '<' || BASE || 'paper/' || i || '> <http://purl.org/dc/terms/title> "' ||
                topic_names[(i % n_topic) + 1] || ': ' ||
                CASE (i % 5)
                    WHEN 0 THEN 'A Survey'
                    WHEN 1 THEN 'New Advances'
                    WHEN 2 THEN 'Scalable Approaches'
                    WHEN 3 THEN 'Deep Dive'
                    ELSE        'Benchmarking Methods'
                END ||
                ' (Paper ' || i || ')" .' || E'\n' ||
            '<' || BASE || 'paper/' || i || '> <http://purl.org/dc/terms/date> "' ||
                yr || '"' || XSD_YEAR || ' .' || E'\n';

        -- Venue: journals for every 3rd paper, conferences otherwise
        IF i % 3 = 0 THEN
            nt := nt ||
                '<' || BASE || 'paper/' || i || '> <http://schema.org/isPartOf> ' ||
                    '<' || BASE || 'journal/' || ((i % n_jour) + 1) || '> .' || E'\n';
        ELSE
            nt := nt ||
                '<' || BASE || 'paper/' || i || '> <http://schema.org/isPartOf> ' ||
                    '<' || BASE || 'conf/' || ((i % n_conf) + 1) || '> .' || E'\n';
        END IF;

        -- Authors: 2 guaranteed; 3rd added for ~2/3 of papers
        nt := nt ||
            '<' || BASE || 'paper/' || i || '> <http://purl.org/dc/terms/creator> ' ||
                '<' || BASE || 'person/' || ((i % n_res) + 1) || '> .' || E'\n' ||
            '<' || BASE || 'paper/' || i || '> <http://purl.org/dc/terms/creator> ' ||
                '<' || BASE || 'person/' || (((i + 71) % n_res) + 1) || '> .' || E'\n';

        IF i % 3 != 1 THEN
            nt := nt ||
                '<' || BASE || 'paper/' || i || '> <http://purl.org/dc/terms/creator> ' ||
                    '<' || BASE || 'person/' || (((i + 137) % n_res) + 1) || '> .' || E'\n';
        END IF;

        -- Topics: 1 guaranteed; 2nd added for every other paper
        nt := nt ||
            '<' || BASE || 'paper/' || i || '> <http://schema.org/about> ' ||
                '<' || BASE || 'topic/' || ((i % n_topic) + 1) || '> .' || E'\n';

        IF i % 2 = 0 THEN
            nt := nt ||
                '<' || BASE || 'paper/' || i || '> <http://schema.org/about> ' ||
                    '<' || BASE || 'topic/' || (((i + 7) % n_topic) + 1) || '> .' || E'\n';
        END IF;

        IF i % 100 = 0 THEN
            PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
            nt := '';
        END IF;
    END LOOP;
    IF length(nt) > 0 THEN
        PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
        nt := '';
    END IF;

    -- ── Citations (paper cites paper, batched every 500) ──────────────────────
    FOR i IN 1..n_cite LOOP
        j := (i % n_paper) + 1;
        k := ((i * 7 + 13) % n_paper) + 1;

        IF j != k THEN
            nt := nt ||
                '<' || BASE || 'paper/' || j || '> <http://schema.org/citation> ' ||
                    '<' || BASE || 'paper/' || k || '> .' || E'\n';
        END IF;

        IF i % 500 = 0 THEN
            PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
            nt := '';
        END IF;
    END LOOP;
    IF length(nt) > 0 THEN
        PERFORM pg_ripple.load_ntriples_into_graph(nt, GRAPH);
    END IF;

    RAISE NOTICE 'Academic KG loaded: % universities, % depts, % researchers, % papers, ~% citations → <%>',
        n_univ, n_dept, n_res, n_paper, n_cite, GRAPH;
END $$;

\echo 'Graph 2 done.'
\echo ''


-- =============================================================================
-- Summary
-- =============================================================================

\echo 'Named-graph triple counts:'
SELECT result->>'g' AS graph_iri, (result->>'triples')::bigint AS triples
FROM   pg_ripple.sparql($$
    SELECT ?g (COUNT(*) AS ?triples)
    WHERE  { GRAPH ?g { ?s ?p ?o } }
    GROUP BY ?g
    ORDER BY ?g
$$);

\echo ''
\echo 'Total triples in store (all graphs):'
SELECT pg_ripple.triple_count() AS total_triples;

\echo ''
\echo 'Done.  Run: psql moire -f examples/sparql_examples.sql'
