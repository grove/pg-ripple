# SPARQL Patterns

Best practices for writing efficient SPARQL queries against pg_ripple.

## Star patterns — let the planner collapse joins

When a single subject has multiple predicates, express them as separate triple patterns in the same WHERE clause. The SQL generator collapses these into a single scan with multiple table joins:

```sparql
-- Good: star pattern — all predicates share the same subject variable
SELECT ?person ?name ?age WHERE {
  ?person <ex:worksAt> <ex:acme> .
  ?person <ex:name>    ?name .
  OPTIONAL { ?person <ex:age> ?age }
}

-- Avoid: separate subqueries for each predicate (forces extra joins)
SELECT ?person ?name ?age WHERE {
  { SELECT ?person WHERE { ?person <ex:worksAt> <ex:acme> } }
  { SELECT ?person ?name WHERE { ?person <ex:name> ?name } }
}
```

## Filter pushdown

SPARQL FILTER expressions on dictionary-encoded constants are evaluated in the SQL WHERE clause at compile time. Constants are encoded to `BIGINT` before SQL is emitted — no dictionary lookups happen at query time.

For best performance, apply filters as early as possible (close to the bound variable in the triple pattern) rather than late in a wrapping subquery.

## OPTIONAL vs INNER JOIN

Use `OPTIONAL` when a result row should still appear even if the optional pattern is not matched. This compiles to a `LEFT JOIN`. Use a plain triple pattern (inner join behavior) when all results must have the variable bound.

```sparql
-- All persons with worksAt, plus name if available (LEFT JOIN)
SELECT ?person ?name WHERE {
  ?person <ex:worksAt> ?company .
  OPTIONAL { ?person <ex:name> ?name }
}

-- Only persons that have both worksAt and name (INNER JOIN)
SELECT ?person ?name WHERE {
  ?person <ex:worksAt> ?company .
  ?person <ex:name>    ?name .
}
```

## Plan cache hit rate

Compiled SPARQL→SQL plans are cached per-backend in an LRU cache. Identical query strings (including whitespace) hit the cache. To maximize cache hit rate:

- Parameterize queries by binding constants into `VALUES` inline data rather than embedding them as literal strings in the query text
- Keep the cache large enough for your query workload via `pg_ripple.plan_cache_size`

Check cache efficiency with `sparql_explain()` — repeated calls for the same query return instantly once the plan is cached.

## Property path recipes

### Transitive closure (follow all hops)

```sparql
SELECT ?target WHERE {
  <ex:alice> <ex:knows>+ ?target
}
```

Compiles to a `WITH RECURSIVE` CTE. Specify a depth limit to avoid runaway queries on dense graphs:

```sql
SET pg_ripple.max_path_depth = 10;
```

### Include the start node (zero or more hops)

```sparql
SELECT ?target WHERE {
  <ex:alice> <ex:follows>* ?target
}
-- Returns alice herself plus all reachable nodes
```

### Multi-predicate path (alternative)

```sparql
SELECT ?contact WHERE {
  <ex:alice> (<ex:knows>|<ex:follows>) ?contact
}
```

### Sequence (two-hop join)

```sparql
SELECT ?friend_of_friend WHERE {
  <ex:alice> <ex:knows>/<ex:knows> ?friend_of_friend
}
```

The `/` operator compiles to a chained join — spargebra represents the intermediate variable as an anonymous blank node which pg_ripple handles by applying an equi-join constraint.

### Inverse path (find who points to a node)

```sparql
SELECT ?who WHERE {
  ?who ^<ex:knows> <ex:bob>
}
-- Equivalent to: <ex:bob> is the object, ?who is the subject
```

## Resource exhaustion safeguards

For user-facing applications where input queries cannot be fully trusted:

```sql
-- Set a per-session depth cap
SET pg_ripple.max_path_depth = 15;

-- Set a per-query time limit
SET statement_timeout = '5s';
```

Both settings can be applied in a connection pool `after_connect` hook or in a row-level security policy. The depth cap is included in the plan cache key and does not cause cross-session pollution.

## VALUES for multi-value lookup

`VALUES` compiles to SQL inline data and is efficient for looking up a known list of resources:

```sparql
SELECT ?person ?name WHERE {
  VALUES ?person { <ex:alice> <ex:bob> <ex:carol> }
  ?person <ex:name> ?name .
}
```

This is more cache-friendly than embedding the values as individual FILTER clauses, since the query structure stays constant while only the VALUES rows change.

## Debugging with sparql_explain

Always inspect the generated SQL before blaming pg_ripple for slow results:

```sql
SELECT pg_ripple.sparql_explain(
    'SELECT ?name WHERE { ?p <ex:name> ?name . FILTER(?name = "Alice") }',
    false
);
```

Look for:
- `vp_rare` table scans where you expected a dedicated VP table — the predicate may not have been promoted yet
- Missing `WHERE` clause conditions — a FILTER may have failed to encode its constant
- Extra `UNION ALL` branches in property paths — expected for `*` and `?` operators

---

## Using the HTTP endpoint

The `pg_ripple_http` companion service exposes a standard SPARQL endpoint for use with any SPARQL-compatible tool or library.

### Python (SPARQLWrapper)

```python
from SPARQLWrapper import SPARQLWrapper, JSON

sparql = SPARQLWrapper("http://localhost:7878/sparql")
sparql.setQuery("""
    SELECT ?name WHERE {
        ?person <https://example.org/name> ?name
    } LIMIT 10
""")
sparql.setReturnFormat(JSON)
results = sparql.query().convert()

for result in results["results"]["bindings"]:
    print(result["name"]["value"])
```

### Java (Apache Jena)

```java
import org.apache.jena.query.*;

String endpoint = "http://localhost:7878/sparql";
String queryStr = "SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10";
Query query = QueryFactory.create(queryStr);

try (QueryExecution qexec = QueryExecutionFactory.sparqlService(endpoint, query)) {
    ResultSet results = qexec.execSelect();
    ResultSetFormatter.out(System.out, results, query);
}
```

### curl

```bash
# URL-encoded GET
curl -G http://localhost:7878/sparql \
  --data-urlencode "query=SELECT ?s WHERE { ?s ?p ?o } LIMIT 5"

# POST with SPARQL body
curl -X POST http://localhost:7878/sparql \
  -H "Content-Type: application/sparql-query" \
  -d "SELECT ?s WHERE { ?s ?p ?o } LIMIT 5"

# SPARQL Update
curl -X POST http://localhost:7878/sparql \
  -H "Content-Type: application/sparql-update" \
  -d "INSERT DATA { <ex:s> <ex:p> \"hello\" }"
```

---

## CONSTRUCT views vs SELECT views (v0.18.0)

Both CONSTRUCT views and SELECT views are pg_trickle stream tables that stay current as triples change. Choose based on what the downstream consumer needs.

| Consideration | SELECT view | CONSTRUCT view |
|---------------|-------------|----------------|
| Output shape | Tabular (columns = SPARQL variables) | Triples (s, p, o, g BIGINT) |
| Best for | Dashboards, APIs, SQL joins | Inference, denormalization, RDF export |
| Template count | 1 row per solution | N rows per solution (N = template size) |
| `decode = true` | Decodes each variable column | Decodes s, p, o to TEXT |

### Materialising inference results

Use a CONSTRUCT view to materialize RDFS/OWL entailments without running Datalog. This is faster for simple one-hop patterns:

```sql
-- Materialise rdfs:subClassOf inheritance one hop
SELECT pg_ripple.create_construct_view(
    'subclass_instances',
    'CONSTRUCT { ?i <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ?super }
     WHERE {
       ?i   <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>     ?sub .
       ?sub <http://www.w3.org/2000/01/rdf-schema#subClassOf>      ?super
     }',
    '5s'
);
```

For multi-hop inference (transitive closure), use a Datalog view with the `rdfs` built-in rule set instead.

### Using ASK views as live constraint monitors

An ASK view maintains a single boolean result that flips as the data changes. Ideal for:

- SHACL-style cardinality checks that are too expensive to run as triggers
- Dashboard "health indicator" lights
- Application-side event triggers (poll the stream table)

```sql
-- Alert when any order has been unshipped for more than 24 hours
SELECT pg_ripple.create_ask_view(
    'stale_orders',
    'ASK { ?order <https://schema.org/orderStatus>
                  <https://schema.org/OrderProcessing> .
           FILTER NOT EXISTS { ?order <https://schema.org/estimatedDelivery> ?d } }',
    '30s'
);

-- Application polls this:
SELECT result FROM pg_ripple.ask_view_stale_orders;
```

When `result` flips from `false` to `true`, the constraint is violated. Use a PostgreSQL NOTIFY/LISTEN or pg_logical replication slot to push the change to application subscribers.
