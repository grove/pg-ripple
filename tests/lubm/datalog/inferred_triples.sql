-- LUBM Datalog sub-suite: inferred triple counts
-- After materializing OWL RL inference from univ1 data, verify that
-- specific inferred triple patterns meet minimum expected counts.
--
-- Run after loading univ1.ttl and calling pg_ripple.infer('owl-rl').

-- Count inferred rdf:type triples for key superclasses
SELECT
    COUNT(*) AS inferred_student_type
FROM pg_ripple.sparql(
    'PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
     PREFIX ub:  <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#>
     SELECT ?X WHERE { ?X rdf:type ub:Student }'
);

SELECT
    COUNT(*) AS inferred_professor_type
FROM pg_ripple.sparql(
    'PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
     PREFIX ub:  <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#>
     SELECT ?X WHERE { ?X rdf:type ub:Professor }'
);

SELECT
    COUNT(*) AS inferred_person_type
FROM pg_ripple.sparql(
    'PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
     PREFIX ub:  <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#>
     SELECT ?X WHERE { ?X rdf:type ub:Person }'
);

-- Verify minimum counts
DO $$
DECLARE
    v_students int;
    v_profs    int;
BEGIN
    SELECT COUNT(*) INTO v_students
    FROM pg_ripple.sparql(
        'PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
         PREFIX ub:  <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#>
         SELECT ?X WHERE { ?X rdf:type ub:Student }'
    );

    IF v_students < 12 THEN
        RAISE EXCEPTION 'expected >= 12 inferred ub:Student triples, got %', v_students;
    END IF;

    SELECT COUNT(*) INTO v_profs
    FROM pg_ripple.sparql(
        'PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
         PREFIX ub:  <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#>
         SELECT ?X WHERE { ?X rdf:type ub:Professor }'
    );

    IF v_profs < 4 THEN
        RAISE EXCEPTION 'expected >= 4 inferred ub:Professor triples, got %', v_profs;
    END IF;
END;
$$;
