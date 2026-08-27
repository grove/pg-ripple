-- v0.130.0 JSON writeback public configuration API.
SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;
LOAD '$libdir/pg_ripple';

CREATE TABLE public.jwb_api_test (
    id INTEGER PRIMARY KEY,
    label TEXT,
    uuid_key UUID,
    payload JSONB
);
TRUNCATE public.jwb_api_test;

SELECT p.prosecdef AS jwb29_security_definer,
       EXISTS (
           SELECT 1 FROM unnest(p.proconfig) AS setting
           WHERE setting LIKE 'search_path=pg_catalog%'
       ) AS jwb29_safe_search_path
FROM pg_catalog.pg_proc p
JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
WHERE n.nspname = 'pg_ripple'
  AND p.proname = 'configure_json_writeback'
  AND pg_catalog.oidvectortypes(p.proargtypes) = 'text, text, text, text[], text';

-- JWB-30: valid configuration stores typed casts and disables stale triggers.
SELECT pg_ripple.register_json_mapping(
    'jwb_api',
    '{"id": "http://example.org/jwb/id", "label": "http://example.org/jwb/label", "uuid_key": "http://example.org/jwb/uuid_key", "payload": "http://example.org/jwb/payload"}'::jsonb
);
SELECT pg_ripple.configure_json_writeback(
    'jwb_api', 'public', 'jwb_api_test', ARRAY['id'], 'replace'
) AS jwb30_configured;

-- JWB-31: inspection exposes the stable public contract; configuration caches
-- casts for integer, UUID, and JSONB target columns.
SELECT target_schema, target_table, key_columns, conflict_policy,
       writeback_enabled, trigger_count, queue_depth
FROM pg_ripple.writeback_inspect('jwb_api');
SELECT writeback_column_casts->>'id' AS integer_type,
       writeback_column_casts->>'uuid_key' AS uuid_type,
       writeback_column_casts->>'payload' AS jsonb_type
FROM _pg_ripple.json_mappings
WHERE name = 'jwb_api';

-- JWB-32: nonexistent relation is rejected.
DO $$
BEGIN
    BEGIN
        PERFORM pg_ripple.configure_json_writeback(
            'jwb_api', 'public', 'jwb_missing', ARRAY['id'], 'error'
        );
        RAISE EXCEPTION 'unexpected success';
    EXCEPTION WHEN OTHERS THEN
        IF SQLERRM LIKE '%target relation%' THEN
            RAISE NOTICE 'JWB-32 PASS: missing target rejected';
        ELSE RAISE;
        END IF;
    END;
END;
$$;

-- JWB-33: missing key column is rejected.
DO $$
BEGIN
    BEGIN
        PERFORM pg_ripple.configure_json_writeback(
            'jwb_api', 'public', 'jwb_api_test', ARRAY['missing_id'], 'error'
        );
        RAISE EXCEPTION 'unexpected success';
    EXCEPTION WHEN OTHERS THEN
        IF SQLERRM LIKE '%does not exist%' THEN
            RAISE NOTICE 'JWB-33 PASS: missing key rejected';
        ELSE RAISE;
        END IF;
    END;
END;
$$;

-- JWB-34: duplicate and empty key names are rejected.
DO $$
BEGIN
    BEGIN
        PERFORM pg_ripple.configure_json_writeback(
            'jwb_api', 'public', 'jwb_api_test', ARRAY['id', 'id'], 'error'
        );
        RAISE EXCEPTION 'unexpected success';
    EXCEPTION WHEN OTHERS THEN
        IF SQLERRM LIKE '%unique, non-empty%' THEN
            RAISE NOTICE 'JWB-34 PASS: duplicate key rejected';
        ELSE RAISE;
        END IF;
    END;
END;
$$;

-- JWB-35: policy is an explicit enum, not an arbitrary catalog string.
DO $$
BEGIN
    BEGIN
        PERFORM pg_ripple.configure_json_writeback(
            'jwb_api', 'public', 'jwb_api_test', ARRAY['id'], 'merge'
        );
        RAISE EXCEPTION 'unexpected success';
    EXCEPTION WHEN OTHERS THEN
        IF SQLERRM LIKE '%conflict_policy%' THEN
            RAISE NOTICE 'JWB-35 PASS: invalid policy rejected';
        ELSE RAISE;
        END IF;
    END;
END;
$$;

-- JWB-36: UUID keys and skip policy are accepted; reconfiguration disables
-- stale triggers and refreshes the cached casts.
SELECT pg_ripple.configure_json_writeback(
    'jwb_api', 'public', 'jwb_api_test', ARRAY['uuid_key'], 'skip'
) AS jwb36_reconfigured;
SELECT key_columns, conflict_policy, writeback_enabled
FROM pg_ripple.writeback_inspect('jwb_api');

DELETE FROM _pg_ripple.json_mappings WHERE name = 'jwb_api';
DROP TABLE public.jwb_api_test;
