# HTTP API Reference

`pg_ripple_http` is a standalone Rust binary that exposes a W3C-compliant SPARQL HTTP endpoint and a RAG retrieval endpoint (v0.28.0) for pg_ripple.

---

## Endpoints

| Method | Path | Description |
|---|---|---|
| `GET` | `/sparql` | SPARQL 1.1 Protocol query endpoint |
| `POST` | `/sparql` | SPARQL 1.1 Protocol query/update endpoint |
| `POST` | `/rag` | RAG retrieval endpoint (v0.28.0) |
| `GET` | `/health` | Health check |
| `GET` | `/metrics` | Prometheus-format metrics |

---

## SPARQL Endpoint

Conforms to the [W3C SPARQL 1.1 Protocol](https://www.w3.org/TR/sparql11-protocol/). See `pg_ripple_http/README.md` for full SPARQL endpoint documentation.

---

## `POST /rag` (v0.28.0)

Execute a RAG retrieval query. The endpoint calls `pg_ripple.rag_retrieve()` and returns both structured JSON results and a concatenated plain-text context ready for use as an LLM system prompt.

### Request

**Content-Type:** `application/json`

**Authorization:** `Bearer <token>` (if `PG_RIPPLE_HTTP_AUTH_TOKEN` is set)

**Body:**

```json
{
  "question": "what treats headaches?",
  "sparql_filter": "?entity a <https://pharma.example/Drug>",
  "k": 5,
  "model": "text-embedding-3-small",
  "output_format": "jsonb"
}
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `question` | string | **yes** | — | Natural-language question to embed and search |
| `sparql_filter` | string | no | `null` | SPARQL WHERE clause fragment filtering candidates |
| `k` | integer | no | `5` | Number of results |
| `model` | string | no | `null` | Override `pg_ripple.embedding_model` GUC |
| `output_format` | string | no | `"jsonb"` | `"jsonb"` or `"jsonld"` |

### Response

**Content-Type:** `application/json`

**200 OK:**

```json
{
  "results": [
    {
      "entity_iri": "https://pharma.example/aspirin",
      "label": "aspirin",
      "context_json": {
        "label": "aspirin",
        "types": ["Drug", "NSAID"],
        "properties": [
          {"predicate": "approvedBy", "object": "FDA"},
          {"predicate": "treats", "object": "headache"}
        ],
        "neighbors": [
          {"iri": "https://pharma.example/ibuprofen", "label": "ibuprofen"},
          {"iri": "https://pharma.example/naproxen", "label": "naproxen"}
        ],
        "contextText": "aspirin. Type: NSAID, Drug. Related: headache, fever, inflammation"
      },
      "distance": 0.12
    }
  ],
  "context": "aspirin. Type: NSAID, Drug. Related: headache, fever, inflammation\n\nibuprofen. Type: Drug. Related: pain, inflammation"
}
```

The `context` field is a concatenated plain-text summary of all results — use it directly as an LLM system prompt.

**503 Service Unavailable:** PostgreSQL connection pool unavailable.

**400 Bad Request:** Invalid JSON body or missing `question` field.

**401 Unauthorized:** Missing or invalid Bearer token.

### JSON-LD Output

When `output_format` is `"jsonld"`, `context_json` includes `@type` and `@context` keys:

```json
{
  "@context": {
    "rdfs": "http://www.w3.org/2000/01/rdf-schema#",
    "ex": "https://pharma.example/"
  },
  "@id": "https://pharma.example/aspirin",
  "@type": ["https://pharma.example/Drug"],
  "rdfs:label": "aspirin",
  "properties": [...],
  "neighbors": [...],
  "contextText": "aspirin. Type: NSAID. Related: headache"
}
```

### curl Example

```bash
# Basic RAG retrieval
curl -X POST http://localhost:7878/rag \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"question": "what treats headaches?", "k": 5}'

# With SPARQL filter and JSON-LD output
curl -X POST http://localhost:7878/rag \
  -H "Content-Type: application/json" \
  -d '{
    "question": "FDA-approved NSAIDs",
    "sparql_filter": "?entity <https://pharma.example/approvedBy> <https://pharma.example/FDA>",
    "k": 10,
    "output_format": "jsonld"
  }'
```

---

## Configuration

| Environment Variable | Default | Description |
|---|---|---|
| `PG_RIPPLE_HTTP_PG_URL` | `postgresql://localhost/postgres` | PostgreSQL connection URL |
| `PG_RIPPLE_HTTP_PORT` | `7878` | Listening port |
| `PG_RIPPLE_HTTP_POOL_SIZE` | `16` | Connection pool size |
| `PG_RIPPLE_HTTP_AUTH_TOKEN` | (none) | Bearer token for authentication |
| `PG_RIPPLE_HTTP_RATE_LIMIT` | `0` | Per-IP rate limit (requests/sec; 0 = unlimited) |
| `PG_RIPPLE_HTTP_CORS_ORIGINS` | `*` | Comma-separated allowed CORS origins |

---

## Security

- All HTTP endpoints respect the `PG_RIPPLE_HTTP_AUTH_TOKEN` Bearer token
- SQL injection is prevented by parameterized queries
- Rate limiting is per source IP (configurable via `PG_RIPPLE_HTTP_RATE_LIMIT`)
- HTTPS termination should be handled by a reverse proxy (nginx, Caddy, etc.)
