-- pg_regress test: refresh_embeddings() (v0.27.0)
-- Tests that refresh_embeddings() completes without ERROR in all configurations.
-- With pgvector absent or API URL not configured, it returns 0 gracefully.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── pgvector status ────────────────────────────────────────────────────────────
SELECT CASE
    WHEN EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'vector')
    THEN 'pgvector available'
    ELSE 'pgvector absent - refresh_embeddings returns 0 (expected in CI)'
END AS pgvector_status;

-- ── Insert an entity to refresh ───────────────────────────────────────────────

SELECT pg_ripple.load_ntriples(
    '<https://example.org/refresh_entity> '
    '<http://www.w3.org/2000/01/rdf-schema#label> '
    '"refresh target" .' || chr(10)
) >= 1 AS entity_loaded;

-- ── refresh_embeddings() must return 0 when API URL is unconfigured ───────────
-- PT601 — embedding API URL not configured — triggers a WARNING, not an ERROR.
SELECT pg_ripple.refresh_embeddings() = 0 AS refresh_zero_without_api;

-- ── refresh_embeddings(force := true) also returns 0 safely ──────────────────
SELECT pg_ripple.refresh_embeddings(force := true) = 0 AS force_refresh_zero_without_api;

-- ── refresh_embeddings() with pgvector disabled returns 0 ────────────────────
SET pg_ripple.pgvector_enabled = off;
SELECT pg_ripple.refresh_embeddings() = 0 AS refresh_zero_when_disabled;
RESET pg_ripple.pgvector_enabled;

-- ── embed_entities() also returns 0 without API URL ──────────────────────────
SELECT pg_ripple.embed_entities() = 0 AS embed_zero_without_api;
