-- pg_regress test: dictionary encode/decode round-trips

-- Basic IRI round-trip
SELECT pg_ripple.decode_id(pg_ripple.encode_term('https://example.org/subject', 0::smallint))
    = 'https://example.org/subject' AS iri_roundtrip;

-- Two distinct IRIs get distinct IDs
SELECT pg_ripple.encode_term('https://example.org/a', 0::smallint)
    != pg_ripple.encode_term('https://example.org/b', 0::smallint) AS distinct_ids;

-- Same IRI encoded twice returns the same ID (idempotent)
SELECT pg_ripple.encode_term('https://example.org/same', 0::smallint)
    = pg_ripple.encode_term('https://example.org/same', 0::smallint) AS idempotent;

-- Blank node (kind=1) gets a different ID than the same string as an IRI (kind=0)
SELECT pg_ripple.encode_term('node1', 1::smallint) != pg_ripple.encode_term('node1', 0::smallint)
    AS blank_vs_iri_distinct;

-- Unknown ID decodes to NULL
-- Unknown positive ID returns NULL (negative IDs are now reserved for inline-encoded values).
SELECT pg_ripple.decode_id(999999999) IS NULL AS unknown_id_is_null;
