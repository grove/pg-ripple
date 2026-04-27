# Full-Text Search

Sometimes the right query against a knowledge graph is not *"find the entity with this exact label"* but *"find any entity whose label, description, or notes mention these words"*. pg_ripple uses PostgreSQL's built-in full-text search machinery — `tsvector`, `tsquery`, GIN indexes — for this, exposed both as a SQL function and as a SPARQL filter.

---

## Setup

Full-text indexing is opt-in per predicate. Tell pg_ripple which literal-valued predicates are searchable:

```sql
SELECT pg_ripple.create_fts_index(
    predicate := '<http://www.w3.org/2000/01/rdf-schema#label>',
    config    := 'english'
);

SELECT pg_ripple.create_fts_index('<http://purl.org/dc/elements/1.1/title>',  'english');
SELECT pg_ripple.create_fts_index('<https://schema.org/description>',          'english');
```

Behind the scenes pg_ripple maintains a generated `tsvector` column on the relevant VP table and a GIN index over it. Inserts and updates flow through automatically.

The `config` parameter is any PostgreSQL text-search configuration name (`english`, `simple`, `spanish`, …). Use `simple` for languages whose stemmer you do not have, or for proper-noun-heavy data.

---

## Searching from SQL

```sql
-- All subjects whose label matches the query.
SELECT * FROM pg_ripple.fts_search(
    predicate := '<http://www.w3.org/2000/01/rdf-schema#label>',
    query     := 'machine & learning'
);
```

The `query` argument follows PostgreSQL `tsquery` syntax: `&` for AND, `|` for OR, `!` for NOT, `<->` for adjacency. See the [PostgreSQL FTS documentation](https://www.postgresql.org/docs/current/textsearch-controls.html).

---

## Searching from SPARQL

The `pg:fts()` SPARQL filter function returns true when an entity's literal value matches the tsquery. It composes naturally with other graph patterns:

```sparql
PREFIX pg:    <http://pg-ripple.io/fn/>
PREFIX rdfs:  <http://www.w3.org/2000/01/rdf-schema#>
PREFIX schema:<https://schema.org/>

SELECT ?paper ?title WHERE {
    ?paper a            <https://example.org/ScholarlyArticle> ;
           schema:author <https://example.org/alice> ;
           rdfs:label    ?title .
    FILTER(pg:fts(?title, "graph & neural & networks"))
}
```

`pg:fts()` only fires when the matched literal lives on a predicate that has been indexed. Otherwise it returns `false` (and emits a debug-level log entry).

---

## Ranking

For ranked results, `pg_ripple.fts_search_ranked()` returns the FTS rank score per row:

```sql
SELECT subject, rank
FROM pg_ripple.fts_search_ranked(
    predicate := '<https://schema.org/description>',
    query     := 'sustainable & supply & chain'
)
ORDER BY rank DESC
LIMIT 25;
```

The ranking is `ts_rank_cd` with default normalisation. To override the normalisation, use `fts_search_ranked(predicate, query, normalisation := 32)`.

---

## Combining FTS and vector search

FTS catches exact lexical matches; vector search catches paraphrase. Combining them improves recall:

```sql
SELECT entity_iri, fused_score
FROM pg_ripple.hybrid_search(
    sparql := 'SELECT ?p WHERE { ?p a <https://example.org/Paper> .
                                 FILTER(pg:fts(?p, "neural & networks")) }',
    text   := 'deep learning architectures',
    k      := 25,
    alpha  := 0.4
);
```

This pattern — *FTS for must-include keywords, vector for semantic broadening* — is one of the most useful tricks in RAG pipelines.

---

## When **not** to use FTS

- The data is structured with controlled vocabularies (`skos:Concept` taxonomies, code lists) — use the [exact SPARQL pattern instead](querying-with-sparql.md).
- Your dataset is small (< 10 K labels). Plain `LIKE '%keyword%'` is fine.
- You only ever query a single language. PostgreSQL's `simple` config is faster for that case than the language-aware ones.

---

## See also

- [Vector & Hybrid Search](vector-and-hybrid-search.md) — the semantic counterpart to FTS.
- [PostgreSQL Full-Text Search documentation](https://www.postgresql.org/docs/current/textsearch.html)
