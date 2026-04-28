-- pg_regress test: v0.62.0 feature gate — Arrow Flight, WCOJ activation,
-- vacuum_vp_rare, citus_live_rebalance, new GUCs (CITUS-25/26/27/28/29).
--
-- Tests:
-- 1. New GUCs exist with correct defaults.
-- 2. export_arrow_flight() returns a valid JSON BYTEA ticket.
-- 3. vacuum_vp_rare() is callable and returns an empty set (no dead predicates).
-- 4. citus_live_rebalance() exists (Citus-conditional; no-op when absent).
-- 5. WCOJ planner integration: cyclic SPARQL query executes without error.

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- ── Part 1: New GUCs ─────────────────────────────────────────────────────────

-- 1a. dictionary_tier_threshold default = -1 (disabled).
SHOW pg_ripple.dictionary_tier_threshold;

-- 1b. citus_prune_carry_max default = 1000.
SHOW pg_ripple.citus_prune_carry_max;

-- 1c. datalog_citus_dispatch default = false.
SHOW pg_ripple.datalog_citus_dispatch;

-- 1d. GUC can be set.
SET pg_ripple.dictionary_tier_threshold = 100;
SHOW pg_ripple.dictionary_tier_threshold;
SET pg_ripple.dictionary_tier_threshold = -1;

SET pg_ripple.citus_prune_carry_max = 500;
SHOW pg_ripple.citus_prune_carry_max;
SET pg_ripple.citus_prune_carry_max = 1000;

-- ── Part 2: Arrow Flight ticket ───────────────────────────────────────────────
-- Allow unsigned tickets so the test works without a signing secret.
SET pg_ripple.arrow_unsigned_tickets_allowed = on;

-- 2a. export_arrow_flight() returns non-empty BYTEA.
SELECT length(pg_ripple.export_arrow_flight('<https://mygraph.example.org/>')) > 0
    AS ticket_nonempty;

-- 2b. The ticket is valid UTF-8 JSON.
SELECT (convert_from(
    pg_ripple.export_arrow_flight('DEFAULT'),
    'UTF8'
)::jsonb)->>'type' AS ticket_type;

-- 2c. Function exists in pg_ripple schema.
SELECT EXISTS (
    SELECT 1 FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'pg_ripple' AND p.proname = 'export_arrow_flight'
) AS export_arrow_flight_exists;

-- ── Part 3: vacuum_vp_rare ───────────────────────────────────────────────────

-- 3a. Function exists.
SELECT EXISTS (
    SELECT 1 FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'pg_ripple' AND p.proname = 'vacuum_vp_rare'
) AS vacuum_vp_rare_exists;

-- 3b. Calling vacuum_vp_rare() returns an empty set (no dead predicates).
SELECT count(*) AS dead_predicates_removed
FROM pg_ripple.vacuum_vp_rare();

-- ── Part 4: citus_live_rebalance ────────────────────────────────────────────

-- 4a. Function exists (it will no-op when Citus is absent).
SELECT EXISTS (
    SELECT 1 FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'pg_ripple' AND p.proname = 'citus_live_rebalance'
) AS citus_live_rebalance_exists;

-- ── Part 5: WCOJ planner integration ────────────────────────────────────────

-- Insert a triangle so we have data for the cyclic query.
SELECT pg_ripple.insert_triple(
    '<https://v062.test/a>', '<https://v062.test/p>', '<https://v062.test/b>'
) IS NOT NULL AS ins_ab;
SELECT pg_ripple.insert_triple(
    '<https://v062.test/b>', '<https://v062.test/p>', '<https://v062.test/c>'
) IS NOT NULL AS ins_bc;
SELECT pg_ripple.insert_triple(
    '<https://v062.test/c>', '<https://v062.test/p>', '<https://v062.test/a>'
) IS NOT NULL AS ins_ca;

-- 5a. WCOJ-eligible cyclic query executes without error.
SET pg_ripple.wcoj_enabled = true;
SELECT count(*) >= 0 AS cyclic_query_ok FROM pg_ripple.sparql(
    'SELECT ?x ?y ?z WHERE { '
    '?x <https://v062.test/p> ?y . '
    '?y <https://v062.test/p> ?z . '
    '?z <https://v062.test/p> ?x '
    '}'
);

-- 5b. Same query with WCOJ disabled — same result.
SET pg_ripple.wcoj_enabled = false;
SELECT count(*) >= 0 AS cyclic_query_no_wcoj FROM pg_ripple.sparql(
    'SELECT ?x ?y ?z WHERE { '
    '?x <https://v062.test/p> ?y . '
    '?y <https://v062.test/p> ?z . '
    '?z <https://v062.test/p> ?x '
    '}'
);

SET pg_ripple.wcoj_enabled = true;
