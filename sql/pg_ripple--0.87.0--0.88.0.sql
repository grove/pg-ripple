-- Migration 0.87.0 → 0.88.0: Datalog-Native PageRank & Graph Analytics
--
-- New SQL schema objects:

-- PageRank scores table (PR-VIEW-01)
CREATE TABLE IF NOT EXISTS _pg_ripple.pagerank_scores (
    node         BIGINT       NOT NULL,
    topic        TEXT         NOT NULL DEFAULT '',
    score        FLOAT8       NOT NULL DEFAULT 0.0,
    score_lower  FLOAT8       NOT NULL DEFAULT 0.0,
    score_upper  FLOAT8       NOT NULL DEFAULT 0.0,
    computed_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    iterations   INT          NOT NULL DEFAULT 0,
    converged    BOOL         NOT NULL DEFAULT false,
    stale        BOOL         NOT NULL DEFAULT false,
    stale_since  TIMESTAMPTZ,
    PRIMARY KEY  (node, topic)
);

-- BRIN index for fast top-N queries (PR-VIEW-01)
CREATE INDEX IF NOT EXISTS pagerank_scores_topic_score_idx
    ON _pg_ripple.pagerank_scores USING BRIN (topic, score);

-- Dirty-edges queue for incremental K-hop refresh (PR-TRICKLE-01)
CREATE TABLE IF NOT EXISTS _pg_ripple.pagerank_dirty_edges (
    id           BIGSERIAL    PRIMARY KEY,
    source_id    BIGINT       NOT NULL,
    target_id    BIGINT       NOT NULL,
    delta        SMALLINT     NOT NULL DEFAULT 1,   -- +1 insert, -1 delete
    enqueued_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS pagerank_dirty_edges_enqueued_idx
    ON _pg_ripple.pagerank_dirty_edges (enqueued_at);

-- Centrality scores table (PR-CENTRALITY-01)
CREATE TABLE IF NOT EXISTS _pg_ripple.centrality_scores (
    node         BIGINT       NOT NULL,
    metric       TEXT         NOT NULL,
    score        FLOAT8       NOT NULL DEFAULT 0.0,
    computed_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    PRIMARY KEY  (node, metric)
);

-- RLS policies for pagerank_scores
ALTER TABLE _pg_ripple.pagerank_scores ENABLE ROW LEVEL SECURITY;
CREATE POLICY pagerank_scores_select ON _pg_ripple.pagerank_scores
    FOR SELECT USING (true);
CREATE POLICY pagerank_scores_write ON _pg_ripple.pagerank_scores
    FOR ALL USING (pg_has_role(current_user, 'pg_ripple', 'USAGE'))
    WITH CHECK (pg_has_role(current_user, 'pg_ripple', 'USAGE'));

-- RLS policies for centrality_scores
ALTER TABLE _pg_ripple.centrality_scores ENABLE ROW LEVEL SECURITY;
CREATE POLICY centrality_scores_select ON _pg_ripple.centrality_scores
    FOR SELECT USING (true);
CREATE POLICY centrality_scores_write ON _pg_ripple.centrality_scores
    FOR ALL USING (pg_has_role(current_user, 'pg_ripple', 'USAGE'))
    WITH CHECK (pg_has_role(current_user, 'pg_ripple', 'USAGE'));

-- New GUCs registered at extension load time (available immediately after ALTER EXTENSION UPDATE):
--   pg_ripple.pagerank_enabled              bool     default off
--     Master switch for the PageRank engine.
--   pg_ripple.pagerank_rules                text     default ''
--     Comma-separated IRI list of edge predicates. Empty = all object-valued predicates.
--   pg_ripple.pagerank_max_iterations       int      default 100
--     Maximum PageRank iteration count before termination.
--   pg_ripple.pagerank_convergence_delta    float8   default 0.0001
--     Convergence threshold: iteration stops when max delta < this value.
--   pg_ripple.pagerank_damping              float8   default 0.85
--     PageRank damping factor.
--   pg_ripple.pagerank_dangling_policy      text     default 'redistribute'
--     Dangling-node policy: 'redistribute' | 'ignore'.
--   pg_ripple.pagerank_include_blank_nodes  bool     default false
--     When off, blank nodes are excluded from computation.
--   pg_ripple.pagerank_on_demand            bool     default off
--     When on, pg:pagerank() triggers an on-demand run if the view is stale.
--   pg_ripple.pagerank_incremental          bool     default off
--     Enable incremental K-hop refresh via pg-trickle.
--   pg_ripple.pagerank_khop_limit           int      default 30
--     Maximum K-hop propagation depth for incremental updates.
--   pg_ripple.pagerank_refresh_schedule     text     default '0 3 * * *'
--     Cron expression for scheduled full pagerank_run().
--   pg_ripple.pagerank_confidence_weighted  bool     default off
--     Multiply edge weights by confidence from _pg_ripple.confidence.
--   pg_ripple.pagerank_confidence_default   float8   default 1.0
--     Default confidence weight for edges without a confidence row.
--   pg_ripple.pagerank_partition            bool     default off
--     Enable graph-partitioned parallel computation.
--   pg_ripple.pagerank_selective_threshold  float8   default 0.0
--     Minimum score below which dirty nodes skip immediate re-propagation.
--   pg_ripple.pagerank_federation_blend     bool     default off
--     Fetch remote SERVICE edges into a local temp graph before pagerank_run().
--   pg_ripple.pagerank_queue_warn_threshold int      default 100000
--     Log a WARNING when the dirty-edges queue exceeds this count.
--   pg_ripple.pagerank_trickle_confidence_attenuation  bool  default on
--     Attenuate K-hop rank deltas by edge confidence (PR-TRICKLE-CONF-01).
--   pg_ripple.pagerank_probabilistic        bool     default off
--     Enable probabilistic PageRank via @weight Datalog rules (PR-PROB-DATALOG-01).
--   pg_ripple.pagerank_shacl_threshold      float8   default 0.5
--     Exclude nodes whose shacl_score() is below this threshold (PR-SHACL-01).
--   pg_ripple.federation_minimum_confidence float8   default 0.5
--     Minimum confidence for remote SERVICE edges in federation blend mode (PR-FED-CONF-01).
--   pg_ripple.katz_alpha                    float8   default 0.01
--     Attenuation factor for Katz centrality.
--
-- New SQL functions (Rust-compiled):
--   pg_ripple.pagerank_run(edge_predicates TEXT[], damping FLOAT8, ...) RETURNS TABLE(...)
--   pg_ripple.pagerank_run_topics(topics TEXT[][]) RETURNS VOID
--   pg_ripple.vacuum_pagerank_dirty() RETURNS BIGINT
--   pg_ripple.pagerank_queue_stats() RETURNS TABLE(...)
--   pg_ripple.explain_pagerank(node_iri TEXT, top_k INT) RETURNS TABLE(...)
--   pg_ripple.export_pagerank(format TEXT, top_k INT, topic TEXT) RETURNS TEXT
--   pg_ripple.centrality_run(metric TEXT, ...) RETURNS TABLE(...)
--   pg_ripple.pagerank_find_duplicates(...) RETURNS TABLE(...)
--
-- New SPARQL functions:
--   pg:pagerank(?node)                — look up pagerank score for default topic
--   pg:pagerank(?node, ?topic)        — look up pagerank score for specific topic
--   pg:pagerank_lower(?node)          — lower confidence bound
--   pg:pagerank_upper(?node)          — upper confidence bound
--   pg:topN(?score, N)                — top-N nodes by score
--   pg:topN_approx(?node, K, error)   — approximate top-K via space-saving sketch
--   pg:explainPagerank(?node)         — score explanation as JSON literal
--   pg:centrality(?node, ?metric)     — centrality score by metric
--   pg:isStale(?node)                 — true when node score was K-hop updated

-- Rollback:
-- DROP TABLE IF EXISTS _pg_ripple.pagerank_scores CASCADE;
-- DROP TABLE IF EXISTS _pg_ripple.pagerank_dirty_edges CASCADE;
-- DROP TABLE IF EXISTS _pg_ripple.centrality_scores CASCADE;
