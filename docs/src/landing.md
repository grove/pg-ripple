# What Is pg_ripple?

**pg_ripple** turns your PostgreSQL database into a knowledge graph store — and into the foundation for AI applications that need *verifiable, structured, traceable* answers rather than hallucinated ones.

You can build a chatbot that answers questions from your own knowledge base, deduplicate customer records across systems, validate data quality with formal rules, run OWL reasoning, and answer SPARQL queries over billions of triples — all inside the PostgreSQL you already operate, with no extra infrastructure.

```sql
-- Ask a natural-language question against your knowledge graph.
SELECT pg_ripple.rag_context('Which drugs interact with metformin?', k := 8);
-- Returns structured context ready to pass to an LLM.

-- Or query directly in SPARQL.
SELECT * FROM pg_ripple.sparql($$
  PREFIX ex: <https://example.org/>
  SELECT ?name WHERE {
    ex:alice <http://xmlns.com/foaf/0.1/knows>+ ?person .
    ?person  <http://xmlns.com/foaf/0.1/name>   ?name .
  }
$$);
-- Follows the foaf:knows relationship through any number of hops.
```

---

## The three problems pg_ripple solves

### 1. AI that knows what it knows — and can prove it

LLM responses fabricate facts because the model has no reliable source of truth. `rag_context()` retrieves structured graph context for any question in a single SQL call, grounding the LLM's answer in your data. Every retrieved fact links back to its source triple, PROV-O provenance, and audit log entry.

→ [AI Overview & Decision Tree](features/ai-overview.md) | [Chatbot Recipe](cookbook/grounded-chatbot.md)

### 2. Record linkage and deduplication at database speed

Deduplicate customer records across CRM and ERP, align external ontologies, or merge research entity mentions — all inside one PostgreSQL transaction. Knowledge-graph embeddings generate candidates; SHACL hard rules block unsafe merges; `owl:sameAs` canonicalization makes the merged view transparent to every downstream query.

→ [Record Linkage](features/record-linkage.md) | [Dedup Recipe](cookbook/dedupe-customers.md)

### 3. Knowledge graph + relational + vector in one transaction

No separate graph store, no separate vector index, no separate schema registry. One `pg_dump` captures everything. One ACID transaction spans triple writes, vector index updates, and ordinary table updates together.

→ [Architecture at a Glance](evaluate/architecture-glance.md) | [Comparison vs Alternatives](evaluate/comparison.md)

---

## Key capabilities

| Capability | What it does |
|---|---|
| **SPARQL 1.1** | W3C standard graph query — full conformance, < 10 ms typical |
| **SHACL validation** | Define and enforce data-quality rules — reject bad data on insert |
| **Datalog reasoning** | Derive new facts from RDFS, OWL RL/EL/QL, or custom rules |
| **Vector + graph hybrid** | SPARQL traversal combined with pgvector HNSW similarity search |
| **RAG pipeline** | `rag_context()` — one call retrieves structured LLM context |
| **NL → SPARQL** | `sparql_from_nl()` — auto-generate SPARQL from natural-language questions |
| **Record linkage** | KGE embeddings + SHACL gates + `owl:sameAs` canonicalization |
| **GraphRAG** | Microsoft GraphRAG ingest, enrich, validate, export pipeline |
| **Federation** | Query Wikidata, DBpedia, and other SPARQL endpoints alongside local data |
| **R2RML** | Generate an RDF graph from any existing PostgreSQL schema |
| **JSON-LD framing** | Export nested JSON documents shaped to your API contract |
| **SPARQL Protocol** | Standard HTTP endpoint via `pg_ripple_http` |

### Key numbers

| Metric | Value |
|---|---|
| Bulk load throughput | > 100 K triples/sec (commodity hardware) |
| SPARQL query latency | < 10 ms for typical star patterns |
| W3C SPARQL 1.1 | 100 % conformance |
| W3C SHACL Core | 100 % conformance |
| W3C OWL 2 RL | 100 % conformance |
| PostgreSQL version | 18 |

---

## Architecture at a glance

```
┌─────────────────────────────────────────────────┐
│                  PostgreSQL 18                   │
│  ┌───────────────────────────────────────────┐  │
│  │              pg_ripple extension           │  │
│  │  ┌─────────┐  ┌────────┐  ┌───────────┐  │  │
│  │  │Dictionary│  │ SPARQL │  │  Datalog   │  │  │
│  │  │ Encoder  │  │ Engine │  │  Engine    │  │  │
│  │  └────┬─────┘  └───┬────┘  └─────┬─────┘  │  │
│  │       │             │             │         │  │
│  │  ┌────┴─────────────┴─────────────┴─────┐  │  │
│  │  │     VP Tables (one per predicate)     │  │  │
│  │  │   HTAP: delta + main + merge worker   │  │  │
│  │  └──────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
         ▲                          ▲
         │ SQL                      │ HTTP
    Application              pg_ripple_http
```

Every IRI, literal, and blank node is mapped to a compact integer ID by the dictionary encoder. Data is stored in Vertical Partitioning (VP) tables — one table per unique predicate — with integer-only joins for fast query execution. The HTAP architecture separates read and write paths so that heavy loads do not block queries.

---

## Next steps

- **Evaluating?** [When to Use pg_ripple](evaluate/when-to-use.md), [Comparison vs Alternatives](evaluate/comparison.md), and [Performance Results](evaluate/performance-results.md) have what you need.
- **Ready to install?** Start with [Installation](getting-started/installation.md) then [Hello World in Five Minutes](getting-started/hello-world.md).
- **Building a RAG or AI application?** Read the [AI Overview](features/ai-overview.md) and the [Grounded Chatbot recipe](cookbook/grounded-chatbot.md).
- **Migrating from a relational database or Neo4j?** See [R2RML](features/r2rml.md) (relational → RDF) and [LPG/Cypher Mapping](features/lpg-mapping.md).
- **Deduplicating records?** See [Record Linkage](features/record-linkage.md) and the [Dedup recipe](cookbook/dedupe-customers.md).
- **Want the full picture?** The [Guided Tutorial](getting-started/tutorial.md) takes you from loading data to inference in 30 minutes.
- **Want to contribute?** See [Contributing](reference/contributing.md).
