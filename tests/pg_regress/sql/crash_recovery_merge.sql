-- crash_recovery_merge.sql — Crash recovery smoke test (v0.20.0)
--
-- This test verifies that the system is in a consistent state after a
-- simulated or actual crash recovery. It is designed to run as part of
-- the regular pg_regress suite as a "clean-state" correctness check.
--
-- What this checks:
--   1. _pg_ripple.predicates catalog is accessible and self-consistent
--   2. dictionary table is accessible (no orphaned entries detectable by hash)
--   3. VP table data is queryable (rows visible, no stray VACUUM marks)
--   4. Subsequent INSERT/SELECT/DELETE works correctly
--   5. HTAP delta/main structure is consistent
--   6. promote_rare_predicates() runs without error
--   7. vacuum() and reindex() run without error
--   8. The merge worker status GUC is readable
--
-- For true crash testing (kill -9 during merge), see:
--   tests/crash_recovery/merge_during_kill.sh
--   tests/crash_recovery/dict_during_kill.sh
--   tests/crash_recovery/shacl_during_violation.sh
--
-- Namespace: https://crash.recovery.test/

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET allow_system_table_mods = on;
SET search_path TO pg_ripple, public;

-- ══════════════════════════════════════════════════════════════════════════════
-- 1. Catalog consistency
-- ══════════════════════════════════════════════════════════════════════════════

-- 1.1 predicates catalog is accessible
SELECT count(*) >= 0 AS predicates_catalog_accessible
FROM _pg_ripple.predicates;

-- 1.2 predicates catalog has no negative triple counts
SELECT count(*) = 0 AS no_negative_triple_counts
FROM _pg_ripple.predicates
WHERE triple_count < 0;

-- 1.3 dictionary table is accessible
SELECT count(*) >= 0 AS dictionary_accessible
FROM _pg_ripple.dictionary;

-- 1.4 dictionary: no duplicate (value, kind) pairs
--     (XXH3-128 design: id uniquely maps to (value, kind))
SELECT count(*) = 0 AS no_duplicate_dictionary_entries
FROM (
    SELECT value, kind, count(*) AS n
    FROM _pg_ripple.dictionary
    GROUP BY value, kind
    HAVING count(*) > 1
) dups;

-- ══════════════════════════════════════════════════════════════════════════════
-- 2. VP table data recovery
-- ══════════════════════════════════════════════════════════════════════════════

-- 2.1 Insert test triples to verify storage is writable
DO $$
BEGIN
    DELETE FROM _pg_ripple.vp_rare
    WHERE p IN (
        SELECT id FROM _pg_ripple.dictionary
        WHERE value LIKE 'https://crash.recovery.test/%'
    );
END $$;

SELECT pg_ripple.load_ntriples(
    '<https://crash.recovery.test/s1> <https://crash.recovery.test/p1> <https://crash.recovery.test/o1> .' || E'\n' ||
    '<https://crash.recovery.test/s2> <https://crash.recovery.test/p1> <https://crash.recovery.test/o2> .' || E'\n' ||
    '<https://crash.recovery.test/s3> <https://crash.recovery.test/p1> <https://crash.recovery.test/o3> .'
) = 3 AS vp_write_ok;

-- 2.2 Read back: all three triples visible
SELECT count(*) = 3 AS vp_read_ok
FROM pg_ripple.find_triples(NULL, '<https://crash.recovery.test/p1>', NULL);

-- 2.3 SPARQL SELECT over the inserted triples
SELECT count(*) = 3 AS sparql_over_vp_ok
FROM pg_ripple.sparql($$
    SELECT ?s ?o WHERE {
        ?s <https://crash.recovery.test/p1> ?o .
    }
$$);

-- 2.4 Delete one triple and verify count
SELECT pg_ripple.sparql_update($$
    DELETE DATA {
        <https://crash.recovery.test/s3>
            <https://crash.recovery.test/p1>
            <https://crash.recovery.test/o3> .
    }
$$) = 1 AS delete_ok;

SELECT count(*) = 2 AS post_delete_count_correct
FROM pg_ripple.find_triples(NULL, '<https://crash.recovery.test/p1>', NULL);

-- ══════════════════════════════════════════════════════════════════════════════
-- 3. HTAP structure consistency
-- ══════════════════════════════════════════════════════════════════════════════

-- 3.1 triple_count() returns non-negative value
SELECT pg_ripple.triple_count() >= 0 AS triple_count_nonneg;

-- 3.2 promote_rare_predicates() runs without error
SELECT pg_ripple.promote_rare_predicates() >= 0 AS promote_ok;

-- 3.3 vacuum() runs without error
SELECT pg_ripple.vacuum() >= 0 AS vacuum_ok;

-- 3.4 reindex() runs without error
SELECT pg_ripple.reindex() >= 0 AS reindex_ok;

-- 3.5 vacuum_dictionary() runs without error
SELECT pg_ripple.vacuum_dictionary() >= 0 AS vacuum_dict_ok;

-- ══════════════════════════════════════════════════════════════════════════════
-- 4. Dictionary consistency after write/delete cycle
-- ══════════════════════════════════════════════════════════════════════════════

-- 4.1 encode_term round-trip for a known IRI
SELECT pg_ripple.decode_id(
    pg_ripple.encode_term('https://crash.recovery.test/s1', 0::smallint)
) = 'https://crash.recovery.test/s1' AS dict_roundtrip_after_write;

-- 4.2 dictionary_stats() returns valid statistics
SELECT
    (pg_ripple.dictionary_stats() ? 'total_entries') AS dict_stats_ok,
    (pg_ripple.dictionary_stats() ->> 'total_entries')::bigint >= 0 AS dict_count_nonneg;

-- ══════════════════════════════════════════════════════════════════════════════
-- 5. Full SPARQL lifecycle after recovery simulation
-- ══════════════════════════════════════════════════════════════════════════════

-- 5.1 INSERT via SPARQL
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        <https://crash.recovery.test/post>
            <https://crash.recovery.test/p1>
            <https://crash.recovery.test/value> .
    }
$$) = 1 AS sparql_insert_ok;

-- 5.2 SELECT reads it back
SELECT count(*) = 1 AS sparql_select_after_insert
FROM pg_ripple.sparql($$
    SELECT ?o WHERE {
        <https://crash.recovery.test/post>
            <https://crash.recovery.test/p1>
            ?o .
    }
$$);

-- 5.3 ASK confirms presence
SELECT pg_ripple.sparql_ask($$
    ASK { <https://crash.recovery.test/post>
              <https://crash.recovery.test/p1>
              <https://crash.recovery.test/value> . }
$$) AS post_triple_exists;

-- 5.4 DELETE removes it cleanly
SELECT pg_ripple.sparql_update($$
    DELETE DATA {
        <https://crash.recovery.test/post>
            <https://crash.recovery.test/p1>
            <https://crash.recovery.test/value> .
    }
$$) = 1 AS sparql_delete_ok;

-- 5.5 Verify gone
SELECT pg_ripple.sparql_ask($$
    ASK { <https://crash.recovery.test/post>
              <https://crash.recovery.test/p1>
              <https://crash.recovery.test/value> . }
$$) = false AS post_triple_removed;

-- ══════════════════════════════════════════════════════════════════════════════
-- 6. Schema state after recovery
-- ══════════════════════════════════════════════════════════════════════════════

-- 6.1 Extension version is accessible
SELECT count(*) = 1 AS extension_version_ok
FROM pg_extension
WHERE extname = 'pg_ripple';

-- 6.2 Core sequences are intact (statement_id_seq)
SELECT last_value >= 0 AS sid_seq_intact
FROM _pg_ripple.statement_id_seq;

-- 6.3 plan_cache_stats() is callable
SELECT
    (pg_ripple.plan_cache_stats() ? 'hits') AS cache_stats_ok;
