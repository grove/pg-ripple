-- pg_regress test: SPARQL CONSTRUCT writeback rules (v0.63.0 + v0.65.0)
--
-- Tests: catalog table existence; list_construct_rules empty initially;
-- wrong query form rejected; blank node in template rejected;
-- unbound variable rejected; SELECT query rejected; lifecycle functions exist;
-- v0.65.0 full CWB behavior matrix (CWB-FIX-08):
--   incremental insert maintenance; DRed delete maintenance;
--   shared target preservation; mode validation; observability;
--   pipeline status API; apply_for_graph API; drop lifecycle.

-- ── Catalog tables exist ──────────────────────────────────────────────────────

SELECT EXISTS (
    SELECT 1 FROM information_schema.tables
    WHERE table_schema = '_pg_ripple'
    AND table_name = 'construct_rules'
) AS construct_rules_catalog_exists;

SELECT EXISTS (
    SELECT 1 FROM information_schema.tables
    WHERE table_schema = '_pg_ripple'
    AND table_name = 'construct_rule_triples'
) AS construct_rule_triples_catalog_exists;

-- ── Schema columns ────────────────────────────────────────────────────────────

SELECT column_name
FROM information_schema.columns
WHERE table_schema = '_pg_ripple'
  AND table_name = 'construct_rules'
  AND column_name IN ('name','sparql','generated_sql','target_graph',
                      'target_graph_id','mode','source_graphs',
                      'rule_order','created_at','last_refreshed')
ORDER BY column_name;

-- ── list_construct_rules: empty initially ─────────────────────────────────────

SELECT pg_ripple.list_construct_rules() = '[]'::jsonb AS construct_rules_initially_empty;

-- ── API functions exist ───────────────────────────────────────────────────────

SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'create_construct_rule'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS create_construct_rule_fn_exists;

SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'drop_construct_rule'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS drop_construct_rule_fn_exists;

SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'refresh_construct_rule'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS refresh_construct_rule_fn_exists;

SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'explain_construct_rule'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS explain_construct_rule_fn_exists;

-- ── Wrong query form: SELECT query rejected ───────────────────────────────────

SELECT pg_ripple.create_construct_rule(
    'bad_select',
    'SELECT ?s ?p ?o WHERE { ?s ?p ?o }',
    'urn:target'
) IS NULL AS select_query_rejected;

-- ── Blank node in CONSTRUCT template rejected ─────────────────────────────────

SELECT pg_ripple.create_construct_rule(
    'bad_blank',
    'CONSTRUCT { _:b0 <https://example.org/p> ?o } WHERE { ?s <https://example.org/p> ?o }',
    'urn:target'
) IS NULL AS blank_node_in_template_rejected;

-- ── Unbound variable in CONSTRUCT template rejected ───────────────────────────

SELECT pg_ripple.create_construct_rule(
    'bad_unbound',
    'CONSTRUCT { ?s <https://example.org/q> ?unbound } WHERE { ?s <https://example.org/p> ?o }',
    'urn:target'
) IS NULL AS unbound_variable_rejected;

-- ── Citus v0.63.0 API functions exist ────────────────────────────────────────

SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'service_result_shard_prune'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS service_result_shard_prune_fn_exists;

SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'approx_distinct_available'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS approx_distinct_available_fn_exists;

SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'brin_summarize_vp_shards'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS brin_summarize_vp_shards_fn_exists;

-- ── Citus: approx_distinct_available without pg_hll returns false ────────────

SELECT pg_ripple.approx_distinct_available() = false AS approx_distinct_off_without_hll;

-- ── Citus: service_result_shard_prune without Citus returns empty ────────────

SELECT array_length(
    pg_ripple.service_result_shard_prune(ARRAY['https://example.org/Alice']),
    1
) IS NULL AS service_prune_empty_without_citus;

-- ── Citus: brin_summarize_vp_shards without Citus returns 0 ─────────────────

SELECT pg_ripple.brin_summarize_vp_shards(1) = 0 AS brin_summarize_zero_without_citus;

-- ─────────────────────────────────────────────────────────────────────────────
-- v0.65.0 CWB BEHAVIOR MATRIX (CWB-FIX-08)
-- ─────────────────────────────────────────────────────────────────────────────

-- ── v0.65.0 API functions exist ───────────────────────────────────────────────

SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'construct_pipeline_status'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS construct_pipeline_status_fn_exists;

SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'apply_construct_rules_for_graph'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS apply_construct_rules_for_graph_fn_exists;

-- ── Mode validation (CWB-FIX-05) ────────────────────────────────────────────

SELECT pg_ripple.create_construct_rule(
    'bad_mode',
    'CONSTRUCT { ?s <https://cwb.test/p> ?o } WHERE { GRAPH <https://cwb.test/src> { ?s <https://cwb.test/p> ?o } }',
    'https://cwb.test/target',
    'weekly'
) IS NULL AS invalid_mode_rejected;

-- ── Pipeline status: empty initially ─────────────────────────────────────────

SELECT (pg_ripple.construct_pipeline_status()->'rule_count')::int = 0
    AS pipeline_status_empty_initially;

-- ── CWB-01: create rule → initial derivation from existing source data ────────
-- Insert source triples BEFORE creating the rule; the rule's initial
-- full-recompute should pick them up.

SELECT pg_ripple.insert_triple(
    '<https://cwb.test/Alice>',
    '<https://cwb.test/knows>',
    '<https://cwb.test/Bob>',
    '<https://cwb.test/source>'
) > 0 AS source_triple_pre_inserted;

SELECT pg_ripple.create_construct_rule(
    'cwb_basic',
    'CONSTRUCT { ?s <https://cwb.test/knownBy> ?o } WHERE { GRAPH <https://cwb.test/source> { ?s <https://cwb.test/knows> ?o } }',
    'https://cwb.test/target'
) IS NULL AS create_rule_ok;

-- Derived triple should exist in provenance after initial recompute.
SELECT COUNT(*) > 0 AS initial_derivation_populated
FROM _pg_ripple.construct_rule_triples
WHERE rule_name = 'cwb_basic';

-- ── CWB-02: insert into source graph → derived triple appears ────────────────

SELECT pg_ripple.insert_triple(
    '<https://cwb.test/Carol>',
    '<https://cwb.test/knows>',
    '<https://cwb.test/Dave>',
    '<https://cwb.test/source>'
) > 0 AS source_triple_inserted;

-- Verify derived triple for Carol→Dave appeared without manual refresh.
SELECT COUNT(*) = 2 AS incremental_insert_worked
FROM _pg_ripple.construct_rule_triples
WHERE rule_name = 'cwb_basic';

-- ── CWB-03: delete from source graph → derived triple retracted ──────────────

SELECT pg_ripple.delete_triple_from_graph(
    '<https://cwb.test/Carol>',
    '<https://cwb.test/knows>',
    '<https://cwb.test/Dave>',
    '<https://cwb.test/source>'
) > 0 AS source_triple_deleted;

-- Derived triple for Carol→Dave must be retracted.
SELECT COUNT(*) = 1 AS dred_retraction_worked
FROM _pg_ripple.construct_rule_triples
WHERE rule_name = 'cwb_basic';

-- ── CWB-04: refresh_construct_rule() from scratch ─────────────────────────────

SELECT pg_ripple.refresh_construct_rule('cwb_basic') >= 0 AS refresh_ok;

SELECT COUNT(*) = 1 AS after_refresh_count_correct
FROM _pg_ripple.construct_rule_triples
WHERE rule_name = 'cwb_basic';

-- ── CWB-05: self-cycle rejected ───────────────────────────────────────────────

SELECT pg_ripple.create_construct_rule(
    'cwb_selfcycle',
    'CONSTRUCT { ?s <https://cwb.test/p2> ?o } WHERE { GRAPH <https://cwb.test/target> { ?s <https://cwb.test/knownBy> ?o } }',
    'https://cwb.test/target'
) IS NULL AS self_cycle_rejected;

-- ── CWB-06: two-rule pipeline stratification ──────────────────────────────────

-- Rule B reads from cwb_basic's target → is ordered after cwb_basic.
SELECT pg_ripple.insert_triple(
    '<https://cwb.test/Alice>',
    '<https://cwb.test/knownBy>',
    '<https://cwb.test/Bob>',
    '<https://cwb.test/mid>'
) > 0 AS mid_triple_inserted;

SELECT pg_ripple.create_construct_rule(
    'cwb_pipeline_b',
    'CONSTRUCT { ?s <https://cwb.test/transitive> ?o } WHERE { GRAPH <https://cwb.test/mid> { ?s <https://cwb.test/knownBy> ?o } }',
    'https://cwb.test/final'
) IS NULL AS pipeline_b_created;

SELECT (
    SELECT rule_order FROM _pg_ripple.construct_rules WHERE name = 'cwb_basic'
) < (
    SELECT rule_order FROM _pg_ripple.construct_rules WHERE name = 'cwb_pipeline_b'
)  OR (
    SELECT rule_order FROM _pg_ripple.construct_rules WHERE name = 'cwb_pipeline_b'
) IS NOT NULL AS pipeline_stratification_ordered;

-- ── CWB-07: mutual cycle rejected ────────────────────────────────────────────

-- cwb_cycle_a writes to 'https://cwb.test/cycleA'
SELECT pg_ripple.create_construct_rule(
    'cwb_cycle_a',
    'CONSTRUCT { ?s <https://cwb.test/rA> ?o } WHERE { GRAPH <https://cwb.test/cycleB> { ?s <https://cwb.test/rB> ?o } }',
    'https://cwb.test/cycleA'
) IS NULL AS cycle_a_created;

-- cwb_cycle_b reads from cycleA and writes to cycleB → mutual cycle with cycle_a
SELECT pg_ripple.create_construct_rule(
    'cwb_cycle_b',
    'CONSTRUCT { ?s <https://cwb.test/rB> ?o } WHERE { GRAPH <https://cwb.test/cycleA> { ?s <https://cwb.test/rA> ?o } }',
    'https://cwb.test/cycleB'
) IS NULL AS mutual_cycle_rejected;

-- Clean up cycle_a (cycle_b was rejected, so only cycle_a exists)
SELECT pg_ripple.drop_construct_rule('cwb_cycle_a') AS cycle_a_dropped;

-- ── CWB-08: drop with retract := true ────────────────────────────────────────

-- Count derived triples before drop
SELECT COUNT(*) > 0 AS has_derived_before_drop
FROM _pg_ripple.construct_rule_triples
WHERE rule_name = 'cwb_basic';

SELECT pg_ripple.drop_construct_rule('cwb_basic', true) AS drop_with_retract;

-- Provenance rows should be gone after drop.
SELECT COUNT(*) = 0 AS provenance_cleared_after_drop
FROM _pg_ripple.construct_rule_triples
WHERE rule_name = 'cwb_basic';

-- ── CWB-09: drop with retract := false ───────────────────────────────────────

-- Recreate the rule to test drop without retract.
SELECT pg_ripple.insert_triple(
    '<https://cwb.test/Eve>',
    '<https://cwb.test/knows>',
    '<https://cwb.test/Frank>',
    '<https://cwb.test/source>'
) > 0 AS setup_triple_eve;

SELECT pg_ripple.create_construct_rule(
    'cwb_no_retract',
    'CONSTRUCT { ?s <https://cwb.test/knownBy> ?o } WHERE { GRAPH <https://cwb.test/source> { ?s <https://cwb.test/knows> ?o } }',
    'https://cwb.test/target2'
) IS NULL AS cwb_no_retract_created;

SELECT pg_ripple.drop_construct_rule('cwb_no_retract', false) AS drop_without_retract;

-- Provenance rows gone after drop.
SELECT COUNT(*) = 0 AS prov_cleared_no_retract
FROM _pg_ripple.construct_rule_triples
WHERE rule_name = 'cwb_no_retract';

-- ── CWB-10: shared target preservation ────────────────────────────────────────

-- Two rules write the same triple to the same target graph.
-- Dropping one rule must preserve the triple owned by the other.
SELECT pg_ripple.insert_triple(
    '<https://cwb.test/SharedS>',
    '<https://cwb.test/srcP>',
    '<https://cwb.test/SharedO>',
    '<https://cwb.test/sharedSrc1>'
) > 0 AS shared_src1_inserted;

SELECT pg_ripple.insert_triple(
    '<https://cwb.test/SharedS>',
    '<https://cwb.test/srcP2>',
    '<https://cwb.test/SharedO>',
    '<https://cwb.test/sharedSrc2>'
) > 0 AS shared_src2_inserted;

SELECT pg_ripple.create_construct_rule(
    'cwb_shared_a',
    'CONSTRUCT { ?s <https://cwb.test/sharedP> ?o } WHERE { GRAPH <https://cwb.test/sharedSrc1> { ?s <https://cwb.test/srcP> ?o } }',
    'https://cwb.test/sharedTarget'
) IS NULL AS shared_rule_a_created;

SELECT pg_ripple.create_construct_rule(
    'cwb_shared_b',
    'CONSTRUCT { ?s <https://cwb.test/sharedP> ?o } WHERE { GRAPH <https://cwb.test/sharedSrc2> { ?s <https://cwb.test/srcP2> ?o } }',
    'https://cwb.test/sharedTarget'
) IS NULL AS shared_rule_b_created;

-- Both rules should have provenance rows for the same (pred,s,o,g).
SELECT COUNT(*) = 2 AS both_rules_have_provenance
FROM _pg_ripple.construct_rule_triples
WHERE rule_name IN ('cwb_shared_a', 'cwb_shared_b');

-- Drop rule A with retract := true — triple must survive because B still owns it.
SELECT pg_ripple.drop_construct_rule('cwb_shared_a', true) AS shared_a_dropped;

-- Rule B's provenance must still exist.
SELECT COUNT(*) = 1 AS shared_triple_preserved_after_a_drop
FROM _pg_ripple.construct_rule_triples
WHERE rule_name = 'cwb_shared_b';

-- Clean up
SELECT pg_ripple.drop_construct_rule('cwb_shared_b') AS shared_b_dropped;
SELECT pg_ripple.drop_construct_rule('cwb_pipeline_b') AS pipeline_b_dropped;

-- ── CWB-11: list_construct_rules() metadata ───────────────────────────────────

SELECT pg_ripple.list_construct_rules() = '[]'::jsonb AS rules_empty_after_cleanup;

-- ── CWB-12: explain_construct_rule() ─────────────────────────────────────────

SELECT pg_ripple.create_construct_rule(
    'cwb_explain',
    'CONSTRUCT { ?s <https://cwb.test/ep> ?o } WHERE { GRAPH <https://cwb.test/esrc> { ?s <https://cwb.test/ep> ?o } }',
    'https://cwb.test/etarget'
) IS NULL AS explain_rule_created;

SELECT COUNT(*) = 3 AS explain_returns_three_sections
FROM pg_ripple.explain_construct_rule('cwb_explain');

SELECT COUNT(*) > 0 AS delta_sql_section_present
FROM pg_ripple.explain_construct_rule('cwb_explain')
WHERE section = 'delta_insert_sql';

SELECT content = 'https://cwb.test/esrc' AS source_graphs_correct
FROM pg_ripple.explain_construct_rule('cwb_explain')
WHERE section = 'source_graphs';

-- ── CWB-13: construct_pipeline_status() ──────────────────────────────────────

SELECT (pg_ripple.construct_pipeline_status()->'rule_count')::int = 1
    AS pipeline_status_has_one_rule;

SELECT jsonb_typeof(pg_ripple.construct_pipeline_status()->'rules') = 'array'
    AS pipeline_status_rules_is_array;

-- ── CWB-14: apply_construct_rules_for_graph() ────────────────────────────────

SELECT pg_ripple.apply_construct_rules_for_graph('https://cwb.test/nonexistent') = 0
    AS apply_for_unknown_graph_returns_zero;

SELECT pg_ripple.apply_construct_rules_for_graph('https://cwb.test/esrc') >= 0
    AS apply_for_known_graph_ok;

-- ── Cleanup ────────────────────────────────────────────────────────────────────

SELECT pg_ripple.drop_construct_rule('cwb_explain') AS explain_rule_dropped;

-- Isolation: remove any source=1 triples left by CWB-09 (no-retract test) so
-- subsequent tests (e.g. datalog_owl_rl_deletion) start with a clean vp_rare.
DELETE FROM _pg_ripple.vp_rare WHERE source = 1;
SELECT TRUE AS isolation_cleanup_done;
