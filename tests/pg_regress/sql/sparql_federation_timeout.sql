-- pg_regress test: SPARQL federation timeout (v0.16.0)
-- Verifies timeout and max_results GUC settings behaviour.

-- Ensure the pg_ripple library is loaded (registers GUCs via _PG_init).
-- In a fresh backend the library isn't loaded until a pg_ripple function runs.
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

-- ─── Timeout GUC ─────────────────────────────────────────────────────────────

-- Verify federation_timeout GUC is registered and has default 30.
SELECT current_setting('pg_ripple.federation_timeout')::int AS default_timeout;

-- Verify federation_max_results GUC is registered and has default 10000.
SELECT current_setting('pg_ripple.federation_max_results')::int AS default_max_results;

-- Verify federation_on_error GUC is registered (default is empty = 'warning').
SELECT COALESCE(
    NULLIF(current_setting('pg_ripple.federation_on_error'), ''),
    'warning'
) AS default_on_error;

-- ─── GUC boundary tests ───────────────────────────────────────────────────────

-- Set timeout to minimum (1 second).
SET pg_ripple.federation_timeout = 1;
SELECT current_setting('pg_ripple.federation_timeout')::int AS min_timeout;

-- Set max_results to a custom value.
SET pg_ripple.federation_max_results = 5000;
SELECT current_setting('pg_ripple.federation_max_results')::int AS custom_max_results;

-- Reset to defaults.
RESET pg_ripple.federation_timeout;
RESET pg_ripple.federation_max_results;

SELECT current_setting('pg_ripple.federation_timeout')::int AS reset_timeout;
SELECT current_setting('pg_ripple.federation_max_results')::int AS reset_max_results;

-- ─── Timeout with unreachable endpoint ───────────────────────────────────────

-- Register unreachable endpoint; short timeout; empty mode; check round-trip.
SELECT pg_ripple.register_endpoint('http://127.0.0.1:19998/timeout-test');

SET pg_ripple.federation_timeout = 1;
SET pg_ripple.federation_on_error = 'empty';

SELECT COUNT(*) AS timeout_empty_count
FROM pg_ripple.sparql(
    'SELECT ?s WHERE { SERVICE <http://127.0.0.1:19998/timeout-test> { ?s ?p ?o } }'
);

RESET pg_ripple.federation_timeout;
RESET pg_ripple.federation_on_error;

SELECT pg_ripple.remove_endpoint('http://127.0.0.1:19998/timeout-test');
