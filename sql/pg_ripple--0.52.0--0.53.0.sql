-- Migration 0.52.0 → 0.53.0: DX, Extended Standards & Architecture
-- See CHANGELOG.md for the full feature list.
--
-- New schema objects:
--   • _pg_ripple.rag_cache — RAG answer cache for pg_ripple.rag_context()
--
-- New SQL functions (compiled from Rust, no SQL DDL required):
--   • pg_ripple.copy_rdf_from(path TEXT, format TEXT DEFAULT 'ntriples') → BIGINT
--     — Load RDF files from server-side paths (ntriples, nquads, turtle, trig, rdfxml)
--
-- New error codes (PT480, PT481):
--   • PT480 — sh:rule detected with inference disabled
--   • PT481 — SHACL-SPARQL constraint query execution failed
--
-- CDC lifecycle events:
--   • pg_notify('pg_ripple_cdc_lifecycle', payload) emitted after each HTAP merge cycle

CREATE TABLE IF NOT EXISTS _pg_ripple.rag_cache (
    question_hash TEXT         NOT NULL,
    k             INT          NOT NULL DEFAULT 10,
    schema_digest TEXT         NOT NULL DEFAULT '',
    result        TEXT         NOT NULL DEFAULT '',
    cached_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (question_hash, k, schema_digest)
);

CREATE INDEX IF NOT EXISTS idx_rag_cache_cached_at
    ON _pg_ripple.rag_cache (cached_at);

-- Ensure a unique index exists on version so ON CONFLICT (version) works.
-- The table was originally created without one (v0.37.0); this backfills it.
CREATE UNIQUE INDEX IF NOT EXISTS schema_version_version_key
    ON _pg_ripple.schema_version (version);

INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
VALUES ('0.53.0', '0.52.0', clock_timestamp())
ON CONFLICT (version) DO NOTHING;
