-- datalog_explain.sql — explain_datalog() tests (v0.40.0)
--
-- Verifies that explain_datalog(text) returns JSONB with:
--   1. Required keys: strata, rules, sql_per_rule, last_run_stats
--   2. 'strata' is an array
--   3. 'rules' contains the expected rule predicate
--   4. Unknown rule set returns empty strata (graceful)

SET search_path TO pg_ripple, public;

CREATE EXTENSION IF NOT EXISTS pg_ripple;

-- ── Setup: load a simple rule set ─────────────────────────────────────────────

SELECT pg_ripple.load_rules($$
?x <https://dl.explain.test/ancestor> ?y :- ?x <https://dl.explain.test/parent> ?y .
?x <https://dl.explain.test/ancestor> ?z :- ?x <https://dl.explain.test/parent> ?y , ?y <https://dl.explain.test/ancestor> ?z .
$$, 'dl_explain_test') AS rules_loaded;

SELECT pg_ripple.load_ntriples($$
<https://dl.explain.test/alice> <https://dl.explain.test/parent> <https://dl.explain.test/bob> .
<https://dl.explain.test/bob>   <https://dl.explain.test/parent> <https://dl.explain.test/carol> .
$$) = 2 AS triples_loaded;

-- ── 1. Required keys present ──────────────────────────────────────────────────

SELECT
    (result ? 'strata')        AND
    (result ? 'rules')         AND
    (result ? 'sql_per_rule')  AND
    (result ? 'last_run_stats') AS has_required_keys
FROM (
    SELECT pg_ripple.explain_datalog('dl_explain_test') AS result
) t;

-- ── 2. 'strata' is an array ───────────────────────────────────────────────────

SELECT jsonb_typeof(result->'strata') = 'array' AS strata_is_array
FROM (
    SELECT pg_ripple.explain_datalog('dl_explain_test') AS result
) t;

-- ── 3. 'rules' is an array with at least 2 entries ───────────────────────────

SELECT jsonb_array_length(result->'rules') >= 2 AS rules_count_correct
FROM (
    SELECT pg_ripple.explain_datalog('dl_explain_test') AS result
) t;

-- ── 4. 'sql_per_rule' is an array ─────────────────────────────────────────────

SELECT jsonb_typeof(result->'sql_per_rule') = 'array' AS sql_per_rule_is_array
FROM (
    SELECT pg_ripple.explain_datalog('dl_explain_test') AS result
) t;

-- ── 5. Unknown rule set returns JSON with empty arrays ───────────────────────

SELECT
    jsonb_array_length(result->'strata') = 0 AS empty_strata_for_unknown
FROM (
    SELECT pg_ripple.explain_datalog('nonexistent_rule_set_xyz') AS result
) t;

-- ── Cleanup ───────────────────────────────────────────────────────────────────
SELECT pg_ripple.drop_rules('dl_explain_test') >= 0 AS rules_dropped;
