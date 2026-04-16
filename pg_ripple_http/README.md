# pg_ripple_http

Standalone HTTP service that exposes a [W3C SPARQL 1.1 Protocol](https://www.w3.org/TR/sparql11-protocol/) endpoint for [pg_ripple](../README.md). Any standard SPARQL client — YASGUI, SPARQLWrapper, Jena, or plain `curl` — can query pg_ripple without a PostgreSQL driver.

## Build

```bash
cargo build --release -p pg_ripple_http
```

The binary is placed at `target/release/pg_ripple_http`.

**Requirements:** Rust 1.88+, and a running PostgreSQL 18 instance with the `pg_ripple` extension installed.

## Run

```bash
./target/release/pg_ripple_http
```

On startup, the service connects to PostgreSQL, verifies that pg_ripple is available, and logs the connection details:

```
INFO pg_ripple_http: connected to postgresql://localhost/postgres (port 7878), triple store contains 12345 triples
INFO pg_ripple_http: pg_ripple_http listening on http://0.0.0.0:7878
```

## Configuration

All configuration is via environment variables:

| Variable | Default | Description |
|---|---|---|
| `PG_RIPPLE_HTTP_PG_URL` | `postgresql://localhost/postgres` | PostgreSQL connection URL |
| `PG_RIPPLE_HTTP_PORT` | `7878` | HTTP listening port |
| `PG_RIPPLE_HTTP_POOL_SIZE` | `16` | Database connection pool size |
| `PG_RIPPLE_HTTP_AUTH_TOKEN` | (unset) | If set, requests must include `Authorization: Bearer <token>` |
| `PG_RIPPLE_HTTP_RATE_LIMIT` | `0` | Max requests/sec per client IP (0 = disabled) |
| `PG_RIPPLE_HTTP_CORS_ORIGINS` | `*` | Comma-separated allowed origins, or `*` for all |

Example:

```bash
export PG_RIPPLE_HTTP_PG_URL="postgresql://user:password@db-host:5432/mydb"
export PG_RIPPLE_HTTP_PORT=8080
export PG_RIPPLE_HTTP_AUTH_TOKEN="my-secret-token"
./target/release/pg_ripple_http
```

## Endpoints

### `GET /health`

Returns `200 OK` when the service is up and the database is reachable. Use for load balancer health checks.

```bash
curl http://localhost:7878/health
```

### `GET /metrics`

Prometheus-compatible metrics.

```bash
curl http://localhost:7878/metrics
```

### `GET /sparql?query=…`

Run a SPARQL query via URL parameter.

```bash
curl -G http://localhost:7878/sparql \
  --data-urlencode "query=SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10"
```

### `POST /sparql`

Run a SPARQL query or update via request body.

| Content-Type | Body |
|---|---|
| `application/sparql-query` | Raw SPARQL SELECT/ASK/CONSTRUCT/DESCRIBE |
| `application/sparql-update` | Raw SPARQL INSERT/DELETE |
| `application/x-www-form-urlencoded` | `query=…` or `update=…` |

```bash
# SELECT
curl -X POST http://localhost:7878/sparql \
  -H "Content-Type: application/sparql-query" \
  -d "SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10"

# Update
curl -X POST http://localhost:7878/sparql \
  -H "Content-Type: application/sparql-update" \
  -d 'INSERT DATA { <http://example.org/alice> <http://example.org/name> "Alice" }'
```

## Content negotiation

Set the `Accept` header to control the response format:

| Accept | Format |
|---|---|
| `application/sparql-results+json` *(default for SELECT/ASK)* | SPARQL Results JSON |
| `application/sparql-results+xml` | SPARQL Results XML |
| `text/csv` | CSV |
| `text/tab-separated-values` | TSV |
| `text/turtle` *(default for CONSTRUCT/DESCRIBE)* | Turtle |
| `application/n-triples` | N-Triples |
| `application/ld+json` | JSON-LD |

```bash
curl -G http://localhost:7878/sparql \
  -H "Accept: text/csv" \
  --data-urlencode "query=SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 5"
```

## Authentication

If `PG_RIPPLE_HTTP_AUTH_TOKEN` is set, every request must include the token:

```bash
curl -G http://localhost:7878/sparql \
  -H "Authorization: Bearer my-secret-token" \
  --data-urlencode "query=SELECT * WHERE { ?s ?p ?o } LIMIT 5"
```

Both `Authorization: Bearer <token>` and `Authorization: Basic <token>` are accepted.

## Docker Compose

The root `docker-compose.yml` runs both PostgreSQL and `pg_ripple_http` together:

```bash
docker compose up
```

Services:

| Service | Port | Description |
|---|---|---|
| `postgres` | 5432 | PostgreSQL 18 + pg_ripple |
| `sparql` | 7878 | SPARQL HTTP endpoint |
