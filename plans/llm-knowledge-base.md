# pg_ripple + pg_trickle as a Compiled LLM Knowledge Base

> **Date:** 2026-05-02
> **Status:** Strategy report, not a committed roadmap item
> **Source article:** [What Is Andrej Karpathy's LLM Knowledge Base Architecture? The Compiler Analogy Explained](https://www.mindstudio.ai/blog/karpathy-llm-knowledge-base-architecture-compiler-analogy)
> **Related local plans:** [GraphRAG synergy](graphrag.md), [pg-trickle relay integration](pg_trickle_relay_integration.md), [future directions](future-directions.md), [ROADMAP](../ROADMAP.md)

## 1. Executive summary

The article frames a useful architecture for LLM knowledge bases: treat raw
documents as source code, run an LLM-powered compilation step before query time,
store the compiled artifact as structured knowledge, and let the query-time LLM
reason over that artifact instead of re-reading raw prose. The strongest idea is
not the use of an LLM by itself. It is the shift of expensive semantic work from
query time to ingest/update time.

pg_ripple already has most of the runtime that this architecture wants: RDF
storage, named graphs, SPARQL, Datalog, SHACL, provenance, GraphRAG export,
hybrid vector retrieval, natural-language-to-SPARQL, and `rag_context()`.
pg_trickle adds the missing production ingredient: live transport and
incremental maintenance. Together they can build something stronger than a static
LLM-generated wiki: a transactionally consistent, event-driven knowledge
compiler whose compiled output is not a pile of text pages, but a governed RDF
graph with validation, inference, provenance, uncertainty, CDC, and downstream
delivery.

The recommended direction is to position the combined system as a **PostgreSQL
native knowledge compiler**:

1. **Source layer:** raw documents, event streams, API payloads, tickets,
   transcripts, and existing RDF/JSON-LD sources land through pg_trickle inboxes
   or direct pg_ripple loaders.
2. **Compilation layer:** a new `pg_ripple.compile_document()` /
   `pg_ripple_compile` workflow runs LLM extraction, summarization, entity
   linking, contradiction checks, and artifact generation.
3. **Compiled store:** pg_ripple stores atomic facts, summaries, QA pairs,
   entity pages, dependency edges, confidence, and provenance as named RDF
   graphs plus companion catalog rows.
4. **Incremental build layer:** pg_trickle propagates changed source rows through
   compilation queues, CONSTRUCT writeback views, Z-set deltas, outboxes, and
   downstream notifications.
5. **Runtime query layer:** users query compiled knowledge through SPARQL,
   `sparql_from_nl()`, `rag_context()`, GraphRAG-style community summaries, and
   exact graph traversal.

This would make pg_ripple less like a vector database with graph features and
more like a database-backed compiler/runtime for organizational knowledge.

## 2. What the article argues

The article's central analogy is:

| Compiler world | LLM knowledge-base world |
| --- | --- |
| Source files | Raw documents, PDFs, web pages, transcripts, tickets |
| Compiler | LLM pipeline that extracts and restructures meaning |
| Compiled binary | Structured knowledge base or wiki |
| Runtime execution | Querying the compiled artifact |
| Compiler errors | Gaps, contradictions, low-quality inputs, unresolved references |

Its practical claims are:

- Standard RAG keeps raw chunks as the main artifact and asks the LLM to
  reinterpret them on every query.
- A compiled knowledge base pays an upfront cost to extract facts,
  relationships, summaries, and likely QA pairs.
- Query-time reasoning becomes easier because the model reads dense structured
  artifacts instead of arbitrary source prose.
- Incremental compilation is the hard production problem: when one source
  document changes, the system should update only affected artifacts and
  downstream entries.
- Compile-time prompt design matters at least as much as query-time prompt
  design because it defines the structure and quality of the knowledge base.

That is a good architecture for a static or periodically refreshed knowledge
base. pg_ripple plus pg_trickle can take it further: use database transactions,
typed graph semantics, differential maintenance, and CDC to make the compiled
knowledge base continuously updatable and operationally reliable.

## 3. Where pg_ripple already matches the model

pg_ripple already implements several pieces that would otherwise have to be
invented for this architecture.

| Need | Existing pg_ripple capability |
| --- | --- |
| Structured knowledge store | RDF triples in vertically partitioned VP tables |
| Stable identifiers | Dictionary-encoded IRIs, blank nodes, literals, named graphs |
| Query runtime | Native SPARQL 1.1 compiled to SQL |
| Semantic validation | SHACL Core, SHACL-SPARQL, async validation |
| Derived knowledge | Datalog, RDFS/OWL RL/EL/QL, CONSTRUCT writeback rules |
| Source attribution | Named graphs, PROV-O, RDF-star annotations |
| Entity resolution | `owl:sameAs` canonicalization and alignment helpers |
| LLM query interface | `sparql_from_nl()`, `repair_sparql()`, LLM endpoint config |
| RAG query packaging | `rag_context()` |
| Hybrid retrieval | pgvector integration, graph-contextualized embeddings |
| GraphRAG exchange | GraphRAG Parquet export and community detection |
| Live change events | CDC subscriptions and JSON-LD event serializer |

The main missing piece is an explicit **source-to-compiled-knowledge pipeline**.
Today pg_ripple can load RDF, JSON, JSON-LD, N-Triples, Turtle, and events, and
it can answer questions over the graph. The article points at the feature that
sits between those halves: use LLMs to compile raw human documents into graph
artifacts before query time.

## 4. Where pg_trickle changes the design

The article treats incremental compilation as a hard problem. pg_trickle gives
us a strong answer because it is already designed around deltas, stream tables,
outboxes, inboxes, and incremental view maintenance.

pg_trickle contributes four important properties:

1. **Transport:** reverse relay can bring documents, events, and source payloads
   from Kafka, NATS, HTTP, SQS, Redis Streams, and other sources into PostgreSQL
   inbox tables.
2. **Change propagation:** Z-set deltas and stream tables can maintain derived
   views without full refresh, which fits the compiler analogy better than a
   batch re-indexer.
3. **Transactional coupling:** inbound event, source registry update, compiled
   graph update, SHACL validation, and outbox publication can share PostgreSQL's
   transaction semantics.
4. **Outbound delivery:** forward relay can publish compiler diagnostics,
   changed knowledge artifacts, refreshed entity pages, and answer-package
   invalidations to downstream systems.

This is the main way to go beyond the article: do not only build a compiled
wiki. Build a **live build system for knowledge**. The source files are
documents and events. The compiled outputs are triples, summaries, entity
profiles, QA pairs, embeddings, and graph indexes. The build graph is stored in
PostgreSQL. pg_trickle moves changes through the build graph.

## 5. Proposed product concept

### 5.1 Working name: pg_ripple_compile

`pg_ripple_compile` can be either a module inside `pg_ripple_http` or a sibling
companion service. The database extension should own catalogs, functions,
validation, and graph writes. The service should own long-running LLM calls,
document fetching, retries, rate limiting, and connector-specific behavior.

The product promise:

> Point pg_ripple at a stream or corpus of human-readable knowledge. It compiles
> the corpus into a governed, queryable, incrementally maintained RDF knowledge
> graph that agents and applications can use at runtime.

This is deliberately more database-like than workflow-builder-like. The value is
not that pg_ripple calls an LLM. The value is that the compiled artifact becomes
part of a strongly governed PostgreSQL knowledge system.

### 5.2 Target users

- **Enterprise documentation teams** that want support, sales, product, and
  engineering knowledge unified without forcing every source into one wiki.
- **Research teams** that ingest papers, notes, lab results, and experiment logs
  and need provenance-preserving synthesis.
- **Customer intelligence teams** that compile interviews, tickets, CRM notes,
  telemetry, and call transcripts into product and account insight graphs.
- **AI platform teams** that need auditable context layers for agents and want
  less query-time LLM work.
- **Integration teams** already using event streams, where knowledge changes
  should be published as structured events.

## 6. Reference architecture

```text
                 SOURCE LAYER
  Documents, PDFs, Markdown, tickets, calls, events, APIs
        |              |                 |
        | direct load   | pg_trickle      | scheduled fetch
        |              | reverse relay   |
        v              v                 v
  +--------------------------------------------------+
  | PostgreSQL source registry and inbox tables       |
  | _pg_ripple.source_documents                       |
  | _pg_ripple.source_fragments                       |
  | pg_trickle inboxes                                |
  +-------------------------+------------------------+
                            |
                            v
                 COMPILATION LAYER
  +--------------------------------------------------+
  | pg_ripple_compile worker                          |
  | - chunk / normalize / classify source             |
  | - LLM fact extraction                             |
  | - summary and QA generation                       |
  | - entity linking and sameAs suggestions           |
  | - confidence and source-trust scoring             |
  | - SHACL and Datalog diagnostics                   |
  +-------------------------+------------------------+
                            |
                            v
                  COMPILED STORE
  +--------------------------------------------------+
  | pg_ripple RDF graph                               |
  | - atomic facts                                    |
  | - entity pages                                    |
  | - summary artifacts                               |
  | - dependency graph                                |
  | - contradiction records                           |
  | - embeddings and graph communities                |
  +-------------------------+------------------------+
                            |
        +-------------------+-------------------+
        |                                       |
        v                                       v
  RUNTIME QUERY                            CHANGE OUTPUT
  SPARQL, rag_context(),                   pg_trickle outbox,
  GraphRAG summaries,                      subscriptions,
  agent navigation graph                   downstream agents
```

## 7. Core data model

The RDF graph should remain the canonical compiled artifact. Catalog tables are
still useful for operational state, scheduling, and dependency tracking.

### 7.1 Source documents

```sql
CREATE TABLE _pg_ripple.source_documents (
    id                  BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source_uri          TEXT NOT NULL UNIQUE,
    source_kind         TEXT NOT NULL, -- document, web_page, ticket, event_stream, transcript
    source_system       TEXT,
    content_hash        TEXT NOT NULL,
    etag                TEXT,
    last_source_update  TIMESTAMPTZ,
    last_seen_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    compiled_graph_iri  TEXT NOT NULL,
    status              TEXT NOT NULL DEFAULT 'pending',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

The named graph is the source boundary. A document with URI
`https://example.com/policies/travel` could compile into
`urn:pg-ripple:source:https%3A%2F%2Fexample.com%2Fpolicies%2Ftravel` or a
deployment-specific graph IRI.

### 7.2 Source fragments

Large documents should be broken into deterministic fragments. The fragment is
the compilation unit, not always the whole document.

```sql
CREATE TABLE _pg_ripple.source_fragments (
    id                  BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    document_id         BIGINT NOT NULL REFERENCES _pg_ripple.source_documents(id),
    fragment_key        TEXT NOT NULL,
    byte_start          BIGINT,
    byte_end            BIGINT,
    token_count         INTEGER,
    content_hash        TEXT NOT NULL,
    compiled_at         TIMESTAMPTZ,
    status              TEXT NOT NULL DEFAULT 'pending',
    UNIQUE (document_id, fragment_key)
);
```

Fragment keys should be stable across edits when possible: headings, page
numbers, section IDs, message IDs, transcript timestamp ranges, or structural
anchors from HTML/Markdown parsers.

### 7.3 Compiler profiles

Compilation should be configurable by domain. A legal corpus, medical corpus,
product-feedback corpus, and source-code-design corpus should not share one
prompt.

```sql
CREATE TABLE _pg_ripple.compiler_profiles (
    name                TEXT PRIMARY KEY,
    version             INTEGER NOT NULL DEFAULT 1,
    output_schema       JSONB NOT NULL,
    prompt_template     TEXT NOT NULL,
    shacl_shapes_graph  TEXT,
    datalog_ruleset     TEXT,
    llm_model           TEXT,
    embedding_model     TEXT,
    max_fragment_tokens INTEGER NOT NULL DEFAULT 4000,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Profiles should be versioned because changing the prompt changes the compiled
artifact. A source compiled under `support_ticket_v3` is not necessarily
equivalent to the same source compiled under `support_ticket_v4`.

### 7.4 Compiler runs and diagnostics

```sql
CREATE TABLE _pg_ripple.compiler_runs (
    id                  BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    profile_name        TEXT NOT NULL REFERENCES _pg_ripple.compiler_profiles(name),
    source_document_id  BIGINT REFERENCES _pg_ripple.source_documents(id),
    source_fragment_id  BIGINT REFERENCES _pg_ripple.source_fragments(id),
    run_kind            TEXT NOT NULL, -- full, incremental, repair, revalidate
    status              TEXT NOT NULL,
    input_hash          TEXT NOT NULL,
    output_hash         TEXT,
    model               TEXT,
    prompt_tokens       INTEGER,
    completion_tokens   INTEGER,
    started_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at         TIMESTAMPTZ
);

CREATE TABLE _pg_ripple.compiler_diagnostics (
    id                  BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    run_id              BIGINT NOT NULL REFERENCES _pg_ripple.compiler_runs(id),
    severity            TEXT NOT NULL, -- info, warning, violation, blocked
    diagnostic_code     TEXT NOT NULL,
    subject_iri         TEXT,
    message             TEXT NOT NULL,
    evidence            JSONB,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Diagnostics are the compiler-error analogue. Examples:

- `unresolved_entity`: extracted entity has no label or cannot be linked.
- `contradictory_fact`: two trusted sources assert mutually exclusive values.
- `low_confidence_extraction`: LLM emitted a fact below the configured threshold.
- `shape_violation`: compiled graph failed SHACL validation.
- `stale_dependency`: an artifact depends on a source fragment that changed.

### 7.5 Artifact dependencies

The dependency graph is the key to incremental compilation.

```sql
CREATE TABLE _pg_ripple.compiled_artifacts (
    id                  BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    artifact_iri        TEXT NOT NULL UNIQUE,
    artifact_kind       TEXT NOT NULL, -- fact, entity_page, summary, qa_pair, index_node, embedding
    graph_iri           TEXT NOT NULL,
    profile_name        TEXT NOT NULL,
    content_hash        TEXT NOT NULL,
    generated_by_run    BIGINT REFERENCES _pg_ripple.compiler_runs(id),
    valid_from          TIMESTAMPTZ NOT NULL DEFAULT now(),
    valid_until         TIMESTAMPTZ
);

CREATE TABLE _pg_ripple.artifact_dependencies (
    artifact_id         BIGINT NOT NULL REFERENCES _pg_ripple.compiled_artifacts(id),
    dependency_kind     TEXT NOT NULL, -- source_fragment, source_document, entity, artifact, rule
    dependency_ref      TEXT NOT NULL,
    dependency_hash     TEXT,
    PRIMARY KEY (artifact_id, dependency_kind, dependency_ref)
);
```

This is the table that lets the system answer: "which entity pages, summaries,
QA pairs, communities, embeddings, and outbox events need to change because this
document fragment changed?"

## 8. RDF vocabulary for compiled knowledge

The report should not prescribe every vocabulary, but a small first-party
namespace would make compiled artifacts predictable.

Suggested namespace: `https://pg-ripple.dev/ns/compile#`, abbreviated `pgc:`.

Core classes:

- `pgc:SourceDocument`
- `pgc:SourceFragment`
- `pgc:CompilerRun`
- `pgc:CompiledArtifact`
- `pgc:EntityPage`
- `pgc:AtomicFact`
- `pgc:GeneratedQuestion`
- `pgc:GeneratedAnswer`
- `pgc:CompilerDiagnostic`
- `pgc:KnowledgeIndex`
- `pgc:IndexEntry`

Core predicates:

- `pgc:compiledFrom`
- `pgc:generatedByRun`
- `pgc:dependsOn`
- `pgc:hasConfidence`
- `pgc:hasEvidenceText`
- `pgc:hasSourceOffset`
- `pgc:hasSummaryShort`
- `pgc:hasSummaryMedium`
- `pgc:hasSummaryLong`
- `pgc:answersQuestion`
- `pgc:relatedArtifact`
- `pgc:invalidatedBy`
- `pgc:diagnosticCode`
- `pgc:repairSuggestion`

PROV-O should be used wherever possible instead of inventing equivalents:

- `prov:wasDerivedFrom` for source linkage.
- `prov:generatedAtTime` for compile timestamps.
- `prov:wasGeneratedBy` for compiler runs.
- `prov:wasAttributedTo` for source systems, users, or agents.

RDF-star annotations should capture statement-level provenance and confidence:

```turtle
<< ex:product-42 ex:hasPainPoint ex:slow-onboarding >>
    prov:wasDerivedFrom <urn:source:support-ticket-987> ;
    pgc:hasConfidence "0.83"^^xsd:double ;
    pgc:hasEvidenceText "It took our new admins three days to get configured" .
```

This makes answers auditable at the fact level, not just document level.

## 9. Proposed APIs

### 9.1 Register a compiler profile

```sql
SELECT pg_ripple.register_compiler_profile(
    name            => 'product_feedback',
    output_schema   => '{
      "entities": ["Customer", "Product", "Feature", "PainPoint"],
      "relationships": ["requests", "blocks", "mentions", "duplicates"],
      "summaries": ["short", "medium", "long"],
      "qa_pairs": true
    }'::jsonb,
    prompt_template => $prompt$
      Extract product feedback as RDF-ready JSON.
      Use only facts supported by the source text.
      Include evidence spans and confidence for each fact.
    $prompt$,
    shacl_shapes_graph => 'urn:pg-ripple:shapes:product-feedback',
    datalog_ruleset    => 'product_feedback_alignment'
);
```

### 9.2 Compile a document

```sql
SELECT pg_ripple.compile_document(
    source_uri      => 'https://docs.example.com/product/onboarding',
    content         => $doc$ ... $doc$,
    profile         => 'product_feedback',
    graph_iri       => 'urn:source:docs:product:onboarding',
    mode            => 'incremental'
);
```

The SQL function should enqueue work and return a run ID for non-trivial
documents. Synchronous compilation is useful for tests and tiny documents, but
production deployments should let the companion worker perform long-running LLM
calls outside backend transactions.

### 9.3 Compile from a pg_trickle inbox

```sql
SELECT pg_ripple.attach_compiler_to_inbox(
    inbox_table       => 'support_ticket_inbox',
    source_uri_expr   => 'payload->>''ticket_url''',
    content_expr      => 'payload->>''body''',
    profile           => 'support_ticket',
    graph_iri_expr    => '''urn:source:support:'' || (payload->>''ticket_id'')'
);
```

This is the event-native path. A ticket update, transcript arrival, or customer
review enters through pg_trickle, becomes a source document/fragment row, and
queues only the affected compilation work.

### 9.4 Recompile changed sources

```sql
SELECT pg_ripple.recompile_changed_sources(
    profile        => 'product_feedback',
    max_documents  => 100,
    reason         => 'source_hash_changed'
);
```

### 9.5 Inspect compiler diagnostics

```sql
SELECT *
FROM pg_ripple.compiler_diagnostics(
    severity_at_least => 'warning',
    graph_iri         => 'urn:source:docs:product:onboarding'
);
```

### 9.6 Query the compiled knowledge base

```sql
SELECT pg_ripple.rag_context(
    question       => 'Which onboarding pain points are mentioned by enterprise customers?',
    max_triples    => 200,
    include_types  => true,
    include_labels => true
);
```

### 9.7 Subscribe to compiled artifact changes

```sql
SELECT pg_ripple.create_subscription(
    name             => 'compiled_product_feedback',
    query            => $sparql$
      CONSTRUCT {
        ?feature ex:hasPainPoint ?pain .
        ?pain pgc:hasConfidence ?confidence .
      }
      WHERE {
        ?feature ex:hasPainPoint ?pain .
        << ?feature ex:hasPainPoint ?pain >> pgc:hasConfidence ?confidence .
        FILTER(?confidence >= 0.75)
      }
    $sparql$,
    outbox_table     => 'compiled_feedback_outbox',
    include_inferred => true
);

SELECT pgtrickle.set_relay_outbox(
    'compiled-feedback-to-kafka',
    outbox => 'compiled_feedback_outbox',
    group  => 'feedback-publisher',
    sink   => '{"type":"kafka","topic":"knowledge.compiled.feedback"}'
);
```

## 10. Compilation outputs

The article lists common compiled outputs. pg_ripple should represent all of
them in graph-native form.

### 10.1 Atomic facts

Atomic facts are normal triples plus provenance annotations. They are the
highest-value artifact because they support exact SPARQL, Datalog, SHACL, and
CDC.

Examples:

- `ex:policy-17 ex:requiresApproval ex:manager-approval`
- `ex:customer-42 ex:requests ex:feature-sso-scim`
- `ex:paper-123 ex:usesMethod ex:contrastive-learning`

### 10.2 Entity pages

The article's "wiki page" maps naturally to an entity-centric subgraph, not
only a Markdown blob. For each entity:

- labels and aliases
- type/class assignments
- canonical URI and `owl:sameAs` links
- short/medium/long summaries
- key relationships
- source coverage list
- contradiction list
- confidence score
- embedding vector
- related entities and communities

The entity page can be rendered as Markdown or JSON-LD, but the stored artifact
should be graph data.

### 10.3 Multi-granularity summaries

Summaries should be RDF literals attached to source fragments, source documents,
entities, communities, and queryable topics.

Suggested predicates:

- `schema:description` for short summary.
- `schema:abstract` for paragraph summary.
- `pgc:hasSummaryLong` for detailed summary.

Summaries should carry `prov:wasDerivedFrom` and a content hash so they can be
invalidated when their dependencies change.

### 10.4 Generated QA pairs

Generated QA pairs are useful for retrieval and evaluation. Store them as first
class artifacts:

```turtle
<urn:qa:source-42:q7> a pgc:GeneratedQuestion ;
    schema:text "What approvals are required for international travel?" ;
    pgc:answersQuestion <urn:qa:source-42:a7> ;
    prov:wasDerivedFrom <urn:source:travel-policy> .

<urn:qa:source-42:a7> a pgc:GeneratedAnswer ;
    schema:text "International travel requires manager and finance approval." ;
    pgc:dependsOn ex:manager-approval, ex:finance-approval .
```

These pairs can train domain examples for `sparql_from_nl()`, seed evaluation
sets, and support fast retrieval for common questions.

### 10.5 Knowledge index graph

Karpathy's related "index file" idea can become a queryable graph instead of a
single `index.md` document.

The index graph should contain:

- top-level topics
- entity clusters
- source coverage by topic
- recommended traversal paths
- representative questions
- community reports
- freshness and confidence metadata

Example:

```sparql
SELECT ?topic ?label ?summary ?related
WHERE {
  GRAPH <urn:pg-ripple:index> {
    ?topic a pgc:IndexEntry ;
           rdfs:label ?label ;
           schema:abstract ?summary ;
           skos:related ?related .
  }
}
```

This gives agents a stable navigation map without depending only on vector
similarity. Embeddings still help, but they become one access path over a richer
index.

## 11. Incremental compilation design

Incremental compilation is where pg_ripple plus pg_trickle can be genuinely
novel.

### 11.1 Change detection

For each source document or fragment, store:

- source URI
- stable fragment key
- content hash
- last source timestamp
- compiler profile version
- output hash
- dependency hash set

If the source content hash and compiler profile version are unchanged, skip the
LLM call. If the prompt/profile changes, recompile only artifacts produced by
that profile.

### 11.2 Dependency invalidation

When a fragment changes:

1. Mark the fragment as dirty.
2. Find artifacts where `artifact_dependencies` references that fragment.
3. Mark dependent entity pages, summaries, QA pairs, embeddings, and index nodes
   as stale.
4. Recompile the changed fragment.
5. Diff old and new compiled triples.
6. Apply triple insertions/deletions through pg_ripple's normal mutation path.
7. Let CONSTRUCT writeback, Datalog, SHACL, subscriptions, and pg_trickle
   outboxes propagate downstream effects.

### 11.3 Graph diff instead of text overwrite

The compiler should not blindly delete a whole source graph and insert a new
one unless the deployment chooses that mode. Better modes:

- `append`: keep previous compiled facts; add newly extracted facts.
- `replace_graph`: delete and reload the source graph.
- `diff`: compute old vs new compiled triples and apply only delta.
- `review`: store candidate facts in a staging graph until approved.

The `diff` mode is the most interesting. It lets pg_trickle and pg_ripple work
as a real incremental build system rather than a batch loader.

### 11.4 pg_trickle stream table use

Potential stream tables:

- `source_document_changes`: new or changed source rows.
- `compiler_work_queue`: work items derived from source deltas.
- `compiled_triple_delta`: LLM output diff represented as Z-set triples.
- `compiled_artifact_changes`: entity-page, summary, QA, and index updates.
- `compiler_diagnostics_outbox`: warnings/errors for UI or human review.

This gives operators observable queues and lets the relay publish progress to
external systems.

### 11.5 Delete and rederive for uncertain derived knowledge

Some derived artifacts are not simple enough for Z-set maintenance. Examples:

- community summaries
- global corpus summaries
- contradiction explanations
- cluster labels
- graph embeddings

For those, use a DRed-like strategy: mark affected artifact as stale, rederive
from surviving dependencies, then atomically swap the artifact version.

### 11.6 Build graph visualization

The dependency graph should be queryable:

```sql
SELECT *
FROM pg_ripple.explain_compilation(
    source_uri => 'https://docs.example.com/product/onboarding'
);
```

Output should show:

- source fragments
- compiler profile
- generated artifacts
- downstream CONSTRUCT views
- Datalog rules touched
- subscriptions/outboxes affected
- stale artifacts waiting for rebuild

This is the knowledge-base equivalent of `cargo tree`, `make -n`, or an EXPLAIN
plan.

## 12. Query-time runtime design

The query-time path should use the compiled graph first, raw source second.

### 12.1 Exact retrieval path

For questions with structural answers, use `sparql_from_nl()` -> SPARQL ->
`rag_context()`.

Examples:

- "Which policies apply to contractors in Germany?"
- "Which accounts mention both SCIM and audit-log gaps?"
- "Which product areas have more than five high-confidence pain points this
  month?"
- "Which documents contradict the current security policy?"

These should compile to SPARQL and Datalog-backed graph traversal, not vector
chunk search.

### 12.2 Index navigation path

For broad questions, query the index graph and community summaries first:

- topic taxonomy
- community reports
- entity centrality
- recent changes
- source coverage

Then perform targeted local graph retrieval. This resembles GraphRAG global and
local search, but keeps the graph live in PostgreSQL.

### 12.3 Hybrid fallback path

Use vector search over summaries, evidence spans, entity descriptions, and QA
pairs when exact SPARQL cannot identify the target. Importantly, the retrieved
vector hit should point back to graph artifacts and evidence spans. The LLM
should receive:

- exact triples
- summaries
- confidence/provenance
- source snippets when needed
- diagnostics or contradictions

The raw document should be fallback evidence, not the primary runtime artifact.

### 12.4 Answer packages

Return a structured answer package before natural-language generation:

```json
{
  "question": "Which onboarding pain points are mentioned by enterprise customers?",
  "triples": [...],
  "summaries": [...],
  "evidence": [...],
  "contradictions": [...],
  "confidence": 0.86,
  "sources": [...],
  "staleness": "fresh"
}
```

The final LLM response should be a rendering of this package. Applications can
also consume the package directly.

## 13. Novel ideas beyond the article

### 13.1 Live knowledge compiler, not static wiki

The article's compiled artifact is wiki-like. pg_ripple can make the compiled
artifact executable: SPARQL queries, Datalog rules, SHACL constraints, CONSTRUCT
views, subscriptions, and CDC events all operate over it.

The compiled output is not just readable. It is computable.

### 13.2 Knowledge CI/CD

Treat source updates like commits:

- compile changed documents
- run SHACL tests
- run Datalog contradiction tests
- run generated QA regression tests
- compare answer-package diffs
- publish only if checks pass

`compiler_diagnostics` becomes the equivalent of build output. pg_trickle outbox
events become CI notifications. Human reviewers can approve or reject staged
graphs.

### 13.3 Semantic pull requests

A document update can produce a semantic diff:

- facts added
- facts removed
- entity links changed
- contradictions introduced/resolved
- summaries invalidated
- answers affected

Instead of reviewing a raw document diff, a domain owner reviews the knowledge
diff. This is a stronger human-in-the-loop workflow than most RAG systems offer.

### 13.4 Source-trust and probabilistic compilation

v0.87.0's uncertain knowledge work fits perfectly. The compiler can assign
confidence to extracted facts, while source graphs carry trust scores. Datalog
can propagate confidence through derived facts. SHACL can produce soft quality
scores.

This supports answers like:

> The strongest supported answer is A with confidence 0.82. Source B disagrees,
> but it has lower trust and older provenance.

That is more useful than hiding conflicts or pretending all extracted facts are
equally true.

### 13.5 Agent memory bus

pg_trickle can turn compiled knowledge changes into events that agents subscribe
to:

- `entity.updated`
- `policy.contradiction.detected`
- `source.needs_review`
- `summary.invalidated`
- `answer_package.changed`

Agents no longer need to poll a vector store. They can react to semantic change
events with exactly-once or at-least-once delivery semantics depending on relay
configuration.

### 13.6 Bidirectional compiled knowledge

The v0.77/v0.78 bidirectional primitives suggest a powerful extension: downstream
systems can write corrections, labels, linkbacks, and human approvals back into
the graph. pg_ripple then resolves conflicts and republishes the resolved
projection.

Example loop:

1. LLM extracts `customer-42 ex:requests ex:sso` from a call transcript.
2. Product manager corrects it to `ex:scim` in a UI.
3. UI sends a correction event through pg_trickle inbox.
4. pg_ripple records the correction in a higher-priority human-review graph.
5. Conflict policy suppresses the lower-confidence extraction.
6. Outbox publishes the resolved knowledge update.

This turns the compiled knowledge base into a learning system without treating
the LLM as the source of truth.

### 13.7 Speculative precompilation

The compiler can generate likely questions and precompute retrieval plans:

- generated QA pairs become few-shot examples for `sparql_from_nl()`
- common questions get cached SPARQL templates
- entity pages precompute neighbor summaries
- community reports precompute broad answer outlines

This lowers query latency and creates stable, testable answer paths.

### 13.8 Knowledge packages

Compile a corpus into a portable package:

- RDF named graphs
- SHACL shapes
- Datalog rules
- compiler profile
- GraphRAG community summaries
- embeddings metadata
- QA evaluation set
- provenance manifest

This could become a distribution format for domain knowledge: install a package
into pg_ripple, run validation, and immediately query it. Think "extension" for
knowledge domains, not only code.

### 13.9 Federated compiled knowledge

A company may not want to centralize all raw source content. pg_ripple's SPARQL
federation plus compiled artifact exchange can support a federated design:

- each department compiles its own documents locally
- only compiled summaries/facts are shared
- sensitive raw text never leaves the source domain
- cross-domain queries use SERVICE or replicated safe summaries

This is a privacy-preserving variation of the architecture.

## 14. Example end-to-end scenarios

### 14.1 Enterprise documentation compiler

Inputs:

- Confluence pages
- GitHub Markdown docs
- Slack/Teams decision logs
- policy PDFs
- support KB articles

Pipeline:

1. pg_trickle reverse relay ingests page-change events and webhook payloads.
2. `pg_ripple_compile` fetches or receives source text.
3. Compiler profile extracts policies, owners, products, procedures, exceptions,
   and related systems.
4. SHACL checks every policy has owner, effective date, and scope.
5. Datalog derives policy applicability and detects conflicts.
6. Index graph organizes knowledge by team, system, policy area, and freshness.
7. `rag_context()` answers employee questions with provenance.
8. pg_trickle publishes `policy.changed` and `policy.contradiction` events.

Why this beats raw RAG: policy questions often require exact applicability,
dates, ownership, exceptions, and contradictions. Those are graph queries, not
chunk similarity problems.

### 14.2 Product intelligence compiler

Inputs:

- support tickets
- Gong/Zoom call transcripts
- CRM notes
- app telemetry events
- product feedback forms

Pipeline:

1. Each source enters through a source-specific graph.
2. Compiler extracts customer, account, feature, pain point, sentiment, urgency,
   and evidence spans.
3. Entity resolution links CRM accounts, ticket users, and transcript speakers.
4. Confidence combines source trust and extraction confidence.
5. Datalog derives themes, duplicates, and account-level risk.
6. Community summaries describe clusters of pain points.
7. Outbox sends high-confidence product signals to roadmap tools.

Novel output: not just "summaries of feedback", but an incrementally maintained
semantic product graph with exact links from feature requests to accounts,
source evidence, confidence, and trend changes.

### 14.3 Research-library compiler

Inputs:

- papers
- lab notes
- benchmark reports
- citations
- experiment metadata

Pipeline:

1. Compiler extracts claims, methods, datasets, metrics, baselines, and
   limitations.
2. Datalog normalizes metric comparisons and derives citation influence.
3. SHACL validates claim structure and required evidence.
4. Contradiction rules identify conflicting results under similar conditions.
5. Index graph maps research areas, methods, and open questions.

Novel output: a living research map where new papers invalidate or strengthen
existing claims.

### 14.4 Operational event compiler

Inputs:

- alerts
- incident reports
- deploy events
- logs summarized by upstream services
- runbook changes

Pipeline:

1. pg_trickle relays events into inbox tables.
2. Compiler turns incidents and logs into typed operational facts.
3. Datalog links symptoms to services, deploys, owners, and runbooks.
4. SHACL ensures incidents have severity, affected service, and timeline.
5. Outbox emits derived `probable_root_cause` or `runbook.updated` events.

Novel output: an agent-ready operations memory that improves after every
incident and can answer "what changed before this alert pattern?" with graph
evidence.

## 15. MVP scope

The first version should be deliberately small. Avoid building a general
workflow engine. Use PostgreSQL catalogs, pg_ripple graph writes, and one
companion worker.

### 15.1 MVP deliverables

1. `_pg_ripple.source_documents`, `_pg_ripple.source_fragments`,
   `_pg_ripple.compiler_profiles`, `_pg_ripple.compiler_runs`,
   `_pg_ripple.compiler_diagnostics`, `_pg_ripple.compiled_artifacts`, and
   `_pg_ripple.artifact_dependencies`.
2. `pg_ripple.register_compiler_profile(...)`.
3. `pg_ripple.enqueue_compile_document(...)`.
4. `pg_ripple.compiler_queue()` and `pg_ripple.compiler_diagnostics()` SRFs.
5. `pg_ripple_compile` worker with OpenAI-compatible endpoint support, mock mode,
   rate limits, retry/backoff, and structured JSON output validation.
6. Basic compiled vocabulary: source, fragment, compiler run, atomic fact,
   summary, QA pair, diagnostic.
7. SHACL validation of compiler output before publish.
8. Named graph write modes: `append`, `replace_graph`, `review`.
9. Graph-level provenance and statement-level confidence annotations.
10. A simple index graph generated from entities, summaries, and communities.
11. pg_trickle inbox attachment for source events.
12. pg_trickle outbox publication for compiler diagnostics and compiled artifact
    changes.

### 15.2 Explicit non-goals for MVP

- No custom UI.
- No arbitrary workflow DAG editor.
- No first-party document connectors beyond pg_trickle-compatible inboxes and
  direct SQL/API calls.
- No automatic deletion of raw source data.
- No promise that LLM-extracted facts are trusted without SHACL/profile checks.
- No global corpus re-summarization on every change.

## 16. Later phases

### Phase 1: Document compiler foundation

- Catalogs and SQL APIs.
- Companion worker.
- Structured LLM output validation.
- Basic RDF/provenance vocabulary.
- Mock mode for CI.
- End-to-end tests on small Markdown and JSON fixtures.

### Phase 2: Incremental compilation

- Stable fragmenter.
- Artifact dependency graph.
- Diff mode for compiled triples.
- Stale artifact tracking.
- pg_trickle stream tables for work queues and outboxes.
- `explain_compilation()`.

### Phase 3: Graph-native compiled wiki

- Entity pages.
- Topic index graph.
- Multi-granularity summaries.
- Generated QA pairs.
- Community summaries maintained from compiled graph.
- `rag_context()` integration that prefers compiled artifacts.

### Phase 4: Review, trust, and uncertainty

- Human-review staging graphs.
- Conflict policies for LLM vs human vs source-system assertions.
- Source trust scores.
- Confidence propagation using probabilistic Datalog.
- Soft SHACL compilation-quality scores.
- Semantic diffs for review.

### Phase 5: Event-native agent ecosystem

- Agent subscriptions to compiled artifact changes.
- Answer-package invalidation events.
- Knowledge package export/import.
- Federated compiled knowledge exchange.
- Benchmarks versus vector RAG and static GraphRAG.

## 17. Evaluation plan

We should evaluate this as a compiler, not only as a chatbot.

### 17.1 Compile-time metrics

- source documents processed per hour
- fragments skipped by content hash
- LLM cost per source document
- structured-output validation failure rate
- SHACL violation rate
- unresolved entity rate
- contradiction rate
- average artifacts per fragment
- percentage of artifacts with evidence spans

### 17.2 Incremental metrics

- changed source -> compiled triple delta latency
- changed source -> refreshed entity page latency
- changed source -> outbox delivery latency
- stale artifact count
- unnecessary recompilation rate
- full refresh avoided count
- dependency fan-out distribution

### 17.3 Query-time metrics

- answer latency
- triples retrieved per answer
- raw source snippets needed per answer
- hallucination/error rate on gold QA set
- citation/evidence coverage
- contradiction disclosure rate
- SPARQL generation repair rate

### 17.4 Comparative benchmarks

Compare four systems on the same corpora:

1. vector RAG over raw chunks
2. static LLM-generated wiki
3. Microsoft GraphRAG-style batch graph
4. pg_ripple + pg_trickle live compiled graph

Question classes:

- direct factual lookup
- multi-hop entity traversal
- aggregation over facts
- contradiction detection
- "what changed?" questions
- broad corpus sensemaking
- time-bounded questions

The combined pg_ripple/pg_trickle system should shine on multi-hop,
aggregation, contradiction, and change-aware questions.

## 18. Security, governance, and correctness risks

### 18.1 Prompt injection

Raw documents can contain instructions aimed at the compiler. The compiler
profile must instruct the LLM to treat source text as data, not instructions.
Output must be schema-validated and SHACL-validated before publish.

### 18.2 LLM hallucinated facts

Every extracted fact should carry confidence and evidence. Profiles can require
that facts without evidence go to a review graph or diagnostic table rather than
the trusted compiled graph.

### 18.3 Destructive recompilation

Replacing a whole graph on every source change is simple but dangerous. The
default should be staging or diff mode for production. Deletions should be
observable and reversible where feasible.

### 18.4 Sensitive source leakage

Summaries can leak raw source content. Compiler profiles should support field
redaction, graph-level RLS, and output policies. pg_trickle relays should not
publish sensitive summaries unless the subscription explicitly allows them.

### 18.5 Non-determinism

LLM outputs can vary. Store prompt version, model, temperature, input hash,
output hash, and run metadata. Critical domains should use deterministic model
settings and review workflows.

### 18.6 Cost explosion

Incremental hashing, fragment-level compilation, profile versioning, and
dependency tracking are mandatory. Without them, the architecture becomes an
expensive batch re-indexer.

### 18.7 Trust boundary confusion

Compiled facts are not automatically true. They are assertions from a compiler
agent. The system should make the asserting graph explicit: source graph,
compiler graph, human-review graph, or resolved projection.

## 19. Why this is strategically interesting

The market already has vector databases, RAG frameworks, agent workflow tools,
and graph databases. The differentiated product is not "RAG with RDF" or "an
LLM that writes triples". The differentiated product is:

- PostgreSQL-native source of truth.
- Exact graph retrieval and reasoning at runtime.
- Compile-time LLM extraction with validation and provenance.
- Incremental recompilation through pg_trickle.
- Change events for agents and downstream systems.
- Built-in conflict, confidence, source trust, and human review paths.

This creates a new category: **compiled operational knowledge**. It is not a
static knowledge base and not an ephemeral agent memory. It is a maintained
artifact with build metadata, tests, provenance, runtime query plans, and change
events.

## 20. Recommended next steps

1. Build a small proof-of-concept around one corpus: Markdown docs or support
   tickets are the best candidates.
2. Add the source/compiler catalog schema in a draft migration plan.
3. Prototype `pg_ripple_compile` as a separate worker that calls existing
   pg_ripple SQL functions rather than embedding LLM calls in backend sessions.
4. Define `pgc:` vocabulary and SHACL shapes for compiler output.
5. Implement a mock compiler profile for CI so tests are deterministic.
6. Demonstrate incremental update: edit one source fragment, recompile only that
   fragment, diff compiled triples, refresh entity page, publish outbox event.
7. Compare the demo against raw vector RAG and static GraphRAG on questions that
   require multi-hop reasoning and change awareness.

The first demo should show the thing that the article does not solve: a source
update arrives as an event, only dependent knowledge artifacts rebuild, SHACL and
Datalog check the result, and downstream agents receive a semantic change event.
That is where pg_ripple and pg_trickle become more than an implementation of the
compiler analogy. They become the build system and runtime for living knowledge.
