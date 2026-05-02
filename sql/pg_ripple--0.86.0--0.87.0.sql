-- Migration 0.86.0 → 0.87.0: Uncertain Knowledge Engine
--
-- New SQL schema objects:

-- Confidence side table (CONF-TABLE-01)
CREATE TABLE IF NOT EXISTS _pg_ripple.confidence (
    statement_id  BIGINT      NOT NULL,
    confidence    FLOAT8      NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    model         TEXT        NOT NULL DEFAULT 'datalog',
    asserted_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY   (statement_id, model)
);

CREATE INDEX IF NOT EXISTS confidence_stmt_idx
    ON _pg_ripple.confidence (statement_id);

-- Trigram index on dictionary for fuzzy SPARQL (FUZZY-SPARQL-01)
DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_trgm') THEN
    CREATE INDEX IF NOT EXISTS dict_trgm_idx
      ON _pg_ripple.dictionary USING GIN (value gin_trgm_ops);
  END IF;
END; $$;

-- SHACL quality score log table (SOFT-SHACL-01d)
CREATE TABLE IF NOT EXISTS _pg_ripple.shacl_score_log (
    graph_iri   TEXT        NOT NULL,
    score       FLOAT8      NOT NULL,
    measured_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- New GUCs registered at extension load time (available immediately after ALTER EXTENSION UPDATE):
--   pg_ripple.probabilistic_datalog   bool     default off
--     Enables @weight rule annotations and confidence propagation in Datalog evaluation.
--   pg_ripple.prob_datalog_cyclic     bool     default off
--     Allows probabilistic evaluation on cyclic rule sets (approximate; requires explicit opt-in).
--   pg_ripple.default_fuzzy_threshold float8   default 0.7
--     Default similarity threshold for pg:fuzzy_match() and pg:confPath() when omitted.
--   pg_ripple.prov_confidence         bool     default off
--     Enables automatic confidence propagation from PROV-O pg:sourceTrust predicates.
--
-- New SQL functions (Rust-compiled):
--   pg_ripple.load_triples_with_confidence(data TEXT, confidence FLOAT8 DEFAULT 1.0,
--       format TEXT DEFAULT 'ntriples', graph_uri TEXT DEFAULT NULL) RETURNS BIGINT
--   pg_ripple.shacl_score(graph_iri TEXT) RETURNS FLOAT8
--   pg_ripple.shacl_report_scored(graph_iri TEXT) RETURNS TABLE(...)
--   pg_ripple.log_shacl_score(graph_iri TEXT) RETURNS VOID
--
-- New SPARQL functions:
--   pg:confidence(?s, ?p, ?o)         — returns highest confidence across models (default 1.0)
--   pg:fuzzy_match(a, b)              — trigram similarity via pg_trgm.similarity()
--   pg:token_set_ratio(a, b)          — word-set similarity via pg_trgm.word_similarity()
--   pg:confPath(predicate, threshold) — confidence-threshold property path operator

-- CONF-RLS-01: Row Level Security policies for _pg_ripple.confidence
-- Allow all authenticated users to read confidence rows
ALTER TABLE _pg_ripple.confidence ENABLE ROW LEVEL SECURITY;
CREATE POLICY confidence_select ON _pg_ripple.confidence
    FOR SELECT USING (true);
-- Only superuser / pg_ripple role may insert/update/delete
CREATE POLICY confidence_write ON _pg_ripple.confidence
    FOR ALL USING (pg_has_role(current_user, 'pg_ripple', 'USAGE'))
    WITH CHECK (pg_has_role(current_user, 'pg_ripple', 'USAGE'));

-- CONF-RLS-01: Row Level Security for shacl_score_log
ALTER TABLE _pg_ripple.shacl_score_log ENABLE ROW LEVEL SECURITY;
CREATE POLICY shacl_score_log_select ON _pg_ripple.shacl_score_log
    FOR SELECT USING (true);
CREATE POLICY shacl_score_log_write ON _pg_ripple.shacl_score_log
    FOR ALL USING (pg_has_role(current_user, 'pg_ripple', 'USAGE'))
    WITH CHECK (pg_has_role(current_user, 'pg_ripple', 'USAGE'));
