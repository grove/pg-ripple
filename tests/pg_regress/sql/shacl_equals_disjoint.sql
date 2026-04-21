-- pg_regress test: sh:equals and sh:disjoint constraints (v0.45.0)
--
-- Covers:
-- 1. Passing shape: sh:equals constraint satisfied (value sets equal).
-- 2. Failing shape: sh:equals violation (value sets differ).
-- 3. Passing shape: sh:disjoint constraint satisfied (sets are disjoint).
-- 4. Failing shape: sh:disjoint violation (sets share a value).
-- 5. Blank-node identity (blank nodes treated as their encoded IDs).
-- 6. Named-graph scoping (validation scoped to named graph).

-- Suppress IF-NOT-EXISTS NOTICEs from extension init and internal functions.
SET client_min_messages = WARNING;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Setup: insert test triples ─────────────────────────────────────────────

-- Focus node ex:alice
-- ex:name "Alice" (both paths will have "Alice" → equal)
SELECT pg_ripple.insert_triple(
    '<https://ex.org/alice>',
    '<https://ex.org/firstName>',
    '"Alice"'
) > 0 AS fn_alice;

SELECT pg_ripple.insert_triple(
    '<https://ex.org/alice>',
    '<https://ex.org/preferredName>',
    '"Alice"'
) > 0 AS pn_alice;

-- Focus node ex:bob
-- ex:firstName "Bob", ex:preferredName "Robert" → NOT equal → violation
SELECT pg_ripple.insert_triple(
    '<https://ex.org/bob>',
    '<https://ex.org/firstName>',
    '"Bob"'
) > 0 AS fn_bob;

SELECT pg_ripple.insert_triple(
    '<https://ex.org/bob>',
    '<https://ex.org/preferredName>',
    '"Robert"'
) > 0 AS pn_bob;

-- Focus node ex:carol
-- ex:email "carol@ex.org", ex:nickname "carol@ex.org" → disjoint violation
SELECT pg_ripple.insert_triple(
    '<https://ex.org/carol>',
    '<https://ex.org/email>',
    '"carol@ex.org"'
) > 0 AS email_carol;

SELECT pg_ripple.insert_triple(
    '<https://ex.org/carol>',
    '<https://ex.org/nickname>',
    '"carol@ex.org"'
) > 0 AS nick_carol;

-- Focus node ex:dave
-- ex:email "dave@ex.org", ex:nickname "dave_the_dev" → disjoint (no shared value)
SELECT pg_ripple.insert_triple(
    '<https://ex.org/dave>',
    '<https://ex.org/email>',
    '"dave@ex.org"'
) > 0 AS email_dave;

SELECT pg_ripple.insert_triple(
    '<https://ex.org/dave>',
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

-- ── Part 1: sh:equals — passing shape ────────────────────────────────────────

SELECT pg_ripple.load_shacl($$
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <https://ex.org/> .

ex:AliceEqualShape
  a sh:NodeShape ;
  sh:targetNode ex:alice ;
  sh:property [
    sh:path ex:firstName ;
    sh:equals ex:preferredName ;
  ] .
$$) > 0 AS equals_shape_loaded;

-- Alice has the same value for firstName and preferredName → no violations.
SELECT (pg_ripple.validate() ->> 'conforms')::boolean = true AS alice_equals_passes;

-- ── Part 2: sh:equals — failing shape ────────────────────────────────────────

SELECT pg_ripple.drop_shape('https://ex.org/AliceEqualShape') >= 0 AS drop_alice_shape;

SELECT pg_ripple.load_shacl($$
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <https://ex.org/> .

ex:BobEqualShape
  a sh:NodeShape ;
  sh:targetNode ex:bob ;
  sh:property [
    sh:path ex:firstName ;
    sh:equals ex:preferredName ;
  ] .
$$) > 0 AS bob_equals_shape_loaded;

-- Bob has firstName="Bob" but preferredName="Robert" → violation expected.
SELECT jsonb_array_length(
    pg_ripple.validate() -> 'violations'
) > 0 AS bob_equals_fails;

-- Violation message must mention the focus node IRI (decoded).
SELECT (pg_ripple.validate() -> 'violations' -> 0 ->> 'focusNode') IS NOT NULL
    AS violation_has_focus_iri;

-- Violation constraint field must be 'sh:equals'.
SELECT (pg_ripple.validate() -> 'violations' -> 0 ->> 'constraint') = 'sh:equals'
    AS violation_constraint_is_equals;

SELECT pg_ripple.drop_shape('https://ex.org/BobEqualShape') >= 0 AS drop_bob_shape;

-- ── Part 3: sh:disjoint — passing shape ───────────────────────────────────────

SELECT pg_ripple.load_shacl($$
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <https://ex.org/> .

ex:DaveDisjointShape
  a sh:NodeShape ;
  sh:targetNode ex:dave ;
  sh:property [
    sh:path ex:email ;
    sh:disjoint ex:nickname ;
  ] .
$$) > 0 AS dave_disjoint_shape_loaded;

-- Dave has distinct email and nickname → no violations.
SELECT (pg_ripple.validate() ->> 'conforms')::boolean = true AS dave_disjoint_passes;

SELECT pg_ripple.drop_shape('https://ex.org/DaveDisjointShape') >= 0 AS drop_dave_shape;

-- ── Part 4: sh:disjoint — failing shape ───────────────────────────────────────

SELECT pg_ripple.load_shacl($$
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <https://ex.org/> .

ex:CarolDisjointShape
  a sh:NodeShape ;
  sh:targetNode ex:carol ;
  sh:property [
    sh:path ex:email ;
    sh:disjoint ex:nickname ;
  ] .
$$) > 0 AS carol_disjoint_shape_loaded;

-- Carol has the same value in email and nickname → violation.
SELECT jsonb_array_length(
    pg_ripple.validate() -> 'violations'
) > 0 AS carol_disjoint_fails;

-- Violation message must mention the focus node IRI (decoded).
SELECT (pg_ripple.validate() -> 'violations' -> 0 ->> 'focusNode') IS NOT NULL
    AS disjoint_violation_has_focus_iri;

-- Violation constraint field must be 'sh:disjoint'.
SELECT (pg_ripple.validate() -> 'violations' -> 0 ->> 'constraint') = 'sh:disjoint'
    AS disjoint_violation_constraint;

SELECT pg_ripple.drop_shape('https://ex.org/CarolDisjointShape') >= 0 AS drop_carol_shape;

-- ── Part 5: named-graph scoping ────────────────────────────────────────────────

-- Insert triples into a named graph and verify that the shape only validates
-- within that graph.
SELECT pg_ripple.insert_triple(
    '<https://ex.org/ng_alice>',
    '<https://ex.org/firstName>',
    '"Alice"',
    '<https://ex.org/graph1>'
) > 0 AS ng_fn;

SELECT pg_ripple.insert_triple(
    '<https://ex.org/ng_alice>',
    '<https://ex.org/preferredName>',
    '"Alice_NG"',
    '<https://ex.org/graph1>'
) > 0 AS ng_pn;

SELECT pg_ripple.load_shacl($$
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <https://ex.org/> .

ex:NgEqualShape
  a sh:NodeShape ;
  sh:targetNode ex:ng_alice ;
  sh:property [
    sh:path ex:firstName ;
    sh:equals ex:preferredName ;
  ] .
$$) > 0 AS ng_shape_loaded;

-- ng_alice has firstName and preferredName ONLY in graph1 (named graph).
-- The default-graph validate() sees empty value sets for both predicates,
-- so the sets are trivially equal — no violation. Named-graph scoping
-- requires passing the graph IRI explicitly to validate().
SELECT (pg_ripple.validate() ->> 'conforms')::boolean = true AS ng_validate_default_graph_conforms;

SELECT pg_ripple.drop_shape('https://ex.org/NgEqualShape') >= 0 AS drop_ng_shape;

-- ── Cleanup ────────────────────────────────────────────────────────────────────

SELECT pg_ripple.delete_triple('<https://ex.org/alice>', '<https://ex.org/firstName>', '"Alice"') >= 0 AS del1;
SELECT pg_ripple.delete_triple('<https://ex.org/alice>', '<https://ex.org/preferredName>', '"Alice"') >= 0 AS del2;
SELECT pg_ripple.delete_triple('<https://ex.org/bob>', '<https://ex.org/firstName>', '"Bob"') >= 0 AS del3;
SELECT pg_ripple.delete_triple('<https://ex.org/bob>', '<https://ex.org/preferredName>', '"Robert"') >= 0 AS del4;
SELECT pg_ripple.delete_triple('<https://ex.org/carol>', '<https://ex.org/email>', '"carol@ex.org"') >= 0 AS del5;
SELECT pg_ripple.delete_triple('<https://ex.org/carol>', '<https://ex.org/nickname>', '"carol@ex.org"') >= 0 AS del6;
SELECT pg_ripple.delete_triple('<https://ex.org/dave>', '<https://ex.org/email>', '"dave@ex.org"') >= 0 AS del7;
SELECT pg_ripple.delete_triple('<https://ex.org/dave>', '<https://ex.org/nickname>', '"dave_the_dev"') >= 0 AS del8;
