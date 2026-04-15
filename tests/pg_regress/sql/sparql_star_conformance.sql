-- sparql_star_conformance.sql — W3C SPARQL-star conformance gate
--
-- Tests the applicable subset of W3C RDF-star / SPARQL-star functionality:
--   - N-Triples-star parsing (subject/object position quoted triples)
--   - Quoted triple dictionary encoding round-trips
--   - Nested quoted triples
--   - Statement identifiers (SID lifecycle)
--   - SPARQL-star ground triple term patterns in BGP
--   - Edge properties via SIDs (annotation pattern)
--   - RDF-star data integrity across bulk loads
--
-- Based on the W3C RDF-star Community Group Test Suite and the
-- RDF 1.2 Semantics specification.
--
-- Limitations documented:
--   - Variable-inside-quoted-triple patterns emit a warning and
--     are treated as no-match (deferred to a future milestone)

SET search_path TO pg_ripple, public;

-- ══════════════════════════════════════════════════════════════════════════════
-- 1. N-Triples-star Parsing
-- ══════════════════════════════════════════════════════════════════════════════

-- 1.1 Standard N-Triples still work
SELECT pg_ripple.load_ntriples(
    '<http://example.org/star/alice> <http://example.org/star/name> "Alice" .' || E'\n' ||
    '<http://example.org/star/bob> <http://example.org/star/name> "Bob" .' || E'\n' ||
    '<http://example.org/star/alice> <http://example.org/star/knows> <http://example.org/star/bob> .'
) = 3 AS nt_standard_parse_ok;

-- 1.2 Object-position quoted triple
SELECT pg_ripple.load_ntriples(
    E'<http://example.org/star/carol> <http://example.org/star/believes> << <http://example.org/star/alice> <http://example.org/star/knows> <http://example.org/star/bob> >> .'
) = 1 AS nt_object_quoted_ok;

-- 1.3 Subject-position quoted triple
SELECT pg_ripple.load_ntriples(
    E'<< <http://example.org/star/alice> <http://example.org/star/knows> <http://example.org/star/bob> >> <http://example.org/star/source> <http://example.org/star/survey2024> .'
) = 1 AS nt_subject_quoted_ok;

-- 1.4 Quoted triple with literal object
SELECT pg_ripple.load_ntriples(
    E'<http://example.org/star/dave> <http://example.org/star/claims> << <http://example.org/star/bob> <http://example.org/star/age> "30"^^<http://www.w3.org/2001/XMLSchema#integer> >> .'
) = 1 AS nt_quoted_literal_object_ok;

-- 1.5 Nested quoted triples (quoted triple inside a quoted triple)
SELECT pg_ripple.load_ntriples(
    E'<http://example.org/star/eve> <http://example.org/star/disputes> << <http://example.org/star/carol> <http://example.org/star/believes> << <http://example.org/star/alice> <http://example.org/star/knows> <http://example.org/star/bob> >> >> .'
) = 1 AS nt_nested_quoted_ok;

-- ══════════════════════════════════════════════════════════════════════════════
-- 2. Dictionary Encoding Round-Trips (KIND_QUOTED_TRIPLE = 5)
-- ══════════════════════════════════════════════════════════════════════════════

-- 2.1 encode_triple returns a non-null ID
SELECT pg_ripple.encode_triple(
    '<http://example.org/star/alice>',
    '<http://example.org/star/knows>',
    '<http://example.org/star/bob>'
) IS NOT NULL AS encode_triple_nonnull;

-- 2.2 Same arguments → same ID (deterministic)
SELECT pg_ripple.encode_triple(
    '<http://example.org/star/alice>',
    '<http://example.org/star/knows>',
    '<http://example.org/star/bob>'
) = pg_ripple.encode_triple(
    '<http://example.org/star/alice>',
    '<http://example.org/star/knows>',
    '<http://example.org/star/bob>'
) AS encode_triple_deterministic;

-- 2.3 Different arguments → different ID
SELECT pg_ripple.encode_triple(
    '<http://example.org/star/alice>',
    '<http://example.org/star/knows>',
    '<http://example.org/star/bob>'
) != pg_ripple.encode_triple(
    '<http://example.org/star/bob>',
    '<http://example.org/star/knows>',
    '<http://example.org/star/alice>'
) AS encode_triple_distinct_ids;

-- 2.4 decode_triple round-trip: subject
SELECT (
    pg_ripple.decode_triple(
        pg_ripple.encode_triple(
            '<http://example.org/star/alice>',
            '<http://example.org/star/knows>',
            '<http://example.org/star/bob>'
        )
    ) ->> 's'
) = '<http://example.org/star/alice>' AS decode_subject_ok;

-- 2.5 decode_triple round-trip: predicate
SELECT (
    pg_ripple.decode_triple(
        pg_ripple.encode_triple(
            '<http://example.org/star/alice>',
            '<http://example.org/star/knows>',
            '<http://example.org/star/bob>'
        )
    ) ->> 'p'
) = '<http://example.org/star/knows>' AS decode_predicate_ok;

-- 2.6 decode_triple round-trip: object
SELECT (
    pg_ripple.decode_triple(
        pg_ripple.encode_triple(
            '<http://example.org/star/alice>',
            '<http://example.org/star/knows>',
            '<http://example.org/star/bob>'
        )
    ) ->> 'o'
) = '<http://example.org/star/bob>' AS decode_object_ok;

-- 2.7 Quoted triple with typed literal object preserves type
SELECT (
    pg_ripple.decode_triple(
        pg_ripple.encode_triple(
            '<http://example.org/star/bob>',
            '<http://example.org/star/age>',
            '"30"^^<http://www.w3.org/2001/XMLSchema#integer>'
        )
    ) ->> 'o'
) LIKE '%30%' AS decode_typed_literal_ok;

-- ══════════════════════════════════════════════════════════════════════════════
-- 3. Statement Identifiers (SID Lifecycle)
-- ══════════════════════════════════════════════════════════════════════════════

-- 3.1 insert_triple returns a valid SID
SELECT pg_ripple.insert_triple(
    '<http://example.org/star/sid_test/s1>',
    '<http://example.org/star/sid_test/p1>',
    '<http://example.org/star/sid_test/o1>'
) > 0 AS insert_returns_positive_sid;

-- 3.2 Different triples get different SIDs
SELECT pg_ripple.insert_triple(
    '<http://example.org/star/sid_test/s2>',
    '<http://example.org/star/sid_test/p2>',
    '<http://example.org/star/sid_test/o2>'
) != pg_ripple.insert_triple(
    '<http://example.org/star/sid_test/s3>',
    '<http://example.org/star/sid_test/p3>',
    '<http://example.org/star/sid_test/o3>'
) AS distinct_sids;

-- 3.3 get_statement retrieves a statement by SID
DO $$
DECLARE
    sid BIGINT;
    stmt JSONB;
BEGIN
    sid := pg_ripple.insert_triple(
        '<http://example.org/star/sid_test/s4>',
        '<http://example.org/star/sid_test/p4>',
        '<http://example.org/star/sid_test/o4>'
    );
    stmt := pg_ripple.get_statement(sid);
    ASSERT stmt IS NOT NULL, 'get_statement returned NULL';
    ASSERT stmt ->> 's' = '<http://example.org/star/sid_test/s4>',
        'get_statement subject mismatch: ' || (stmt ->> 's');
    ASSERT stmt ->> 'p' = '<http://example.org/star/sid_test/p4>',
        'get_statement predicate mismatch: ' || (stmt ->> 'p');
    ASSERT stmt ->> 'o' = '<http://example.org/star/sid_test/o4>',
        'get_statement object mismatch: ' || (stmt ->> 'o');
    RAISE NOTICE 'get_statement round-trip: PASS';
END $$;

-- ══════════════════════════════════════════════════════════════════════════════
-- 4. Edge Properties via SIDs (Annotation Pattern)
-- ══════════════════════════════════════════════════════════════════════════════

-- 4.1 Use a SID as the subject of annotation triples (LPG edge properties)
DO $$
DECLARE
    base_sid BIGINT;
    cnt BIGINT;
BEGIN
    -- Insert base triple: Alice knows Bob
    base_sid := pg_ripple.insert_triple(
        '<http://example.org/star/annot/alice>',
        '<http://example.org/star/annot/knows>',
        '<http://example.org/star/annot/bob>'
    );

    -- Annotate the edge with provenance and temporal metadata
    -- Use the SID's encoded form as the subject
    PERFORM pg_ripple.insert_triple(
        pg_ripple.decode_id(base_sid),
        '<http://example.org/star/annot/since>',
        '"2020-01-01"^^<http://www.w3.org/2001/XMLSchema#date>'
    );

    PERFORM pg_ripple.insert_triple(
        pg_ripple.decode_id(base_sid),
        '<http://example.org/star/annot/source>',
        '<http://example.org/star/annot/survey>'
    );

    -- Verify: the base SID triple is retrievable
    SELECT count(*) INTO cnt FROM pg_ripple.find_triples(
        '<http://example.org/star/annot/alice>',
        '<http://example.org/star/annot/knows>',
        '<http://example.org/star/annot/bob>'
    );
    ASSERT cnt = 1, 'base triple not found';

    RAISE NOTICE 'annotation pattern: PASS (base_sid=%)', base_sid;
END $$;

-- ══════════════════════════════════════════════════════════════════════════════
-- 5. SPARQL-star Ground Triple Term Patterns
-- ══════════════════════════════════════════════════════════════════════════════

-- Set up data: Alice knows Bob, Carol believes << Alice knows Bob >>
-- (Already loaded in section 1)

-- 5.1 SPARQL query with ground quoted triple in object position
--     "Who believes that Alice knows Bob?"
SELECT count(*) >= 1 AS sparql_star_object_ground
FROM pg_ripple.sparql($$
    SELECT ?who WHERE {
        ?who <http://example.org/star/believes>
             << <http://example.org/star/alice> <http://example.org/star/knows> <http://example.org/star/bob> >> .
    }
$$);

-- 5.2 SPARQL query with ground quoted triple in subject position
--     "What is the source of << Alice knows Bob >>?"
SELECT count(*) >= 1 AS sparql_star_subject_ground
FROM pg_ripple.sparql($$
    SELECT ?src WHERE {
        << <http://example.org/star/alice> <http://example.org/star/knows> <http://example.org/star/bob> >>
            <http://example.org/star/source> ?src .
    }
$$);

-- 5.3 SPARQL ASK with ground quoted triple
SELECT pg_ripple.sparql_ask($$
    ASK {
        <http://example.org/star/carol> <http://example.org/star/believes>
            << <http://example.org/star/alice> <http://example.org/star/knows> <http://example.org/star/bob> >> .
    }
$$) AS sparql_star_ask_ok;

-- 5.4 Ground quoted triple with typed literal
SELECT count(*) >= 1 AS sparql_star_typed_literal
FROM pg_ripple.sparql($$
    SELECT ?who WHERE {
        ?who <http://example.org/star/claims>
             << <http://example.org/star/bob> <http://example.org/star/age> "30"^^<http://www.w3.org/2001/XMLSchema#integer> >> .
    }
$$);

-- ══════════════════════════════════════════════════════════════════════════════
-- 6. RDF-star Data Integrity
-- ══════════════════════════════════════════════════════════════════════════════

-- 6.1 Multiple loads of the same quoted triple yield the same dictionary ID
DO $$
DECLARE
    id1 BIGINT;
    id2 BIGINT;
BEGIN
    id1 := pg_ripple.encode_triple(
        '<http://example.org/star/x>',
        '<http://example.org/star/y>',
        '<http://example.org/star/z>'
    );
    id2 := pg_ripple.encode_triple(
        '<http://example.org/star/x>',
        '<http://example.org/star/y>',
        '<http://example.org/star/z>'
    );
    ASSERT id1 = id2, 'quoted triple encode not idempotent';
    RAISE NOTICE 'encode idempotency: PASS';
END $$;

-- 6.2 Distinct quoted triples (different predicate) get distinct IDs
DO $$
DECLARE
    id1 BIGINT;
    id2 BIGINT;
BEGIN
    id1 := pg_ripple.encode_triple(
        '<http://example.org/star/a>',
        '<http://example.org/star/p1>',
        '<http://example.org/star/b>'
    );
    id2 := pg_ripple.encode_triple(
        '<http://example.org/star/a>',
        '<http://example.org/star/p2>',
        '<http://example.org/star/b>'
    );
    ASSERT id1 != id2, 'different quoted triples got same ID';
    RAISE NOTICE 'encode distinctness: PASS';
END $$;

-- 6.3 Bulk load preserves quoted triple relationships
DO $$
DECLARE
    cnt BIGINT;
BEGIN
    -- Load a batch with both standard and star triples
    PERFORM pg_ripple.load_ntriples(
        E'<http://example.org/star/batch/s1> <http://example.org/star/batch/p1> <http://example.org/star/batch/o1> .\n' ||
        E'<http://example.org/star/batch/annotator> <http://example.org/star/batch/annotates> << <http://example.org/star/batch/s1> <http://example.org/star/batch/p1> <http://example.org/star/batch/o1> >> .\n' ||
        E'<http://example.org/star/batch/s2> <http://example.org/star/batch/p2> <http://example.org/star/batch/o2> .'
    );

    -- Verify the annotation triple exists
    SELECT count(*) INTO cnt FROM pg_ripple.find_triples(
        '<http://example.org/star/batch/annotator>',
        '<http://example.org/star/batch/annotates>',
        NULL
    );
    ASSERT cnt >= 1, 'annotation triple not found after bulk load';
    RAISE NOTICE 'bulk load star integrity: PASS';
END $$;

-- ══════════════════════════════════════════════════════════════════════════════
-- 7. Known Limitations (documented, not failures)
-- ══════════════════════════════════════════════════════════════════════════════

-- 7.1 Variable-inside-quoted-triple patterns are not yet supported.
--     The query should still execute (returning 0 rows) with a WARNING,
--     not crash or error.
SELECT count(*) AS var_in_qt_returns_zero
FROM pg_ripple.sparql($$
    SELECT ?s ?p ?o WHERE {
        ?x <http://example.org/star/believes> << ?s ?p ?o >> .
    }
$$);
