-- v0.134.0: typed metadata, portal-backed bindings, graph rows, and counters.

LOAD '$libdir/pg_ripple';

\pset format unaligned
\pset tuples_only on

SELECT (_pg_ripple.sparql_stream_metadata(
    'SELECT ?s ?o WHERE { ?s <https://v0134.test/p> ?o }'
) ->> 'form') = 'select' AS stream_metadata_form;
SELECT (_pg_ripple.sparql_stream_metadata(
    'SELECT ?s ?o WHERE { ?s <https://v0134.test/p> ?o }'
) -> 'variables') = '["s", "o"]'::jsonb AS stream_metadata_variables;

SELECT pg_ripple.load_ntriples(
    '<https://v0134.test/s> <https://v0134.test/p> "hello"@en .'
) >= 1 AS stream_fixture_loaded;

SELECT (SELECT result -> 's' ->> 'type'
        FROM _pg_ripple.sparql_stream_bindings(
            'SELECT ?s ?o WHERE { ?s <https://v0134.test/p> ?o }'
        )
        LIMIT 1) = 'uri' AS stream_iri_is_typed;
SELECT (SELECT result -> 'o' ->> 'xml:lang'
        FROM _pg_ripple.sparql_stream_bindings(
            'SELECT ?s ?o WHERE { ?s <https://v0134.test/p> ?o }'
        )
        LIMIT 1) = 'en' AS stream_language_is_preserved;

SELECT count(*) >= 1 AS stream_construct_has_rows
FROM _pg_ripple.sparql_stream_triples(
    'CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }'
);

SELECT (pg_ripple.streaming_metrics() ->> 'cursor_rows_streamed')::bigint >= 2
    AS stream_row_counter_is_live;
