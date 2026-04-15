# Security

## Supported versions

| Version | Security support |
|---|---|
| 0.5.x | ✅ Active |
| 0.4.x | ✅ Active |
| 0.3.x | ✅ Active |
| 0.2.x | ⚠️ Best-effort |
| 0.1.x | ⚠️ Best-effort |

We recommend running the latest release.

## Reporting a vulnerability

To report a security vulnerability, please email the maintainers at the address listed in the GitHub repository's security advisory page (`Security → Report a vulnerability`). Do not open a public GitHub issue for security reports.

We aim to acknowledge reports within 48 hours and provide a fix within 30 days for critical issues.

## SQL injection prevention

All SPARQL query parameters are encoded to `BIGINT` dictionary IDs before any SQL is generated. User-supplied IRI and literal strings never appear in generated SQL query text. The SPARQL→SQL translation pipeline is designed to be injection-safe.

## No known vulnerabilities

There are no known vulnerabilities in pg_ripple 0.5.0 at the time of release.
