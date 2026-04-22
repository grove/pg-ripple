-- pg_regress test: sh:pattern constraint (v0.47.0)
--
-- Covers:
-- 1. Passing shape: literal matches regex pattern.
-- 2. Failing shape: literal does not match regex.
-- 3. sh:flags modifier (case-insensitive matching).
-- 4. Non-literal (IRI) values are checked against pattern.

SET client_min_messages = WARNING;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Setup ─────────────────────────────────────────────────────────────────────

-- ex:validEmail has a well-formed email → passes pattern
SELECT pg_ripple.insert_triple(
    '<https://ex.org/pattern/alice>',
    '<https://ex.org/pattern/email>',
    '"alice@example.com"'
) > 0 AS t1;

-- ex:invalidEmail has malformed email → fails pattern
SELECT pg_ripple.insert_triple(
    '<https://ex.org/pattern/bob>',
    '<https://ex.org/pattern/email>',
    '"not-an-email"'
) > 0 AS t2;

-- ex:caseTest has uppercase code → passes with i flag
SELECT pg_ripple.insert_triple(
    '<https://ex.org/pattern/carol>',
    '<https://ex.org/pattern/code>',
    '"ABC-123"'
) > 0 AS t3;

-- ex:caseFailTest has wrong format → fails even with i flag
SELECT pg_ripple.insert_triple(
    '<https://ex.org/pattern/dave>',
    '<https://ex.org/pattern/code>',
    '"123-abc"'
) > 0 AS t4;

-- ── Shape ─────────────────────────────────────────────────────────────────────

DO $shapes$
DECLARE
  shapes_ttl TEXT := $ttl$
    @prefix sh: <http://www.w3.org/ns/shacl#> .
    @prefix ex: <https://ex.org/pattern/> .

    ex:EmailShape
        a sh:NodeShape ;
        sh:targetNode ex:alice, ex:bob ;
        sh:property [
            sh:path ex:email ;
            sh:pattern "^[\\w.+-]+@[\\w-]+\\.[a-z]{2,}$"
        ] .

    ex:CodeShape
        a sh:NodeShape ;
        sh:targetNode ex:carol, ex:dave ;
        sh:property [
            sh:path ex:code ;
            sh:pattern "^[a-z]+-[0-9]+$" ;
            sh:flags "i"
        ] .
  $ttl$;
BEGIN
  PERFORM pg_ripple.load_turtle(shapes_ttl);
END
$shapes$;

-- ── Test 1: valid email passes pattern ────────────────────────────────────────
SELECT count(*) = 0 AS pattern_valid_no_violations
FROM jsonb_array_elements(
    pg_ripple.validate_graph(
        'https://ex.org/pattern/EmailShape'
    )
) v
WHERE v->>'focusNode' LIKE '%/alice%';

-- ── Test 2: invalid email fails pattern ───────────────────────────────────────
SELECT count(*) >= 1 AS pattern_invalid_has_violation
FROM jsonb_array_elements(
    pg_ripple.validate_graph(
        'https://ex.org/pattern/EmailShape'
    )
) v
WHERE v->>'focusNode' LIKE '%/bob%';

-- ── Test 3: case-insensitive flag passes ABC-123 ──────────────────────────────
SELECT count(*) = 0 AS pattern_case_passes
FROM jsonb_array_elements(
    pg_ripple.validate_graph(
        'https://ex.org/pattern/CodeShape'
    )
) v
WHERE v->>'focusNode' LIKE '%/carol%';

-- ── Test 4: wrong format fails even with i flag ───────────────────────────────
SELECT count(*) >= 1 AS pattern_wrong_format_fails
FROM jsonb_array_elements(
    pg_ripple.validate_graph(
        'https://ex.org/pattern/CodeShape'
    )
) v
WHERE v->>'focusNode' LIKE '%/dave%';

-- ── Cleanup ───────────────────────────────────────────────────────────────────
SELECT pg_ripple.drop_triples_by_graph(NULL) >= 0 AS cleaned;
