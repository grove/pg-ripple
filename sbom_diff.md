# SBOM Diff: v0.60.0 → v0.61.0

**Generated:** 2025  
**Previous version:** v0.60.0  
**Current version:** v0.61.0

## Summary

| Category | Count |
|---|---|
| Packages added | 0 |
| Packages removed | 0 |
| Packages updated | 0 |
| Packages unchanged | — |

## Changes

No new Rust crate dependencies were added in v0.61.0. All v0.61.0 features were
implemented using existing crates already present in the dependency graph:

- `spargebra` — SPARQL algebra types (existing)
- `oxrdf` — RDF graph model (existing)
- `pgrx` — PostgreSQL Rust bindings (existing)
- `axum` — HTTP service (existing)
- `serde_json` — JSON serialization (existing)

The `dbt-pg-ripple` Python package (`clients/dbt-pg-ripple/`) is a new
deliverable but is a separate distribution artifact (PyPI package) not
included in the Rust SBOM.

## Full SBOM

See [sbom.json](sbom.json) for the complete CycloneDX SBOM.

## Verification

```bash
# Regenerate the SBOM
cargo cyclonedx --format json --output sbom.json

# Diff against previous release SBOM (if available)
# diff <(jq -r '.components[].name' sbom_v0.60.0.json | sort) \
#      <(jq -r '.components[].name' sbom.json | sort)
```
