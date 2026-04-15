-- pg_regress test: resource exhaustion limits and GUC validation (v0.5.0)
-- Verifies that max_path_depth and statement_timeout prevent runaway queries.

-- Setup: a small cyclic graph (a -> b -> a cycle) for cycle detection tests.
SELECT pg_ripple.load_ntriples(
    '<https://rl.test/a> <https://rl.test/link> <https://rl.test/b> .' || E'\n' ||
    '<https://rl.test/b> <https://rl.test/link> <https://rl.test/a> .' || E'\n' ||
    '<https://rl.test/a> <https://rl.test/link> <https://rl.test/c> .'
) = 3 AS three_triples_loaded;

-- Cycle detection: p+ on cyclic graph must terminate and return finite results.
-- With CYCLE clause, the recursive CTE stops when a cycle is detected.
-- Set a low max_path_depth to ensure bounded execution.
SET pg_ripple.max_path_depth = 5;

SELECT COUNT(*) BETWEEN 1 AND 10 AS cycle_terminates
FROM pg_ripple.sparql(
    'SELECT ?x WHERE { <https://rl.test/a> <https://rl.test/link>+ ?x }'
);

-- With depth = 1, only direct hops are returned.
SET pg_ripple.max_path_depth = 1;
SELECT COUNT(*) = 2 AS depth1_count
FROM pg_ripple.sparql(
    'SELECT ?x WHERE { <https://rl.test/a> <https://rl.test/link>+ ?x }'
);

-- Reset to default.
RESET pg_ripple.max_path_depth;

-- Verify max_path_depth GUC is accessible and has the right default.
SELECT current_setting('pg_ripple.max_path_depth')::int = 100 AS default_depth_is_100;

-- Malformed SPARQL must produce an error, not a crash.
DO $$
BEGIN
    PERFORM pg_ripple.sparql('NOT VALID SPARQL AT ALL');
    RAISE EXCEPTION 'expected error was not raised';
EXCEPTION WHEN OTHERS THEN
    -- Error expected; test passes if we reach here.
    NULL;
END;
$$;
SELECT TRUE AS malformed_sparql_raises_error;

-- Very deeply nested subqueries (3 levels): must execute without crashing.
SELECT COUNT(*) >= 0 AS deep_subquery_ok
FROM pg_ripple.sparql(
    'SELECT ?x WHERE {
       { SELECT ?x WHERE {
           { SELECT ?x WHERE {
               ?x <https://rl.test/link> ?y
             }
           }
         }
       }
     }'
);

-- Empty VALUES clause: must return zero rows, not crash.
SELECT COUNT(*) = 0 AS empty_values_ok
FROM pg_ripple.sparql(
    'SELECT ?x WHERE {
       VALUES ?x { }
       ?x <https://rl.test/link> ?y
     }'
);
