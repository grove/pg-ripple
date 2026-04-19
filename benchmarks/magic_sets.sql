-- benchmarks/magic_sets.sql
--
-- Benchmark: full materialization (infer_with_stats) vs goal-directed
-- inference (infer_goal) on an RDFS subClassOf hierarchy.
--
-- Run inside psql connected to a pg18 database with pg_ripple installed:
--
--   \timing on
--   \i benchmarks/magic_sets.sql
--
-- Expected result: infer_goal for a single leaf class is substantially
-- faster than full materialization when the hierarchy is large.

SET search_path TO pg_ripple, public;
\timing on

-- ──────────────────────────────────────────────────────────────────────────
-- 1. Set up a synthetic 5-level class hierarchy with 10 nodes per level
-- ──────────────────────────────────────────────────────────────────────────

DO $$
DECLARE
    i   int;
    j   int;
    lvl int;
    parent_iri text;
    child_iri  text;
BEGIN
    -- Root class
    PERFORM pg_ripple.insert_triple(
        '<https://bench.example/cls0_0>',
        '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
        '<http://www.w3.org/2002/07/owl#Class>'
    );

    FOR lvl IN 1..5 LOOP
        FOR j IN 0..9 LOOP
            child_iri  := format('<https://bench.example/cls%s_%s>', lvl, j);
            parent_iri := format('<https://bench.example/cls%s_%s>', lvl - 1, j / 2);

            -- Declare child as a class
            PERFORM pg_ripple.insert_triple(
                child_iri,
                '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
                '<http://www.w3.org/2002/07/owl#Class>'
            );
            -- Link child to parent via rdfs:subClassOf
            PERFORM pg_ripple.insert_triple(
                child_iri,
                '<http://www.w3.org/2000/01/rdf-schema#subClassOf>',
                parent_iri
            );
        END LOOP;
    END LOOP;

    -- Add 1000 instances of leaf-level classes
    FOR i IN 1..1000 LOOP
        PERFORM pg_ripple.insert_triple(
            format('<https://bench.example/inst%s>', i),
            '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
            format('<https://bench.example/cls5_%s>', i % 10)
        );
    END LOOP;
END $$;

-- Refresh statistics so cost-based reorder has accurate cardinalities.
ANALYZE;

SELECT pg_ripple.load_rules_builtin('rdfs') AS rules_loaded;

-- ──────────────────────────────────────────────────────────────────────────
-- 2. Benchmark A: full materialization
-- ──────────────────────────────────────────────────────────────────────────

\echo '=== BENCHMARK A: full materialization (infer_with_stats) ==='
SELECT pg_ripple.infer_with_stats('rdfs');

-- ──────────────────────────────────────────────────────────────────────────
-- 3. Delete derived triples for a clean comparison.
-- ──────────────────────────────────────────────────────────────────────────

DELETE FROM _pg_ripple.vp_rare WHERE source = 1;

-- ──────────────────────────────────────────────────────────────────────────
-- 4. Benchmark B: goal-directed inference for one leaf class
-- ──────────────────────────────────────────────────────────────────────────

\echo '=== BENCHMARK B: infer_goal for a single leaf class ==='
SELECT pg_ripple.infer_goal(
    'rdfs',
    '?x <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://bench.example/cls5_0>'
);

-- ──────────────────────────────────────────────────────────────────────────
-- 5. Cleanup
-- ──────────────────────────────────────────────────────────────────────────

SELECT pg_ripple.drop_rules('rdfs');
DELETE FROM _pg_ripple.vp_rare WHERE 1=1;
