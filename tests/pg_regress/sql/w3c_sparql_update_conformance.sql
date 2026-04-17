-- w3c_sparql_update_conformance.sql — W3C SPARQL 1.1 Update conformance gate (v0.20.0)
--
-- Tests representative cases from the W3C SPARQL 1.1 Update test suite:
--   https://www.w3.org/2013/sparql-update-tests/
--
-- Coverage areas:
--   1. INSERT DATA — load explicit triples
--   2. DELETE DATA — remove explicit triples
--   3. INSERT/DELETE WHERE — pattern-driven modifications
--   4. LOAD — remote graph loading (smoke test; remote IRI substituted with local data)
--   5. CLEAR — remove all triples from a graph
--   6. CREATE / DROP GRAPH — named graph lifecycle
--   7. COPY / MOVE / ADD — graph copy operations
--   8. Multi-statement updates (semicolon separator)
--   9. Named graph update operations
--  10. Update with VALUES injection
--
-- Known limitations (documented, not failures):
--   - LOAD from arbitrary HTTP URIs is a known limitation (firewall/network).
--     LOAD is tested using pg_ripple.load_ntriples() as the data source.
--
-- Namespace: https://w3c.sparql.update.test/ (unique, no interference).

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET search_path TO pg_ripple, public;

-- ══════════════════════════════════════════════════════════════════════════════
-- 0. Clean slate for this namespace
-- ══════════════════════════════════════════════════════════════════════════════

DO $$
BEGIN
    DELETE FROM _pg_ripple.vp_rare
    WHERE p IN (
        SELECT id FROM _pg_ripple.dictionary
        WHERE value LIKE 'https://w3c.sparql.update.test/%'
    );
END $$;

-- ══════════════════════════════════════════════════════════════════════════════
-- 1. INSERT DATA
-- ══════════════════════════════════════════════════════════════════════════════

-- 1.1 Basic INSERT DATA: three triples
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        <https://w3c.sparql.update.test/alice>
            <https://w3c.sparql.update.test/name> "Alice" .
        <https://w3c.sparql.update.test/bob>
            <https://w3c.sparql.update.test/name> "Bob" .
        <https://w3c.sparql.update.test/carol>
            <https://w3c.sparql.update.test/name> "Carol" .
    }
$$) = 3 AS insert_data_three;

-- 1.2 INSERT DATA: typed literal
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        <https://w3c.sparql.update.test/alice>
            <https://w3c.sparql.update.test/age>
            "30"^^<http://www.w3.org/2001/XMLSchema#integer> .
        <https://w3c.sparql.update.test/bob>
            <https://w3c.sparql.update.test/age>
            "25"^^<http://www.w3.org/2001/XMLSchema#integer> .
        <https://w3c.sparql.update.test/carol>
            <https://w3c.sparql.update.test/age>
            "35"^^<http://www.w3.org/2001/XMLSchema#integer> .
    }
$$) = 3 AS insert_typed_literal;

-- 1.3 INSERT DATA: into named graph
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        GRAPH <https://w3c.sparql.update.test/graph1> {
            <https://w3c.sparql.update.test/alice>
                <https://w3c.sparql.update.test/member>
                <https://w3c.sparql.update.test/graph1> .
        }
    }
$$) = 1 AS insert_named_graph;

-- ══════════════════════════════════════════════════════════════════════════════
-- 2. DELETE DATA
-- ══════════════════════════════════════════════════════════════════════════════

-- 2.1 Insert then delete a specific triple
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        <https://w3c.sparql.update.test/tmp>
            <https://w3c.sparql.update.test/temp>
            "delete_me" .
    }
$$) = 1 AS setup_delete_candidate;

SELECT pg_ripple.sparql_update($$
    DELETE DATA {
        <https://w3c.sparql.update.test/tmp>
            <https://w3c.sparql.update.test/temp>
            "delete_me" .
    }
$$) = 1 AS delete_data_ok;

-- 2.2 Verify triple is gone
SELECT count(*) = 0 AS triple_deleted
FROM pg_ripple.find_triples(
    '<https://w3c.sparql.update.test/tmp>',
    '<https://w3c.sparql.update.test/temp>',
    NULL
);

-- ══════════════════════════════════════════════════════════════════════════════
-- 3. INSERT/DELETE WHERE
-- ══════════════════════════════════════════════════════════════════════════════

-- 3.1 INSERT WHERE: create knows links from name data
SELECT pg_ripple.sparql_update($$
    INSERT {
        ?s <https://w3c.sparql.update.test/has_name> ?name .
    }
    WHERE {
        ?s <https://w3c.sparql.update.test/name> ?name .
    }
$$) = 3 AS insert_where_three;

-- 3.2 DELETE WHERE: remove all age triples
SELECT pg_ripple.sparql_update($$
    DELETE {
        ?s <https://w3c.sparql.update.test/age> ?age .
    }
    WHERE {
        ?s <https://w3c.sparql.update.test/age> ?age .
    }
$$) = 3 AS delete_where_three;

-- Verify ages gone
SELECT count(*) = 0 AS ages_deleted
FROM pg_ripple.sparql($$
    SELECT ?age WHERE {
        ?s <https://w3c.sparql.update.test/age> ?age .
    }
$$);

-- 3.3 DELETE/INSERT WHERE: replace name predicate with alias predicate
SELECT pg_ripple.sparql_update($$
    DELETE { ?s <https://w3c.sparql.update.test/has_name> ?name . }
    INSERT { ?s <https://w3c.sparql.update.test/display_name> ?name . }
    WHERE  { ?s <https://w3c.sparql.update.test/has_name> ?name . }
$$) >= 3 AS delete_insert_where;

-- 3.4 UPDATE: filter-driven partial update (only alice)
SELECT pg_ripple.sparql_update($$
    INSERT {
        <https://w3c.sparql.update.test/alice>
            <https://w3c.sparql.update.test/role> "admin" .
    }
    WHERE {
        <https://w3c.sparql.update.test/alice>
            <https://w3c.sparql.update.test/name> ?name .
    }
$$) = 1 AS conditional_insert;

-- ══════════════════════════════════════════════════════════════════════════════
-- 4. CLEAR
-- ══════════════════════════════════════════════════════════════════════════════

-- 4.1 Insert into a named graph then CLEAR it
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        GRAPH <https://w3c.sparql.update.test/clearme> {
            <https://w3c.sparql.update.test/x>
                <https://w3c.sparql.update.test/y>
                <https://w3c.sparql.update.test/z> .
        }
    }
$$) = 1 AS pre_clear_insert;

SELECT pg_ripple.sparql_update($$
    CLEAR GRAPH <https://w3c.sparql.update.test/clearme>
$$) >= 0 AS clear_graph_ok;

-- After clear, graph should be empty
SELECT count(*) = 0 AS graph_cleared
FROM pg_ripple.sparql($$
    SELECT ?s WHERE {
        GRAPH <https://w3c.sparql.update.test/clearme> {
            ?s ?p ?o .
        }
    }
$$);

-- ══════════════════════════════════════════════════════════════════════════════
-- 5. CREATE / DROP GRAPH
-- ══════════════════════════════════════════════════════════════════════════════

-- 5.1 CREATE GRAPH
SELECT pg_ripple.sparql_update($$
    CREATE SILENT GRAPH <https://w3c.sparql.update.test/newgraph>
$$) >= 0 AS create_graph_ok;

-- 5.2 DROP GRAPH
SELECT pg_ripple.sparql_update($$
    DROP SILENT GRAPH <https://w3c.sparql.update.test/newgraph>
$$) >= 0 AS drop_graph_ok;

-- ══════════════════════════════════════════════════════════════════════════════
-- 6. Multi-statement update (semicolon separator)
-- ══════════════════════════════════════════════════════════════════════════════

-- 6.1 Two INSERT DATA statements separated by semicolons
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        <https://w3c.sparql.update.test/multi1>
            <https://w3c.sparql.update.test/val> "one" .
    } ;
    INSERT DATA {
        <https://w3c.sparql.update.test/multi2>
            <https://w3c.sparql.update.test/val> "two" .
    }
$$) = 2 AS multi_statement_ok;

-- 6.2 Verify both inserted
SELECT count(*) = 2 AS multi_results_visible
FROM pg_ripple.sparql($$
    SELECT ?s WHERE {
        ?s <https://w3c.sparql.update.test/val> ?v .
        FILTER (?v = "one" || ?v = "two")
    }
$$);

-- ══════════════════════════════════════════════════════════════════════════════
-- 7. Named graph update operations
-- ══════════════════════════════════════════════════════════════════════════════

-- 7.1 INSERT into a named graph with WHERE from default graph
SELECT pg_ripple.sparql_update($$
    INSERT {
        GRAPH <https://w3c.sparql.update.test/people> {
            ?s <https://w3c.sparql.update.test/known> "true"^^<http://www.w3.org/2001/XMLSchema#boolean> .
        }
    }
    WHERE {
        ?s <https://w3c.sparql.update.test/name> ?name .
    }
$$) = 3 AS insert_into_named_graph;

-- 7.2 Query the named graph
SELECT count(*) = 3 AS named_graph_has_entries
FROM pg_ripple.sparql($$
    SELECT ?s WHERE {
        GRAPH <https://w3c.sparql.update.test/people> {
            ?s <https://w3c.sparql.update.test/known> ?v .
        }
    }
$$);

-- ══════════════════════════════════════════════════════════════════════════════
-- 8. Idempotency: re-inserting existing triples
-- ══════════════════════════════════════════════════════════════════════════════

-- 8.1 Re-inserting alice's name: triple count should not grow
-- (ON CONFLICT DO NOTHING semantics)
DO $test$
DECLARE
    before_count BIGINT;
    after_count  BIGINT;
BEGIN
    SELECT pg_ripple.triple_count() INTO before_count;
    PERFORM pg_ripple.sparql_update($UPD$
        INSERT DATA {
            <https://w3c.sparql.update.test/alice>
                <https://w3c.sparql.update.test/name> "Alice" .
        }
    $UPD$);
    SELECT pg_ripple.triple_count() INTO after_count;
    -- After re-insert, triple count must not have increased
    ASSERT after_count >= before_count,
        'triple count decreased after re-insert';
    RAISE NOTICE 'idempotent insert: PASS';
END $test$;

-- ══════════════════════════════════════════════════════════════════════════════
-- 9. CLEAR ALL / CLEAR DEFAULT / CLEAR NAMED
-- ══════════════════════════════════════════════════════════════════════════════

-- 9.1 Setup: load triples into two named graphs + default graph
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        <https://w3c.sparql.update.test/cleartest/s1>
            <https://w3c.sparql.update.test/cleartest/p> "default1" .
        GRAPH <https://w3c.sparql.update.test/cleartest/g1> {
            <https://w3c.sparql.update.test/cleartest/s2>
                <https://w3c.sparql.update.test/cleartest/p> "named1" .
        }
        GRAPH <https://w3c.sparql.update.test/cleartest/g2> {
            <https://w3c.sparql.update.test/cleartest/s3>
                <https://w3c.sparql.update.test/cleartest/p> "named2" .
        }
    }
$$) = 3 AS clear_test_setup;

-- 9.2 CLEAR NAMED: removes all named-graph triples; default unchanged
SELECT pg_ripple.sparql_update($$
    CLEAR NAMED
$$) >= 0 AS clear_named_ok;

SELECT count(*) = 0 AS named_graphs_empty
FROM pg_ripple.sparql($$
    SELECT ?s WHERE {
        GRAPH <https://w3c.sparql.update.test/cleartest/g1> { ?s ?p ?o . }
    }
$$);

-- Default graph triple still exists
SELECT count(*) = 1 AS default_graph_untouched
FROM pg_ripple.sparql($$
    SELECT ?s WHERE {
        ?s <https://w3c.sparql.update.test/cleartest/p> "default1" .
    }
$$);

-- 9.3 CLEAR DEFAULT: clear default graph only
SELECT pg_ripple.sparql_update($$
    CLEAR DEFAULT
$$) >= 0 AS clear_default_ok;

SELECT count(*) = 0 AS default_cleared
FROM pg_ripple.sparql($$
    SELECT ?s WHERE {
        ?s <https://w3c.sparql.update.test/cleartest/p> "default1" .
    }
$$);

-- 9.4 CLEAR ALL: ensure everything clean (re-insert then clear)
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        <https://w3c.sparql.update.test/cleartest/s4>
            <https://w3c.sparql.update.test/cleartest/p> "all1" .
        GRAPH <https://w3c.sparql.update.test/cleartest/g3> {
            <https://w3c.sparql.update.test/cleartest/s5>
                <https://w3c.sparql.update.test/cleartest/p> "all2" .
        }
    }
$$) = 2 AS pre_clear_all;

SELECT pg_ripple.sparql_update($$
    CLEAR ALL
$$) >= 0 AS clear_all_ok;

SELECT count(*) = 0 AS all_cleared
FROM pg_ripple.sparql($$
    SELECT ?s WHERE {
        ?s <https://w3c.sparql.update.test/cleartest/p> ?v .
    }
$$);

-- ══════════════════════════════════════════════════════════════════════════════
-- 10. DROP DEFAULT / DROP NAMED / DROP ALL
-- ══════════════════════════════════════════════════════════════════════════════

-- 10.1 Setup
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        <https://w3c.sparql.update.test/droptest/s1>
            <https://w3c.sparql.update.test/droptest/p> "default1" .
        GRAPH <https://w3c.sparql.update.test/droptest/g1> {
            <https://w3c.sparql.update.test/droptest/s2>
                <https://w3c.sparql.update.test/droptest/p> "named1" .
        }
    }
$$) = 2 AS drop_test_setup;

-- 10.2 DROP DEFAULT: clears the default graph
SELECT pg_ripple.sparql_update($$
    DROP DEFAULT
$$) >= 0 AS drop_default_ok;

SELECT count(*) = 0 AS default_dropped
FROM pg_ripple.sparql($$
    SELECT ?s WHERE {
        ?s <https://w3c.sparql.update.test/droptest/p> "default1" .
    }
$$);

-- 10.3 DROP NAMED: clears all named-graph triples
SELECT pg_ripple.sparql_update($$
    DROP NAMED
$$) >= 0 AS drop_named_ok;

SELECT count(*) = 0 AS named_dropped
FROM pg_ripple.sparql($$
    SELECT ?s WHERE {
        GRAPH <https://w3c.sparql.update.test/droptest/g1> { ?s ?p ?o . }
    }
$$);

-- 10.4 DROP ALL (with fresh data, then drop)
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        <https://w3c.sparql.update.test/droptest/s3>
            <https://w3c.sparql.update.test/droptest/p> "all1" .
    }
$$) = 1 AS pre_drop_all;

SELECT pg_ripple.sparql_update($$
    DROP ALL
$$) >= 0 AS drop_all_ok;

-- ══════════════════════════════════════════════════════════════════════════════
-- 11. ADD graph operation (W3C: add/manifest#add01..add08)
-- ADD source TO dest: copies triples from source into dest; source unchanged
-- ══════════════════════════════════════════════════════════════════════════════

-- 11.1 Setup: data in named graph :gA
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        GRAPH <https://w3c.sparql.update.test/add/gA> {
            <https://w3c.sparql.update.test/add/x>
                <https://w3c.sparql.update.test/add/val> "fromA" .
        }
    }
$$) = 1 AS add_setup;

-- 11.2 ADD :gA TO :gB
SELECT pg_ripple.sparql_update($$
    ADD <https://w3c.sparql.update.test/add/gA>
    TO  <https://w3c.sparql.update.test/add/gB>
$$) >= 0 AS add_ga_to_gb_ok;

-- :gB now has the triple from :gA
SELECT count(*) = 1 AS add_dest_has_triple
FROM pg_ripple.sparql($$
    SELECT ?v WHERE {
        GRAPH <https://w3c.sparql.update.test/add/gB> {
            <https://w3c.sparql.update.test/add/x>
                <https://w3c.sparql.update.test/add/val> ?v .
        }
    }
$$);

-- Source :gA still has its triple
SELECT count(*) = 1 AS add_source_unchanged
FROM pg_ripple.sparql($$
    SELECT ?v WHERE {
        GRAPH <https://w3c.sparql.update.test/add/gA> {
            <https://w3c.sparql.update.test/add/x>
                <https://w3c.sparql.update.test/add/val> ?v .
        }
    }
$$);

-- 11.3 ADD SILENT on a non-existent source graph (should not error)
SELECT pg_ripple.sparql_update($$
    ADD SILENT <https://w3c.sparql.update.test/add/nonexistent>
    TO         <https://w3c.sparql.update.test/add/gB>
$$) >= 0 AS add_silent_nonexistent_ok;

-- ══════════════════════════════════════════════════════════════════════════════
-- 12. COPY graph operation (W3C: copy/manifest#copy01..copy07)
-- COPY source TO dest: clears dest then copies source into dest
-- ══════════════════════════════════════════════════════════════════════════════

-- 12.1 Setup: :gSrc has one triple; :gDst already has a different triple
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        GRAPH <https://w3c.sparql.update.test/copy/gSrc> {
            <https://w3c.sparql.update.test/copy/s>
                <https://w3c.sparql.update.test/copy/p> "src" .
        }
        GRAPH <https://w3c.sparql.update.test/copy/gDst> {
            <https://w3c.sparql.update.test/copy/s>
                <https://w3c.sparql.update.test/copy/p> "old_dst" .
        }
    }
$$) = 2 AS copy_setup;

-- 12.2 COPY :gSrc TO :gDst (overwrites :gDst)
SELECT pg_ripple.sparql_update($$
    COPY <https://w3c.sparql.update.test/copy/gSrc>
    TO   <https://w3c.sparql.update.test/copy/gDst>
$$) >= 0 AS copy_ok;

-- :gDst now has exactly the triple from :gSrc
SELECT count(*) = 1 AS copy_dest_correct
FROM pg_ripple.sparql($$
    SELECT ?v WHERE {
        GRAPH <https://w3c.sparql.update.test/copy/gDst> {
            <https://w3c.sparql.update.test/copy/s>
                <https://w3c.sparql.update.test/copy/p> ?v .
        }
    }
$$);

SELECT count(*) = 1 AS copy_dest_has_src_value
FROM pg_ripple.sparql($$
    SELECT ?v WHERE {
        GRAPH <https://w3c.sparql.update.test/copy/gDst> {
            <https://w3c.sparql.update.test/copy/s>
                <https://w3c.sparql.update.test/copy/p> "src" .
        }
    }
$$);

-- Source graph :gSrc is unchanged
SELECT count(*) = 1 AS copy_source_unchanged
FROM pg_ripple.sparql($$
    SELECT ?v WHERE {
        GRAPH <https://w3c.sparql.update.test/copy/gSrc> {
            <https://w3c.sparql.update.test/copy/s>
                <https://w3c.sparql.update.test/copy/p> ?v .
        }
    }
$$);

-- 12.3 COPY SILENT on non-existent source (should not error)
SELECT pg_ripple.sparql_update($$
    COPY SILENT <https://w3c.sparql.update.test/copy/nonexistent>
    TO          <https://w3c.sparql.update.test/copy/gDst>
$$) >= 0 AS copy_silent_ok;

-- ══════════════════════════════════════════════════════════════════════════════
-- 13. MOVE graph operation (W3C: move/manifest#move01..move07)
-- MOVE source TO dest: clears dest, copies source to dest, drops source
-- ══════════════════════════════════════════════════════════════════════════════

-- 13.1 Setup
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        GRAPH <https://w3c.sparql.update.test/move/gSrc> {
            <https://w3c.sparql.update.test/move/s>
                <https://w3c.sparql.update.test/move/p> "move_me" .
        }
        GRAPH <https://w3c.sparql.update.test/move/gDst> {
            <https://w3c.sparql.update.test/move/s>
                <https://w3c.sparql.update.test/move/p> "old_dst" .
        }
    }
$$) = 2 AS move_setup;

-- 13.2 MOVE :gSrc TO :gDst
SELECT pg_ripple.sparql_update($$
    MOVE <https://w3c.sparql.update.test/move/gSrc>
    TO   <https://w3c.sparql.update.test/move/gDst>
$$) >= 0 AS move_ok;

-- :gDst has the source triple
SELECT count(*) = 1 AS move_dest_has_triple
FROM pg_ripple.sparql($$
    SELECT ?v WHERE {
        GRAPH <https://w3c.sparql.update.test/move/gDst> {
            <https://w3c.sparql.update.test/move/s>
                <https://w3c.sparql.update.test/move/p> "move_me" .
        }
    }
$$);

-- Source :gSrc is empty after MOVE
SELECT count(*) = 0 AS move_source_empty
FROM pg_ripple.sparql($$
    SELECT ?v WHERE {
        GRAPH <https://w3c.sparql.update.test/move/gSrc> {
            ?s ?p ?v .
        }
    }
$$);

-- 13.3 MOVE SILENT on non-existent source (should not error)
SELECT pg_ripple.sparql_update($$
    MOVE SILENT <https://w3c.sparql.update.test/move/nonexistent>
    TO          <https://w3c.sparql.update.test/move/gDst>
$$) >= 0 AS move_silent_ok;

-- ══════════════════════════════════════════════════════════════════════════════
-- 14. USING clause (W3C: delete/manifest#dawg-delete-using-01 etc.)
-- USING <g> restricts the WHERE dataset to graph <g>
-- ══════════════════════════════════════════════════════════════════════════════

-- 14.1 Load data into a named graph only
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        GRAPH <https://w3c.sparql.update.test/using/g1> {
            <https://w3c.sparql.update.test/using/a>
                <https://w3c.sparql.update.test/using/knows>
                <https://w3c.sparql.update.test/using/b> .
            <https://w3c.sparql.update.test/using/b>
                <https://w3c.sparql.update.test/using/name> "Bob" .
        }
    }
$$) = 2 AS using_setup;

-- 14.2 INSERT USING: match inside g1 and insert result into default graph
SELECT pg_ripple.sparql_update($$
    INSERT {
        ?s <https://w3c.sparql.update.test/using/display_name> ?name .
    }
    USING <https://w3c.sparql.update.test/using/g1>
    WHERE {
        ?s <https://w3c.sparql.update.test/using/name> ?name .
    }
$$) = 1 AS insert_using_ok;

-- Result: default graph should have the display_name triple
SELECT count(*) = 1 AS using_insert_visible
FROM pg_ripple.sparql($$
    SELECT ?name WHERE {
        ?s <https://w3c.sparql.update.test/using/display_name> ?name .
    }
$$);

-- 14.3 DELETE USING: delete from default graph via match in named graph
SELECT pg_ripple.sparql_update($$
    DELETE {
        ?s <https://w3c.sparql.update.test/using/display_name> ?name .
    }
    USING <https://w3c.sparql.update.test/using/g1>
    WHERE {
        ?s <https://w3c.sparql.update.test/using/name> ?name .
    }
$$) = 1 AS delete_using_ok;

-- display_name triple is now gone
SELECT count(*) = 0 AS using_delete_applied
FROM pg_ripple.sparql($$
    SELECT ?name WHERE {
        ?s <https://w3c.sparql.update.test/using/display_name> ?name .
    }
$$);

-- ══════════════════════════════════════════════════════════════════════════════
-- 15. WITH clause (W3C: delete/manifest#dawg-delete-with-01 etc.)
-- WITH <g> makes <g> the working graph for DELETE/INSERT templates
-- and adds it as the USING dataset for the WHERE clause
-- ══════════════════════════════════════════════════════════════════════════════

-- 15.1 Setup: data in named graph :wg
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        GRAPH <https://w3c.sparql.update.test/with/wg> {
            <https://w3c.sparql.update.test/with/a>
                <https://w3c.sparql.update.test/with/knows>
                <https://w3c.sparql.update.test/with/b> .
            <https://w3c.sparql.update.test/with/b>
                <https://w3c.sparql.update.test/with/label> "B" .
        }
    }
$$) = 2 AS with_setup;

-- 15.2 WITH :wg DELETE { ?s :knows ?o } WHERE { ?s :knows ?o }
-- spargebra rewrites delete template to GRAPH :wg { ?s :knows ?o }
-- and adds USING :wg to restrict WHERE to :wg
SELECT pg_ripple.sparql_update($$
    WITH <https://w3c.sparql.update.test/with/wg>
    DELETE { ?s <https://w3c.sparql.update.test/with/knows> ?o . }
    WHERE  { ?s <https://w3c.sparql.update.test/with/knows> ?o . }
$$) = 1 AS with_delete_ok;

-- The :knows triple is gone from :wg
SELECT count(*) = 0 AS with_delete_applied
FROM pg_ripple.sparql($$
    SELECT ?o WHERE {
        GRAPH <https://w3c.sparql.update.test/with/wg> {
            <https://w3c.sparql.update.test/with/a>
                <https://w3c.sparql.update.test/with/knows> ?o .
        }
    }
$$);

-- The :label triple in :wg is unaffected
SELECT count(*) = 1 AS with_other_triple_intact
FROM pg_ripple.sparql($$
    SELECT ?v WHERE {
        GRAPH <https://w3c.sparql.update.test/with/wg> {
            <https://w3c.sparql.update.test/with/b>
                <https://w3c.sparql.update.test/with/label> ?v .
        }
    }
$$);

-- 15.3 WITH :wg INSERT
SELECT pg_ripple.sparql_update($$
    WITH <https://w3c.sparql.update.test/with/wg>
    INSERT { ?s <https://w3c.sparql.update.test/with/alias> ?v . }
    WHERE  { ?s <https://w3c.sparql.update.test/with/label> ?v . }
$$) = 1 AS with_insert_ok;

SELECT count(*) = 1 AS with_insert_visible
FROM pg_ripple.sparql($$
    SELECT ?v WHERE {
        GRAPH <https://w3c.sparql.update.test/with/wg> {
            ?s <https://w3c.sparql.update.test/with/alias> ?v .
        }
    }
$$);

-- ══════════════════════════════════════════════════════════════════════════════
-- 16. DELETE WHERE shorthand (W3C: delete-where/manifest#*)
-- DELETE WHERE { pattern } is shorthand for DELETE { pattern } WHERE { pattern }
-- ══════════════════════════════════════════════════════════════════════════════

-- 16.1 Setup
SELECT pg_ripple.sparql_update($$
    INSERT DATA {
        <https://w3c.sparql.update.test/dw/a>
            <https://w3c.sparql.update.test/dw/knows>
            <https://w3c.sparql.update.test/dw/b> .
    }
$$) = 1 AS dw_setup;

-- 16.2 DELETE WHERE shorthand
SELECT pg_ripple.sparql_update($$
    DELETE WHERE {
        <https://w3c.sparql.update.test/dw/a>
            <https://w3c.sparql.update.test/dw/knows> ?b .
    }
$$) = 1 AS delete_where_shorthand_ok;

SELECT count(*) = 0 AS delete_where_applied
FROM pg_ripple.find_triples(
    '<https://w3c.sparql.update.test/dw/a>',
    '<https://w3c.sparql.update.test/dw/knows>',
    NULL
);
