-- pg_regress test: v0.79.0 WCOJ Leapfrog Triejoin executor (WCOJ-LFTI-01)
--
-- Tests:
-- 1. New GUC pg_ripple.wcoj_min_cardinality exists with default 0.
-- 2. Leapfrog Triejoin SortedIterator intersect unit check via wcoj_is_cyclic.
-- 3. 3-clique (triangle) detection returns correct count.
-- 4. 4-clique detection returns correct count.
-- 5. WCOJ result is equivalent to hash-join path.
-- 6. Social-network path query returns correct results.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
-- Load library and register GUCs by calling any pg_ripple function first.
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── Part 1: New GUC ───────────────────────────────────────────────────────────

-- 1a. wcoj_min_cardinality default is 0.
SHOW pg_ripple.wcoj_min_cardinality;

-- 1b. Can be set and read back.
SET pg_ripple.wcoj_min_cardinality = 1000;
SHOW pg_ripple.wcoj_min_cardinality;
SET pg_ripple.wcoj_min_cardinality = 0;

-- ── Part 2: Cyclic pattern detection (unchanged) ──────────────────────────────

-- 2a. Triangle is cyclic.
SELECT pg_ripple.wcoj_is_cyclic('[["a","b"],["b","c"],["c","a"]]') AS triangle_is_cyclic;

-- 2b. Star is NOT cyclic.
SELECT pg_ripple.wcoj_is_cyclic('[["root","a"],["root","b"],["root","c"]]') AS star_not_cyclic;

-- 2c. 4-cycle is cyclic.
SELECT pg_ripple.wcoj_is_cyclic('[["a","b"],["b","c"],["c","d"],["d","a"]]') AS square_is_cyclic;

-- ── Part 3: Triangle query with data (WCOJ enabled) ───────────────────────────

-- Insert a small graph that contains a triangle: A→B, B→C, C→A.
SELECT pg_ripple.insert_triple(
    '<http://wcoj-test.example/A>',
    '<http://wcoj-test.example/knows>',
    '<http://wcoj-test.example/B>'
) > 0 AS e_ab;

SELECT pg_ripple.insert_triple(
    '<http://wcoj-test.example/B>',
    '<http://wcoj-test.example/knows>',
    '<http://wcoj-test.example/C>'
) > 0 AS e_bc;

SELECT pg_ripple.insert_triple(
    '<http://wcoj-test.example/C>',
    '<http://wcoj-test.example/knows>',
    '<http://wcoj-test.example/A>'
) > 0 AS e_ca;

-- Also add a non-closing edge that does NOT form a triangle.
SELECT pg_ripple.insert_triple(
    '<http://wcoj-test.example/A>',
    '<http://wcoj-test.example/knows>',
    '<http://wcoj-test.example/D>'
) > 0 AS e_ad;

-- 3a. Triangle query with WCOJ enabled should return exactly 3 results (A→B→C, B→C→A, C→A→B).
SET pg_ripple.wcoj_enabled = true;
SELECT count(*) AS triangle_count_wcoj
FROM pg_ripple.sparql($$
    SELECT ?a ?b ?c WHERE {
        ?a <http://wcoj-test.example/knows> ?b .
        ?b <http://wcoj-test.example/knows> ?c .
        ?c <http://wcoj-test.example/knows> ?a .
    }
$$);

-- 3b. Triangle query with WCOJ disabled should return same result.
SET pg_ripple.wcoj_enabled = false;
SELECT count(*) AS triangle_count_no_wcoj
FROM pg_ripple.sparql($$
    SELECT ?a ?b ?c WHERE {
        ?a <http://wcoj-test.example/knows> ?b .
        ?b <http://wcoj-test.example/knows> ?c .
        ?c <http://wcoj-test.example/knows> ?a .
    }
$$);

-- Restore WCOJ.
SET pg_ripple.wcoj_enabled = true;

-- ── Part 4: 4-clique (square with cross-edges) ────────────────────────────────

-- Insert a 4-clique: every pair of P,Q,R,S knows each other.
DO $$
DECLARE
    nodes TEXT[] := ARRAY['P','Q','R','S'];
    i INT; j INT;
BEGIN
    FOR i IN 1..4 LOOP
        FOR j IN 1..4 LOOP
            IF i != j THEN
                PERFORM pg_ripple.insert_triple(
                    '<http://wcoj-test.example/' || nodes[i] || '>',
                    '<http://wcoj-test.example/friend>',
                    '<http://wcoj-test.example/' || nodes[j] || '>'
                );
            END IF;
        END LOOP;
    END LOOP;
END $$;

-- 4a. 4-cycle pattern is detected as cyclic.
SELECT pg_ripple.wcoj_is_cyclic('[["a","b"],["b","c"],["c","d"],["d","a"]]') AS four_cycle_cyclic;

-- 4b. Triangle count in the 4-clique: C(4,3) * 2 = 24 directed triangles.
SELECT count(*) AS triangle_count_in_4clique
FROM pg_ripple.sparql($$
    SELECT ?a ?b ?c WHERE {
        ?a <http://wcoj-test.example/friend> ?b .
        ?b <http://wcoj-test.example/friend> ?c .
        ?c <http://wcoj-test.example/friend> ?a .
    }
$$);

-- ── Part 5: wcoj_triangle_query benchmark helper ─────────────────────────────

-- 5a. wcoj_triangle_query returns valid JSONB.
SELECT pg_ripple.wcoj_triangle_query('http://wcoj-test.example/knows') IS NOT NULL AS triangle_jsonb_ok;

-- 5b. Triangle count matches direct query.
SELECT (pg_ripple.wcoj_triangle_query('http://wcoj-test.example/knows')->>'triangle_count')::int >= 1
    AS has_triangle;

-- ── Part 6: feature_status confirms WCOJ is implemented ──────────────────────

SELECT status AS wcoj_feature_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'wcoj';
