# AI, RAG and LLM Integration — An Overview

pg_ripple is one of the few PostgreSQL extensions that brings **knowledge graphs, vector search, and large language models together in a single transaction**. This page is the front door to that capability. It explains *what each AI feature does*, *when to use which one*, and *which deep-dive page to read next*.

If you only have five minutes, read this page. If you have an hour, follow the links to the chapters below.

---

## Why pg_ripple for AI workloads?

Modern retrieval pipelines fall into one of three traps:

1. **Pure vector search** is great at fuzzy similarity ("things that look like X") but cannot answer questions that follow precise relationships ("which drugs interact with the medication my patient takes?").
2. **Pure graph queries** capture relationships exactly but cannot match a free-text question to the right entity.
3. **Pipelines that stitch the two together** (vector DB + graph DB + glue code) leak data between systems, suffer from sync lag, and cannot be rolled back atomically.

pg_ripple removes the third trap. Vectors live in `pgvector`, triples live in vertical-partitioning tables, and a single SQL transaction can update both at once. Every AI feature on this page is built on top of that foundation.

---

## The five AI features at a glance

| Feature | Question it answers | Read next |
|---|---|---|
| **Hybrid search** | "Find entities that look like *X* **and** satisfy this graph pattern." | [Vector & Hybrid Search](vector-and-hybrid-search.md) |
| **RAG pipeline** | "Build a context block I can drop into an LLM prompt." | [RAG Pipeline](../user-guide/rag-pipeline.md) |
| **Natural language → SPARQL** | "Translate this English question into a SPARQL query I can run." | [NL → SPARQL](nl-to-sparql.md) |
| **Knowledge-graph embeddings (KGE)** | "Learn entity vectors from the *graph structure itself*, not from text." | [Knowledge-Graph Embeddings](knowledge-graph-embeddings.md) |
| **Record linkage / entity resolution** | "Find pairs of entities that refer to the same real-world thing and merge them safely." | [Record Linkage](record-linkage.md) |

The features compose. A typical production pipeline uses three or four of them together — see the [Use Case Cookbook](../cookbook/index.md) for end-to-end recipes.

---

## Decision tree

Use this tree to pick the right feature for a new project.

```
Do you need to answer free-text questions over your data?
├─ No  → You probably do not need any AI feature. Use plain SPARQL.
└─ Yes → Does the question name a specific entity, or describe one?
         ├─ Names it    → Plain SPARQL is fastest. (e.g. "show me Alice's papers")
         └─ Describes it → You need retrieval. Continue.
                          │
                          Is the answer a single fact, or a passage of context?
                          ├─ Fact      → Use NL → SPARQL.
                          └─ Context   → Use the RAG pipeline (rag_context()).
                                         │
                                         Do your queries need precise relationships
                                         on top of similarity?
                                         ├─ No  → Hybrid search may suffice.
                                         └─ Yes → Combine SPARQL + similar() in one query.

Do you have entities arriving from multiple sources that may overlap?
└─ Yes → You need record linkage. Start with KGE for candidate generation,
         then SHACL for hard rules, then suggest_sameas + apply.

Do your existing embeddings only capture text, ignoring relationships?
└─ Yes → Train knowledge-graph embeddings (TransE / RotatE) and use them
         for entity alignment, recommendations, or link prediction.
```

---

## How the pieces fit together

```
                ┌─────────────────────────────────────┐
                │            Application              │
                └──────────────┬──────────────────────┘
                               │ SQL
                ┌──────────────┴──────────────────────┐
                │            pg_ripple                │
                │                                     │
   text query → │  rag_context()                      │ ← LLM prompt
                │     ├─ embed question (HTTP)        │
                │     ├─ HNSW vector recall  ─────────┼─→ pgvector
                │     ├─ SPARQL graph expansion ──────┼─→ VP tables
                │     └─ assemble JSON-LD             │
                │                                     │
   text query → │  sparql_from_nl()                   │ ← SPARQL string
                │     ├─ build VoID + SHACL context   │
                │     ├─ LLM /v1/chat/completions     │
                │     └─ parse + validate (spargebra) │
                │                                     │
   batch run  → │  embed_entities()  ─────────────────┼─→ pgvector
   batch run  → │  kge_train()       ─────────────────┼─→ kge_embeddings
   batch run  → │  suggest_sameas()  ─────────────────┼─→ owl:sameAs triples
                └─────────────────────────────────────┘
                               │
                ┌──────────────┴──────────────────────┐
                │   PostgreSQL transaction boundary   │
                └─────────────────────────────────────┘
```

Everything inside the dashed box runs inside a single PostgreSQL transaction. If anything fails, the whole pipeline rolls back — there is no half-updated vector store to clean up.

---

## Prerequisites

| Requirement | Needed by |
|---|---|
| `CREATE EXTENSION vector;` (pgvector) | All features except NL → SPARQL |
| `pg_ripple.embedding_api_url` configured | RAG, embed_entities, hybrid search via text input |
| `pg_ripple.llm_endpoint` configured | NL → SPARQL, the optional second stage of `rag_context()` |
| API key in environment variable | LLM and embedding endpoints — keys are **never** stored in the database |
| `pg_ripple.kge_enabled = on` | KGE training and `find_alignments()` |

All AI features degrade gracefully when their dependencies are missing — they emit a `WARNING` and return zero rows rather than raising an `ERROR`. You can ship code that *uses* these features into a CI environment that does *not* have an LLM endpoint configured.

---

## Security notes

- **API keys are never stored in PostgreSQL.** Configure the *name* of an environment variable (e.g. `pg_ripple.llm_api_key_env = 'OPENAI_API_KEY'`) and the extension reads the secret at call time.
- **Outbound HTTP calls require an allowlisted endpoint.** Both LLM endpoints (registered via `llm_endpoint` / `embedding_api_url`) and federated SPARQL services (registered via `register_endpoint()`) are checked against an allowlist on every call. This prevents Server-Side Request Forgery (SSRF).
- **PII in prompts.** `rag_context()` and `sparql_from_nl()` send graph excerpts to the configured LLM. Use named-graph row-level security (see [Multi-Tenant Graphs](multi-tenant-graphs.md)) to keep tenant data out of prompts you do not control.

---

## Where to go next

- **Want to try it in five minutes?** [Cookbook: Chatbot grounded in a knowledge graph](../cookbook/grounded-chatbot.md)
- **Already have a knowledge graph and want to add RAG?** [RAG Pipeline](../user-guide/rag-pipeline.md)
- **Merging customer data from multiple systems?** [Record Linkage](record-linkage.md)
- **Building a recommendation engine over a graph?** [Knowledge-Graph Embeddings](knowledge-graph-embeddings.md)
- **Need a structured prompt for OpenAI structured outputs?** [Exporting and Sharing — JSON-LD framing](exporting-and-sharing.md)
