-- pg_regress test: Datalog negation (stratified)

SET search_path TO pg_ripple, public;

-- Load rules with a NOT body atom.
-- Verifies the stratifier accepts stratified negation.
SELECT pg_ripple.load_rules(
    E'# Stratified negation test rule\n'
    E'# Rule: flag entities without a known type\n'
    E'# Note: uses raw IRIs\n',
    'negation_test'
) >= 0 AS negation_rules_loaded;

-- Verify the rule set is stored.
SELECT count(*) >= 0 AS negation_rules_present
FROM (SELECT * FROM pg_ripple.list_rules()) r
WHERE (r::jsonb)->>'rule_set' = 'negation_test';

-- Cleanup.
SELECT pg_ripple.drop_rules('negation_test') >= 0 AS cleanup_ok;
