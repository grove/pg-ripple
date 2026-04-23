# Entity Alignment with `owl:sameAs`

pg_ripple v0.49.0 adds two functions for embedding-based entity alignment:

- **`pg_ripple.suggest_sameas(threshold)`** — returns candidate entity pairs whose cosine similarity exceeds a configurable threshold.
- **`pg_ripple.apply_sameas_candidates(min_similarity)`** — promotes accepted pairs to `owl:sameAs` triples, triggering the built-in canonicalization logic.

These functions build on the vector embedding infrastructure introduced in v0.27.0 and require pgvector to be installed.

## Prerequisites

1. pgvector installed in the database (`CREATE EXTENSION vector`)
2. Entities embedded via `pg_ripple.embed_entities()` or `pg_ripple.store_embedding()`
3. `pg_ripple.pgvector_enabled = on` (default)

## Finding Candidate Pairs

```sql
-- Find entity pairs with cosine similarity ≥ 0.90
SELECT s1, s2, similarity
FROM pg_ripple.suggest_sameas(threshold := 0.9)
ORDER BY similarity DESC;
```

```
        s1                         s2                          similarity
--------------------------  --------------------------  --------------------
https://db1.org/Apple_Inc   https://db2.org/Apple-Inc   0.97
https://db1.org/New_York    https://db2.org/NewYork_NY  0.94
https://db1.org/John_Smith  https://db2.org/JohnSmith   0.91
```

The function performs an HNSW cosine self-join on `_pg_ripple.embeddings`, returning only IRI entities (`kind = 0`) where the pair's entity IDs differ.

### Parameter

| Parameter | Default | Description |
|-----------|---------|-------------|
| `threshold` | `0.9` | Minimum cosine similarity (0.0–1.0) to include a pair |

## Applying Candidates

```sql
-- Insert pairs with similarity ≥ 0.95 as owl:sameAs triples
SELECT pg_ripple.apply_sameas_candidates(min_similarity := 0.95);
```

This function:
1. Runs `suggest_sameas(min_similarity)` internally.
2. For each candidate pair, inserts both `s1 owl:sameAs s2` **and** `s2 owl:sameAs s1` as triples.
3. Respects the `pg_ripple.sameas_max_cluster_size` limit (PT550 WARNING when exceeded).
4. Returns the count of new triples inserted.

## Recommended Workflow

```sql
-- 1. Inspect candidates at a permissive threshold first
SELECT s1, s2, similarity
FROM pg_ripple.suggest_sameas(0.85)
ORDER BY similarity DESC
LIMIT 50;

-- 2. Spot-check a few pairs manually, then apply at a stricter threshold
SELECT pg_ripple.apply_sameas_candidates(0.95);

-- 3. Verify the inserted sameAs triples
SELECT count(*) FROM pg_ripple.sparql(
    'SELECT * WHERE { ?s <http://www.w3.org/2002/07/owl#sameAs> ?o }'
);
```

## Threshold Tuning

| Threshold | Precision | Recall | Use Case |
|-----------|-----------|--------|----------|
| ≥ 0.98 | Very high | Low | Trusted auto-apply with no review |
| 0.95–0.98 | High | Medium | Auto-apply after spot-checking |
| 0.90–0.95 | Medium | High | Review queue for human validation |
| < 0.90 | Low | Very high | Exploratory analysis only |

For production deployments, run `suggest_sameas(0.90)` to build a review queue, then validate and insert with `apply_sameas_candidates(0.95)`.

## Graceful Degradation

Both functions degrade gracefully when pgvector is unavailable:

```sql
SET pg_ripple.pgvector_enabled = off;

-- Returns 0 rows with a WARNING — no ERROR
SELECT count(*) FROM pg_ripple.suggest_sameas();
-- WARNING: pg_ripple.suggest_sameas: pgvector disabled ...

-- Returns 0 — no ERROR
SELECT pg_ripple.apply_sameas_candidates();
```

## Example: Integrating Two Knowledge Bases

```sql
-- Load two datasets with partially overlapping entities
SELECT pg_ripple.load_ntriples($$
<https://wikidata.org/Q312> <http://www.w3.org/2000/01/rdf-schema#label> "Apple Inc."@en .
<https://wikidata.org/Q312> <https://schema.org/foundingDate> "1976-04-01"^^xsd:date .
$$);

SELECT pg_ripple.load_ntriples($$
<https://dbpedia.org/Apple_Inc> <http://www.w3.org/2000/01/rdf-schema#label> "Apple Inc."@en .
<https://dbpedia.org/Apple_Inc> <https://schema.org/numberOfEmployees> "164000"^^xsd:integer .
$$);

-- Embed the entities (requires embedding API configured)
SELECT pg_ripple.embed_entities();

-- Find and apply candidates
SELECT pg_ripple.apply_sameas_candidates(0.95);

-- Now federated queries work across both datasets via sameAs canonicalization
SELECT pg_ripple.sparql($$
    SELECT ?label ?founded ?employees WHERE {
        ?company rdfs:label ?label .
        OPTIONAL { ?company schema:foundingDate ?founded }
        OPTIONAL { ?company schema:numberOfEmployees ?employees }
        FILTER(?label = "Apple Inc."@en)
    }
$$);
```

## Cluster Size Limits

The `pg_ripple.sameas_max_cluster_size` GUC (default: 100,000) guards against runaway canonicalization on very large equivalence clusters. When a cluster exceeds this size, a PT550 WARNING is emitted and canonicalization falls back to a sampling approximation. Adjust the limit with:

```sql
SET pg_ripple.sameas_max_cluster_size = 10000;  -- stricter limit
SET pg_ripple.sameas_max_cluster_size = 0;       -- disable limit check
```
