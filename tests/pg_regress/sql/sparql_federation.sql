-- pg_regress test: SPARQL federation (v0.16.0)
-- Tests for SERVICE clause endpoint registration, allowlist enforcement,
-- error handling, local view rewrite, and management API.
--
-- NOTE: These tests do NOT make real HTTP calls to external endpoints.
-- They verify the allowlist, management API, and error-handling semantics
-- using a fake (unregistered) URL and a pre-registered test endpoint.

-- ─── Endpoint management API ─────────────────────────────────────────────────

-- Register an endpoint; it should appear in list_endpoints().
SELECT pg_ripple.register_endpoint('http://sparql.test/endpoint');

-- list_endpoints() must return the newly registered endpoint.
SELECT url, enabled, local_view_name
FROM pg_ripple.list_endpoints()
WHERE url = 'http://sparql.test/endpoint';

-- Register a second endpoint with a local_view_name override.
SELECT pg_ripple.register_endpoint(
    'http://sparql.local/view-endpoint',
    'test_stream'
);

-- Verify local_view_name is stored.
SELECT url, local_view_name
FROM pg_ripple.list_endpoints()
WHERE url = 'http://sparql.local/view-endpoint';

-- Disable an endpoint.
SELECT pg_ripple.disable_endpoint('http://sparql.test/endpoint');

-- Disabled endpoint should show enabled = false.
SELECT url, enabled
FROM pg_ripple.list_endpoints()
WHERE url = 'http://sparql.test/endpoint';

-- Re-enable via register_endpoint.
SELECT pg_ripple.register_endpoint('http://sparql.test/endpoint');

SELECT url, enabled
FROM pg_ripple.list_endpoints()
WHERE url = 'http://sparql.test/endpoint';

-- Remove an endpoint.
SELECT pg_ripple.remove_endpoint('http://sparql.local/view-endpoint');

-- It should no longer appear in list_endpoints().
SELECT COUNT(*) AS removed_count
FROM pg_ripple.list_endpoints()
WHERE url = 'http://sparql.local/view-endpoint';

-- ─── Allowlist enforcement ────────────────────────────────────────────────────

-- Attempting to use an unregistered endpoint in a SERVICE clause must raise
-- an ERROR (SSRF protection). We wrap in a DO block to catch the error.
DO $$
DECLARE
    raised BOOLEAN := false;
BEGIN
    BEGIN
        PERFORM pg_ripple.sparql(
            'SELECT ?s WHERE { SERVICE <http://unregistered.test/sparql> { ?s ?p ?o } }'
        );
    EXCEPTION WHEN OTHERS THEN
        IF SQLERRM LIKE '%federation endpoint not registered%' THEN
            raised := true;
        END IF;
    END;
    IF NOT raised THEN
        RAISE EXCEPTION 'Expected "federation endpoint not registered" error was not raised';
    END IF;
END $$;

SELECT 'unregistered endpoint raises error' AS ssrf_protection_check;

-- SERVICE SILENT on unregistered endpoint must return empty (no error).
-- With SILENT, a warning is emitted and results are empty.
SELECT COUNT(*) AS silent_unregistered_count
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { SERVICE SILENT <http://also-unregistered.test/sparql> { ?s ?p ?o } }'
);

-- ─── Direct _pg_ripple.federation_endpoints table checks ─────────────────────

-- Verify the enabled endpoint is stored correctly.
SELECT url, enabled
FROM _pg_ripple.federation_endpoints
WHERE url = 'http://sparql.test/endpoint';

-- ─── federation_on_error GUC ─────────────────────────────────────────────────

-- Default: SERVICE failure on registered-but-unreachable endpoint returns
-- empty (warning mode). Test using a registered but unreachable URL.
SELECT pg_ripple.register_endpoint('http://127.0.0.1:19999/sparql');

-- With default federation_on_error = 'warning', a failed SERVICE call returns
-- empty results (and emits a WARNING). The test checks the count is 0.
SET pg_ripple.federation_on_error = 'empty';
SET pg_ripple.federation_timeout = 2;

SELECT COUNT(*) AS unreachable_empty_count
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { SERVICE <http://127.0.0.1:19999/sparql> { ?s ?p ?o } }'
);

-- With federation_on_error = 'error', a failed call raises an ERROR.
SET pg_ripple.federation_on_error = 'error';

DO $$
DECLARE
    raised BOOLEAN := false;
BEGIN
    BEGIN
        PERFORM pg_ripple.sparql(
            'SELECT ?s WHERE { SERVICE <http://127.0.0.1:19999/sparql> { ?s ?p ?o } }'
        );
    EXCEPTION WHEN OTHERS THEN
        raised := true;
    END;
    IF NOT raised THEN
        RAISE EXCEPTION 'Expected ERROR for unreachable endpoint was not raised';
    END IF;
END $$;

SELECT 'federation_on_error=error raises on failure' AS on_error_error_check;

-- Reset GUC to defaults.
RESET pg_ripple.federation_on_error;
RESET pg_ripple.federation_timeout;

-- ─── Health table existence ───────────────────────────────────────────────────

-- The federation_health table must exist after extension install.
SELECT EXISTS(
    SELECT 1 FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname = '_pg_ripple' AND c.relname = 'federation_health'
) AS health_table_exists;

-- ─── Cleanup ──────────────────────────────────────────────────────────────────

SELECT pg_ripple.remove_endpoint('http://sparql.test/endpoint');
SELECT pg_ripple.remove_endpoint('http://127.0.0.1:19999/sparql');

SELECT COUNT(*) AS endpoints_after_cleanup
FROM _pg_ripple.federation_endpoints;

-- ─── B7-4: SERVICE SILENT + circuit-breaker interaction ──────────────────────
-- v0.60.0: SERVICE SILENT must correctly swallow PT605 (circuit-breaker-open)
-- and return the empty solution sequence per SPARQL 1.1 §8.3.1.

-- Register a fresh endpoint so the circuit-breaker starts in the closed state.
SELECT pg_ripple.register_endpoint('http://cb-silent.test/sparql');

-- Trip the circuit-breaker by forcing an error threshold.
-- Set a very short timeout so a connection to a non-existent host fails fast
-- and the circuit-breaker trips after one failure.
SET pg_ripple.federation_timeout = 50;
SET pg_ripple.circuit_breaker_threshold = 1;

-- First call (non-SILENT) — trips the circuit-breaker.
DO $$
BEGIN
    BEGIN
        PERFORM pg_ripple.sparql(
            'SELECT ?s WHERE { SERVICE <http://cb-silent.test/sparql> { ?s ?p ?o } }'
        );
    EXCEPTION WHEN OTHERS THEN
        -- Expected failure — circuit-breaker now trips after threshold=1 failure.
        NULL;
    END;
END $$;

-- Second call with SERVICE SILENT — circuit-breaker is open (PT605).
-- SILENT must return empty results, not raise an error.
SELECT pg_ripple.sparql(
    'SELECT ?s WHERE { SERVICE SILENT <http://cb-silent.test/sparql> { ?s ?p ?o } }'
) AS silent_cb_result;

SELECT 'SERVICE SILENT swallows PT605 circuit-breaker-open' AS b7_4_check;

-- Reset circuit-breaker GUCs and cleanup.
RESET pg_ripple.federation_timeout;
RESET pg_ripple.circuit_breaker_threshold;
SELECT pg_ripple.remove_endpoint('http://cb-silent.test/sparql');
