-- pg_regress test: SPARQL federation performance (v0.19.0)
-- Tests for connection pooling GUCs, result caching, TTL expiry, variable
-- projection, batch SERVICE detection, complexity hints, partial result GUC,
-- adaptive timeout GUC, and deduplication correctness.
--
-- NOTE: These tests do NOT make real HTTP calls.
-- They verify the new GUCs, schema changes, and API surface introduced in v0.19.0.
--
-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
-- This line is a no-op when run after setup, but ensures the extension is
-- available when this file is run individually (during test discovery).
CREATE EXTENSION IF NOT EXISTS pg_ripple;

-- Load library and register GUCs by calling any pg_ripple function first.
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

-- ─── New GUCs exist and are settable ─────────────────────────────────────────

-- Pool size GUC (default: 4, range 1–32).
SET pg_ripple.federation_pool_size = 8;
SHOW pg_ripple.federation_pool_size;
RESET pg_ripple.federation_pool_size;
SHOW pg_ripple.federation_pool_size;

-- Cache TTL GUC (default: 0 = disabled).
SET pg_ripple.federation_cache_ttl = 300;
SHOW pg_ripple.federation_cache_ttl;
RESET pg_ripple.federation_cache_ttl;
SHOW pg_ripple.federation_cache_ttl;

-- On-partial GUC (default: empty string → 'empty' behaviour).
SET pg_ripple.federation_on_partial = 'use';
SHOW pg_ripple.federation_on_partial;
RESET pg_ripple.federation_on_partial;

-- Adaptive timeout GUC (default: off).
SET pg_ripple.federation_adaptive_timeout = on;
SHOW pg_ripple.federation_adaptive_timeout;
RESET pg_ripple.federation_adaptive_timeout;
SHOW pg_ripple.federation_adaptive_timeout;

-- ─── federation_cache table exists ───────────────────────────────────────────

SELECT EXISTS(
    SELECT 1 FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname = '_pg_ripple' AND c.relname = 'federation_cache'
) AS cache_table_exists;

-- Index on expires_at must exist.
SELECT EXISTS(
    SELECT 1 FROM pg_indexes
    WHERE schemaname = '_pg_ripple'
      AND tablename = 'federation_cache'
      AND indexname = 'idx_federation_cache_expires'
) AS cache_index_exists;

-- ─── federation_endpoints has complexity column ───────────────────────────────

SELECT column_name, data_type, column_default
FROM information_schema.columns
WHERE table_schema = '_pg_ripple'
  AND table_name = 'federation_endpoints'
  AND column_name = 'complexity';

-- ─── register_endpoint with complexity argument ───────────────────────────────

SELECT pg_ripple.register_endpoint(
    'http://fast.sparql.test/endpoint',
    NULL,
    'fast'
);

SELECT url, enabled, complexity
FROM pg_ripple.list_endpoints()
WHERE url = 'http://fast.sparql.test/endpoint';

SELECT pg_ripple.register_endpoint(
    'http://slow.sparql.test/endpoint',
    NULL,
    'slow'
);

SELECT url, complexity
FROM pg_ripple.list_endpoints()
WHERE url = 'http://slow.sparql.test/endpoint';

-- ─── set_endpoint_complexity ─────────────────────────────────────────────────

SELECT pg_ripple.set_endpoint_complexity('http://fast.sparql.test/endpoint', 'normal');

SELECT url, complexity
FROM pg_ripple.list_endpoints()
WHERE url = 'http://fast.sparql.test/endpoint';

-- ─── Cache TTL disabled by default → cache table remains empty ───────────────

-- With cache_ttl = 0 (default), no rows should be in federation_cache.
SELECT COUNT(*) AS cache_rows_when_disabled
FROM _pg_ripple.federation_cache;

-- ─── Cache row insertion and expiry (TTL path) ───────────────────────────────

-- Insert a test row directly to verify the schema and expiry index.
INSERT INTO _pg_ripple.federation_cache (url, query_hash, result_jsonb, expires_at)
VALUES ('http://test.cache/sparql', 12345, '{"head":{"vars":[]},"results":{"bindings":[]}}',
        now() - INTERVAL '1 second');

-- One row, already expired.
SELECT COUNT(*) AS expired_rows FROM _pg_ripple.federation_cache
WHERE url = 'http://test.cache/sparql' AND expires_at <= now();

-- Evict expired rows.
DELETE FROM _pg_ripple.federation_cache WHERE expires_at <= now();
SELECT COUNT(*) AS rows_after_eviction FROM _pg_ripple.federation_cache;

-- ─── Variable projection: explicit vars in sent SPARQL query ─────────────────
-- Register an unreachable endpoint and confirm the WARNING contains the
-- explicit projection (SELECT ?o ?p ?s) rather than SELECT *.
SELECT pg_ripple.register_endpoint('http://127.0.0.1:19998/sparql');
SET pg_ripple.federation_on_error = 'empty';
SET pg_ripple.federation_timeout = 1;

SELECT COUNT(*) AS projection_test_count
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { SERVICE <http://127.0.0.1:19998/sparql> { ?s ?p ?o } }'
);

RESET pg_ripple.federation_on_error;
RESET pg_ripple.federation_timeout;

-- ─── Partial result GUC accepted ─────────────────────────────────────────────

SET pg_ripple.federation_on_partial = 'use';
SET pg_ripple.federation_on_error = 'empty';
SET pg_ripple.federation_timeout = 1;

SELECT COUNT(*) AS partial_use_empty_count
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { SERVICE <http://127.0.0.1:19998/sparql> { ?s ?p ?o } }'
);

RESET pg_ripple.federation_on_partial;
RESET pg_ripple.federation_on_error;
RESET pg_ripple.federation_timeout;

-- ─── Adaptive timeout GUC boundary ───────────────────────────────────────────

-- With no health data, adaptive timeout falls back to federation_timeout.
SET pg_ripple.federation_adaptive_timeout = on;
SET pg_ripple.federation_on_error = 'empty';
SET pg_ripple.federation_timeout = 1;

SELECT COUNT(*) AS adaptive_fallback_count
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { SERVICE <http://127.0.0.1:19998/sparql> { ?s ?p ?o } }'
);

RESET pg_ripple.federation_adaptive_timeout;
RESET pg_ripple.federation_on_error;
RESET pg_ripple.federation_timeout;

-- ─── Deduplication correctness ───────────────────────────────────────────────
-- Insert a triple and verify round-trip via SPARQL still works (encode_results
-- deduplication must not produce different IDs from the non-deduped path).
SELECT pg_ripple.insert_triple(
    '<http://example.org/dedup-s>',
    '<http://example.org/dedup-p>',
    '<http://example.org/dedup-o>'
) > 0 AS inserted;

-- Verify 1 result row is returned for the inserted triple.
SELECT COUNT(*) = 1 AS dedup_round_trip_ok
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { ?s <http://example.org/dedup-p> <http://example.org/dedup-o> }'
);

-- ─── Cleanup ──────────────────────────────────────────────────────────────────

SELECT pg_ripple.remove_endpoint('http://fast.sparql.test/endpoint');
SELECT pg_ripple.remove_endpoint('http://slow.sparql.test/endpoint');
SELECT pg_ripple.remove_endpoint('http://127.0.0.1:19998/sparql');

SELECT COUNT(*) AS endpoints_after_cleanup
FROM _pg_ripple.federation_endpoints;
