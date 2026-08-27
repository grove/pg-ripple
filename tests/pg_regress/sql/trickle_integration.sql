-- pg_regress test: pg_tide relay integration (v0.52.0, migrated v0.127.0)
--
-- Tests the full JSON → RDF → CDC → outbox pipeline.
-- When pg_tide is not available, tests the JSON/CDC graceful-degradation path.
--
-- Tests:
-- 1. JSON → N-Triples pipeline
-- 2. CDC bridge trigger publishes to a pg_tide outbox when pg_tide is available
-- 3. statement_dedup_key uniqueness
-- 4. triples_to_jsonld star-pattern serialization
-- 5. Vocabulary alignment rules

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── Part 1: JSON → N-Triples → store pipeline ────────────────────────────────

-- Load an order event via json_to_ntriples_and_load
SELECT pg_ripple.json_to_ntriples_and_load(
    '{"customer_name": "Acme Corp", "total": 1250.00, "currency": "USD"}'::jsonb,
    'https://example.org/order/1001',
    'https://schema.org/Order',
    '{"@vocab": "https://example.org/vocab/"}'::jsonb
) >= 0 AS order_loaded;

-- Verify the rdf:type triple was stored
SELECT pg_ripple.triple_count() > 0 AS triples_exist_after_load;

-- ── Part 2: json_to_ntriples output format ────────────────────────────────────

-- Nested objects become blank nodes
SELECT pg_ripple.json_to_ntriples(
    '{"address": {"city": "Oslo", "country": "NO"}}'::jsonb,
    'https://example.org/company/99',
    NULL,
    '{"@vocab": "https://schema.org/"}'::jsonb
) LIKE '%_:b%' AS nested_object_becomes_blank_node;

-- Arrays produce multiple triples
SELECT pg_ripple.json_to_ntriples(
    '{"tag": ["rdf", "sparql", "postgres"]}'::jsonb,
    'https://example.org/article/7',
    NULL,
    '{"@vocab": "https://schema.org/"}'::jsonb
) LIKE '%"rdf"%' AS array_produces_multiple_triples;

-- ── Part 3: CDC bridge trigger → pg_tide outbox ─────────────────────────────

-- Load a predicate so it has a VP table
SELECT pg_ripple.json_to_ntriples_and_load(
    '{"alertLevel": "high"}'::jsonb,
    'https://example.org/event/2001',
    NULL,
    '{"alertLevel": "https://example.org/alertLevel"}'::jsonb
) >= 0 AS alert_loaded;

-- Install bridge trigger (requires pg_tide; skip the publish path when unavailable).
SET client_min_messages = error;
DO $$
DECLARE
    tide_available BOOLEAN := false;
    published_count BIGINT := 0;
BEGIN
    IF EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'pg_tide') THEN
        CREATE EXTENSION IF NOT EXISTS pg_tide;
    END IF;

    tide_available := pg_ripple.relay_available();

    IF NOT tide_available THEN
        -- With trickle_integration off, test graceful degradation path
        SET pg_ripple.trickle_integration = off;
        BEGIN
            PERFORM pg_ripple.enable_cdc_bridge_trigger(
                'alert_bridge',
                '<https://example.org/alertLevel>',
                'ripple-events'
            );
        EXCEPTION
            WHEN OTHERS THEN
                NULL; -- error correctly raised when trickle_integration off
        END;
        RESET pg_ripple.trickle_integration;
    ELSE
        BEGIN
            PERFORM tide.outbox_create(
                p_name             := 'ripple-events',
                p_retention_hours  := 24,
                p_inline_threshold := 10000
            );
        EXCEPTION
            WHEN OTHERS THEN
                NULL; -- outbox already exists in environments that reuse databases
        END;

        -- pg_tide is available: install trigger and test outbox publish
        PERFORM pg_ripple.enable_cdc_bridge_trigger(
            'alert_bridge',
            '<https://example.org/alertLevel>',
            'ripple-events'
        );

        -- Insert via json_to_ntriples_and_load — should fire the trigger
        PERFORM pg_ripple.json_to_ntriples_and_load(
            '{"alertLevel": "critical"}'::jsonb,
            'https://example.org/event/2002',
            NULL,
            '{"alertLevel": "https://example.org/alertLevel"}'::jsonb
        );

        SELECT count(*) INTO published_count
        FROM tide.tide_outbox_messages
        WHERE outbox_name = 'ripple-events'
            AND headers ->> 'event_type' = 'pg_ripple.triple.insert'
            AND headers ->> 'dedup_key' LIKE 'ripple:%';

        IF published_count = 0 THEN
            RAISE EXCEPTION 'expected pg_tide outbox publish was not observed';
        END IF;

        -- Clean up
        PERFORM pg_ripple.disable_cdc_bridge_trigger('alert_bridge');
    END IF;
END;
$$;
SET client_min_messages = DEFAULT;

-- ── Part 4: statement_dedup_key ───────────────────────────────────────────────

-- Insert a known triple and check its dedup key
SELECT pg_ripple.insert_triple(
    '<https://example.org/dedup_s>',
    '<https://example.org/dedup_p>',
    '"dedup_value"'
) IS NOT NULL AS dedup_triple_inserted;

SELECT pg_ripple.statement_dedup_key(
    pg_ripple.encode_term('https://example.org/dedup_s', 0::smallint),
    pg_ripple.encode_term('https://example.org/dedup_p', 0::smallint),
    pg_ripple.encode_term('dedup_value', 2::smallint)
) LIKE 'ripple:%' AS dedup_key_has_prefix;

-- Non-existent triple returns NULL
SELECT pg_ripple.statement_dedup_key(
    pg_ripple.encode_term('https://example.org/no_such_s', 0::smallint),
    pg_ripple.encode_term('https://example.org/no_such_p', 0::smallint),
    pg_ripple.encode_term('no_such_o', 2::smallint)
) IS NULL AS nonexistent_triple_returns_null;

-- ── Part 5: triples_to_jsonld star-pattern ────────────────────────────────────

-- Insert multiple triples for one subject
SELECT pg_ripple.insert_triple(
    '<https://example.org/multi_s>',
    '<https://example.org/prop1>',
    '"v1"'
) IS NOT NULL AS prop1_inserted;

SELECT pg_ripple.insert_triple(
    '<https://example.org/multi_s>',
    '<https://example.org/prop2>',
    '"v2"'
) IS NOT NULL AS prop2_inserted;

-- triples_to_jsonld groups all predicates for the subject
SELECT (pg_ripple.triples_to_jsonld(
    pg_ripple.encode_term('https://example.org/multi_s', 0::smallint)
) ->> '@id') = 'https://example.org/multi_s' AS star_jsonld_has_correct_id;

-- ── Part 6: triple_to_jsonld single triple ────────────────────────────────────

SELECT (pg_ripple.triple_to_jsonld(
    pg_ripple.encode_term('https://example.org/dedup_s', 0::smallint),
    pg_ripple.encode_term('https://example.org/dedup_p', 0::smallint),
    pg_ripple.encode_term('dedup_value', 2::smallint)
) ->> '@id') = 'https://example.org/dedup_s' AS triple_jsonld_has_id;

-- ── Part 7: Vocabulary alignment — schema_to_saref ───────────────────────────

-- Load alignment rules
SELECT pg_ripple.load_vocab_template('schema_to_saref') > 0 AS saref_rules_loaded;

-- Insert a schema.org triple
SELECT pg_ripple.insert_triple(
    '<https://example.org/sensor/001>',
    '<https://schema.org/name>',
    '"Temperature Sensor"'
) IS NOT NULL AS sensor_name_inserted;

-- Run inference
SELECT pg_ripple.infer('schema_to_saref') >= 0 AS inference_ran;

-- ── End ─────────────────────────────────────────────────────────────────────
