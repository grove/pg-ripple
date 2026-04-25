-- pg_regress test: OWL 2 QL profile / DL-Lite query rewriting
-- v0.57.0 Feature L-3.2

SET search_path TO pg_ripple, public;

-- Load OWL QL built-in rule set.
SELECT pg_ripple.load_rules_builtin('owl-ql') > 0 AS owl_ql_rules_loaded;

-- Verify the QL rule set is present.
SELECT count(*) > 0 AS owl_ql_has_rules
FROM (SELECT * FROM pg_ripple.list_rules()) r
WHERE r::text LIKE '%owl-ql%';

-- Test: owl_profile GUC accepts 'QL'.
SET pg_ripple.owl_profile = 'QL';
SHOW pg_ripple.owl_profile;
RESET pg_ripple.owl_profile;

-- Cleanup.
SELECT pg_ripple.drop_rules('owl-ql') >= 0 AS cleanup_ok;
