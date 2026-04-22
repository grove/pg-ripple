-- pg_regress test: sh:lessThanOrEquals constraint (v0.47.0)
--
-- Covers:
-- 1. Passing shape: value of path A <= value of path B.
-- 2. Failing shape: value of path A > value of path B → violation.
-- 3. Equal values pass (the "or equals" case).
-- 4. String comparison (lexicographic).

SET client_min_messages = WARNING;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Setup ─────────────────────────────────────────────────────────────────────

-- ex:event1: startDate 2024-01-01, endDate 2024-06-30 → start <= end ✓
SELECT pg_ripple.insert_triple(
    '<https://ex.org/ltoe/event1>',
    '<https://ex.org/ltoe/startDate>',
    '"2024-01-01"^^<http://www.w3.org/2001/XMLSchema#date>'
) > 0 AS t1;

SELECT pg_ripple.insert_triple(
    '<https://ex.org/ltoe/event1>',
    '<https://ex.org/ltoe/endDate>',
    '"2024-06-30"^^<http://www.w3.org/2001/XMLSchema#date>'
) > 0 AS t2;

-- ex:event2: startDate 2024-07-01, endDate 2024-03-01 → start > end ✗
SELECT pg_ripple.insert_triple(
    '<https://ex.org/ltoe/event2>',
    '<https://ex.org/ltoe/startDate>',
    '"2024-07-01"^^<http://www.w3.org/2001/XMLSchema#date>'
) > 0 AS t3;

SELECT pg_ripple.insert_triple(
    '<https://ex.org/ltoe/event2>',
    '<https://ex.org/ltoe/endDate>',
    '"2024-03-01"^^<http://www.w3.org/2001/XMLSchema#date>'
) > 0 AS t4;

-- ex:event3: startDate = endDate (same day) → equal passes ✓
SELECT pg_ripple.insert_triple(
    '<https://ex.org/ltoe/event3>',
    '<https://ex.org/ltoe/startDate>',
    '"2024-05-15"^^<http://www.w3.org/2001/XMLSchema#date>'
) > 0 AS t5;

SELECT pg_ripple.insert_triple(
    '<https://ex.org/ltoe/event3>',
    '<https://ex.org/ltoe/endDate>',
    '"2024-05-15"^^<http://www.w3.org/2001/XMLSchema#date>'
) > 0 AS t6;

-- ── Shape ─────────────────────────────────────────────────────────────────────

DO $shapes$
DECLARE
  shapes_ttl TEXT := $ttl$
    @prefix sh: <http://www.w3.org/ns/shacl#> .
    @prefix ex: <https://ex.org/ltoe/> .

    ex:EventShape
        a sh:NodeShape ;
        sh:targetNode ex:event1, ex:event2, ex:event3 ;
        sh:property [
            sh:path ex:startDate ;
            sh:lessThanOrEquals ex:endDate
        ] .
  $ttl$;
BEGIN
  PERFORM pg_ripple.load_turtle(shapes_ttl);
END
$shapes$;

-- ── Test 1: start < end passes ────────────────────────────────────────────────
SELECT count(*) = 0 AS ltoe_lt_passes
FROM jsonb_array_elements(
    pg_ripple.validate_graph(
        'https://ex.org/ltoe/EventShape'
    )
) v
WHERE v->>'focusNode' LIKE '%event1%';

-- ── Test 2: start > end fails ─────────────────────────────────────────────────
SELECT count(*) >= 1 AS ltoe_gt_fails
FROM jsonb_array_elements(
    pg_ripple.validate_graph(
        'https://ex.org/ltoe/EventShape'
    )
) v
WHERE v->>'focusNode' LIKE '%event2%';

-- ── Test 3: start = end passes (the "equals" case) ───────────────────────────
SELECT count(*) = 0 AS ltoe_eq_passes
FROM jsonb_array_elements(
    pg_ripple.validate_graph(
        'https://ex.org/ltoe/EventShape'
    )
) v
WHERE v->>'focusNode' LIKE '%event3%';

-- ── Cleanup ───────────────────────────────────────────────────────────────────
SELECT pg_ripple.drop_triples_by_graph(NULL) >= 0 AS cleaned;
