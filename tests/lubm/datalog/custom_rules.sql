-- LUBM Datalog sub-suite: custom rule validation
-- Defines an ad-hoc Datalog rule on LUBM data and validates that the
-- rule compiler handles custom rules correctly (catches edge cases in
-- rule parsing, compilation, and stratification).
--
-- Run after loading tests/lubm/fixtures/univ1.ttl.

-- Custom rule 1: transitive subOrganizationOf closure
-- Derives ?X rdf:type ub:Organization for anything that is
-- a subOrganization of something (domain inference).
SELECT pg_ripple.load_rules(
    '?X <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>
        <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#Organization> :-
        ?X <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#subOrganizationOf> ?Y .',
    'lubm_custom'
) AS rules_loaded;

-- Run inference for the custom rule set
SELECT pg_ripple.infer('lubm_custom') AS derived_triples;

-- Verify: Department0 and ResearchGroup0 should be typed as Organization
SELECT COUNT(*) AS org_count
FROM pg_ripple.sparql(
    'PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
     PREFIX ub:  <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#>
     SELECT ?X WHERE { ?X rdf:type ub:Organization }'
);

-- Custom rule 2: advisor transitivity
-- If A advises B and B advises C, derive A indirectly_advises C.
SELECT pg_ripple.load_rules(
    '?A <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#indirectlyAdvises> ?C :-
        ?A <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#advisor> ?B ,
        ?B <http://www.lehigh.edu/~zhp2/2004/0401/univ-bench.owl#advisor> ?C .',
    'lubm_custom2'
) AS rules_loaded;

-- Run inference (should produce 0 results since no advisor chains in univ1,
-- but must not crash)
SELECT pg_ripple.infer('lubm_custom2') AS derived_triples;

-- Verify that list_rules shows both custom rule sets
SELECT COUNT(*) AS custom_rule_sets
FROM (
    SELECT DISTINCT elem->>'rule_set' AS rule_set
    FROM jsonb_array_elements(
        (SELECT pg_ripple.list_rules())::jsonb
    ) AS r(elem)
    WHERE elem->>'rule_set' LIKE 'lubm_custom%'
) t;
