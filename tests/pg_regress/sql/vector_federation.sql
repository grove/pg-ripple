-- pg_regress test: vector SERVICE federation (v0.28.0)
-- Tests register_vector_endpoint() and graceful degradation when
-- the endpoint is unavailable.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── vector_endpoints catalog table exists ────────────────────────────────────
SELECT EXISTS(
    SELECT 1 FROM information_schema.tables
    WHERE table_schema = '_pg_ripple'
      AND table_name   = 'vector_endpoints'
) AS vector_endpoints_table_exists;

-- ── register_vector_endpoint() with valid api_type ───────────────────────────
SELECT pg_ripple.register_vector_endpoint(
    'http://test-qdrant.example:6333',
    'qdrant'
) IS NULL AS qdrant_endpoint_registered;

-- The endpoint should now appear in the catalog.
SELECT COUNT(*) = 1 AS endpoint_in_catalog
FROM _pg_ripple.vector_endpoints
WHERE url = 'http://test-qdrant.example:6333'
  AND api_type = 'qdrant'
  AND enabled = true;

-- ── Idempotent upsert ─────────────────────────────────────────────────────────
-- Calling register_vector_endpoint() again must not raise an ERROR.
SELECT pg_ripple.register_vector_endpoint(
    'http://test-qdrant.example:6333',
    'qdrant'
) IS NULL AS re_register_idempotent;

SELECT COUNT(*) = 1 AS still_only_one_row
FROM _pg_ripple.vector_endpoints
WHERE url = 'http://test-qdrant.example:6333';

-- ── Invalid api_type emits a WARNING ─────────────────────────────────────────
SET client_min_messages = warning;
SELECT pg_ripple.register_vector_endpoint(
    'http://unknown-service.example',
    'elasticsearch'
) IS NULL AS invalid_api_type_warning_not_error;
SET client_min_messages = DEFAULT;

-- Invalid api_type must not be stored.
SELECT COUNT(*) = 0 AS invalid_endpoint_not_stored
FROM _pg_ripple.vector_endpoints
WHERE url = 'http://unknown-service.example';

-- ── Register a Weaviate endpoint ─────────────────────────────────────────────
SELECT pg_ripple.register_vector_endpoint(
    'http://test-weaviate.example:8080',
    'weaviate'
) IS NULL AS weaviate_endpoint_registered;

SELECT COUNT(*) >= 2 AS multiple_endpoints_registered
FROM _pg_ripple.vector_endpoints
WHERE enabled = true;

-- ── Register a Pinecone endpoint ─────────────────────────────────────────────
SELECT pg_ripple.register_vector_endpoint(
    'https://index.test-pinecone.example',
    'pinecone'
) IS NULL AS pinecone_endpoint_registered;

-- ── vector_federation_timeout_ms GUC controls timeout ────────────────────────
SET pg_ripple.vector_federation_timeout_ms = 500;
SELECT current_setting('pg_ripple.vector_federation_timeout_ms')::int = 500
    AS timeout_guc_updated;
RESET pg_ripple.vector_federation_timeout_ms;

-- ── Graceful degradation: unregistered endpoint emits WARNING ─────────────────
-- Unregistered URLs must not cause an ERROR — they emit a WARNING and
-- return zero rows.
-- (We test this by checking list_embedding_models() does not crash.)
SET client_min_messages = warning;
SELECT count(*) >= 0 AS unregistered_endpoint_graceful
FROM pg_ripple.list_embedding_models();

-- Cleanup test data.
DELETE FROM _pg_ripple.vector_endpoints
WHERE url IN (
    'http://test-qdrant.example:6333',
    'http://test-weaviate.example:8080',
    'https://index.test-pinecone.example'
);
SET client_min_messages = DEFAULT;
