-- pg_regress test: basic triple CRUD

-- Insert a triple and verify the count
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
) > 0 AS insert_returns_sid;

SELECT pg_ripple.triple_count() >= 1 AS count_positive;

-- Find by subject
SELECT count(*) >= 1 AS found_by_subject
FROM pg_ripple.find_triples('<https://example.org/alice>', NULL, NULL);

-- Find by predicate
SELECT count(*) >= 1 AS found_by_predicate
FROM pg_ripple.find_triples(NULL, '<https://example.org/knows>', NULL);

-- Find by object
SELECT count(*) >= 1 AS found_by_object
FROM pg_ripple.find_triples(NULL, NULL, '<https://example.org/bob>');

-- Full pattern match
SELECT count(*) = 1 AS found_exact
FROM pg_ripple.find_triples(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
);

-- Delete the triple
SELECT pg_ripple.delete_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
) = 1 AS deleted_one;

-- Verify it's gone
SELECT count(*) = 0 AS gone_after_delete
FROM pg_ripple.find_triples(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
);
