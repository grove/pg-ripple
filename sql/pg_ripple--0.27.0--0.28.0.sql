-- Migration 0.27.0 → 0.28.0: Advanced Hybrid Search & RAG Pipeline
--
-- New SQL-level deliverables:
--
--   _pg_ripple.embedding_queue (entity_id, enqueued_at)
--     Populated by trigger on _pg_ripple.dictionary when pg_ripple.auto_embed = true.
--     Drained by the background worker in batches of pg_ripple.embedding_batch_size.
--
-- New SQL functions (compiled from Rust):
--   pg_ripple.hybrid_search(sparql_query, query_text, k, alpha, model)
--     → TABLE(entity_id, entity_iri, rrf_score, sparql_rank, vector_rank)
--   pg_ripple.contextualize_entity(entity_iri, depth, max_neighbors) → TEXT
--   pg_ripple.rag_retrieve(question, sparql_filter, k, model, output_format)
--     → TABLE(entity_iri, label, context_json, distance)
--   pg_ripple.list_embedding_models() → TABLE(model, entity_count, dimensions)
--   pg_ripple.add_embedding_triples() → BIGINT
--   pg_ripple.register_vector_endpoint(url, api_type) → VOID
--
-- New GUC parameters (registered in _PG_init):
--   pg_ripple.auto_embed                  (bool, default false)
--   pg_ripple.embedding_batch_size        (integer, default 100)
--   pg_ripple.use_graph_context           (bool, default false)
--   pg_ripple.vector_federation_timeout_ms (integer, default 5000)
--
-- New error codes:
--   PT607 — vector service endpoint not registered
--
-- pg_ripple_http new endpoint:
--   POST /rag  — calls pg_ripple.rag_retrieve() and formats context for LLM consumption;
--                supports output_format 'jsonb' (default) and 'jsonld'
--
-- No changes to VP table schema or the dictionary table.

CREATE TABLE IF NOT EXISTS _pg_ripple.embedding_queue (
    entity_id   BIGINT      NOT NULL PRIMARY KEY,
    enqueued_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Trigger function: enqueue new dictionary entries for embedding when auto_embed is on.
-- The GUC pg_ripple.auto_embed is checked at runtime inside the Rust-compiled trigger
-- body; this stub creates the queue table so the trigger can be attached by the
-- Rust _PG_init code on extension load.
COMMENT ON TABLE _pg_ripple.embedding_queue IS
    'Queue of entity_ids awaiting embedding by the background worker. '
    'Populated by a trigger on _pg_ripple.dictionary when pg_ripple.auto_embed = true.';
