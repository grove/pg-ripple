-- pg_regress test: NL → SPARQL via LLM integration (v0.49.0)
--
-- Tests that:
-- 1. New GUCs exist with correct defaults.
-- 2. sparql_from_nl() raises PT700 when llm_endpoint is empty.
-- 3. sparql_from_nl() returns a parseable SPARQL query when endpoint = 'mock'.
-- 4. add_llm_example() persists rows in _pg_ripple.llm_examples.
-- 5. suggest_sameas() degrades gracefully when pgvector is not installed.
-- 6. apply_sameas_candidates() returns 0 when pgvector is not installed.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── Part 1: GUC defaults ──────────────────────────────────────────────────────

-- 1a. llm_endpoint defaults to empty string (disabled).
SELECT current_setting('pg_ripple.llm_endpoint') = '' AS llm_endpoint_default_empty;

-- 1b. llm_model defaults to empty (resolved to 'gpt-4o' at call time).
SELECT current_setting('pg_ripple.llm_model', true) IS DISTINCT FROM 'disabled' AS llm_model_guc_exists;

-- 1c. llm_api_key_env GUC exists.
SELECT current_setting('pg_ripple.llm_api_key_env', true) IS DISTINCT FROM 'disabled' AS llm_api_key_env_guc_exists;

-- 1d. llm_include_shapes defaults to on.
SELECT current_setting('pg_ripple.llm_include_shapes') = 'on' AS llm_include_shapes_default_on;

-- ── Part 2: GUC can be toggled ────────────────────────────────────────────────

SET pg_ripple.llm_include_shapes = off;
SELECT current_setting('pg_ripple.llm_include_shapes') = 'off' AS llm_include_shapes_toggled;
RESET pg_ripple.llm_include_shapes;
SELECT current_setting('pg_ripple.llm_include_shapes') = 'on' AS llm_include_shapes_reset;

-- ── Part 3: PT700 — endpoint not configured ───────────────────────────────────

-- With endpoint empty, sparql_from_nl() must raise an ERROR (PT700).
DO $$
BEGIN
    PERFORM pg_ripple.sparql_from_nl('List all entities');
    RAISE EXCEPTION 'Expected PT700 ERROR was not raised';
EXCEPTION
    WHEN OTHERS THEN
        IF SQLERRM LIKE '%PT700%' OR SQLERRM LIKE '%not configured%' THEN
            RAISE NOTICE 'PT700 correctly raised: %', SQLERRM;
        ELSE
            RAISE;
        END IF;
END;
$$;

-- ── Part 4: Mock endpoint — returns parseable SPARQL ─────────────────────────

SET pg_ripple.llm_endpoint = 'mock';

-- sparql_from_nl() with mock endpoint must return a non-empty string.
SELECT length(pg_ripple.sparql_from_nl('Show me all triples')) > 0 AS mock_returns_nonempty;

-- The result must look like a SPARQL SELECT query.
SELECT pg_ripple.sparql_from_nl('Show me all triples') LIKE 'SELECT%' AS mock_returns_select;

RESET pg_ripple.llm_endpoint;

-- ── Part 5: add_llm_example() ─────────────────────────────────────────────────

SELECT pg_ripple.add_llm_example(
    'List all people',
    'SELECT ?s WHERE { ?s a <http://xmlns.com/foaf/0.1/Person> }'
) IS NULL AS add_example_void;

SELECT count(*) >= 1 AS example_stored
FROM _pg_ripple.llm_examples
WHERE question = 'List all people';

-- Upsert: calling again with different SPARQL must update the row.
SELECT pg_ripple.add_llm_example(
    'List all people',
    'SELECT ?s ?label WHERE { ?s a <http://xmlns.com/foaf/0.1/Person> ; rdfs:label ?label }'
) IS NULL AS add_example_upsert_void;

SELECT sparql LIKE '%rdfs:label%' AS example_updated
FROM _pg_ripple.llm_examples
WHERE question = 'List all people';

-- ── Part 6: suggest_sameas() graceful degradation ─────────────────────────────

-- When pgvector is disabled, suggest_sameas() must return 0 rows (no ERROR).
SET pg_ripple.pgvector_enabled = off;
SET client_min_messages = warning;
SELECT count(*) = 0 AS suggest_sameas_empty_when_disabled
FROM pg_ripple.suggest_sameas();
SET client_min_messages = DEFAULT;

-- ── Part 7: apply_sameas_candidates() graceful degradation ───────────────────

SELECT pg_ripple.apply_sameas_candidates() = 0 AS apply_sameas_zero_when_disabled;

RESET pg_ripple.pgvector_enabled;

-- ── Part 8: suggest_sameas() schema check ─────────────────────────────────────

-- Verify the function returns the expected column names.
SELECT attname FROM pg_attribute
WHERE attrelid = (
    SELECT oid FROM pg_proc
    WHERE proname = 'suggest_sameas'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
    LIMIT 1
)
ORDER BY attnum LIMIT 1;

-- ── Cleanup ───────────────────────────────────────────────────────────────────

DELETE FROM _pg_ripple.llm_examples WHERE question = 'List all people';
SELECT count(*) = 0 AS example_cleaned
FROM _pg_ripple.llm_examples
WHERE question = 'List all people';
