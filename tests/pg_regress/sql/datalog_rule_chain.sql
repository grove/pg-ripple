-- pg_regress test: Datalog rule chaining (multi-hop derivation)

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- Load an ancestor hierarchy.
SELECT pg_ripple.load_ntriples(
    '<https://dlch.test/a> <https://dlch.test/parent> <https://dlch.test/b> .' || E'\n' ||
    '<https://dlch.test/b> <https://dlch.test/parent> <https://dlch.test/c> .' || E'\n' ||
    '<https://dlch.test/c> <https://dlch.test/parent> <https://dlch.test/d> .'
) = 3 AS three_parent_triples;

-- 1. Add ancestor rule (transitive closure).
SELECT pg_ripple.add_rule(
    'ancestor_chain',
    '?x <https://dlch.test/ancestor> ?z :- ?x <https://dlch.test/parent> ?z'
) IS NOT NULL AS base_rule_added;

SELECT pg_ripple.add_rule(
    'ancestor_chain',
    '?x <https://dlch.test/ancestor> ?z :- ?x <https://dlch.test/parent> ?y, ?y <https://dlch.test/ancestor> ?z'
) IS NOT NULL AS recursive_rule_added;

-- 2. Run inference.
SELECT pg_ripple.infer('ancestor_chain') >= 0 AS inference_ran;

-- 3. a is ancestor of c (2 hops).
SELECT COUNT(*) = 1 AS a_ancestor_of_c
FROM pg_ripple.sparql($$
    SELECT ?x WHERE {
        <https://dlch.test/a> <https://dlch.test/ancestor> <https://dlch.test/c> .
        BIND("found" AS ?x)
    }
$$);

-- 4. a is ancestor of d (3 hops).
SELECT COUNT(*) = 1 AS a_ancestor_of_d
FROM pg_ripple.sparql($$
    SELECT ?x WHERE {
        <https://dlch.test/a> <https://dlch.test/ancestor> <https://dlch.test/d> .
        BIND("found" AS ?x)
    }
$$);

-- 5. Cleanup rules.
SELECT pg_ripple.disable_rule_set('ancestor_chain') IS NOT NULL AS rules_disabled;
