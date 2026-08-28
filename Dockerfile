# pg_ripple Batteries-Included Docker Image
#
# Builds a PostgreSQL 18 image that pre-installs:
#   - pg_ripple  (this repo)
#   - pg_trickle (incremental materialized views, IVM only since v0.46.0)
#   - pg_tide    (relay, outbox, and inbox subsystem — extracted from pg_trickle v0.46.0)
#   - PostGIS    (geospatial queries via GeoSPARQL)
#   - pgvector   (vector similarity search for hybrid SPARQL + semantic)
#
# Usage:
#   docker build -t pg-ripple:local .
#   docker run --rm -p 5432:5432 -e POSTGRES_PASSWORD=ripple pg-ripple:local
#
# The resulting image is published to ghcr.io as part of each release:
#   docker run --rm -p 5432:5432 -e POSTGRES_PASSWORD=ripple \
#     ghcr.io/trickle-labs/pg-ripple:0.133.0
#
# Authentication:
#   v0.128.1: SCRAM authentication (the postgres:18 base image default) applies
#   to all TCP connections out of the box — the image no longer weakens this.
#   For local development ONLY, set PG_RIPPLE_DEV_TRUST_AUTH=1 to make
#   docker/00-pg_hba.sh add passwordless trust rules for external TCP
#   connections. Never set this in a production or otherwise reachable
#   deployment. See docker/00-pg_hba.sh for details.

# ── Build stage ───────────────────────────────────────────────────────────────
# Build a fresh gosu binary from source using Go 1.26 (fixes all gosu stdlib
# CVEs: CVE-2025-68121 CRITICAL + CVE-2026-32280/32281/32283 HIGH which are
# only fixed in Go ≥1.25.9/1.26.2). CGO_ENABLED=0 produces a static binary
# that is fully portable on any glibc/musl system.
FROM golang:1.26-bookworm@sha256:659cc38c1a394eeb4dd7e31fff6df128bd33444dcc7afd70e3bed5225749dbc0 AS gosu-builder
ARG GOSU_VERSION=1.17
ARG GOSU_COMMIT=0d1847490b448a17eb347e5e357f2c0478df87ad
RUN apt-get update -qq \
    && apt-get install -y --no-install-recommends git \
    && rm -rf /var/lib/apt/lists/* \
    && git init /tmp/gosu \
    && cd /tmp/gosu \
    && git remote add origin https://github.com/tianon/gosu.git \
    && git fetch --depth 1 origin "${GOSU_COMMIT}" \
    && git checkout --detach "${GOSU_COMMIT}" \
    && CGO_ENABLED=0 go build -trimpath -o /go/bin/gosu .

# pgrx 0.18 requires Rust stable. Use rust:1-bookworm which tracks the latest
# stable 1.x release.
FROM rust:1-bookworm@sha256:4e4a7e7939c17991ab35f2b8c2e67593980f771d28f6b1254b1850f860fd0c7f AS builder

ARG PGRX_VERSION=0.18.0
ARG POSTGIS_VERSION=3.5.6
ARG PGVECTOR_VERSION=0.8.2
ARG PG_TRICKLE_VERSION=0.68.0
ARG PG_TIDE_VERSION=0.33.0
ARG PG_TRICKLE_COMMIT=98b1651c33e46f9ffbfb17a1333cd802cf647282
ARG PGVECTOR_COMMIT=cab9da72c04353f143bb06b42ab70a403daac64a
ARG PG_TIDE_COMMIT=10f2881b211d904c741a96535fd7986f51716cff

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
       libgeos-dev \
       libproj-dev \
       libgdal-dev \
       libjson-c-dev \
       libxml2-dev \
       libprotobuf-c-dev \
       protobuf-c-compiler \
       git \
       cmake \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-pgrx (pinned to match Cargo.toml)
RUN cargo install cargo-pgrx --version "=${PGRX_VERSION}" --locked

WORKDIR /build

# Copy manifest files first so dependency layers are cached separately from src.
COPY Cargo.toml Cargo.lock build.rs pg_ripple.control .versions.toml ./
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

# ── Build pg_trickle ──────────────────────────────────────────────────────────
RUN git init /tmp/pg_trickle \
    && cd /tmp/pg_trickle \
    && git remote add origin https://github.com/trickle-labs/pg-trickle.git \
    && git fetch --depth 1 origin "${PG_TRICKLE_COMMIT}" \
    && git checkout --detach "${PG_TRICKLE_COMMIT}" \
    && cargo pgrx package \
         --pg-config /usr/lib/postgresql/18/bin/pg_config \
         --features pg18

# ── Build pgvector ────────────────────────────────────────────────────────────
RUN git init /tmp/pgvector \
    && cd /tmp/pgvector \
    && git remote add origin https://github.com/pgvector/pgvector.git \
    && git fetch --depth 1 origin "${PGVECTOR_COMMIT}" \
    && git checkout --detach "${PGVECTOR_COMMIT}" \
    && make PG_CONFIG=/usr/lib/postgresql/18/bin/pg_config \
    && make PG_CONFIG=/usr/lib/postgresql/18/bin/pg_config install

# ── Build pg_tide ────────────────────────────────────────────────────────────
RUN git init /tmp/pg_tide \
    && cd /tmp/pg_tide \
    && git remote add origin https://github.com/trickle-labs/pg-tide.git \
    && git fetch --depth 1 origin "${PG_TIDE_COMMIT}" \
    && git checkout --detach "${PG_TIDE_COMMIT}" \
    && cargo pgrx package \
         --pg-config /usr/lib/postgresql/18/bin/pg_config \
         --features pg18

# ── Build PostGIS ─────────────────────────────────────────────────────────────
ARG POSTGIS_SHA256=4f3e51f14c19ba5408c87ef2339eac8f3fa1cc297573dae574d9908d6d597766
RUN curl -fsSL \
      "https://download.osgeo.org/postgis/source/postgis-${POSTGIS_VERSION}.tar.gz" \
      -o /tmp/postgis.tar.gz \
    && echo "${POSTGIS_SHA256}  /tmp/postgis.tar.gz" | sha256sum -c - \
    && tar xzf /tmp/postgis.tar.gz -C /tmp \
    && rm /tmp/postgis.tar.gz \
    && cd /tmp/postgis-${POSTGIS_VERSION} \
    && ./configure \
         --with-pgconfig=/usr/lib/postgresql/18/bin/pg_config \
         --without-topology \
         --without-address-standardizer \
    && make -j"$(nproc)" \
    && make install

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM postgres:18-bookworm@sha256:a10c981235b4f635e65df0cfb66a5598064628128505dbc6a3ed4ca303717521

LABEL org.opencontainers.image.source="https://github.com/trickle-labs/pg-ripple"
LABEL org.opencontainers.image.description="PostgreSQL 18 with pg_ripple, pg_trickle, pg_tide, PostGIS, pgvector"
LABEL org.opencontainers.image.licenses="Apache-2.0"

# Replace the base image's gosu (compiled with old Go stdlib) with our freshly
# built version to eliminate HIGH/CRITICAL stdlib CVEs (CVE-2025-68121 et al.).
COPY --from=gosu-builder /go/bin/gosu /usr/local/bin/gosu

# Runtime deps for PostGIS, pgvector, and pg_trgm (CONF-SBOM-01c: required for fuzzy SPARQL v0.87.0)
# Use runtime library packages, not -dev packages — headers and static libs are
# only needed at compile time and account for ~900 MB of unnecessary bloat.
# apt-get upgrade -y patches all OS packages to their latest versions, which
# eliminates CVEs with available fixes (e.g. glibc CVE-2026-0861, libcap2
# CVE-2026-4878, systemd CVE-2026-29111) that Trivy flags on the base image.
RUN apt-get update -qq \
    && apt-get upgrade -y --no-install-recommends \
    && apt-get install -y --no-install-recommends \
       libgeos-c1v5 \
       libproj25 \
       libgdal32 \
       libjson-c5 \
       libprotobuf-c1 \
       postgresql-contrib \
    && rm -rf /var/lib/apt/lists/*

# ── pg_ripple ─────────────────────────────────────────────────────────────────
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

# ── pg_trickle ────────────────────────────────────────────────────────────────
COPY --from=builder \
    /tmp/pg_trickle/target/release/pg_trickle-pg18/usr/lib/postgresql/18/lib/pg_trickle.so \
    /usr/lib/postgresql/18/lib/

COPY --from=builder \
    /tmp/pg_trickle/target/release/pg_trickle-pg18/usr/share/postgresql/18/extension/ \
    /usr/share/postgresql/18/extension/

# ── pg_tide ───────────────────────────────────────────────────────────────────
COPY --from=builder \
    /tmp/pg_tide/target/release/pg_tide-pg18/usr/lib/postgresql/18/lib/pg_tide.so \
    /usr/lib/postgresql/18/lib/

COPY --from=builder \
    /tmp/pg_tide/target/release/pg_tide-pg18/usr/share/postgresql/18/extension/ \
    /usr/share/postgresql/18/extension/

# ── pgvector ──────────────────────────────────────────────────────────────────
COPY --from=builder \
    /usr/lib/postgresql/18/lib/vector.so \
    /usr/lib/postgresql/18/lib/

COPY --from=builder \
    /usr/share/postgresql/18/extension/vector* \
    /usr/share/postgresql/18/extension/

# ── PostGIS ───────────────────────────────────────────────────────────────────
COPY --from=builder \
    /usr/lib/postgresql/18/lib/postgis-3.so \
    /usr/lib/postgresql/18/lib/

COPY --from=builder \
    /usr/share/postgresql/18/extension/postgis* \
    /usr/share/postgresql/18/extension/

# Initialization scripts — executed by the postgres entrypoint on first start,
# in lexicographic order.  See comments in each file for details.
COPY docker/ /docker-entrypoint-initdb.d/

# Expose PostgreSQL (5432) and SPARQL HTTP (7878) ports
EXPOSE 5432 7878

# Run as the non-root postgres user instead of root (security hardening S1-1).
# The postgres user is created by the base image.
USER postgres

# pg_ripple creates a schema named "pg_ripple".  PostgreSQL 18 blocks creation
# of schemas whose names start with "pg_" unless allow_system_table_mods is on.
# Passing it as a command argument ensures the flag is active both during init
# (when the entrypoint runs the scripts above) and at every subsequent start.
CMD ["postgres", "-c", "allow_system_table_mods=on", "-c", "shared_preload_libraries=pg_ripple,pg_trickle"]
