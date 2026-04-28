[← Back to Blog Index](README.md)

# When Semantic Search Meets Knowledge Graphs

## Hybrid retrieval with pgvector and SPARQL

---

Semantic search finds things that *sound* similar. Knowledge graphs find things that *are* related. Most systems give you one or the other. pg_ripple gives you both, in the same query, using the same PostgreSQL instance.

This post explains why combining vector similarity search with graph queries produces better results than either alone, and how pg_ripple makes the combination work.

---

## The Limits of Vector Search Alone

You have a product catalog with 500,000 items. Each has an embedding from a text encoder (OpenAI, Cohere, sentence-transformers — doesn't matter). A user searches "comfortable office chair for tall people." pgvector returns the 10 most similar items by cosine distance.

The results are decent. But:

- Result #3 is a standing desk. The embedding caught "office" and "tall" but missed that the user wants a chair.
- Result #7 is a children's ergonomic chair. "Comfortable" and "chair" matched, but the context is wrong.
- Result #9 is discontinued and out of stock. The embedding doesn't know about inventory status.

Vector search operates on semantic similarity in embedding space. It doesn't know about product categories, stock status, compatibility requirements, or any other structured relationship. It finds items that are *textually* or *semantically* near the query — not items that satisfy the user's actual intent.

---

## The Limits of Graph Queries Alone

SPARQL can express the structured constraints precisely:

```sparql
SELECT ?product ?name WHERE {
  ?product rdf:type ex:OfficeChair .
  ?product ex:seatHeightMax ?height .
  FILTER(?height > 55)   # Tall-person friendly
  ?product ex:inStock true .
  ?product rdfs:label ?name .
}
```

This returns every in-stock office chair with a high seat. But it doesn't understand "comfortable" — that's a subjective quality that lives in text descriptions and user reviews, not in structured fields.

Graph queries find exact structural matches. Vector search finds fuzzy semantic matches. You need both.

---

## The pg:similar() Function

pg_ripple integrates with pgvector through a custom SPARQL function:

```sparql
SELECT ?product ?name ?score WHERE {
  ?product rdf:type ex:OfficeChair .
  ?product ex:inStock true .
  ?product ex:seatHeightMax ?height .
  FILTER(?height > 55)
  ?product rdfs:label ?name .
  BIND(pg:similar(?product, "comfortable office chair for tall people") AS ?score)
}
ORDER BY DESC(?score)
LIMIT 10
```

This query:

1. Uses SPARQL graph patterns to filter by type (office chair), stock status, and seat height.
2. Uses `pg:similar()` to compute vector similarity between each candidate product's embedding and the search query.
3. Orders by similarity score.

The graph patterns do the structured filtering. The vector function does the semantic ranking. Each does what it's good at.

---

## How pg:similar() Works

Under the hood, `pg:similar()` compiles to a pgvector cosine distance query:

1. The search query text is sent to the configured embedding endpoint (or looked up in the embedding cache).
2. The query embedding is compared against the product embeddings stored in `_pg_ripple.embeddings`.
3. The HNSW index on the embedding column provides approximate nearest neighbor results.

The critical optimization: the SPARQL graph patterns execute first, producing a set of candidate product IDs. The vector search is restricted to these candidates. If SPARQL filtering reduces 500,000 products to 200 in-stock tall-friendly office chairs, the vector search only computes 200 distances — not 500,000.

This filter-then-rank pattern is the most efficient way to combine structured and unstructured search. It's the opposite of the common approach (vector search first, filter afterward), which wastes vector computation on items that would have been filtered out.

---

## Reciprocal Rank Fusion

Sometimes you want the vector search to influence which candidates survive, not just how they're ranked. Reciprocal Rank Fusion (RRF) combines two ranked lists into one:

```sparql
SELECT ?product ?name ?rrf_score WHERE {
  # Structural ranking: products with more positive reviews
  {
    SELECT ?product (COUNT(?review) AS ?review_count) WHERE {
      ?product ex:hasReview ?review .
      ?review ex:sentiment "positive" .
    }
    GROUP BY ?product
  }

  ?product rdfs:label ?name .
  BIND(pg:similar(?product, "comfortable chair") AS ?vec_score)

  # RRF: combine structural rank and vector rank
  BIND(pg:rrf(?review_count, ?vec_score) AS ?rrf_score)
}
ORDER BY DESC(?rrf_score)
LIMIT 10
```

RRF is a rank-level fusion method: it takes two ranked lists (here, by review count and by vector similarity) and produces a combined ranking where items that appear high in both lists are promoted. It's simple, parameter-free, and surprisingly effective compared to more complex fusion methods.

---

## Graph-Contextualized Embeddings

Standard embeddings encode the text of an entity (its label, description, etc.) into a vector. But entities in a knowledge graph have structure — relationships, types, positions in hierarchies — that pure text embeddings miss.

pg_ripple's graph-contextualized embedding feature (since v0.28.0) enriches the embedding input with graph context:

```sql
SELECT pg_ripple.compute_embeddings(
  predicate => 'rdfs:label',
  context_depth => 2
);
```

With `context_depth => 2`, the embedding for each entity includes not just its label, but also:
- Its types (`rdf:type`)
- Labels of directly connected entities (1-hop neighbors)
- Labels of 2-hop neighbors

So the embedding for "Alice" includes context like "Alice, Person, works at Acme Corp, Technology company, based in San Francisco." This produces embeddings that capture structural position in the graph, not just the entity's own text.

The improvement is measurable: on a benchmark with 100,000 entities and relationship-dependent queries, graph-contextualized embeddings improve top-10 recall by 15–25% compared to text-only embeddings.

---

## The Incremental Embedding Worker

Embeddings become stale when the underlying data changes. If a product's description is updated, its embedding should be recomputed. If a new product is added, it needs an embedding.

pg_ripple's background embedding worker (since v0.28.0) handles this incrementally:

- When triples are inserted or updated for entities that have embeddings, the affected entities are queued for re-embedding.
- The worker processes the queue in batches, calling the configured embedding endpoint.
- New embeddings are written to the embedding table, and the HNSW index is updated.

For a graph that changes by 0.1% per day (common for product catalogs and knowledge bases), the incremental worker recomputes ~500 embeddings per day instead of 500,000. The HNSW index stays fresh without a full rebuild.

---

## Practical Example: Research Paper Search

A university research portal stores papers as RDF:

```turtle
ex:paper42 a ex:Paper ;
  dc:title "Attention Is All You Need" ;
  dc:creator ex:vaswani, ex:shazeer ;
  ex:topic ex:transformers, ex:attention ;
  ex:cites ex:paper17, ex:paper23 ;
  ex:publicationYear 2017 .
```

A researcher searches: "recent papers on efficient transformers by Google authors"

```sparql
SELECT ?paper ?title ?score WHERE {
  ?paper rdf:type ex:Paper .
  ?paper ex:publicationYear ?year .
  FILTER(?year >= 2023)
  ?paper dc:creator ?author .
  ?author ex:affiliation ex:Google .
  ?paper dc:title ?title .
  BIND(pg:similar(?paper, "efficient transformers") AS ?score)
}
ORDER BY DESC(?score)
LIMIT 20
```

The graph patterns handle: type filtering, recency (year >= 2023), and affiliation (Google). The vector similarity handles: "efficient transformers" — a semantic concept that doesn't have a single structured field.

Without the graph constraints, vector search would return papers from any year, any author, including non-Google affiliations. Without the vector search, the query would return all recent Google transformer papers, without ranking by relevance to "efficiency."

The combination is strictly more useful than either alone.

---

## When to Use Hybrid vs. Pure SPARQL

- **Structured queries with exact constraints:** Pure SPARQL. "Find all employees in the engineering department who joined after 2023." No vector search needed.
- **Fuzzy queries with no structural constraints:** Pure vector search. "Find documents similar to this one." No SPARQL needed.
- **Fuzzy queries with structural constraints:** Hybrid. "Find in-stock products similar to 'comfortable chair' in the furniture category." This is the sweet spot.

Most real search problems are hybrid. The structured constraints come from the application context (user's filters, access control, business rules). The fuzzy ranking comes from the user's natural language query. pg_ripple lets you express both in a single SPARQL query, executed in a single PostgreSQL instance, without a separate search service.
