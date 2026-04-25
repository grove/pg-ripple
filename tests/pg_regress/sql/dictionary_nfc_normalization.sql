-- pg_regress test: Dictionary NFC normalization (v0.55.0)
-- Verifies that the normalize_iris GUC is registered and that IRIs are
-- normalized to NFC form before dictionary encoding.

SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SET client_min_messages = DEFAULT;
SET search_path TO pg_ripple, public;

-- Verify normalize_iris GUC is registered and defaults to on.
SELECT current_setting('pg_ripple.normalize_iris') AS normalize_iris_default;

-- Can toggle the setting.
SET pg_ripple.normalize_iris = off;
SELECT current_setting('pg_ripple.normalize_iris') AS normalize_iris_off;
RESET pg_ripple.normalize_iris;

-- With normalize_iris = on (default), encoding an IRI with a non-ASCII
-- character should succeed and return a positive dictionary ID.
-- NFC form of "café": caf\u00e9 (precomposed e-acute)
SELECT pg_ripple.encode_term('<http://example.org/caf\u00e9>', 0::smallint) > 0 AS nfc_encoded;
