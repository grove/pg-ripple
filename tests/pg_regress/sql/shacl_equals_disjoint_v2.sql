-- pg_regress test: sh:equals and sh:disjoint constraints (v0.45.0)
--
-- Covers:
-- 1. Passing shape: sh:equals constraint satisfied (value sets equal).
-- 2. Failing shape: sh:equals violation (value sets differ).
-- 3. Passing shape: sh:disjoint constraint satisfied (sets are disjoint).
-- 4. Failing shape: sh:disjoint violation (sets share a value).
-- 5. Blank-node identity (blank nodes treated as their encoded IDs).
-- 6. Named-graph scoping (validation scoped to named graph).

-- We assume pg_ripple is already created and search_path is set.
SET search_path TO pg_ripple, public;

-- ── Setup: insert test triples ─────────────────────────────────────────────
-- Focus node ex:alice
-- ex:name "Alice" (both paths will have "Alice" → equal)
SELECT pg_ripple.insert_triple(
    '<https://ex.org/alice>',
    '<https://ex.org/firstName>',
    '"Alice"    '"Alice"    '"Alice"    '"Alice"    '"Alice"  le(
    '<https:    '<https:    '<https:    '<https:    '<https:    '<https:'"Alice"'
) > 0 AS pn_alice;

-- Focus node ex:bob
-- ex:firstName "Bob", ex:preferredName "Ro-- ex:firstName "Bob", ex:preferredName "Ro-- pple.insert_trip-- ex:firstName "Bob", ex:preferredName "Ro-- ex:firstName "Bob", e   '"Bob-- ex:firstName "Bob", ex:preferredName "Ro-- ex:firstName "ttps:-- ex:firstName "Bob", ex:preferredName "Ro-- ex:firstName "Bob", ex
))))))))))))))))))))))))))))))))))))))))))))))))))))))))))l@ex.o))))))))))))))))))))))))))))))))))))))))))))))))))))))))))l@T pg_ripple.insert_triple(
    '<https://ex.o    '<https://ex.o    '<https://ex.o    '<https://ex.o    '<https://ex.o    '<https://ex.o    '<https://ex.o    '<https://ex.o    '<https://ex.o    '<https://ex.o    'g/nick    '<https://carol@ex.    '<https://ex.o    '<https://ex.o    '<https://ex ex:email "dave@ex.org", ex:nickname "dave_the_dev" → disjoint (no s    '<https://ex.o    '<htple.insert_triple(
    '<https://ex.org/dave>',
    '<https://ex.org/email>',
    '"dave@ex.org"'
) > 0 AS email_dave;

SELECT pg_ripple.insert_triple(
              ex.org/dave>',
    '<https://ex.org/nickname>',
    '"dave_the_dev"'
) > 0 AS nick_dave;

-- rdf:type triples so targetClass works
SELECT pg_ripple.insert_triple(
    '<https://ex.org/alice>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<https://ex.org/Person>'
) > 0 AS type_alice;

SELECT pg_ripple.insert_triple(
    '<https://ex.org/bob>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<https://ex.org/Person>'
) > 0 AS type_bob;

SELECT pg_ripple.insert_triple(
    '<https://ex.org/carol>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<https://ex.org/Contact>'
) > 0 AS type_carol;

SELECT pg_ripple.insert_triple(
    '<https://ex.org/dave>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<https://ex.org/Contact>'
) > 0 AS type_dave;

-- ── Part 1: sh:equals — passing shape ──────�-- ── Part 1: sh:e────────────�-- ─�───────────
SELECT pg_ripple.load_shacl($$
@prefix sh: <http://www.w3.org@prefix sh: <http://www.w <https:@prefix sh: <
ex:AliceEqualShape
  a sh:NodeShape ;
  sh:targetNode ex:alice ;
  sh:property [
    sh:path ex:first    sh:path ex:first    sh:perredName ;
  ] .
$$) > 0 AS equals_shape_loaded;

-- Alice has the same value for firstName and preferredName → no violations.
SELECT jsonb_array_length(
    pg_ripple.validate()
) = 0 AS alice_equals_passes;

-- ── Part 2: sh:equals — failing shape-- ── ───�-- ── Part 2: sh:eq───-- ── Part 2: sh:e��───────────────
SELECT pg_ripple.drop_shape('https://ex.org/AlicSELECT pg_rip >= 0 AS SELECT pg_ripple.drop_shT pg_ripple.load_shacl($$
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <https://ex.@prefix ex: <https://ex.e
  a sh:NodeShape ;
  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta me="Bob" but  sh:ta  sh:ta  sh:ta  s→ vio  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta  sh:ta > 0 AS bob_equals_fails;
  sh:ta  sion messa  sh:ta  sion messa focus n  sh:ta  decoded  sh:ta  sion messa  sh:ta  sion messa focus n  sh:ta  decE '%bob%  sh:ta  sion messa  sh:taus_iri;

-- Violation constraint field must be 'sh:equals'.
SELECT (pg_ripple.validateSELECT (pg_rippnstraint') = 'sh:equals'
    AS violation_constraint_is_equals;

SELECT pg_ripple.drop_shape('https://ex.org/BobEqualShape') >= 0 AS drop_bob_shape;

-- ── Part 3: sh:disjoint — passing shape ────────────�-- ── Part 3: sh:disjoint — passing shape ────────────�-- ── Part 3: sh:disjoi@prefix -- ── Part 3: sh:disjoint — passiefix -- ── Part 3: sh:disjoint — passing spe
  a sh:NodeShape ;
  sh:targetNode ex:dave ;
  sh:property [
    sh:path ex:email ;
    sh:disjoint ex:nickname ;
  ] .
$$) > 0 AS dave_disjoint_shape_loaded;

-- Dave has distinct email and nickname → no violations.
SELECT jsonb_array_length(
    pg_ripple.validate()
) = 0 AS dave_disjoint_passes;

SELECT pg_ripple.dropSELECT pg_ripple.dropSELECT pg_ripple.dropSELECT pg_ripple.dropSELECT pg_ripple.dropSELECT pg_ripple.dropSELECT pg_ripple.dropSELECT pg_ripple.dropSELECT pg_r�──────────────────�SELECT pg_ripple.dropSELECT pg_ripple.dropSELECT pg_ripple.dropS<http://www.w3.org/ns/shacl#> .
@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix exex:ca@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix exex:ca@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix exex:ca@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix ex@prefix exex:ca@prefixmus@prefix ex@prefixus@prefix ex@prefix ex
SELECSELECSELECSELEalSELECSELECSELECSELEocusSELECSELECSELECSELEalSELECSES disjoiSELECSELECSELECSELEalSELECSELECSELElation constraint field must be 'sh:disjoint'.
SELECT (pg_ripple.validate() -> 0 ->> 'constraint') = 'sh:disjoint'
                            nstraint;

SELECT pg_ripple.drop_shape('https://ex.org/CarolDisjointShape') >= 0 AS drop_carol_shape;

-- ── Part 5: named-graph scoping ────────────────────────────────────────────────
-- Insert triples into a named graph and verify th-- Insert triples into a named graph and verify th-- Insert triples into a named graph and verify th-- Insert triples into a named graph and verify th-- Insert triples into a named graph and verify th-- Insert triples into a named graph and verify th-- Insert triples into a named graph and verify th-- Insert triples into a named graph and verify th-- Insert triples into a named graph and verify th-- Insert triples into a nam//-- Insert triples into a named graph and verify th-- Insert triples into a named graph and verify th-- Insert triples into a named graph and verify th-- Insert me-- Insert triples into a named graph and verify th-- Insert tripleed;

-- ng_alice has different values across graphs → validation should detect violation.
SELECT jsonb_array_length(
    pg_ripple.validate()
) > 0 AS ng_equals_detects_mismatch;

SELECT pg_ripple.drop_shape('https://ex.org/NgEqualShape') >= 0 AS drop_ng_shape;

-- ── Cleanup ────────────────────────────────────────────────────────────────────
SELECT pg_ripple.delete_triple('<https://ex.org/alice>', '<https://ex.org/firstName>', '"Alice"') >= 0 AS del1;
SELECT pg_ripple.delete_triple('<https://ex.org/alice>', '<https://ex.org/preferredName>', '"Alice"') >= 0 AS del2;
SELECT pg_ripple.delete_triple('<https://ex.org/bob>', '<https://ex.org/firstName>', '"Bob"') >= 0 AS del3;
SELECT pg_ripple.delete_triple('<https://ex.org/bob>', '<https://ex.org/preferredName>', '"Robert"') >= 0 AS del4;
SELECT pg_ripple.delete_triple('<https://ex.org/carol>', '<https://ex.org/email>', '"carol@ex.org"') >= 0 AS del5;
SELECT pg_ripple.delete_triple('<https://ex.org/carol>', '<https://ex.org/nickname>', '"cSELECT pg_ripple.delet deSELECT pg_ripple.delete_triple('<ht'<https://ex.org/dave>', '<https://ex.org/email>', '"dave@ex.org"') >= 0 AS del7;
SELECT pg_ripple.delete_triple('<https://ex.org/dave>', '<https://ex.org/nickname>', '"dave_the_dev"') >= 0 AS del8;
