-- pg_regress test: triple CRUD with vp_rare routing and named graphs

-- Insert into default graph (threshold 1000 not crossed → goes to vp_rare)
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
) > 0 AS insert_returns_sid;

-- Count should be at least 1 (may include triples from earlier tests)
SELECT pg_ripple.triple_count() >= 1 AS count_positive;

-- Find by subject (queries vp_rare)
SELECT count(*) >= 1 AS found_by_subject
FROM pg_ripple.find_triples('<https://example.org/alice>', NULL, NULL);

-- Find by predicate
SELECT count(*) >= 1 AS found_by_predicate
FROM pg_ripple.find_triples(NULL, '<https://example.org/knows>', NULL);

-- Find by object
SELECT count(*) >= 1 AS found_by_object
FROM pg_ripple.find_triples(NULL, NULL, '<https://example.org/bob>');

-- Full pattern match
SELECT count(*) >= 1 AS found_exact
FROM pg_ripple.find_triples(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
);

-- Insert a triple with a plain literal object
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<https://example.org/name>',
    '"Alice"'
) > 0 AS literal_insert_returns_sid;

-- Verify the literal can be found
SELECT count(*) = 1 AS found_literal
FROM pg_ripple.find_triples('<https://example.org/alice>', '<https://example.org/name>', NULL);

-- Delete the triple
SELECT pg_ripple.delete_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
) >= 1 AS deleted_at_least_one;

-- Verify it is gone
SELECT count(*) = 0 AS gone_after_delete
FROM pg_ripple.find_triples(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
);

-- Prefix management
SELECT pg_ripple.register_prefix('ex', 'https://example.org/');
SELECT count(*) >= 1 AS prefix_registered
FROM pg_ripple.prefixes()
WHERE prefix = 'ex';
