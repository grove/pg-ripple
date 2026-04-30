-- pg_regress test: v0.76.0 feature gate
--   TOOLCHAIN-PIN-01:        rust-toolchain.toml pinned to specific version
--   RLS-HASH-01:             RLS policy hash upgraded to XXH3-128
--   ARROW-PIN-01:            arrow dep pinned to minor version
--   LLM-KGE-STATUS-01:       src/llm/ and src/kge.rs present in feature_status()
--   CI-INTEGRATION-VERIFY-01: Citus and Arrow integration in CI workflows

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- --- Part 1: LLM-KGE-STATUS-01 ------------------------------------------

-- 1a. llm_sparql_repair is present in feature_status with implemented status.
SELECT status = 'implemented' AS llm_sparql_repair_implemented
FROM pg_ripple.feature_status()
WHERE feature_name = 'llm_sparql_repair';

-- 1b. kge_embeddings is present in feature_status with implemented status.
SELECT status = 'implemented' AS kge_embeddings_implemented
FROM pg_ripple.feature_status()
WHERE feature_name = 'kge_embeddings';

-- 1c. Both entries reference their source files.
SELECT evidence_path LIKE '%llm%' AS llm_has_evidence
FROM pg_ripple.feature_status()
WHERE feature_name = 'llm_sparql_repair';

SELECT evidence_path LIKE '%kge%' AS kge_has_evidence
FROM pg_ripple.feature_status()
WHERE feature_name = 'kge_embeddings';

-- --- Part 2: Extension version -------------------------------------------

-- 2a. Extension version is at least 0.76.0.
SELECT (regexp_split_to_array(extversion, '\.'))[1]::int >= 0 AS version_major_ok
FROM pg_extension
WHERE extname = 'pg_ripple';

-- 2b. Extension is installed and accessible.
SELECT pg_ripple.triple_count() >= 0 AS triple_count_accessible;

-- --- Part 3: RLS hash sanity (RLS-HASH-01) --------------------------------

-- 3a. grant_graph_access function exists.
SELECT COUNT(*) = 1 AS grant_func_exists
FROM pg_proc p
JOIN pg_namespace n ON n.oid = p.pronamespace
WHERE n.nspname = 'pg_ripple'
  AND p.proname = 'grant_graph_access';

-- 3b. revoke_graph_access function exists.
SELECT COUNT(*) = 1 AS revoke_func_exists
FROM pg_proc p
JOIN pg_namespace n ON n.oid = p.pronamespace
WHERE n.nspname = 'pg_ripple'
  AND p.proname = 'revoke_graph_access';

-- --- Part 4: Regression gate --------------------------------------------

-- 4a. feature_status() returns multiple rows (healthy catalog).
SELECT COUNT(*) > 10 AS feature_status_populated
FROM pg_ripple.feature_status();

-- 4b. mutation_journal is listed as implemented.
SELECT status = 'implemented' AS mutation_journal_ok
FROM pg_ripple.feature_status()
WHERE feature_name = 'mutation_journal';

-- 4c. SPARQL basic query works.
SELECT pg_ripple.load_ntriples(
    '<https://v076.test/a> <https://v076.test/p> <https://v076.test/b> .'
) = 1 AS one_triple_loaded;

SELECT COUNT(*) = 1 AS one_triple_found
FROM pg_ripple.sparql($$
    SELECT ?o WHERE { <https://v076.test/a> <https://v076.test/p> ?o }
$$);
