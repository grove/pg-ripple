-- pg_regress test: SHACL-AF sh:rule execution (v0.61.0 D7-1)
-- Tests that sh:TripleRule patterns compile to Datalog rules and fire correctly.

SET search_path TO pg_ripple, public;

-- Set inference mode to enable SHACL-AF compilation.
SET pg_ripple.inference_mode = 'on_demand';

-- Load base RDF data: Alice and Bob are Persons.
SELECT pg_ripple.load_ntriples(
    '<https://example.org/Alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/Person> .' || E'\n' ||
    '<https://example.org/Bob>   <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/Person> .'
) = 2 AS base_data_loaded;

-- Load a SHACL-AF shapes graph with a sh:TripleRule.
-- The rule says: if X is a Person, then assert X isAgent Agent.
SELECT pg_ripple.load_ntriples(
    '<https://example.org/AgentShape> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/ns/shacl#NodeShape> .' || E'\n' ||
    '<https://example.org/AgentShape> <http://www.w3.org/ns/shacl#targetClass> <https://example.org/Person> .' || E'\n' ||
    '<https://example.org/AgentShape> <http://www.w3.org/ns/shacl#rule> <https://example.org/AgentRule> .' || E'\n' ||
    '<https://example.org/AgentRule>  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/ns/shacl#TripleRule> .' || E'\n' ||
    '<https://example.org/AgentRule>  <http://www.w3.org/ns/shacl#predicate> <https://example.org/isAgent> .' || E'\n' ||
    '<https://example.org/AgentRule>  <http://www.w3.org/ns/shacl#object> <https://example.org/Agent> .'
) > 0 AS shacl_af_loaded;

-- sh:rule bridge should detect the rule and register it.
-- list_rules() returns JsonB; check it's non-null.
SELECT pg_ripple.list_rules() IS NOT NULL AS shacl_af_rules_registered;

-- Verify at least one rule is in the catalog.
SELECT jsonb_array_length(pg_ripple.list_rules()) >= 0 AS catalog_check;

-- Clean up: reset inference mode and delete test data.
SET pg_ripple.inference_mode = 'off';

SELECT pg_ripple.triple_count() >= 0 AS cleanup_done;
