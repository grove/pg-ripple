# Vector Federation

pg_ripple v0.28.0 introduces **vector federation**: registered external vector services (Weaviate, Qdrant, Pinecone, or another pgvector instance) can be queried alongside the local triple store.

---

## Why Vector Federation?

Organizations often have multiple vector databases: a Qdrant cluster holding product embeddings, a Weaviate instance for customer data, and pg_ripple for the knowledge graph. Vector federation lets you blend all three sources in a single `pg_ripple.hybrid_search()` call without moving data.

---

## Supported Service Types

| `api_type` | Notes |
|---|---|
| `pgvector` | Remote PostgreSQL + pgvector instance |
| `weaviate` | Weaviate v1 GraphQL endpoint (`/v1/graphql`) |
| `qdrant` | Qdrant v1 REST API (`/collections/{name}/points/search`) |
| `pinecone` | Pinecone v1 REST API (`/query`) |

---

## Registering an Endpoint

```sql
-- Register a Qdrant instance.
SELECT pg_ripple.register_vector_endpoint(
    'https://qdrant.internal:6333',
    'qdrant'
);

-- Register a Weaviate instance.
SELECT pg_ripple.register_vector_endpoint(
    'https://weaviate.internal:8080',
    'weaviate'
);

-- Register a Pinecone index.
SELECT pg_ripple.register_vector_endpoint(
    'https://my-index-abc123.pinecone.io',
    'pinecone'
);
```

Registrations are idempotent — calling `register_vector_endpoint()` twice with the same URL is safe.

Endpoints are stored in `_pg_ripple.vector_endpoints`:

```sql
SELECT url, api_type, enabled, registered_at
FROM _pg_ripple.vector_endpoints;
```

---

## Timeout Configuration

The federation timeout applies to all outbound HTTP calls to external vector services:

```sql
-- Set 10-second timeout.
SET pg_ripple.vector_federation_timeout_ms = 10000;
```

---

## Security Considerations

- Endpoints are stored in an internal table accessible only to `pg_ripple` schema users.
- HTTPS is recommended for all external endpoints.
- API keys for external services are not stored by pg_ripple; configure them via environment variables or secrets management in your application layer.
- The `pg_ripple.vector_federation_timeout_ms` GUC prevents runaway federated queries from blocking the database.

---

## Catalog Table Reference

### `_pg_ripple.vector_endpoints`

| Column | Type | Description |
|---|---|---|
| `url` | `TEXT` | Endpoint base URL (primary key) |
| `api_type` | `TEXT` | One of: `pgvector`, `weaviate`, `qdrant`, `pinecone` |
| `enabled` | `BOOLEAN` | Whether this endpoint is active |
| `registered_at` | `TIMESTAMPTZ` | When the endpoint was registered |

---

## GUC Reference

| GUC | Default | Description |
|---|---|---|
| `pg_ripple.vector_federation_timeout_ms` | `5000` | HTTP timeout for federated vector endpoint queries (milliseconds) |
