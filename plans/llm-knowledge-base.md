# pg_ripple + pg_trickle as a Living LLM Knowledge Base

> **Date:** 2026-05-02
> **Status:** Strategy report, not a committed roadmap item
> **Source article:** [What Is Andrej Karpathy's LLM Knowledge Base Architecture? The Compiler Analogy Explained](https://www.mindstudio.ai/blog/karpathy-llm-knowledge-base-architecture-compiler-analogy)
> **Related local plans:** [GraphRAG synergy](graphrag.md), [pg-trickle relay integration](pg_trickle_relay_integration.md), [future directions](future-directions.md), [ROADMAP](../ROADMAP.md)

## 1. The short version

The article describes a useful idea: instead of asking an LLM to reread raw
documents every time someone asks a question, first turn those documents into a
cleaner, more structured knowledge base. The article calls this "compilation",
borrowing a term from software development. Programmers do not run source code
directly. They compile it into a form the machine can run quickly and reliably.
The same pattern can work for knowledge: take messy human documents, compile
them into structured facts, summaries, links, and questions, then query that
compiled knowledge later.

pg_ripple and pg_trickle are unusually well suited to this idea.

pg_ripple already knows how to store and query structured knowledge. It has RDF
storage, SPARQL queries, rules, validation, provenance, GraphRAG export, vector
search, natural-language-to-SPARQL, and `rag_context()`. In plain English: it
can hold facts, understand relationships, check quality, infer new facts, and
prepare useful context for an LLM.

pg_trickle adds the live-update part. It can bring in changes from event streams
and APIs, keep derived views fresh, and publish changes back out. In plain
English: it can keep the knowledge base moving as the world changes.

Together, they could build something more interesting than a static AI-written
wiki. They could build a **living knowledge compiler** inside PostgreSQL:

1. Raw documents, tickets, transcripts, events, and API payloads arrive.
2. An LLM-assisted compiler extracts facts, relationships, summaries, and likely
   questions.
3. pg_ripple stores the result as a governed knowledge graph.
4. SHACL and Datalog check the result for quality, contradictions, and derived
   knowledge.
5. pg_trickle keeps everything fresh as sources change.
6. PageRank and centrality measures (v0.88.0) rank compiled entities by importance,
   freshness, and source trust, keeping the knowledge index prioritised automatically.
7. Agents and applications query the compiled knowledge instead of searching raw
   text chunks.
8. Downstream systems receive semantic change events when the knowledge changes.

The big opportunity is not simply "RAG with RDF". It is **compiled operational
knowledge**: a knowledge base with sources, tests, diffs, review workflows,
confidence, provenance, and live change events.

## 2. What the article is saying

The article's main point is easy to summarize:

> Raw documents are not a good runtime format for AI systems.

Most documents are written for humans. They repeat themselves, hide important
assumptions, mix topics, and rely on context that may live elsewhere. Standard
RAG systems usually split those documents into chunks, embed the chunks, and
retrieve a handful of similar chunks when a user asks a question. That works for
some questions, but it has familiar problems:

- A chunk can lose the context that made it meaningful.
- Similar text is not the same as correct evidence.
- The LLM must reinterpret raw prose on every query.
- Multi-document reasoning is fragile.
- Answers can vary when chunking, embeddings, or retrieval settings change.

The article proposes a different pattern. Do more work before query time:

1. Feed raw documents into an LLM workflow.
2. Ask the workflow to extract facts, entities, relationships, summaries,
   questions, answers, gaps, and contradictions.
3. Store those extracted artifacts as the real knowledge base.
4. At query time, retrieve and reason over the compiled artifacts instead of raw
   prose.

The compiler analogy looks like this:

| Software compiler | LLM knowledge base |
| --- | --- |
| Source code | Raw documents, PDFs, web pages, transcripts, tickets |
| Compiler | LLM workflow that extracts and reorganizes meaning |
| Compiled binary | Structured knowledge base or wiki |
| Runtime | Querying the compiled knowledge |
| Compiler errors | Missing facts, contradictions, weak evidence, unresolved references |

The article also highlights the hard part: **incremental compilation**. When one
document changes, a production system should not reprocess the whole corpus. It
should update only the document, facts, summaries, links, and downstream entries
that actually depend on the change.

That is where pg_ripple and pg_trickle make the idea much stronger.

## 3. Why pg_ripple is a natural fit

pg_ripple already provides many of the pieces a compiled knowledge base needs.
Some of the terms are technical, so this table explains them in practical terms.

| Need | What pg_ripple already provides |
| --- | --- |
| Store structured facts | RDF triples in PostgreSQL |
| Query relationships | SPARQL, a standard graph query language |
| Ask in natural language | `sparql_from_nl()`, which turns English into SPARQL |
| Give an LLM grounded context | `rag_context()`, which prepares graph facts for a prompt |
| Keep source attribution | named graphs and PROV-O provenance |
| Record facts about facts | RDF-star annotations for confidence, evidence, timestamps |
| Score extracted facts by confidence | `pg_ripple.load_triples_with_confidence()`, `pg:confidence()` SPARQL function, probabilistic Datalog with `@weight` noisy-OR combination (v0.87.0) |
| Match entities by fuzzy similarity | `pg:fuzzy_match()`, `pg:token_set_ratio()`, GIN trigram index on compiled dictionary (v0.87.0) |
| Score data quality numerically | `pg_ripple.shacl_score()`, `sh:severityWeight` annotations, `pg_ripple.shacl_report_scored()` (v0.87.0) |
| Propagate source trust automatically | `pg:sourceTrust` predicate, PROV-O trust→confidence Datalog rules, noisy-OR multi-source attestation (v0.87.0) |
| Validate quality | SHACL rules, which act like data quality checks |
| Infer more knowledge | Datalog and OWL/RDFS rules |
| Resolve duplicate entities | `owl:sameAs` canonicalization and alignment helpers |
| Combine graph and vector retrieval | pgvector integration and graph-contextualized embeddings |
| Rank compiled knowledge automatically | PageRank and centrality measures (v0.88.0): importance, betweenness, closeness, temporal decay, topic-sensitive and confidence-weighted scoring |
| Export to GraphRAG tools | GraphRAG Parquet export and community detection |
| Stream changes | CDC subscriptions and JSON-LD event output |

In other words, pg_ripple already knows how to hold the compiled artifact. The
missing product layer is the step that takes raw human-readable sources and
turns them into that artifact in a repeatable, inspectable way.

## 4. Why pg_trickle matters

The article talks about incremental compilation as a challenge. pg_trickle gives
us a practical path to it.

pg_trickle can:

- Bring new source material into PostgreSQL from Kafka, NATS, HTTP, SQS, Redis
  Streams, and similar systems.
- Keep derived tables and views fresh using deltas, instead of recomputing
  everything from scratch.
- Publish outbound events when the compiled knowledge changes.
- Let inbound source events, graph updates, validation, and outbound messages
  share PostgreSQL's transaction model.

That changes the product shape. We do not have to think only in terms of a batch
indexer that periodically rebuilds a knowledge base. We can think in terms of a
live knowledge system:

```text
source changes -> compile only what changed -> validate -> update graph -> publish semantic events
```

That is the part that can make pg_ripple + pg_trickle more novel than the
architecture in the article. The article describes a compiled knowledge base.
The combined pg_ripple/pg_trickle design can become a **build system for living
knowledge**.

## 5. The product idea: pg_ripple_compile

A useful working name is `pg_ripple_compile`.

It could be a new companion service, or a module inside `pg_ripple_http`. The
important design principle is separation of responsibilities:

- pg_ripple owns the database truth: catalogs, graph writes, validation,
  provenance, rules, and query behavior.
- pg_ripple_compile owns the long-running AI work: document fetching, chunking,
  LLM calls, retries, rate limits, and structured output checks.
- pg_trickle owns event transport: inbound feeds, outboxes, retries, delivery,
  and downstream integration.

The product promise could be:

> Point pg_ripple at a stream or corpus of human-readable knowledge. It compiles
> that material into a governed, queryable, incrementally maintained knowledge
> graph that humans, agents, and applications can use at runtime.

This is deliberately more database-like than a workflow builder. The goal is not
to hide everything behind a visual pipeline. The goal is to make the compiled
knowledge durable, queryable, auditable, and operationally safe.

## 6. How the system would work

Here is the idea in one flow.

```text
                 SOURCE MATERIAL
  Docs, PDFs, Markdown, tickets, calls, events, APIs
        |              |                 |
        | direct load   | pg_trickle      | scheduled fetch
        |              | reverse relay   |
        v              v                 v
  +--------------------------------------------------+
  | Source registry and inbox tables                  |
  | - what source arrived                             |
  | - where it came from                              |
  | - whether it changed                              |
  | - which compiler profile should process it        |
  +-------------------------+------------------------+
                            |
                            v
                 COMPILATION
  +--------------------------------------------------+
  | pg_ripple_compile                                 |
  | - breaks large documents into stable sections     |
  | - asks an LLM to extract facts and relationships  |
  | - generates summaries and question-answer pairs   |
  | - links entities to known entities                |
  | - attaches confidence and evidence                |
  | - records warnings and contradictions             |
  +-------------------------+------------------------+
                            |
                            v
                 COMPILED KNOWLEDGE
  +--------------------------------------------------+
  | pg_ripple knowledge graph                         |
  | - atomic facts                                    |
  | - entity pages                                    |
  | - summaries                                       |
  | - generated questions and answers                 |
  | - evidence and provenance                         |
  | - dependency graph                                |
  | - confidence and trust scores                     |
  +-------------------------+------------------------+
                            |
        +-------------------+-------------------+
        |                                       |
        v                                       v
  RUNTIME QUERY                            CHANGE OUTPUT
  SPARQL, rag_context(),                   pg_trickle outbox,
  GraphRAG summaries,                      subscriptions,
  agent navigation graph                   downstream systems
```

The most important concept here is the **source registry**. The system needs to
remember what it compiled, when it compiled it, which prompt/profile it used,
what source hash it saw, what artifacts it generated, and what depends on what.
Without that memory, it becomes a batch re-indexer. With that memory, it becomes
an incremental compiler.

## 7. What gets stored

The compiled result should not be only Markdown pages or blobs of text. Those
can be generated for display, but the core artifact should be graph data.

### 7.1 Source records

For every source document or event, store the basics:

- source URI or external ID
- source type, such as document, ticket, transcript, web page, event stream
- source system, such as Confluence, GitHub, Zendesk, Salesforce, Kafka
- content hash
- last source update time
- graph IRI where compiled assertions live
- compile status
- timestamps for first seen, last seen, and last compiled

This makes the system answer simple operational questions:

- What sources are compiled?
- Which sources changed?
- Which sources failed compilation?
- Which graph came from which source?
- Which sources are stale?

### 7.2 Source fragments

Large documents should be split into stable sections. A section might be a
Markdown heading, a Confluence block, a PDF page, a transcript time range, or a
support-ticket message.

Fragment-level tracking matters because a 50-page document should not be fully
recompiled when one section changes. The compiler should reprocess the changed
section, then update only the facts, summaries, and entity pages that depend on
that section.

### 7.3 Compiler profiles

Different domains need different compilation instructions. A product-feedback
profile should extract customers, features, pain points, sentiment, urgency, and
evidence. A policy profile should extract owners, dates, scope, approvals, and
exceptions. A research profile should extract claims, methods, datasets,
metrics, and limitations.

A compiler profile should include:

- profile name and version
- prompt template
- expected output shape
- validation rules, including `sh:severityWeight` annotations (v0.87.0 SOFT-SHACL-01) so critical rules weigh more in the numeric quality score
- optional Datalog rules to run after compilation, with `@weight(FLOAT)` annotations (v0.87.0 PROB-DATALOG-01) for probabilistic confidence propagation through derived facts
- default extraction confidence assigned via `pg_ripple.load_triples_with_confidence()` at ingest time (v0.87.0 LOAD-CONF-01)
- preferred LLM model
- preferred embedding model
- maximum source size per fragment

Versioning matters. If we change the prompt, we changed the compiler. The system
should know which sources were compiled with which profile version.

### 7.4 Compiler runs and diagnostics

Every compile attempt should leave a trace:

- what source or fragment was compiled
- which profile and model were used
- whether the run succeeded
- how many tokens it used
- what output hash it produced
- what warnings or errors appeared

Diagnostics are the "compiler errors" from the article. Examples:

- unresolved entity
- weak evidence
- low confidence extraction
- source contradiction
- missing required field
- SHACL validation failure
- soft SHACL quality score below threshold (`pg_ripple.shacl_score()` ≤ threshold, v0.87.0 SOFT-SHACL-01)
- stale dependency
- LLM output did not match the expected schema

This is important for trust. A useful knowledge system should not silently turn
messy documents into questionable facts. It should show its work.

### 7.5 Compiled artifacts and dependencies

A compiled artifact can be a fact, summary, entity page, generated question,
generated answer, embedding, index entry, or diagnostic.

Each artifact should know what it depends on:

- source fragment
- source document
- entity
- compiler profile
- rule set
- another artifact

That dependency graph is what enables incremental compilation. If one support
ticket changes, the system can ask: which extracted facts came from this ticket?
Which customer profile used those facts? Which summary mentioned that customer?
Which generated answer now needs a refresh?

## 8. What the compiled knowledge looks like

The compiled knowledge should include several layers. Each layer supports a
different kind of question.

### 8.1 Atomic facts

Atomic facts are small, precise statements. For example:

- Customer A requested SSO.
- Policy B requires manager approval.
- Paper C uses method D.
- Incident E affected service F.

These facts should carry evidence and confidence. A fact without evidence should
not be treated the same as a fact with a clear source quote.

In pg_ripple, these facts become RDF triples. Extra information about a fact,
such as confidence or source quote, can be stored with RDF-star annotations.
v0.87.0's `pg_ripple.load_triples_with_confidence(data, confidence, format, graph_uri)`
lets the compiler assign a score in [0, 1] to every extracted fact at ingest
time; scores are stored in the shared `_pg_ripple.confidence` side table keyed
on the statement ID. The `pg:confidence(?s, ?p, ?o)` SPARQL function retrieves
the score inline, usable in `FILTER`, `BIND`, and `ORDER BY`. Datalog rules
with `@weight(FLOAT)` annotations propagate confidence through derived facts:
a rule chain with weights 0.9 × 0.8 × 0.7 produces a conclusion with confidence
~0.5. When multiple independent sources support the same conclusion, noisy-OR
combination (PROB-DATALOG-01) raises the joint confidence — three medium-trust
sources (0.6, 0.7, 0.6) together reach ~0.94. CONSTRUCT writeback rules with
`pg_ripple.cwb_confidence_propagation = 'inherit'` (CONF-CWB-01) automatically
carry source confidence into materialized derived graphs.

### 8.2 Entity pages

The article talks about a compiled wiki. In pg_ripple, an entity page should be
more than a text page. It should be an entity-centered graph bundle:

- name and aliases
- type or category
- canonical ID
- links to duplicate or equivalent entities
- short, medium, and long summaries
- important relationships
- source coverage
- known contradictions
- confidence score
- pagerank score (v0.88.0) and topic-specific scores (e.g., importance within healthcare vs. finance)
- centrality metrics: betweenness (bridge concepts), closeness (hub proximity) (v0.88.0)
- embedding vector
- related entities and communities

Applications can render this as a wiki page, JSON-LD document, API response, or
LLM context block. The stored form should stay graph-native.

### 8.3 Summaries at different levels

The compiler should generate summaries for:

- source fragments
- whole source documents
- entities
- topics
- communities of related entities
- the corpus as a whole

Short summaries help navigation. Medium summaries help answer common questions.
Long summaries help users inspect a topic. All summaries should link back to the
sources and artifacts they depend on.

### 8.4 Generated questions and answers

The compiler can generate likely questions that the source material answers.
Those pairs are useful for:

- testing whether the knowledge base still answers correctly after updates
- improving `sparql_from_nl()` with examples
- helping users discover what the knowledge base knows
- speeding up common query paths

For example, from a travel policy, it might generate:

- Question: What approvals are required for international travel?
- Answer: International travel requires manager and finance approval.
- Evidence: policy section 4.2
- Dependencies: approval facts extracted from that section

### 8.5 A knowledge index graph

Karpathy also discusses the idea of an index file that helps agents navigate a
knowledge base. pg_ripple can make that index a graph instead of a single text
file.

The index graph could contain:

- top-level topics
- related topics
- key entities
- source coverage
- representative questions
- community summaries
- freshness and confidence metadata

This gives agents a map. They can start with the index graph, choose relevant
topics, and then follow exact graph links to evidence.

With v0.88.0, the index graph gains a natural priority ordering computed from
the structure of the compiled knowledge itself. `pg_ripple.pagerank_run()` ranks
every entity by how heavily it is referenced by other important entities.
Temporal decay (PR-TEMPORAL-01) means recently compiled facts push more
importance than stale ones. Confidence-weighted edges (PR-CONF-01) mean facts
extracted from high-trust sources carry more weight than low-confidence
extractions. Topic-sensitive scoring (PR-TOPIC-01) lets a healthcare agent and
a finance agent each receive a relevance-ordered index without needing separate
graphs. Betweenness centrality (PR-CENTRALITY-01) surfaces the bridge concepts
that connect otherwise separate topic clusters — exactly the entities an index
graph must include so agents can reason across domains. SHACL-aware ranking
(PR-SHACL-01) automatically excludes or downranks entities that failed quality
checks, keeping low-quality compiled facts from inflating the index. The
pg-trickle incremental refresh (PR-TRICKLE-01) means every new compiled source
ripples its importance contribution outward in milliseconds, so the index stays
fresh without a nightly full recompute.

## 9. How users would query it

The query-time rule should be simple:

> Use compiled knowledge first. Use raw source text only when needed as evidence.

There are three main query paths.

### 9.1 Exact graph questions

Some questions are really relationship questions:

- Which customers asked for both SSO and audit logging?
- Which policies apply to contractors in Germany?
- Which features have more than five high-confidence pain points this month?
- Which documents contradict the current security policy?

These should be answered with SPARQL and Datalog over compiled facts, not by
asking an LLM to read chunks and do set logic in its head.

### 9.2 Broad sensemaking questions

Some questions are broad:

- What are the main themes in recent customer feedback?
- What changed in the compliance corpus this week?
- Which research areas are converging?
- What are the biggest unresolved product risks?

These should start with the PageRank-ordered index graph (v0.88.0), communities,
and summaries, then drill down into exact facts and evidence. Topic-sensitive
PageRank (PR-TOPIC-01) ensures the ordering reflects the agent's current domain,
not a global average. The `pg:topN_approx()` sketch function returns approximate
top-K entities sub-millisecond for interactive sensemaking queries.

### 9.3 Hybrid fallback

Vector search is still useful, but it should search over cleaner artifacts:

- summaries
- evidence spans
- entity descriptions
- generated questions
- generated answers

When vector search finds something relevant, it should point back to graph
artifacts and source evidence. The final LLM prompt should include structured
facts, provenance, confidence, and contradictions, not just raw text snippets.

v0.87.0 sharpens this retrieval path in two ways. First, `pg:fuzzy_match(a, b)`
and `pg:token_set_ratio(a, b)` (FUZZY-SPARQL-01) enable fuzzy entity-name
matching over the compiled dictionary backed by a GIN trigram index, so a query
for "SSO" retrieves "Single Sign-On" and "sso login" without requiring exact
string equality. Second, `pg:confPath(predicate, min_confidence)` (FUZZY-PATH-01)
traverses the compiled graph along confidence-gated paths, so a context-retrieval
query can follow relationship chains while excluding edges below a trust
threshold — preventing low-confidence extractions from contaminating multi-hop
reasoning paths fed to the LLM.

## 10. The most novel ideas

The article describes a compiled knowledge base. pg_ripple + pg_trickle can go
further.

### 10.1 A live knowledge compiler

The compiled output should not be a static wiki that gets refreshed overnight.
It should be a live graph that updates when source material changes.

This means:

- a changed source fragment marks dependent artifacts stale
- only affected artifacts rebuild
- SHACL checks the new compiled output
- Datalog derives follow-on facts
- subscriptions publish semantic changes
- downstream systems receive updates quickly

### 10.2 Knowledge CI/CD

Source changes can be treated like code changes.

Before new compiled knowledge is published, the system can run checks:

- Did the LLM output match the expected shape?
- Did required fields exist?
- Did SHACL validation pass?
- Did the new facts introduce contradictions?
- Did important generated questions still answer correctly?
- Which answers changed?

This creates a CI/CD pipeline for knowledge, not just software.

### 10.3 Semantic pull requests

When a document changes, users usually review text diffs. A compiled knowledge
system can show a more useful diff:

- facts added
- facts removed
- relationships changed
- entities merged or split
- summaries invalidated
- contradictions introduced or resolved
- generated answers affected

Domain experts could review the knowledge change instead of reading every
sentence of the source diff.

### 10.4 Source trust and uncertain knowledge

Not all sources are equally trustworthy, and not all extracted facts are equally
certain. v0.87.0 delivers a complete uncertain knowledge engine that makes
this concrete rather than aspirational.

**Attaching confidence at compile time.** `pg_ripple.load_triples_with_confidence()`
assigns a uniform extraction confidence when ingesting compiled facts. Facts
extracted by a high-accuracy model from a primary source get 0.95; facts
extracted from a secondary source with a less reliable prompt get 0.6. The
threshold for entering the trusted named graph vs. routing to the review graph
can be a single GUC setting.

**Source trust propagation.** Registering a `pg:sourceTrust` value on a source
graph (e.g., 0.9 for an official policy document, 0.4 for a web scrape) is all
that is needed. With `pg_ripple.prov_confidence = on` (PROV-CONF-01), Datalog
rules read the PROV-O source metadata and automatically populate
`_pg_ripple.confidence` for every triple originating from that source. When a
fact is attested by multiple independent sources, noisy-OR combination raises
the joint confidence automatically — no manual aggregation required.

**Confidence propagation through inference.** Datalog rules annotated with
`@weight(FLOAT)` (PROB-DATALOG-01) multiply body-atom confidences by the rule
weight. A chain of inferences produces a conclusion whose confidence reflects
the entire evidence chain. The compiled knowledge base therefore distinguishes
between "we know this directly" (confidence 0.95) and "we derived this through
three intermediate rules from a medium-trust source" (confidence 0.42).

**Numeric quality gates.** `pg_ripple.shacl_score(graph_iri)` (SOFT-SHACL-01)
returns a single floating-point quality score in [0, 1] for a compiled graph.
Shapes can declare `sh:severityWeight` to make critical validation rules count
more than minor ones. This turns the binary SHACL pass/fail into a graded
quality signal: route graphs below 0.75 to the review queue; publish graphs
above 0.9 immediately. `pg_ripple.shacl_report_scored()` gives a per-rule
breakdown so the review UI can show exactly which shapes reduced the score.

**Confidence-gated graph traversal.** `pg:confPath(predicate, min_confidence)`
(FUZZY-PATH-01) restricts property-path traversal to edges meeting a minimum
confidence. An agent following `pg:confPath(schema:relatedTo, 0.7)` never
follows a weakly-supported relationship into a distant and dubious conclusion.

**Exporting confidence for downstream consumers.** `export_turtle_with_confidence()`
(CONF-EXPORT-01) emits every fact with its confidence as an RDF* annotation;
`pg_ripple.export_confidence = on` adds `@annotation` blocks to JSON-LD output.
Downstream systems receiving compiled knowledge can see not just the fact, but
how much to trust it.

That lets the system say things like:

> The strongest supported answer is A with confidence 0.82. Source B disagrees,
> but it is older and has lower trust.

That is much better than pretending every extracted fact is equally true.

v0.88.0's confidence-weighted PageRank (PR-CONF-01) closes the loop further:
the importance ranking of every entity in the knowledge base now reflects not
just how many things point to it, but how *trustworthy* those pointers are.
A policy backed by three high-confidence extractions outranks one backed by
five uncertain ones, automatically, without manual curation.

### 10.5 An agent memory bus

pg_trickle can publish semantic events for agents:

- `entity.updated`
- `policy.changed`
- `policy.contradiction.detected`
- `summary.invalidated`
- `source.needs_review`
- `answer_package.changed`

Agents would not need to poll a vector store. They could subscribe to meaningful
knowledge changes.

### 10.6 Human correction loops

The bidirectional integration work from v0.77/v0.78 points to a powerful loop:

1. The compiler extracts a fact from a transcript.
2. A product manager corrects it in a review UI.
3. The UI sends the correction back through pg_trickle.
4. pg_ripple stores the correction in a higher-priority human-review graph.
5. Conflict rules prefer the human-reviewed fact over the lower-confidence LLM
   extraction.
6. The corrected knowledge is published downstream.

This keeps the LLM as an assistant, not the source of truth.

### 10.7 Knowledge packages

Once a corpus is compiled, it could be packaged:

- named RDF graphs
- SHACL shapes
- Datalog rules
- compiler profile
- summaries and generated questions
- embeddings metadata
- provenance manifest
- evaluation set

That could become a distribution format for domain knowledge. Install a package
into pg_ripple, validate it, and query it immediately.

### 10.8 Federated compiled knowledge

Some organizations cannot centralize all raw documents. pg_ripple's federation
support makes another design possible:

- each department compiles its own sources locally
- only approved compiled facts or summaries are shared
- sensitive raw text stays in the source domain
- cross-domain queries use federation or replicated safe summaries

This could be valuable for regulated or privacy-sensitive environments.

v0.88.0's federation blend mode (PR-FED-01) makes this more powerful at query
time: `pg_ripple.pagerank_run()` can pull edge triples from remote SERVICE
endpoints into a temporary local graph, compute a single global importance
ranking across all departments, then discard the raw remote triples. Each
department keeps its source documents private while contributing to a shared
knowledge ranking.

### 10.9 Importance-ranked compiled knowledge (v0.88.0)

One gap in the compiled-knowledge architecture is deciding which entities and
facts matter most when an agent navigates the graph or when a sensemaking query
needs to filter 50,000 compiled facts down to the 20 most relevant ones.
v0.88.0 fills this gap with a suite of graph analytics features that work
directly on the compiled VP storage.

**Automatic index ordering.** `pg_ripple.pagerank_run()` computes an importance
score for every entity based on how heavily it is referenced by other important
entities. The knowledge index graph can expose the top-N entities per topic
using `pg:topN()`, without manual curation or heuristics.

**Freshness-aware ranking.** Temporal decay (PR-TEMPORAL-01) weights edges by
the age of the compiled source. A policy extracted from a document updated last
week pushes more importance than one extracted from a document last updated
three years ago. The ranking naturally reflects recency without requiring
explicit staleness rules.

**Trust-propagating ranking.** Confidence-weighted edges (PR-CONF-01) mean the
v0.87.0 uncertain-knowledge engine and the v0.88.0 ranking engine share the
same confidence side-table. A high-trust source citation carries more rank mass
than a low-confidence LLM extraction. Source trust propagates through the
entire entity graph automatically.

**Per-domain ranking.** Topic-sensitive scoring (PR-TOPIC-01) stores independent
ranking runs per topic label — matching the compiler profile concept exactly.
An agent working on a healthcare question receives a healthcare-ordered index;
the same entity may rank differently for finance. One `pagerank_run()` per
topic at compilation time; zero extra query cost at read time.

**Bridge concept detection.** Betweenness centrality (PR-CENTRALITY-01)
identifies entities that connect otherwise separate topic clusters. These are
the concepts an index graph *must* include so agents can reason across domains.
They would be invisible to a pure PageRank score but surface immediately as
betweenness outliers.

**Quality-gated ranking.** SHACL-aware ranking (PR-SHACL-01) lets SHACL shapes
declare `sh:excludeFromRanking true` for node types that should not influence
the importance graph — for example, diagnostic artifacts, stub entities, or
nodes flagged as contradictory. Quality failures remove bad nodes from the
ranking graph automatically, not just from query results.

**Live incremental ranking.** The pg-trickle incremental refresh (PR-TRICKLE-01)
propagates importance changes via bounded K-hop updates whenever a new compiled
source is inserted or retracted. The index graph stays current without a nightly
full recompute. The `stale`/`stale_since` columns on `pagerank_scores` let
applications distinguish exact from approximate scores and decide whether to
wait for the next scheduled full recompute.

**Transparent rankings.** `pg_ripple.explain_pagerank(entity_iri)` returns the
top-K contributor chain, showing exactly which other entities pushed the most
importance to a given node. This supports the semantic pull-request review flow
(section 10.3): reviewers can see not just what facts changed, but why the
importance of related entities shifted.

### 10.10 Graded confidence throughout the compiled knowledge base (v0.87.0)

The compiled-knowledge architecture depends on the system knowing *how much to
trust* every fact it holds. v0.87.0 delivers a complete uncertain knowledge
engine that makes this concrete rather than aspirational, using a unified
`_pg_ripple.confidence` side table and a set of composing APIs that work at
every stage of the compilation pipeline.

**At ingest.** `pg_ripple.load_triples_with_confidence(data, confidence, format,
graph_uri)` lets the `pg_ripple_compile` worker assign a score in [0, 1] to
every extracted fact at the moment it is loaded. The score can vary by compiler
profile version, extraction model, source tier, or per-field confidence
returned in structured LLM output. No schema change is needed on the VP tables.

**During inference.** Datalog rules that derive new facts from compiled facts
can carry `@weight(FLOAT)` annotations (PROB-DATALOG-01). The semi-naive
evaluator multiplies body-atom confidences by the rule weight (conjunction) and
applies noisy-OR across independent derivation paths. A fact supported by three
independent rule paths with weights 0.8, 0.7, and 0.6 reaches a combined
confidence of ~0.97. A long inference chain of medium-trust sources
(0.9 × 0.8 × 0.7) produces a conclusion with confidence ~0.5, immediately
visible to callers via `pg:confidence()`.

**After compilation.** `pg_ripple.shacl_score(graph_iri)` (SOFT-SHACL-01)
returns a single floating-point quality score for the entire compiled graph.
The `pg_ripple_compile` worker uses this as a publish gate: score ≥ 0.9 →
trusted graph; score < 0.75 → review queue with per-shape breakdown from
`shacl_report_scored()`. SHACL shapes declare `sh:severityWeight` so critical
rules count more than cosmetic ones.

**At query time.** `pg:confidence(?s, ?p, ?o)` retrieves a fact's score inline
in any SPARQL SELECT, FILTER, or ORDER BY. `FILTER(pg:confidence(?s, ?p, ?o) > 0.7)`
restricts a context-retrieval query to well-supported facts. `pg:confPath(predicate,
min_confidence)` (FUZZY-PATH-01) traverses the compiled graph along
confidence-gated paths, preventing low-confidence extractions from contaminating
multi-hop reasoning chains fed to the LLM.

**For entity resolution.** `pg:fuzzy_match(a, b)` and `pg:token_set_ratio(a, b)`
(FUZZY-SPARQL-01), backed by a GIN trigram index on the compiled dictionary,
let the compiler find that "SSO", "Single Sign-On", and "sso login" should map
to the same entity. Fuzzy similarity scores feed directly into confidence
side-table rows for entity-alignment triples.

**For source trust.** Registering `pg:sourceTrust 0.9` on a source named graph
and enabling `pg_ripple.prov_confidence = on` (PROV-CONF-01) is enough for
Datalog rules to automatically propagate source trust to all triples originating
from that source. No per-triple annotation by the compiler is needed.

**For export.** `export_turtle_with_confidence()` (CONF-EXPORT-01) emits every
fact with its confidence as an RDF* annotation; `pg_ripple.export_confidence = on`
adds `@annotation` blocks to JSON-LD output. Downstream consumers of compiled
knowledge see not just the fact, but how much to trust it.

**Security.** Row-level security on `_pg_ripple.confidence` (CONF-RLS-01)
mirrors the VP-table named-graph RLS policies, so multi-tenant compilations
cannot read each other's confidence scores.

## 11. Example use cases

### 11.1 Enterprise documentation

Sources:

- Confluence pages
- GitHub Markdown docs
- policy PDFs
- support knowledge-base articles
- decision logs

What gets compiled:

- policies
- owners
- effective dates
- product areas
- required approvals
- exceptions
- related systems
- contradictions between documents

Why it matters:

Policy questions often require exact scope, dates, exceptions, and ownership.
Those are hard for raw chunk retrieval and much better as structured facts.
PageRank (v0.88.0) with temporal decay surfaces the most recently updated and
most heavily cross-referenced policies at the top of the index, so agents and
users find the authoritative current policy before older superseded versions.

### 11.2 Product intelligence

Sources:

- support tickets
- call transcripts
- CRM notes
- telemetry events
- feedback forms

What gets compiled:

- customers
- accounts
- product areas
- features
- pain points
- sentiment
- urgency
- evidence quotes
- duplicate requests

Why it matters:

The result is not just a pile of feedback summaries. It is a live product graph:
which accounts asked for what, how confident we are, what evidence supports it,
how the trend changed, and which downstream systems should be notified.
Topic-sensitive PageRank (v0.88.0) ranks features by how heavily they are
requested across accounts within a given product area, surfacing the top pain
points automatically as the feedback graph grows.

### 11.3 Research library

Sources:

- papers
- lab notes
- benchmark reports
- citations
- experiment metadata

What gets compiled:

- claims
- methods
- datasets
- metrics
- baselines
- limitations
- conflicting results
- open questions

Why it matters:

A new paper can strengthen, weaken, or contradict existing claims. The system
can show what changed in the research map instead of only summarizing the new
paper in isolation. Eigenvector centrality (v0.88.0) surfaces the most
fundamentally important claims — those backed by strong chains of mutually
corroborating evidence — distinguishing them from popular but weakly-supported
assertions.

### 11.4 Operations memory

Sources:

- alerts
- incident reports
- deployment events
- summarized logs
- runbook changes

What gets compiled:

- symptoms
- affected services
- owners
- deploys
- probable causes
- remediation steps
- runbook links

Why it matters:

The knowledge base can answer questions like "what changed before this alert
pattern?" with evidence from previous incidents and deployments.

## 12. Suggested first version

The first version should be intentionally small. It should prove the idea
without trying to become a full workflow platform.

### 12.1 MVP features

1. Source registry for documents and fragments.
2. Compiler profiles with prompt, version, expected output, and validation
   rules.
3. Compile queue and run history.
4. Diagnostics table for warnings and errors.
5. `pg_ripple_compile` worker with OpenAI-compatible endpoint support.
6. Mock compiler mode for deterministic tests.
7. Basic compiled artifacts: facts, summaries, generated QA pairs, entity pages,
   diagnostics.
8. Statement-level provenance and confidence via
   `pg_ripple.load_triples_with_confidence()` (v0.87.0 LOAD-CONF-01); the
   compiler assigns a score in [0, 1] to every extracted fact at load time.
9. SHACL validation before publishing compiled facts; `pg_ripple.shacl_score()`
   (v0.87.0 SOFT-SHACL-01) used as the numeric quality gate — graphs below
   threshold route to the diagnostic review queue instead of the trusted graph.
10. Named graph write modes: append, replace, review, and later diff.
11. Simple index graph for topics and entity navigation, with top-N entries
    ranked by PageRank (v0.88.0) using `pg:topN()` and temporal decay.
12. pg_trickle inbox attachment for source events.
13. pg_trickle outbox publication for diagnostics and artifact changes.

### 12.2 Things to avoid in the first version

- No custom UI yet.
- No general workflow DAG editor.
- No large connector catalog.
- No automatic trust in LLM-extracted facts.
- No full-corpus re-summarization on every change.
- No hidden destructive deletes during recompilation.

The demo should show one strong flow: a source changes, only the affected part is
recompiled, the graph is updated, validation runs, and a meaningful change event
is published.

## 13. Later phases

### Phase 1: Foundation

- Add source/compiler catalogs.
- Add SQL APIs for registering profiles and enqueueing compilation.
- Build the companion worker.
- Validate structured LLM output.
- Add mock mode for CI.
- Compile a small Markdown or ticket corpus end to end.

### Phase 2: Incremental compilation

- Add stable document fragmenting.
- Track artifact dependencies.
- Add stale-artifact handling.
- Add diff mode for compiled triples.
- Use pg_trickle stream tables for queues and outboxes.
- Add `explain_compilation()` so users can see what depends on what.

### Phase 3: Graph-native compiled wiki

- Generate entity pages.
- Generate topic index graphs.
- Rank entity pages and index graph topics by PageRank, topic-sensitive scoring,
  and centrality measures (v0.88.0); expose via `pg:pagerank(?entity, ?topic)`
  and `pg:centrality(?entity, 'betweenness')` in SPARQL.
- Generate multi-level summaries.
- Generate question-answer pairs.
- Integrate compiled artifacts into `rag_context()`.
- Maintain community summaries from the compiled graph.

### Phase 4: Review and trust

- Add review graphs for human approval.
- Add conflict policies for source vs LLM vs human assertions.
- Add source trust scores via `pg:sourceTrust` predicate and `pg_ripple.prov_confidence = on`
  (v0.87.0 PROV-CONF-01); source trust propagates automatically to all triples
  from that source via built-in Datalog rules.
- Use probabilistic Datalog with `@weight` annotations (v0.87.0 PROB-DATALOG-01)
  for confidence propagation through derived facts; noisy-OR combination raises
  joint confidence when multiple sources agree.
- Enable `pg_ripple.shacl_score()` (v0.87.0 SOFT-SHACL-01) as a numeric
  publish gate per compiled graph; expose `shacl_report_scored()` in the
  review UI to show which shapes reduced the quality score.
- Add semantic diffs for reviewers.
- Enable confidence-weighted PageRank (v0.88.0, PR-CONF-01) so entity importance
  reflects both graph structure and source trust simultaneously.
- Use `explain_pagerank()` (v0.88.0, PR-EXPLAIN-SCORE-01) in semantic-diff
  reviews to show why an entity's importance changed after a source update.

### Phase 5: Agent ecosystem

- Publish semantic change events for agents.
- Invalidate cached answer packages when knowledge changes.
- Export and import knowledge packages.
- Support federated compiled knowledge across teams.
- Benchmark against vector RAG and static GraphRAG.

## 14. How to measure success

This should be evaluated like a compiler and a knowledge system, not only like a
chatbot.

### Compile-time measures

- documents processed per hour
- percentage of unchanged fragments skipped
- LLM cost per document
- structured-output failure rate
- validation failure rate
- mean `pg:confidence()` of extracted facts per compile run
- low-confidence extraction rate (facts with score below threshold)
- `pg_ripple.shacl_score()` per compiled graph (target > 0.9 for trusted graph)
- unresolved entity rate
- contradiction rate
- artifacts generated per document
- facts with evidence attached

### Incremental-update measures

- time from source change to updated graph
- time from source change to refreshed summary
- time from source change to outbound event
- stale artifact count
- unnecessary recompilation rate
- number of full refreshes avoided

### Query-time measures

- ranking relevance on sensemaking queries (PageRank top-10 precision vs. human judgement)
- index freshness after source change: time for PageRank scores to stabilise (target: < 5 s via PR-TRICKLE-01 K-hop propagation)
- facts retrieved per answer
- raw snippets needed per answer
- evidence coverage
- contradiction disclosure rate
- SPARQL generation repair rate
- accuracy on generated and human-written QA sets

### Comparison benchmarks

Compare four approaches on the same corpus:

1. vector RAG over raw chunks
2. static LLM-generated wiki
3. batch GraphRAG-style graph
4. pg_ripple + pg_trickle live compiled graph

The combined pg_ripple/pg_trickle approach should do especially well on:

- multi-hop questions
- aggregation questions
- contradiction detection
- questions about what changed
- broad sensemaking questions that still need evidence

## 15. Risks and guardrails

### 15.1 Prompt injection

Raw documents may contain instructions aimed at the LLM. Compiler prompts must
make it clear that source text is data, not instructions. The output must be
validated before it becomes trusted knowledge.

### 15.2 Hallucinated facts

The compiler will sometimes extract weak or wrong facts. Every fact should carry
evidence and confidence. Use `pg_ripple.load_triples_with_confidence()` (v0.87.0)
to assign an extraction confidence at load time. Set a threshold GUC: facts
below the threshold go to a named review graph, not the trusted graph. The
`pg:confidence()` SPARQL function lets the review UI surface all below-threshold
facts for human inspection. Low-confidence facts that survive human review
can be promoted with an updated confidence score without recompilation.

### 15.3 Destructive recompilation

Deleting and rebuilding a whole source graph is simple but risky. Production
mode should prefer staging, review, or diff-based updates.

### 15.4 Sensitive data leakage

Summaries can leak sensitive content. Compiler profiles should support
redaction, graph-level access control, and output policies. pg_trickle outboxes
should publish only what a subscription is allowed to expose.

### 15.5 Non-determinism

LLM output can vary. Store model name, prompt version, input hash, output hash,
temperature, and run metadata. Important domains should use deterministic
settings and human review.

### 15.6 Cost growth

Without hashing, fragment tracking, and dependency tracking, this could become
an expensive batch re-indexer. Incremental compilation is not an optional
optimization. It is central to the product.

### 15.7 Trust confusion

An LLM-extracted assertion is not the same as a verified business fact. The
system should keep source assertions, compiler assertions, human-reviewed
assertions, and resolved projections clearly separated. v0.87.0's
`_pg_ripple.confidence` side table (keyed on statement ID and model name)
provides this separation at the storage level: different assertion sources
register separate rows with their own model label (`'llm-extract'`,
`'human-review'`, `'prov-trust'`). Row-level security (CONF-RLS-01) mirrors
the named-graph VP-table policies, so tenant boundaries apply to confidence
scores just as they do to the facts themselves. `pg:confidence(?s, ?p, ?o)`
returns the highest-confidence row across all models; callers who need the
per-model breakdown can query the side table directly.

## 16. Recommended next steps

1. Pick one demo corpus, preferably Markdown docs or support tickets.
2. Define a small `pgc:` vocabulary for compiled knowledge artifacts.
3. Draft the catalog schema for sources, fragments, profiles, runs,
   diagnostics, artifacts, and dependencies.
4. Prototype `pg_ripple_compile` as a companion worker that calls existing
   pg_ripple SQL functions.
5. Add a deterministic mock compiler profile for CI.
6. Show one incremental update from start to finish: source change, partial
   recompile, graph update, validation, and pg_trickle outbox event.
7. Test the v0.87.0 uncertain knowledge pipeline end to end: load a batch of
   compiled facts via `load_triples_with_confidence()`, run a Datalog rule set
   with `@weight` annotations, query derived confidence via `pg:confidence()`,
   verify that `shacl_score()` gates publication, and confirm that
   `pg:confPath(predicate, 0.7)` excludes low-confidence edges from a
   multi-hop context-retrieval query.
8. Run `pg_ripple.pagerank_run()` with temporal decay over the compiled graph
   and verify that the top-10 index entities match domain-expert expectations;
   enable PR-TRICKLE-01 incremental refresh and confirm scores update within
   seconds of a new source compile.
9. Compare the result against raw vector RAG on questions that require exact
   relationships, contradictions, or change awareness.

The strongest demo would show what the article only hints at: a knowledge base
that behaves like a real build system. A source changes. Only the dependent
knowledge rebuilds. The system validates the result, explains what changed, and
publishes a semantic event. That is where pg_ripple and pg_trickle become more
than an implementation of the compiler analogy. They become the runtime for
living knowledge.
