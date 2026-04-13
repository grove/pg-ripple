-- pg_regress test: dictionary encode/decode round-trips
-- sql/dictionary.sql

-- Basic IRI round-trip
SELECT pg_ripple.decode_id(pg_ripple.encode_term('https://example.org/subject', 0))
    = 'https://example.org/subject' AS iri_roundtrip;

-- Two distinct IRIs get distinct IDs
SELECT pg_ripple.encode_term('https://example.org/a', 0)
    != pg_ripple.encode_term('https://example.org/b', 0) AS distinct_ids;

-- Same IRI encoded twice returns the same ID (idempotent)
SELECT pg_ripple.encode_term('https://example.org/same', 0)
    = pg_ripple.encode_term('https://example.org/same', 0) AS idempotent;

-- Blank node (kind=1) gets a different ID than the same string as an IRI (kind=0)
SELECT pg_ripple.encode_term('node1', 1) != pg_ripple.encode_term('node1', 0)
    AS blank_vs_iri_distinct;

-- Unknown ID decodes to NULL
SELECT pg_ripple.decode_id(0) IS NULL AS unknown_id_is_null;
