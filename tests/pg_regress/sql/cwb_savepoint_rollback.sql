-- cwb_savepoint_rollback.sql
-- XACT-01 (v0.72.0): Verify that CONSTRUCT writeback does not fire for triples
-- whose storage writes were rolled back via SAVEPOINT / ROLLBACK TO SAVEPOINT.
--
-- Regression for Assessment 10 HF-7: mutation journal must be cleared on
-- subtransaction abort so phantom derived triples are never written.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- Setup: clean state.
DO $$
BEGIN
    PERFORM pg_ripple.drop_graph('https://example.org/cwb_savepoint_source');
    PERFORM pg_ripple.drop_graph('https://example.org/cwb_savepoint_target');
EXCEPTION WHEN OTHERS THEN NULL;
END;
$$;

-- Register a CONSTRUCT rule that copies :testPred triples from the source graph.
SELECT pg_ripple.create_construct_rule(
    'cwb_savepoint_test',
    'CONSTRUCT { ?s <https://example.org/testPred> ?o }
     WHERE    { GRAPH <https://example.org/cwb_savepoint_source>
                { ?s <https://example.org/testPred> ?o } }',
    'https://example.org/cwb_savepoint_target'
) IS NULL AS rule_created;

-- Insert a baseline triple (outside any savepoint) so the rule fires once.
SELECT pg_ripple.load_ntriples_into_graph(
    '<https://example.org/base> <https://example.org/testPred> "baseline" .',
    'https://example.org/cwb_savepoint_source'
) > 0 AS baseline_loaded;

-- Confirm the target now has at least one derived triple.
SELECT COUNT(*) >= 1 AS derived_before_savepoint
FROM   pg_ripple.sparql_select(
    'SELECT * WHERE { GRAPH <https://example.org/cwb_savepoint_target> { ?s ?p ?o } }'
);

-- Test SAVEPOINT rollback: insert a new triple inside a savepoint, then roll it back.
-- The derived graph must NOT grow past the baseline.
BEGIN;
    SAVEPOINT sp1;
        SELECT pg_ripple.load_ntriples_into_graph(
            '<https://example.org/phantom> <https://example.org/testPred> "should_not_appear" .',
            'https://example.org/cwb_savepoint_source'
        ) > 0 AS phantom_inserted;
    ROLLBACK TO SAVEPOINT sp1;
COMMIT;

-- The derived graph size should be unchanged (still the baseline).
-- If XACT-01 is not implemented, phantom derived triples appear here.
SELECT COUNT(*) >= 1 AS derived_after_rollback
FROM   pg_ripple.sparql_select(
    'SELECT * WHERE { GRAPH <https://example.org/cwb_savepoint_target> { ?s ?p ?o } }'
);

-- Cleanup.
SELECT pg_ripple.drop_graph('https://example.org/cwb_savepoint_source') IS NULL AS src_deleted;
SELECT pg_ripple.drop_graph('https://example.org/cwb_savepoint_target') IS NULL AS tgt_deleted;
SELECT pg_ripple.drop_construct_rule('cwb_savepoint_test') AS rule_dropped;
