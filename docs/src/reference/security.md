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

There are no known vulnerabilities in pg_ripple 0.20.0 at the time of release.

Phase 2 of the security review (external penetration testing, fuzzing the SPARQL parser, and network-layer analysis of the HTTP endpoint) is planned for v0.21.0.

