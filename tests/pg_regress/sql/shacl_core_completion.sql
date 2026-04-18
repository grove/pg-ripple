-- shacl_core_completion.sql — SHACL Core completion tests (v0.23.0)
--
-- Covers new constraints added in v0.23.0:
--   1. sh:hasValue
--   2. sh:nodeKind  (sh:IRI, sh:Literal, sh:BlankNode)
--   3. sh:languageIn
--   4. sh:uniqueLang
--   5. sh:lessThan / sh:greaterThan
--   6. sh:closed / sh:ignoredProperties
--   7. Block comments (/* ... */) in Turtle SHACL documents
--
-- Uses unique IRIs (<http://shacl.core23.test/…>) to avoid interference.
-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.

SET search_path TO pg_ripple, public;

-- ─────────────────────────────────────────────────────────────────────────────
-- 1.  sh:hasValue — focus node must have a specific fixed value
-- ─────────────────────────────────────────────────────────────────────────────

SELECT pg_ripple.load_shacl($SHACL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <http://shacl.core23.test/> .

ex:HasValueShape
    a sh:NodeShape ;
    sh:targetClass ex:Country ;
    sh:property [
        sh:path ex:continent ;
        sh:hasValue ex:Europe ;
    ] .
$SHACL$);

-- Conforming: continent = ex:Europe.
SELECT pg_ripple.load_ntriples('
<http://shacl.core23.test/Germany>   <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <http://shacl.core23.test/Country> .
<http://shacl.core23.test/Germany>   <http://shacl.core23.test/continent>               <http://shacl.core23.test/Europe> .
');

-- Non-conforming: continent = ex:Asia, not ex:Europe.
SELECT pg_ripple.load_ntriples('
<http://shacl.core23.test/Japan>     <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <http://shacl.core23.test/Country> .
<http://shacl.core23.test/Japan>     <http://shacl.core23.test/continent>               <http://shacl.core23.test/Asia> .
');

WITH r AS (SELECT pg_ripple.validate() AS rpt)
SELECT
    NOT jsonb_path_exists(rpt, '$.violations[*] ? (@.focusNode == $n)',
        jsonb_build_object('n', 'http://shacl.core23.test/Germany'))  AS germany_conforms,
    jsonb_path_exists(rpt, '$.violations[*] ? (@.focusNode == $n && @.shapeIRI == $s)',
        jsonb_build_object('n', 'http://shacl.core23.test/Japan',
                           's', 'http://shacl.core23.test/HasValueShape')) AS japan_violates
FROM r;

-- ─────────────────────────────────────────────────────────────────────────────
-- 2.  sh:nodeKind — value nodes must be of the specified kind
-- ─────────────────────────────────────────────────────────────────────────────

SELECT pg_ripple.load_shacl($SHACL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <http://shacl.core23.test/> .

ex:NodeKindShape
    a sh:NodeShape ;
    sh:targetClass ex:Book ;
    sh:property [
        sh:path ex:author ;
        sh:nodeKind sh:IRI ;
    ] .
$SHACL$);

-- Conforming: author is an IRI.
SELECT pg_ripple.load_ntriples('
<http://shacl.core23.test/Book1>     <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <http://shacl.core23.test/Book> .
<http://shacl.core23.test/Book1>     <http://shacl.core23.test/author>                  <http://shacl.core23.test/Author1> .
');

-- Non-conforming: author is a literal.
SELECT pg_ripple.load_ntriples('
<http://shacl.core23.test/Book2>     <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <http://shacl.core23.test/Book> .
<http://shacl.core23.test/Book2>     <http://shacl.core23.test/author>                  "Anonymous" .
');

WITH r AS (SELECT pg_ripple.validate() AS rpt)
SELECT
    NOT jsonb_path_exists(rpt, '$.violations[*] ? (@.focusNode == $n && @.shapeIRI == $s)',
        jsonb_build_object('n', 'http://shacl.core23.test/Book1',
                           's', 'http://shacl.core23.test/NodeKindShape'))  AS book1_conforms,
    jsonb_path_exists(rpt, '$.violations[*] ? (@.focusNode == $n && @.shapeIRI == $s)',
        jsonb_build_object('n', 'http://shacl.core23.test/Book2',
                           's', 'http://shacl.core23.test/NodeKindShape'))  AS book2_violates
FROM r;

-- ─────────────────────────────────────────────────────────────────────────────
-- 3.  sh:languageIn — literal must have a language from the given list
-- ─────────────────────────────────────────────────────────────────────────────

SELECT pg_ripple.load_shacl($SHACL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <http://shacl.core23.test/> .

ex:LangShape
    a sh:NodeShape ;
    sh:targetClass ex:Article ;
    sh:property [
        sh:path ex:title ;
        sh:languageIn ( "en" "de" ) ;
    ] .
$SHACL$);

-- Conforming: title is in English.
SELECT pg_ripple.load_ntriples('
<http://shacl.core23.test/Article1>  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <http://shacl.core23.test/Article> .
<http://shacl.core23.test/Article1>  <http://shacl.core23.test/title>                   "A title"@en .
');

-- Non-conforming: title is in French.
SELECT pg_ripple.load_ntriples('
<http://shacl.core23.test/Article2>  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <http://shacl.core23.test/Article> .
<http://shacl.core23.test/Article2>  <http://shacl.core23.test/title>                   "Un titre"@fr .
');

WITH r AS (SELECT pg_ripple.validate() AS rpt)
SELECT
    NOT jsonb_path_exists(rpt, '$.violations[*] ? (@.focusNode == $n && @.shapeIRI == $s)',
        jsonb_build_object('n', 'http://shacl.core23.test/Article1',
                           's', 'http://shacl.core23.test/LangShape'))  AS article1_conforms,
    jsonb_path_exists(rpt, '$.violations[*] ? (@.focusNode == $n && @.shapeIRI == $s)',
        jsonb_build_object('n', 'http://shacl.core23.test/Article2',
                           's', 'http://shacl.core23.test/LangShape'))  AS article2_violates
FROM r;

-- ─────────────────────────────────────────────────────────────────────────────
-- 4.  sh:uniqueLang — no two values for the property may share a language tag
-- ─────────────────────────────────────────────────────────────────────────────

SELECT pg_ripple.load_shacl($SHACL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <http://shacl.core23.test/> .

ex:UniqueLangShape
    a sh:NodeShape ;
    sh:targetClass ex:Concept ;
    sh:property [
        sh:path ex:label ;
        sh:uniqueLang true ;
    ] .
$SHACL$);

-- Conforming: one English label, one German label.
SELECT pg_ripple.load_ntriples('
<http://shacl.core23.test/Concept1>  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <http://shacl.core23.test/Concept> .
<http://shacl.core23.test/Concept1>  <http://shacl.core23.test/label>                   "Hello"@en .
<http://shacl.core23.test/Concept1>  <http://shacl.core23.test/label>                   "Hallo"@de .
');

-- Non-conforming: two English labels.
SELECT pg_ripple.load_ntriples('
<http://shacl.core23.test/Concept2>  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <http://shacl.core23.test/Concept> .
<http://shacl.core23.test/Concept2>  <http://shacl.core23.test/label>                   "Hello"@en .
<http://shacl.core23.test/Concept2>  <http://shacl.core23.test/label>                   "Hi"@en .
');

WITH r AS (SELECT pg_ripple.validate() AS rpt)
SELECT
    NOT jsonb_path_exists(rpt, '$.violations[*] ? (@.focusNode == $n && @.shapeIRI == $s)',
        jsonb_build_object('n', 'http://shacl.core23.test/Concept1',
                           's', 'http://shacl.core23.test/UniqueLangShape'))  AS concept1_conforms,
    jsonb_path_exists(rpt, '$.violations[*] ? (@.focusNode == $n && @.shapeIRI == $s)',
        jsonb_build_object('n', 'http://shacl.core23.test/Concept2',
                           's', 'http://shacl.core23.test/UniqueLangShape'))  AS concept2_violates
FROM r;

-- ─────────────────────────────────────────────────────────────────────────────
-- 5.  sh:lessThan / sh:greaterThan — cross-property ordering constraints
-- ─────────────────────────────────────────────────────────────────────────────

SELECT pg_ripple.load_shacl($SHACL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <http://shacl.core23.test/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:DateRangeShape
    a sh:NodeShape ;
    sh:targetClass ex:Event ;
    sh:property [
        sh:path ex:startDate ;
        sh:lessThan ex:endDate ;
    ] .
$SHACL$);

-- Conforming: start < end.
SELECT pg_ripple.load_ntriples('
<http://shacl.core23.test/Event1>   <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <http://shacl.core23.test/Event> .
<http://shacl.core23.test/Event1>   <http://shacl.core23.test/startDate>               "2024-01-01"^^<http://www.w3.org/2001/XMLSchema#date> .
<http://shacl.core23.test/Event1>   <http://shacl.core23.test/endDate>                 "2024-12-31"^^<http://www.w3.org/2001/XMLSchema#date> .
');

-- Non-conforming: start > end.
SELECT pg_ripple.load_ntriples('
<http://shacl.core23.test/Event2>   <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <http://shacl.core23.test/Event> .
<http://shacl.core23.test/Event2>   <http://shacl.core23.test/startDate>               "2024-12-31"^^<http://www.w3.org/2001/XMLSchema#date> .
<http://shacl.core23.test/Event2>   <http://shacl.core23.test/endDate>                 "2024-01-01"^^<http://www.w3.org/2001/XMLSchema#date> .
');

WITH r AS (SELECT pg_ripple.validate() AS rpt)
SELECT
    NOT jsonb_path_exists(rpt, '$.violations[*] ? (@.focusNode == $n && @.shapeIRI == $s)',
        jsonb_build_object('n', 'http://shacl.core23.test/Event1',
                           's', 'http://shacl.core23.test/DateRangeShape'))  AS event1_conforms,
    jsonb_path_exists(rpt, '$.violations[*] ? (@.focusNode == $n && @.shapeIRI == $s)',
        jsonb_build_object('n', 'http://shacl.core23.test/Event2',
                           's', 'http://shacl.core23.test/DateRangeShape'))  AS event2_violates
FROM r;

-- ─────────────────────────────────────────────────────────────────────────────
-- 6.  sh:closed / sh:ignoredProperties
-- ─────────────────────────────────────────────────────────────────────────────

SELECT pg_ripple.load_shacl($SHACL$
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <http://shacl.core23.test/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

ex:ClosedShape
    a sh:NodeShape ;
    sh:targetClass ex:Strict ;
    sh:closed true ;
    sh:ignoredProperties ( rdf:type ) ;
    sh:property [
        sh:path ex:name ;
        sh:minCount 1 ;
    ] .
$SHACL$);

-- Conforming: only rdf:type and ex:name.
SELECT pg_ripple.load_ntriples('
<http://shacl.core23.test/S1>        <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <http://shacl.core23.test/Strict> .
<http://shacl.core23.test/S1>        <http://shacl.core23.test/name>                    "OK" .
');

-- Non-conforming: extra property ex:extra not in shape.
SELECT pg_ripple.load_ntriples('
<http://shacl.core23.test/S2>        <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <http://shacl.core23.test/Strict> .
<http://shacl.core23.test/S2>        <http://shacl.core23.test/name>                    "Also OK" .
<http://shacl.core23.test/S2>        <http://shacl.core23.test/extra>                   "Not OK" .
');

WITH r AS (SELECT pg_ripple.validate() AS rpt)
SELECT
    NOT jsonb_path_exists(rpt, '$.violations[*] ? (@.focusNode == $n && @.shapeIRI == $s)',
        jsonb_build_object('n', 'http://shacl.core23.test/S1',
                           's', 'http://shacl.core23.test/ClosedShape'))  AS s1_conforms,
    jsonb_path_exists(rpt, '$.violations[*] ? (@.focusNode == $n && @.shapeIRI == $s)',
        jsonb_build_object('n', 'http://shacl.core23.test/S2',
                           's', 'http://shacl.core23.test/ClosedShape'))  AS s2_violates
FROM r;

-- ─────────────────────────────────────────────────────────────────────────────
-- 7.  Block comments in Turtle SHACL documents should be stripped
-- ─────────────────────────────────────────────────────────────────────────────

-- This should parse successfully (block comment inside Turtle).
SELECT pg_ripple.load_shacl($SHACL$
/* This is a block comment — should be stripped before parsing */
@prefix sh:  <http://www.w3.org/ns/shacl#> .
@prefix ex:  <http://shacl.core23.test/> .
/* Another block comment
   spanning multiple lines */
ex:CommentTestShape
    a sh:NodeShape ;
    sh:targetClass ex:CommentTest ;
    sh:property [
        sh:path ex:val ;
        sh:minCount 1 ; /* inline comment */
    ] .
$SHACL$) IS NOT NULL AS block_comment_parse_ok;
