-- pg_regress test: SPARQL property paths (v0.5.0)
-- Uses unique predicate namespace https://pp.test/ to avoid interference.

-- Clean up any pp.test data from previous runs to ensure idempotent counts.
DO $$
BEGIN
    DELETE FROM _pg_ripple.vp_rare
    WHERE p IN (
        SELECT id FROM _pg_ripple.dictionary
        WHERE value IN ('https://pp.test/follows', 'https://pp.test/likes')
    );
END $$;

-- Setup: load a chain graph for property path tests.
-- Chain: a -> b -> c -> d (using pp.test/follows)
-- Star: a -> x, a -> y, a -> z (using pp.test/likes)
SELECT pg_ripple.load_ntriples(
    '<https://pp.test/a> <https://pp.test/follows> <https://pp.test/b> .' || E'\n' ||
    '<https://pp.test/b> <https://pp.test/follows> <https://pp.test/c> .' || E'\n' ||
    '<https://pp.test/c> <https://pp.test/follows> <https://pp.test/d> .' || E'\n' ||
    '<https://pp.test/a> <https://pp.test/likes>   <https://pp.test/x> .' || E'\n' ||
    '<https://pp.test/a> <https://pp.test/likes>   <https://pp.test/y> .' || E'\n' ||
    '<https://pp.test/a> <https://pp.test/likes>   <https://pp.test/z> .'
) = 6 AS six_triples_loaded;

-- OneOrMore (p+): a follows+ b/c/d => 3 results
SELECT COUNT(*) AS follows_plus_count
FROM pg_ripple.sparql(
    'SELECT ?target WHERE { <https://pp.test/a> <https://pp.test/follows>+ ?target }'
);

-- ASK OneOrMore: a follows+ d must be true (transitive chain)
SELECT pg_ripple.sparql_ask(
    'ASK { <https://pp.test/a> <https://pp.test/follows>+ <https://pp.test/d> }'
) AS ask_a_follows_plus_d;

-- ASK OneOrMore: a follows+ a must be false (no self-loop)
SELECT pg_ripple.sparql_ask(
    'ASK { <https://pp.test/a> <https://pp.test/follows>+ <https://pp.test/a> }'
) AS ask_a_follows_plus_a;

-- ZeroOrMore (p*): a follows* includes a itself plus all reachable
SELECT COUNT(*) >= 4 AS follows_star_at_least_four
FROM pg_ripple.sparql(
    'SELECT ?target WHERE { <https://pp.test/a> <https://pp.test/follows>* ?target }'
);

-- ZeroOrOne (p?): a follows? b/a => direct + identity
SELECT COUNT(*) >= 1 AS follows_maybe_b
FROM pg_ripple.sparql(
    'SELECT ?target WHERE { <https://pp.test/a> <https://pp.test/follows>? ?target }'
);

-- Sequence (a/b): a follows/follows ?x = nodes two hops from a = c
SELECT COUNT(*) AS two_hop_count
FROM pg_ripple.sparql(
    'SELECT ?x WHERE { <https://pp.test/a> <https://pp.test/follows>/<https://pp.test/follows> ?x }'
);

-- Alternative (a|b): a (follows|likes) ?x = b + x + y + z
SELECT COUNT(*) AS alt_count
FROM pg_ripple.sparql(
    'SELECT ?x WHERE { <https://pp.test/a> (<https://pp.test/follows>|<https://pp.test/likes>) ?x }'
);

-- Inverse (^p): ?who follows+ a — should return empty (nothing follows a)
SELECT COUNT(*) AS inverse_count
FROM pg_ripple.sparql(
    'SELECT ?who WHERE { ?who <https://pp.test/follows>+ <https://pp.test/a> }'
);

-- Inverse (^p): ?who follows b — should return a (a follows b)
SELECT COUNT(*) AS inverse_direct_count
FROM pg_ripple.sparql(
    'SELECT ?who WHERE { ?who ^<https://pp.test/follows> <https://pp.test/b> }'
);

-- max_path_depth GUC: set low depth and verify query still executes (not crash)
SET pg_ripple.max_path_depth = 2;
SELECT COUNT(*) AS follows_plus_depth2
FROM pg_ripple.sparql(
    'SELECT ?target WHERE { <https://pp.test/a> <https://pp.test/follows>+ ?target }'
);
RESET pg_ripple.max_path_depth;

-- sparql_explain includes the generated SQL for a path query
SELECT pg_ripple.sparql_explain(
    'SELECT ?x WHERE { <https://pp.test/a> <https://pp.test/follows>+ ?x }',
    FALSE
) LIKE '-- SPARQL Algebra --%' AS path_explain_ok;
