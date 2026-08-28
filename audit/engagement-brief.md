# External security audit engagement brief

This brief scopes the v1.0.0 external security audit planned after v0.132.0.

## Scope

- Rust extension code under `src/`.
- HTTP companion code under `pg_ripple_http/src/`.
- SQL migrations, Docker images, and Helm chart.

## Methodology

The assessment should cover OWASP ASVS level 2, the PostgreSQL extension
attack surface, SSRF, injection, authentication bypass, and privilege
escalation. Reviewers should exercise the migration, conformance, and release
evidence workflows as part of the reproducibility check.

## Candidates and timing

Trail of Bits, Cure53, and NCC Group are candidate firms. The audit should
start within four weeks of the v0.132.0 tag and deliver its report within
eight weeks after starting.

## Deliverables

1. Raw findings with severity, reproduction steps, and affected versions.
2. Remediation confirmation for every finding.
3. A public summary suitable for the v1.0.0 release dossier.
