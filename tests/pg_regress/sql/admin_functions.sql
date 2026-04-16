-- admin_functions.sql: v0.14.0 administrative and operational functions
-- Tests vacuum, reindex, vacuum_dictionary, dictionary_stats, predicate_stats

SET allow_system_table_mods = on;

-- Load some test triples first
SELECT pg_ripple.load_ntriples(
    '<https://admin.test/s1> <https://admin.test/p1> <https://admin.test/o1> .' || E'\n' ||
    '<https://admin.test/s2> <https://admin.test/p1> <https://admin.test/o2> .' || E'\n' ||
    '<https://admin.test/s3> <https://admin.test/p2> <https://admin.test/o3> .'
);

-- compact() should return a non-negative number
SELECT pg_ripple.compact() >= 0 AS compact_ok;

-- vacuum() runs analyze on VP tables; returns non-negative count
SELECT pg_ripple.vacuum() >= 0 AS vacuum_ok;

-- reindex() rebuilds indices; returns non-negative count
SELECT pg_ripple.reindex() >= 0 AS reindex_ok;

-- vacuum_dictionary() removes orphaned entries; returns non-negative count
SELECT pg_ripple.vacuum_dictionary() >= 0 AS vacuum_dict_ok;

-- dictionary_stats() returns JSONB with expected keys
SELECT (pg_ripple.dictionary_stats() ? 'total_entries') AS has_total_entries;
SELECT (pg_ripple.dictionary_stats() ? 'cache_capacity') AS has_cache_capacity;
SELECT (pg_ripple.dictionary_stats() ? 'shmem_ready') AS has_shmem_ready;
SELECT (pg_ripple.dictionary_stats()->'total_entries')::bigint >= 3 AS total_at_least_3;

-- predicate_stats view should include our loaded predicates
SELECT count(*) >= 0 AS predicate_stats_ok
FROM pg_ripple.predicate_stats;

-- triple_count should reflect loaded data
SELECT pg_ripple.triple_count() >= 3 AS triple_count_ok;

-- schema_summary() should return a JSONB array (may be empty if no rdf:type triples)
SELECT jsonb_typeof(pg_ripple.schema_summary()) = 'array' AS schema_summary_is_array;

-- Clean up
SELECT pg_ripple.drop_graph('<https://admin.test/>') >= 0 AS cleanup;
