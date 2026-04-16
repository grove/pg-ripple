-- pg_regress test: Datalog OWL RL rule set

SET search_path TO pg_ripple, public;

-- Load OWL RL built-in rule set.
SELECT pg_ripple.load_rules_builtin('owl-rl') > 0 AS owl_rl_rules_loaded;

-- Verify the rule set is present.
SELECT count(*) > 10 AS owl_rl_has_many_rules
FROM (SELECT * FROM pg_ripple.list_rules()) r
WHERE (r::jsonb)->>'rule_set' = 'owl-rl';

-- Enable and then disable.
SELECT pg_ripple.disable_rule_set('owl-rl');
SELECT pg_ripple.enable_rule_set('owl-rl');

-- Cleanup.
SELECT pg_ripple.drop_rules('owl-rl') >= 0 AS cleanup_ok;
