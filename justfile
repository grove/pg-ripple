# pg_ripple — project commands
# https://github.com/casey/just

set dotenv-load := false

# Default PostgreSQL major version
pg := "18"

# Default database for benchmarks
db := "postgres"

# List available recipes
[group: "help"]
default:
    @just --list --unsorted

# ── Build ─────────────────────────────────────────────────────────────────

# Compile the extension (debug)
[group: "build"]
build:
    cargo build --features pg{{pg}}

# Compile the extension (release)
[group: "build"]
build-release:
    cargo build --release --features pg{{pg}}

# ── Lint & Format ─────────────────────────────────────────────────────────

# Format source code
[group: "lint"]
fmt:
    cargo fmt

# Check formatting only (no files changed)
[group: "lint"]
fmt-check:
    cargo fmt -- --check

# Lint with clippy (warnings as errors)
[group: "lint"]
clippy:
    cargo clippy --all-targets --features pg{{pg}} -- -D warnings

# Check formatting and run clippy
[group: "lint"]
lint: fmt-check clippy

# ── Tests ─────────────────────────────────────────────────────────────────

# Run tests via pgrx against a pgrx-managed postgres
[group: "test"]
test:
    cargo pgrx test pg{{pg}}

# Run pgrx regression tests
[group: "test"]
test-regress:
    cargo pgrx regress pg{{pg}} --postgresql-conf "allow_system_table_mods=on"

# Verify all migration SQL scripts apply cleanly in sequence (pgrx pg18 must be running)
[group: "test"]
test-migration:
    bash tests/test_migration_chain.sh

# Run all tests (unit + pgrx + regress + migration chain)
[group: "test"]
test-all: test test-regress test-migration

# ── Development ───────────────────────────────────────────────────────────

# Start a pgrx-managed PostgreSQL instance
[group: "dev"]
start:
    cargo pgrx start pg{{pg}}

# Stop the pgrx-managed PostgreSQL instance
[group: "dev"]
stop:
    cargo pgrx stop pg{{pg}}

# Install the extension into the running pgrx instance
[group: "dev"]
install:
    cargo pgrx install --pg-config /opt/homebrew/bin/pg_config-18 && \
        install_name_tool -id "$(/opt/homebrew/bin/pg_config-18 --pkglibdir)/pg_ripple.dylib" \
            "$(/opt/homebrew/bin/pg_config-18 --pkglibdir)/pg_ripple.dylib"

# ── Benchmarks ────────────────────────────────────────────────────────────

# Load BSBM data (override db via: just db=mydb bench-bsbm-load)
[group: "bench"]
bench-bsbm-load scale="1":
    BSBM_SCALE={{scale}} envsubst '$BSBM_SCALE' < benchmarks/bsbm/bsbm_load.sql | psql -h /tmp -p 5432 -d {{db}}

# Run BSBM query mix (12 standard BSBM queries)
[group: "bench"]
bench-bsbm-queries:
    psql -h /tmp -p 5432 -d {{db}} -f benchmarks/bsbm/bsbm_queries.sql

# Run BSBM HTAP concurrent workload (insert + query under load)
[group: "bench"]
bench-bsbm-htap:
    psql -h /tmp -p 5432 -d {{db}} -f benchmarks/bsbm/bsbm_htap.sql

# Run pgbench BSBM sustained throughput test
[group: "bench"]
bench-bsbm-pgbench duration="60" clients="10" jobs="4":
    pgbench -h /tmp -p 5432 -d {{db}} -f benchmarks/bsbm/bsbm_pgbench.sql -T {{duration}} -c {{clients}} -j {{jobs}}

# Run all BSBM benchmarks in sequence (load → queries → HTAP → pgbench)
[group: "bench"]
bench-bsbm-all scale="1" duration="60" clients="10" jobs="4": (bench-bsbm-load scale) bench-bsbm-queries bench-bsbm-htap (bench-bsbm-pgbench duration clients jobs)

# Run BSBM at 100M-triple scale (scale=30 ≈ 100M triples; runs for hours — use nightly CI)
# Results are written to /tmp/pg_ripple_bsbm_100m_results.txt
[group: "bench"]
bench-bsbm-100m db="pg_ripple_bsbm100m": (bench-bsbm-load "30")
    psql -h /tmp -p 5432 -d {{db}} -c "SELECT pg_ripple.triple_count() AS total_triples;" | tee /tmp/pg_ripple_bsbm_100m_results.txt
    psql -h /tmp -p 5432 -d {{db}} -f benchmarks/bsbm/bsbm_queries.sql 2>&1 | tee -a /tmp/pg_ripple_bsbm_100m_results.txt
    @echo "BSBM 100M results written to /tmp/pg_ripple_bsbm_100m_results.txt"

# ── Crash Recovery ────────────────────────────────────────────────────────

# Run the crash-recovery test suite (nightly; requires cargo pgrx start pg18)
[group: "test"]
test-crash-recovery:
    bash tests/crash_recovery/merge_during_kill.sh
    bash tests/crash_recovery/dict_during_kill.sh
    bash tests/crash_recovery/shacl_during_violation.sh

# ── Memory Leak Detection ─────────────────────────────────────────────────

# Run a curated subset of unit tests under Valgrind to detect heap leaks.
# Requires: valgrind installed; a locally-installed pg18 (not pgrx-managed).
# Timeout: up to 2 hours for the full suite.
[group: "test"]
test-valgrind:
    @echo "Running Valgrind leak check on curated unit test subset..."
    @echo "This may take up to 2 hours. Log: /tmp/pg_ripple_valgrind.log"
    valgrind \
        --leak-check=full \
        --show-leak-kinds=definite \
        --error-exitcode=1 \
        --log-file=/tmp/pg_ripple_valgrind.log \
        cargo pgrx test pg{{pg}} -- --test-filter "dict::tests" 2>&1 | tail -20
    @grep -E "definitely lost: 0|no leaks" /tmp/pg_ripple_valgrind.log && \
        echo "Valgrind: no definite leaks found" || \
        (echo "Valgrind: definite leaks detected — see /tmp/pg_ripple_valgrind.log"; exit 1)

# ── Docker ────────────────────────────────────────────────────────────────

# Build the Docker image locally
[group: "docker"]
docker-build tag="local":
    docker build -t pg-ripple:{{tag}} .

# Run the sandbox container (default postgres password: ripple)
[group: "docker"]
docker-run tag="local":
    docker run --rm -p 5432:5432 -e POSTGRES_PASSWORD=ripple pg-ripple:{{tag}}

# Build then run in one step
[group: "docker"]
docker tag="local": (docker-build tag) (docker-run tag)

# ── Documentation ─────────────────────────────────────────────────────────

# Serve the documentation site locally via mdBook (opens browser)
[group: "dev"]
docs-serve:
    mdbook serve docs --open

# ── Release ───────────────────────────────────────────────────────────────

# Prepare a new release: bump version in Cargo.toml and pg_ripple.control,
# then remind you to create a migration script.
#
# Usage:  just release 0.52.0
[group: "release"]
release VERSION:
    @echo "=== Preparing release v{{VERSION}} ==="
    sed -i '' 's/^version = "[0-9.]*"/version = "{{VERSION}}"/' Cargo.toml
    sed -i '' "s/^default_version = '[0-9.]*'/default_version = '{{VERSION}}'/" pg_ripple.control
    @echo "Bumped Cargo.toml and pg_ripple.control to {{VERSION}}"
    @echo ""
    @echo "Next steps:"
    @echo "  1. Create sql/pg_ripple--PREV--{{VERSION}}.sql"
    @echo "  2. Update CHANGELOG.md"
    @echo "  3. git add -A && git commit -m 'v{{VERSION}}: prepare release'"
    @echo "  4. git tag v{{VERSION}} && git push --tags"

# BUILD-02 (v0.84.0): Bump all version strings atomically.
# Updates Cargo.toml (root + pg_ripple_http), pg_ripple.control,
# COMPATIBLE_EXTENSION_MIN in pg_ripple_http/src/main.rs,
# docker-compose.yml image tag, creates a stub migration script,
# and appends a CHANGELOG stub for the new version.
#
# Usage:  just bump-version 0.85.0
[group: "release"]
bump-version NEW_VERSION:
    @OLD_VERSION=$(grep '^version = ' Cargo.toml | head -1 | grep -oP '"\\K[^"]+'); \
     echo "Bumping $$OLD_VERSION → {{NEW_VERSION}}"; \
     sed -i '' "s/^version = \"$$OLD_VERSION\"/version = \"{{NEW_VERSION}}\"/" Cargo.toml; \
     sed -i '' "s/^version = \"$$OLD_VERSION\"/version = \"{{NEW_VERSION}}\"/" pg_ripple_http/Cargo.toml; \
     sed -i '' "s/^default_version = '$$OLD_VERSION'/default_version = '{{NEW_VERSION}}'/" pg_ripple.control; \
     sed -i '' "s/const COMPATIBLE_EXTENSION_MIN: &str = \"$$OLD_VERSION\"/const COMPATIBLE_EXTENSION_MIN: \&str = \"{{NEW_VERSION}}\"/" pg_ripple_http/src/main.rs; \
     sed -i '' "s|ghcr.io/grove/pg_ripple:$$OLD_VERSION|ghcr.io/grove/pg_ripple:{{NEW_VERSION}}|g" docker-compose.yml; \
     MIGRATION_FILE="sql/pg_ripple--$$OLD_VERSION--{{NEW_VERSION}}.sql"; \
     if [ ! -f "$$MIGRATION_FILE" ]; then \
       echo "-- Migration $$OLD_VERSION → {{NEW_VERSION}}" > "$$MIGRATION_FILE"; \
       echo "-- Schema changes: TODO" >> "$$MIGRATION_FILE"; \
       echo "Created $$MIGRATION_FILE"; \
     else \
       echo "$$MIGRATION_FILE already exists"; \
     fi; \
     echo ""; \
     echo "=== Version bump complete ==="; \
     echo "Next: update CHANGELOG.md and sql/$$MIGRATION_FILE"

# BUILD-02 (v0.84.0): Regenerate sbom.json using cargo-cyclonedx.
# Requires cargo install cargo-cyclonedx.
[group: "release"]
regen-sbom:
    cargo cyclonedx --format json
    @echo "sbom.json regenerated. Review changes with: git diff sbom.json"

# BUILD-02 (v0.84.0): Check that all version strings are consistent.
# Verifies: Cargo.toml root, pg_ripple_http/Cargo.toml, pg_ripple.control,
# COMPATIBLE_EXTENSION_MIN in pg_ripple_http/src/main.rs, docker-compose.yml.
[group: "release"]
check-version-sync:
    @CARGO_VER=$(grep '^version = ' Cargo.toml | head -1 | grep -oP '"\\K[^"]+'); \
     HTTP_VER=$(grep '^version = ' pg_ripple_http/Cargo.toml | head -1 | grep -oP '"\\K[^"]+'); \
     CTRL_VER=$(grep '^default_version' pg_ripple.control | grep -oP "'\\K[^']+"); \
     COMPAT_VER=$(grep 'COMPATIBLE_EXTENSION_MIN' pg_ripple_http/src/main.rs | grep -oP '"\\K[^"]+' | head -1); \
     DC_VER=$(grep 'ghcr.io/grove/pg_ripple:' docker-compose.yml | grep -oP ':\\K[0-9.]+' | head -1); \
     FAIL=0; \
     echo "Cargo.toml:         $$CARGO_VER"; \
     echo "pg_ripple_http:     $$HTTP_VER"; \
     echo "pg_ripple.control:  $$CTRL_VER"; \
     echo "COMPAT_EXTENSION_MIN: $$COMPAT_VER"; \
     echo "docker-compose.yml: $$DC_VER"; \
     [ "$$CARGO_VER" = "$$HTTP_VER" ]   || { echo "FAIL: pg_ripple_http version mismatch"; FAIL=1; }; \
     [ "$$CARGO_VER" = "$$CTRL_VER" ]   || { echo "FAIL: pg_ripple.control version mismatch"; FAIL=1; }; \
     [ "$$CARGO_VER" = "$$COMPAT_VER" ] || { echo "FAIL: COMPATIBLE_EXTENSION_MIN mismatch"; FAIL=1; }; \
     [ "$$CARGO_VER" = "$$DC_VER" ]     || { echo "FAIL: docker-compose.yml image tag mismatch"; FAIL=1; }; \
     if [ $$FAIL -eq 0 ]; then echo "OK: all versions consistent at $$CARGO_VER"; fi; \
     exit $$FAIL

# BUILD-02 (v0.84.0): Regenerate the OpenAPI spec from the running HTTP service.
# Requires pg_ripple_http to be running on $PG_RIPPLE_HTTP_URL (default: http://localhost:3000).
[group: "release"]
regen-openapi:
    @URL=$${PG_RIPPLE_HTTP_URL:-http://localhost:3000}; \
     echo "Fetching OpenAPI spec from $$URL/openapi.json"; \
     curl -fsSL "$$URL/openapi.json" -o pg_ripple_http/openapi.json && \
     echo "Saved to pg_ripple_http/openapi.json"; \
     if command -v yq >/dev/null 2>&1; then \
       yq -P pg_ripple_http/openapi.json > pg_ripple_http/openapi.yaml && \
       echo "Converted to pg_ripple_http/openapi.yaml"; \
     else \
       echo "(Skipping YAML: yq not installed)"; \
     fi

# ── Release Quality Gate ──────────────────────────────────────────────────

# SBOM-03: Verify that sbom.json version matches Cargo.toml version.
# Fails with exit 1 if they differ (prevents stale SBOM in releases).
[group: "release"]
check-sbom-version:
    @CARGO_VER=$(grep '^version = ' Cargo.toml | head -1 | grep -oP '"\\K[^"]+'); \
     SBOM_VER=$(python3 -c "import json; print(json.load(open('sbom.json'))['version'])"); \
     if [ "$$CARGO_VER" = "$$SBOM_VER" ]; then \
       echo "OK: sbom.json version matches Cargo.toml ($$CARGO_VER)"; \
     else \
       echo "FAIL: sbom.json version ($$SBOM_VER) != Cargo.toml version ($$CARGO_VER)"; \
       echo "Regenerate sbom.json with: cargo cyclonedx --format json"; \
       exit 1; \
     fi

# Run the full release assessment quality gate (v0.64.0 TRUTH-08).
# Checks: migration continuity, GitHub Actions pinning, SECURITY DEFINER lint,
# roadmap evidence, docs/API drift, feature-status smoke, release evidence dry run.
# Usage: just assess-release [VERSION]
[group: "release"]
assess-release VERSION="":
    @echo "=== pg_ripple release assessment ==="
    @echo ""
    @echo "--- SBOM version check ---"
    @just check-sbom-version
    @echo ""
    @echo "--- Migration headers lint ---"
    bash scripts/check_migration_headers.sh
    @echo ""
    @echo "--- GitHub Actions pinning lint ---"
    bash scripts/check_github_actions_pinned.sh
    @echo ""
    @echo "--- SECURITY DEFINER lint ---"
    bash scripts/check_no_security_definer.sh
    @echo ""
    @echo "--- Roadmap evidence check ---"
    python3 scripts/check_roadmap_evidence.py --version {{VERSION}}
    @echo ""
    @echo "--- API drift check ---"
    python3 scripts/check_api_drift.py --version {{VERSION}}
    @echo ""
    @echo "--- README version check ---"
    bash scripts/check_readme_version.sh
    @echo ""
    @echo "--- Version sync check ---"
    @CARGO_VER=$(grep '^version = ' Cargo.toml | head -1 | grep -oP '"\\K[^"]+'); \
     CTRL_VER=$(grep '^default_version' pg_ripple.control | grep -oP "'\\K[^']+"); \
     if [ "$$CARGO_VER" = "$$CTRL_VER" ]; then \
       echo "OK: Cargo.toml and pg_ripple.control both at v$$CARGO_VER"; \
     else \
       echo "FAIL: version mismatch — Cargo.toml=$$CARGO_VER control=$$CTRL_VER"; exit 1; \
     fi
    @if [ -n "{{VERSION}}" ]; then \
       echo ""; \
       echo "--- Release evidence dry run ---"; \
       bash scripts/generate_release_evidence.sh {{VERSION}}; \
     fi
    @echo ""
    @echo "=== Assessment complete ==="
