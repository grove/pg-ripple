-- sparql_path_in_graph.sql — Property paths inside GRAPH (v0.40.0 bug fix)
--
-- Verifies that SPARQL property path expressions inside GRAPH {} work correctly.
-- Before v0.40.0 the graph filter was not threaded into property path CTEs.

SET search_path TO pg_ripple, public;

CREATE EXTENSION IF NOT EXISTS pg_ripple;

-- ── Setup ─────────────────────────────────────────────────────────────────────

-- Load into named graph g1.
SELECT pg_ripple.load_ntriples_into_graph($$
<https://path.graph.test/a> <https://path.graph.test/knows> <https://path.graph.test/b> .
<https://path.graph.test/b> <https://path.graph.test/knows> <https://path.graph.test/c> .
<https://path.graph.test/c> <https://path.graph.test/knows> <https://path.graph.test/d> .
$$, 'https://path.graph.test/g1') = 3 AS g1_loaded;

-- Load into named graph g2 (different chain).
SELECT pg_ripple.load_ntriples_into_graph($$
<https://path.graph.test/x> <https://path.graph.test/knows> <https://path.graph.test/y> .
<https://path.graph.test/y> <https://path.graph.test/knows> <https://path.graph.test/z> .
$$, 'https://path.graph.test/g2') = 2 AS g2_loaded;

-- ── Test 1: Kleene-plus path inside GRAPH ─────────────────────────────────────

-- From g1: a+ path from a reaches b, c, d (3 hops at most) → 3 results.
SELECT count(*) >= 1 AS path_in_g1_works
FROM pg_ripple.sparql($$
    SELECT ?t WHERE {
        GRAPH <https://path.graph.test/g1> {
            <https://path.graph.test/a> <https://path.graph.test/knows>+ ?t .
        }
    }
$$);

-- From g2: path from x should NOT reach b, c, d (different graph).
SELECT count(*) = 0 AS g2_not_polluted_by_g1
FROM pg_ripple.sparql($$
    SELECT ?t WHERE {
        GRAPH <https://path.graph.test/g2> {
            <https://path.graph.test/a> <https://path.graph.test/knows>+ ?t .
        }
    }
$$);

-- ── Test 2: Simple predicate path inside GRAPH ────────────────────────────────

SELECT count(*) = 1 AS single_hop_in_g1
FROM pg_ripple.sparql($$
    SELECT ?b WHERE {
        GRAPH <https://path.graph.test/g1> {
            <https://path.graph.test/a> <https://path.graph.test/knows> ?b .
        }
    }
$$);

-- Same predicate in g2 (x → y).
SELECT count(*) = 1 AS single_hop_in_g2
FROM pg_ripple.sparql($$
    SELECT ?y WHERE {
        GRAPH <https://path.graph.test/g2> {
            <https://path.graph.test/x> <https://path.graph.test/knows> ?y .
        }
    }
$$);

-- ── Cleanup ───────────────────────────────────────────────────────────────────
SELECT pg_ripple.drop_graph('https://path.graph.test/g1') IS NOT NULL AS g1_dropped;
SELECT pg_ripple.drop_graph('https://path.graph.test/g2') IS NOT NULL AS g2_dropped;
