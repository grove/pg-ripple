-- pg_regress test: Datalog built-in predicates (number, string, type checks)

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- Load mixed-type test data.
SELECT pg_ripple.load_ntriples(
    '<https://dlbi.test/alice> <https://dlbi.test/age> "30"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://dlbi.test/alice> <https://dlbi.test/name> "Alice" .' || E'\n' ||
    '<https://dlbi.test/bob> <https://dlbi.test/age> "25"^^<http://www.w3.org/2001/XMLSchema#integer> .' || E'\n' ||
    '<https://dlbi.test/bob> <https://dlbi.test/name> "Bob" .'
) = 4 AS four_triples_loaded;

-- 1. Add a Datalog rule that uses arithmetic comparison.
CREATE TEMP TABLE _dlbi_rule AS
SELECT pg_ripple.add_rule(
    'senior_member',
    '?p <https://dlbi.test/isSenior> "true" :- ?p <https://dlbi.test/age> ?a, ?a >= 28'
) AS rule_id;
SELECT rule_id > 0 AS rule_added FROM _dlbi_rule;

-- 2. Run inference.
SELECT pg_ripple.infer('senior_member') >= 0 AS inference_ran;

-- 3. Query the derived predicate.
SELECT COUNT(*) = 1 AS one_senior_found
FROM pg_ripple.sparql($$
    SELECT ?p WHERE {
        ?p <https://dlbi.test/isSenior> "true" .
    }
$$);

-- 4. Cleanup.
SELECT pg_ripple.remove_rule(rule_id) >= 0 AS rule_removed FROM _dlbi_rule;
DROP TABLE _dlbi_rule;
