-- pg_regress test: Datalog named-graph scope (regression for g=0 default bug)
--
-- Regression test for: Datalog SQL generator defaulting rule_graph_scope to
-- 'default', injecting WHERE g = 0 on every unscoped body atom and therefore
-- silently ignoring triples stored in named graphs (g != 0).
--
-- The fix: unwrap_or fallback changed from "default" to "all" in compiler.rs.
-- Without the fix, tests 3 and 4 below would return 0 rows instead of 1.

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- ── Setup ─────────────────────────────────────────────────────────────────────

-- Drop any leftover graphs from prior runs.
SELECT pg_ripple.drop_graph('https://ngs.test/graph1');
SELECT pg_ripple.drop_graph('https://ngs.test/graph2');

-- Insert base triples into a named graph (g != 0) via N-Quads.
SELECT pg_ripple.load_nquads(
    '<https://ngs.test/alice> <https://ngs.test/email> <mailto:alice@example.com> <https://ngs.test/graph1> .' || E'\n' ||
    '<https://ngs.test/bob>   <https://ngs.test/email> <mailto:bob@example.com>   <https://ngs.test/graph1> .' || E'\n'
) = 2 AS base_triples_loaded_into_named_graph;

-- ── 1. Verify triples are in the named graph (not g=0) ───────────────────────
SELECT pg_ripple.triple_count_in_graph('https://ngs.test/graph1') = 2
    AS triples_in_named_graph;

-- ── 2. Load a simple rule with no explicit GRAPH clause ──────────────────────
SELECT pg_ripple.add_rule(
    'ngs_test',
    '?x <https://ngs.test/hasContact> ?y :- ?x <https://ngs.test/email> ?y'
) IS NOT NULL AS rule_added;

-- ── 3. Run inference — must find email triples in named graph ─────────────────
--
-- With the bug (rule_graph_scope defaulting to 'default'), infer() adds g=0
-- filters and derives nothing.  With the fix (defaulting to 'all'), it sees
-- the named-graph triples and derives hasContact for both alice and bob.
SELECT pg_ripple.infer('ngs_test') >= 0 AS inference_ran;

SELECT COUNT(*) = 2 AS two_contacts_derived
FROM pg_ripple.sparql($$
    SELECT ?x ?y WHERE {
        ?x <https://ngs.test/hasContact> ?y
    }
$$);

-- ── 4. Explicit rule_graph_scope = 'all' produces the same result ─────────────
SET pg_ripple.rule_graph_scope = 'all';
SELECT pg_ripple.infer('ngs_test') >= 0 AS inference_ran_all;
SELECT COUNT(*) = 2 AS two_contacts_all
FROM pg_ripple.sparql($$
    SELECT ?x ?y WHERE {
        ?x <https://ngs.test/hasContact> ?y
    }
$$);
RESET pg_ripple.rule_graph_scope;

-- ── 5. rule_graph_scope = 'default' must NOT match triples in named graphs ────
--
-- First clean up derived triples, then re-run with scope 'default'.
-- The rule head triples (hasContact) live in the default graph (g=0), but the
-- body looks for email triples in g=0 only — where there are none.
-- Derived count should be 0 on a clean run.

-- Clean up previously derived triples so the re-run starts fresh.
SELECT pg_ripple.drop_rules('ngs_test') >= 0 AS rules_dropped_for_scope_test;

-- Reload rule.
SELECT pg_ripple.add_rule(
    'ngs_test_default_scope',
    '?x <https://ngs.test/hasContact2> ?y :- ?x <https://ngs.test/email> ?y'
) IS NOT NULL AS rule_added_for_scope_test;

SET pg_ripple.rule_graph_scope = 'default';
-- infer() succeeds but derives 0 hasContact2 triples because email triples are
-- in named graphs, not the default graph.  The return value counts SQL
-- executions (>= 1 for a 1-rule set), so we only assert it ran successfully.
SELECT pg_ripple.infer('ngs_test_default_scope') >= 0 AS infer_ran_with_default_scope;
-- Confirm no hasContact2 triples were actually materialised.
SELECT COUNT(*) = 0 AS no_contacts2_derived_with_default_scope
FROM pg_ripple.sparql($$
    SELECT ?x ?y WHERE {
        ?x <https://ngs.test/hasContact2> ?y
    }
$$);
RESET pg_ripple.rule_graph_scope;

-- ── 6. Triples in the default graph ARE matched when scope = 'default' ────────
-- Insert one email triple into the default graph (g=0) and verify derivation.
SELECT pg_ripple.insert_triple(
    '<https://ngs.test/carol>',
    '<https://ngs.test/email>',
    '<mailto:carol@example.com>'
) IS NOT NULL AS carol_email_in_default_graph;

SET pg_ripple.rule_graph_scope = 'default';
SELECT pg_ripple.infer('ngs_test_default_scope') = 1 AS one_derived_in_default_graph;
RESET pg_ripple.rule_graph_scope;

-- Verify the derived triple is visible.
SELECT COUNT(*) = 1 AS carol_contact_derived
FROM pg_ripple.sparql($$
    SELECT ?y WHERE {
        <https://ngs.test/carol> <https://ngs.test/hasContact2> ?y
    }
$$);

-- ── Cleanup ───────────────────────────────────────────────────────────────────
SELECT pg_ripple.drop_rules('ngs_test_default_scope') >= 0 AS rules_cleaned;
SELECT pg_ripple.drop_graph('https://ngs.test/graph1') >= 0 AS graph1_cleaned;
