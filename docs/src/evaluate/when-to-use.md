# When to Use pg_ripple

pg_ripple is a PostgreSQL extension that turns your database into a knowledge graph store. This page helps you decide whether it fits your architecture.

## Decision flowchart

Ask yourself these questions in order:

1. **Do you already run PostgreSQL?** If yes, pg_ripple integrates with zero additional infrastructure for the data store. If you run a different database, evaluate the migration cost.
2. **Do you need to model complex relationships?** If your data is primarily tabular with few joins, standard SQL may be simpler. If you have deeply nested, many-to-many, or hierarchical relationships, a graph model helps.
3. **Do you need a standard query language?** SPARQL is a W3C standard with broad tool support. If you prefer a property-graph query language (Cypher/GQL), consider Neo4j or Amazon Neptune.
4. **Do you need reasoning or validation?** pg_ripple includes SHACL validation and Datalog reasoning. Standalone triple stores like Virtuoso or Blazegraph may not.
5. **Do you need graph context for LLM prompts?** pg_ripple combines SPARQL graph traversal with pgvector similarity search in a single query — something pure vector databases cannot do.

## Comparison matrix

| Criterion | pg_ripple | Plain SQL | Virtuoso / Blazegraph | Neo4j | Pure vector DB |
|---|---|---|---|---|---|
| Deployment | PostgreSQL extension | Any RDBMS | Standalone JVM | Standalone | Standalone |
| Query language | SPARQL 1.1 | SQL | SPARQL 1.1 | Cypher | Proprietary |
| Data model | RDF (triples) | Relational | RDF (triples) | Property graph | Vectors + metadata |
| Schema validation | SHACL | CHECK / triggers | Varies | Constraints | None |
| Reasoning | Datalog (RDFS, OWL RL) | Manual SQL | RDFS / OWL (varies) | None built-in | None |
| Vector search | pgvector integration | pgvector | Not built-in | Limited | Native |
| Hybrid graph+vector | Yes (single query) | Manual joins | No | No | No |
| HTTP API | pg_ripple_http | Build your own | Built-in | Built-in | Built-in |
| Transactions | Full PostgreSQL ACID | Full ACID | Varies | ACID | Varies |
| Backup/restore | pg_dump/pg_restore | Standard | Custom tools | Custom tools | Custom tools |
| Operational complexity | Low (PostgreSQL) | Low | Medium–High | Medium | Medium |

## When pg_ripple is a good fit

- You already operate PostgreSQL and want to avoid managing a separate graph database
- Your data has rich, interconnected relationships (ontologies, catalogs, supply chains)
- You need SPARQL 1.1 compliance for interoperability with W3C-standard tools
- You need to validate data quality against formal rules (SHACL)
- You need to derive new facts from existing data (Datalog reasoning, OWL RL, RDFS)
- You want to combine graph traversal with vector similarity for RAG pipelines
- You need full ACID transactions on graph data

## When pg_ripple is not the best fit

- **Graph datasets exceeding ~1 billion triples**: pg_ripple has been tested to 100M triples. For very large datasets, consider distributed solutions.
- **Property graph with Cypher/GQL**: if your team already uses Cypher and Neo4j, migrating to SPARQL has a learning curve. pg_ripple speaks SPARQL, not Cypher.
- **Pure vector search workload**: if you only need approximate nearest neighbor search without graph traversal, pgvector alone is simpler.
- **Real-time streaming graphs**: pg_ripple processes data in transactions, not continuous streams. For streaming graph analytics, consider Apache Flink with a graph library.
- **No PostgreSQL in your stack**: if you run MySQL, MongoDB, or a managed NoSQL service and have no plans to adopt PostgreSQL, introducing it solely for pg_ripple adds operational overhead.

## AI/LLM comparison: when does graph context outperform flat vector retrieval?

Graph-augmented retrieval helps when:

- The query requires **multi-hop reasoning** — "find papers by co-authors of Alice's co-authors" cannot be answered by vector similarity alone
- **Entity deduplication** matters — `owl:sameAs` canonicalization ensures the same entity is not embedded multiple times with different IRIs
- **Structured output** is needed — JSON-LD framing produces token-efficient, structured context that flat top-k results cannot provide
- **Provenance** matters — graph traversal can trace why a fact is relevant, not just that it is similar

Pure vector search (Qdrant, Weaviate, pgvector-only) is sufficient when:

- The query is a simple "find similar documents" without relationship constraints
- Your corpus is unstructured text without entity-level structure
- Latency requirements are sub-millisecond at millions of vectors

## Next steps

- [Installation](../getting-started/installation.md) — get pg_ripple running
- [Hello World](../getting-started/hello-world.md) — load and query data in five minutes
