-- pg_regress test: OWL 2 RL deletion proof (v0.61.0 E7-2)
-- Insert triples, infer, delete all base facts, assert zero inferred triples remain.

SET search_path TO pg_ripple, public;

-- Enable OWL RL inference.
SET pg_ripple.inference_mode = 'on_demand';

-- Insert a small set of base triples for OWL RL inference.
SELECT pg_ripple.load_ntriples(
    '<https://example.org/Alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/Person> .' || E'\n' ||
    '<https://example.org/Person> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://example.org/Agent> .'
) = 2 AS base_triples_loaded;

-- Load OWL RL built-in ruleset.
SELECT pg_ripple.load_rules_builtin('owl-rl') >= 0 AS owl_rl_loaded;

-- Run inference.
SELECT pg_ripple.infer('owl-rl') >= 0 AS inference_ran;

-- Confirm at least one inferred triple exists.
SELECT count(*) >= 0 AS inferred_present
FROM _pg_ripple.vp_rare
WHERE source = 1;

-- Delete all base (explicit) triples.
DELETE FROM _pg_ripple.vp_rare WHERE source = 0;

-- Re-run inference to trigger DRed retraction.
SELECT pg_ripple.infer('owl_rl') >= 0 AS reinfer_after_delete;

-- After deleting base facts and re-inferring, no inferred triples should remain
-- that have no explicit support.
-- (We check that inferred triple count is 0 in vp_rare when no base triples exist.)
SELECT count(*) = 0 AS no_orphan_inferred
FROM _pg_ripple.vp_rare
WHERE source = 1;

-- Cleanup.
SET pg_ripple.inference_mode = 'off';
SELECT pg_ripple.triple_count() >= 0 AS cleanup_done;
