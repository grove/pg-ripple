-- pg_regress test: OWL 2 EL profile classification
-- v0.57.0 Feature L-3.1

SET search_path TO pg_ripple, public;

-- Load OWL EL built-in rule set.
SELECT pg_ripple.load_rules_builtin('owl-el') > 0 AS owl_el_rules_loaded;

-- Verify the EL rule set is present.
SELECT count(*) > 0 AS owl_el_has_rules
FROM (SELECT * FROM pg_ripple.list_rules()) r
WHERE r::text LIKE '%owl-el%';

-- Test: OWL EL profile GUC value is accepted.
SET pg_ripple.owl_profile = 'EL';
SHOW pg_ripple.owl_profile;

SET pg_ripple.owl_profile = 'RL';
SHOW pg_ripple.owl_profile;

SET pg_ripple.owl_profile = 'QL';
SHOW pg_ripple.owl_profile;

-- Reset to default.
RESET pg_ripple.owl_profile;

-- Cleanup.
SELECT pg_ripple.drop_rules('owl-el') >= 0 AS cleanup_ok;
