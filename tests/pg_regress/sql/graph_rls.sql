-- graph_rls.sql: v0.14.0 graph-level Row-Level Security tests

SET allow_system_table_mods = on;

-- Load triples into the default graph (g=0) and a named graph
SELECT pg_ripple.load_ntriples(
    '<https://rls.test/s1> <https://rls.test/p> <https://rls.test/o1> .'
);

SELECT pg_ripple.insert_triple(
    '<https://rls.test/s2>',
    '<https://rls.test/p>',
    '<https://rls.test/o2>',
    '<https://rls.test/secret_graph>'
) > 0 AS insert_ok;

-- grant_graph: add access for a role (even if it does not exist yet — just tests the function)
SELECT pg_ripple.grant_graph('postgres', '<https://rls.test/secret_graph>', 'read');

-- list_graph_access should return the grant
SELECT count(*) >= 1 AS grant_present
FROM jsonb_array_elements(pg_ripple.list_graph_access()) AS entry
WHERE entry->>'permission' = 'read';

-- grant admin permission
SELECT pg_ripple.grant_graph('postgres', '<https://rls.test/secret_graph>', 'admin');

-- two entries now
SELECT count(*) >= 2 AS two_grants
FROM jsonb_array_elements(pg_ripple.list_graph_access()) AS entry;

-- revoke single permission
SELECT pg_ripple.revoke_graph('postgres', '<https://rls.test/secret_graph>', 'read');

SELECT count(*) = 1 AS one_grant_left
FROM jsonb_array_elements(pg_ripple.list_graph_access()) AS entry;

-- revoke all permissions for role+graph
SELECT pg_ripple.revoke_graph('postgres', '<https://rls.test/secret_graph>');

SELECT count(*) = 0 AS no_grants
FROM jsonb_array_elements(pg_ripple.list_graph_access()) AS entry;

-- enable_graph_rls() should succeed and return true
SELECT pg_ripple.enable_graph_rls() AS rls_enabled;

-- After RLS is enabled, grant read again
SELECT pg_ripple.grant_graph('postgres', '<https://rls.test/secret_graph>', 'read');

-- Superuser can read the named graph (superuser bypasses RLS by default in PG)
SELECT count(*) >= 1 AS superuser_sees_secret
FROM pg_ripple.find_triples(NULL, '<https://rls.test/p>', NULL);

-- rls_bypass GUC: superuser can toggle it (just verify it can be set)
SET pg_ripple.rls_bypass = on;
SELECT current_setting('pg_ripple.rls_bypass') = 'on' AS bypass_guc_works;
SET pg_ripple.rls_bypass = off;

-- Clean up
SELECT pg_ripple.revoke_graph('postgres', '<https://rls.test/secret_graph>', 'read');
SELECT pg_ripple.drop_graph('<https://rls.test/secret_graph>') >= 0 AS cleanup;
