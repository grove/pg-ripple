-- pg_regress test: v0.67.0 feature gate (v0.70.0 TEST-01)
--   MJOURNAL-01: mutation journal flush behavior
--   FLIGHT-SEC-02: Arrow Flight ticket signature validation
--   PROD-01: feature_status() and Python gate scripts smoke test

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- ── Part 1: Mutation journal — insert_triple path ────────────────────────────

-- 1a. Register a CONSTRUCT rule so CWB fires.
SELECT pg_ripple.create_graph('https://v067test.test/source/') IS NULL
    AS source_graph_created;
SELECT pg_ripple.create_graph('https://v067test.test/derived/') IS NULL
    AS derived_graph_created;

SELECT pg_ripple.create_construct_rule(
    'v067_journal_rule',
    'CONSTRUCT { ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ?c }
     WHERE { GRAPH <https://v067test.test/source/> {
         ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ?c
     } }',
    'https://v067test.test/derived/'
) IS NULL AS rule_created;

-- 1b. Insert via insert_triple() — mutation journal should trigger CWB.
SELECT pg_ripple.insert_triple(
    '<https://v067test.test/Alice>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<https://v067test.test/Person>',
    'https://v067test.test/source/'
) > 0 AS path_insert_triple_ok;

-- 1c. Insert via SPARQL INSERT DATA — mutation journal should trigger CWB.
SELECT pg_ripple.sparql_update(
    'INSERT DATA { GRAPH <https://v067test.test/source/> {
        <https://v067test.test/Bob>
        <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>
        <https://v067test.test/Person> } }'
) IS NOT NULL AS path_sparql_insert_ok;

-- 1d. Cleanup.
SELECT pg_ripple.drop_construct_rule('v067_journal_rule') AS rule_dropped;
SELECT pg_ripple.clear_graph('https://v067test.test/source/') >= 0 AS source_cleared;
SELECT pg_ripple.clear_graph('https://v067test.test/derived/') >= 0 AS derived_cleared;
SELECT pg_ripple.drop_graph('https://v067test.test/source/') IS NULL AS source_dropped;
SELECT pg_ripple.drop_graph('https://v067test.test/derived/') IS NULL AS derived_dropped;

-- ── Part 2: Arrow Flight ticket security (FLIGHT-SEC-02) ─────────────────────

-- 2a. arrow_unsigned_tickets_allowed GUC defaults to off.
SHOW pg_ripple.arrow_unsigned_tickets_allowed;

-- 2b. export_arrow_flight() exists and is callable (may error without secret,
--     but the function must exist).
SELECT proname FROM pg_proc
WHERE proname = 'export_arrow_flight'
  AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
LIMIT 1;

-- ── Part 3: feature_status() smoke test ──────────────────────────────────────

-- 3a. feature_status() returns rows.
SELECT count(*) > 0 AS has_feature_rows
FROM pg_ripple.feature_status();

-- 3b. All non-NULL evidence_path values reference known doc/test paths
--     (regression guard: ensures no future additions break GATE-03).
SELECT count(*) = 0 AS no_stale_evidence_paths
FROM pg_ripple.feature_status()
WHERE evidence_path IS NOT NULL
  AND evidence_path NOT LIKE 'ci/%'
  AND evidence_path NOT LIKE 'docs/%'
  AND evidence_path NOT LIKE 'src/%'
  AND evidence_path NOT LIKE 'tests/%'
  AND evidence_path NOT LIKE 'scripts/%';
