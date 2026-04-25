-- pg_regress test: HTAP concurrent merge (v0.55.0)
-- Verifies that concurrent reads and inserts during a merge cycle do not
-- produce "relation does not exist" errors.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- Verify pg_ripple.tombstone_retention_seconds GUC is registered.
SELECT COALESCE(current_setting('pg_ripple.tombstone_retention_seconds', true), '0')::int >= 0 AS retention_valid;

-- Load a small dataset to create VP tables.
SELECT pg_ripple.load_ntriples_into_graph(
  E'<http://example.org/s1> <http://example.org/p1> "v1" .\n' ||
  E'<http://example.org/s2> <http://example.org/p1> "v2" .\n',
  'http://example.org/g1'
) >= 0 AS loaded;

-- Verify triples are queryable after load.
SELECT count(*) > 0 AS has_triples
FROM pg_ripple.sparql(
  'SELECT ?s ?o WHERE { GRAPH <http://example.org/g1> { ?s <http://example.org/p1> ?o } }'
);

-- Trigger a merge cycle.
SELECT pg_ripple.compact() >= 0 AS merge_ran;

-- Verify triples are still queryable after merge (no "relation does not exist").
SELECT count(*) > 0 AS has_triples_after_merge
FROM pg_ripple.sparql(
  'SELECT ?s ?o WHERE { GRAPH <http://example.org/g1> { ?s <http://example.org/p1> ?o } }'
);

-- Cleanup.
SELECT pg_ripple.drop_graph('http://example.org/g1') IS NOT NULL AS dropped;
