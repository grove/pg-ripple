-- pg_regress test: Extended VP (ExtVP) semi-join tables (v0.11.0)
--
-- Tests the ExtVP catalog, list functions, and the lifecycle of
-- manually-registered ExtVP catalog entries.
-- Actual stream table creation requires pg_trickle and is exercised
-- separately when pg_trickle is available.

SET search_path TO pg_ripple, public;

-- ── Catalog baseline ──────────────────────────────────────────────────────────

SELECT pg_ripple.list_extvp() = '[]'::jsonb AS no_extvp_yet;

-- ── ExtVP catalog table structure ────────────────────────────────────────────

SELECT COUNT(*) = 7 AS extvp_has_all_columns
FROM information_schema.columns
WHERE table_schema = '_pg_ripple'
  AND table_name = 'extvp_tables'
  AND column_name IN ('name','pred1_id','pred2_id',
                       'generated_sql','schedule','stream_table','created_at');

-- Verify the index on pred1_id exists.
SELECT EXISTS (
    SELECT 1 FROM pg_indexes
    WHERE schemaname = '_pg_ripple'
      AND tablename  = 'extvp_tables'
      AND indexname  = 'idx_extvp_pred1'
) AS pred1_index_exists;

SELECT EXISTS (
    SELECT 1 FROM pg_indexes
    WHERE schemaname = '_pg_ripple'
      AND tablename  = 'extvp_tables'
      AND indexname  = 'idx_extvp_pred2'
) AS pred2_index_exists;

-- ── Manual catalog entry to test list_extvp() ────────────────────────────────

-- Insert some triples so the predicates are known.
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<http://xmlns.com/foaf/0.1/name>',
    '"Alice"'
) > 0 AS foaf_name_inserted;

SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<http://xmlns.com/foaf/0.1/knows>',
    '<https://example.org/bob>'
) > 0 AS foaf_knows_inserted;

-- Directly insert a catalog entry to simulate create_extvp without pg_trickle.
-- REDUNDANT-01: pred1_iri/pred2_iri columns dropped; insert by pred_id only.
INSERT INTO _pg_ripple.extvp_tables
  (name, pred1_id, pred2_id, generated_sql, schedule, stream_table)
VALUES (
  'knows_name_ss',
  pg_ripple.encode_term('http://xmlns.com/foaf/0.1/knows', 0::smallint),
  pg_ripple.encode_term('http://xmlns.com/foaf/0.1/name',  0::smallint),
  'SELECT p1.s, p1.o AS o1, p2.o AS o2 FROM _pg_ripple.vp_rare p1 WHERE EXISTS (SELECT 1 FROM _pg_ripple.vp_rare p2 WHERE p2.s = p1.s)',
  '10s',
  '_pg_ripple.extvp_knows_name_ss'
) ON CONFLICT (name) DO NOTHING;

-- list_extvp() should return one entry.
SELECT jsonb_array_length(pg_ripple.list_extvp()) = 1 AS one_extvp;

-- The entry should have the correct name and predicate IRIs.
SELECT (pg_ripple.list_extvp()->0->>'name') = 'knows_name_ss' AS extvp_correct_name;
SELECT (pg_ripple.list_extvp()->0->>'pred1_iri') = 'http://xmlns.com/foaf/0.1/knows'
    AS extvp_correct_pred1;
SELECT (pg_ripple.list_extvp()->0->>'pred2_iri') = 'http://xmlns.com/foaf/0.1/name'
    AS extvp_correct_pred2;
SELECT (pg_ripple.list_extvp()->0->>'schedule') = '10s' AS extvp_correct_schedule;

-- ── Cleanup ───────────────────────────────────────────────────────────────────
DELETE FROM _pg_ripple.extvp_tables WHERE name = 'knows_name_ss';

SELECT pg_ripple.list_extvp() = '[]'::jsonb AS extvp_cleaned_up;
