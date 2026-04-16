-- upgrade_path.sql: v0.14.0 upgrade path verification
-- Verifies that core functionality is intact after extension setup and
-- that the administrative API is fully available.

SET allow_system_table_mods = on;

-- 1. Basic triple store operations still work
SELECT pg_ripple.load_ntriples(
    '<https://upgrade.test/a> <https://upgrade.test/knows> <https://upgrade.test/b> .' || E'\n' ||
    '<https://upgrade.test/b> <https://upgrade.test/knows> <https://upgrade.test/c> .'
);

SELECT pg_ripple.triple_count() >= 2 AS triples_loaded;

-- 2. Dictionary encode/decode works
SELECT pg_ripple.decode_id(pg_ripple.encode_term('https://upgrade.test/a', 0::smallint)) = 'https://upgrade.test/a' AS dict_roundtrip;

-- 3. SPARQL SELECT works
SELECT count(*) >= 2 AS sparql_select_ok
FROM pg_ripple.sparql('SELECT ?s ?o WHERE { ?s <https://upgrade.test/knows> ?o }');

-- 4. Administrative functions available (v0.14.0)
SELECT pg_ripple.vacuum() >= 0 AS vacuum_available;
SELECT pg_ripple.reindex() >= 0 AS reindex_available;
SELECT pg_ripple.vacuum_dictionary() >= 0 AS vacuum_dict_available;
SELECT (pg_ripple.dictionary_stats() ? 'total_entries') AS dict_stats_available;

-- 5. Graph RLS API available (v0.14.0)
SELECT pg_ripple.enable_graph_rls() AS rls_api_available;
SELECT pg_ripple.grant_graph('postgres', '<https://upgrade.test/g>', 'read');
SELECT count(*) >= 0 AS list_access_available
FROM jsonb_array_elements(pg_ripple.list_graph_access());
SELECT pg_ripple.revoke_graph('postgres', '<https://upgrade.test/g>');

-- 6. schema_summary() available
SELECT jsonb_typeof(pg_ripple.schema_summary()) = 'array' AS schema_summary_available;

-- 7. Export still works
SELECT length(pg_ripple.export_ntriples(NULL)) >= 0 AS export_ok;

-- 8. predicate_stats view accessible
SELECT count(*) >= 0 AS predicate_stats_accessible FROM pg_ripple.predicate_stats;

-- Clean up
SELECT pg_ripple.drop_graph('<https://upgrade.test/>') >= 0 AS cleanup;
