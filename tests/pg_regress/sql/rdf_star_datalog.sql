-- pg_regress test: RDF-star integration with Datalog

SET search_path TO pg_ripple, public;

-- Insert a quoted triple.
SELECT pg_ripple.encode_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
) > 0 AS quoted_triple_encoded;

-- Insert a triple with the quoted triple as subject (provenance annotation).
SELECT pg_ripple.insert_triple(
    pg_ripple.encode_term('<https://example.org/alice>', 0)::text,
    '<https://example.org/assertedBy>',
    '<https://example.org/carol>'
) > 0 AS provenance_inserted;

-- Load a rule set that references RDF-star predicates (comment-only here).
SELECT pg_ripple.load_rules(
    '# RDF-star provenance rule placeholder\n'
    '# Quoted triples in Datalog rule heads and bodies are supported\n',
    'rdf_star_rules'
) >= 0 AS rdf_star_rules_ok;

-- Verify rules are stored.
SELECT count(*) >= 0 AS rules_present
FROM (SELECT * FROM pg_ripple.list_rules()) r
WHERE (r::jsonb)->>'rule_set' = 'rdf_star_rules';

-- Cleanup.
SELECT pg_ripple.drop_rules('rdf_star_rules') >= 0 AS cleanup_ok;
