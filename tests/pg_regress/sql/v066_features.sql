-- pg_regress test: v0.66.0 feature gate — streaming cursors, Arrow Flight v2,
-- WCOJ explain metadata, streaming observability, BRIN summarise API.
--
-- Tests:
-- 1. New GUCs exist with correct defaults.
-- 2. export_arrow_flight() returns arrow_flight_v2 ticket type.
-- 3. streaming_metrics() returns a JSONB object with all expected keys.
-- 4. citus_brin_summarise_all() is callable.
-- 5. explain_sparql_jsonb() includes wcoj metadata block.
-- 6. sparql_cursor() function exists.

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- ── Part 1: New GUCs ─────────────────────────────────────────────────────────

-- 1a. arrow_flight_expiry_secs default = 3600.
SHOW pg_ripple.arrow_flight_expiry_secs;

-- 1b. GUC can be updated and reset.
SET pg_ripple.arrow_flight_expiry_secs = 7200;
SHOW pg_ripple.arrow_flight_expiry_secs;
SET pg_ripple.arrow_flight_expiry_secs = 3600;

-- ── Part 2: Arrow Flight v2 ticket ───────────────────────────────────────────
-- Allow unsigned tickets so the test works without a signing secret.
SET pg_ripple.arrow_unsigned_tickets_allowed = on;

-- 2a. export_arrow_flight() returns non-empty BYTEA.
SELECT length(pg_ripple.export_arrow_flight('DEFAULT')) > 0
    AS ticket_nonempty;

-- 2b. Ticket type is arrow_flight_v2.
SELECT (convert_from(
    pg_ripple.export_arrow_flight('DEFAULT'),
    'UTF8'
)::jsonb)->>'type' AS ticket_type;

-- 2c. Ticket contains required fields.
SELECT (convert_from(
    pg_ripple.export_arrow_flight('<https://v066.test/graph>'),
    'UTF8'
)::jsonb) ? 'exp' AS has_expiry;

-- ── Part 3: streaming_metrics() ──────────────────────────────────────────────

-- 3a. streaming_metrics() returns a JSONB object.
SELECT jsonb_typeof(pg_ripple.streaming_metrics()) = 'object'
    AS metrics_is_object;

-- 3b. All expected keys are present.
SELECT (pg_ripple.streaming_metrics() ? 'cursor_pages_opened')
    AND (pg_ripple.streaming_metrics() ? 'cursor_pages_fetched')
    AND (pg_ripple.streaming_metrics() ? 'cursor_rows_streamed')
    AND (pg_ripple.streaming_metrics() ? 'arrow_batches_sent')
    AND (pg_ripple.streaming_metrics() ? 'arrow_ticket_rejections')
    AND (pg_ripple.streaming_metrics() ? 'citus_brin_summarise_completed')
    AS all_metric_keys_present;

-- ── Part 4: citus_brin_summarise_all() ───────────────────────────────────────

-- 4a. Function is callable (returns 0 when no VP main tables exist yet).
SELECT pg_ripple.citus_brin_summarise_all() >= 0 AS brin_summarise_ok;

-- ── Part 5: WCOJ explain metadata ────────────────────────────────────────────

-- 5a. explain_sparql function exists in pg_ripple schema (WCOJ metadata added v0.66.0).
SELECT EXISTS (
    SELECT 1 FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'pg_ripple' AND p.proname = 'explain_sparql'
) AS explain_sparql_exists;

-- 5b. The sparql_explain_jsonb test covers full WCOJ block content.
-- Here we verify the feature is registered in feature_status.
SELECT status AS wcoj_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'wcoj';

-- ── Part 6: sparql_cursor() ──────────────────────────────────────────────────

-- 6a. sparql_cursor exists in pg_ripple schema.
SELECT EXISTS (
    SELECT 1 FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'pg_ripple' AND p.proname = 'sparql_cursor'
) AS sparql_cursor_exists;

-- 6b. sparql_cursor runs without error on an empty result.
SELECT count(*) >= 0 AS cursor_empty_ok
FROM pg_ripple.sparql_cursor(
    'SELECT ?s WHERE { ?s <https://v066.test/nonexistent> ?o }'
);
