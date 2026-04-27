-- pg_regress test: SPARQL CONSTRUCT writeback rules (v0.63.0)
--
-- Tests: catalog table existence; list_construct_rules empty initially;
-- wrong query form rejected; blank node in template rejected;
-- unbound variable rejected; SELECT query rejected; lifecycle functions exist.

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
