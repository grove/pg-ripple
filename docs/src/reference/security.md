# Security

## Supported versions

| Version | Security support |
|---|---|
| 0.14.x | ✅ Active |
| 0.13.x | ✅ Active |
| 0.12.x | ✅ Active |
| 0.11.x | ⚠️ Best-effort |
| 0.10.x | ⚠️ Best-effort |
| < 0.10 | ❌ End of life |

We recommend running the latest release.

## Reporting a vulnerability

To report a security vulnerability, please email the maintainers via the GitHub repository's security advisory page (`Security → Report a vulnerability`). Do not open a public GitHub issue for security reports.

We aim to acknowledge reports within 48 hours and provide a fix within 30 days for critical issues.

## SQL injection prevention

All SPARQL query parameters are encoded to `BIGINT` dictionary IDs before any SQL is generated. User-supplied IRI and literal strings never appear in generated SQL query text. The SPARQL→SQL translation pipeline is designed to be injection-safe by construction.

The dictionary encoding layer strips angle brackets from IRI notation and applies XXH3-128 hashing. The resulting integer is what goes into all generated SQL. No concatenation of user strings into SQL ever occurs in the query path.

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

There are no known vulnerabilities in pg_ripple 0.14.0 at the time of release.

