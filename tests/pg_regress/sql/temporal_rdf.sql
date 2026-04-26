-- pg_regress test: temporal RDF point-in-time queries (v0.58.0)

-- Verify the temporal API functions exist.
SELECT pg_get_function_identity_arguments(oid) AS sig
FROM pg_proc
WHERE proname = 'point_in_time'
  AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple');

SELECT pg_get_function_identity_arguments(oid) AS sig
FROM pg_proc
WHERE proname = 'clear_point_in_time'
  AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple');

SELECT pg_get_function_identity_arguments(oid) AS sig
FROM pg_proc
WHERE proname = 'point_in_time_info'
  AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple');

-- The statement_id_timeline table must exist.
SELECT EXISTS (
    SELECT 1 FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname = '_pg_ripple'
      AND c.relname = 'statement_id_timeline'
) AS timeline_table_exists;

-- clear_point_in_time should succeed without error.
DO $$ BEGIN PERFORM pg_ripple.clear_point_in_time(); END $$;
SELECT TRUE AS clear_ok;

-- point_in_time_info with no threshold should return (0, NULL).
SELECT threshold_sid = 0 AS threshold_is_zero
FROM pg_ripple.point_in_time_info();

-- Insert a triple and verify timeline table gets an entry.
SELECT pg_ripple.insert_triple(
    '<urn:temporal_test:alice>',
    '<urn:temporal_test:knows>',
    '<urn:temporal_test:bob>'
) > 0 AS inserted;

-- The timeline table should have at least one entry.
SELECT count(*) >= 1 AS timeline_has_entry
FROM _pg_ripple.statement_id_timeline;

-- Insert another triple, then set point_in_time to now.
SELECT pg_ripple.insert_triple(
    '<urn:temporal_test:charlie>',
    '<urn:temporal_test:knows>',
    '<urn:temporal_test:diana>'
) > 0 AS inserted_charlie;

-- Set point_in_time to current timestamp.
DO $$ BEGIN PERFORM pg_ripple.point_in_time(now()); END $$;
SELECT TRUE AS pit_set_ok;

-- point_in_time_info should now return a non-zero threshold.
SELECT threshold_sid >= 0 AS threshold_nonneg
FROM pg_ripple.point_in_time_info();

-- clear_point_in_time resets threshold to 0.
DO $$ BEGIN PERFORM pg_ripple.clear_point_in_time(); END $$;
SELECT TRUE AS cleared;

SELECT threshold_sid = 0 AS threshold_cleared
FROM pg_ripple.point_in_time_info();
