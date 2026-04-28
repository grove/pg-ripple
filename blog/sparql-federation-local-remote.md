[← Back to Blog Index](README.md)

# Querying the World from Inside PostgreSQL

## SPARQL federation: local data + remote endpoints in one query

---

Your knowledge graph lives in pg_ripple. Wikidata has 100 billion triples about the world. DBpedia has structured data extracted from Wikipedia. Your industry consortium publishes a SPARQL endpoint with regulatory data.

You need to join your local data with theirs. The traditional approach: download their data, load it into your instance, keep it synchronized. For Wikidata alone, that's a 100 TB download and a significant infrastructure commitment.

SPARQL federation offers a different approach: query their data live, from inside your SPARQL query, without downloading anything.

---

## The SERVICE Clause

SPARQL 1.1 defines the `SERVICE` keyword for federated queries:

```sparql
SELECT ?person ?name ?population WHERE {
  # Local data: people and their birthplaces
  ?person ex:birthPlace ?city .
  ?person foaf:name ?name .

  # Remote data: city populations from Wikidata
  SERVICE <https://query.wikidata.org/sparql> {
    ?city wdt:P1082 ?population .
  }
}
```

This query joins your local person-birthplace data with Wikidata's population data — without loading Wikidata into your database.

pg_ripple executes this as follows:

1. The local triple patterns (`ex:birthPlace`, `foaf:name`) are evaluated against local VP tables as usual.
2. The `SERVICE` clause is sent to the remote endpoint as a SPARQL query.
3. The remote results are joined with the local results.

The join happens inside PostgreSQL. pg_ripple fetches the remote results, materializes them in a temporary table, and lets the PostgreSQL optimizer handle the join planning.

---

## Cost-Based Federation Planning

Not all federated queries are created equal. Consider:

```sparql
SELECT ?person ?abstract WHERE {
  ?person rdf:type ex:Employee .       # 10,000 local employees
  ?person owl:sameAs ?dbpedia_id .     # 2,000 with DBpedia links

  SERVICE <http://dbpedia.org/sparql> {
    ?dbpedia_id dbo:abstract ?abstract .
    FILTER(LANG(?abstract) = "en")
  }
}
```

A naive execution sends the full `SERVICE` block to DBpedia with no bindings — asking for all English abstracts of all entities. DBpedia returns millions of results, most of which don't match any local employee.

pg_ripple's federation planner (FedX-style, since v0.42.0) does better:

1. **Evaluate local patterns first.** Get the 2,000 `?dbpedia_id` values from local data.
2. **Push bindings to the remote query.** Rewrite the SERVICE query to include `VALUES ?dbpedia_id { ... }` with the 2,000 bound IDs.
3. **Send the bound query.** DBpedia returns at most 2,000 abstracts — exactly the ones needed.

This binding push-down reduces network transfer from "all abstracts in DBpedia" (~2 TB) to "2,000 specific abstracts" (~2 MB). The difference is not a percentage improvement — it's a categorical change from "query times out" to "query completes in 3 seconds."

---

## Connection Pooling and Result Caching

Remote SPARQL endpoints have rate limits, connection limits, and variable latency. Sending 100 concurrent queries to Wikidata is a good way to get your IP blocked.

pg_ripple manages remote connections through:

### Connection Pool

Each remote endpoint gets a connection pool (configurable per-endpoint). Multiple SPARQL queries that reference the same endpoint share connections, with backpressure when the pool is exhausted.

```sql
-- Configure pool for a remote endpoint
SELECT pg_ripple.federation_set_pool(
  endpoint => 'https://query.wikidata.org/sparql',
  max_connections => 4,
  timeout_ms => 30000
);
```

### Result Cache

Remote query results are cached with a configurable TTL. If the same SERVICE query (with the same bindings) is executed within the TTL, the cached result is returned without a network round trip.

```sql
SELECT pg_ripple.federation_set_cache(
  endpoint => 'https://query.wikidata.org/sparql',
  ttl_seconds => 3600  -- Cache for 1 hour
);
```

### Query Rewriting

pg_ripple rewrites SERVICE queries for efficiency:

- **Projection push-down:** Only select the variables needed by the outer query.
- **Filter push-down:** Move FILTER expressions into the SERVICE block when they only reference remote variables.
- **LIMIT push-down:** If the outer query has a LIMIT, push it into the SERVICE query.

### Batching

When binding push-down produces many values, pg_ripple batches the VALUES clause to stay within endpoint URL length limits and query complexity limits:

```sparql
-- Instead of one query with 2,000 VALUES:
-- Split into 10 queries with 200 VALUES each
-- Execute in parallel (up to pool size)
-- Merge results
```

---

## The SSRF Allowlist

Federation means pg_ripple makes outbound HTTP requests. This is a security concern — a crafted SPARQL query could use `SERVICE` to probe internal network services (Server-Side Request Forgery).

pg_ripple mitigates this with an allowlist:

```sql
-- Only these endpoints are allowed
SET pg_ripple.federation_allowlist = 'https://query.wikidata.org/sparql,https://dbpedia.org/sparql';
```

When the allowlist is set (which it is by default — empty, meaning no federation), SERVICE clauses that target unlisted endpoints are rejected with an error. There's no way to bypass this from SPARQL — the allowlist is enforced at the federation executor level, not the parser level.

For internal deployments where all endpoints are trusted, the allowlist can include wildcard patterns:

```sql
SET pg_ripple.federation_allowlist = 'https://*.internal.company.com/sparql';
```

---

## Multi-Endpoint Queries

Federated queries can reference multiple remote endpoints in the same query:

```sparql
SELECT ?drug ?name ?side_effect ?gene WHERE {
  # Local: drug-gene associations from our research
  ?drug ex:associatedGene ?gene .

  # Remote 1: drug names from ChEMBL
  SERVICE <https://chembl.example.org/sparql> {
    ?drug rdfs:label ?name .
  }

  # Remote 2: side effects from SIDER
  SERVICE <https://sider.example.org/sparql> {
    ?drug sider:sideEffect ?side_effect .
  }
}
```

pg_ripple's planner evaluates local patterns first, then dispatches the two SERVICE queries in parallel (since they're independent — neither depends on the other's results). The results are joined after both complete.

---

## Parallel SERVICE Execution

Since v0.42.0, independent SERVICE clauses within the same query execute in parallel. The planner builds a dependency graph:

- If SERVICE B depends on a variable bound by SERVICE A, B waits for A.
- If SERVICE A and SERVICE B are independent, they execute concurrently.

For the drug example above, both ChEMBL and SIDER queries execute simultaneously, cutting the total wall time roughly in half.

---

## When Federation Doesn't Work Well

- **High-latency endpoints.** If the remote endpoint takes 5 seconds per request, no amount of optimization helps. The query is as fast as the slowest endpoint.
- **Unselective SERVICE queries.** A SERVICE block with no bound variables and no filters returns the entire remote dataset. This is usually a query design problem, not a federation problem.
- **Inconsistent schemas.** If the remote endpoint uses different IRIs for the same concept, the join produces no results. This is the ontology alignment problem — `owl:sameAs` and SKOS mappings help, but they require upfront work.
- **Endpoint reliability.** Remote endpoints go down, change their URLs, or deprecate SPARQL versions. Federation introduces a runtime dependency on external services.

For stable, well-documented endpoints (Wikidata, DBpedia, institutional SPARQL endpoints), federation works well. For ad-hoc endpoints with no SLA, consider periodic bulk downloads instead.

---

## The Alternative: Download Everything

If you need sub-second query latency and 100% availability, download the remote data and load it locally. pg_ripple's bulk loader handles this efficiently:

```sql
-- Load a Wikidata dump (N-Triples format, gzipped)
SELECT pg_ripple.load_ntriples_file('/data/wikidata-latest.nt.gz');
```

The trade-off: freshness. A local copy is stale as soon as the remote data changes. Federation gives you live data at the cost of latency and availability. Local copies give you speed at the cost of freshness.

pg_ripple supports both. Use federation for data that changes frequently and where freshness matters. Use local copies for reference data that changes slowly. Mix them in the same query if you need to.
