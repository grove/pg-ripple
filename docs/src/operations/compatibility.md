# pg_ripple / pg_ripple_http Compatibility Matrix

`pg_ripple` (the PostgreSQL extension) and `pg_ripple_http` (the standalone HTTP companion
service) are versioned independently. This page documents which extension versions are compatible
with which HTTP companion versions and what guarantees apply to the combination.

## Versioning policy

- **pg_ripple** follows semantic versioning tied to extension features and PostgreSQL catalog changes.
- **pg_ripple_http** follows its own version series (currently `0.x.y`) since it is a standalone
  binary with its own release cadence. The HTTP companion version number tracks the *minimum*
  pg_ripple extension version it was tested against.

A given `pg_ripple_http` release is compatible with the extension version range
`[tested_with, next_major)`. The HTTP companion logs a warning at startup if the installed
extension version is outside its known-compatible range.

## Compatibility table

| pg_ripple_http version | pg_ripple extension range | Notes |
|------------------------|---------------------------|-------|
| 0.136.x | ≥ 0.135.0 | Hardened candidate for the frozen v1 query contract; external audit evidence is tracked separately |
| 0.135.x | ≥ 0.135.0 | Typed query bindings, governed prefix mode, and stable v1 query manifest |
| 0.133.x | ≥ 0.132.0 | Resilience health API and recovery qualification (v0.133.0) |
| 0.134.x | ≥ 0.134.0 | Performance, scale, and true streaming qualification (v0.134.0) |
| 0.132.x | ≥ 0.131.0 | Conformance evidence, feature and route truth checks, bounded predicate expansion, and keyset-paginated exports |
| 0.131.x | ≥ 0.130.0 | Secure-by-default HTTP configuration, central route authorization, PostgreSQL TLS, outbound policy, and trusted-proxy enforcement |
| 0.130.x | ≥ 0.129.0 | Installation/migration integrity: independent migration graph, schema fingerprints, and validated JSON writeback configuration API |
| 0.129.x | ≥ 0.128.0 | JSON writeback and mutation integrity; async queue coverage and typed direct writeback |
| 0.128.x | ≥ 0.127.0 | JSON mapping relational writeback API; breaking change: requires the v0.128 JSON writeback catalog and queue schema |
| 0.127.x | ≥ 0.126.0 | pg_tide relay bridge migration; breaking change: uses named outbox events via `tide.outbox_publish` and `relay_set_*_v2` APIs |
| 0.126.x | ≥ 0.125.0 | Federation credential-status and temporal snapshot APIs; breaking change: requires the v0.126 HTTP route and credential catalog additions |
| 0.125.x | ≥ 0.124.0 | Temporal graph snapshot and diff APIs; breaking change: requires the v0.125 snapshot catalog and metrics |
| 0.124.x | ≥ 0.123.0 | Rule-library observability and benchmark history APIs; breaking change: requires the v0.124 rule-library and benchmark catalog additions |
| 0.123.x | ≥ 0.122.0 | A17 observability & docs: replica pool Prometheus gauges (OBS-M-01), rule-library stream latency/error counters (OBS-M-02), `bench_workload_result()` SQL wrapper (ERG-L-01) |
| 0.122.x | ≥ 0.121.0 | A17 god-module decomposition & test coverage closure (H17-02); WatDiv correctness gating; pg_regress coverage for v0.119.0–v0.120.0 features |
| 0.121.x | ≥ 0.120.0 | A17 security hardening (H17-01 SSRF, SEC-M-03 CGNAT/multicast, BUG-M-01 silent degradation, OBS-L-01 mutation journal); mutation journal for rule-library ops |
| 0.120.x | ≥ 0.119.0 | Feature 11: rule-library federation (publish/subscribe over Arrow Flight); Feature 12: read-replica routing (`?replica=ok`); `compat_check()` JSON schema |
| 0.119.x | ≥ 0.118.0 | Feature 6: federation circuit breaker Prometheus gauges; Feature 7: `bench_workload()` BSBM/WatDiv/PageRank/PPRL profiles; Allen's interval relations |
| 0.118.x | ≥ 0.117.0 | Feature 4: diagnostic snapshot bundle (`/admin/diagnostic-snapshot`); Feature 5: tenant quota enforcement; multi-tenant knowledge graphs |
| 0.117.x | ≥ 0.116.0 | Feature 3: PPRL Bloom-filter encodes; neuro-symbolic entity resolution; LLM explanation cache; proof-tree generation |
| 0.116.x | ≥ 0.115.0 | A16 remediation arc: ER stage latency (M16-03), owl:sameAs assertion counter, Bayesian propagation latency, conflict detection counter; advisory lifecycle policy (M16-21) |
| 0.115.x | ≥ 0.114.0 | A15 remediation: `er_stage_duration_seconds` histogram labels (M16-03); PPRL/LLM/proof-tree observability; GNN link-prediction integration |
| 0.114.x | ≥ 0.113.0 | A15 High/Medium: `bidi_relay_dropped_total` counter (H15-03); merge/stratum/SHACL/CDC Prometheus metrics (M15-19); WatDiv 100-template suite |
| 0.113.x | ≥ 0.112.0 | UNNEST-array bulk load path; Citus object shard pruning; direct `COPY` bulk load; per-graph RLS; `explain_inference()` |
| 0.93.x | ≥ 0.92.0 | pg_tide integration: `pg_tide_available()` SQL function; BIDI relay comments updated to pg_tide API |
| 0.92.x | ≥ 0.88.0 | A14 Low polish: PageRank bounds, damping guide, SERVICE SILENT TLS, SOURCE_DATE_EPOCH, conformance badge CI |
| 0.91.x | ≥ 0.88.0 | OBS-01 PageRank IVM Prometheus gauges; HTTP-03 middleware extraction; HTTP-04 EXPLAIN row estimation; API-05 `explain_pagerank_json()`; CDC watermark batching GUCs |
| 0.90.x | ≥ 0.88.0 | A14 Medium correctness/performance: PageRank streaming, convergence-norm GUC, advisory lock, module splits |
| 0.89.x | ≥ 0.88.0 | A14 High remediation: `check_auth_write` on mutating handlers, GUC name audit, rate limit 100 req/s default |
| 0.88.x | ≥ 0.87.0 | Adds `pagerank_scores`, `centrality_scores`, `pagerank_dirty_edges`; requires `COMPATIBLE_EXTENSION_MIN = "0.87.0"` |
| 0.87.x | ≥ 0.86.0 | Adds `_pg_ripple.confidence`, `shacl_score_log`; probabilistic Datalog, fuzzy SPARQL filters, soft SHACL scoring |
| 0.86.x | ≥ 0.85.0 | D13-01 (v0.86.0): SSE streaming, graceful shutdown, new Prometheus metrics (O13-02), Arrow 413 guard (S13-08), CORS counter (S13-03), structured JSON logs (O13-04) |
| 0.85.x | ≥ 0.84.0 | COMPATIBLE_EXTENSION_MIN bumped to 0.84.0 (S13-05); strict compat mode (PG_RIPPLE_HTTP_STRICT_COMPAT); schema.rs/federation.rs splits; encode_batch GUC API |
| 0.84.x | ≥ 0.83.0 | OpenAPI spec served at /openapi; per-query-type metrics (METRICS-LABELS-01); /health/ready deep check (HEALTH-DEEP-01); SPARQL Update in HTTP |
| 0.83.x | ≥ 0.82.0 | CDC outbox bridge triggers, JSON-LD event serializer, vocabulary templates, pg-trickle detection |
| 0.82.x | ≥ 0.80.0 | SBOM (cargo-cyclonedx), structured error bodies (HTTP-ERR-01), Arrow ticket nonce cache (FLIGHT-NONCE-01), per-IP governor |
| 0.81.x | ≥ 0.80.0 | SHACL-SPARQL constraints, COPY rdf FROM, RAG hardening, CDC lifecycle events, OpenAPI spec |
| 0.80.x | ≥ 0.79.0 | SQL-injection parameterization (SQL-INJ-01), /explorer auth gate (EXPLORER-AUTH-01) |
| 0.76.x | ≥ 0.79.0 | COMPAT-MIN-01 (v0.80.0): requires `sparql_update_cursor()` (v0.76.0) and `feature_status()` wcoj/shacl_sparql entries (v0.79.0); `/explorer` now requires auth (EXPLORER-AUTH-01) |
| 0.75.x | ≥ 0.78.0 | Bidirectional integration operations (v0.78.0), SPARQL subscription SSE (v0.73.0) |
| 0.74.x | ≥ 0.77.0 | Bidirectional integration primitives (v0.77.0), SPARQL 1.2 tracking (v0.73.0) |
| 0.73.x | ≥ 0.76.0 | Fuzz hardening (v0.76.0), CONTRIBUTING.md (v0.73.0), Helm chart SHA pin |
| 0.16.x | ≥ 0.70.0 | First version with `Body::from_stream` Arrow Flight; compatibility check added (v0.71.0 COMPAT-01) |
| 0.15.x | 0.66.0 – 0.69.x | Arrow Flight security (FLIGHT-SEC-02), SPARQL cursor streaming (STREAM-01) |
| 0.14.x | 0.63.0 – 0.65.x | CONSTRUCT writeback rules (v0.63.0), Datalog REST API (v0.39.0) |
| 0.13.x | 0.57.0 – 0.62.x | OWL 2 EL/QL profiles, KGE embeddings, visual graph explorer |
| 0.12.x | 0.51.0 – 0.56.x | Non-root container, HTTP streaming, OTLP tracing |
| 0.11.x | 0.40.0 – 0.50.x | SPARQL cursors, explain/observability, OpenTelemetry |
| 0.10.x | 0.38.0 – 0.39.x | Module restructuring, all 27 Datalog SQL functions |
| 0.9.x | 0.33.0 – 0.37.x | Docs site rebuild, parallel Datalog, HTAP stability |
| ≤ 0.8.x | 0.15.0 – 0.32.x | HTTP endpoint, bulk-load, basic SPARQL |


## Startup version check

Starting with `pg_ripple_http` 0.16.0, the HTTP companion performs a compatibility check at
startup. It queries the installed extension version and compares it against its known-compatible
range. If the extension is older than the minimum supported version:

- **Warning** is logged: the companion starts but logs a prominent warning.
- The `GET /ready` endpoint returns HTTP 503 with `{"compatible": false, ...}` if the extension
  is below the hard minimum.

The check can be disabled with `PG_RIPPLE_HTTP_SKIP_COMPAT_CHECK=1` for testing scenarios where
an older extension is intentionally paired with a newer companion.

> **⚠ Production warning (COMPAT-DOC-01 / MF-R):**
> `PG_RIPPLE_HTTP_SKIP_COMPAT_CHECK=1` is intended **only for testing and development** where
> you deliberately need to run a mismatched pair (e.g., integration tests against an older
> extension). **Do not set this in production environments.** Skipping the check allows the
> HTTP companion to serve requests to an incompatible extension, which can result in silent data
> corruption, unexpected errors, or security vulnerabilities when new SQL functions are called
> against an older extension schema. If you need to silence the compatibility warning in
> production, upgrade the extension or the companion to a compatible version pair instead.

## Independent versioning rationale

The HTTP companion is distributed as a pre-built binary. Extension upgrades (`ALTER EXTENSION
pg_ripple UPDATE`) are applied in-database and do not require rebuilding or redeploying the
companion. This means:

1. A single `pg_ripple_http` binary can serve multiple extension versions within its compatible range.
2. Extension-only changes (new SQL functions, GUCs, performance improvements) do not require a
   companion update.
3. Breaking API changes (new required request fields, removed endpoints) do require a companion
   update.

## Upgrade procedure

1. Upgrade the extension first: `ALTER EXTENSION pg_ripple UPDATE TO 'X.Y.Z';`
2. Restart or redeploy `pg_ripple_http` if a companion upgrade is also required.
3. Verify compatibility via `GET /ready` — returns `{"compatible": true}` when correctly paired.

See also: [Arrow Flight Reference](../reference/arrow-flight.md), [HTTP API](../reference/http-api.md).

## Pre-GA upgrade support policy

Until v1.0.0, CI runs the complete historical sequential migration chain. Release qualification
also tests direct upgrades from the six most recent published pre-GA release artifacts. After
v1.0.0, each minor release supports direct upgrades from the two
previous minor releases and sequential upgrades from older versions that remain in the migration
graph.

## pg_tide / pg_trickle Extension Compatibility

pg_ripple integrates with two companion PostgreSQL extensions:

| Extension | Purpose | Required for |
|-----------|---------|--------------|
| pg_trickle ≥ 0.46.0 | Incremental materialized view maintenance (IVM only) | SPARQL views, Datalog views, CONSTRUCT/DESCRIBE/ASK views, ExtVP |
| pg_tide ≥ 0.33.0 | Relay, outbox, and inbox subsystem | Bidirectional relay (BIDI-OUTBOX-01, BIDI-INBOX-01), hub-and-spoke integration |

> **Architecture change (pg_ripple v0.93.0 / pg_trickle v0.46.0)**: pg_trickle v0.46.0 extracted
> the relay, outbox, and inbox subsystem (~6,150 Rust LOC + ~2,500 SQL LOC) into the new standalone
> `pg_tide` extension (`trickle-labs/pg-tide`). Starting with v0.93.0, pg_ripple recognises
> pg_tide as the recommended relay transport layer.

### pg_ripple + pg_trickle compatibility

| pg_ripple version | pg_trickle version | IVM status |
|-------------------|--------------------|------------|
| ≥ 0.112.0 | ≥ 0.57.0 | ✅ IVM only (relay features in pg_tide) |
| ≥ 0.93.0 | ≥ 0.46.0 | ✅ IVM only (relay features in pg_tide) |
| ≥ 0.93.0 | ≤ 0.45.0 | ⚠ IVM works; relay features require manual migration to pg_tide |
| 0.52.0 – 0.92.0 | any | ✅ Full relay+IVM (pre-extraction) |

### pg_ripple + pg_tide compatibility

| pg_ripple version | pg_tide version | Relay status |
|-------------------|-----------------|--------------|
| ≥ 0.127.0 | ≥ 0.33.0 | ✅ Full relay support with named outbox publishing via `tide.outbox_publish()` and stable `relay_set_*_v2` configuration APIs |
| 0.93.0 – 0.126.x | ≥ 0.4.0 | ✅ Relay API detection and docs; CDC bridge users should upgrade to 0.127.0 for current named-outbox publish semantics |
| ≥ 0.93.0 | 0.1.0 – 0.3.x | ✅ Core relay support (tide.* API, older feature set) |
| ≥ 0.93.0 | not installed | ⚠ Core pg_ripple + IVM work; bidirectional relay unavailable |
| < 0.93.0 | any | pg_tide not yet supported (use pg_trickle ≤ 0.45.0 for relay) |

### Recommended stack (pg_ripple ≥ 0.127.0)

```sql
CREATE EXTENSION pg_tide;      -- relay, outbox, inbox (trickle-labs/pg-tide ≥ 0.33.0)
CREATE EXTENSION pg_trickle;   -- IVM (trickle-labs/pg-trickle ≥ 0.46.0)
CREATE EXTENSION pg_ripple;    -- RDF triple store (≥ 0.127.0)
```

Call `pg_ripple.pg_tide_available()` to verify pg_tide is installed at runtime.
Call `pg_ripple.pg_trickle_available()` to verify pg_trickle is installed at runtime.
