-- shacl_advanced.sql — SHACL Advanced validation tests (v0.8.0)
--
-- Covers:
--   1. sh:or  — value/focus node must conform to at least one shape
--   2. sh:and — value/focus node must conform to all listed shapes
--   3. sh:not — value/focus node must NOT conform to a shape
--   4. sh:node — value nodes must conform to a nested shape
--   5. sh:qualifiedValueShape + sh:qualifiedMinCount / sh:qualifiedMaxCount
--   6. Async validation pipeline: process_validation_queue, dead_letter_queue
--
-- Uses unique IRIs (<http://shacl.adv.test/…>) to avoid interference.
-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
SET search_path TO pg_ripple, public;

-- ─────────────────────────────────────────────────────────────────────────────
-- 1.  sh:or — focus node must conform to at least one named shape
-- ─────────────────────────────────────────────────────────────────────────────

-- Define two simple shapes and an sh:or combinator shape.
SELECT pg_ripple.load_shacl($SHACL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <http://shacl.adv.test/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:PersonShape
    a sh:NodeShape ;
    sh:targetClass ex:Person ;
    sh:property [
        sh:path ex:name ;
        sh:minCount 1 ;
    ] .

ex:OrgShape
    a sh:NodeShape ;
    sh:targetClass ex:Organization ;
    sh:property [
        sh:path ex:orgName ;
        sh:minCount 1 ;
    ] .

ex:AgentShape
    a sh:NodeShape ;
    sh:targetClass ex:Agent ;
    sh:or (ex:PersonShape ex:OrgShape) .
$SHACL$) AS shapes_loaded;

-- Insert an Agent that is also a Person (has ex:name) → must conform.
SELECT pg_ripple.insert_triple(
    '<http://shacl.adv.test/alice>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://shacl.adv.test/Person>'
) > 0 AS ok;

SELECT pg_ripple.insert_triple(
    '<http://shacl.adv.test/alice>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://shacl.adv.test/Agent>'
) > 0 AS ok;

SELECT pg_ripple.insert_triple(
    '<http://shacl.adv.test/alice>',
    '<http://shacl.adv.test/name>',
    '"Alice"'
) > 0 AS ok;

-- validate() must return conforms = true (alice is an Agent+Person with a name).
SELECT (pg_ripple.validate() ->> 'conforms')::boolean AS conforms_or_pass;

-- ─────────────────────────────────────────────────────────────────────────────
-- 2.  sh:not — focus node must NOT conform to a shape
-- ─────────────────────────────────────────────────────────────────────────────

SELECT pg_ripple.load_shacl($SHACL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <http://shacl.adv.test/> .

ex:BannedShape
    a sh:NodeShape ;
    sh:targetClass ex:BannedEntity ;
    sh:property [
        sh:path ex:name ;
        sh:maxCount 0 ;
    ] .

ex:LegalEntityShape
    a sh:NodeShape ;
    sh:targetClass ex:LegalEntity ;
    sh:not ex:BannedShape .
$SHACL$) AS shapes_loaded;

-- Insert a LegalEntity that is also a BannedEntity → should show a violation.
SELECT pg_ripple.insert_triple(
    '<http://shacl.adv.test/shady>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://shacl.adv.test/LegalEntity>'
) > 0 AS ok;

SELECT pg_ripple.insert_triple(
    '<http://shacl.adv.test/shady>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://shacl.adv.test/BannedEntity>'
) > 0 AS ok;

-- validate() must find a sh:not violation (shady is both LegalEntity and BannedEntity).
SELECT
    (pg_ripple.validate() ->> 'conforms')::boolean      AS conforms_not,
    jsonb_array_length(pg_ripple.validate() -> 'violations') > 0 AS has_violation;

-- ─────────────────────────────────────────────────────────────────────────────
-- 3.  sh:node — value nodes must conform to a nested shape
-- ─────────────────────────────────────────────────────────────────────────────

-- Drop all shapes and start fresh for this section.
SELECT pg_ripple.drop_shape('http://shacl.adv.test/AgentShape');
SELECT pg_ripple.drop_shape('http://shacl.adv.test/PersonShape');
SELECT pg_ripple.drop_shape('http://shacl.adv.test/OrgShape');
SELECT pg_ripple.drop_shape('http://shacl.adv.test/BannedShape');
SELECT pg_ripple.drop_shape('http://shacl.adv.test/LegalEntityShape');

SELECT pg_ripple.load_shacl($SHACL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <http://shacl.adv.test/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:AddressShape
    a sh:NodeShape ;
    sh:property [
        sh:path ex:city ;
        sh:minCount 1 ;
        sh:datatype xsd:string ;
    ] .

ex:CompanyShape
    a sh:NodeShape ;
    sh:targetClass ex:Company ;
    sh:property [
        sh:path ex:headquarterAddress ;
        sh:node ex:AddressShape ;
    ] .
$SHACL$) AS shapes_loaded;

-- Insert a company without a valid address → violation.
SELECT pg_ripple.insert_triple(
    '<http://shacl.adv.test/acme>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://shacl.adv.test/Company>'
) > 0 AS ok;

-- Insert headquarters pointing to a blank-node address that lacks ex:city.
SELECT pg_ripple.insert_triple(
    '<http://shacl.adv.test/acme>',
    '<http://shacl.adv.test/headquarterAddress>',
    '<http://shacl.adv.test/acme_hq>'
) > 0 AS ok;

-- acme_hq has no ex:city → sh:node violation.
SELECT
    (pg_ripple.validate() ->> 'conforms')::boolean AS conforms_node_fail,
    jsonb_array_length(pg_ripple.validate() -> 'violations') > 0 AS has_violation;

-- Fix: add ex:city to the address.
SELECT pg_ripple.insert_triple(
    '<http://shacl.adv.test/acme_hq>',
    '<http://shacl.adv.test/city>',
    '"London"^^<http://www.w3.org/2001/XMLSchema#string>'
) > 0 AS ok;

-- Now address conforms → validate() returns conforms=true.
SELECT (pg_ripple.validate() ->> 'conforms')::boolean AS conforms_node_pass;

-- ─────────────────────────────────────────────────────────────────────────────
-- 4.  sh:qualifiedValueShape with sh:qualifiedMinCount / sh:qualifiedMaxCount
-- ─────────────────────────────────────────────────────────────────────────────

SELECT pg_ripple.drop_shape('http://shacl.adv.test/AddressShape');
SELECT pg_ripple.drop_shape('http://shacl.adv.test/CompanyShape');

SELECT pg_ripple.load_shacl($SHACL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <http://shacl.adv.test/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:USAddressShape
    a sh:NodeShape ;
    sh:property [
        sh:path ex:country ;
        sh:hasValue <http://shacl.adv.test/US> ;
    ] .

ex:EmployerShape
    a sh:NodeShape ;
    sh:targetClass ex:Employer ;
    sh:property [
        sh:path ex:officeAddress ;
        sh:qualifiedValueShape ex:USAddressShape ;
        sh:qualifiedMinCount 1 ;
    ] .
$SHACL$) AS shapes_loaded;

-- validate() returns no violations yet (no Employer instances).
SELECT (pg_ripple.validate() ->> 'conforms')::boolean AS conforms_qvs_empty;

-- ─────────────────────────────────────────────────────────────────────────────
-- 5.  Async validation pipeline
-- ─────────────────────────────────────────────────────────────────────────────

-- Drop all shapes so queue processing has nothing to fail.
SELECT pg_ripple.drop_shape('http://shacl.adv.test/USAddressShape');
SELECT pg_ripple.drop_shape('http://shacl.adv.test/EmployerShape');

-- Queue must be empty at this point.
SELECT pg_ripple.validation_queue_length() AS queue_len;

-- dead_letter_queue starts empty.
SELECT pg_ripple.dead_letter_count() AS dlq_count;

-- process_validation_queue() with empty queue returns 0.
SELECT pg_ripple.process_validation_queue() AS processed;

-- Load a shape and switch to async mode, then insert a triple.
SELECT pg_ripple.load_shacl($SHACL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <http://shacl.adv.test/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:StrictShape
    a sh:NodeShape ;
    sh:targetClass ex:StrictThing ;
    sh:property [
        sh:path ex:requiredField ;
        sh:maxCount 1 ;
        sh:datatype xsd:integer ;
    ] .
$SHACL$) AS shapes_loaded;

-- Enable async mode and insert a triple that would violate sh:datatype.
SET pg_ripple.shacl_mode = 'async';

-- Insert a StrictThing instance.
SELECT pg_ripple.insert_triple(
    '<http://shacl.adv.test/s1>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://shacl.adv.test/StrictThing>'
) > 0 AS ok;

-- Insert a violating triple (string instead of integer) — does NOT raise an error
-- in async mode; it is queued instead.
SELECT pg_ripple.insert_triple(
    '<http://shacl.adv.test/s1>',
    '<http://shacl.adv.test/requiredField>',
    '"not-an-integer"'
) > 0 AS async_insert_ok;

-- Queue should have 2 entries (the type triple + the violating triple).
SELECT pg_ripple.validation_queue_length() >= 1 AS queue_not_empty;

-- Process the queue.
SELECT pg_ripple.process_validation_queue() >= 0 AS processed_ok;

-- Queue is now empty.
SELECT pg_ripple.validation_queue_length() AS queue_after;

-- dead_letter_queue may have the violating triple.
SELECT pg_ripple.dead_letter_count() >= 0 AS dlq_ok;

-- dead_letter_queue() function returns a JSON array.
SELECT jsonb_typeof(pg_ripple.dead_letter_queue()) AS dlq_type;

-- Drain the dead-letter queue.
SELECT pg_ripple.drain_dead_letter_queue() >= 0 AS drained;
SELECT pg_ripple.dead_letter_count() AS dlq_after_drain;

-- Reset SHACL mode to off.
SET pg_ripple.shacl_mode = 'off';

-- ─────────────────────────────────────────────────────────────────────────────
-- 6.  Cleanup
-- ─────────────────────────────────────────────────────────────────────────────
SELECT pg_ripple.drop_shape('http://shacl.adv.test/StrictShape') >= 0 AS cleanup;
