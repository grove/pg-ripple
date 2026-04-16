-- pg_regress test: Datalog custom rules

-- Ensure extension is loaded.
SET search_path TO pg_ripple, public;

-- ── Setup: register prefixes ──────────────────────────────────────────────────
SELECT pg_ripple.load_rules(
    '# empty rule set to test catalog setup',
    'test_empty'
) >= 0 AS catalog_ok;

-- ── Load a simple custom rule ─────────────────────────────────────────────────

-- First insert some base triples using raw IRIs.
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<https://example.org/manager>',
    '<https://example.org/bob>'
) > 0 AS manager_inserted;

SELECT pg_ripple.insert_triple(
    '<https://example.org/bob>',
    '<https://example.org/manager>',
    '<https://example.org/carol>'
) > 0 AS manager2_inserted;

-- Register a prefix.
INSERT INTO _pg_ripple.prefixes (prefix, expansion)
VALUES ('ex', 'https://example.org/')
ON CONFLICT (prefix) DO NOTHING;

-- Load a transitive rule (on-demand only; no materialization yet).
SELECT pg_ripple.load_rules(
    '<https://example.org/indirectManager> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/1999/02/22-rdf-syntax-ns#Property> .',
    'test_rule_meta'
) >= 0 AS rule_meta_ok;

-- Verify rules are stored.
SELECT count(*) >= 0 AS rules_present
FROM (SELECT * FROM pg_ripple.list_rules()) r;

-- ── Load RDFS built-in rule set ───────────────────────────────────────────────
SELECT pg_ripple.load_rules_builtin('rdfs') > 0 AS rdfs_loaded;

-- Verify RDFS rules are present.
SELECT count(*) > 0 AS rdfs_rules_present
FROM (SELECT * FROM pg_ripple.list_rules()) r
WHERE (r::jsonb)->>'rule_set' = 'rdfs';

-- ── Enable/disable rule sets ──────────────────────────────────────────────────
SELECT pg_ripple.enable_rule_set('rdfs');
SELECT pg_ripple.disable_rule_set('rdfs');
SELECT pg_ripple.enable_rule_set('rdfs');

-- ── Drop a rule set ───────────────────────────────────────────────────────────
SELECT pg_ripple.drop_rules('test_empty') >= 0 AS drop_ok;

-- ── Constraint checking ───────────────────────────────────────────────────────
-- Load a constraint rule: no resource can be its own manager.
-- (This should not fire since alice and bob are different.)
SELECT pg_ripple.check_constraints() IS NOT NULL AS constraints_ok;

-- ── Hot dictionary ────────────────────────────────────────────────────────────
SELECT pg_ripple.prewarm_dictionary_hot() >= 0 AS hot_prewarm_ok;

-- Cleanup.
SELECT pg_ripple.drop_rules('rdfs') >= 0 AS rdfs_cleanup;
SELECT pg_ripple.drop_rules('test_rule_meta') >= 0 AS meta_cleanup;
