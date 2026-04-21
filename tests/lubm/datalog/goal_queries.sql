-- LUBM Datalog sub-suite: direct goal queries
-- Uses pg_ripple.infer_goal() to query Datalog-computed facts directly,
-- independently of the SPARQL translator.  Validates that the inference
-- engine and SPARQL query layer produce consistent results.
--
-- Run after loading univ1.ttl and calling pg_ripple.load_rules_builtin('owl-rl').

-- Goal query: which resources are of type ub:Student via inference?
SELECT pg_ripple.infer_goal(
    'owl-rl',
    '?X <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#Student>'
) AS student_goal_result;

-- Goal query: which resources are of type ub:Professor via inference?
SELECT pg_ripple.infer_goal(
    'owl-rl',
    '?X <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#Professor>'
) AS professor_goal_result;

-- Validate that goal count matches SPARQL count for Q6 (all students)
DO $$
DECLARE
    v_goal_json   jsonb;
    v_goal_count  int;
    v_sparql_count int;
BEGIN
    -- Get count via infer_goal
    v_goal_json := pg_ripple.infer_goal(
        'owl-rl',
        '?X <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#Student>'
    );
    v_goal_count := jsonb_array_length(
        COALESCE(v_goal_json->'bindings', '[]'::jsonb)
    );

    -- Get count via SPARQL
    SELECT COUNT(*) INTO v_sparql_count
    FROM pg_ripple.sparql(
        'PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
         PREFIX ub:  <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#>
         SELECT ?X WHERE { ?X rdf:type ub:Student }'
    );

    IF v_goal_count <> v_sparql_count THEN
        RAISE WARNING
            'goal query returned % results but SPARQL returned % — inference/SPARQL inconsistency',
            v_goal_count, v_sparql_count;
    END IF;
END;
$$;
