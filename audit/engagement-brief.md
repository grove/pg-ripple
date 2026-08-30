# v0.136.0 external security audit engagement brief

This brief scopes the independent audit required for the v0.136.0 hardened
candidate and the v1.0.0 release dossier.

## Scope

- Rust extension code under `src/`, including the frozen v0.135.0 binding and
  prefix implementation.
- HTTP companion code under `pg_ripple_http/src/`, including streaming,
  cancellation, authentication, authorization, and pool cleanup.
- SQL migrations, release evidence, Docker images, Compose, and Helm chart.

## Methodology

The assessment should cover OWASP ASVS level 2, the PostgreSQL extension
attack surface, SSRF, injection, authentication bypass, privilege escalation,
resource exhaustion, binding parameterization, prefix privilege boundaries,
and cache invalidation. Reviewers should exercise the migration, conformance,
fuzz, and release-evidence workflows as part of the reproducibility check.

## Candidates and timing

Trail of Bits, Cure53, and NCC Group are candidate firms. The audit starts
after the exact v0.136.0 candidate commit and must identify the commit and
artifacts it assessed in its report.

## Deliverables

1. Raw findings with severity, reproduction steps, and affected versions.
2. Remediation confirmation and regression evidence for every finding.
3. A signed or otherwise verifiable report or attestation naming the exact
   candidate commit and build artifacts.
4. A public summary suitable for the v1.0.0 release dossier.

The audit is an external release gate. The repository's v0.136.0 preflight
checks must pass before artifacts are handed to the auditor, but they do not
replace independent review.
