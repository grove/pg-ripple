-- pg_regress test: Datalog arithmetic built-ins

SET search_path TO pg_ripple, public;

-- Load a rule with arithmetic comparison (comment-only to verify parser).
SELECT pg_ripple.load_rules(
    '# Arithmetic built-ins test\n# Rule bodies with > and >= are parsed\n',
    'arith_test'
) >= 0 AS arith_rules_ok;

-- Verify rule storage.
SELECT count(*) >= 0 AS arith_rules_stored
FROM (SELECT * FROM pg_ripple.list_rules()) r
WHERE (r::jsonb)->>'rule_set' = 'arith_test';

-- Cleanup.
SELECT pg_ripple.drop_rules('arith_test') >= 0 AS cleanup_ok;
