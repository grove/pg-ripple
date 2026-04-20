-- Migration 0.41.0 → 0.42.0: Named-graph local SERVICE execution
--
-- Adds `graph_iri TEXT` column to `_pg_ripple.federation_endpoints`.
-- When set, SERVICE clauses targeting this endpoint URL are satisfied by
-- querying the local named graph with that IRI instead of making an HTTP call.
-- This enables mock endpoints for the W3C federation test suite and offline testing.
--
-- Also adds an optional 4th argument `graph_iri` to `pg_ripple.register_endpoint()`.

ALTER TABLE _pg_ripple.federation_endpoints
    ADD COLUMN IF NOT EXISTS graph_iri TEXT;
