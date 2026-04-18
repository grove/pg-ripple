-- Migration 0.26.0 → 0.27.0: Vector + SPARQL Hybrid Foundation
--
-- New SQL-level deliverables (require pgvector to be installed):
--
--   _pg_ripple.embeddings (entity_id, model, embedding vector(N), updated_at)
--     HNSW index on (embedding vector_cosine_ops)
--
-- New SQL functions (compiled from Rust):
--   pg_ripple.embed_entities(graph_iri, model, batch_size) → BIGINT
--   pg_ripple.similar_entities(query_text, k, model) → TABLE(entity_id, entity_iri, distance)
--   pg_ripple.store_embedding(entity_iri, embedding, model) → VOID
--   pg_ripple.refresh_embeddings(graph_iri, model, force) → BIGINT
--
-- New GUC parameters (registered in _PG_init):
--   pg_ripple.embedding_model
--   pg_ripple.embedding_dimensions
--   pg_ripple.embedding_api_url
--   pg_ripple.embedding_api_key
--   pg_ripple.pgvector_enabled
--   pg_ripple.embedding_index_type   ('hnsw' | 'ivfflat', default 'hnsw')
--   pg_ripple.embedding_precision    ('single' | 'half', default 'single')
--
-- SPARQL extension function:
--   pg:similar(?entity, "query_text", k) — registered in the function registry
--
-- The embeddings table is created conditionally: if pgvector is not installed,
-- a stub table with BYTEA is created instead and all similarity functions
-- degrade gracefully (empty results + WARNING).

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector') THEN
        -- pgvector is present: create the full embeddings table
        EXECUTE $sql$
            CREATE TABLE IF NOT EXISTS _pg_ripple.embeddings (
                entity_id   BIGINT      NOT NULL,
                model       TEXT        NOT NULL DEFAULT 'default',
                embedding   vector(1536),
                updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (entity_id, model)
            );
            CREATE INDEX IF NOT EXISTS embeddings_hnsw_idx
                ON _pg_ripple.embeddings
                USING hnsw (embedding vector_cosine_ops);
        $sql$;
        RAISE NOTICE 'pg_ripple 0.27.0: embeddings table created with HNSW index (pgvector present)';
    ELSE
        -- pgvector is absent: create a stub table for forward compatibility
        EXECUTE $sql$
            CREATE TABLE IF NOT EXISTS _pg_ripple.embeddings (
                entity_id   BIGINT      NOT NULL,
                model       TEXT        NOT NULL DEFAULT 'default',
                embedding   BYTEA,
                updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (entity_id, model)
            );
        $sql$;
        RAISE WARNING 'pg_ripple 0.27.0: pgvector not installed — embeddings table created as stub; install pgvector to enable hybrid search';
    END IF;
END;
$$;
