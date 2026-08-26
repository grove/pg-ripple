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

# ── ER Benchmarks (v0.110.0) ──────────────────────────────────────────────────

# Run both ER benchmarks: Magellan (correctness gate) + freshness latency
[group: "bench"]
bench-er: bench-er-magellan bench-er-freshness

# Run the Magellan ER benchmark (Abt-Buy F1 >= 0.78, DBLP-ACM F1 >= 0.90)
[group: "bench"]
bench-er-magellan:
    bash benchmarks/er_magellan.sh

# Run the ER freshness benchmark (p95 insert-to-match latency < 500 ms)
[group: "bench"]
bench-er-freshness:
    bash benchmarks/er_freshness.sh

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

# Build docs with mdBook (succeeds without mdbook-admonish; install it for styled callouts)
[group: "docs"]
docs-build:
    mdbook build docs

# Check that all relative Markdown links in docs/ resolve to real files
[group: "docs"]
docs-check-links:
    python3 scripts/check_docs_links.py

# Check that every docs/src/*.md page is referenced in SUMMARY.md
[group: "docs"]
docs-check-summary:
    python3 scripts/check_docs_summary.py

# Report GUC defaults that differ between src/gucs/ and the documentation (non-blocking)
[group: "docs"]
docs-check-guc-drift:
    python3 scripts/check_guc_drift.py

# Report HTTP routes in routing/mod.rs that are not listed in http-api.md (non-blocking)
[group: "docs"]
docs-check-http-routes:
    python3 scripts/check_http_routes.py

# Report SQL pg_extern functions not in the docs corpus (non-blocking until --strict)
[group: "docs"]
docs-check-sql-coverage:
    python3 scripts/check_sql_coverage.py

# Report PT error codes in src/ that are not documented in docs/ or CHANGELOG.md
[group: "docs"]
docs-check-pt-codes:
    bash scripts/check_pt_codes.sh

# Report SQL/SPARQL code blocks in Getting Started and Feature pages that lack a label (report-only)
[group: "docs"]
docs-check-examples:
    python3 scripts/check_docs_examples.py

# Run all docs validation checks (links, summary, GUC drift, HTTP routes, SQL coverage, PT codes)
[group: "docs"]
docs-check: docs-check-links docs-check-summary docs-check-guc-drift docs-check-http-routes docs-check-sql-coverage docs-check-pt-codes

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
# H15-01 (v0.94.0): accepts an optional COMPAT_MIN to pin COMPATIBLE_EXTENSION_MIN
# independently from the release version (it is a *minimum*, not the current version).
# When COMPAT_MIN is omitted, COMPATIBLE_EXTENSION_MIN is left unchanged.
#
# Usage:  just bump-version 0.94.0
#         just bump-version 0.95.0 0.94.0   # also bumps COMPATIBLE_EXTENSION_MIN
# ROAD-02 (v0.89.0): atomically updates all nine version references and creates
# CHANGELOG + migration stubs. Use bump-version-dry to preview without writing.
[group: "release"]
bump-version NEW_VERSION COMPAT_MIN="":
    @OLD_VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/.*"\([^"]*\)".*/\1/'); \
     echo "Bumping $OLD_VERSION → {{NEW_VERSION}}"; \
     sed -i '' "s/^version = \"$OLD_VERSION\"/version = \"{{NEW_VERSION}}\"/" Cargo.toml; \
     sed -i '' "s/^version = \"$OLD_VERSION\"/version = \"{{NEW_VERSION}}\"/" pg_ripple_http/Cargo.toml; \
     sed -i '' "s/^default_version = '$OLD_VERSION'/default_version = '{{NEW_VERSION}}'/" pg_ripple.control; \
     sed -i '' "s/^comment = '.*'/comment = 'High-performance RDF triple store with SPARQL 1.1, SHACL, Datalog, HTAP, federation, and Datalog-native PageRank (v{{NEW_VERSION}})'/" pg_ripple.control; \
     if [ -n "{{COMPAT_MIN}}" ]; then \
       OLD_COMPAT=$(grep 'COMPATIBLE_EXTENSION_MIN' pg_ripple_http/src/main.rs | grep -oE '"[0-9]+\.[0-9]+\.[0-9]+"' | tr -d '"' | head -1); \
       sed -i '' "s/COMPATIBLE_EXTENSION_MIN: \&str = \"$OLD_COMPAT\"/COMPATIBLE_EXTENSION_MIN: \&str = \"{{COMPAT_MIN}}\"/" pg_ripple_http/src/main.rs; \
       echo "Updated COMPATIBLE_EXTENSION_MIN: $OLD_COMPAT → {{COMPAT_MIN}}"; \
     fi; \
     sed -i '' "s|ghcr.io/trickle-labs/pg_ripple:$OLD_VERSION|ghcr.io/trickle-labs/pg_ripple:{{NEW_VERSION}}|g" docker-compose.yml; \
     MIGRATION_FILE="sql/pg_ripple--$OLD_VERSION--{{NEW_VERSION}}.sql"; \
     if [ ! -f "$MIGRATION_FILE" ]; then \
       printf -- "-- Migration $OLD_VERSION → {{NEW_VERSION}}\n-- Schema changes: TODO\n" > "$MIGRATION_FILE"; \
       echo "Created $MIGRATION_FILE"; \
     else \
       echo "$MIGRATION_FILE already exists"; \
     fi; \
     CHANGELOG_SECTION="## [{{NEW_VERSION}}] — $(date +%Y-%m-%d)\n\n### Added\n- TODO\n\n### Changed\n- TODO\n\n### Fixed\n- TODO\n\n"; \
     if grep -q "## \[{{NEW_VERSION}}\]" CHANGELOG.md 2>/dev/null; then \
       echo "CHANGELOG.md already contains ## [{{NEW_VERSION}}] section"; \
     else \
       TMP=$(mktemp); \
       awk -v section="$CHANGELOG_SECTION" '/^## \[[0-9]/{if(!done){printf section; done=1}} {print}' CHANGELOG.md > "$TMP" && mv "$TMP" CHANGELOG.md; \
       echo "Added CHANGELOG.md section for v{{NEW_VERSION}}"; \
     fi; \
     echo ""; \
     echo "=== Version bump complete ==="; \
     echo "Files updated: Cargo.toml, pg_ripple_http/Cargo.toml, pg_ripple.control,"; \
     echo "  docker-compose.yml, CHANGELOG.md"; \
     echo "Next: fill in CHANGELOG.md and $MIGRATION_FILE"

# Dry-run for bump-version: prints proposed changes without writing any files.
# ROAD-02 (v0.89.0)
# Usage: just bump-version-dry 0.86.0
[group: "release"]
bump-version-dry NEW_VERSION:
    @OLD_VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/.*"\([^"]*\)".*/\1/'); \
     echo "=== bump-version dry-run: $OLD_VERSION → {{NEW_VERSION}} ==="; \
     echo ""; \
     echo "[Cargo.toml]          version = \"{{NEW_VERSION}}\""; \
     echo "[pg_ripple_http/Cargo.toml] version = \"{{NEW_VERSION}}\""; \
     echo "[pg_ripple.control]   default_version = '{{NEW_VERSION}}'"; \
     echo "[pg_ripple_http/src/main.rs] COMPATIBLE_EXTENSION_MIN = \"{{NEW_VERSION}}\""; \
     echo "[docker-compose.yml]  ghcr.io/trickle-labs/pg_ripple:{{NEW_VERSION}}"; \
     MIGRATION_FILE="sql/pg_ripple--$OLD_VERSION--{{NEW_VERSION}}.sql"; \
     if [ -f "$MIGRATION_FILE" ]; then \
       echo "[migration]           $MIGRATION_FILE (already exists)"; \
     else \
       echo "[migration]           $MIGRATION_FILE (will be created)"; \
     fi; \
     if grep -q "## \[{{NEW_VERSION}}\]" CHANGELOG.md 2>/dev/null; then \
       echo "[CHANGELOG.md]        ## [{{NEW_VERSION}}] section already exists"; \
     else \
       echo "[CHANGELOG.md]        ## [{{NEW_VERSION}}] stub section will be added"; \
     fi; \
     echo ""; \
     echo "Run 'just bump-version {{NEW_VERSION}}' to apply."

# BUILD-02 (v0.84.0): Regenerate sbom.json using cargo-cyclonedx.
# Requires cargo install cargo-cyclonedx.
[group: "release"]
regen-sbom:
    cargo cyclonedx --format json
    @echo "sbom.json regenerated. Review changes with: git diff sbom.json"

# BUILD-02 (v0.84.0): Check that all version strings are consistent.
# H15-01 (v0.94.0): COMPATIBLE_EXTENSION_MIN only needs to be ≤ current version,
# not equal.  This matches the CI lint-version-sync gate in ci.yml.
# Verifies: Cargo.toml root, pg_ripple_http/Cargo.toml, pg_ripple.control,
# COMPATIBLE_EXTENSION_MIN in pg_ripple_http/src/main.rs, docker-compose.yml.
[group: "release"]
check-version-sync:
    @CARGO_VER=$(grep '^version = ' Cargo.toml | head -1 | sed 's/.*"\([^"]*\)".*/\1/'); \
     HTTP_VER=$(grep '^version = ' pg_ripple_http/Cargo.toml | head -1 | sed 's/.*"\([^"]*\)".*/\1/'); \
     CTRL_VER=$(grep '^default_version' pg_ripple.control | sed "s/.*'\([^']*\)'.*/\1/"); \
     COMPAT_VER=$(grep 'COMPATIBLE_EXTENSION_MIN' pg_ripple_http/src/main.rs | head -1 | sed 's/.*"\([0-9]*\.[0-9]*\.[0-9]*\)".*/\1/'); \
     DC_VER=$(grep 'ghcr.io/trickle-labs/pg_ripple:' docker-compose.yml | head -1 | sed 's/.*pg_ripple:\([0-9.]*\).*/\1/'); \
     FAIL=0; \
     echo "Cargo.toml:              $CARGO_VER"; \
     echo "pg_ripple_http:          $HTTP_VER"; \
     echo "pg_ripple.control:       $CTRL_VER"; \
     echo "COMPATIBLE_EXTENSION_MIN: $COMPAT_VER"; \
     echo "docker-compose.yml:      $DC_VER"; \
     [ "$CARGO_VER" = "$HTTP_VER" ]   || { echo "FAIL: pg_ripple_http version mismatch"; FAIL=1; }; \
     [ "$CARGO_VER" = "$CTRL_VER" ]   || { echo "FAIL: pg_ripple.control version mismatch"; FAIL=1; }; \
     [ "$CARGO_VER" = "$DC_VER" ]     || { echo "FAIL: docker-compose.yml image tag mismatch"; FAIL=1; }; \
     LOWEST=$(printf '%s\n%s' "$COMPAT_VER" "$CARGO_VER" | sort -V | head -1); \
     [ "$LOWEST" = "$COMPAT_VER" ] || { echo "FAIL: COMPATIBLE_EXTENSION_MIN ($COMPAT_VER) > extension version ($CARGO_VER)"; FAIL=1; }; \
     if [ $FAIL -eq 0 ]; then echo "OK: all versions consistent at $CARGO_VER (COMPAT_MIN=$COMPAT_VER)"; fi; \
     exit $FAIL

# BUILD-02 (v0.84.0): Regenerate the OpenAPI spec from the running HTTP service.
# Requires pg_ripple_http to be running on $PG_RIPPLE_HTTP_URL (default: http://localhost:3000).
[group: "release"]
regen-openapi:
    @URL=${PG_RIPPLE_HTTP_URL:-http://localhost:3000}; \
     echo "Fetching OpenAPI spec from $URL/openapi.json"; \
     curl -fsSL "$URL/openapi.json" -o pg_ripple_http/openapi.json && \
     echo "Saved to pg_ripple_http/openapi.json"; \
     if command -v yq >/dev/null 2>&1; then \
       yq -P pg_ripple_http/openapi.json > pg_ripple_http/openapi.yaml && \
       echo "Converted to pg_ripple_http/openapi.yaml"; \
     else \
       echo "(Skipping YAML: yq not installed)"; \
     fi

# ── Release Quality Gate ──────────────────────────────────────────────────

# Feature 9 (v0.120.0): Generate per-tenant Helm values fragment.
#
# Usage:  just generate-helm-values TENANT=acme
#
# Queries _pg_ripple.tenants via pg_ripple_http and emits a per-tenant
# values-<name>.yaml fragment for use with `helm install --values`.
# Requires pg_ripple_http to be running and PG_RIPPLE_HTTP_URL set.
[group: "release"]
generate-helm-values TENANT="":
    @if [ -z "{{TENANT}}" ]; then \
       echo "Usage: just generate-helm-values TENANT=<name>"; \
       exit 1; \
     fi; \
     URL=${PG_RIPPLE_HTTP_URL:-http://localhost:7878}; \
     OUTFILE="values-{{TENANT}}.yaml"; \
     echo "Fetching tenant '{{TENANT}}' from $URL/tenants/{{TENANT}} ..."; \
     RESPONSE=$(curl -fsSL "$URL/tenants/{{TENANT}}" 2>&1); \
     if [ $? -ne 0 ]; then \
       echo "ERROR: could not fetch tenant from $URL/tenants/{{TENANT}}"; \
       echo "Response: $RESPONSE"; \
       exit 1; \
     fi; \
     GRAPH_IRI=$(echo "$RESPONSE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('graph_iri',''))"); \
     QUOTA=$(echo "$RESPONSE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('quota_triples', 0))"); \
     echo "# pg_ripple per-tenant Helm values for tenant: {{TENANT}}" > $OUTFILE; \
     echo "# Generated by: just generate-helm-values TENANT={{TENANT}}" >> $OUTFILE; \
     echo "tenant:" >> $OUTFILE; \
     echo "  name: \"{{TENANT}}\"" >> $OUTFILE; \
     echo "  graphIri: \"$GRAPH_IRI\"" >> $OUTFILE; \
     echo "  quotaTriples: $QUOTA" >> $OUTFILE; \
     echo "Wrote $OUTFILE"; \
     cat $OUTFILE

# SBOM-03: Verify that sbom.json version matches Cargo.toml version.
# Fails with exit 1 if they differ (prevents stale SBOM in releases).
[group: "release"]
check-sbom-version:
    @CARGO_VER=$(grep '^version = ' Cargo.toml | head -1 | sed 's/.*"\([^"]*\)".*/\1/'); \
     SBOM_VER=$(python3 -c "import json; print(json.load(open('sbom.json')).get('metadata',{}).get('component',{}).get('version','MISSING'))"); \
     if [ "$CARGO_VER" = "$SBOM_VER" ]; then \
       echo "OK: sbom.json version matches Cargo.toml ($CARGO_VER)"; \
     else \
       echo "FAIL: sbom.json version ($SBOM_VER) != Cargo.toml version ($CARGO_VER)"; \
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
    @CARGO_VER=$(grep '^version = ' Cargo.toml | head -1 | sed 's/.*"\([^"]*\)".*/\1/'); \
     CTRL_VER=$(grep '^default_version' pg_ripple.control | sed "s/.*'\([^']*\)'.*/\1/"); \
     if [ "$CARGO_VER" = "$CTRL_VER" ]; then \
       echo "OK: Cargo.toml and pg_ripple.control both at v$CARGO_VER"; \
     else \
       echo "FAIL: version mismatch — Cargo.toml=$CARGO_VER control=$CTRL_VER"; exit 1; \
     fi
    @if [ -n "{{VERSION}}" ]; then \
       echo ""; \
       echo "--- Release evidence dry run ---"; \
       bash scripts/generate_release_evidence.sh {{VERSION}}; \
     fi
    @echo ""
    @echo "=== Assessment complete ==="
