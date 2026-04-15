-- pg_ripple--0.3.0--0.4.0.sql
-- Migration from v0.3.0 (SPARQL Query Engine Basic) to v0.4.0 (RDF-star / Statement Identifiers)
--
-- Schema changes:
--   - Add qt_s, qt_p, qt_o columns to _pg_ripple.dictionary to support quoted triple encoding
--
-- New functions added by the Rust binary:
--   pg_ripple.encode_triple(s TEXT, p TEXT, o TEXT) RETURNS BIGINT
--   pg_ripple.decode_triple(id BIGINT) RETURNS JSONB
--   pg_ripple.get_statement(i BIGINT) RETURNS JSONB
--   pg_ripple.insert_triple() now returns the statement ID (BIGINT)
--   pg_ripple.load_ntriples() now accepts N-Triples-star input
--
-- Quoted triple support in dictionary:
--   KIND_QUOTED_TRIPLE = 5 (new kind discriminant)
--   Quoted triples stored as XXH3-128(s_id || p_id || o_id)
--

-- Add columns to support quoted triple encoding (kind=5)
ALTER TABLE _pg_ripple.dictionary
    ADD COLUMN qt_s BIGINT,
    ADD COLUMN qt_p BIGINT,
    ADD COLUMN qt_o BIGINT;

COMMENT ON COLUMN _pg_ripple.dictionary.qt_s IS
    'Subject ID of a quoted triple (kind=5); NULL for all other term types.';
COMMENT ON COLUMN _pg_ripple.dictionary.qt_p IS
    'Predicate ID of a quoted triple (kind=5); NULL for all other term types.';
COMMENT ON COLUMN _pg_ripple.dictionary.qt_o IS
    'Object ID of a quoted triple (kind=5); NULL for all other term types.';
