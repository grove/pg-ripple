# pg_ripple Docker image
#
# Multi-stage build:
#   builder — compiles the pgrx extension against PostgreSQL 18 dev headers
#   final   — slim postgres:18 image with the .so and SQL files installed
#
# Usage:
#   docker build -t pg-ripple:local .
#   docker run --rm -p 5432:5432 -e POSTGRES_PASSWORD=ripple pg-ripple:local
#   psql -h localhost -U postgres -d postgres  # no password required
#
# The resulting image is also published to ghcr.io as part of each release:
#   docker run --rm -p 5432:5432 -e POSTGRES_PASSWORD=ripple \
#     ghcr.io/grove/pg-ripple:latest
#
# Authentication:
#   The container is configured for development/testing with trust authentication
#   enabled for external TCP connections. See docker/00-pg_hba.sh for details.
#   For production deployments, use password-based authentication instead.

# ── Build stage ───────────────────────────────────────────────────────────────
# Build a fresh gosu binary from source using Go 1.26 (fixes all gosu stdlib
# CVEs: CVE-2025-68121 CRITICAL + CVE-2026-32280/32281/32283 HIGH which are
# only fixed in Go ≥1.25.9/1.26.2). CGO_ENABLED=0 produces a static binary
# that is fully portable on any glibc/musl system.
FROM golang:1.26-bookworm AS gosu-builder
RUN CGO_ENABLED=0 go install github.com/tianon/gosu@latest

# pgrx 0.18 requires Rust stable. Use rust:1-bookworm which tracks the latest
# stable 1.x release.
FROM rust:1-bookworm AS builder

ARG PGRX_VERSION=0.18.0

# Add the PostgreSQL Global Development Group APT repository so we get the
# exact PostgreSQL 18 server development headers that match postgres:18-bookworm.
RUN apt-get update -qq \
    && apt-get install -y --no-install-recommends gnupg curl ca-certificates \
    && curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
       | gpg --dearmor -o /usr/share/keyrings/postgresql.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/postgresql.gpg] \
https://apt.postgresql.org/pub/repos/apt bookworm-pgdg main" \
       > /etc/apt/sources.list.d/pgdg.list \
    && apt-get update -qq \
    && apt-get install -y --no-install-recommends \
       build-essential \
       pkg-config \
       libssl-dev \
       libclang-dev \
       clang \
       libreadline-dev \
       libicu-dev \
       bison \
       flex \
       postgresql-server-dev-18 \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-pgrx (pinned to match Cargo.toml)
RUN cargo install cargo-pgrx --version "=${PGRX_VERSION}" --locked

WORKDIR /build

# Copy manifest files first so dependency layers are cached separately from src.
COPY Cargo.toml Cargo.lock build.rs pg_ripple.control ./
COPY src/   ./src/
COPY sql/   ./sql/
COPY pg_ripple_http/ ./pg_ripple_http/

# Tell pgrx to use the system PostgreSQL 18 (avoids downloading a second copy).
RUN cargo pgrx init --pg18 /usr/lib/postgresql/18/bin/pg_config

# Package the extension into the standard PostgreSQL shared-library layout:
#   target/release/pg_ripple-pg18/
#     usr/lib/postgresql/18/lib/pg_ripple.so
#     usr/share/postgresql/18/extension/pg_ripple.control
#     usr/share/postgresql/18/extension/pg_ripple--*.sql
RUN cargo pgrx package \
      --pg-config /usr/lib/postgresql/18/bin/pg_config \
      --features pg18

# Build the SPARQL Protocol HTTP service.
RUN cargo build --release -p pg_ripple_http

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM postgres:18-bookworm

LABEL org.opencontainers.image.source="https://github.com/grove/pg-ripple"
LABEL org.opencontainers.image.description="PostgreSQL 18 with pg_ripple RDF/SPARQL extension"
LABEL org.opencontainers.image.licenses="Apache-2.0"

# Replace the base image's gosu (compiled with old Go stdlib) with our freshly
# built version to eliminate HIGH/CRITICAL stdlib CVEs (CVE-2025-68121 et al.).
COPY --from=gosu-builder /go/bin/gosu /usr/local/bin/gosu

# Copy shared library
COPY --from=builder \
    /build/target/release/pg_ripple-pg18/usr/lib/postgresql/18/lib/pg_ripple.so \
    /usr/lib/postgresql/18/lib/

# Copy extension control file and all SQL migration scripts
COPY --from=builder \
    /build/target/release/pg_ripple-pg18/usr/share/postgresql/18/extension/ \
    /usr/share/postgresql/18/extension/

# Copy the SPARQL Protocol HTTP service binary
COPY --from=builder \
    /build/target/release/pg_ripple_http \
    /usr/local/bin/pg_ripple_http

# Initialization scripts — executed by the postgres entrypoint on first start,
# in lexicographic order.  See comments in each file for details.
COPY docker/ /docker-entrypoint-initdb.d/

# Expose PostgreSQL (5432) and SPARQL HTTP (7878) ports
EXPOSE 5432 7878

# v0.51.0: Run as the non-root postgres user instead of root.
# This is required for production deployments (security hardening S1-1).
# The postgres user is created by the base image.
USER postgres

# pg_ripple creates a schema named "pg_ripple".  PostgreSQL 18 blocks creation
# of schemas whose names start with "pg_" unless allow_system_table_mods is on.
# Passing it as a command argument ensures the flag is active both during init
# (when the entrypoint runs the scripts above) and at every subsequent start.
CMD ["postgres", "-c", "allow_system_table_mods=on"]
