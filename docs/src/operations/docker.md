# Docker Deployment

## Batteries-Included Image

The `ghcr.io/grove/pg_ripple:<version>` image bundles four extensions in a
single container:

| Extension | Purpose |
|-----------|---------|
| `pg_ripple` | RDF triple store with native SPARQL |
| `PostGIS` | Geospatial queries via GeoSPARQL |
| `pgvector` | Vector similarity search for hybrid SPARQL + semantic |
| `pg_trickle` (optional) | Incremental materialised SPARQL views |

No additional setup is needed — simply start the container and `CREATE EXTENSION`.

## Quick Start

```bash
docker run --rm -p 5432:5432 \
  -e POSTGRES_PASSWORD=ripple \
  ghcr.io/grove/pg_ripple:0.54.0
```

```bash
psql -h localhost -U postgres -c "CREATE EXTENSION pg_ripple CASCADE;"
psql -h localhost -U postgres \
  -c "SELECT pg_ripple.load_ntriples('<https://example.org/s> <https://example.org/p> <https://example.org/o> .');"
```

Enable optional extensions:

```sql
CREATE EXTENSION postgis;   -- GeoSPARQL functions
CREATE EXTENSION vector;    -- hybrid vector + SPARQL search
```

## Docker Compose

The repository ships a `docker-compose.yml` that starts pg_ripple and the
SPARQL HTTP service together:

```bash
docker compose up -d
curl http://localhost:7878/health
curl -G http://localhost:7878/sparql \
  --data-urlencode "query=SELECT * WHERE { ?s ?p ?o } LIMIT 10"
```

## Pre-Installed Extension Versions

| Extension | Version | Notes |
|-----------|---------|-------|
| pg_ripple | 0.54.0 | This release |
| PostGIS | 3.4.3 | Geospatial support |
| pgvector | 0.7.4 | Vector index (HNSW / IVFFlat) |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_PASSWORD` | *(required)* | Superuser password |
| `POSTGRES_DB` | `postgres` | Database to create |
| `POSTGRES_USER` | `postgres` | Superuser name |

The SPARQL HTTP service (`pg_ripple_http`) also accepts:

| Variable | Default | Description |
|----------|---------|-------------|
| `PG_RIPPLE_HTTP_PG_URL` | `postgresql://postgres:…@localhost/postgres` | Connection string |
| `PG_RIPPLE_HTTP_PORT` | `7878` | Listening port |
| `PG_RIPPLE_HTTP_POOL_SIZE` | `8` | Connection pool size |
| `PG_RIPPLE_HTTP_CORS_ORIGINS` | `*` | CORS allowed origins |

## Example: GeoSPARQL Query

```sql
CREATE EXTENSION postgis;

-- Load a geo triple
SELECT pg_ripple.load_ntriples(
  '<https://example.org/Berlin> <http://www.opengis.net/ont/geosparql#asWKT> "POINT(13.405 52.52)"^^<http://www.opengis.net/ont/geosparql#wktLiteral> .'
);

-- Query via SPARQL
SELECT * FROM pg_ripple.sparql(
  'SELECT ?city ?wkt WHERE { ?city <http://www.opengis.net/ont/geosparql#asWKT> ?wkt }'
);
```

## Example: Hybrid Vector + SPARQL Search

```sql
CREATE EXTENSION vector;

-- Create an embedding for a resource
INSERT INTO _pg_ripple.embeddings (subject_id, embedding)
SELECT id, '[0.1, 0.2, ...]'::vector
FROM _pg_ripple.dictionary
WHERE value = 'https://example.org/Berlin';

-- Hybrid search: semantic similarity + SPARQL filter
SELECT * FROM pg_ripple.hybrid_search(
  query_embedding := '[0.1, 0.2, ...]'::vector,
  sparql_filter   := '?s <http://schema.org/type> <http://schema.org/City>',
  k               := 10
);
```

## Publishing to GHCR

Every release is automatically published to GitHub Container Registry via the
release GitHub Actions workflow. You can also build the batteries-included image
locally:

```bash
docker build --tag my-pg-ripple:local .
```
