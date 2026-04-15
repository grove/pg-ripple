# Playground

The quickest way to try pg_ripple is with Docker. No PostgreSQL installation required.

## Start the sandbox

```bash
docker run --rm -p 5432:5432 \
  -e POSTGRES_PASSWORD=ripple \
  ghcr.io/grove/pg-ripple:0.5.0
```

> **Image note**: The Docker image is published as part of each release. If the image is not yet available for your version, see [Building locally](#building-locally) below.

Connect with any PostgreSQL client:

```bash
psql -h localhost -U postgres -d postgres
# password: ripple
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

- [Installation](installation.md) — install pg_ripple into your own PostgreSQL instance
- [Getting Started](getting-started.md) — five-minute tutorial
- [SPARQL Queries](sql-reference/sparql-query.md) — full SPARQL reference
