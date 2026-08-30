-- v0.135.0: typed application bindings and governed prefix mode.

LOAD '$libdir/pg_ripple';

\pset format unaligned
\pset tuples_only on

SELECT current_setting('pg_ripple.sparql_prefix_mode') = 'strict';
SELECT current_setting('pg_ripple.sparql_max_initial_bindings') = '64';

SELECT pg_ripple.load_ntriples(
    '<https://v0135.test/alice> <https://v0135.test/email> "alice@example.test" .'
) = 1;

SELECT count(*) = 1
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { ?s <https://v0135.test/email> ?email }',
    '{"email":{"type":"literal","value":"alice@example.test"}}'::jsonb
);
SELECT count(*) = 1
FROM pg_ripple.sparql(
    'SELECT ?email WHERE { ?s <https://v0135.test/email> ?o }',
    '{"email":{"type":"literal","value":"alice@example.test"}}'::jsonb
);
SELECT count(*) = 0
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { ?s <https://v0135.test/email> ?email }',
    jsonb_build_object(
        'email', jsonb_build_object(
            'type', 'literal',
            'value', 'not syntax } ; INSERT DATA { <x> <y> <z> } --'
        )
    )
);
SELECT (result ->> 's') = '<https://v0135.test/alice>'
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { ?s <https://v0135.test/email> ?email }',
    '{"email":{"type":"literal","value":"alice@example.test"}}'::jsonb
);
SELECT (result ->> 'result') = 'true'
FROM pg_ripple.sparql(
    'ASK WHERE { ?s <https://v0135.test/email> ?email }',
    '{"email":{"type":"literal","value":"alice@example.test"}}'::jsonb
);
SELECT count(*) = 1
FROM pg_ripple.sparql_construct(
    'CONSTRUCT { ?s <https://v0135.test/hasEmail> ?email } WHERE { ?s <https://v0135.test/email> ?email }',
    '{"email":{"type":"literal","value":"alice@example.test"}}'::jsonb
);
SELECT count(*) = 1
FROM pg_ripple.sparql_describe(
    'DESCRIBE ?s WHERE { ?s <https://v0135.test/email> ?email }',
    '{"email":{"type":"literal","value":"alice@example.test"}}'::jsonb
);
SELECT count(*) = 1
FROM pg_ripple.sparql_cursor(
    'SELECT ?s WHERE { ?s <https://v0135.test/email> ?email }',
    '{"email":{"type":"literal","value":"alice@example.test"}}'::jsonb
);

SELECT pg_ripple.plan_cache_reset();
SELECT count(*) FROM pg_ripple.sparql(
    'SELECT ?s WHERE { ?s <https://v0135.test/email> ?email }',
    '{"email":{"type":"literal","value":"alice@example.test"}}'::jsonb
);
SELECT count(*) FROM pg_ripple.sparql(
    'SELECT ?s WHERE { ?s <https://v0135.test/email> ?email }',
    '{"email":{"type":"literal","value":"nobody@example.test"}}'::jsonb
);
SELECT hits = 1 AND misses = 1 FROM pg_ripple.plan_cache_stats();

SELECT pg_ripple.plan_cache_reset();
DO $$
BEGIN
    FOR i IN 1..100 LOOP
        PERFORM * FROM pg_ripple.sparql(
            'SELECT ?s WHERE { ?s <https://v0135.test/email> ?email }',
            jsonb_build_object(
                'email', jsonb_build_object(
                    'type', 'literal', 'value', 'user-' || i::text
                )
            )
        );
    END LOOP;
END
$$;
SELECT hits = 99 AND misses = 1 FROM pg_ripple.plan_cache_stats();

DO $$
BEGIN
    PERFORM * FROM pg_ripple.sparql(
        'SELECT ?s WHERE { ?s <https://v0135.test/email> ?email }',
        '{"other":{"type":"literal","value":"x"}}'::jsonb
    );
    RAISE EXCEPTION 'expected PT0576';
EXCEPTION WHEN others THEN
    IF SQLERRM NOT LIKE '%PT0576%' THEN RAISE; END IF;
END
$$;

DO $$
BEGIN
    PERFORM * FROM pg_ripple.sparql(
        'INSERT DATA {}', '{}'::jsonb
    );
    RAISE EXCEPTION 'expected PT0579';
EXCEPTION WHEN others THEN
    IF SQLERRM NOT LIKE '%PT0579%' THEN RAISE; END IF;
END
$$;
