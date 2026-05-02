# PageRank & Graph Analytics (v0.88.0)

pg_ripple v0.88.0 adds a **Datalog-native PageRank engine** that runs entirely inside PostgreSQL
via SPI and the existing Datalog aggregation infrastructure.  All scores are persisted in
`_pg_ripple.pagerank_scores` and are queryable with standard SQL — no external process required.

## Quick Start

```sql
-- Load some RDF triples
SELECT pg_ripple.load_ntriples($nt$
  <http://example.org/Alice> <http://xmlns.com/foaf/0.1/knows> <http://example.org/Bob> .
  <http://example.org/Bob>   <http://xmlns.com/foaf/0.1/knows> <http://example.org/Carol> .
  <http://example.org/Carol> <http://xmlns.com/foaf/0.1/knows> <http://example.org/Alice> .
$nt$, 'http://example.org/graph');

-- Run PageRank (default: damping=0.85, 100 iterations)
SELECT node_iri, score
FROM pg_ripple.pagerank_run()
ORDER BY score DESC;
```

## SQL Functions

### `pg_ripple.pagerank_run(...)`

Runs iterative PageRank and persists results.  Returns one row per node.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `damping` | `float8` | `0.85` | Damping factor (teleportation probability = 1 − damping) |
| `max_iterations` | `int` | `100` | Maximum power-iteration steps |
| `convergence_delta` | `float8` | `0.0001` | L1-norm convergence threshold |
| `direction` | `text` | `'forward'` | `'forward'`, `'reverse'`, or `'undirected'` |
| `topic` | `text` | `''` | Topic label — enables topic-sensitive PageRank |
| `edge_predicates` | `text[]` | `NULL` | Restrict to these predicate IRIs (default: all) |
| `graph_uri` | `text` | `NULL` | Named graph to evaluate (default: all graphs) |
| `decay_rate` | `float8` | `0.0` | Temporal decay exponent (0 = disabled) |
| `bias` | `float8` | `0.15` | Personalization vector weight |

### `pg_ripple.centrality_run(metric text, ...)`

Computes an alternative centrality measure and stores results in
`_pg_ripple.centrality_scores`.

Supported `metric` values: `'betweenness'`, `'closeness'`, `'degree'`, `'pagerank'`.

```sql
SELECT COUNT(*) FROM pg_ripple.centrality_run('betweenness');
SELECT * FROM _pg_ripple.centrality_scores ORDER BY score DESC LIMIT 10;
```

### `pg_ripple.explain_pagerank(node_iri text, top_k int DEFAULT 5)`

Returns the score explanation tree for a node — which other nodes contributed how much.

```sql
SELECT depth, contributor_iri, contribution, path
FROM pg_ripple.explain_pagerank('<http://example.org/Alice>', 5);
```

### `pg_ripple.export_pagerank(format text, top_k bigint DEFAULT 10000, topic text DEFAULT '')`

Serialises scores in `'csv'`, `'turtle'`, `'ntriples'`, or `'jsonld'` format.

```sql
SELECT pg_ripple.export_pagerank('turtle', 1000);
```

### `pg_ripple.pagerank_queue_stats()`

Returns IVM dirty-edge queue metrics useful for monitoring.

```sql
SELECT * FROM pg_ripple.pagerank_queue_stats();
-- queued_edges | max_delta | oldest_enqueue | estimated_drain_seconds
```

### `pg_ripple.vacuum_pagerank_dirty()`

Drains processed entries from `_pg_ripple.pagerank_dirty_edges`.

### `pg_ripple.pagerank_find_duplicates(metric text, centrality_threshold float8, fuzzy_threshold float8)`

Returns candidate duplicate node pairs detected via centrality + label similarity.

## GUC Parameters

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.pagerank_enabled` | `off` | Master switch |
| `pg_ripple.pagerank_damping` | `0.85` | Damping factor |
| `pg_ripple.pagerank_max_iterations` | `100` | Iteration cap |
| `pg_ripple.pagerank_convergence_delta` | `0.0001` | Convergence threshold |
| `pg_ripple.pagerank_incremental` | `off` | Enable IVM (k-hop refresh) |
| `pg_ripple.pagerank_confidence_weighted` | `off` | Weight edges by confidence scores |
| `pg_ripple.pagerank_partition` | `off` | Partition evaluation per named graph |
| `pg_ripple.pagerank_probabilistic` | `off` | Probabilistic Datalog score bounds |
| `pg_ripple.pagerank_queue_warn_threshold` | `100000` | Warn when dirty-edge queue exceeds this |

## REST API (pg_ripple_http)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/pagerank/run` | POST | Trigger computation |
| `/pagerank/results` | GET | Top-N scores |
| `/pagerank/status` | GET | Last run metadata |
| `/pagerank/vacuum-dirty` | POST | Drain queue |
| `/pagerank/export` | GET | Export scores |
| `/pagerank/explain/{node_iri}` | GET | Score explanation |
| `/pagerank/queue-stats` | GET | IVM queue metrics |
| `/centrality/run` | POST | Compute centrality |
| `/centrality/results` | GET | Centrality scores |
| `/pagerank/find-duplicates` | POST | Entity deduplication |

## Storage

Scores are persisted in `_pg_ripple.pagerank_scores`:

```sql
\d _pg_ripple.pagerank_scores
-- node BIGINT, topic TEXT, score FLOAT8,
-- score_lower FLOAT8, score_upper FLOAT8,
-- computed_at TIMESTAMPTZ, iterations INT, converged BOOL,
-- stale BOOL, stale_since TIMESTAMPTZ
```

The IVM dirty-edge queue (`_pg_ripple.pagerank_dirty_edges`) tracks which edges changed
since the last full run, enabling k-hop incremental refresh (PR-TRICKLE-01).

## Feature Status

```sql
SELECT feature, status, version FROM pg_ripple.feature_status()
WHERE feature LIKE 'pagerank%' OR feature LIKE 'centrality%';
```
