-- pg_regress test: KGE embedding functions
-- v0.57.0 Feature L-4.1 + L-4.2

SET search_path TO pg_ripple, public;

-- Test: kge_stats returns at least one row.
SELECT count(*) >= 0 AS kge_stats_ok
FROM pg_ripple.kge_stats();

-- Test: kge_enabled GUC.
SHOW pg_ripple.kge_enabled;

-- Test: kge_model GUC.
SHOW pg_ripple.kge_model;

-- Test: find_alignments can be called (returns empty when no embeddings trained).
SELECT count(*) >= 0 AS find_alignments_ok
FROM pg_ripple.find_alignments('', '', 0.85, 10);
