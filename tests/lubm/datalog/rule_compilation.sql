-- LUBM Datalog sub-suite: rule compilation correctness
-- Validates that loading the OWL RL built-in rule set works correctly
-- and the compiled rules have the expected properties.
--
-- Run after loading tests/lubm/fixtures/univ1.ttl into pg_ripple.

-- Load OWL RL rules
SELECT pg_ripple.load_rules_builtin('owl-rl') AS rules_loaded;

-- Verify rule count is reasonable (OWL RL has at least 20 rules)
SELECT
    COUNT(*) AS total_rules,
    COUNT(*) FILTER (WHERE rule_set = 'owl-rl') AS owl_rl_rules,
    COUNT(*) FILTER (WHERE is_recursive = true) AS recursive_rules
FROM (
    SELECT *
    FROM jsonb_array_elements(
        (SELECT pg_ripple.list_rules())::jsonb
    ) WITH ORDINALITY AS r(elem, n),
    LATERAL (SELECT
        elem->>'rule_set'    AS rule_set,
        (elem->>'is_recursive')::boolean AS is_recursive
    ) AS attrs
) t;

-- Verify stratification metadata exists
SELECT
    COUNT(*) FILTER (WHERE stratum IS NOT NULL) AS rules_with_stratum
FROM (
    SELECT *
    FROM jsonb_array_elements(
        (SELECT pg_ripple.list_rules())::jsonb
    ) WITH ORDINALITY AS r(elem, n),
    LATERAL (SELECT
        (elem->>'stratum')::int AS stratum
    ) AS attrs
    WHERE elem->>'rule_set' = 'owl-rl'
) t;
