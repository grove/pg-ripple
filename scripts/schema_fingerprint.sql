-- Emit one normalized JSON document describing the installed pg_ripple schema.
-- Run with: psql -XAt -f scripts/schema_fingerprint.sql > fingerprint.json
WITH schemas AS (
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'name', n.nspname,
        'owner', pg_get_userbyid(n.nspowner)
    ) ORDER BY n.nspname), '[]'::jsonb) AS value
    FROM pg_namespace n
    WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
), tables AS (
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'schema', n.nspname,
        'name', c.relname,
        'kind', c.relkind::text,
        'owner', pg_get_userbyid(c.relowner),
        'persistence', c.relpersistence::text,
        'rls', c.relrowsecurity,
        'force_rls', c.relforcerowsecurity,
        'comment', obj_description(c.oid, 'pg_class'),
        'columns', COALESCE((
            SELECT jsonb_agg(jsonb_build_object(
                'name', a.attname,
                'type', format_type(a.atttypid, a.atttypmod),
                'not_null', a.attnotnull,
                'default', pg_get_expr(d.adbin, d.adrelid),
                'identity', a.attidentity::text,
                'generated', a.attgenerated::text
            ) ORDER BY a.attnum)
            FROM pg_attribute a
            LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
            WHERE a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
        ), '[]'::jsonb)
    ) ORDER BY n.nspname, c.relname), '[]'::jsonb) AS value
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
      AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
), indexes AS (
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'schema', n.nspname,
        'table', c.relname,
        'name', i.relname,
        'definition', pg_get_indexdef(i.oid),
        'comment', obj_description(i.oid, 'pg_class')
    ) ORDER BY n.nspname, c.relname, i.relname), '[]'::jsonb) AS value
    FROM pg_index x
    JOIN pg_class c ON c.oid = x.indrelid
    JOIN pg_class i ON i.oid = x.indexrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
), constraints AS (
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'schema', n.nspname,
        'table', c.relname,
        'name', con.conname,
        'type', con.contype::text,
        'definition', pg_get_constraintdef(con.oid, true),
        'comment', obj_description(con.oid, 'pg_constraint')
    ) ORDER BY n.nspname, c.relname, con.conname), '[]'::jsonb) AS value
    FROM pg_constraint con
    JOIN pg_class c ON c.oid = con.conrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
), triggers AS (
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'schema', n.nspname,
        'table', c.relname,
        'name', t.tgname,
        'enabled', t.tgenabled::text,
        'definition', pg_get_triggerdef(t.oid, true),
        'comment', obj_description(t.oid, 'pg_trigger')
    ) ORDER BY n.nspname, c.relname, t.tgname), '[]'::jsonb) AS value
    FROM pg_trigger t
    JOIN pg_class c ON c.oid = t.tgrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE NOT t.tgisinternal
      AND n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
), policies AS (
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'schema', n.nspname,
        'table', c.relname,
        'name', p.polname,
        'command', p.polcmd::text,
        'roles', ARRAY(SELECT pg_get_userbyid(r) FROM unnest(p.polroles) AS role(r) ORDER BY r),
        'using', pg_get_expr(p.polqual, p.polrelid),
        'check', pg_get_expr(p.polwithcheck, p.polrelid)
    ) ORDER BY n.nspname, c.relname, p.polname), '[]'::jsonb) AS value
    FROM pg_policy p
    JOIN pg_class c ON c.oid = p.polrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
), sequences AS (
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'schema', n.nspname,
        'name', c.relname,
        'owner', pg_get_userbyid(c.relowner),
        'type', format_type(s.seqtypid, NULL),
        'start', s.seqstart,
        'increment', s.seqincrement,
        'min', s.seqmin,
        'max', s.seqmax,
        'cache', s.seqcache,
        'cycle', s.seqcycle,
        'comment', obj_description(c.oid, 'pg_class')
    ) ORDER BY n.nspname, c.relname), '[]'::jsonb) AS value
    FROM pg_sequence s
    JOIN pg_class c ON c.oid = s.seqrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
), event_triggers AS (
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'name', e.evtname,
        'owner', pg_get_userbyid(e.evtowner),
        'event', e.evtevent,
        'enabled', e.evtenabled::text,
        'tags', e.evttags
    ) ORDER BY e.evtname), '[]'::jsonb) AS value
    FROM pg_event_trigger e
), functions AS (
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'schema', n.nspname,
        'name', p.proname,
        'kind', p.prokind::text,
        'arguments', pg_get_function_identity_arguments(p.oid),
        'result', pg_get_function_result(p.oid),
        'volatility', p.provolatile::text,
        'parallel', p.proparallel::text,
        'security_definer', p.prosecdef,
        'config', p.proconfig,
        'acl', ARRAY(
            SELECT a::text
            FROM unnest(COALESCE(p.proacl, ARRAY[]::aclitem[])) AS acl(a)
            ORDER BY a::text
        ),
        'comment', obj_description(p.oid, 'pg_proc')
    ) ORDER BY n.nspname, p.proname, pg_get_function_identity_arguments(p.oid)), '[]'::jsonb) AS value
    FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
), extensions AS (
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'name', e.extname,
        'version', e.extversion,
        'members', COALESCE((
            SELECT jsonb_agg(pg_describe_object(d.classid, d.objid, d.objsubid) ORDER BY pg_describe_object(d.classid, d.objid, d.objsubid))
            FROM pg_depend d
            WHERE d.refclassid = 'pg_extension'::regclass AND d.refobjid = e.oid
        ), '[]'::jsonb)
    ) ORDER BY e.extname), '[]'::jsonb) AS value
    FROM pg_extension e
), gucs AS (
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'name', name,
        'type', vartype,
        'default', boot_val
    ) ORDER BY name), '[]'::jsonb) AS value
    FROM pg_settings
    WHERE name LIKE 'pg_ripple.%'
)
SELECT jsonb_build_object(
    'schemas', schemas.value,
    'tables', tables.value,
    'indexes', indexes.value,
    'constraints', constraints.value,
    'triggers', triggers.value,
    'policies', policies.value,
    'sequences', sequences.value,
    'event_triggers', event_triggers.value,
    'functions', functions.value,
    'extensions', extensions.value,
    'gucs', gucs.value
)
FROM schemas, tables, indexes, constraints, triggers, policies, sequences,
     event_triggers, functions, extensions, gucs;
