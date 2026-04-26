-- pg_regress test: SPARQL-DL OWL T-Box routing (v0.58.0)

-- Verify SPARQL-DL API functions exist.
SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'sparql_dl_subclasses'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS sparql_dl_subclasses_exists;

SELECT EXISTS (
    SELECT 1 FROM pg_proc
    WHERE proname = 'sparql_dl_superclasses'
      AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_ripple')
) AS sparql_dl_superclasses_exists;

-- Insert an OWL subClassOf triple.
SELECT pg_ripple.insert_triple(
    '<http://example.org/Dog>',
    '<http://www.w3.org/2002/07/owl#subClassOf>',
    '<http://example.org/Animal>'
) > 0 AS inserted_subclass;

SELECT pg_ripple.insert_triple(
    '<http://example.org/Cat>',
    '<http://www.w3.org/2002/07/owl#subClassOf>',
    '<http://example.org/Animal>'
) > 0 AS inserted_subclass_cat;

SELECT pg_ripple.insert_triple(
    '<http://example.org/Poodle>',
    '<http://www.w3.org/2002/07/owl#subClassOf>',
    '<http://example.org/Dog>'
) > 0 AS inserted_subclass_poodle;

-- Query subclasses of Animal.
SELECT count(*) >= 2 AS got_animal_subclasses
FROM pg_ripple.sparql_dl_subclasses('<http://example.org/Animal>');

-- Query superclasses of Dog.
SELECT count(*) >= 1 AS got_dog_superclasses
FROM pg_ripple.sparql_dl_superclasses('<http://example.org/Dog>');

-- Verify the Animal subclasses include Dog.
SELECT bool_or(subclass_iri LIKE '%Dog%') AS includes_dog
FROM pg_ripple.sparql_dl_subclasses('<http://example.org/Animal>');

-- Verify Cat is a subclass of Animal.
SELECT bool_or(subclass_iri LIKE '%Cat%') AS includes_cat
FROM pg_ripple.sparql_dl_subclasses('<http://example.org/Animal>');

-- Query subclasses of Dog (should include Poodle).
SELECT count(*) >= 1 AS poodle_subclass_of_dog
FROM pg_ripple.sparql_dl_subclasses('<http://example.org/Dog>');

-- Non-existent class returns empty set.
SELECT count(*) = 0 AS empty_for_nonexistent
FROM pg_ripple.sparql_dl_subclasses('<http://example.org/NonExistent>');
