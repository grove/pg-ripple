# pg_ripple Blog

> **Note:** This blog directory is an experiment. All posts were generated with
> AI assistance (GitHub Copilot / Claude) as a way to explore how well
> LLM-generated technical writing holds up for a niche systems engineering
> topic. The technical content has been reviewed for accuracy, but treat the
> posts as drafts — not as officially reviewed documentation.

---

## Posts

### Core Concepts & Architecture

| Post | Summary |
|------|---------|
| [Why RDF Inside PostgreSQL?](why-rdf-in-postgresql.md) | The case for a triple store that lives where your data already is — no ETL pipeline, no separate cluster, no impedance mismatch. Why VP storage on top of PostgreSQL beats both property-graph databases and standalone triplestores for most knowledge-graph workloads. |
| [Vertical Partitioning: One Table Per Predicate](vertical-partitioning-explained.md) | Inside pg_ripple's storage model: each predicate gets its own table with `(s, o, g)` columns. Why this beats a single wide `(s, p, o)` table by 10–100× for selective queries, and how rare-predicate consolidation keeps the catalog sane. |
| [Everything Is an Integer](dictionary-encoding-integer-joins.md) | Dictionary encoding with XXH3-128: every IRI, blank node, and literal is hashed to `BIGINT` before it touches storage. Why string comparisons in a triple store are a performance bug, and how integer-only joins change the cost model. |
| [How SPARQL Becomes a PostgreSQL Query Plan](sparql-to-sql-translation.md) | The translation pipeline from SPARQL text to `spargebra` algebra to SQL to SPI execution. Filter pushdown, self-join elimination, and why the PostgreSQL optimizer is surprisingly good at graph queries when you give it the right SQL. |

### Storage & Performance

| Post | Summary |
|------|---------|
| [HTAP for Triples: Reads and Writes at the Same Time](htap-reads-and-writes.md) | The delta/main/tombstone split that lets pg_ripple handle concurrent OLTP writes and analytical SPARQL queries without locking. Background merge workers, BRIN indexes, and the query path that unions it all together. |
| [Leapfrog Triejoin: When Triangle Queries Meet Optimal Joins](leapfrog-triejoin.md) | Standard hash joins go quadratic on cyclic patterns like triangles and cliques. Leapfrog triejoin doesn't. How pg_ripple compiles worst-case optimal joins into PostgreSQL, and what a 10–100× speedup looks like in practice. |
| [Property Paths Are Just Recursive CTEs](property-paths-recursive-ctes.md) | SPARQL property paths (`foaf:knows+`, `skos:broader*`) compiled to `WITH RECURSIVE … CYCLE`. Why PostgreSQL 18's hash-based cycle detection matters, bounded depth for safety, and early fixpoint termination for hierarchies. |

### Reasoning & Inference

| Post | Summary |
|------|---------|
| [Datalog Inside PostgreSQL](datalog-inside-postgresql.md) | Automatic fact derivation from rules — RDFS subclass inference, OWL RL reasoning, transitive closure, all running as SQL inside your database. Semi-naive evaluation, stratified negation, and why Datalog is the query language your ontology already speaks. |
| [Magic Sets: Ask a Question, Infer Only What You Need](magic-sets-goal-directed.md) | Full materialization computes every possible inference. Magic sets rewrite rules so that only facts reachable from your query are derived. The difference between 2 million inferred triples and 47. |
| [owl:sameAs Without the Explosion](owl-sameas-entity-resolution.md) | Entity canonicalization at query time: union-find over `owl:sameAs` chains, canonical ID rewriting in the dictionary, and why naive `owl:sameAs` handling turns a 1-second query into a 10-minute one. |

### Data Quality & Validation

| Post | Summary |
|------|---------|
| [SHACL: Schema Validation for the Schema-Less](shacl-data-quality.md) | RDF has no schema. SHACL gives it one — declaratively, after the fact, without migration scripts. How pg_ripple compiles SHACL shapes into DDL constraints and async validation pipelines, with hints that feed the SPARQL query planner. |

### Interoperability & Federation

| Post | Summary |
|------|---------|
| [Querying the World from Inside PostgreSQL](sparql-federation-local-remote.md) | SPARQL `SERVICE` clauses that reach out to Wikidata, DBpedia, or your company's other SPARQL endpoints — federated from inside a single query. Cost-based planning, connection pooling, result caching, and the SSRF allowlist that keeps it safe. |
| [From Flat Triples to Nested JSON](json-ld-framing-nested-json.md) | A CONSTRUCT query returns triples. JSON-LD framing turns those triples into the nested JSON your API consumers actually want. Frame-driven shaping, `@embed`, `@reverse`, and why you don't need a separate API layer for graph-to-tree conversion. |

### AI & Semantic Search

| Post | Summary |
|------|---------|
| [When Semantic Search Meets Knowledge Graphs](vector-sparql-hybrid-search.md) | pgvector finds things that *sound* similar. SPARQL finds things that *are* related. Combine them: `pg:similar()` in a SPARQL query, reciprocal rank fusion, graph-contextualized embeddings, and hybrid retrieval that outperforms either alone. |
| [GraphRAG: Feeding LLMs with Structured Knowledge](graphrag-knowledge-export.md) | LLMs hallucinate when they don't have context. Knowledge graphs have context but no language model. GraphRAG bridges the gap: Parquet export, Datalog-enriched entity graphs, community detection, and the `rag_context()` pipeline that turns a question into grounded answers. |

### Advanced

| Post | Summary |
|------|---------|
| [RDF-star: Making Statements About Statements](rdf-star-statements-about-statements.md) | "Alice knows Bob" is a triple. "Alice knows Bob, according to the HR system, as of January 2024" is a statement about a statement. How pg_ripple stores RDF-star quoted triples in the dictionary, why reification is dead, and what this means for provenance, temporal data, and trust. |

---

## Contributing

These posts are deliberately rough-edged — they're drafts exploring how the extension works, not polished marketing copy. If you spot a technical inaccuracy, open an issue or PR. If you want to write a post, open a discussion first to avoid duplication.
