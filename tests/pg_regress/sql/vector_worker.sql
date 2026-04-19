-- pg_regress test: embedding background worker queue (v0.28.0)
-- Tests that _pg_ripple.embedding_queue is populated when auto_embed = true.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- Force the pg_ripple shared library to load so GUCs are registered.
SELECT pg_ripple.triple_count() >= 0 AS extension_loaded;

-- ── embedding_queue table exists ──────────────────────────────────────────────
SELECT EXISTS(
    SELECT 1 FROM information_schema.tables
    WHERE table_schema = '_pg_ripple'
      AND table_name   = 'embedding_queue'
) AS embedding_queue_table_exists;

-- ── auto_embed GUC is registered with default value 'off' ────────────────────
SELECT current_setting('pg_ripple.auto_embed') AS auto_embed_default;

-- ── embedding_batch_size GUC is registered ────────────────────────────────────
SELECT current_setting('pg_ripple.embedding_batch_size')::int >= 1
    AS embedding_batch_size_registered;

-- ── use_graph_context GUC is registered ──────────────────────────────────────
SELECT current_setting('pg_ripple.use_graph_context') AS use_graph_context_default;

-- ── vector_federation_timeout_ms GUC is registered ───────────────────────────
SELECT current_setting('pg_ripple.vector_federation_timeout_ms')::int >= 100
    AS vector_federation_timeout_ms_registered;

-- ── auto_embed trigger exists on dictionary ───────────────────────────────────
SELECT EXISTS(
    SELECT 1 FROM pg_trigger t
    JOIN pg_class c ON c.oid = t.tgrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname = '_pg_ripple'
      AND c.relname = 'dictionary'
      AND t.tgname  = 'auto_embed_dict_trigger'
) AS auto_embed_trigger_exists;

-- ── Insert with auto_embed=off does NOT populate the queue ────────────────────
-- Truncate the queue first.
TRUNCATE _pg_ripple.embedding_queue;

SET pg_ripple.auto_embed = 'off';
SELECT pg_ripple.insert_triple(
    '<https://worker-test.example/Entity1>',
    '<http://www.w3.org/2000/01/rdf-schema#label>',
    '"entity one"'
) IS NULL AS triple_inserted;

SELECT count(*) = 0 AS queue_empty_when_auto_embed_off
FROM _pg_ripple.embedding_queue;

-- ── Insert with auto_embed=on DOES populate the queue ─────────────────────────
-- Note: the trigger only fires on new dictionary insertions.
-- We insert a new IRI not yet in the dictionary.
SET pg_ripple.auto_embed = 'on';
SELECT pg_ripple.insert_triple(
    '<https://worker-test.example/NewEntity42>',
    '<http://www.w3.org/2000/01/rdf-schema#label>',
    '"new entity forty two"'
) IS NULL AS triple_inserted_auto_embed_on;

-- The queue should now contain at least one entry for the new entity IRI.
SELECT count(*) >= 1 AS queue_populated_when_auto_embed_on
FROM _pg_ripple.embedding_queue;

-- Reset for subsequent tests.
SET pg_ripple.auto_embed = 'off';
TRUNCATE _pg_ripple.embedding_queue;
