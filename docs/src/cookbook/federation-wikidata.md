# Cookbook: Federation with Wikidata and DBpedia

**Goal.** Combine data from your local knowledge graph with information from public SPARQL endpoints — Wikidata, DBpedia, or any other — in a single query. No ETL, no copying data, no scheduled synchronisation.

**Why pg_ripple.** SPARQL 1.1 `SERVICE` federation is built in. A cost-based planner pushes filters to the remote endpoint, a circuit-breaker handles timeouts, and an in-process cache avoids hitting remote endpoints on every query.

**Time to first result.** ~10 minutes.

---

## Step 1 — Register remote endpoints

Registering an endpoint once lets the planner assign it a cost estimate and lets the health monitor track its availability.

```sql
SELECT pg_ripple.register_federation_endpoint(
    endpoint := 'https://dbpedia.org/sparql',
    label    := 'DBpedia'
);

SELECT pg_ripple.register_federation_endpoint(
    endpoint        := 'https://query.wikidata.org/sparql',
    label           := 'Wikidata',
    -- Optional: pin the TLS certificate to prevent MITM.
    pin_fingerprint := NULL   -- e.g. 'sha256:AA:BB:CC:...'
);
```

Without registration, `SERVICE` still works but gets a generic cost estimate and no health monitoring.

## Step 2 — Load your local facts

```sql
SELECT pg_ripple.load_turtle($TTL$
@prefix ex:   <https://example.org/people/> .
@prefix dbr:  <http://dbpedia.org/resource/> .
@prefix owl:  <http://www.w3.org/2002/07/owl#> .

ex:alice  <http://schema.org/name>  "Alice Smith" .
ex:alice  owl:sameAs  dbr:Alice_Smith_(scientist) .

ex:bob    <http://schema.org/name>  "Bob Jones" .
ex:bob    owl:sameAs  dbr:Robert_Jones_(chemist) .
$TTL$);
```

The `owl:sameAs` links tell the planner that your local entities correspond to DBpedia resources. With `pg_ripple.sameas_reasoning = on` (the default), the SERVICE query can use either IRI.

## Step 3 — A simple federated query

Retrieve birth dates from DBpedia for people you have locally:

```sql
SELECT * FROM pg_ripple.sparql($$
    PREFIX schema: <http://schema.org/>
    PREFIX dbo:    <http://dbpedia.org/ontology/>
    PREFIX owl:    <http://www.w3.org/2002/07/owl#>

    SELECT ?name ?birthDate WHERE {
        -- Local: who do we know about?
        ?local  schema:name    ?name ;
                owl:sameAs     ?remote .

        -- Remote: fetch their birth dates from DBpedia.
        SERVICE <https://dbpedia.org/sparql> {
            ?remote  dbo:birthDate  ?birthDate .
        }
    }
$$);
```

The planner sends `?remote dbo:birthDate ?birthDate` to DBpedia with the values of `?remote` bound from the local result — this is **bound-join federation**, the most efficient pattern.

## Step 4 — Multi-source enrichment

Enrich a medical graph with both Wikidata (drug indications) and a local proprietary store (clinical trial data):

```sql
SELECT * FROM pg_ripple.sparql($$
    PREFIX ex:  <https://example.org/>
    PREFIX wd:  <http://www.wikidata.org/entity/>
    PREFIX wdt: <http://www.wikidata.org/prop/direct/>
    PREFIX owl: <http://www.w3.org/2002/07/owl#>

    SELECT ?drugName ?indication ?trialId WHERE {
        -- Local store: drugs and their Wikidata cross-links.
        ?drug  ex:name       ?drugName ;
               owl:sameAs    ?wikidataDrug .

        -- Wikidata: approved indications.
        SERVICE <https://query.wikidata.org/sparql> {
            ?wikidataDrug  wdt:P2175  ?indicationEntity .
            ?indicationEntity  wdt:P1813  ?indication .   -- short name
        }

        -- Local store: clinical trial IDs.
        OPTIONAL { ?drug  ex:clinicalTrial  ?trialId }
    }
    ORDER BY ?drugName
$$);
```

## Step 5 — Check endpoint health and cache

```sql
-- Which endpoints are reachable right now?
SELECT endpoint, label, last_ping_ms, is_healthy
FROM pg_ripple.federation_endpoint_health()
ORDER BY last_ping_ms;

-- How many remote results are cached?
SELECT * FROM pg_ripple.federation_cache_stats();

-- Clear the cache to force a fresh fetch.
SELECT pg_ripple.reset_cache_stats();
```

## Step 6 — Inspect the federation plan

Before running a heavy federated query, check how the planner intends to execute it:

```sql
SELECT pg_ripple.explain_sparql($$
    PREFIX schema: <http://schema.org/>
    PREFIX dbo:    <http://dbpedia.org/ontology/>
    SELECT ?name ?birthDate WHERE {
        ?local schema:name ?name .
        SERVICE <https://dbpedia.org/sparql> {
            ?local dbo:birthDate ?birthDate .
        }
    }
$$, analyze := false);
```

A healthy federation plan shows a **BoundJoin** node rather than an **IndependentService** node. BoundJoin batches local values into a VALUES clause and sends one remote request; IndependentService executes the SERVICE clause for every row — much slower for large local result sets.

If you see IndependentService on a query that should be bound-join, ensure the shared variable (`?local` in the example) is bound by the local pattern *before* the SERVICE clause in the query text.

---

## Circuit breaker and timeouts

| GUC | Default | Effect |
|---|---|---|
| `pg_ripple.federation_timeout_ms` | `5000` | Abort a remote call after this many milliseconds |
| `pg_ripple.federation_retry_count` | `2` | Retry this many times before tripping the circuit breaker |
| `pg_ripple.federation_circuit_open_ms` | `60000` | Once a circuit trips, wait this long before retrying |

When a circuit is open, queries that use that SERVICE endpoint return an empty binding for that sub-pattern and continue with local results — they do not fail outright. The `pg_ripple.federation_endpoint_health()` view shows which circuits are open.

---

## Security note

Only register endpoints you trust and control (or well-known public endpoints). The federation circuit sends query text to the remote endpoint — potentially including literal values from your local store. For sensitive data, use a local SPARQL-over-HTTP cache (or [pg_ripple_http](../features/apis-and-integration.md)) as a proxy rather than federating directly to a public endpoint.

---

## See also

- [Federation (SPARQL SERVICE)](../user-guide/sql-reference/federation.md) — full SQL function reference.
- [Vector Federation](../user-guide/vector-federation.md) — federating vector search to Weaviate / Qdrant / Pinecone.
- [SPARQL Query Debugger](../user-guide/explain-sparql.md)
