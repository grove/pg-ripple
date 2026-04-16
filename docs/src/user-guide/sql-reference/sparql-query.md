# SPARQL Queries

pg_ripple executes SPARQL 1.1 queries by compiling them to SQL that runs natively inside the PostgreSQL engine. The generated SQL is visible via `sparql_explain()`.

## sparql

```sql
pg_ripple.sparql(query TEXT) RETURNS TABLE(…)
```

Executes a SPARQL SELECT query and returns results as a relational table. Each projected variable becomes a `TEXT` column. Values are returned in N-Triples notation for IRIs and blank nodes, and literal notation for literals.

```sql
SELECT name, age
FROM pg_ripple.sparql('
  SELECT ?name ?age WHERE {
    ?person <https://example.org/name> ?name .
    OPTIONAL { ?person <https://example.org/age> ?age }
  }
');
```

> **Note**: The column names in the returned table match the SPARQL variable names without the `?` prefix.

---

## sparql_ask

```sql
pg_ripple.sparql_ask(query TEXT) RETURNS BOOLEAN
```

Executes a SPARQL ASK query and returns `true` if at least one solution exists.

```sql
SELECT pg_ripple.sparql_ask(
    'ASK { <https://example.org/alice> <https://example.org/knows> ?x }'
);
```

---

## sparql_explain

```sql
pg_ripple.sparql_explain(query TEXT, verbose BOOLEAN DEFAULT FALSE) RETURNS TEXT
```

Returns the SQL generated for a SPARQL query without executing it. Useful for debugging and performance analysis.

```sql
SELECT pg_ripple.sparql_explain(
    'SELECT ?name WHERE { ?p <https://example.org/name> ?name }',
    false
);
```

---

## Supported SPARQL 1.1 features

### SELECT

```sparql
SELECT ?s ?p WHERE { ?s ?p <https://example.org/bob> }
SELECT DISTINCT ?s WHERE { ?s <ex:knows> ?o }
SELECT ?s WHERE { ?s <ex:knows> ?o } ORDER BY ?s LIMIT 10 OFFSET 5
```

### FILTER

```sparql
SELECT ?name WHERE {
  ?p <ex:name> ?name .
  FILTER(STRLEN(?name) > 3)
}

SELECT ?age WHERE {
  ?p <ex:age> ?age .
  FILTER(?age >= 18 && ?age < 65)
}
```

### OPTIONAL (LeftJoin)

```sparql
SELECT ?person ?name ?email WHERE {
  ?person <ex:worksAt> <ex:acme> .
  OPTIONAL { ?person <ex:name> ?name }
  OPTIONAL { ?person <ex:email> ?email }
}
```

### UNION / MINUS

```sparql
-- UNION
SELECT ?contact WHERE {
  { ?alice <ex:knows> ?contact } UNION { ?bob <ex:knows> ?contact }
}

-- MINUS (anti-join)
SELECT ?person WHERE {
  ?person <ex:worksAt> ?company .
  MINUS { ?person <ex:worksAt> <ex:acme> }
}
```

### Aggregates and GROUP BY

```sparql
SELECT ?company (COUNT(?person) AS ?headcount) WHERE {
  ?person <ex:worksAt> ?company
} GROUP BY ?company HAVING (COUNT(?person) >= 2)
```

Supported aggregate functions: `COUNT`, `SUM`, `AVG`, `MIN`, `MAX`, `GROUP_CONCAT`.

### Subqueries

```sparql
SELECT ?company ?headcount WHERE {
  {
    SELECT ?company (COUNT(?p) AS ?headcount) WHERE {
      ?p <ex:worksAt> ?company
    } GROUP BY ?company
  }
  FILTER(?headcount >= 2)
}
```

### BIND / VALUES

```sparql
-- BIND
SELECT ?person ?label WHERE {
  ?person <ex:worksAt> ?company .
  BIND(<ex:employee> AS ?label)
}

-- VALUES (inline data)
SELECT ?person ?company WHERE {
  VALUES ?person { <ex:alice> <ex:bob> }
  ?person <ex:worksAt> ?company
}
```

### Property paths

```sparql
-- OneOrMore (+): transitive closure
SELECT ?target WHERE { <ex:alice> <ex:knows>+ ?target }

-- ZeroOrMore (*): transitive closure including identity
SELECT ?target WHERE { <ex:alice> <ex:follows>* ?target }

-- ZeroOrOne (?): direct or identity
SELECT ?target WHERE { <ex:alice> <ex:follows>? ?target }

-- Sequence (/)
SELECT ?target WHERE { <ex:alice> <ex:knows>/<ex:knows> ?target }

-- Alternative (|)
SELECT ?target WHERE { <ex:alice> (<ex:knows>|<ex:follows>) ?target }

-- Inverse (^)
SELECT ?who WHERE { ?who ^<ex:knows> <ex:bob> }
```

Property paths compile to PostgreSQL `WITH RECURSIVE` CTEs with the PG18 `CYCLE` clause for hash-based cycle detection. See [max_path_depth](../configuration.md#max_path_depth) to limit traversal depth.

### Named graphs

```sparql
SELECT ?s ?p ?o WHERE {
  GRAPH <https://example.org/graph1> { ?s ?p ?o }
}
```

### ASK

```sparql
ASK { <ex:alice> <ex:knows> <ex:bob> }
```

---

## Plan cache

Compiled SPARQL→SQL plans are cached per-backend in an LRU cache (configurable via [`pg_ripple.plan_cache_size`](../configuration.md#plan_cache_size)). Repeated identical queries skip recompilation.

The cache key includes the query text and the current value of `pg_ripple.max_path_depth`. Changing the GUC invalidates cached path query plans.

---

## sparql_construct

```sql
pg_ripple.sparql_construct(query TEXT) RETURNS SETOF JSONB
```

Executes a SPARQL CONSTRUCT query and returns the constructed triples as JSONB objects. Each result row has three keys: `"s"` (subject), `"p"` (predicate), and `"o"` (object), all in N-Triples notation.

### Explicit template form

```sql
SELECT *
FROM pg_ripple.sparql_construct('
    CONSTRUCT { ?b <https://example.org/knownBy> ?a }
    WHERE { ?a <https://example.org/knows> ?b }
');
-- Returns: {"s": "<https://...bob>", "p": "<https://...knownBy>", "o": "<https://...alice>"}
```

### CONSTRUCT WHERE (bare form)

The `CONSTRUCT WHERE` shorthand returns the matched triples directly:

```sql
SELECT *
FROM pg_ripple.sparql_construct('
    CONSTRUCT WHERE { <https://example.org/alice> <https://example.org/knows> ?o }
');
```

---

## sparql_describe

```sql
pg_ripple.sparql_describe(query TEXT, strategy TEXT DEFAULT current_setting('pg_ripple.describe_strategy'))
    RETURNS SETOF JSONB
```

Executes a SPARQL DESCRIBE query and returns the description of the named resources as JSONB triples `{s, p, o}`.

```sql
-- Describe a single resource (CBD algorithm)
SELECT *
FROM pg_ripple.sparql_describe(
    'DESCRIBE <https://example.org/alice>'
);

-- Describe all people (resources identified by a WHERE pattern)
SELECT *
FROM pg_ripple.sparql_describe(
    'DESCRIBE ?person WHERE { ?person a <https://example.org/Person> }'
);
```

### describe_strategy GUC

`pg_ripple.describe_strategy` (default: `'cbd'`) sets the default expansion algorithm:

| Value | Algorithm | Description |
|---|---|---|
| `'cbd'` | Concise Bounded Description | All outgoing arcs; recursively expands blank node objects |
| `'scbd'` | Symmetric CBD | CBD + all incoming arcs to the named resource |
| `'simple'` | Simple description | Outgoing arcs only; no blank-node recursion |

```sql
-- Use SCBD for this session
SET pg_ripple.describe_strategy = 'scbd';

SELECT * FROM pg_ripple.sparql_describe('DESCRIBE <https://example.org/alice>');
```

You can also pass the strategy as the second argument to `sparql_describe`:

```sql
SELECT * FROM pg_ripple.sparql_describe(
    'DESCRIBE <https://example.org/alice>',
    'scbd'
);
```

> **Note**: CONSTRUCT and DESCRIBE return JSONB in v0.5.1. Turtle and JSON-LD serialization output are planned for v0.9.0.

---

## HTTP Protocol Endpoint

pg_ripple includes a companion HTTP service (`pg_ripple_http`) that implements the W3C SPARQL 1.1 Protocol, allowing standard SPARQL clients to connect without any pg_ripple-specific drivers.

### Starting the HTTP service

```bash
export PG_RIPPLE_HTTP_PG_URL="postgresql://user:pass@localhost/mydb"
export PG_RIPPLE_HTTP_PORT=7878
pg_ripple_http
```

### Configuration

| Environment variable | Default | Description |
|---|---|---|
| `PG_RIPPLE_HTTP_PG_URL` | `postgresql://localhost/postgres` | PostgreSQL connection string |
| `PG_RIPPLE_HTTP_PORT` | `7878` | HTTP listen port |
| `PG_RIPPLE_HTTP_POOL_SIZE` | `16` | Connection pool size |
| `PG_RIPPLE_HTTP_AUTH_TOKEN` | *(none)* | Bearer/Basic auth token |
| `PG_RIPPLE_HTTP_CORS_ORIGINS` | `*` | Comma-separated CORS origins |
| `PG_RIPPLE_HTTP_RATE_LIMIT` | `0` | Rate limit (0 = unlimited) |

### SPARQL 1.1 Protocol conformance

The endpoint at `/sparql` supports all standard request forms:

- `GET /sparql?query=...` (URL-encoded query)
- `POST /sparql` with `Content-Type: application/sparql-query`
- `POST /sparql` with `Content-Type: application/sparql-update`
- `POST /sparql` with `Content-Type: application/x-www-form-urlencoded` (`query=...` or `update=...`)

### Accept header formats

| Accept header | Used for | MIME type |
|---|---|---|
| `application/sparql-results+json` | SELECT, ASK (default) | JSON Results |
| `application/sparql-results+xml` | SELECT, ASK | XML Results |
| `text/csv` | SELECT | CSV |
| `text/tab-separated-values` | SELECT | TSV |
| `text/turtle` | CONSTRUCT, DESCRIBE (default) | Turtle |
| `application/n-triples` | CONSTRUCT, DESCRIBE | N-Triples |
| `application/ld+json` | CONSTRUCT, DESCRIBE | JSON-LD |

### Examples

```bash
# SELECT query
curl -G http://localhost:7878/sparql \
  --data-urlencode "query=SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10"

# ASK query with JSON results
curl -G http://localhost:7878/sparql \
  --data-urlencode "query=ASK { <http://example.org/alice> ?p ?o }"

# CONSTRUCT query with Turtle output
curl -H "Accept: text/turtle" -G http://localhost:7878/sparql \
  --data-urlencode "query=CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 10"

# SPARQL Update via POST
curl -X POST http://localhost:7878/sparql \
  -H "Content-Type: application/sparql-update" \
  -d "INSERT DATA { <http://example.org/s> <http://example.org/p> \"value\" }"

# Health check
curl http://localhost:7878/health

# Prometheus metrics
curl http://localhost:7878/metrics
```

### Docker

Use Docker Compose to run PostgreSQL with pg_ripple and the HTTP endpoint together:

```bash
docker compose up -d
curl http://localhost:7878/health
```

### Connecting SPARQL tools

The `/sparql` endpoint is compatible with standard SPARQL tools:

- **YASGUI**: Set endpoint URL to `http://localhost:7878/sparql`
- **Python SPARQLWrapper**: `sparql = SPARQLWrapper("http://localhost:7878/sparql")`
- **Apache Jena**: `QueryExecutionFactory.sparqlService("http://localhost:7878/sparql", query)`
- **Protege**: Add SPARQL tab, set endpoint to `http://localhost:7878/sparql`

