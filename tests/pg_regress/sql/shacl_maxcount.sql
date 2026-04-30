-- pg_regress test: SHACL validate() API sanity checks

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- 1. validate() returns a JSONB with a 'conforms' field.
SELECT (pg_ripple.validate() IS NOT NULL) AS validate_returns_jsonb;
SELECT (pg_ripple.validate() ? 'conforms') AS validate_has_conforms;

-- 2. validate() returns a JSONB with a 'violations' field.
SELECT (pg_ripple.validate() ? 'violations') AS validate_has_violations;

-- 3. violations field is an array.
SELECT jsonb_typeof(pg_ripple.validate() -> 'violations') = 'array' AS violations_is_array;

-- 4. load_shacl() function exists and accepts Turtle.
SELECT pg_ripple.load_shacl($SHACL$
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <https://shmc76.test/> .
ex:TestShape a sh:NodeShape ;
    sh:targetClass ex:TestClass ;
    sh:property [ sh:path ex:label ; sh:minCount 1 ] .
$SHACL$) IS NOT NULL AS load_shacl_works;
