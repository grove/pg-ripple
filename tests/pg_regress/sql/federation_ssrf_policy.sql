-- pg_regress test: federation SSRF policy GUC (v0.55.0)
-- Verifies that the federation_endpoint_policy and federation_allowed_endpoints
-- GUCs are registered, accessible, and default to expected values.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- Verify federation_endpoint_policy GUC is registered and defaults to 'default-deny'.
SELECT current_setting('pg_ripple.federation_endpoint_policy') AS endpoint_policy;

-- Verify federation_allowed_endpoints GUC is registered (empty by default).
SELECT current_setting('pg_ripple.federation_allowed_endpoints', true) IS NOT NULL AS has_allowed_endpoints;

-- Switch to open mode and back.
SET pg_ripple.federation_endpoint_policy = 'open';
SELECT current_setting('pg_ripple.federation_endpoint_policy') AS policy_open;
SET pg_ripple.federation_endpoint_policy = 'allowlist';
SELECT current_setting('pg_ripple.federation_endpoint_policy') AS policy_allowlist;
RESET pg_ripple.federation_endpoint_policy;

-- Configure an allowlist entry and verify it is stored.
SET pg_ripple.federation_allowed_endpoints = 'https://dbpedia.org/sparql';
SELECT current_setting('pg_ripple.federation_allowed_endpoints') AS allowed_ep;
RESET pg_ripple.federation_allowed_endpoints;

-- Verify that a SERVICE query to a cloud-metadata endpoint raises an error
-- when policy = 'default-deny'.
-- (Uses DO block to catch the error and return a controlled result.)
DO $$
BEGIN
  PERFORM pg_ripple.sparql(
    'SELECT * WHERE { SERVICE <http://169.254.169.254/> { ?s ?p ?o } }'
  );
  RAISE NOTICE 'ERROR: SERVICE should have been blocked';
EXCEPTION WHEN OTHERS THEN
  IF SQLERRM LIKE '%PT606%' THEN
    RAISE NOTICE 'OK: SERVICE blocked with PT606';
  ELSE
    RAISE NOTICE 'UNEXPECTED ERROR: %', SQLERRM;
  END IF;
END
$$;
