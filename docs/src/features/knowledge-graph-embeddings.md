# Knowledge-Graph Embeddings (KGE)

A **knowledge-graph embedding** is a vector representation of an entity learned from the **structure of the graph** — its relationships — rather than from any text describing the entity. KGEs power three jobs that text embeddings do badly:

1. **Entity alignment across graphs** — match `Apple` in graph A to `Apple Inc.` in graph B based on shared neighbours, not on the strings.
2. **Link prediction** — score plausible edges that are *not* in the graph today (recommendations, cold-start, schema completion).
3. **Cluster discovery** — find groups of structurally-similar entities even when they have no labels in common.

pg_ripple ships two well-known KGE models, **TransE** and **RotatE**, with a unified SQL interface and an HNSW index for fast nearest-neighbour search.

> Available since v0.55.0 (`pg_ripple.kge_enabled` GUC). Requires `pgvector`.

---

## How TransE and RotatE differ

| Model | Geometric idea | Best for | Cost |
|---|---|---|---|
| **TransE** | A relation is a *translation* in vector space: `head + relation ≈ tail` | Hierarchies, simple relational patterns | Cheap; trains in minutes on millions of triples |
| **RotatE** | A relation is a *rotation* in complex vector space | Symmetric, antisymmetric, inverse, and composition patterns | ~2× the cost of TransE; better quality on dense graphs |

When in doubt, start with TransE. If your graph has lots of inverse or symmetric relations (`spouse`, `siblingOf`, `coAuthor`), switch to RotatE.

---

## Quick start

```sql
-- 1. Enable the feature.
SET pg_ripple.kge_enabled = on;

-- 2. Train a model on the entire store.
SELECT pg_ripple.kge_train(
    model        := 'TransE',
    dimensions   := 128,
    epochs       := 100,
    learning_rate:= 0.01,
    margin       := 1.0
);

-- 3. Inspect the trained vectors.
SELECT entity_iri, vector
FROM _pg_ripple.kge_embeddings
LIMIT 5;

-- 4. Use the vectors for entity alignment.
SELECT * FROM pg_ripple.find_alignments(
    source_graph := 'https://example.org/g1',
    target_graph := 'https://example.org/g2',
    threshold    := 0.9
);
```

---

## Choosing hyperparameters

| Parameter | Default | Tuning advice |
|---|---|---|
| `dimensions` | `128` | 50 for small graphs (< 100 K entities), 200–400 for very large or dense graphs |
| `epochs` | `100` | Until validation loss plateaus; check every 25 epochs |
| `learning_rate` | `0.01` | Halve it if loss oscillates; double it if loss decreases too slowly |
| `margin` | `1.0` | TransE only; the margin between positive and negative triple scores |
| `batch_size` | `1024` | Larger batches give smoother gradients but use more memory |

Training writes its loss curve to the PostgreSQL log so you can monitor convergence in real time.

---

## Three things people get wrong

1. **Embedding too early.** KGEs need a connected graph. If you train before loading `owl:sameAs` and inverse properties, the model learns isolated islands. Always materialise built-in RDFS / OWL inference (`pg_ripple.infer('rdfs')`, `pg_ripple.infer('owl-rl')`) **before** training.
2. **Comparing across models.** A vector trained with TransE 128-dim is meaningless to a model trained with RotatE 256-dim. The `_pg_ripple.kge_embeddings` table tracks `(model, dimensions)` per row; queries automatically scope to one model.
3. **Forgetting to re-train.** KGE quality drifts as the graph grows. Schedule a retrain whenever the entity count grows by ~25 %, or weekly for high-velocity ingestion.

---

## Use case: link prediction

```sql
-- Score the plausibility of a candidate triple.
SELECT pg_ripple.kge_score(
    head     := '<https://example.org/Alice>',
    relation := '<https://example.org/worksAt>',
    tail     := '<https://example.org/MIT>'
);
-- Returns a real-valued score; higher = more plausible.

-- Find the top-10 most plausible employers for Alice.
SELECT tail, score
FROM pg_ripple.kge_predict_tails(
    head     := '<https://example.org/Alice>',
    relation := '<https://example.org/worksAt>',
    k        := 10
);
```

This is the foundation for cold-start recommendations and schema-completion workflows.

---

## Use case: cross-graph alignment

`find_alignments()` is a thin wrapper that performs an HNSW cosine search of every entity in `source_graph` against every entity in `target_graph`, returning pairs above a threshold. The output is shaped exactly like `suggest_sameas()`, so it plugs into the [Record Linkage](record-linkage.md) pipeline unchanged.

```sql
SELECT s1, s2, similarity
FROM pg_ripple.find_alignments(
    source_graph := 'https://wikidata.example/',
    target_graph := 'https://internal-kb.example/',
    threshold    := 0.92
)
ORDER BY similarity DESC;
```

---

## Storage and indexing

| Object | Purpose |
|---|---|
| `_pg_ripple.kge_embeddings(entity_id, model, dimensions, vector)` | One row per (entity, model). Vector type is pgvector. |
| HNSW index on `(model, vector vector_cosine_ops)` | Sub-millisecond top-k cosine queries |
| `_pg_ripple.kge_models(name, dimensions, trained_at, loss)` | One row per training run, for monitoring |

A 1 M-entity graph with 128-dim TransE embeddings occupies ~512 MB in `_pg_ripple.kge_embeddings`. Plan disk accordingly.

---

## When **not** to use KGE

- Your graph is small (< 10 K entities). TransE will overfit; text embeddings are simpler.
- Your entities have no informative relationships. KGE has nothing to learn from.
- You need explainable scores. KGE is a black box; SHACL constraints and `owl:sameAs` are the right answer for regulator-facing decisions.

---

## See also

- [Record Linkage](record-linkage.md) — uses `find_alignments()` for cross-graph entity resolution.
- [Vector & Hybrid Search](vector-and-hybrid-search.md) — text-embedding cousin of KGE.
- [Reasoning & Inference](reasoning-and-inference.md) — materialise inference *before* training to densify the graph.
