# Deployment Models

pg_ripple runs as a PostgreSQL 18 extension. It can be deployed in any environment that supports PostgreSQL 18 with extension loading. This page covers the three primary deployment models and provides production-ready configuration examples.

---

## Deployment Options at a Glance

| Model | Best For | Complexity | SPARQL Protocol |
|---|---|---|---|
| Standalone PostgreSQL | Production, existing PG infrastructure | Low | Via `pg_ripple_http` sidecar |
| Docker / Compose | Evaluation, CI/CD, small deployments | Low | Built-in |
| Managed PostgreSQL | Cloud-native, minimal ops | Medium | Via `pg_ripple_http` sidecar |

```admonish tip title="Recommendation"
Use **Docker Compose** for evaluation and development. Use a **dedicated PostgreSQL 18 instance** for production workloads — this gives full control over shared memory, background workers, and storage configuration.
```

---

## Model 1: Standalone PostgreSQL

Install pg_ripple into a standard PostgreSQL 18 instance. This is the recommended production deployment.

### Prerequisites

- PostgreSQL 18.x installed from packages or source
- Rust toolchain (for building from source) or a pre-built `.so`/`.dylib`
- `pgrx` 0.17 (if building from source)

### Installation

```bash
# Build and install from source
cargo pgrx install --pg-config $(which pg_config) --release

# Or if using a specific PG18 binary
cargo pgrx install --pg-config /usr/lib/postgresql/18/bin/pg_config --release
```

### PostgreSQL Configuration

Add to `postgresql.conf`:

```ini
# Required: load pg_ripple at server start for background workers and shared memory
shared_preload_libraries = 'pg_ripple'

# Shared memory for dictionary cache (adjust for your dataset)
pg_ripple.dictionary_cache_size = 65536   # 64K entries (default)
pg_ripple.cache_budget = 64               # MB (default)

# HTAP merge worker
pg_ripple.merge_threshold = 10000
pg_ripple.merge_interval_secs = 60
pg_ripple.worker_database = 'mydb'        # database the merge worker connects to
```

### Enable the Extension

```sql
CREATE EXTENSION pg_ripple;

-- Verify installation
SELECT pg_ripple.stats();
```

### Add SPARQL Protocol Endpoint

The SPARQL Protocol HTTP endpoint is provided by `pg_ripple_http`, a standalone companion service:

```bash
# Build the HTTP service
cd pg_ripple_http
cargo build --release

# Run it
PG_RIPPLE_HTTP_PG_URL="postgresql://user:pass@localhost/mydb" \
PG_RIPPLE_HTTP_PORT=7878 \
./target/release/pg_ripple_http
```

```admonish info title="pg_ripple_http is optional"
You can use pg_ripple entirely through SQL — `pg_ripple.sparql()`, `pg_ripple.insert_triple()`, etc. The HTTP service adds W3C SPARQL Protocol compatibility for tools like Yasgui, RDF4J, or federated queries from other endpoints.
```

---

## Model 2: Docker / Docker Compose

The Docker deployment bundles PostgreSQL 18, pg_ripple, and pg_ripple_http into containers managed by Docker Compose. This is the fastest way to get started.

### docker-compose.yml

```yaml
# Docker Compose for pg_ripple with SPARQL Protocol HTTP endpoint.
#
# Usage:
#   docker compose up -d
#   curl http://localhost:7878/health
#   curl -G http://localhost:7878/sparql \
#     --data-urlencode "query=SELECT * WHERE { ?s ?p ?o } LIMIT 10"

services:
  postgres:
    build: .
    ports:
      - "5432:5432"
    environment:
      POSTGRES_PASSWORD: ripple
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 5s
      retries: 5

  sparql:
    build: .
    entrypoint: ["/usr/local/bin/pg_ripple_http"]
    ports:
      - "7878:7878"
    environment:
      PG_RIPPLE_HTTP_PG_URL: "postgresql://postgres:ripple@postgres/postgres"
      PG_RIPPLE_HTTP_PORT: "7878"
      PG_RIPPLE_HTTP_POOL_SIZE: "8"
      PG_RIPPLE_HTTP_CORS_ORIGINS: "*"
    depends_on:
      postgres:
        condition: service_healthy

volumes:
  pgdata:
```

### Starting the Stack

```bash
docker compose up -d

# Wait for health check
docker compose ps

# Test SPARQL endpoint
curl http://localhost:7878/health

# Run a query
curl -G http://localhost:7878/sparql \
  --data-urlencode "query=SELECT * WHERE { ?s ?p ?o } LIMIT 5"
```

### Loading Data via Docker

```bash
# Copy a Turtle file into the container and load it
docker compose cp data.ttl postgres:/tmp/data.ttl
docker compose exec postgres psql -U postgres -c \
  "SELECT pg_ripple.load_turtle_file('/tmp/data.ttl');"

# Or load inline
docker compose exec postgres psql -U postgres -c \
  "SELECT pg_ripple.load_turtle('@prefix ex: <http://example.org/> .
    ex:Alice ex:knows ex:Bob .
    ex:Bob ex:age \"30\"^^<http://www.w3.org/2001/XMLSchema#integer> .');"
```

### Production Hardening for Docker

For production Docker deployments, add resource limits and persistent configuration:

```yaml
services:
  postgres:
    build: .
    ports:
      - "5432:5432"
    environment:
      POSTGRES_PASSWORD: ${PG_PASSWORD}
    volumes:
      - pgdata:/var/lib/postgresql/data
      - ./postgresql.conf:/etc/postgresql/postgresql.conf
    command: postgres -c config_file=/etc/postgresql/postgresql.conf
    deploy:
      resources:
        limits:
          memory: 4G
          cpus: "2.0"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 5s
      retries: 5

  sparql:
    build: .
    entrypoint: ["/usr/local/bin/pg_ripple_http"]
    ports:
      - "7878:7878"
    environment:
      PG_RIPPLE_HTTP_PG_URL: "postgresql://postgres:${PG_PASSWORD}@postgres/postgres"
      PG_RIPPLE_HTTP_PORT: "7878"
      PG_RIPPLE_HTTP_POOL_SIZE: "16"
      PG_RIPPLE_HTTP_CORS_ORIGINS: "https://yourdomain.com"
      PG_RIPPLE_HTTP_AUTH_TOKEN: ${SPARQL_AUTH_TOKEN}
    depends_on:
      postgres:
        condition: service_healthy
    deploy:
      resources:
        limits:
          memory: 512M
          cpus: "1.0"
```

```admonish warning title="Security"
Never use default passwords in production. Set `POSTGRES_PASSWORD` and `PG_RIPPLE_HTTP_AUTH_TOKEN` via environment variables or Docker secrets. Restrict `PG_RIPPLE_HTTP_CORS_ORIGINS` to your actual domain.
```

---

## Model 3: Managed PostgreSQL Services

pg_ripple can run on managed PostgreSQL services that support custom extensions and PostgreSQL 18. The key requirements are:

1. **PostgreSQL 18** — pg_ripple uses PG18-specific features (e.g., `WITH RECURSIVE ... CYCLE`).
2. **Custom extension loading** — The service must allow installing `.so` extensions and adding to `shared_preload_libraries`.
3. **Shared memory access** — Required for the dictionary cache and merge worker.

### Supported Managed Services

| Service | Custom Extensions | shared_preload_libraries | Status |
|---|---|---|---|
| AWS RDS for PostgreSQL | Yes (via custom builds) | Yes | Supported with custom AMI |
| Azure Database for PostgreSQL Flexible Server | Yes | Yes | Supported |
| Google Cloud SQL | Limited | Limited | Partial support |
| Self-managed on EC2/GCE/Azure VM | Full control | Full control | Fully supported |

```admonish note title="Cloud VM recommendation"
For managed cloud deployments, running PostgreSQL 18 on a cloud VM (EC2, GCE, Azure VM) with the extension installed gives full control and avoids managed service limitations. Use the managed service's block storage for durability and snapshots for backups.
```

### Managed Service Configuration

When running on a managed service:

```ini
# Add to the PostgreSQL parameter group / configuration
shared_preload_libraries = 'pg_ripple'

# Shared memory — managed services often cap this; start conservative
pg_ripple.dictionary_cache_size = 32768
pg_ripple.cache_budget = 32

# Merge worker targets the primary database
pg_ripple.worker_database = 'mydb'
```

### pg_ripple_http as a Sidecar

On managed services, run `pg_ripple_http` as a sidecar container or systemd service:

```bash
# Kubernetes sidecar example
PG_RIPPLE_HTTP_PG_URL="postgresql://user:pass@pg-host:5432/mydb" \
PG_RIPPLE_HTTP_PORT=7878 \
PG_RIPPLE_HTTP_POOL_SIZE=16 \
pg_ripple_http
```

---

## pg_ripple_http Configuration Reference

The HTTP companion service is configured entirely through environment variables:

| Variable | Default | Description |
|---|---|---|
| `PG_RIPPLE_HTTP_PG_URL` | *(required)* | PostgreSQL connection string |
| `PG_RIPPLE_HTTP_PORT` | `7878` | HTTP listen port |
| `PG_RIPPLE_HTTP_POOL_SIZE` | `8` | Connection pool size |
| `PG_RIPPLE_HTTP_CORS_ORIGINS` | `*` | Allowed CORS origins |
| `PG_RIPPLE_HTTP_AUTH_TOKEN` | *(none)* | Bearer token for authentication |
| `PG_RIPPLE_HTTP_RATE_LIMIT` | `0` | Requests per second (0 = unlimited) |

### Endpoints

| Path | Method | Description |
|---|---|---|
| `/sparql` | GET, POST | SPARQL Protocol query/update endpoint |
| `/health` | GET | Health check (returns 200 if PG connection is live) |
| `/metrics` | GET | Prometheus-compatible metrics |

---

## Network Architecture

```
                    ┌─────────────┐
                    │   Clients   │
                    └──────┬──────┘
                           │
              ┌────────────┴────────────┐
              │                         │
              ▼                         ▼
     ┌─────────────────┐      ┌──────────────────┐
     │  pg_ripple_http  │      │  psql / JDBC /   │
     │  :7878           │      │  application     │
     │  (SPARQL Proto)  │      │  (:5432)         │
     └────────┬─────────┘      └────────┬─────────┘
              │                         │
              └────────────┬────────────┘
                           │
                           ▼
              ┌─────────────────────────┐
              │  PostgreSQL 18          │
              │  + pg_ripple extension  │
              │  + merge worker (BGW)   │
              └─────────────────────────┘
```

```admonish tip title="Read replicas"
For read-heavy workloads, PostgreSQL streaming replication works out of the box. Read replicas receive all VP table changes through WAL. Point read-only SPARQL queries to replicas via a separate `pg_ripple_http` instance connected to the replica.
```

---

## Post-Deployment Verification

After deploying pg_ripple, verify the installation:

```sql
-- Check extension version
SELECT extversion FROM pg_extension WHERE extname = 'pg_ripple';

-- Verify stats (confirms shared memory and merge worker)
SELECT pg_ripple.stats();

-- Run a health check
SELECT pg_ripple.canary();

-- Insert and query a test triple
SELECT pg_ripple.insert_triple(
    '<http://example.org/test>',
    '<http://example.org/status>',
    '"deployed"'
);

SELECT * FROM pg_ripple.sparql('
    SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 1
');
```

```admonish success title="Healthy deployment checklist"
- `stats()` returns `merge_worker_pid > 0`
- `canary()` shows `merge_worker: "ok"` and `catalog_consistent: true`
- `encode_cache_hits / (hits + misses) > 0.90` after initial data load
- SPARQL queries return results
```
