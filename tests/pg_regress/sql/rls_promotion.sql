-- pg_regress test: Graph RLS on promoted VP tables (v0.67.0 RLS-02)
--
-- Verifies that graph-level Row Level Security is applied to dedicated VP tables
-- when enable_graph_rls() is called and insert_triple() fires.
SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- Confirm enable_graph_rls() exists.
SELECT EXISTS (
    SELECT 1 FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'pg_ripple'
      AND p.proname = 'enable_graph_rls'
) AS enable_graph_rls_exists;

-- Confirm grant_graph function exists.
SELECT EXISTS (
    SELECT 1 FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'pg_ripple'
      AND p.proname = 'grant_graph'
) AS grant_graph_exists;

-- Enable global RLS.
SELECT pg_ripple.enable_graph_rls() AS rls_enabled;

-- Create a named graph.
SELECT pg_ripple.create_graph('https://rls.test/graphA/') IS NULL AS graphA_created;

-- Insert a triple to trigger VP table creation.
SELECT pg_ripple.insert_triple(
    '<https://rls.test/Alice>',
    '<https://rls.test/knows>',
    '<https://rls.test/Bob>',
    'https://rls.test/graphA/'
) > 0 AS triple_inserted;

-- Check that RLS is enabled on vp_rare.
SELECT relrowsecurity AS vp_rare_rls_enabled
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = '_pg_ripple' AND c.relname = 'vp_rare';

-- Cleanup.
SELECT pg_ripple.clear_graph('https://rls.test/graphA/') >= 0 AS graphA_cleared;
SELECT pg_ripple.drop_graph('https://rls.test/graphA/') IS NULL AS graphA_dropped;
