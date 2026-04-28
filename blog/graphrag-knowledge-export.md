[← Back to Blog Index](README.md)

# GraphRAG: Feeding LLMs with Structured Knowledge

## From knowledge graphs to grounded, hallucination-resistant answers

---

Large language models hallucinate. Ask GPT-4 about your company's org chart and it'll confidently invent employees, departments, and reporting chains. Ask it about drug interactions and it'll cite studies that don't exist.

The fix is retrieval-augmented generation: before the LLM generates an answer, retrieve relevant context from a trusted data source and include it in the prompt. The LLM still generates the language, but the facts come from your data.

Most RAG implementations use vector search as the retrieval engine: embed the question, find similar text chunks, stuff them into the prompt. This works for document-centric questions. It fails for structured questions — "who reports to the VP of Engineering?" or "what medications are contraindicated for this patient?" — because the answer requires traversing relationships, not finding similar text.

GraphRAG uses a knowledge graph as the retrieval source. The relationships are explicit, the types are precise, and the traversal is exact. pg_ripple implements the full pipeline.

---

## The RAG Pipeline

Standard RAG:

```
User question → Embed question → Vector search → Top-K chunks → LLM prompt → Answer
```

GraphRAG with pg_ripple:

```
User question → SPARQL query generation → Graph traversal → Structured context → LLM prompt → Answer
```

The critical difference: vector search finds text that's *near* the question. Graph traversal finds facts that *answer* the question. "Who reports to Alice?" isn't a similarity problem — it's a lookup + traversal.

---

## The rag_context() Function

pg_ripple provides a single function that handles the full retrieval pipeline:

```sql
SELECT pg_ripple.rag_context(
  question => 'What medications are contraindicated for patients taking warfarin?',
  max_triples => 100,
  include_types => true,
  include_labels => true
);
```

This function:

1. **Generates a SPARQL query** from the natural language question (using the configured LLM endpoint via `sparql_from_nl()`).
2. **Executes the SPARQL query** against the local knowledge graph.
3. **Serializes the results** as a structured context block suitable for LLM consumption.
4. **Returns the context** as text that can be included in an LLM prompt.

The output might look like:

```
Context from knowledge graph:

Warfarin (ex:warfarin) is a drug of type Anticoagulant.
Warfarin is contraindicated with:
  - Aspirin (ex:aspirin) — reason: increased bleeding risk
  - Ibuprofen (ex:ibuprofen) — reason: increased bleeding risk
  - Vitamin K supplements (ex:vitk) — reason: reduced efficacy

Warfarin interacts with:
  - Amiodarone (ex:amiodarone) — severity: major — effect: increased INR
  - Fluconazole (ex:fluconazole) — severity: major — effect: increased warfarin levels
```

This context is precise, sourced from the knowledge graph, and includes provenance (the entity URIs). The LLM uses it to generate a natural-language answer grounded in real data.

---

## Why Graph Context Beats Chunk Context

Consider the question: "What are the side effects of drug X and which of them are also side effects of drug Y?"

**Vector RAG:** Searches for text chunks about drug X and drug Y. Returns paragraphs from drug monographs that mention side effects. The LLM must extract side effect lists from unstructured text and compute the intersection. This is error-prone — LLMs are bad at set operations over unstructured data.

**Graph RAG:** Executes a SPARQL query:

```sparql
SELECT ?side_effect ?label WHERE {
  ex:drugX ex:hasSideEffect ?side_effect .
  ex:drugY ex:hasSideEffect ?side_effect .
  ?side_effect rdfs:label ?label .
}
```

Returns exactly the intersection. No text extraction. No set computation by the LLM. The answer is precise because the computation was precise.

The difference is structural vs. textual retrieval. When the question has a structural answer — relationships, types, aggregates, intersections — graph retrieval is categorically better.

---

## Datalog-Enriched Context

The knowledge graph you query isn't limited to explicit facts. pg_ripple's Datalog engine can derive implicit facts that enrich the context:

```sql
-- Datalog rule: propagate drug category contraindications
SELECT pg_ripple.datalog_add_rule(
  'contraindicated(D1, D2) :-
     drug_class(D1, C1), drug_class(D2, C2),
     class_contraindication(C1, C2).'
);
SELECT pg_ripple.datalog_infer();
```

Now when `rag_context()` retrieves contraindications for warfarin, it includes both:
- Explicit contraindications (directly stated in the data)
- Inferred contraindications (derived through drug class rules)

The LLM's answer is more complete because the knowledge graph is more complete.

---

## The Parquet Export Path

For integration with Microsoft's GraphRAG toolkit (or any external graph analysis pipeline), pg_ripple exports knowledge graph data as Parquet files:

```sql
SELECT pg_ripple.graphrag_export(
  output_dir => '/data/graphrag/',
  format => 'parquet',
  include_inferred => true,
  include_embeddings => true
);
```

This produces:
- `entities.parquet` — nodes with labels, types, and embeddings
- `relationships.parquet` — edges with types and properties
- `communities.parquet` — community assignments (if community detection has been run)

The Parquet files can be loaded directly into Microsoft GraphRAG's indexing pipeline, LangChain, LlamaIndex, or any tool that reads Parquet.

---

## Community Detection for Summarization

GraphRAG's most powerful feature is hierarchical community summarization: cluster the graph into communities, summarize each community with an LLM, and use the summaries as high-level context for broad questions.

pg_ripple supports this through Datalog-based community detection:

```sql
-- Detect communities using label propagation
SELECT pg_ripple.detect_communities(
  predicate => 'ex:relatedTo',
  algorithm => 'label_propagation',
  max_iterations => 20
);
```

The communities are stored as triples (`?entity ex:community ?community_id`), which means they're queryable with SPARQL and exportable with the Parquet export.

For questions like "What are the main research areas in our publication graph?", the community summaries provide high-level answers that individual entity lookups can't.

---

## SHACL Quality Enforcement

Garbage in, garbage out — even for LLMs. If the knowledge graph has data quality issues (missing labels, wrong types, dangling references), the RAG context will be poor and the LLM's answers will suffer.

pg_ripple's SHACL integration provides a quality gate:

```sql
-- Validate graph quality before RAG export
SELECT * FROM pg_ripple.shacl_validate()
WHERE severity = 'Violation';
```

Common quality checks for RAG:

- Every entity has a `rdfs:label` (so the context includes human-readable names).
- Every relationship has a defined type (so the context is specific, not generic).
- Critical entities have embeddings (so hybrid retrieval works).
- No dangling references (so traversal doesn't hit dead ends).

Running SHACL validation before an LLM workflow ensures the context is clean. This is especially important for healthcare, legal, and financial knowledge graphs where incorrect context can lead to harmful answers.

---

## End-to-End Example

```sql
-- 1. Load domain knowledge
SELECT pg_ripple.load_turtle_file('/data/pharma_ontology.ttl');
SELECT pg_ripple.load_turtle_file('/data/drug_interactions.ttl');

-- 2. Run inference
SELECT pg_ripple.datalog_load_ruleset('owl2rl');
SELECT pg_ripple.datalog_infer();

-- 3. Compute embeddings
SELECT pg_ripple.compute_embeddings(
  predicate => 'rdfs:label',
  context_depth => 2
);

-- 4. Validate quality
SELECT count(*) FROM pg_ripple.shacl_validate()
WHERE severity = 'Violation';
-- 0 violations

-- 5. Answer a question
SELECT pg_ripple.rag_context(
  question => 'What are the contraindications for prescribing warfarin to a patient already on aspirin?',
  max_triples => 50
);
```

The output is a structured context block that any LLM can use to generate a grounded, accurate answer — with provenance, without hallucination, and with the full richness of the knowledge graph behind it.

---

## When to Use Graph RAG vs. Vector RAG

| Question Type | Best Retrieval | Why |
|---------------|---------------|-----|
| "Find documents about X" | Vector | Semantic similarity over text |
| "What is the relationship between A and B?" | Graph | Explicit relationship traversal |
| "List all X that satisfy condition Y" | Graph | Structured filtering |
| "Summarize the main themes in this dataset" | Graph (communities) | Hierarchical summarization |
| "Find things similar to X that also have property Y" | Hybrid | Filter + rank |

Most enterprise knowledge management questions are structural — they ask about relationships, categories, hierarchies, and constraints. These are graph questions. Vector RAG is a fallback for when the question is too vague for a structured query.

pg_ripple supports both, in the same system, without a separate vector database or a separate graph database. That's the advantage of building on PostgreSQL: everything is one `SELECT` away.
