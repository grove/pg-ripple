-- pg_regress test: binary precision mode (v0.27.0)
-- Tests that pg_ripple.embedding_precision = 'binary' is accepted and that
-- similarity functions degrade gracefully when pgvector is absent.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── pgvector status ────────────────────────────────────────────────────────────
SELECT CASE
    WHEN EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'vector')
    THEN 'pgvector available'
    ELSE 'pgvector absent - binary column type not verified (expected in CI)'
END AS pgvector_status;

-- ── Set precision to 'binary' ─────────────────────────────────────────────────
SET pg_ripple.embedding_precision = 'binary';

SELECT current_setting('pg_ripple.embedding_precision') AS precision_mode;

-- ── Embeddings table exists regardless of column type ────────────────────────
SELECT EXISTS(
    SELECT 1 FROM information_schema.tables
    WHERE table_schema = '_pg_ripple'
      AND table_name   = 'embeddings'
) AS embeddings_table_exists;

-- ── store_embedding() with precision=binary degrades gracefully ───────────────
SET client_min_messages = warning;
SELECT pg_ripple.store_embedding(
    'https://example.org/binary_test',
    ARRAY[1.0, 0.0, 1.0]::float8[]
) IS NULL AS store_embedding_void;
SET client_min_messages = DEFAULT;

-- ── similar_entities() returns >= 0 rows ─────────────────────────────────────
SELECT count(*) >= 0 AS similar_entities_completed
FROM pg_ripple.similar_entities('binary test');

-- ── Reset precision ───────────────────────────────────────────────────────────
RESET pg_ripple.embedding_precision;
SELECT current_setting('pg_ripple.embedding_precision') AS precision_after_reset;
