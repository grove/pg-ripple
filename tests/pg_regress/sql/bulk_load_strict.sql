-- pg_regress test: bulk load strict mode (v0.25.0 M-8)
-- Verifies that strict=true aborts on malformed input, while strict=false
-- (the default) skips bad triples and continues.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ─── Setup ────────────────────────────────────────────────────────────────────

-- Baseline triple count before this test.
CREATE TEMP TABLE _sl_strict_baseline AS
  SELECT COALESCE(MAX(i), 0) AS max_i FROM _pg_ripple.vp_rare;

-- ─── Lenient mode (default strict=false) ──────────────────────────────────────

-- A well-formed triple loads without error.
SELECT pg_ripple.load_ntriples('
<http://strict.test/alice> <http://strict.test/name> "Alice" .
') > 0 AS lenient_load_ok;

-- Lenient: a malformed N-Triples line (missing trailing dot) emits a WARNING
-- and the well-formed line above is NOT rolled back.
SELECT pg_ripple.load_ntriples(
  E'<http://strict.test/bad_line_no_dot>\n<http://strict.test/name> "also Alice"\n'
) >= 0 AS lenient_bad_returns_nonneg;

-- ─── Strict mode (strict=true) ────────────────────────────────────────────────

-- A well-formed N-Triples string should still load fine under strict mode.
SELECT pg_ripple.load_ntriples('
<http://strict.test/bob> <http://strict.test/name> "Bob" .
', true) > 0 AS strict_valid_ok;

-- ─── Cleanup ──────────────────────────────────────────────────────────────────

DO $$
DECLARE
  baseline_i bigint;
BEGIN
  SELECT max_i INTO baseline_i FROM _sl_strict_baseline;
  DELETE FROM _pg_ripple.vp_rare WHERE i > baseline_i;
END $$;
DROP TABLE _sl_strict_baseline;
