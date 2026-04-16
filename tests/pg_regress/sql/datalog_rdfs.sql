-- pg_regress test: Datalog RDFS entailment

SET search_path TO pg_ripple, public;

-- Load RDFS built-in rule set.
SELECT pg_ripple.load_rules_builtin('rdfs') > 0 AS rdfs_rules_loaded;

-- Verify the rule set is present.
SELECT count(*) > 5 AS rdfs_has_multiple_rules
FROM (SELECT * FROM pg_ripple.list_rules()) r
WHERE (r::jsonb)->>'rule_set' = 'rdfs';

-- Cleanup.
SELECT pg_ripple.drop_rules('rdfs') >= 0 AS cleanup_ok;
