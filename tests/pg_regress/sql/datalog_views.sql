-- pg_regress test: Datalog views (v0.11.0)
--
-- Tests the Datalog view catalog and lifecycle, including the
-- create_datalog_view_from_rule_set() function which references
-- a previously loaded rule set.

SET search_path TO pg_ripple, public;

-- ── Catalog baseline ──────────────────────────────────────────────────────────

-- No Datalog views registered at start.
SELECT pg_ripple.list_datalog_views() = '[]'::jsonb AS no_datalog_views_yet;

-- ── create_datalog_view_from_rule_set requires pg_trickle ────────────────────
-- We can only test the error branch here when pg_trickle is absent.
-- When pg_trickle IS present the behavior is integration-tested separately.

-- Verify that create_datalog_view_from_rule_set exists and has the right args.
SELECT COUNT(*) >= 1 AS rule_set_variant_exists
FROM pg_proc p
JOIN pg_namespace n ON n.oid = p.pronamespace
WHERE n.nspname = 'pg_ripple'
  AND p.proname = 'create_datalog_view_from_rule_set';

-- ── load_and_store_rules path (no pg_trickle needed) ─────────────────────────
-- Load some triples to act as base data.
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
) > 0 AS alice_knows_bob;

SELECT pg_ripple.insert_triple(
    '<https://example.org/bob>',
    '<https://example.org/knows>',
    '<https://example.org/carol>'
) > 0 AS bob_knows_carol;

-- Load a transitive rule set (same as the datalog_custom test).
SELECT pg_ripple.load_rules_builtin('rdfs') > 0 AS rdfs_loaded_for_view_test;

-- Verify the rule set is active.
SELECT EXISTS(
    SELECT 1 FROM _pg_ripple.rule_sets
    WHERE name = 'rdfs' AND active = true
) AS rdfs_rule_set_active;

-- ── Catalog: datalog_views table ─────────────────────────────────────────────

-- Direct catalog INSERT to simulate a registered Datalog view (without pg_trickle).
INSERT INTO _pg_ripple.datalog_views
  (name, rules, rule_set, goal, generated_sql, schedule, decode, stream_table, variables)
VALUES
  ('test_dl_view',
   NULL,
   'rdfs',
   'SELECT ?s ?o WHERE { ?s <http://www.w3.org/2000/01/rdf-schema#subClassOf> ?o }',
   'SELECT _t0.s AS s, _t0.o AS o FROM _pg_ripple.vp_rare AS _t0 WHERE _t0.p = 0',
   '10s',
   false,
   'pg_ripple.test_dl_view',
   '["s","o"]'::jsonb)
ON CONFLICT (name) DO NOTHING;

-- list_datalog_views() should now return one entry.
SELECT jsonb_array_length(pg_ripple.list_datalog_views()) = 1 AS one_datalog_view;

-- The entry should have the correct name.
SELECT (pg_ripple.list_datalog_views()->0->>'name') = 'test_dl_view' AS correct_name;

-- The entry should reference the rdfs rule set.
SELECT (pg_ripple.list_datalog_views()->0->>'rule_set') = 'rdfs' AS correct_rule_set;

-- ── Cleanup: remove catalog entry ────────────────────────────────────────────
DELETE FROM _pg_ripple.datalog_views WHERE name = 'test_dl_view';

SELECT pg_ripple.list_datalog_views() = '[]'::jsonb AS cleaned_up;
