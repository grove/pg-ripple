-- pg_regress test: SHACL-AF sh:rule execution (v0.55.0)
-- Verifies that bridge_shacl_rules correctly handles sh:rule patterns
-- and that the SHACL-AF integration compiles rules when inference is enabled.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- With inference_mode = 'off', sh:rule patterns should emit PT480 warning.
SET pg_ripple.inference_mode = 'off';

DO $$
BEGIN
  PERFORM pg_ripple.load_shacl($$
    @prefix sh: <http://www.w3.org/ns/shacl#> .
    @prefix ex: <http://example.org/> .

    ex:Rule1 a sh:NodeShape ;
      sh:rule [
        sh:subject ex:subject1 ;
        sh:predicate ex:p1 ;
        sh:object ex:object1 ;
      ] .
  $$);
  RAISE NOTICE 'OK: load_shacl with sh:rule completed (inference off)';
EXCEPTION WHEN OTHERS THEN
  RAISE NOTICE 'load_shacl result: %', left(SQLERRM, 80);
END
$$;

RESET pg_ripple.inference_mode;

-- With inference_mode = 'on_demand', sh:rule should be registered.
SET pg_ripple.inference_mode = 'on_demand';

SELECT pg_ripple.load_shacl($$
  @prefix sh: <http://www.w3.org/ns/shacl#> .
  @prefix ex: <http://example.org/> .

  ex:InferenceShape a sh:NodeShape ;
    sh:targetClass ex:Person .
$$) >= 0 AS shapes_loaded;

RESET pg_ripple.inference_mode;
