# pg_ripple — project commands
# https://github.com/casey/just

set dotenv-load := false

# Default PostgreSQL major version
pg := "18"

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
    cargo pgrx install --pg-config $(which pg_config)

# ── Benchmarks ────────────────────────────────────────────────────────────

# Load BSBM data (default: scale factor 1)
[group: "bench"]
bench-bsbm-load scale="1":
    psql -v scale={{scale}} -f benchmarks/bsbm/bsbm_load.sql

# Run BSBM query mix (12 standard BSBM queries)
[group: "bench"]
bench-bsbm-queries:
    psql -f benchmarks/bsbm/bsbm_queries.sql

# Run BSBM HTAP concurrent workload (insert + query under load)
[group: "bench"]
bench-bsbm-htap:
    psql -f benchmarks/bsbm/bsbm_htap.sql

# Run pgbench BSBM sustained throughput test
[group: "bench"]
bench-bsbm-pgbench duration="60" clients="10" jobs="4":
    pgbench -f benchmarks/bsbm/bsbm_pgbench.sql -T {{duration}} -c {{clients}} -j {{jobs}} postgres

# Run all BSBM benchmarks in sequence (load → queries → HTAP → pgbench)
[group: "bench"]
bench-bsbm-all scale="1" duration="60" clients="10" jobs="4": (bench-bsbm-load scale) bench-bsbm-queries bench-bsbm-htap (bench-bsbm-pgbench duration clients jobs)

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
