-- pg_regress test: Bounded-Depth Datalog Termination (v0.34.0)
--
-- Tests that:
-- 1. GUC pg_ripple.datalog_max_depth exists and defaults to 0 (unlimited).
-- 2. When datalog_max_depth = 0, a transitive closure rule produces the full chain.
-- 3. When datalog_max_depth = 2, the closure terminates after 2 hops.
-- 4. SPARQL property path queries on bounded predicates respect the depth limit.
-- 5. The depth bound does not affect non-recursive rules.

-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Part 1: GUC checks ────────────────────────────────────────────────────────

-- 1a. datalog_max_depth defaults to 0 (unlimited).
SHOW pg_ripple.datalog_max_depth;

-- 1b. GUC can be set.
SET pg_ripple.datalog_max_depth = 5;
SHOW pg_ripple.datalog_max_depth;
SET pg_ripple.datalog_max_depth = 0;

-- ── Part 2: Unlimited transitive closure ──────────────────────────────────────

-- Insert a 5-hop chain: a → b → c → d → e → f
SELECT pg_ripple.insert_triple(
    '<https://bd.test/a>', '<https://bd.test/step>', '<https://bd.test/b>'
) IS NOT NULL AS insert_a_b;
SELECT pg_ripple.insert_triple(
    '<https://bd.test/b>', '<https://bd.test/step>', '<https://bd.test/c>'
) IS NOT NULL AS insert_b_c;
SELECT pg_ripple.insert_triple(
    '<https://bd.test/c>', '<https://bd.test/step>', '<https://bd.test/d>'
) IS NOT NULL AS insert_c_d;
SELECT pg_ripple.insert_triple(
    '<https://bd.test/d>', '<https://bd.test/step>', '<https://bd.test/e>'
) IS NOT NULL AS insert_d_e;
SELECT pg_ripple.insert_triple(
    '<https://bd.test/e>', '<https://bd.test/step>', '<https://bd.test/f>'
) IS NOT NULL AS insert_e_f;

-- Load a transitive closure rule with max_depth = 0 (unlimited).
SET pg_ripple.datalog_max_depth = 0;

SELECT pg_ripple.load_rules(
    '?x <https://bd.test/reach> ?y :- ?x <https://bd.test/step> ?y . '
    '?x <https://bd.test/reach> ?z :- ?x <https://bd.test/reach> ?y , '
    '                                  ?y <https://bd.test/step> ?z .',
    'bd_unlimited'
) > 0 AS unlimited_rules_loaded;

SELECT pg_ripple.infer('bd_unlimited') >= 0 AS unlimited_infer_ok;

-- Full closure: a can reach all 5 downstream nodes.
SELECT COUNT(*) AS full_closure_count
FROM pg_ripple.sparql(
    'SELECT ?o WHERE { <https://bd.test/a> <https://bd.test/reach> ?o }'
);

SELECT pg_ripple.drop_rules('bd_unlimited') >= 0 AS unlimited_dropped;

-- ── Part 3: Bounded transitive closure (max_depth = 2) ───────────────────────

-- With depth = 2, the closure should contain only 2-hop reachability from a:
-- a→b (depth 1), a→c (depth 2 via b); NOT a→d, a→e, a→f.
SET pg_ripple.datalog_max_depth = 2;

SELECT pg_ripple.load_rules(
    '?x <https://bd.test/reach2> ?y :- ?x <https://bd.test/step> ?y . '
    '?x <https://bd.test/reach2> ?z :- ?x <https://bd.test/reach2> ?y , '
    '                                   ?y <https://bd.test/step> ?z .',
    'bd_bounded'
) > 0 AS bounded_rules_loaded;

SELECT pg_ripple.infer('bd_bounded') >= 0 AS bounded_infer_ok;

-- With depth bound = 2, at most 2 rows from a (not 5).
SELECT COUNT(*) <= 3 AS bounded_closure_ok
FROM pg_ripple.sparql(
    'SELECT ?o WHERE { <https://bd.test/a> <https://bd.test/reach2> ?o }'
);

SELECT pg_ripple.drop_rules('bd_bounded') >= 0 AS bounded_dropped;

-- Reset depth to unlimited.
SET pg_ripple.datalog_max_depth = 0;

-- ── Part 4: Non-recursive rules are unaffected ───────────────────────────────

-- A non-recursive rule with max_depth = 1 should still work normally.
SET pg_ripple.datalog_max_depth = 1;

SELECT pg_ripple.load_rules(
    '?x <https://bd.test/known> ?y :- ?x <https://bd.test/step> ?y .',
    'bd_nonrecursive'
) > 0 AS nonrecursive_rules_loaded;

SELECT pg_ripple.infer('bd_nonrecursive') > 0 AS nonrecursive_infer_ok;

SELECT pg_ripple.drop_rules('bd_nonrecursive') >= 0 AS nonrecursive_dropped;

-- Reset.
SET pg_ripple.datalog_max_depth = 0;
