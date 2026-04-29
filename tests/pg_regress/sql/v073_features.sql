-- pg_regress test: v0.73.0 feature gate
--   SUB-01:              SPARQL live subscriptions (subscribe_sparql / unsubscribe_sparql)
--   JSON-MAPPING-01:     Named bidirectional JSON<->RDF mapping registry
--   JSONLD-INGEST-02:    Multi-graph JSON-LD ingest (json_ld_load)
--   FEATURE-STATUS-02:   New feature_status entries for v0.73.0
--   SPARQL12-01:         SPARQL 1.2 tracking document
--   CONTRIB-01:          CONTRIBUTING.md present
--   TAXONOMY-01:         Feature status taxonomy doc
--   HELM-01:             Helm chart sidecar image config

CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;
SET search_path TO pg_ripple, public;

-- ── Part 1: SUB-01 — SPARQL live subscriptions ───────────────────────────────

-- 1a. sparql_subscription is in feature_status with experimental status.
SELECT status AS sparql_subscription_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'sparql_subscription';

-- 1b. subscribe_sparql and unsubscribe_sparql are callable.
SELECT pg_ripple.subscribe_sparql(
    'test_sub_v073',
    'SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 1',
    NULL
) IS NOT DISTINCT FROM NULL AS subscribe_returns_void;

-- 1c. list_sparql_subscriptions returns the registered subscription.
SELECT count(*) = 1 AS subscription_registered
FROM pg_ripple.list_sparql_subscriptions()
WHERE subscription_id = 'test_sub_v073';

-- 1d. unsubscribe removes the subscription.
SELECT pg_ripple.unsubscribe_sparql('test_sub_v073') IS NOT DISTINCT FROM NULL AS unsubscribe_returns_void;

SELECT count(*) = 0 AS subscription_removed
FROM pg_ripple.list_sparql_subscriptions()
WHERE subscription_id = 'test_sub_v073';

-- 1e. sparql_subscriptions table exists in _pg_ripple schema.
SELECT count(*) > 0 AS subscription_table_exists
FROM information_schema.tables
WHERE table_schema = '_pg_ripple'
  AND table_name = 'sparql_subscriptions';

-- ── Part 2: JSON-MAPPING-01 — Named JSON<->RDF mapping registry ──────────────

-- 2a. json_mapping feature is in feature_status with experimental status.
SELECT status AS json_mapping_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'json_mapping';

-- 2b. register_json_mapping is callable.
SELECT pg_ripple.register_json_mapping(
    'test_mapping_v073',
    '{"@context": {"name": "http://schema.org/name", "age": {"@id": "http://schema.org/age", "@type": "xsd:integer"}}}',
    NULL
) IS NOT DISTINCT FROM NULL AS register_mapping_returns_void;

-- 2c. json_mappings table exists.
SELECT count(*) > 0 AS json_mappings_table_exists
FROM information_schema.tables
WHERE table_schema = '_pg_ripple'
  AND table_name = 'json_mappings';

-- 2d. json_mapping_warnings table exists.
SELECT count(*) > 0 AS json_mapping_warnings_table_exists
FROM information_schema.tables
WHERE table_schema = '_pg_ripple'
  AND table_name = 'json_mapping_warnings';

-- 2e. Registered mapping is visible in the catalog.
SELECT count(*) = 1 AS mapping_in_catalog
FROM _pg_ripple.json_mappings
WHERE name = 'test_mapping_v073';

-- ── Part 3: JSONLD-INGEST-02 — Multi-graph JSON-LD ingest ────────────────────

-- 3a. json_ld_multi_ingest feature is in feature_status.
SELECT status AS jsonld_multi_ingest_status
FROM pg_ripple.feature_status()
WHERE feature_name = 'json_ld_multi_ingest';

-- 3b. json_ld_load is callable with a simple single-node document.
SELECT pg_ripple.json_ld_load(
    '{"@id": "http://example.org/v073/alice", "http://schema.org/name": "Alice"}'::jsonb,
    'http://example.org/v073/graph'
) >= 0 AS json_ld_load_returns_count;

-- ── Part 4: FEATURE-STATUS-02 — New feature_status entries ───────────────────

-- 4a. sparql_12 feature is present.
SELECT count(*) > 0 AS sparql_12_present
FROM pg_ripple.feature_status()
WHERE feature_name = 'sparql_12';

-- 4b. llm_sparql_repair feature is present.
SELECT count(*) > 0 AS llm_sparql_repair_present
FROM pg_ripple.feature_status()
WHERE feature_name = 'llm_sparql_repair';

-- 4c. kge_embeddings feature is present.
SELECT count(*) > 0 AS kge_embeddings_present
FROM pg_ripple.feature_status()
WHERE feature_name = 'kge_embeddings';

-- 4d. sparql_nl_to_sparql feature is present.
SELECT count(*) > 0 AS sparql_nl_to_sparql_present
FROM pg_ripple.feature_status()
WHERE feature_name = 'sparql_nl_to_sparql';

-- 4e. Total feature coverage has grown.
SELECT count(DISTINCT feature_name) >= 25 AS has_sufficient_feature_coverage
FROM pg_ripple.feature_status();

-- ── Part 5: General API stability ────────────────────────────────────────────

-- 5a. sparql() is callable.
SELECT count(*) >= 0 AS sparql_callable
FROM pg_ripple.sparql('SELECT * WHERE { ?s ?p ?o } LIMIT 0');

-- 5b. sparql_update() is callable.
SELECT pg_ripple.sparql_update('INSERT DATA {}') IS NOT NULL AS sparql_update_callable;

-- 5c. Extension version is 0.73.0.
SELECT default_version = '0.73.0' AS correct_version
FROM pg_catalog.pg_available_extensions
WHERE name = 'pg_ripple';
