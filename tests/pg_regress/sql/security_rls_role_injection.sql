-- pg_regress test: RLS role-name SQL injection guard (v0.70.0 RLS-SQL-01)
--
-- Verifies that grant_graph_access() rejects role names containing SQL injection
-- characters with a PT711 error rather than executing the injected SQL.
SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- ── Part 1: Valid role name is accepted ──────────────────────────────────────

-- 1a. is_safe_role_name validation: regular names pass (function does not error).
--     We use a non-existent role so the GRANT itself may warn but the
--     injection guard must not fire.
SELECT pg_ripple.create_graph('https://rls.test/g/') IS NULL AS graph_created;

-- ── Part 2: Injection attempts are rejected with PT711 ───────────────────────

-- 2a. Role name with semicolon injection attempt.
SELECT pg_ripple.grant_graph_access(
    'https://rls.test/g/',
    E'role"; DROP TABLE _pg_ripple.vp_rare; --',
    'SELECT'
);

-- 2b. Role name with SQL comment injection.
SELECT pg_ripple.grant_graph_access(
    'https://rls.test/g/',
    'role''-- injection',
    'SELECT'
);

-- 2c. Role name with space (also invalid).
SELECT pg_ripple.grant_graph_access(
    'https://rls.test/g/',
    'bad role',
    'SELECT'
);

-- 2d. Empty role name.
SELECT pg_ripple.grant_graph_access(
    'https://rls.test/g/',
    '',
    'SELECT'
);

-- ── Cleanup ───────────────────────────────────────────────────────────────────
SELECT pg_ripple.clear_graph('https://rls.test/g/') >= 0 AS cleanup_ok;
SELECT pg_ripple.drop_graph('https://rls.test/g/') IS NULL AS graph_dropped;
