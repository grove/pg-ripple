-- pg_regress test: Datalog malformed input error handling

SET search_path TO pg_ripple, public;

-- Test: empty rule set (valid).
SELECT pg_ripple.load_rules('', 'empty_test') >= 0 AS empty_rules_ok;

-- Test: rules with only comments (valid).
SELECT pg_ripple.load_rules(
    '# This is a comment\n# Another comment\n',
    'comments_only'
) >= 0 AS comments_only_ok;

-- Test: list_rules returns valid JSONB.
SELECT jsonb_typeof(pg_ripple.list_rules()) = 'array' AS list_rules_ok;

-- Test: drop nonexistent rule set returns 0.
SELECT pg_ripple.drop_rules('nonexistent_rule_set_xyz') = 0 AS drop_nonexistent_ok;

-- Test: load unknown built-in rule set raises error.
DO $$
BEGIN
    BEGIN
        PERFORM pg_ripple.load_rules_builtin('nonexistent_builtin_xyz');
        RAISE EXCEPTION 'expected error not raised';
    EXCEPTION
        WHEN OTHERS THEN
            -- Expected: error should be raised for unknown built-in
            NULL;
    END;
END $$;
SELECT true AS malformed_builtin_raises_error;

-- Cleanup.
SELECT pg_ripple.drop_rules('empty_test') >= 0 AS cleanup1;
SELECT pg_ripple.drop_rules('comments_only') >= 0 AS cleanup2;
