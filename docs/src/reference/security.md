# Security

## Supported versions

| Version | Security support |
|---|---|
| 0.20.x | ✅ Active |
| 0.19.x | ✅ Active |
| 0.18.x | ✅ Active |
| 0.17.x | ⚠️ Best-effort |
| 0.16.x | ⚠️ Best-effort |
| < 0.16 | ❌ End of life |

We recommend running the latest release.

## Reporting a vulnerability

To report a security vulnerability, please email the maintainers via the GitHub repository's security advisory page (`Security → Report a vulnerability`). Do not open a public GitHub issue for security reports.

We aim to acknowledge reports within 48 hours and provide a fix within 30 days for critical issues.

---

## Security Review Phase 1 (v0.20.0)

v0.20.0 completed a Phase 1 security review covering three areas: SPI query generation, shared memory safety, and dictionary cache timing side-channels.

### SPI injection audit

**Finding: No SQL injection vectors found.**

All SPARQL query parameters are encoded to `BIGINT` dictionary IDs before any SQL is generated. User-supplied IRI and literal strings never appear in generated SQL query text.

Audited files: `src/sparql/sqlgen.rs`, `src/datalog/compiler.rs`

Mitigations in place:

| Pattern | Mitigation |
|---|---|
| IRI constants in FILTER | `encode_term()` called at translation time; integer ID emitted into SQL |
| Literal constants (string, numeric, typed) | Same: `encode_term()` at translation time |
| Predicate table names | OID looked up from `_pg_ripple.predicates.table_oid`; never string-concatenated |
| Datalog rule heads/bodies | All terms encoded before SQL is compiled; variable names never interpolated |
| Named graph IRIs in UPDATE | Encoded via `encode_graph()` before SQL generation |

**Checklist (all items: mitigated)**

- [x] No `format!()` with user-supplied strings in `sqlgen.rs`
- [x] No `format!()` with user-supplied strings in `datalog/compiler.rs`
- [x] All IRI/literal constants pass through `encode_term()` before SQL
- [x] Table name references use OID from `_pg_ripple.predicates`
- [x] Property path SQL uses parameterised CTEs, not string interpolation

### Shared memory safety audit

**Finding: No data races, bounds violations, or use-after-free found.**

Audited file: `src/shmem.rs` and all `pgrx::PgSharedMem` call sites.

| Risk | Status |
|---|---|
| Concurrent dictionary cache reads without lock | Mitigated — LRU cache access wrapped in `PgLwLock` |
| Shared plan cache reads | Mitigated — lock-free read via atomic pointer; writer holds exclusive lock |
| Background merge worker accessing shmem after restart | Mitigated — worker re-attaches after `_PG_init` on startup |
| Stale pointer after shmem recreation (DROP EXTENSION + re-create) | Mitigated — `PgSharedMem` is invalidated on extension drop; re-attach forced by `_PG_init` |

**Checklist (all items: mitigated)**

- [x] All `PgSharedMem` writes hold the appropriate `PgLwLock` exclusive lock
- [x] No raw pointer aliasing in `src/shmem.rs`
- [x] Background worker uses `BackgroundWorker::attach_signal_handlers()` before touching shmem
- [x] No `unsafe` block in shmem code without a `// SAFETY:` comment

### Dictionary cache timing side-channel analysis

**Finding: Low risk; no sensitive metadata leaked.**

The dictionary cache is an LRU keyed on (value, kind). Cache hit/miss latency difference is approximately 50–500 ns depending on cache size. This latency difference does not leak:

- The size of the dictionary (triple count is visible via `pg_ripple.triple_count()` anyway)
- IRI patterns stored by other users (graph-level RLS controls row visibility before dictionary lookup is reached for query results)
- Literal content (encode/decode is called by the authenticated session only)

Recommendation for high-security deployments: enable graph-level RLS (`pg_ripple.enable_graph_rls()`) to ensure that cross-user IRI lookups do not occur.

---

## SQL injection prevention

All SPARQL query parameters are encoded to `BIGINT` dictionary IDs before any SQL is generated. User-supplied IRI and literal strings never appear in generated SQL query text. The SPARQL→SQL translation pipeline is designed to be injection-safe by construction.

The dictionary encoding layer strips angle brackets from IRI notation and applies XXH3-128 hashing. The resulting integer is what goes into all generated SQL. No concatenation of user strings into SQL ever occurs in the query path.

See the [Security Review Phase 1](#security-review-phase-1-v0200) section above for the full audit checklist.

## Graph-level access control (v0.14.0)

pg_ripple supports graph-level Row-Level Security (RLS) from v0.14.0 onwards. When enabled:

- Named graph triples are only visible to roles listed in `_pg_ripple.graph_access` with `'read'` or higher permission.
- Write operations to a named graph require `'write'` or `'admin'` permission.
- The default graph (g = 0) is always accessible to all roles.
- Superusers can set `SET pg_ripple.rls_bypass = on` to bypass policies.

Enable RLS with:

```sql
SELECT pg_ripple.enable_graph_rls();
SELECT pg_ripple.grant_graph('app_user', '<https://example.org/confidential>', 'read');
```

## Hardening GUCs

| GUC | Recommended value | Notes |
|---|---|---|
| `pg_ripple.rls_bypass` | `off` (default) | Ensure superusers do not leave this on in production sessions |
| `pg_ripple.shacl_mode` | `'sync'` or `'async'` | Enable constraint validation to prevent invalid data |
| `pg_ripple.enforce_constraints` | `'error'` | Reject transactions that violate Datalog constraint rules |
| `pg_ripple.merge_threshold` | Match your write rate | Prevents unbounded delta growth |

## No known vulnerabilities

There are no known vulnerabilities in pg_ripple at the time of this release.

---

## Security Review Phase 2 (v0.22.0)

v0.22.0 completed a Phase 2 security review covering the HTTP companion service, the privilege model, and authentication hardening.

### Rate limiting (pg_ripple_http)

The HTTP companion service now enforces per-source-IP rate limiting using the `tower_governor` crate.

**Configuration:**

Set the `PG_RIPPLE_HTTP_RATE_LIMIT` environment variable to the maximum requests per second allowed from a single IP address (default `0` = unlimited):

```bash
PG_RIPPLE_HTTP_RATE_LIMIT=100 pg_ripple_http
```

When the limit is exceeded, the service returns:

```
HTTP/1.1 429 Too Many Requests
Retry-After: 1
```

A value of `0` disables rate limiting entirely (suitable for trusted internal deployments behind a gateway that handles rate limiting upstream).

### Error redaction policy

All 4xx and 5xx error responses from `pg_ripple_http` are now redacted. Internal database details — schema names, GUC values, file paths, and PostgreSQL error messages — are never returned to API clients.

Instead, clients receive a structured JSON error with a category and a trace ID:

```json
{"error": "sparql_query_error", "trace_id": "550e8400-e29b-41d4-a716-446655440000"}
```

The full error and trace ID are logged at `ERROR` level on the server side. Use the `trace_id` to correlate client-reported errors with server logs.

**Error categories:**

| Category | HTTP Status | Meaning |
|---|---|---|
| `sparql_query_error` | 400 | SELECT/ASK/CONSTRUCT/DESCRIBE query failed |
| `sparql_update_error` | 400 | SPARQL UPDATE failed |
| `service_unavailable` | 503 | Database connection pool unavailable |
| `database_unavailable` | 503 | Health check query failed |

### Constant-time authentication

The `PG_RIPPLE_HTTP_AUTH_TOKEN` bearer token is now compared using `constant_time_eq` from the `constant_time_eq` crate, preventing timing-based side-channel attacks that could leak token length or content via response time differences.

### Federation URL scheme enforcement

`pg_ripple.register_endpoint()` now rejects any URL whose scheme is not `http` or `https` with `ERRCODE_INVALID_PARAMETER_VALUE`:

```sql
SELECT pg_ripple.register_endpoint('file:///etc/passwd');
-- ERROR: register_endpoint: URL scheme must be http or https
```

This prevents registration of `file://`, `gopher://`, or other schemes that could be used for server-side request forgery even though `ureq` would reject them at connection time.

### Privilege model hardening

The `_pg_ripple` internal schema is now explicitly locked away from unprivileged roles. The migration script `pg_ripple--0.21.0--0.22.0.sql` revokes all access:

```sql
REVOKE ALL ON SCHEMA _pg_ripple FROM PUBLIC;
REVOKE ALL ON ALL TABLES IN SCHEMA _pg_ripple FROM PUBLIC;
REVOKE ALL ON ALL SEQUENCES IN SCHEMA _pg_ripple FROM PUBLIC;
```

Unprivileged roles cannot read `_pg_ripple.dictionary`, `_pg_ripple.vp_*`, or any internal table directly. The public `pg_ripple.*` API functions run as the invoking role (`SECURITY INVOKER`, which is the PostgreSQL default). The only exception is the DDL event-trigger guard function `_pg_ripple.ddl_guard_vp_tables()`, which uses `SECURITY DEFINER` because it must inspect `pg_event_trigger_dropped_objects()` — a system function that requires elevated privilege. No other function in the extension uses `SECURITY DEFINER`. The CI lint step `scripts/check_no_security_definer.sh` enforces this invariant on every commit.

**Checklist (all items: mitigated)**

- [x] Rate limiting enforced per source IP — excess returns `429` with `Retry-After`
- [x] Error responses never expose internal database details
- [x] Auth token compared with `constant_time_eq` (timing-safe)
- [x] `register_endpoint()` rejects non-http/https URL schemes
- [x] `_pg_ripple` schema inaccessible to `PUBLIC`
- [x] `load_*_file()` validates canonical path against data directory (v0.25.0)

---

## Security Review Phase 2 (v0.25.0)

v0.25.0 added two further hardening measures.

### File-path bulk loader validation (S-8)

All `load_*_file()` functions (`load_turtle_file`, `load_ntriples_file`, `load_nquads_file`, `load_trig_file`, `load_rdfxml_file`) now validate that the resolved (canonical, symlink-followed) path lies within the PostgreSQL data directory before reading the file.

This prevents a superuser from accidentally loading files outside the cluster directory using symlink tricks:

```sql
-- Rejected: /etc/passwd is outside the data directory
SELECT pg_ripple.load_turtle_file('/etc/passwd');
-- ERROR: permission denied: "/etc/passwd" is outside the database cluster directory
```

The implementation calls `std::fs::canonicalize()` to resolve symlinks, then checks that the result starts with `current_setting('data_directory')`. This matches the access model used by PostgreSQL's own `COPY FROM FILE`.

### Federation cache key upgrade (H-12)

The `_pg_ripple.federation_cache.query_hash` column was upgraded from `BIGINT` (XXH3-64) to `TEXT` (32-char hex XXH3-128 fingerprint). The 64-bit hash had a birthday bound of approximately 2.1 billion distinct cached queries before a 50% collision probability, which is thin for a long-running server with a large query workload. The 128-bit hash makes collision negligible at any practical query volume.

The migration script truncates the cache table before changing the column type — cache rows are ephemeral and can be safely discarded.


### LLM API key security (H-2, v0.55.0)

The `pg_ripple.llm_api_key_env` GUC accepts the **name** of an environment variable that holds the LLM API key — it does **not** accept the key value itself.

**Correct usage:**
```sql
-- Set the env var name; the key lives in the environment
SET pg_ripple.llm_api_key_env = 'MY_LLM_API_KEY';
-- Then set the env var in your systemd unit or docker-compose:
-- MY_LLM_API_KEY=sk-...
```

**Wrong (and warned):**
```sql
-- This emits a WARNING because the value looks like a raw API key
SET pg_ripple.llm_api_key_env = 'sk-abc123...';
-- WARNING: pg_ripple.llm_api_key_env looks like a raw API key, not an
-- environment variable name. Set it to the NAME of an env var ...
```

Storing API keys directly in GUCs is insecure because:
- They appear in `pg_settings` (visible to any user with `SHOW` privilege)
- They appear in PostgreSQL error logs and `pg_stat_activity`
- They may be included in backups and `pg_dump` output

Use environment variables, a secrets manager (HashiCorp Vault, AWS Secrets Manager), or
PostgreSQL's `ALTER SYSTEM SET ... IN VAULT` integration to keep API keys out of the database
server configuration.
