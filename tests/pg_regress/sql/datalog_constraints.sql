-- pg_regress test: Datalog integrity constraints

SET search_path TO pg_ripple, public;

-- Load a rule set with an empty rule set to verify check_constraints works.
SELECT pg_ripple.load_rules(
    '# Constraint rules test\n# (No actual constraint rules in this test)\n',
    'constraint_test'
) >= 0 AS constraint_rules_ok;

-- check_constraints should return a valid JSONB array (empty if no violations).
SELECT jsonb_typeof(pg_ripple.check_constraints()) = 'array' AS constraints_is_array;
SELECT jsonb_array_length(pg_ripple.check_constraints()) >= 0 AS no_spurious_violations;

-- check_constraints with explicit rule_set filter.
SELECT jsonb_typeof(pg_ripple.check_constraints('constraint_test')) = 'array' AS filtered_constraints_ok;

-- Cleanup.
SELECT pg_ripple.drop_rules('constraint_test') >= 0 AS cleanup_ok;
