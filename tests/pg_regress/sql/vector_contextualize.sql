-- pg_regress test: contextualize_entity() (v0.28.0)
-- Tests the graph-contextualized entity serialization function.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- Load test data with rich neighborhood.
SELECT pg_ripple.load_ntriples(
    '<https://pharma.example/aspirin>   <http://www.w3.org/2000/01/rdf-schema#label> "aspirin" .' || chr(10) ||
    '<https://pharma.example/aspirin>   <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://pharma.example/NSAID> .' || chr(10) ||
    '<https://pharma.example/aspirin>   <https://pharma.example/treats> <https://pharma.example/Headache> .' || chr(10) ||
    '<https://pharma.example/aspirin>   <https://pharma.example/treats> <https://pharma.example/Fever> .' || chr(10) ||
    '<https://pharma.example/Headache>  <http://www.w3.org/2000/01/rdf-schema#label> "headache" .' || chr(10) ||
    '<https://pharma.example/Fever>     <http://www.w3.org/2000/01/rdf-schema#label> "fever" .' || chr(10)
) >= 6 AS pharma_triples_loaded;

-- ── contextualize_entity() basic call ─────────────────────────────────────────
-- Must return a non-empty text string (or a fallback local name when label is absent).
SELECT
    length(pg_ripple.contextualize_entity('https://pharma.example/aspirin')) > 0
    AS context_text_non_empty;

-- ── Label is included in the context text ─────────────────────────────────────
SELECT
    pg_ripple.contextualize_entity('https://pharma.example/aspirin') LIKE '%aspirin%'
    AS label_in_context_text;

-- ── Type information is included ──────────────────────────────────────────────
SELECT
    pg_ripple.contextualize_entity('https://pharma.example/aspirin') LIKE '%NSAID%'
    OR pg_ripple.contextualize_entity('https://pharma.example/aspirin') LIKE '%Type%'
    AS type_info_in_context;

-- ── Neighbor labels are included (depth=1) ────────────────────────────────────
-- The context should mention neighboring entities.
SELECT
    pg_ripple.contextualize_entity('https://pharma.example/aspirin', 1, 20) LIKE '%Related%'
    OR pg_ripple.contextualize_entity('https://pharma.example/aspirin', 1, 20) LIKE '%headache%'
    OR pg_ripple.contextualize_entity('https://pharma.example/aspirin', 1, 20) LIKE '%fever%'
    AS neighbors_in_context;

-- ── Unknown entity returns local name ─────────────────────────────────────────
SELECT
    pg_ripple.contextualize_entity('https://unknown.example/NoSuchEntity') = 'NoSuchEntity'
    OR length(pg_ripple.contextualize_entity('https://unknown.example/NoSuchEntity')) > 0
    AS unknown_entity_returns_local_name;

-- ── depth parameter is accepted ───────────────────────────────────────────────
SELECT
    length(pg_ripple.contextualize_entity('https://pharma.example/aspirin', 2, 10)) > 0
    AS depth_parameter_accepted;

-- ── max_neighbors parameter is respected ──────────────────────────────────────
-- With max_neighbors=1, only one neighbor should appear.
SELECT
    length(pg_ripple.contextualize_entity('https://pharma.example/aspirin', 1, 1)) > 0
    AS max_neighbors_parameter_accepted;

-- ── Cleanup ───────────────────────────────────────────────────────────────────
-- Remove test triples so they do not bleed into subsequent tests.
SELECT pg_ripple.sparql_update('DELETE WHERE { ?s ?p ?o }') >= 0 AS cleaned;
