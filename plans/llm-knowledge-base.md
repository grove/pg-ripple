# pg_ripple + pg_trickle as a Living LLM Knowledge Base

> **Date:** 2026-05-02  
> **Status:** Strategy document — not a committed roadmap item  
> **Inspiration:** [Karpathy's compiler analogy for LLM knowledge bases](https://www.mindstudio.ai/blog/karpathy-llm-knowledge-base-architecture-compiler-analogy)  
> **Related plans:** [GraphRAG synergy](graphrag.md) · [pg-trickle relay](pg_trickle_relay_integration.md) · [future directions](future-directions.md) · [ROADMAP](../ROADMAP.md)

---

## 1. The idea in one paragraph

Andrej Karpathy proposed treating knowledge preparation as compilation. You do
not run source code directly; you compile it into a form the machine can execute
quickly and reliably. The same pattern applies to knowledge: take messy human
documents, compile them into structured facts, summaries, relationships, and
questions, then query the compiled result at runtime rather than re-reading raw
text every time.

pg_ripple and pg_trickle make this idea significantly stronger than the original
description, because they turn a compiled knowledge base into a **living one**:
one that updates incrementally as sources change, validates its own quality,
ranks its own entries by importance and freshness, and publishes semantic change
events to downstream consumers.

The product this enables is not simply "RAG with RDF". It is a
**knowledge build system inside PostgreSQL**: sources in, structured governed
knowledge out, incremental updates, and a full audit trail.

---

## 2. The compiler analogy

The core insight is that raw documents are a bad runtime format for AI systems.
They repeat themselves, bury assumptions, rely on context that lives elsewhere,
and require the LLM to reinterpret prose from scratch on every query.

The standard fix is RAG: split documents into chunks, embed them, retrieve
similar chunks at query time. That helps, but it has well-known limits. A chunk
can lose the context that gave it meaning. Similarity is not the same as
correctness. Multi-document reasoning is fragile. Answers vary with chunking
and retrieval settings.

Compilation does more work before query time:

| Software compiler | LLM knowledge compiler |
|---|---|
| Source code | Raw documents, PDFs, tickets, transcripts, events |
| Compiler | LLM workflow that extracts and structures meaning |
| Compiled binary | Governed knowledge graph |
| Runtime | Querying compiled facts, summaries, and relationships |
| Compiler errors | Low-confidence extractions, contradictions, missing evidence |
| Incremental build | Reprocess only changed fragments and their dependants |

The hardest part of this design is **incremental compilation**: when one
document changes, only the knowledge that depends on that document should
rebuild. That is exactly what pg_ripple and pg_trickle solve together.

---

## 3. What pg_ripple already provides

pg_ripple already covers most of what a compiled knowledge base needs.

| Need | pg_ripple capability |
|---|---|
| Store structured facts | RDF triples in PostgreSQL VP storage |
| Query relationships | SPARQL 1.1 with full property paths and aggregates |
| Natural-language queries | `sparql_from_nl()` — English to SPARQL |
| LLM-ready context | `rag_context()` — graph facts formatted for a prompt |
| Source attribution | Named graphs and PROV-O provenance |
| Facts about facts | RDF-star annotations for confidence, evidence, timestamps |
| Assign extraction confidence | `load_triples_with_confidence()`, `pg:confidence()` (v0.87) |
| Propagate source trust | `pg:sourceTrust` predicate, automatic PROV-O Datalog rules (v0.87) |
| Probabilistic inference | Datalog rules with `@weight` and noisy-OR combination (v0.87) |
| Fuzzy entity matching | `pg:fuzzy_match()`, `pg:token_set_ratio()`, GIN trigram index (v0.87) |
| Numeric data quality scores | `shacl_score()`, `sh:severityWeight`, `shacl_report_scored()` (v0.87) |
| Validate knowledge quality | SHACL shapes as data quality contracts |
| Infer new knowledge | Datalog and RDFS/OWL 2 RL rules |
| Resolve duplicate entities | `owl:sameAs` canonicalization |
| Rank entities by importance | PageRank, topic-sensitive scoring, temporal decay (v0.88) |
| Detect bridge concepts | Betweenness centrality, eigenvector centrality (v0.88) |
| Find recent authorities | Katz centrality with time-aware edge weights (v0.88) |
| Incremental importance updates | pg-trickle K-hop rank propagation in milliseconds (v0.88) |
| Hybrid graph + vector retrieval | pgvector integration and graph-contextualized embeddings |
| GraphRAG export | GraphRAG Parquet export and community detection |
| Stream knowledge changes | CDC subscriptions and JSON-LD event output |

The missing product layer is the step that takes raw human-readable sources and
reliably turns them into that compiled artifact. That is `pg_ripple_compile`.

---

## 4. What pg_trickle adds

The article treats incremental compilation as a hard problem. pg_trickle gives
us a practical solution.

pg_trickle can:

- **Ingest** new source material from Kafka, NATS, HTTP, SQS, Redis Streams, and
  similar systems into PostgreSQL as stream tables.
- **Propagate changes** through derived views using Z-set differential dataflow,
  updating only what actually changed.
- **Publish outbound events** when compiled knowledge changes, so downstream
  agents and systems react immediately.
- **Share transactions** with graph writes, validation, and messaging — all in
  one PostgreSQL transaction, with no partial updates.

With pg_trickle, the pipeline becomes:

```
source changes -> recompile only affected fragments -> validate -> update graph
               -> refresh importance rankings (K-hop) -> publish semantic events
```

That transforms the design from a batch re-indexer into a **build system for
living knowledge**.

---

## 5. The product: pg_ripple_compile

The compiler layer is a distinct concern from storage and transport.

| Component | Responsibility |
|---|---|
| **pg_ripple** | Database truth: graph writes, validation, rules, provenance, queries |
| **pg_ripple_compile** | Long-running AI work: document fetching, chunking, LLM calls, structured output, retries |
| **pg_trickle** | Event transport: inbound feeds, change propagation, outboxes, downstream delivery |

The product promise:

> Point pg_ripple at a stream or corpus of human-readable knowledge. It compiles
> that material into a governed, queryable, incrementally maintained knowledge
> graph that humans, agents, and applications can use at runtime.

This is deliberately database-like. The compiled knowledge is durable,
queryable, auditable, and operationally safe.

---

## 6. Architecture

```
               SOURCE MATERIAL
  Documents, PDFs, tickets, transcripts, events, APIs
       |                  |                    |
       | direct load      | pg_trickle          | scheduled fetch
       |                  | reverse relay       |
       v                  v                    v
  +---------------------------------------------------+
  |  Source registry and inbox tables                 |
  |  * what arrived * where from * content hash       |
  |  * which compiler profile to use * status         |
  +----------------------+----------------------------+
                         |
                         v
               COMPILATION (pg_ripple_compile)
  +---------------------------------------------------+
  |  * split document into stable sections             |
  |  * extract facts, relationships, entities          |
  |  * generate summaries and Q&A pairs                |
  |  * attach confidence and evidence                  |
  |  * record warnings and contradictions              |
  +----------------------+----------------------------+
                         |
                         v
               COMPILED KNOWLEDGE (pg_ripple)
  +---------------------------------------------------+
  |  * atomic facts with confidence and evidence       |
  |  * entity pages with summaries and embeddings      |
  |  * topic index graphs ranked by PageRank           |
  |  * dependency graph for incremental updates        |
  |  * SHACL quality gates before publication          |
  |  * Datalog inference and uncertain knowledge       |
  +----------+-------------------------+---------------+
             |                         |
             v                         v
  RUNTIME QUERY                  CHANGE OUTPUT
  SPARQL, rag_context(),         pg_trickle outbox:
  sparql_from_nl(),              entity.updated,
  GraphRAG summaries,            policy.contradiction.detected,
  agent navigation               summary.invalidated,
                                 source.needs_review
```

The **source registry** is the key enabler of incremental compilation. The
system must remember what it compiled, when, with which prompt version, what
source hash it saw, and what knowledge depends on what. Without that memory it
is a batch re-indexer. With it, it is a build system.

---

## 7. What gets stored

### 7.1 Source records

For every source document or event, the registry stores:

- Source URI or external ID
- Source type (document, ticket, transcript, event stream) and system
  (Confluence, GitHub, Zendesk, Kafka)
- Content hash and last-seen timestamp
- Named graph IRI where compiled assertions live
- Compile status and last compile timestamp

This answers the operational questions that matter: What is compiled? What
changed? What failed? What is stale?

### 7.2 Source fragments

Large documents must be split into stable sections — a Markdown heading, a PDF
page, a ticket message, a transcript time segment. Fragment-level tracking is
what makes incremental compilation practical: a 50-page document should not
fully recompile when one paragraph changes.

### 7.3 Compiler profiles

Different domains need different extraction instructions. A compiler profile
defines:

- Prompt template and expected output schema
- SHACL validation rules, with `sh:severityWeight` annotations so critical
  rules weigh more in the numeric quality score
- Optional Datalog rules with `@weight` annotations for probabilistic
  confidence propagation through derived facts
- Default extraction confidence level assigned at ingest time
- Preferred LLM and embedding models, and maximum fragment size

Profile versioning matters. Changing the prompt is changing the compiler. The
system must know which knowledge was produced by which profile version.

### 7.4 Compiler runs and diagnostics

Every compile attempt leaves a record:

- Source and fragment compiled, profile and model used, success or failure
- Token count, output hash
- Warnings: unresolved entities, weak evidence, low confidence, source
  contradictions, missing required fields, SHACL failures, output schema
  mismatches

These are the "compiler errors" from the analogy. A knowledge system that hides
its work is not trustworthy.

### 7.5 Compiled artifacts and dependency graph

A compiled artifact can be a fact, summary, entity page, Q&A pair, embedding,
index entry, or diagnostic. Each artifact records what it depends on: which
source fragment, which entity, which compiler profile, which rule set, and which
other artifacts.

That dependency graph enables incremental recompilation. When a support ticket
changes, the system asks: which facts came from this ticket? Which entity pages
used those facts? Which summaries mentioned those entities? Only those artifacts
rebuild.

---

## 8. The compiled knowledge

The output is not a blob of text or a collection of Markdown pages. Those can
be generated for display, but the stored form is graph-native.

### 8.1 Atomic facts with confidence

Every extracted fact is an RDF triple. Additional context — confidence, source
quote, evidence span — is stored as an RDF-star annotation.

**How confidence flows through the pipeline.**
`load_triples_with_confidence(data, confidence, format, graph_uri)` assigns a
score in [0, 1] to each fact at ingest time. The score reflects the extraction
model's reliability, the source tier, and any per-field confidence returned in
structured LLM output. Datalog rules with `@weight(FLOAT)` annotations propagate
confidence through inference: a chain with weights 0.9 x 0.8 x 0.7 produces a
conclusion with confidence ~0.5. When multiple independent sources support the
same conclusion, noisy-OR combination raises the joint confidence automatically —
three sources at 0.6, 0.7, and 0.6 reach ~0.94 together. `pg:confidence(?s, ?p, ?o)`
retrieves any fact's score inline in SPARQL, usable in `FILTER`, `BIND`, and
`ORDER BY`.

**Source trust.** Registering `pg:sourceTrust 0.9` on a source named graph and
enabling `prov_confidence = on` causes built-in Datalog rules to automatically
populate confidence for every triple from that source. No per-triple annotation
is needed from the compiler.

**Quality gates.** `shacl_score(graph_iri)` returns a float in [0, 1] for the
entire compiled graph. The compiler worker uses this as a publication gate:
score >= 0.9 sends to the trusted graph; score < 0.75 sends to the review
queue. `shacl_report_scored()` provides a per-shape breakdown for the review UI.

### 8.2 Entity pages

An entity page is an entity-centered graph bundle:

- Name, aliases, type, canonical ID
- Duplicate or equivalent entity links (`owl:sameAs`)
- Short, medium, and long summaries
- Key relationships and source coverage
- Known contradictions and confidence score
- PageRank score — overall and per topic (v0.88)
- Centrality metrics: betweenness (bridge role), closeness (hub proximity) (v0.88)
- Embedding vector and community membership

Applications render this as a wiki page, JSON-LD document, API response, or LLM
context block. The stored form stays graph-native.

### 8.3 Summaries at multiple levels

The compiler generates summaries for source fragments, whole documents,
entities, topics, communities of related entities, and the corpus as a whole.
Every summary links back to the artifacts it depends on, enabling invalidation
when those artifacts change.

### 8.4 Generated questions and answers

The compiler generates question-answer pairs from source material for three
purposes:

- **Testing**: verify the knowledge base still answers correctly after an update.
- **Discovery**: help users find what the knowledge base knows.
- **Query tuning**: give `sparql_from_nl()` working examples to improve
  NL-to-SPARQL translation.

Each Q&A pair records the evidence it depends on. When that evidence updates,
the pair is flagged for regeneration.

### 8.5 The knowledge index graph

Agents need a map of the knowledge base. The index graph provides it: top-level
topics, key entities per topic ranked by importance, source coverage, freshness
metadata, representative questions, and community summaries.

With v0.88 PageRank, the index ordering is computed automatically from the
structure of the knowledge itself.

- **Freshness-aware ranking.** Temporal decay (PR-TEMPORAL-01) weights edges by
  the age of the compiled source. Recently compiled facts push more importance
  than stale ones, so the index naturally reflects recency.
- **Trust-propagating ranking.** Confidence-weighted edges (PR-CONF-01) mean
  high-trust source citations carry more rank mass than uncertain LLM
  extractions.
- **Per-domain ranking.** Topic-sensitive scoring (PR-TOPIC-01) stores
  independent ranking runs per topic label. A healthcare agent and a finance
  agent each receive a relevance-ordered index with zero extra query cost.
- **Bridge concept detection.** Betweenness centrality (PR-CENTRALITY-01)
  surfaces entities that connect otherwise separate topic clusters — entities
  that PageRank alone would miss but that an index graph must include.
- **Quality-gated ranking.** SHACL-aware ranking (PR-SHACL-01) excludes nodes
  that failed quality checks, keeping low-quality compiled facts from inflating
  the index.
- **Live incremental ranking.** The pg-trickle incremental refresh
  (PR-TRICKLE-01) propagates importance changes via bounded K-hop updates
  within seconds of a new compiled source. The `stale`/`stale_since` columns
  on `pagerank_scores` let applications distinguish exact from approximate
  scores.

---

## 9. Query paths

The runtime rule is simple: use compiled knowledge first; use raw source text
only to verify evidence.

### 9.1 Exact relationship questions

Questions like "Which customers requested both SSO and audit logging?", "Which
policies apply to contractors in Germany?", or "Which features have five or more
high-confidence pain points this month?" are relationship questions. They are
answered with SPARQL and Datalog over compiled facts, not by asking an LLM to do
set logic over raw text chunks.

### 9.2 Sensemaking questions

Questions like "What are the main themes in recent customer feedback?" or "What
changed in the compliance corpus this week?" start with the PageRank-ordered
index graph, communities, and summaries, then drill down into exact facts and
evidence. `pg:topN_approx()` returns approximate top-K entities sub-millisecond
for interactive sensemaking queries. Topic-sensitive scoring ensures the ranking
reflects the agent's domain, not a global average.

### 9.3 Hybrid fallback

Vector search remains useful but should search over cleaner artifacts —
summaries, entity descriptions, evidence spans, generated Q&A — rather than raw
chunks. When a vector search finds something relevant, it returns graph artifact
identifiers that resolve to structured facts, provenance, and confidence.

`pg:fuzzy_match(a, b)` and `pg:token_set_ratio(a, b)`, backed by a GIN trigram
index, enable fuzzy entity-name matching so a query for "SSO" finds
"Single Sign-On" and "sso login" without exact string equality.
`pg:confPath(predicate, min_confidence)` traverses the compiled graph along
confidence-gated paths, preventing low-confidence edges from contaminating
multi-hop reasoning chains fed to the LLM.

---

## 10. What makes this novel

The article describes a compiled knowledge base. pg_ripple + pg_trickle go
further on seven fronts.

### 10.1 Live incremental compilation

The compiled output is not a static wiki refreshed overnight. It is a live
graph:

1. A source fragment changes.
2. The dependency graph identifies affected facts, summaries, entity pages, and
   Q&A pairs.
3. Only the affected artifacts rebuild.
4. SHACL validates the new output.
5. Datalog derives follow-on facts.
6. pg-trickle publishes semantic change events.
7. Downstream systems update in near real time.

This is the distinction between a batch re-indexer and a build system.

### 10.2 Knowledge CI/CD

Before new compiled knowledge is published, the system runs checks:

- Does the LLM output match the expected schema?
- Do required fields exist?
- Does `shacl_score()` exceed the publication threshold?
- Do new facts introduce contradictions?
- Do important Q&A pairs still answer correctly?
- Which answers changed?

This is a CI/CD pipeline for knowledge, not software.

### 10.3 Semantic pull requests

When a document changes, users normally review text diffs. A compiled knowledge
system can show a more informative diff:

- Facts added or removed
- Relationships changed
- Entities merged or split
- Summaries invalidated
- Contradictions introduced or resolved
- Generated answers affected
- Importance scores shifted, and why — shown by `explain_pagerank()`

Domain experts review the knowledge change instead of reading every sentence of
the source diff.

### 10.4 Uncertain knowledge and graded trust

Not all sources are equally trustworthy and not all extracted facts are equally
certain. v0.87 delivers a complete uncertain knowledge engine that makes this
concrete throughout the pipeline.

**At ingest.** `load_triples_with_confidence()` assigns extraction confidence at
load time. A primary source compiled by a reliable model gets 0.95; a web scrape
with a weaker prompt gets 0.6. A single GUC threshold routes facts below the
cutoff to the review graph instead of the trusted graph.

**Through inference.** Datalog rules with `@weight(FLOAT)` multiply body-atom
confidences by the rule weight. A chain from a medium-trust source
(0.9 x 0.8 x 0.7) produces a conclusion with confidence ~0.5, visible via
`pg:confidence()`. When multiple independent paths support the same conclusion,
noisy-OR combination raises the joint confidence automatically.

**Via source trust.** A single `pg:sourceTrust 0.9` annotation on a named graph
plus `prov_confidence = on` is enough for built-in Datalog rules to propagate
trust to every triple from that source automatically.

**As a quality gate.** `shacl_score(graph_iri)` returns a float in [0, 1].
Shapes declare `sh:severityWeight` so critical rules count more than cosmetic
ones. Graphs below 0.75 route to the review queue; `shacl_report_scored()`
explains which shapes reduced the score.

**At query time.** `FILTER(pg:confidence(?s, ?p, ?o) > 0.7)` restricts any
SPARQL query to well-supported facts. `pg:confPath(predicate, min_confidence)`
traverses only confident edges, blocking uncertain extractions from contaminating
multi-hop LLM context.

**In the importance ranking.** v0.88 confidence-weighted PageRank (PR-CONF-01)
closes the loop: entity importance reflects how *trustworthy* the incoming
citations are, not just how many there are. A policy backed by three
high-confidence extractions outranks one backed by five uncertain ones,
automatically.

**On export.** `export_turtle_with_confidence()` emits every fact with its
confidence as an RDF-star annotation. Downstream consumers see not just the
fact, but how much to trust it.

This lets the system answer:
> The strongest supported answer is A with confidence 0.82. Source B disagrees,
> but it is older and has lower trust.

### 10.5 Agent memory bus

pg_trickle publishes typed semantic events that agents subscribe to:

- `entity.updated`
- `policy.changed`
- `policy.contradiction.detected`
- `summary.invalidated`
- `source.needs_review`
- `answer_package.changed`

Agents react to meaningful knowledge changes instead of polling a vector store.

### 10.6 Human correction loops

1. The compiler extracts a fact from a transcript.
2. A domain expert corrects it in a review UI.
3. The correction travels back through pg_trickle.
4. pg_ripple stores it in a higher-priority human-review named graph.
5. Conflict rules prefer the human-reviewed fact over the lower-confidence
   LLM extraction.
6. The corrected knowledge is published downstream.

The LLM is an assistant; the expert is the source of truth.

### 10.7 Knowledge packages

A compiled corpus can be packaged for distribution:

- Named RDF graphs plus SHACL shapes and Datalog rules
- Compiler profile version and prompt hash
- Summaries, generated Q&A pairs, embeddings metadata
- Provenance manifest and evaluation set

Install a package into pg_ripple, validate it, and query it immediately.

### 10.8 Federated compiled knowledge

Organizations that cannot centralize all raw documents can compile locally and
share only approved facts. v0.88 federation blend mode (PR-FED-01) extends this
at query time: `pagerank_run()` pulls edge triples from remote SERVICE endpoints
into a temporary local graph, computes a global importance ranking across all
departments, then discards the raw remote triples. Confidence-gated federation
(PR-FED-CONF-01) filters remote edges below `federation_minimum_confidence`
before they influence the ranking, preventing low-quality external sources from
distorting global scores.

---

## 11. Example use cases

### 11.1 Enterprise documentation

**Sources:** Confluence pages, GitHub Markdown, policy PDFs, decision logs

**Compiled:** Policies, owners, effective dates, required approvals, exceptions,
related systems, contradictions between documents.

**Why it matters:** Policy questions need exact scope, dates, and exceptions —
that is structured graph reasoning, not chunk retrieval. Temporal decay (v0.88)
surfaces the most recently updated policies at the top of the index, so agents
find the authoritative current version before superseded ones.

### 11.2 Product intelligence

**Sources:** Support tickets, call transcripts, CRM notes, feedback forms

**Compiled:** Customers, accounts, features, pain points, sentiment, urgency,
evidence quotes, duplicate requests.

**Why it matters:** The result is not a pile of summaries. It is a live product
graph: which accounts asked for what, how confident we are, what evidence
supports it, and how the trend changed. Topic-sensitive PageRank (PR-TOPIC-01)
ranks features by how heavily they are requested within a given product area,
surfacing the top pain points automatically as the feedback graph grows.

### 11.3 Research library

**Sources:** Papers, lab notes, benchmark reports, citations, experiment metadata

**Compiled:** Claims, methods, datasets, metrics, baselines, limitations,
conflicting results, open questions.

**Why it matters:** A new paper can strengthen or contradict existing claims.
The system shows what changed in the research map, not just a summary of the new
paper. Eigenvector centrality (v0.88) identifies the claims backed by the
strongest mutually corroborating chains of evidence, distinguishing them from
popular but weakly-supported assertions.

### 11.4 Operations memory

**Sources:** Alerts, incident reports, deployment events, runbook changes

**Compiled:** Symptoms, affected services, owners, deploys, probable causes,
remediation steps, runbook links.

**Why it matters:** "What changed before this alert pattern appeared?" requires
evidence from previous incidents, deployments, and runbooks — exactly what
structured graph reasoning over compiled operational knowledge provides.

---

## 12. First version

The first version should prove the idea with one strong end-to-end flow: a
source changes, only the affected part recompiles, the graph is updated,
validation runs, and a meaningful change event is published.

### 12.1 MVP features

1. Source registry: sources, fragments, profiles, runs, diagnostics.
2. Compiler profiles with prompt template, version, output schema, and
   validation rules.
3. `pg_ripple_compile` worker with an OpenAI-compatible endpoint and a
   deterministic mock mode for CI.
4. Compiled artifacts: atomic facts, summaries, entity pages, Q&A pairs,
   diagnostics.
5. Statement-level provenance and confidence via `load_triples_with_confidence()`.
6. `shacl_score()` as a numeric publication gate; graphs below threshold route
   to the diagnostic review queue.
7. Named graph write modes: append, replace, and review.
8. A topic index graph with top-N entities ranked by PageRank and temporal
   decay.
9. pg-trickle inbox for source events and outbox for artifact change events.

### 12.2 What to avoid in the first version

- No custom UI.
- No general workflow DAG editor.
- No large connector catalog.
- No automatic trust in LLM-extracted facts.
- No full-corpus re-summarization on every change.
- No hidden destructive deletes during recompilation.

---

## 13. Delivery phases

### Phase 1 — Foundation

- Source and compiler catalogs.
- SQL APIs for registering profiles and enqueueing compilation.
- `pg_ripple_compile` companion worker.
- Structured LLM output validation and mock mode for CI.
- End-to-end compile of a small Markdown or ticket corpus.

### Phase 2 — Incremental compilation

- Stable document fragmentation.
- Artifact dependency tracking.
- Stale-artifact invalidation and selective recompilation.
- Diff mode for compiled triples.
- pg-trickle stream tables for compile queues and outboxes.
- `explain_compilation()` — what depends on what.

### Phase 3 — Graph-native knowledge wiki

- Entity pages with summaries and embeddings.
- Topic index graphs ranked by PageRank, topic-sensitive scoring, and
  centrality measures.
- Multi-level summaries and Q&A pairs with evidence links.
- Compiled artifacts integrated into `rag_context()`.
- Community summaries from the compiled graph.

### Phase 4 — Review and trust

- Review graphs for human approval and conflict policies.
- Source trust via `pg:sourceTrust` and `prov_confidence = on`.
- Probabilistic Datalog with `@weight` for confidence propagation.
- `shacl_score()` as a numeric publish gate with `shacl_report_scored()` in
  the review UI.
- Semantic diffs for reviewers, including `explain_pagerank()` for importance
  shifts.
- Confidence-weighted PageRank so entity importance reflects source trust.

### Phase 5 — Agent ecosystem

- Typed semantic change events for agents via pg-trickle.
- Cached answer package invalidation.
- Knowledge package export and import.
- Federated compiled knowledge with confidence-gated remote edges.
- Benchmark against vector RAG and static GraphRAG.

---

## 14. How to measure success

### Compile-time metrics

| Metric | Target |
|---|---|
| Fragment skip rate (unchanged fragments) | > 80% on re-runs |
| LLM structured-output failure rate | < 5% |
| Mean `pg:confidence()` of extracted facts | > 0.75 per run |
| `shacl_score()` of published graphs | > 0.9 |
| Contradiction rate | < 2% |
| Facts with evidence attached | > 95% |

### Incremental-update metrics

| Metric | Target |
|---|---|
| Source change to updated graph | < 10 s |
| Source change to refreshed summaries | < 30 s |
| Source change to outbound event | < 5 s |
| Unnecessary full recompiles avoided | > 90% of updates |
| PageRank score stabilization after new compile | < 5 s via PR-TRICKLE-01 |

### Query-time metrics

| Metric | Target |
|---|---|
| PageRank top-10 precision vs. human judgement | > 0.8 |
| SPARQL generation repair rate | < 10% |
| Accuracy on generated QA sets | > 85% |
| Accuracy on multi-hop questions | Outperform vector RAG |
| Contradiction disclosure rate | 100% |

### Comparison benchmark

Run four approaches on the same corpus:

1. Vector RAG over raw chunks
2. Static LLM-generated wiki
3. Batch GraphRAG-style graph
4. pg_ripple + pg_trickle live compiled graph

The combined approach should do especially well on multi-hop questions,
aggregation, contradiction detection, change-awareness queries, and broad
sensemaking that still requires structured evidence.

---

## 15. Risks and guardrails

### Prompt injection

Raw documents may contain instructions aimed at the LLM. Compiler prompts must
frame source text as data, not instructions. Structured output must be validated
before it enters the trusted graph.

### Hallucinated facts

The compiler will occasionally extract wrong facts. Every fact must carry
evidence and confidence. Facts below the `load_triples_with_confidence()`
threshold go to a named review graph, not the trusted graph. `pg:confidence()`
surfaces below-threshold facts for human inspection. Accepted facts can be
promoted with an updated confidence score without full recompilation.

### Destructive recompilation

Deleting and rebuilding a whole source graph is risky in production. Production
mode should prefer staging, review, or diff-based updates.

### Sensitive data leakage

Summaries can leak sensitive content. Compiler profiles should support
redaction, graph-level access control, and output policies. pg-trickle outboxes
should publish only what a subscription is permitted to expose.

### Non-determinism

LLM output varies. Store model name, prompt version, input hash, output hash,
temperature, and run metadata. High-stakes domains should use deterministic
settings and require human review.

### Cost growth

Without fragment hashing and dependency tracking, this becomes an expensive
batch re-indexer. Incremental compilation is not an optional optimization — it
is central to the design. Cost grows proportionally to what actually changed, not
proportionally to corpus size.

### Trust confusion

An LLM-extracted assertion is not the same as a verified business fact. The
`_pg_ripple.confidence` side table keeps source assertions, compiler assertions,
human-reviewed assertions, and trust-propagated scores in separate rows keyed by
model label (`'llm-extract'`, `'human-review'`, `'prov-trust'`). Row-level
security mirrors the named-graph VP-table policies. `pg:confidence()` returns
the highest-confidence row; callers who need the per-model breakdown query the
side table directly.

---

## 16. Recommended next steps

1. Pick one demo corpus: a Markdown documentation set or a support-ticket
   export.
2. Define a small `pgc:` vocabulary for compiled knowledge artifacts: source,
   fragment, profile, run, artifact, dependency.
3. Draft the catalog schema for sources, fragments, profiles, runs, diagnostics,
   and artifacts.
4. Prototype `pg_ripple_compile` as a companion worker that calls existing
   pg_ripple SQL functions.
5. Add a deterministic mock compiler profile for CI.
6. Show one complete incremental update: source change, partial recompile,
   graph update, SHACL validation, pg-trickle outbox event.
7. Validate the uncertain knowledge pipeline: load compiled facts via
   `load_triples_with_confidence()`, run Datalog rules with `@weight`, verify
   derived confidence via `pg:confidence()`, confirm `shacl_score()` gates
   publication.
8. Run `pagerank_run()` with temporal decay over the compiled graph; enable
   PR-TRICKLE-01 incremental refresh and confirm scores update within seconds
   of a new compile.
9. Compare against raw vector RAG on multi-hop questions, aggregation,
   contradiction detection, and change-awareness queries.

The strongest demo shows what the article only hints at: a knowledge base that
behaves like a real build system. A source changes. Only the dependent knowledge
rebuilds. The system validates the result, explains what changed, and publishes
a semantic event. That is where pg_ripple and pg_trickle become more than an
implementation of the compiler analogy. They become the runtime for living
knowledge.
