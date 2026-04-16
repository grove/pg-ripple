# SPARQL Protocol (HTTP Endpoint)

pg_ripple v0.15.0 ships with a companion HTTP service (`pg_ripple_http`) that implements the [W3C SPARQL 1.1 Protocol](https://www.w3.org/TR/sparql11-protocol/). Any standard SPARQL client — YASGUI, Protégé, SPARQLWrapper, Jena, or plain `curl` — can query pg_ripple without driver-specific configuration.

---

## Architecture

`pg_ripple_http` is a standalone Rust binary built with [axum](https://github.com/tokio-rs/axum) and [tokio](https://tokio.rs/). It connects to PostgreSQL via [deadpool-postgres](https://crates.io/crates/deadpool-postgres), translates HTTP requests into calls to `pg_ripple.sparql()`, `sparql_ask()`, `sparql_construct()`, `sparql_describe()`, and `sparql_update()`, then formats the results according to the requested content type.

```
┌────────────┐    HTTP     ┌──────────────────┐    SQL/SPI    ┌────────────┐
│   Client   │ ──────────► │  pg_ripple_http   │ ────────────► │ PostgreSQL │
│  (YASGUI,  │ ◄────────── │  (axum + tokio)   │ ◄──────────── │ + pg_ripple│
│  curl, …)  │   JSON/XML  └──────────────────┘               └────────────┘
└────────────┘
```

---

## Quick start with Docker Compose

The easiest way to run both PostgreSQL and the HTTP endpoint:

```bash
docker compose up
```

This starts two containers:

| Service | Port | Description |
|---------|------|-------------|
| `postgres` | 5432 | PostgreSQL 18 with pg_ripple installed |
| `sparql` | 7878 | HTTP endpoint for SPARQL queries |

Once running:

```bash
# Health check
curl http://localhost:7878/health

# Run a SPARQL query
curl -G http://localhost:7878/sparql \
  --data-urlencode "query=SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10"
```

---

## Endpoints

### GET /sparql

Query via URL parameter. For simple queries and browser-based tools.

```
GET /sparql?query=SELECT+?s+?p+?o+WHERE+{+?s+?p+?o+}+LIMIT+10
```

### POST /sparql

Query via request body. Supports two content types:

| Content-Type | Body format |
|---|---|
| `application/sparql-query` | Raw SPARQL query text |
| `application/x-www-form-urlencoded` | `query=...` (URL-encoded) |

```bash
# Raw SPARQL body
curl -X POST http://localhost:7878/sparql \
  -H "Content-Type: application/sparql-query" \
  -d "SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10"

# Form-encoded
curl -X POST http://localhost:7878/sparql \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "query=SELECT+?s+?p+?o+WHERE+{+?s+?p+?o+}+LIMIT+10"
```

### POST /sparql (Update)

SPARQL Update operations use `Content-Type: application/sparql-update`:

```bash
curl -X POST http://localhost:7878/sparql \
  -H "Content-Type: application/sparql-update" \
  -d "INSERT DATA { <http://example.org/alice> <http://example.org/name> \"Alice\" }"
```

### GET /health

Returns `200 OK` when the service is ready. Use for load balancer health checks.

### GET /metrics

Prometheus-compatible metrics:

| Metric | Description |
|--------|-------------|
| `pg_ripple_query_count` | Total SPARQL queries processed |
| `pg_ripple_error_count` | Total query errors |
| `pg_ripple_query_duration_seconds_total` | Cumulative query execution time |

```bash
curl http://localhost:7878/metrics
```

---

## Content negotiation

Set the `Accept` header to choose the response format:

| Accept header | Format | Suitable for |
|---|---|---|
| `application/sparql-results+json` | SPARQL Results JSON | JavaScript apps, YASGUI |
| `application/sparql-results+xml` | SPARQL Results XML | Java/Jena clients |
| `text/csv` | CSV | Spreadsheets, pandas |
| `text/tab-separated-values` | TSV | CLI pipelines |
| `text/turtle` | Turtle | CONSTRUCT/DESCRIBE results |
| `application/n-triples` | N-Triples | Streaming pipelines |
| `application/ld+json` | JSON-LD | REST APIs, Linked Data Platform |

If no `Accept` header is set, the default is `application/sparql-results+json` for SELECT/ASK and `text/turtle` for CONSTRUCT/DESCRIBE.

```bash
# Get results as CSV
curl -G http://localhost:7878/sparql \
  -H "Accept: text/csv" \
  --data-urlencode "query=SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 5"

# Get CONSTRUCT results as JSON-LD
curl -G http://localhost:7878/sparql \
  -H "Accept: application/ld+json" \
  --data-urlencode "query=CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 5"
```

---

## Authentication

`pg_ripple_http` supports Bearer token and HTTP Basic authentication. Configure via environment variables:

| Variable | Description |
|----------|-------------|
| `PG_RIPPLE_AUTH_TOKEN` | If set, all requests must include `Authorization: Bearer <token>` |
| `PG_RIPPLE_AUTH_BASIC` | If set (format: `user:pass`), requests must include HTTP Basic auth |

When neither variable is set, authentication is disabled.

```bash
# Start with bearer token auth
PG_RIPPLE_AUTH_TOKEN=my-secret-token pg_ripple_http

# Query with auth
curl -G http://localhost:7878/sparql \
  -H "Authorization: Bearer my-secret-token" \
  --data-urlencode "query=SELECT * WHERE { ?s ?p ?o } LIMIT 5"
```

---

## CORS

Cross-Origin Resource Sharing headers are enabled by default via `tower-http`, allowing browser-based tools like YASGUI to query the endpoint directly.

---

## Connection pooling

The HTTP service uses `deadpool-postgres` for connection pooling. Configure via standard `PG*` environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `PGHOST` | `localhost` | PostgreSQL hostname |
| `PGPORT` | `5432` | PostgreSQL port |
| `PGDATABASE` | `postgres` | Database name |
| `PGUSER` | `postgres` | Database user |
| `PGPASSWORD` | (none) | Database password |
| `PG_RIPPLE_POOL_SIZE` | `10` | Maximum connections in the pool |

---

## Client examples

### Python (SPARQLWrapper)

```python
from SPARQLWrapper import SPARQLWrapper, JSON

sparql = SPARQLWrapper("http://localhost:7878/sparql")
sparql.setQuery("SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10")
sparql.setReturnFormat(JSON)
results = sparql.query().convert()

for row in results["results"]["bindings"]:
    print(row["s"]["value"], row["p"]["value"], row["o"]["value"])
```

### JavaScript (fetch)

```javascript
const response = await fetch('http://localhost:7878/sparql', {
  method: 'POST',
  headers: {
    'Content-Type': 'application/sparql-query',
    'Accept': 'application/sparql-results+json'
  },
  body: 'SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10'
});
const data = await response.json();
```

### curl

```bash
# SELECT
curl -G http://localhost:7878/sparql \
  --data-urlencode "query=SELECT ?s WHERE { ?s a <http://xmlns.com/foaf/0.1/Person> }"

# ASK
curl -G http://localhost:7878/sparql \
  --data-urlencode "query=ASK { <http://example.org/alice> ?p ?o }"

# CONSTRUCT as Turtle
curl -G http://localhost:7878/sparql \
  -H "Accept: text/turtle" \
  --data-urlencode "query=CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 100"
```

### Apache Jena (Java)

```java
QueryExecution qe = QueryExecution.service("http://localhost:7878/sparql")
    .query("SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10")
    .build();
ResultSet rs = qe.execSelect();
ResultSetFormatter.out(rs);
```
