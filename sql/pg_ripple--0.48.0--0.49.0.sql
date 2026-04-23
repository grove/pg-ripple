-- Migration 0.48.0 → 0.49.0
--
-- Theme: AI & LLM Integration
--
-- New capabilities:
--
-- NL → SPARQL via LLM function calling (Feature C-1):
--   • pg_ripple.sparql_from_nl(question TEXT) RETURNS TEXT
--     Converts a natural-language question to a SPARQL SELECT query using a
--     configured OpenAI-compatible LLM endpoint.  Set llm_endpoint = 'mock'
--     for integration testing without an external LLM dependency.
--   • pg_ripple.add_llm_example(question TEXT, sparql TEXT)
--     Stores a few-shot example in _pg_ripple.llm_examples.
--   • New GUCs:
--       pg_ripple.llm_endpoint       (string, default '')
--       pg_ripple.llm_model          (string, default 'gpt-4o')
--       pg_ripple.llm_api_key_env    (string, default 'PG_RIPPLE_LLM_API_KEY')
--       pg_ripple.llm_include_shapes (bool,   default on)
--   • Error codes: PT700, PT701, PT702
--
-- Embedding-based owl:sameAs candidate generation (Feature C-2):
--   • pg_ripple.suggest_sameas(threshold REAL DEFAULT 0.9)
--     Returns TABLE(s1 TEXT, s2 TEXT, similarity REAL)
--     HNSW cosine self-join on _pg_ripple.embeddings.
--   • pg_ripple.apply_sameas_candidates(min_similarity REAL DEFAULT 0.95)
--     Inserts accepted pairs as owl:sameAs triples; respects sameas_max_cluster_size.

-- Schema change: add _pg_ripple.llm_examples table.
CREATE TABLE IF NOT EXISTS _pg_ripple.llm_examples (
    question    TEXT        NOT NULL PRIMARY KEY,
    sparql      TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
COMMENT ON TABLE _pg_ripple.llm_examples IS
    'Few-shot question/SPARQL examples for the NL-to-SPARQL LLM integration. '
    'Populated via pg_ripple.add_llm_example().';

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.49.0', '0.48.0')
ON CONFLICT DO NOTHING;
