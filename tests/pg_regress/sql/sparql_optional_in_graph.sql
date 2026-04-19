-- sparql_optional_in_graph.sql — OPTIONAL inside GRAPH with dedicated VP predicate (v0.40.0 bug fix)
--
-- Verifies that OPTIONAL {} inside GRAPH <g> {} works correctly for
-- predicates stored in dedicated VP tables (not vp_rare).
-- Before v0.40.0 this produced: column "..." does not exist

SET search_path TO pg_ripple, public;

CREATE EXTENSION IF NOT EXISTS pg_ripple;

-- ── Setup ─────────────────────────────────────────────────────────────────────

SELECT pg_ripple.load_ntriples($$
<https://opt.graph.test/alice> <https://opt.graph.test/name>  "Alice" .
<https://opt.graph.test/alice> <https://opt.graph.test/age>   "30"^^<http://www.w3.org/2001/XMLSchema#integer> .
<https://opt.graph.test/bob>   <https://opt.graph.test/name>  "Bob" .
$$) = 3 AS base_triples_loaded;

-- Load into a named graph.
SELECT pg_ripple.load_ntriples_into_graph($$
<https://opt.graph.test/carol> <https://opt.graph.test/name>  "Carol" .
<https://opt.graph.test/carol> <https://opt.graph.test/age>   "25"^^<http://www.w3.org/2001/XMLSchema#integer> .
<https://opt.graph.test/dave>  <https://opt.graph.test/name>  "Dave" .
$$, 'https://opt.graph.test/g1') = 3 AS named_graph_triples_loaded;

-- ── Test 1: OPTIONAL inside GRAPH — correct NULL/non-NULL counts ───────────────

-- In named graph g1: carol has age, dave does not.
-- Expected: 2 rows; 1 with age, 1 with NULL age.
SELECT count(*) = 2 AS row_count
FROM pg_ripple.sparql($$
    SELECT ?person ?age WHERE {
        GRAPH <https://opt.graph.test/g1> {
            ?person <https://opt.graph.test/name> ?name .
            OPTIONAL { ?person <https://opt.graph.test/age> ?age }
        }
    }
$$);

-- Verify the person with age is carol.
SELECT (result->>'age') IS NOT NULL AS carol_has_age
FROM pg_ripple.sparql($$
    SELECT ?age WHERE {
        GRAPH <https://opt.graph.test/g1> {
            <https://opt.graph.test/carol> <https://opt.graph.test/name> ?n .
            OPTIONAL { <https://opt.graph.test/carol> <https://opt.graph.test/age> ?age }
        }
    }
$$);

-- Verify dave has NULL age.
SELECT (result->>'age') IS NULL AS dave_has_null_age
FROM pg_ripple.sparql($$
    SELECT ?age WHERE {
        GRAPH <https://opt.graph.test/g1> {
            <https://opt.graph.test/dave> <https://opt.graph.test/name> ?n .
            OPTIONAL { <https://opt.graph.test/dave> <https://opt.graph.test/age> ?age }
        }
    }
$$);

-- ── Cleanup ───────────────────────────────────────────────────────────────────
SELECT pg_ripple.drop_graph('https://opt.graph.test/g1') IS NOT NULL AS graph_dropped;
