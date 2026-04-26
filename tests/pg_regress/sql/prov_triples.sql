-- pg_regress test: PROV-O provenance tracking (v0.58.0)

-- Verify PROV-O API functions exist.
SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'prov_stats'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS prov_stats_fn_exists;

SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'prov_enabled'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS prov_enabled_fn_exists;

-- prov_catalog table must exist.
SELECT EXISTS (
    SELECT 1 FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname = '_pg_ripple'
      AND c.relname = 'prov_catalog'
) AS prov_catalog_exists;

-- Verify GUC exists and defaults to off.
SELECT current_setting('pg_ripple.prov_enabled') = 'off' AS prov_guc_off;

-- prov_enabled() returns false by default.
SELECT pg_ripple.prov_enabled() = false AS prov_disabled_by_default;

-- prov_stats() should return empty set when no provenance has been emitted.
SELECT count(*) = 0 AS no_prov_without_load
FROM pg_ripple.prov_stats();

-- Enable provenance.
SET pg_ripple.prov_enabled = on;

-- Load some triples to trigger provenance emission.
SELECT pg_ripple.load_ntriples(
    '<urn:prov_test:alice> <urn:prov_test:knows> <urn:prov_test:bob> .' || chr(10) ||
    '<urn:prov_test:bob> <urn:prov_test:knows> <urn:prov_test:charlie> .',
    false
) >= 2 AS loaded_ok;

-- prov_stats() should now have at least one entry.
SELECT count(*) >= 1 AS prov_entry_created
FROM pg_ripple.prov_stats();

-- The provenance entry should have a non-empty activity_iri.
SELECT bool_and(activity_iri <> '') AS activity_iri_set
FROM pg_ripple.prov_stats();

-- The provenance entry should have a positive triple_count.
SELECT bool_and(triple_count >= 1) AS triple_count_positive
FROM pg_ripple.prov_stats();

-- Disable provenance.
SET pg_ripple.prov_enabled = off;

-- After disabling, new loads should not create prov entries.
-- Count before the no-prov load.
SELECT count(*) AS prov_count_before_no_prov_load FROM pg_ripple.prov_stats();

SELECT pg_ripple.load_ntriples(
    '<urn:prov_test:no_prov:a> <urn:prov_test:no_prov:p> <urn:prov_test:no_prov:o> .',
    false
) >= 1 AS loaded_no_prov;

-- Count must still be exactly 1 (only the first load when prov was on).
SELECT count(*) = 1 AS prov_count_stable
FROM pg_ripple.prov_stats();

-- Reset.
RESET pg_ripple.prov_enabled;
