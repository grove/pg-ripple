# §2.8 APIs and Integration

## What and Why

pg_ripple's SQL functions are powerful, but most applications do not talk to PostgreSQL
directly. The **pg_ripple_http** companion service exposes a W3C-compliant SPARQL
Protocol endpoint over HTTP, so any SPARQL client, programming language, or tool can
query your knowledge graph.

This chapter covers:

- **pg_ripple_http**: the standalone SPARQL endpoint service.
- **Application code examples**: Python, JavaScript, and Java.
- **SPARQL federation**: query remote SPARQL endpoints from within pg_ripple.
- **Caching strategies**: plan cache, connection pooling, and result caching.

---

## How It Works

### pg_ripple_http Architecture

`pg_ripple_http` is a standalone Rust binary (not a PostgreSQL extension) that:

1. Connects to PostgreSQL via a **deadpool** connection pool.
2. Receives SPARQL queries via HTTP GET/POST (W3C SPARQL Protocol).
3. Calls `pg_ripple.sparql()`, `pg_ripple.sparql_construct()`, etc. via SQL.
4. Returns results in standard formats: SPARQL Results JSON/XML, Turtle, N-Triples, JSON-LD.
5. Exposes a `/rag` endpoint for AI retrieval.

### Supported Endpoints

| Method | Path | Content-Type | Description |
|---|---|---|---|
| GET | `/sparql?query=...` | Accept header | SPARQL query via URL parameter |
| POST | `/sparql` | `application/sparql-query` | SPARQL query in request body |
| POST | `/sparql` | `application/x-www-form-urlencoded` | SPARQL query as form parameter |
| POST | `/sparql` | `application/sparql-update` | SPARQL Update in request body |
| POST | `/rag` | `application/json` | RAG retrieval endpoint |
| GET | `/health` | `application/json` | Health check |
| GET | `/metrics` | `text/plain` | Prometheus metrics |

### Response Formats (Content Negotiation)

| Accept header | Format |
|---|---|
| `application/sparql-results+json` | SPARQL Results JSON (default for SELECT/ASK) |
| `application/sparql-results+xml` | SPARQL Results XML |
| `text/csv` | CSV |
| `text/tab-separated-values` | TSV |
| `text/turtle` | Turtle (for CONSTRUCT/DESCRIBE) |
| `application/n-triples` | N-Triples (for CONSTRUCT/DESCRIBE) |
| `application/ld+json` | JSON-LD (for CONSTRUCT/DESCRIBE) |

---

## Worked Examples

### Starting pg_ripple_http

```bash
# Set environment variables
export PG_RIPPLE_DATABASE_URL="postgresql://user:pass@localhost:5432/mydb"
export PG_RIPPLE_LISTEN="0.0.0.0:8080"
export PG_RIPPLE_AUTH_TOKEN="my-secret-token"  # optional

# Start the server
pg_ripple_http
```

Configuration via environment variables:

| Variable | Default | Description |
|---|---|---|
| `PG_RIPPLE_DATABASE_URL` | `postgresql://localhost/postgres` | PostgreSQL connection string |
| `PG_RIPPLE_LISTEN` | `127.0.0.1:8080` | Listen address and port |
| `PG_RIPPLE_AUTH_TOKEN` | (none) | Bearer token for authentication |
| `PG_RIPPLE_POOL_SIZE` | `10` | Connection pool size |
| `PG_RIPPLE_RATE_LIMIT` | `100` | Requests per second per IP |
| `PG_RIPPLE_CORS_ORIGIN` | `*` | CORS allowed origins |

### Querying via curl

**SPARQL SELECT via GET:**

```bash
curl -G http://localhost:8080/sparql \
  --data-urlencode 'query=PREFIX dct: <http://purl.org/dc/terms/> SELECT ?paper ?title WHERE { ?paper dct:title ?title } LIMIT 10' \
  -H "Accept: application/sparql-results+json"
```

**SPARQL SELECT via POST (body):**

```bash
curl -X POST http://localhost:8080/sparql \
  -H "Content-Type: application/sparql-query" \
  -H "Accept: application/sparql-results+json" \
  -d 'PREFIX dct: <http://purl.org/dc/terms/>
      PREFIX bibo: <http://purl.org/ontology/bibo/>
      SELECT ?paper ?title
      WHERE {
          ?paper a bibo:AcademicArticle ;
                 dct:title ?title .
      }
      ORDER BY ?title
      LIMIT 20'
```

**SPARQL CONSTRUCT as Turtle:**

```bash
curl -X POST http://localhost:8080/sparql \
  -H "Content-Type: application/sparql-query" \
  -H "Accept: text/turtle" \
  -d 'PREFIX dct: <http://purl.org/dc/terms/>
      PREFIX ex: <https://example.org/>
      CONSTRUCT { ?paper ex:hasTitle ?title }
      WHERE { ?paper dct:title ?title }'
```

**SPARQL CONSTRUCT as JSON-LD:**

```bash
curl -X POST http://localhost:8080/sparql \
  -H "Content-Type: application/sparql-query" \
  -H "Accept: application/ld+json" \
  -d 'PREFIX dct: <http://purl.org/dc/terms/>
      PREFIX ex: <https://example.org/>
      CONSTRUCT { ?paper ex:hasTitle ?title }
      WHERE { ?paper dct:title ?title }'
```

**SPARQL Update:**

```bash
curl -X POST http://localhost:8080/sparql \
  -H "Content-Type: application/sparql-update" \
  -d 'PREFIX ex: <https://example.org/>
      PREFIX dct: <http://purl.org/dc/terms/>
      INSERT DATA {
          ex:paper/new1 dct:title "A New Discovery" .
      }'
```

**RAG endpoint:**

```bash
curl -X POST http://localhost:8080/rag \
  -H "Content-Type: application/json" \
  -d '{
    "question": "What papers discuss knowledge graphs?",
    "sparql_filter": "?entity a <http://purl.org/ontology/bibo/AcademicArticle> .",
    "k": 5,
    "output_format": "jsonld"
  }'
```

The RAG response includes a `context` field with pre-formatted text for LLM prompts:

```json
{
  "results": [
    {
      "entity_iri": "https://example.org/paper/42",
      "label": "Knowledge Graphs in Practice",
      "context_json": {"@type": ["AcademicArticle"], "...": "..."},
      "distance": 0.12
    }
  ],
  "context": "Knowledge Graphs in Practice (AcademicArticle): A comprehensive survey..."
}
```

**Authentication** (when `PG_RIPPLE_AUTH_TOKEN` is set):

```bash
curl -X POST http://localhost:8080/sparql \
  -H "Authorization: Bearer my-secret-token" \
  -H "Content-Type: application/sparql-query" \
  -d 'SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 5'
```

### Python with psycopg2

Query pg_ripple directly from Python via SQL:

```python
import json
import psycopg2

conn = psycopg2.connect("dbname=mydb user=postgres")
cur = conn.cursor()

# Execute a SPARQL query
cur.execute("""
    SELECT * FROM pg_ripple.sparql(%s)
""", ("""
    PREFIX dct:  <http://purl.org/dc/terms/>
    PREFIX bibo: <http://purl.org/ontology/bibo/>
    
    SELECT ?paper ?title ?author
    WHERE {
        ?paper a bibo:AcademicArticle ;
               dct:title ?title ;
               dct:creator ?author .
    }
    ORDER BY ?title
    LIMIT 20
""",))

for row in cur.fetchall():
    result = json.loads(row[0])
    print(f"Paper: {result['paper']}")
    print(f"Title: {result['title']}")
    print(f"Author: {result['author']}")
    print()

# Load Turtle data
cur.execute("""
    SELECT pg_ripple.load_turtle(%s)
""", ("""
    @prefix ex: <https://example.org/> .
    @prefix dct: <http://purl.org/dc/terms/> .
    ex:paper/new dct:title "Loaded from Python" .
""",))
conn.commit()

# Export as JSON-LD
cur.execute("SELECT pg_ripple.export_jsonld()")
jsonld = json.loads(cur.fetchone()[0])
print(json.dumps(jsonld, indent=2))

cur.close()
conn.close()
```

### Python with SPARQLWrapper

Query the pg_ripple_http endpoint using the standard SPARQLWrapper library:

```python
from SPARQLWrapper import SPARQLWrapper, JSON, TURTLE

# Point to the pg_ripple_http endpoint
sparql = SPARQLWrapper("http://localhost:8080/sparql")

# SELECT query
sparql.setQuery("""
    PREFIX dct:  <http://purl.org/dc/terms/>
    PREFIX bibo: <http://purl.org/ontology/bibo/>
    
    SELECT ?paper ?title
    WHERE {
        ?paper a bibo:AcademicArticle ;
               dct:title ?title .
    }
    LIMIT 10
""")
sparql.setReturnFormat(JSON)
results = sparql.query().convert()

for binding in results["results"]["bindings"]:
    print(f"{binding['paper']['value']}: {binding['title']['value']}")

# CONSTRUCT query as Turtle
sparql.setQuery("""
    PREFIX dct: <http://purl.org/dc/terms/>
    PREFIX ex:  <https://example.org/>
    
    CONSTRUCT { ?paper ex:hasTitle ?title }
    WHERE { ?paper dct:title ?title }
""")
sparql.setReturnFormat(TURTLE)
turtle_output = sparql.query().convert()
print(turtle_output.decode("utf-8"))
```

### JavaScript with pg

Query pg_ripple directly from Node.js:

```javascript
const { Client } = require('pg');

async function main() {
    const client = new Client({ connectionString: 'postgresql://localhost/mydb' });
    await client.connect();

    // SPARQL SELECT
    const { rows } = await client.query(
        `SELECT * FROM pg_ripple.sparql($1)`,
        [`
            PREFIX dct:  <http://purl.org/dc/terms/>
            PREFIX bibo: <http://purl.org/ontology/bibo/>
            
            SELECT ?paper ?title
            WHERE {
                ?paper a bibo:AcademicArticle ;
                       dct:title ?title .
            }
            LIMIT 10
        `]
    );

    for (const row of rows) {
        const result = row.result;
        console.log(`Paper: ${result.paper}, Title: ${result.title}`);
    }

    // Load Turtle
    const loadResult = await client.query(
        `SELECT pg_ripple.load_turtle($1)`,
        [`
            @prefix ex: <https://example.org/> .
            @prefix dct: <http://purl.org/dc/terms/> .
            ex:paper/fromjs dct:title "Loaded from JavaScript" .
        `]
    );
    console.log(`Loaded: ${loadResult.rows[0].load_turtle} triples`);

    // Export JSON-LD
    const jsonldResult = await client.query(`SELECT pg_ripple.export_jsonld()`);
    console.log(JSON.stringify(jsonldResult.rows[0].export_jsonld, null, 2));

    await client.end();
}

main().catch(console.error);
```

### JavaScript with fetch (HTTP endpoint)

```javascript
async function sparqlQuery(query) {
    const response = await fetch('http://localhost:8080/sparql', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/sparql-query',
            'Accept': 'application/sparql-results+json',
        },
        body: query,
    });
    return response.json();
}

const results = await sparqlQuery(`
    PREFIX dct: <http://purl.org/dc/terms/>
    SELECT ?paper ?title
    WHERE { ?paper dct:title ?title }
    LIMIT 10
`);

for (const binding of results.results.bindings) {
    console.log(`${binding.paper.value}: ${binding.title.value}`);
}
```

### Java with JDBC

```java
import java.sql.*;
import org.json.JSONObject;

public class PgRippleExample {
    public static void main(String[] args) throws Exception {
        Connection conn = DriverManager.getConnection(
            "jdbc:postgresql://localhost:5432/mydb", "postgres", "password"
        );

        // SPARQL SELECT
        PreparedStatement stmt = conn.prepareStatement(
            "SELECT * FROM pg_ripple.sparql(?)"
        );
        stmt.setString(1,
            "PREFIX dct: <http://purl.org/dc/terms/> " +
            "PREFIX bibo: <http://purl.org/ontology/bibo/> " +
            "SELECT ?paper ?title " +
            "WHERE { " +
            "    ?paper a bibo:AcademicArticle ; " +
            "           dct:title ?title . " +
            "} LIMIT 10"
        );

        ResultSet rs = stmt.executeQuery();
        while (rs.next()) {
            String jsonStr = rs.getString("result");
            JSONObject result = new JSONObject(jsonStr);
            System.out.println("Paper: " + result.getString("paper"));
            System.out.println("Title: " + result.getString("title"));
        }
        rs.close();
        stmt.close();

        // Load Turtle
        PreparedStatement loadStmt = conn.prepareStatement(
            "SELECT pg_ripple.load_turtle(?)"
        );
        loadStmt.setString(1,
            "@prefix ex: <https://example.org/> .\n" +
            "@prefix dct: <http://purl.org/dc/terms/> .\n" +
            "ex:paper/fromjava dct:title \"Loaded from Java\" .\n"
        );
        ResultSet loadRs = loadStmt.executeQuery();
        if (loadRs.next()) {
            System.out.println("Loaded: " + loadRs.getLong(1) + " triples");
        }
        loadRs.close();
        loadStmt.close();

        conn.close();
    }
}
```

---

## SPARQL Federation

pg_ripple can query remote SPARQL endpoints from within a SPARQL query using the
`SERVICE` keyword. This lets you join local data with remote datasets like Wikidata
or DBpedia.

### Querying a Remote SPARQL Endpoint

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct:    <http://purl.org/dc/terms/>
PREFIX rdfs:   <http://www.w3.org/2000/01/rdf-schema#>
PREFIX wd:     <http://www.wikidata.org/entity/>
PREFIX wdt:    <http://www.wikidata.org/prop/direct/>

SELECT ?paper ?title ?wikidataLabel
WHERE {
    ?paper dct:title ?title ;
           dct:subject ?topic .
    
    SERVICE <https://query.wikidata.org/sparql> {
        ?topic rdfs:label ?wikidataLabel .
        FILTER (LANG(?wikidataLabel) = "en")
    }
}
LIMIT 10
');
```

### Vector Federation

Register external vector services for federated similarity search
(see [Vector Federation](../user-guide/vector-federation.md) for full details):

```sql
-- Register a Qdrant endpoint
SELECT pg_ripple.register_vector_endpoint(
    'https://qdrant.internal:6333',
    'qdrant'
);

-- Register a Weaviate endpoint
SELECT pg_ripple.register_vector_endpoint(
    'https://weaviate.internal:8080',
    'weaviate'
);
```

```admonish tip
Federation queries add network latency. Set timeouts to prevent slow remote
endpoints from blocking local queries:
```sql
SET pg_ripple.vector_federation_timeout_ms = 5000;
```
```

---

## Common Patterns

### Pattern: Connection Pooling

For high-traffic applications, use a connection pooler (PgBouncer, pgcat) between your
application and PostgreSQL:

```
App → PgBouncer (port 6432) → PostgreSQL (port 5432)
```

pg_ripple_http uses its own connection pool internally (configurable via `PG_RIPPLE_POOL_SIZE`).

### Pattern: Result Caching

Cache SPARQL results at the application level for frequently-repeated queries:

```python
import json
import hashlib
import redis
import psycopg2

cache = redis.Redis()

def cached_sparql(query, ttl=300):
    key = f"sparql:{hashlib.sha256(query.encode()).hexdigest()}"
    cached = cache.get(key)
    if cached:
        return json.loads(cached)

    conn = psycopg2.connect("dbname=mydb")
    cur = conn.cursor()
    cur.execute("SELECT * FROM pg_ripple.sparql(%s)", (query,))
    results = [json.loads(row[0]) for row in cur.fetchall()]
    cur.close()
    conn.close()

    cache.setex(key, ttl, json.dumps(results))
    return results
```

### Pattern: SPARQL Views for Pre-Computed Results

For dashboard queries that run frequently, create SPARQL views (requires pg_trickle):

```sql
-- Create a pre-computed view of paper counts per institution
SELECT pg_ripple.create_sparql_view(
    'papers_by_institution',
    'PREFIX dct: <http://purl.org/dc/terms/>
     PREFIX schema: <https://schema.org/>
     PREFIX foaf: <http://xmlns.com/foaf/0.1/>
     SELECT ?inst ?instName (COUNT(DISTINCT ?paper) AS ?count)
     WHERE {
         ?paper dct:creator ?author .
         ?author schema:affiliation ?inst .
         ?inst foaf:name ?instName .
     }
     GROUP BY ?inst ?instName',
    '30s',
    true
);

-- Query the view directly (instant, no SPARQL parsing)
SELECT * FROM pg_ripple.papers_by_institution;
```

### Pattern: Prometheus Monitoring

pg_ripple_http exposes Prometheus metrics at `/metrics`:

```bash
curl http://localhost:8080/metrics
```

Metrics include:

- `pg_ripple_http_requests_total` — total request count by endpoint and status
- `pg_ripple_http_request_duration_seconds` — request latency histogram
- `pg_ripple_http_active_connections` — current active connections

---

## Performance and Trade-offs

### Direct SQL vs HTTP Endpoint

| Access method | Latency overhead | Best for |
|---|---|---|
| Direct SQL (psycopg2, JDBC) | None | Server-side applications, ETL |
| pg_ripple_http | ~1-5ms per request | Web applications, REST APIs, federated queries |

### Connection Pool Sizing

Rule of thumb: set pool size to `2 * CPU cores` for OLTP workloads. For SPARQL-heavy
analytics, `4 * CPU cores` may be better:

```bash
export PG_RIPPLE_POOL_SIZE=20
```

### Rate Limiting

pg_ripple_http includes built-in rate limiting to prevent abuse:

```bash
export PG_RIPPLE_RATE_LIMIT=100  # requests per second per IP
```

For public-facing endpoints, combine with a reverse proxy (nginx, Caddy) for additional
protection.

### CORS Configuration

For browser-based applications:

```bash
export PG_RIPPLE_CORS_ORIGIN="https://myapp.example.com"
```

Set to `*` for development; restrict to specific origins in production.

---

## Gotchas and Debugging

### Authentication Errors

If `PG_RIPPLE_AUTH_TOKEN` is set, all requests must include the `Authorization` header:

```
HTTP 401: Missing or invalid authorization token
```

Fix: include `Authorization: Bearer <token>` in the request headers.

### Connection Refused

If pg_ripple_http cannot connect to PostgreSQL:

```
Error: connection refused (os error 61)
```

Fix: check `PG_RIPPLE_DATABASE_URL` and ensure PostgreSQL is running and accepting connections.

### Content-Type Negotiation

If you get unexpected response formats, check the `Accept` header. pg_ripple_http uses
content negotiation:

```bash
# Explicitly request JSON results
curl -H "Accept: application/sparql-results+json" ...

# Explicitly request Turtle for CONSTRUCT
curl -H "Accept: text/turtle" ...
```

### Federation Timeouts

Remote SPARQL endpoints can be slow. If federation queries time out:

```sql
-- Increase the timeout
SET pg_ripple.vector_federation_timeout_ms = 30000;
```

For SPARQL federation (SERVICE keyword), pg_ripple uses PostgreSQL's
`statement_timeout` for the overall query:

```sql
SET statement_timeout = '60s';
```

### Health Check

Use the `/health` endpoint for load balancer configuration:

```bash
curl http://localhost:8080/health
# Returns: {"status": "ok", "pool_size": 10, "pool_available": 8}
```

---

## Next Steps

- **[§2.3 Querying with SPARQL](../features/querying-with-sparql.md)** — SPARQL query reference for the queries you send via APIs.
- **[§2.7 AI Retrieval and GraphRAG](../features/ai-retrieval-graph-rag.md)** — RAG endpoint details and LLM integration.
- **[§2.6 Exporting and Sharing](../features/exporting-and-sharing.md)** — export formats returned by the HTTP endpoint.
