-- pg_regress test: Admin API functions (stats, version, vacuum)

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- 1. stats() returns a JSONB object.
SELECT pg_typeof(pg_ripple.stats()) = 'jsonb'::regtype AS stats_is_jsonb;

-- 2. stats() contains total_triples.
SELECT (pg_ripple.stats() ? 'total_triples') AS stats_has_triple_count;

-- 3. triple_count() is consistent with stats().
SELECT pg_ripple.triple_count() = (pg_ripple.stats() ->> 'total_triples')::bigint AS counts_consistent;

-- 4. Extension version is available.
SELECT length(extversion) > 0 AS version_nonempty
FROM pg_extension
WHERE extname = 'pg_ripple';

-- 5. register_prefix and lookup via prefixes() work.
DO $$ BEGIN PERFORM pg_ripple.register_prefix('ex76', 'https://ex76.test/'); END $$;
SELECT COUNT(*) = 1 AS prefix_registered
FROM pg_ripple.prefixes()
WHERE prefix = 'ex76';
SELECT expansion = 'https://ex76.test/' AS prefix_lookup_ok
FROM pg_ripple.prefixes()
WHERE prefix = 'ex76';

-- 6. vacuum() function exists and completes.
SELECT pg_ripple.vacuum() IS NOT NULL AS vacuum_ok;

-- 7. list_rules() returns a result (even if empty).
SELECT COUNT(*) >= 0 AS list_rules_ok
FROM pg_ripple.list_rules();
