-- pg_regress test: GraphRAG Parquet export functions (v0.26.0)
--
-- Covers:
--   1. export_graphrag_entities() returns >= 0 rows written
--   2. export_graphrag_relationships() returns >= 0 rows written
--   3. export_graphrag_text_units() returns >= 0 rows written
--   4. Functions return 0 when no matching data exists in a named graph
--   5. Functions return > 0 when entities exist
--
-- Namespace: https://graphrag.test/export/

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET search_path TO pg_ripple, public;

-- ── Insert test entities for export ──────────────────────────────────────────
SELECT pg_ripple.load_turtle($DATA$
@prefix gr:   <https://graphrag.org/ns/> .
@prefix rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .

<https://graphrag.test/export/entity/e1>
    rdf:type       gr:Entity ;
    gr:title       "Export Entity One" ;
    gr:type        "ORGANIZATION" ;
    gr:description "First export test entity" ;
    gr:frequency   "5"^^xsd:integer ;
    gr:degree      "3"^^xsd:integer .

<https://graphrag.test/export/entity/e2>
    rdf:type       gr:Entity ;
    gr:title       "Export Entity Two" ;
    gr:type        "PERSON" .

<https://graphrag.test/export/rel/r1>
    rdf:type    gr:Relationship ;
    gr:source   <https://graphrag.test/export/entity/e1> ;
    gr:target   <https://graphrag.test/export/entity/e2> ;
    gr:weight   "0.75"^^xsd:float .

<https://graphrag.test/export/tu/t1>
    rdf:type       gr:TextUnit ;
    gr:text        "This is a sample text unit for export testing." ;
    gr:tokenCount  "9"^^xsd:integer .
$DATA$) > 0 AS test_data_loaded;

-- ── Export entities to /tmp (default graph) ───────────────────────────────────
SELECT pg_ripple.export_graphrag_entities(
    '',
    '/tmp/graphrag_regress_entities.parquet'
) >= 1 AS entities_exported;

-- ── Export relationships to /tmp ──────────────────────────────────────────────
SELECT pg_ripple.export_graphrag_relationships(
    '',
    '/tmp/graphrag_regress_relationships.parquet'
) >= 0 AS relationships_exported;

-- ── Export text units to /tmp ─────────────────────────────────────────────────
SELECT pg_ripple.export_graphrag_text_units(
    '',
    '/tmp/graphrag_regress_text_units.parquet'
) >= 1 AS text_units_exported;

-- ── Export from non-existent named graph returns 0 ───────────────────────────
SELECT pg_ripple.export_graphrag_entities(
    'https://graphrag.test/export/nonexistent',
    '/tmp/graphrag_regress_empty.parquet'
) = 0 AS empty_graph_returns_zero;

-- ── Cleanup: remove graphrag.test triples ────────────────────────────────────
DO $$
BEGIN
    DELETE FROM _pg_ripple.vp_rare
    WHERE s IN (SELECT id FROM _pg_ripple.dictionary WHERE value LIKE 'https://graphrag.test/%')
       OR p IN (SELECT id FROM _pg_ripple.dictionary WHERE value LIKE 'https://graphrag.test/%')
       OR s IN (SELECT id FROM _pg_ripple.dictionary WHERE value LIKE 'https://graphrag.org/ns/%')
       OR p IN (SELECT id FROM _pg_ripple.dictionary WHERE value LIKE 'https://graphrag.org/ns/%');
END $$;
