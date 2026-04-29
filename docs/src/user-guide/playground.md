# Playground

The quickest way to try pg_ripple is with Docker. No PostgreSQL installation required.

## Start the sandbox

```bash
docker run --rm -p 5432:5432 \
  -e POSTGRES_PASSWORD=ripple \
  ghcr.io/grove/pg-ripple:latest
```

> **Note**: The sandbox container is configured for development/testing and uses trust authentication for external TCP connections. For production use, see [Installation](../getting-started/installation.md) and [Security](../operations/security.md).

Connect with any PostgreSQL client (no password required):

```bash
psql -h localhost -U postgres -d postgres
```

## Pre-loaded example dataset

The sandbox image includes a small FOAF-style dataset pre-loaded in the `examples` database:

```bash
psql -h localhost -U postgres -d examples
```

```sql
-- Who does Alice know?
SELECT * FROM pg_ripple.sparql('
  SELECT ?name WHERE {
    <https://example.org/alice> <https://xmlns.com/foaf/0.1/knows> ?person .
    ?person <https://xmlns.com/foaf/0.1/name> ?name
  }
');
```

```sql
-- Transitive: everyone reachable from Alice through knows+
SELECT * FROM pg_ripple.sparql('
  SELECT ?target WHERE {
    <https://example.org/alice>
      <https://xmlns.com/foaf/0.1/knows>+
    ?target
  }
');
```

```sql
-- Count people by organisation
SELECT * FROM pg_ripple.sparql('
  SELECT ?org (COUNT(?person) AS ?headcount) WHERE {
    ?person <https://xmlns.com/foaf/0.1/member> ?org
  } GROUP BY ?org ORDER BY DESC(?headcount)
');
```

## Try your own data

```sql
-- Load your own N-Triples
SELECT pg_ripple.load_ntriples('
<https://my.example/a> <https://my.example/p> <https://my.example/b> .
<https://my.example/b> <https://my.example/p> <https://my.example/c> .
');

-- Run a path query
SELECT * FROM pg_ripple.sparql('
  SELECT ?target WHERE {
    <https://my.example/a> <https://my.example/p>+ ?target
  }
');
```

## Building locally

To build the Docker image yourself:

```bash
git clone https://github.com/grove/pg-ripple.git
cd pg-ripple
docker build -t pg-ripple:local .
docker run --rm -p 5432:5432 -e POSTGRES_PASSWORD=ripple pg-ripple:local
```

## Next steps

- [Installation](../getting-started/installation.md) — install pg_ripple into your own PostgreSQL instance
- [Getting Started](../getting-started/hello-world.md) — five-minute tutorial
- [SPARQL Queries](sql-reference/sparql-query.md) — full SPARQL reference
