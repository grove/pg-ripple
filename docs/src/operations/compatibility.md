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
