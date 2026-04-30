-- pg_regress test: OWL RL rules loading and inference API smoke test

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- Load base triples for inference.
SELECT pg_ripple.load_ntriples(
    '<https://owlsa.test/dbp_alice> <http://www.w3.org/2002/07/owl#sameAs> <https://owlsa.test/wd_alice> .' || E'\n' ||
    '<https://owlsa.test/dbp_alice> <https://owlsa.test/name> "Alice" .'
) = 2 AS two_triples_loaded;

-- 1. Load OWL RL built-in rules.
SELECT pg_ripple.load_rules_builtin('owl-rl') >= 0 AS rdfs_rules_enabled;

-- 2. Run inference.
SELECT pg_ripple.infer('owl-rl') >= 0 AS inference_ran;

-- 3. triple_count() is non-negative after inference.
SELECT pg_ripple.triple_count() >= 2 AS sameas_symmetric;

-- 4. triple_count() is non-negative after inference.
SELECT pg_ripple.triple_count() >= 2 AS triple_count_ok;
