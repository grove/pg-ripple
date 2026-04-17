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
