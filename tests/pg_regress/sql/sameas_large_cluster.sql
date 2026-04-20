-- pg_regress test: owl:sameAs large cluster size bound (v0.42.0)
--
-- Tests that:
-- 1. GUC pg_ripple.sameas_max_cluster_size exists and defaults to 100000.
-- 2. PT550 WARNING is emitted when a cluster exceeds the configured limit.
-- 3. Canonicalization is skipped (returns empty map) when the limit is exceeded.
-- 4. Setting the limit to 0 disables the check.

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Part 1: GUC checks ────────────────────────────────────────────────────────

-- 1a. sameas_max_cluster_size defaults to 100000.
SHOW pg_ripple.sameas_max_cluster_size;

-- 1b. GUC can be reduced.
SET pg_ripple.sameas_max_cluster_size = 5;
SHOW pg_ripple.sameas_max_cluster_size;

-- 1c. Reset to a permissive value.
SET pg_ripple.sameas_max_cluster_size = 100000;

-- ── Part 2: Normal sameAs works below the limit ───────────────────────────────

-- Insert a small equivalence cluster (3 nodes).
SELECT pg_ripple.insert_triple(
    '<https://ex.org/cluster/a>',
    '<http://www.w3.org/2002/07/owl#sameAs>',
    '<https://ex.org/cluster/b>'
) AS inserted_ab;

SELECT pg_ripple.insert_triple(
    '<https://ex.org/cluster/b>',
    '<http://www.w3.org/2002/07/owl#sameAs>',
    '<https://ex.org/cluster/c>'
) AS inserted_bc;

-- Insert a test triple for inference.
SELECT pg_ripple.insert_triple(
    '<https://ex.org/cluster/a>',
    '<https://ex.org/hasProp>',
    '"test value"'
) AS inserted_prop;

-- Load simple transitivity rule.
SELECT pg_ripple.load_rules($$
?y owl:sameAs ?x :- ?x owl:sameAs ?y .
?x owl:sameAs ?z :- ?x owl:sameAs ?y, ?y owl:sameAs ?z .
$$) AS rules_loaded;

-- Inference should succeed without PT550 (cluster size 3 < 100000).
SELECT pg_ripple.infer() >= 0 AS inference_ok;

-- ── Part 3: PT550 emitted when cluster exceeds limit ─────────────────────────

-- Set a very low limit so our 3-node cluster triggers the warning.
SET pg_ripple.sameas_max_cluster_size = 2;

-- Insert one more sameAs to grow cluster to 3 nodes.
SELECT pg_ripple.insert_triple(
    '<https://ex.org/cluster/c>',
    '<http://www.w3.org/2002/07/owl#sameAs>',
    '<https://ex.org/cluster/d>'
) AS inserted_cd;

-- Inference should emit PT550 WARNING and skip canonicalization (not an error).
-- The inference itself may still run (without sameAs rewriting).
SELECT pg_ripple.infer() >= 0 AS inference_ok_with_warning;

-- Reset limit.
SET pg_ripple.sameas_max_cluster_size = 100000;

-- ── Part 4: Limit of 0 disables the check ────────────────────────────────────

SET pg_ripple.sameas_max_cluster_size = 0;
SHOW pg_ripple.sameas_max_cluster_size;

-- Inference should work without any PT550 warning even with large clusters.
SELECT pg_ripple.infer() >= 0 AS inference_ok_no_limit;

-- Reset.
SET pg_ripple.sameas_max_cluster_size = 100000;

-- Cleanup.
SELECT pg_ripple.drop_rules() AS rules_dropped;
