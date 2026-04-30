-- pg_regress test: v0.75.0 feature gate
--   PROPPATH-TEST-01:          Property path inside OPTIONAL and GRAPH with vp_rare
--   FEATURE-STATUS-JOURNAL-01: mutation_journal entry in feature_status()
--   RLS-ERROR-01/ROLE-DOC-01:  RLS error surfacing and role-name documentation
--   UNWRAP-AUDIT-01:           Verified: no bare unwrap() in production paths
--   FUZZ-URL-01:               url_host_parser fuzz target added (see fuzz/fuzz_targets/)

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- ─── Part 1: FEATURE-STATUS-JOURNAL-01 ─────────────────────────────────────

-- 1a. mutation_journal row is present with implemented status.
SELECT status AS mutation_journal_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'mutation_journal';

-- 1b. mutation_journal evidence path references the source file.
SELECT evidence_path LIKE '%mutation_journal%' AS mutation_journal_evidSELECT evidence_path LIKE '%mutation_journal%' AS mutation_journal_evidSELECal';

-- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -��-- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- ��────────

-- 2a. Load test triples using a unique namespace (predicate stays in vp_rare).
SELECT pg_ripple.load_ntriples(
    '<https://pp75.test/a> <https://    '<https://pp75.test/a> <https:// b> .'    '<https://pp75.test/a> <https://    '<https://pp5.test/parent> <https://pp75.test/c> .' ||     '<https://pp75.test/a> <https://    '<https://pp75.test/a> <https:// b> .'    '<https://pp75.test/a> <https://    '<https://pp5.test/parent> <https:/) = 4 AS four_triples_loaded;

-- 2b. Confirm predicate lives in vp_rare.
SELECT COUNT(*) > 0 AS parent_pred_in_vp_rare
FROM _pg_ripple.vp_rare FROM _pg_ripple.vp_rare FROM _pg_rippd = v.p
WHERE d.value = 'https://pp75.test/parent';

-- 2c. Property pa-- 2c. Property pa-- 2c. Property pa-- 2c. Property pa-- 2c. Property pa-- 2c. Property pa-- 2c. Property pa-- 2c. Property pa-- 2c. Property pa-- 2c <https://pp75.test/parent>+ ?anc
    }
$$);

-- 2d. Property path inside OPTIONAL.
SELECT COUNT(*) AS opt_proppath_row_count
FROM pg_ripple.sparql($$
    SELECT ?x ?ancestor WHERE {
                                                                            p75.test/parent>+ ?ancestor }
    }
$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$l($$
    SELECT ?child ?anc WHERE {
        GRAPH <https://pp75.test/graph1> {
            ?ch            ?ch            ?ch            ?ch            ?ch      Zer            ?ch            ?ch          panic.
SELECT COUNT(*) >= 0 AS zero_or_more_opt_ok
FROM pg_ripple.sparql($$
    SELECT ?x ?anc WHERE {
        ?x <https://pp75.test/label> ?lbl .
        OPTIONAL { ?x <https://pp75.test/parent>* ?anc }
    }
$$);

----------------------------------ATE-01 ──────────────────────----------------------------��───────────

-- 3a. Extension is registered in pg_extension.
SELECT extname = 'pg_ripple' AS extension_registered
FROM pg_extension
WHERE extname = 'pg_ripple';
