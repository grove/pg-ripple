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

# Run all tests (unit + pgrx + regress)
[group: "test"]
test-all: test test-regress

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
