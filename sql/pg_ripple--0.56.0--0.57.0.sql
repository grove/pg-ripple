-- Migration 0.56.0 → 0.57.0: OWL 2 EL/QL profiles, KGE embeddings, entity alignment,
--   LLM SPARQL repair, ontology mapping, multi-tenant isolation,
--   columnar VP storage, adaptive indexing, probabilistic Datalog
--
-- New SQL objects in this release:
--   • _pg_ripple.kge_embeddings     — knowledge-graph embedding vectors (v0.57.0 L-4.1)
--   • _pg_ripple.tenants            — tenant management catalog (v0.57.0 L-5.3)
--   • _pg_ripple.catalog_events     — extended with predicate_id column (v0.57.0 L-2.2)
--
-- New Rust-compiled SQL functions (no SQL migration needed):
--   • pg_ripple.kge_stats()               — KGE embedding statistics (L-4.1)
--   • pg_ripple.find_alignments()         — entity alignment via KGE (L-4.2)
--   • pg_ripple.repair_sparql()           — LLM-augmented SPARQL repair (L-4.3)
--   • pg_ripple.suggest_mappings()        — automated ontology mapping (L-4.4)
--   • pg_ripple.create_tenant()           — multi-tenant graph isolation (L-5.3)
--   • pg_ripple.drop_tenant()             — remove tenant (L-5.3)
--   • pg_ripple.tenant_stats()            — per-tenant statistics (L-5.3)
--
-- New built-in rule sets (no SQL migration needed):
--   • 'owl-el'  — OWL 2 EL profile rules (L-3.1)
--   • 'owl-ql'  — OWL 2 QL / DL-Lite rules (L-3.2)
--
-- New GUCs:
--   • pg_ripple.owl_profile                   (text, default NULL = 'RL')
--   • pg_ripple.probabilistic_datalog         (bool, default off)
--   • pg_ripple.kge_enabled                   (bool, default off)
--   • pg_ripple.kge_model                     (text, default 'transe')
--   • pg_ripple.columnar_threshold            (int, default -1 = disabled)
--   • pg_ripple.adaptive_indexing_enabled     (bool, default off)

-- L-4.1: KGE embeddings table.
-- Created with vector(64) — requires pgvector extension.
-- Silently skipped if pgvector is not installed.
DO $$
BEGIN
    CREATE TABLE IF NOT EXISTS _pg_ripple.kge_embeddings (
        entity_id   BIGINT      NOT NULL PRIMARY KEY,
        embedding   vector(64),
        model       TEXT        NOT NULL DEFAULT 'transe',
        trained_at  TIMESTAMPTZ NOT NULL DEFAULT now()
    );

    -- HNSW index for fast ANN similarity search.
    CREATE INDEX IF NOT EXISTS idx_kge_embeddings_hnsw
        ON _pg_ripple.kge_embeddings
        USING hnsw (embedding vector_cosine_ops);

EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'pg_ripple: kge_embeddings table creation skipped (pgvector not available): %', SQLERRM;
END;
$$;

-- L-5.3: Tenant management catalog.
CREATE TABLE IF NOT EXISTS _pg_ripple.tenants (
    tenant_name    TEXT        NOT NULL PRIMARY KEY,
    graph_iri      TEXT        NOT NULL,
    quota_triples  BIGINT      NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- L-2.2: Add predicate_id column to catalog_events for adaptive index advisor.
ALTER TABLE _pg_ripple.catalog_events
    ADD COLUMN IF NOT EXISTS predicate_id BIGINT;

-- Record migration in schema_version.
INSERT INTO _pg_ripple.schema_version (version, upgraded_from, installed_at)
VALUES ('0.57.0', '0.56.0', clock_timestamp());
