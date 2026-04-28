# pg_ripple — Future Directions

> **Status:** Speculative strategy document, not a roadmap commitment.
> **Audience:** Maintainers, contributors, prospective adopters, and anyone
> trying to understand where pg_ripple could go after v1.0.0.
> **Authority rule:** [implementation_plan.md](implementation_plan.md) and
> [../ROADMAP.md](../ROADMAP.md) remain authoritative for what is *planned*.
> This document explores what *could* be planned.

---

## 0. How to read this document

pg_ripple stands at an unusual crossroads. After 67 numbered releases it has
become — measured by surface area alone — one of the most feature-complete
open-source RDF stacks in existence: a SPARQL 1.1 engine, a SHACL validator,
a Datalog reasoner, an HTAP storage architecture, a federation client with a
circuit breaker, an HTTP service with Arrow Flight export, OWL 2 RL/EL/QL
profiles, KG embeddings, a Citus sharding integration, GeoSPARQL, temporal
queries, PROV-O, R2RML, GraphRAG export, and an LLM-aware RAG retriever.
That breadth is unique among PostgreSQL extensions and unusual even compared
to standalone triple stores.

The question this document tries to answer is not *"what features are
missing?"* — there are always missing features. The question is *"which of
the many directions pg_ripple could plausibly take in the next five years
would create the most lasting value?"*

The directions are grouped into fifteen thematic vectors plus one
explicit set of anti-directions. Each vector is
scored along three axes — **strategic value**, **engineering cost**, and
**risk** — and concludes with a recommended posture: *invest*, *opportunistic*,
*watch*, or *decline*. None of these are decisions; they are the framing the
maintainers can use when planning v1.2, v2.0, and beyond.

---

## 1. Where pg_ripple sits today

Before discussing where it could go, it is worth being precise about what
pg_ripple actually is *as a product*, separate from what it is *as a piece
of code*.

### 1.1 The product proposition

pg_ripple's distinctive claim is **"a complete knowledge graph stack inside
PostgreSQL"**. Every other RDF system worth comparing it to (Virtuoso,
Blazegraph, GraphDB, Stardog, Apache Jena/Fuseki, Oxigraph, AllegroGraph,
Amazon Neptune) is a separate process with its own storage engine, its own
backup story, its own access control, its own monitoring stack, and its own
HA/DR primitives. pg_ripple is the only mature implementation that delegates
all of those concerns to PostgreSQL.

That is both its strongest and most fragile differentiator. Strongest because
PostgreSQL operations is a multi-billion-dollar industry built on decades of
DBA expertise that pg_ripple inherits for free. Fragile because the moment a
serious workload exceeds what a single PostgreSQL cluster can serve, the
"inside PostgreSQL" advantage erodes — and that ceiling is real.

### 1.2 The technical proposition

Underneath the product, pg_ripple is also:

- A **dictionary-encoded vertically partitioned RDF triple store** with
  HTAP delta/main split, BRIN-indexed cold storage, and a per-predicate
  table layout that lets the PostgreSQL planner do something other graph
  databases force a custom planner to do.
- A **SPARQL 1.1 → SQL compiler** that emits PostgreSQL plans the existing
  optimizer can reason about — including parallel queries, AIO, and skip
  scan — rather than re-implementing query planning from scratch.
- A **Datalog reasoner** with magic sets, semi-naive evaluation, well-founded
  semantics, parallel stratum execution, tabling, DRed retraction, and OWL
  RL/EL/QL profiles compiled to recursive CTEs.
- A **CDC-friendly write path** that integrates with `pg_trickle` for
  incremental view maintenance and emits NOTIFY events for live
  subscriptions.

The combination of a *standards-compliant graph stack* with a *familiar
relational substrate* is rare. The two largest production RDF stores
(Wikidata Query Service, DBpedia) both run on Blazegraph and both have spent
years looking for replacements; the most credible replacements (Qlever,
Oxigraph) optimise heavily for one workload (read-heavy SPARQL on static
dumps) and skip the operational baggage that makes a real database useful.
pg_ripple is one of the few systems that takes the operational baggage
seriously.

### 1.3 The honest limits

Three constraints bound everything that follows:

1. **PostgreSQL single-writer.** Even with Citus, writes are coordinated
   through one node and ultimately one logical clock. Wikidata-scale
   write throughput (tens of thousands of triples per second sustained)
   is achievable but not effortless.
2. **JIT-less dynamic SQL.** The SPARQL → SQL pipeline produces parameterised
   PostgreSQL plans. PostgreSQL's planner is excellent for OLTP and OLAP
   workloads with well-known statistics, but graph workloads have famously
   bad cardinality estimates, and dynamic plans cannot be compiled to
   native code the way Qlever or DuckDB-style engines can.
3. **No first-party UI.** pg_ripple is a database. There is no Workbench,
   no schema visualiser, no SPARQL playground beyond the experimental
   visual graph explorer. Adoption ceilings imposed by the absence of UI
   should not be underestimated.

These three constraints implicitly define which directions are realistic
and which are wishful thinking. The vectors below are scored with all three
in mind.

---

## 2. Vector A — Deep PostgreSQL integration

**Posture: Invest. This is the moat.**

pg_ripple's first-mover advantage is being the only serious RDF stack
inside PostgreSQL. Everything that deepens that integration widens the moat.

### A.1 PostgreSQL 19 / 20 readiness

PostgreSQL 19 is expected to land async-IO improvements, expanded skip-scan
coverage, and (likely) a stable interface for table access methods that
covers more of what columnar storage needs. pg_ripple should plan two
upgrade tracks:

- **Tracking upgrade**: maintain feature parity within ~30 days of each
  major PostgreSQL release.
- **Capability upgrade**: when a new PostgreSQL feature unlocks a
  pg_ripple use case (e.g. native columnar table access methods unlocking
  a true column-store VP option), publish a release that adopts it.

This requires a CI matrix that tracks PG18, PG19-beta, PG20-dev, and the
PG-master branch. Cost is real (3× CI minutes, occasional pgrx version
churn) but the payoff is the project never falls behind the substrate
that is its chief differentiator.

### A.2 Native table access method

Today every VP table is a heap with B-tree indices. PostgreSQL's table
access method API (introduced in PG12, gradually maturing) allows an
extension to register a different physical storage layout. A pg_ripple
TAM could:

- Store `(s, o)` and `(o, s)` columnar with run-length encoding (most
  predicates have a small set of distinct subjects/objects).
- Skip the heap row header overhead (24 bytes per tuple) which dominates
  storage size for narrow VP tables.
- Enable vectorised execution of the inner loops PostgreSQL already supports.

This is a multi-quarter project (the TAM API is famously underspecified
for non-heap layouts) and would compete with Citus's columnar TAM, but
delivered well it could **halve disk footprint** on Wikidata-scale data
sets while preserving PostgreSQL semantics.

### A.3 Logical replication of triples

v0.54 added logical-decoding-based RDF replication. A natural extension
is to make the **replicated stream itself a first-class API**: any
downstream system that speaks PostgreSQL logical replication (Debezium,
Materialize, RisingWave, ClickHouse, BemiDB) can subscribe to a stream
of *RDF events* — not row-level changes — and act on them. This is
qualitatively different from the v1.1 Kafka sink because it preserves
exactly-once semantics through the WAL.

### A.4 PostgreSQL parallel workers for SPARQL

PostgreSQL parallel query is gated on the planner deeming a query
parallel-safe. Today many SPARQL → SQL plans fall outside that window
(custom functions are not always proven parallel-safe, recursive CTEs
disable parallelism). A focused effort to:

- Mark every internal function `PARALLEL SAFE` where provably true.
- Replace recursive CTEs in the property-path translator with parallel-safe
  equivalents where the depth is bounded.
- Audit dictionary lookups for parallel safety.

…could let large SPARQL queries fan out across worker processes, often
delivering 4–8× speedup on multi-core hardware without any new code in
the SPARQL engine.

### A.5 Use of PG18 AIO across the storage layer

PG18 introduced asynchronous I/O. Any sequential scan that touches a
multi-billion-row VP table is a candidate. The merge worker, the BRIN
summarisation pass, and the bulk-load path are the obvious targets.
The change is mostly "add `effective_io_concurrency` knobs and verify
they propagate through to the new AIO subsystem".

### A.6 Graph-aware statistics and cardinality feedback

The PostgreSQL planner is excellent when it has statistics that match
the workload. RDF workloads routinely violate the assumptions behind
ordinary table statistics: predicates have power-law distributions,
objects often carry semantic type constraints not visible to the SQL
planner, and star patterns correlate strongly across predicates sharing
the same subject. pg_ripple should invest in graph-aware statistics
that remain native to PostgreSQL rather than replacing the optimizer.

The obvious starting point is a `_pg_ripple.graph_stats` catalog with:

- Per-predicate subject/object distinct counts.
- Predicate co-occurrence sketches for common star patterns.
- Path-length histograms for high-traffic transitive predicates.
- Named-graph selectivity summaries.
- Literal datatype and language-tag frequency histograms.

The SPARQL translator can use these summaries to choose join order,
push filters earlier, and decide when WCOJ mode is worth the overhead.
The generated SQL would still execute through PostgreSQL; pg_ripple
would simply hand the planner a shape that is less likely to explode.

### A.7 Relational-to-graph zero-copy views

R2RML direct mapping exists, but it still frames relational data as a
load operation. A deeper PostgreSQL-native move is **zero-copy semantic
views**: users define RDF mappings over ordinary relational tables, and
SPARQL queries can traverse those mappings without materialising triples
unless explicitly requested.

This is the inverse of the usual triple-store story. Instead of asking
an enterprise to copy all customer/product/order data into an RDF store,
pg_ripple would let them create a semantic layer over the relational
tables they already trust. For many users this is the difference between
"interesting sidecar" and "deployable in production".

The long-term version would combine:

- R2RML/RML mappings compiled to SQL views.
- A cost model that decides when to query relational source tables
  directly and when to materialise triples into VP tables.
- Change capture from base tables into mapped named graphs.
- SHACL validation against mapped relational data before graph export.

The result would make pg_ripple a semantic layer for PostgreSQL itself,
not only a store for already-RDF data.

---

## 3. Vector B — Graph protocol pluralism

**Posture: Invest selectively. The graph world is no longer SPARQL-only.**

The single biggest demographic the SPARQL world ignores is the
property-graph community: every Neo4j user, every TigerGraph user, every
Memgraph user, every JanusGraph user, every Amazon Neptune Cypher user,
every Spark GraphFrames user. ISO/IEC 39075 GQL was ratified in April 2024
and is, in effect, "Cypher with a real spec". This is the largest
addressable market expansion pg_ripple has access to.

### B.1 Cypher / GQL — beyond v1.1

v1.1 ships a read+write Cypher transpiler. The natural follow-on work
includes:

- **Full openCypher 9 conformance** with the openCypher TCK as a CI gate.
- **GQL conformance** as the standard matures (the test suite is still
  taking shape — being early-mover here is high signal).
- **Mixed Cypher/SPARQL transactions**: a single SQL function call that
  contains both. Trivially expressible because both compile to the same
  storage layer.
- **Cypher-native indexing hints** so existing Neo4j applications can
  port their hint comments without modification.
- **APOC compatibility shim**: the most-used Neo4j procedures (path
  expansion, NLP, JSON ingest) implemented as `pg_ripple_apoc.*`. A
  surprisingly large fraction of Neo4j applications are really APOC
  applications.

### B.2 Property graphs as a first-class storage option

Today pg_ripple represents property graph data as RDF (subject = node,
property = edge, value = literal). For workloads that are *primarily*
property-graph workloads, this is wasteful — a typical Neo4j edge has
several properties on the edge itself, and modelling that in RDF requires
either reification (expensive) or RDF-star (well-supported but verbose).

A native property-graph storage tier — `_pg_ripple.pg_node` and
`_pg_ripple.pg_edge` tables with first-class JSONB property columns —
could coexist with the VP tables, with a unified query layer that knows
how to join both. This is essentially the *OneGraph 1G* model that
underlies the SID design choices already in place. Extending it to a
full PG-native storage option is a 2–3 release effort but radically
expands the workloads pg_ripple can serve well.

### B.3 GraphQL over the graph

A subset of GraphQL queries can be answered directly from any graph
store. A `pg_ripple_graphql` companion (similar in spirit to
Hasura/PostgREST but for the RDF schema) would expose any named graph
as a GraphQL endpoint, with the schema inferred from class definitions
and SHACL shapes. This pairs naturally with the JSON-LD framing work
already in place (v0.17).

### B.4 RDF 1.2 / SPARQL 1.2 and beyond

The W3C RDF 1.2 working group is finalising RDF-star as a normative
standard. SPARQL 1.2 is being scoped. Track these standards, run the
emerging conformance suites, and aim to be the *first* commercial-grade
SPARQL 1.2 implementation. Standards-leadership credibility is hard to
buy and has long tail value (citations, conference invitations, formal
sponsorship opportunities).

### B.5 SQL-facing semantic projections

Many PostgreSQL users will never write SPARQL, Cypher, or GQL, but they
still want graph-derived facts in BI tools, dashboards, and SQL models.
pg_ripple should provide a first-class projection layer that exposes
classes, shapes, or named SPARQL queries as ordinary SQL views with
stable schemas.

Examples:

```sql
SELECT pg_ripple.create_class_view(
  class_iri => 'https://schema.org/Person',
  view_name => 'kg_person'
);
```

or:

```sql
SELECT pg_ripple.create_query_view(
  name => 'customer_risk_summary',
  query => 'SELECT ?customer ?risk ?evidence WHERE { ... }'
);
```

The strategic point is not novelty; it is interoperability. Tableau,
Power BI, Metabase, Superset, dbt, Looker, and every SQL client can
consume a view. The graph remains the source of meaning, but SQL remains
the surface most enterprises already know how to govern.

### B.6 Graph API compatibility shims

The graph ecosystem is fragmented across SPARQL Protocol, Neo4j Bolt,
Gremlin, TinkerPop, RDF4J Repository API, Jena Dataset API, and vendor
REST dialects. pg_ripple should not implement every protocol inside the
extension, but a set of compatibility shims could make migrations much
less painful:

- A Bolt-compatible read endpoint for common Neo4j drivers.
- A Gremlin/TinkerPop subset for traversal-heavy Java applications.
- RDF4J and Jena repository adapters that treat pg_ripple as a storage
  backend.
- A SPARQL Graph Store Protocol compliance test suite for the HTTP
  companion.

These shims are unglamorous but valuable: they convert "rewrite the
application" into "swap the backend and fix edge cases".

---

## 4. Vector C — AI/LLM-native knowledge layer

**Posture: Invest hard. This is where the market is moving and pg_ripple
has a non-obvious lead.**

The AI ecosystem of 2025–2027 is collectively rediscovering knowledge
graphs. The same shape of problem keeps appearing under different names:
GraphRAG, knowledge-augmented generation, structured RAG, "agents need
state", "LLMs hallucinate without grounding". Every one of these is a
knowledge-graph problem dressed up.

pg_ripple already has a credible AI story (vector embeddings, RAG
retriever, JSON-LD framing for prompts, GraphRAG export, NL→SPARQL).
The opportunity is to *own* the "knowledge layer for AI applications"
position the way pgvector owns the "vector store inside PostgreSQL"
position.

### C.1 Native multi-modal entity store

Today an entity in pg_ripple is a set of triples. AI applications want
each entity to also carry:

- A canonical text description (for embedding).
- A vector embedding (for similarity search).
- A set of structured properties (for filters).
- A provenance record (for citation).
- An optional image/audio/video attachment (for multi-modal retrieval).

The storage primitives are already there (VP tables, embeddings table,
PROV-O, large object support). What is missing is a *first-class
"Entity" object* with a single Python/TypeScript SDK that handles all
of the above as one operation. Building this is mostly client-library
work and would substantially lower the on-ramp for AI engineers.

### C.2 Agentic graph operations

LLM agents need to *write* to the graph, not just read. Today this
requires the agent to construct SPARQL Update queries — a high-friction
interface. A safer pattern is:

- A small set of typed mutation functions (`add_fact`, `add_entity`,
  `link_entities`, `flag_for_review`).
- A *staging graph* the agent writes to, that becomes promoted to
  production only after SHACL validation passes.
- A *verification trail*: every agent-written triple is tagged with the
  agent identity, the prompt, and the supporting evidence document(s).

This is essentially a managed-write API for AI agents. It plays to all
of pg_ripple's strengths (SHACL validation, PROV-O, named graphs, RLS).

### C.3 Knowledge-graph-aware embeddings

The KGE work in v0.57 (TransE, RotatE) is research-oriented. The
production-relevant extension is:

- Continuously-trained embeddings that update incrementally as triples
  arrive (the canonical example is GraphSAGE-style inductive
  embeddings).
- Hybrid retrieval that fuses *text* embeddings (from external models),
  *graph structural* embeddings (from KGE), and *symbolic* SPARQL
  filters in a single query, with a tunable fusion strategy.
- A SQL-callable training loop (probably running outside the database
  process but coordinated through pg_ripple) so model retraining is
  observable and operable through the same Postgres tooling everything
  else uses.

### C.4 LLM-as-a-judge SHACL repair

When SHACL validation fails on incoming data, today the only response
is to reject the data. A more useful response is to *propose a fix* and
let a reviewer accept it. An LLM is reasonably competent at this for
many common SHACL violation classes (missing `rdf:type`, value out of
expected range, wrong datatype). A `pg_ripple.shacl_repair_suggest()`
function that takes a validation failure and returns one or more
candidate fixes (with confidence scores) would close a major usability
gap in data-quality pipelines.

### C.5 Native MCP server

Anthropic's Model Context Protocol (MCP) has emerged as the *de facto*
standard for connecting LLM agents to external tools. A first-class
`pg_ripple_mcp` server (alongside `pg_ripple_http`) that exposes:

- `query_graph(sparql)` — NL or SPARQL.
- `find_similar(text, limit)` — vector + graph hybrid.
- `get_entity(iri)` — full entity card with provenance.
- `add_fact(s, p, o, source)` — staged write with validation.
- `explain_inference(iri, predicate)` — derivation tree for any inferred
  fact.

…with appropriate auth and rate limiting, would make pg_ripple a
plug-and-play knowledge-tool for any MCP-enabled agent (Claude Desktop,
Cursor, Goose, every IDE assistant).

### C.6 RAG-evaluation harness

RAG quality is hard to measure. A `pg_ripple_eval` harness — borrowing
from RAGAS, TruLens, ARES — that lets users run their RAG pipeline
against held-out gold standards and report Recall@k, MRR, and
faithfulness, all from SQL, would differentiate pg_ripple from "just
a vector store".

### C.7 Long-lived agent memory

Most agent systems treat memory as either a vector index or a transcript
log. Knowledge graphs offer a richer model: facts can be scoped, dated,
validated, contradicted, retired, and explained. pg_ripple could become
the durable memory substrate for agents by formalising a memory model
around named graphs:

- **Episodic graph**: what the agent observed or did.
- **Semantic graph**: distilled facts the agent believes.
- **Policy graph**: rules that constrain what the agent may do.
- **Preference graph**: stable user/project preferences.
- **Evidence graph**: citations and provenance for every retained fact.

This would make pg_ripple useful not merely for RAG retrieval but for
stateful AI systems that need to remember, revise, and justify.

### C.8 Prompt and tool governance

Once agents can query and mutate a graph, pg_ripple sits on a security
boundary. It should expose explicit controls for AI tool use:

- Per-tool allow/deny policies stored as RDF or Datalog rules.
- Graph-scoped access decisions for prompt context assembly.
- Prompt-injection detection based on provenance and source trust.
- Audit trails linking model output to graph facts and tool calls.
- Redaction policies that strip sensitive facts before prompt assembly.

This is a natural extension of RLS, named graphs, SHACL, and PROV-O.
It positions pg_ripple as a governance layer for AI systems, not only a
retrieval backend.

### C.9 Synthetic graph generation and simulation

Many teams cannot share production graphs for development, benchmarking,
or support because the data is sensitive. pg_ripple could provide
synthetic graph generation that preserves structural properties without
leaking actual entities:

- Degree distribution and predicate-frequency preservation.
- SHACL-shape-aware fake entity generation.
- Literal synthesis with privacy constraints.
- Workload replay against synthetic graphs for performance debugging.
- Differentially-private summaries for support bundles.

This would make operational support much easier and also produce better
benchmark corpora than purely artificial datasets.

---

## 5. Vector D — Distributed scale

**Posture: Invest, but bound the ambition.**

The Citus integration (v0.59 onwards) is a credible answer to "how do I
go beyond one machine?" but it is not a credible answer to "how do I
serve Wikidata?" or "how do I run multi-tenant SaaS on top of pg_ripple?".
The first of those is interesting; the second is a market.

### D.1 Multi-tenant SaaS topology

A common deployment shape is *one pg_ripple cluster, hundreds or thousands
of small graphs*. Multi-tenant features already in place (per-graph RLS,
tenant quotas) cover the basics. What is missing:

- Per-tenant resource governors (CPU and IO time budgets).
- Per-tenant rate limiting at the HTTP layer.
- Per-tenant audit logs surfaced through a tenant-facing dashboard.
- Per-tenant dictionary scopes (so a noisy tenant cannot pollute the
  shared dictionary cache).
- A tenant *export* / *import* primitive that round-trips a single
  tenant's graph as a self-contained backup.

Delivered as a coherent set this would make pg_ripple the obvious choice
for any AI startup building a "knowledge layer per customer" product.

### D.2 Beyond Citus

Citus is solid but Citus is also Microsoft-owned and Microsoft's commitment
beyond Azure is uncertain. Three alternative scale-out paths deserve
exploration:

- **YugabyteDB / CockroachDB compatibility**: both speak the PostgreSQL
  wire protocol but have different storage and transaction semantics.
  An audit pass would surface where pg_ripple makes assumptions that
  do not hold (e.g. table OIDs, CTID).
- **PG18 native partitioning**: VP tables are natural candidates for
  hash partitioning on subject. PG18 partitioning is mature. A
  partitioned-VP option would give a single-node scale-out path that
  does not require Citus at all.
- **Read-replica-driven analytical fan-out**: heavy SPARQL queries
  routed to replicas via the read-replica routing already in place,
  combined with a coordinator that splits federated queries across
  replicas. This is "Citus lite" using only stock PostgreSQL.

### D.3 Storage tiering — hot, warm, cold

Wikidata is ~16B triples. Most production knowledge graphs have a long
tail of rarely-accessed data. A *cold* tier — VP tables on object
storage (S3, GCS) accessed via PostgreSQL foreign data wrappers or
DuckDB-style extension — would let pg_ripple credibly serve graphs in
the 100B-triple range without keeping them all on local SSD. This pairs
naturally with the existing `vp_rare` archival pattern.

### D.4 Geographic federation

Multi-region deployments today require either (a) full replication
(expensive) or (b) full federation (slow). A *partial replication* model
where each region holds the named graphs it most often queries plus
metadata about where the rest live, with the federation planner using
that metadata to route SERVICE calls intelligently, would unlock
geo-distributed knowledge graphs without the cost of full mirroring.

### D.5 Edge and intermittently-connected graphs

Industrial, defense, maritime, and remote-field deployments often need a
local knowledge graph that keeps working when disconnected from the
central service. PostgreSQL already runs well at the edge; pg_ripple
could lean into that with an edge-sync profile:

- Named-graph subscriptions replicated to edge nodes.
- Conflict-free merge policies for facts collected offline.
- Compact N-Quads or Arrow snapshots for low-bandwidth sync.
- Local SHACL validation before reconnect.
- Provenance that records whether a fact was observed centrally or at
  the edge.

This is a different scale story from Citus. Citus scales one logical
cluster; edge sync scales many occasionally-connected clusters.

### D.6 Workload placement advisor

As pg_ripple accumulates storage options — dedicated VP tables, `vp_rare`,
delta/main, Citus shards, cold object storage, materialized views,
semantic relational views — users will need guidance about where data
should live. A workload placement advisor could inspect query history,
predicate cardinalities, graph temperature, and tenant boundaries, then
recommend:

- Promote/demote predicates.
- Add or drop graph-leading indexes.
- Move named graphs to cold storage.
- Materialise a query view.
- Rebalance Citus shards.
- Split a tenant into a separate cluster.

The strongest version would emit executable SQL plus a risk explanation,
similar to how PostgreSQL's `EXPLAIN` explains planner choices.

---

## 6. Vector E — Reasoning frontier

**Posture: Opportunistic. The current reasoner already exceeds what most
users need; selective investment can preserve research credibility.**

pg_ripple's Datalog/OWL stack is genuinely strong: magic sets, semi-naive,
well-founded semantics, parallel strata, DRed, magic-set demand
transformation, OWL 2 RL/EL/QL. Few open-source systems match it.

### E.1 OWL 2 DL profile

OWL 2 DL is undecidable in the worst case but practically tractable for
many ontologies. Adding a *bounded* OWL 2 DL reasoner — perhaps by
embedding HermiT or Konclude as a sidecar, or by implementing a
PostgreSQL-native variant of the ELK consequence-based algorithm —
would close the gap with commercial reasoners (Stardog, RDFox, GraphDB
Enterprise).

### E.2 Probabilistic and weighted reasoning

ProbLog, Markov Logic Networks, and weighted Datalog are emerging from
research into production. A `pg_ripple.weighted_inference()` mode that
treats facts and rules as weighted (probability, fuzzy weight,
evidential mass) would open use cases in entity resolution, fraud
detection, and biomedical reasoning that the strict-logic reasoner
cannot serve.

### E.3 Temporal reasoning beyond `point_in_time()`

The current temporal feature lets users query the graph as-of a past
time. A richer temporal reasoner would support:

- Allen interval relationships (`before`, `meets`, `overlaps`).
- Temporal property paths (`ex:knows[2020-2024]`).
- Continuous aggregation over time-series facts attached to entities.

This connects naturally to TimescaleDB integration — another PostgreSQL
extension — for hybrid temporal-graph workloads.

### E.4 Counterfactual / abductive inference

"Given the goal G, what minimal set of facts would, if added, entail G?"
Abductive reasoning over Datalog is a classic AI problem with clear
applications in diagnostics, root-cause analysis, and *what-if*
modelling. A research-grade `pg_ripple.abduce(goal, max_facts)` function
would be a unique capability among production triple stores.

### E.5 Schema mining and rule learning

The inverse of inference: given a graph, learn the rules. AMIE+ and
its successors are standard for RDF rule mining; NetKAT-like systems
do the same for property graphs. A `pg_ripple.mine_rules(graph,
min_support, min_confidence)` function that surfaces likely rules and
proposes them for human review would help users grow ontologies that
match their actual data.

### E.6 Truth-maintenance as an explicit subsystem

DRed retraction exists as an implementation detail. A future direction
is to expose truth-maintenance concepts directly: users should be able
to ask why a fact currently exists, what would make it disappear, which
source facts support it, and how expensive it would be to maintain under
different update patterns.

This would turn derivation metadata into a product feature:

- `why(fact)` returns derivation trees and source evidence.
- `why_not(pattern)` returns missing facts or failed rule branches.
- `impact_delete(fact)` estimates derived facts that would be retracted.
- `maintenance_cost(rule)` reports incremental maintenance complexity.

For regulated and AI workloads, explanation is not a bonus. It is the
reason to choose a symbolic graph instead of an opaque embedding store.

### E.7 Constraint solving and planning

SHACL validates whether a graph satisfies constraints; Datalog derives
facts. The adjacent capability is *planning*: find assignments or action
sequences that make the graph satisfy a goal. This touches classic logic
programming, but a bounded PostgreSQL-native version is plausible:

- Resource allocation with graph constraints.
- Supply-chain substitution planning.
- Data remediation plans that satisfy SHACL.
- Role/permission assignment that avoids policy violations.

The risk is high because it can turn into a general solver. The safer
path is to ship small, bounded planning primitives that compile to SQL
and surface cost/timeout controls clearly.

---

## 7. Vector F — Developer experience and first-party UI

**Posture: Invest. The biggest adoption blocker is not features.**

The single most common complaint about RDF systems is "I cannot see my
data". The visual graph explorer in v0.62 is a start. A serious
first-party UX investment would dwarf its value.

### F.1 pg_ripple Studio

A web application — packaged with `pg_ripple_http`, optional, off by
default — that provides:

- **Schema browser**: classes, properties, SHACL shapes, tenants,
  named graphs, all in one tree view.
- **SPARQL playground** with autocomplete, prefix management, query
  history, saved queries, and one-click sharing.
- **Visual query builder**: drag-and-drop pattern construction for
  users who do not know SPARQL.
- **Graph explorer**: starting from any IRI, expand neighbours
  interactively, with optional layout algorithms.
- **Inference inspector**: click any derived triple, see its derivation
  tree (already exposed via `explain_inference()`).
- **Live query monitor**: pg_stat_statements integration, top-N slow
  queries, query plan visualisation.
- **Data quality dashboard**: SHACL violations over time, tenant
  quotas, dictionary growth.

This is a 6–12 month project for a single dedicated frontend engineer.
The leverage on adoption is hard to overstate.

### F.2 SDKs

pg_ripple speaks PostgreSQL wire protocol so any client *works*, but
the ergonomics are bad. First-party SDKs would:

- **Python** (`pip install pg-ripple`): typed wrappers over `sparql`,
  `cypher`, `cypher_write`, `infer`, `validate`, `subscribe`. Pandas
  and Arrow-flight integration. Pydantic models generated from SHACL
  shapes.
- **TypeScript / Node**: similar surface, with Zod schemas from SHACL.
- **Rust**: thin client over `tokio-postgres` with `oxrdf` types
  surfaced directly.
- **Java / Kotlin**: integrations with RDF4J and Apache Jena so
  existing JVM RDF code can use pg_ripple as a backend without rewrite.
- **Go**: high-performance client for streaming bulk loads and CDC
  consumers.

### F.3 IDE integrations

- **VS Code extension**: `.sparql` syntax highlighting, on-save
  validation, `EXPLAIN` integration, schema-aware autocomplete from a
  configured pg_ripple endpoint.
- **JetBrains plugin**: same, for IDEA/PyCharm.
- **Cursor / Copilot rules**: pre-baked AI rules for working with
  pg_ripple SPARQL/Datalog code (mirroring the SKILL files already in
  this repo).

### F.4 Onboarding

Today's installation requires Rust, pgrx, PostgreSQL 18, and patience.
Three asks reduce this to minutes:

- Pre-built binary packages for the major Linux distros (the apt/dnf
  PostgreSQL repos already package extensions; pg_ripple should be
  among them).
- A `docker run pg-ripple/quickstart` that boots a PG18 + pg_ripple
  + sample dataset + Studio in one command.
- A hosted free-tier playground (potentially as part of any
  commercial offering — see §10) so prospective users can try
  pg_ripple without installing anything.

### F.5 Solution templates

Instead of starting from blank SQL, users should be able to start from
templates that encode a whole pattern:

- Customer 360 graph.
- Research-paper citation graph.
- Product catalogue with schema.org export.
- Governance, risk, and compliance evidence graph.
- RAG knowledge base with vector + graph retrieval.
- Data lineage graph for a warehouse or lakehouse.

Each template would include a starter ontology, SHACL shapes, sample
data, queries, dashboards, and a short tutorial. This is documentation,
product, and test fixture all at once.

### F.6 Certification and learning path

The project should eventually publish a structured learning path:

- "RDF for SQL users".
- "SPARQL for analysts".
- "SHACL for data quality engineers".
- "Datalog for application developers".
- "Operating pg_ripple in production".

Certification sounds premature, but a lightweight exam or badge can be
valuable for consultants and enterprise teams trying to prove internal
competence. It also creates a vocabulary for community support: when a
user says they completed the operator track, maintainers know what
knowledge they can assume.

---

## 8. Vector G — Standards leadership and academic credibility

**Posture: Opportunistic. Cheap to acquire, durable to hold.**

pg_ripple already passes 100% of the W3C SPARQL 1.1, SHACL Core, and
OWL 2 RL conformance suites. That is genuinely rare. The opportunity is
to *be seen* doing it.

### G.1 Public conformance dashboard

A page (probably `conformance.pg-ripple.io`) that publishes daily test
results across SPARQL 1.1, SHACL Core, OWL 2 RL/EL/QL, GeoSPARQL,
LUBM, WatDiv, BSBM, and the upcoming SPARQL 1.2 / RDF 1.2 suites.
Comparable to <https://www.sqlite.org/testing.html> or
<https://duckdb.org/testing>. This signals seriousness in a way
README badges cannot.

### G.2 Academic publication track

The technical foundations of pg_ripple (HTAP for RDF, magic-set
demand transformation in PostgreSQL, OWL 2 RL via recursive CTEs,
Citus shard-pruning for SPARQL, RDF-star with shared statement IDs)
are all publishable contributions. SIGMOD, VLDB, ISWC, and ESWC
are the obvious venues. Two solid papers a year is achievable and
generates a citation tail that compounds.

### G.3 Engagement with W3C / DBLP / LDBC

- Active membership in the W3C RDF-star and SPARQL 1.2 working
  groups.
- Submission of pg_ripple as a reference implementation for the
  upcoming standards.
- Participation in LDBC benchmark development (LDBC-SNB,
  LDBC-Semantic-Web).
- Sponsorship of SemWebPro, ESWC, and ISWC conferences.

### G.4 Pre-bundled reference datasets

Wikidata, DBpedia, UniProt, MeSH, SNOMED-CT (for those licensed),
GeoNames, schema.org. Each available as a one-line load:

```sql
SELECT pg_ripple.load_reference_dataset('wikidata-latest');
```

…with proper attribution and update tracking. This is enormously
valuable for evaluation, demos, and academic work, and consolidates
pg_ripple's position as "the" reference RDF system to start from.

### G.5 Interoperability test farm

Conformance to standards is necessary but not sufficient. Users migrate
from real systems with real quirks. A public interoperability test farm
could run the same workloads across pg_ripple, Jena, Oxigraph, Qlever,
Virtuoso, GraphDB, Stardog (where licensed), RDF4J, and Neptune-like
interfaces, publishing:

- Query result diffs.
- Update semantics differences.
- Federation behaviour differences.
- SHACL report shape differences.
- RDF-star compatibility status.

This would be valuable even when pg_ripple is not fastest. It makes the
project a trustworthy source of semantic-web operational knowledge.

### G.6 Reproducible benchmark notebooks

Benchmarks should be inspectable, not just claimed. Publish Jupyter or
Quarto notebooks that generate each benchmark corpus, load it, run the
queries, and render the result charts. This supports academic review,
sales engineering, and internal performance work with the same artifact.

---

## 9. Vector H — Operational excellence

**Posture: Invest. Production credibility is mostly about boring things.**

### H.1 Backup, restore, and PITR for triples

`pg_dump` works today (and the v0.60 round-trip CI test verifies it),
but it is row-level. A `pg_ripple_dump` that emits *graphs* as Turtle
or N-Quads — preserving prefix registries, SHACL shapes, Datalog
rules, and CONSTRUCT writeback rules — would be much more useful for
graph-aware backup, archival, and migration. Pair with point-in-time
restore (already free from PostgreSQL) for a credible DR story.

### H.2 First-class observability

OpenTelemetry tracing exists. The next layer of work:

- A maintained Grafana dashboard pack (JSON) with everything an
  operator needs for first-day pg_ripple operations.
- Pre-baked Prometheus alerting rules (HTAP merge backlog, SPARQL
  query timeouts, federation circuit breaker open, dictionary cache
  thrash).
- Out-of-the-box integration with pg_stat_monitor, auto_explain, and
  PgAnalyze.
- A `pg_ripple.diagnostic_bundle()` SQL function that produces a
  single tarball with all the artifacts a support engineer needs to
  triage an incident.

### H.3 Upgrade paths

Migrations are written today (per AGENTS.md). The harder operational
problem is *zero-downtime major version upgrades*. The combination of
pg_upgrade + ALTER EXTENSION UPDATE works in principle but is rarely
tested at scale. A multi-billion-triple staging environment that
exercises the upgrade path on every release would harden a story
production users currently take on faith.

### H.4 Multi-version support window

Define a long-term-support track (every other minor release? every
fourth?), commit to backporting security fixes for 18 months, and
publish CVE acknowledgements promptly. This is what enterprise
adopters expect and what most open-source databases struggle to
provide consistently.

### H.5 SLO-driven test suite

Beyond conformance and unit tests, an SLO suite that verifies:

- p95 SPARQL latency on a 100M-triple corpus stays within an envelope
  release-over-release.
- Bulk load throughput stays above a published baseline.
- Memory ceiling for streaming cursors holds under adversarial input.
- HTAP merge does not stall under sustained write pressure.

Failure of any SLO blocks the release. This codifies "we did not
regress" in a way no benchmark suite alone can.

### H.6 Security posture as a release artifact

The v1.0 security audit closes one moment in time. Future releases need
security posture as a living artifact:

- Threat model per subsystem: extension SQL, HTTP service, federation,
  Arrow tickets, MCP, agent writes, Citus, and CDC.
- SAST/DAST reports attached to releases.
- Signed SBOM and SLSA provenance for every binary artifact.
- Public vulnerability response policy with timelines and backport
  commitments.
- A security changelog separate from feature release notes.

This is especially important because pg_ripple sits near both data and
AI boundaries. It will be asked to hold sensitive facts and expose them
to tools that may be model-driven.

### H.7 Disaster drills and chaos testing

Production readiness is partly the ability to recover when the worst
thing happens. pg_ripple should run scheduled drills that intentionally
interrupt:

- HTAP merge cutover.
- Citus shard rebalance.
- Arrow export streams.
- Federation endpoint calls.
- WAL-based RDF replication.
- Construct-rule incremental maintenance.

The goal is not theatrical chaos engineering; it is evidence. Each drill
should produce a short report: failure injected, expected degradation,
observed behaviour, recovery time, and follow-up fixes.

---

## 10. Vector I — Commercial and community sustainability

**Posture: Required. Open source without sustainability is borrowed time.**

pg_ripple is large enough that maintenance alone is a non-trivial
commitment. Three sustainability paths exist; they are not mutually
exclusive.

### I.1 Foundation governance

Migrate copyright assignment / contribution governance to a neutral
foundation (Apache, Linux Foundation, OpenSSF, or PostgreSQL Global
Development Group as a contrib extension). The political work is
substantial but the credibility yield with enterprises is large —
a foundation-hosted project survives the loss of any single
maintainer.

### I.2 Commercial sponsor / dual-license offering

A paid tier could plausibly include:

- The first-party UI (Studio).
- Enterprise SSO / RBAC integrations (SAML, OIDC, Okta).
- A managed hosted offering (think Supabase or Crunchy Bridge for RDF).
- Long-term support contracts.
- Priority security patching.

The open-source core remains permissively licensed (Apache-2.0 already);
the commercial offering captures value from operations and support, not
from feature gating. This is the model used by Timescale, Citus,
Hashicorp (pre-BUSL), Supabase, and others.

### I.3 Grant funding

NLnet, Sovereign Tech Fund, EU Next Generation Internet, NSF POSE, and
the various AI-safety funders have all funded comparable open-source
infrastructure. A grant program manager (paid 0.2 FTE) is enough to
keep a steady pipeline of grant applications moving and could plausibly
fund 1–2 FTEs of development.

### I.4 Community

- An annual pg_ripple conference (or co-located track at PGConf or ISWC).
- A monthly community call.
- A funded contributor onboarding programme (Outreachy, Google Summer of
  Code, university capstone projects).
- A clear path from drive-by contributor to committer to maintainer.

### I.5 Naming and branding

The name `pg_ripple` is fine for an extension. If the project's ambitions
include a hosted product, a Studio UI, and SDKs across five languages, a
parent brand (e.g. **Ripple Knowledge Graph**, with `pg_ripple` as the
PostgreSQL-extension component) becomes useful. Decide deliberately,
not by accident.

### I.6 Ecosystem partnerships

Some future directions are too large for pg_ripple to own alone.
Strategic partnerships could make them tractable:

- PostgreSQL vendors for managed extension availability.
- pgvector / pgvectorscale maintainers for hybrid retrieval.
- Timescale for temporal graph + hypertable integration.
- Citus and Crunchy Data for distributed operations guidance.
- W3C / RDF4J / Apache Jena communities for standards and API adapters.
- AI framework maintainers for LangChain, LlamaIndex, MCP, and agent
  memory integrations.

The goal is not logo collection. It is reducing integration drift by
making pg_ripple part of other projects' test matrices.

### I.7 Hosted service strategy

A hosted pg_ripple service is not just "PostgreSQL with an extension".
It would need opinionated defaults:

- One-click dataset loading.
- Studio enabled by default.
- Graph-aware backups and tenant export.
- Per-tenant usage metering.
- Managed embedding workers and model credentials.
- Federation allowlists and outbound network controls.
- Support bundles and SLO dashboards built in.

If the project pursues a commercial path, the hosted service is the
clearest value capture. The open-source extension remains the engine;
the hosted product sells time, reliability, and operational confidence.

---

## 11. Vector J — Adjacent technologies to absorb

**Posture: Watch and selectively absorb. Each item below is a project unto
itself; doing all of them is a way to do none of them.**

### J.1 Vector indexing beyond pgvector

pgvector's HNSW is good but the field is moving (DiskANN, ScaNN,
LM-style learned indexes). A pluggable vector backend (`pgvecto.rs`,
`pgvectorscale`, `lantern`) would let users pick the right index for
their workload without pg_ripple needing to implement them.

### J.2 Spatial beyond PostGIS

The current GeoSPARQL implementation goes through PostGIS. For users
already invested in PostGIS this is ideal; for users on managed
PostgreSQL services without PostGIS access, an Apache SIS or
GeoArrow-based fallback would matter.

### J.3 Time-series via TimescaleDB

Many knowledge graphs have time-series facts attached to entities
(sensor readings, financial prices, IoT telemetry). Native integration
with TimescaleDB's hypertables — exposing time-series data as RDF
triples without copying — would unlock a class of workloads
(industrial digital-twin, observability over assets) that today
requires custom glue code.

### J.4 Search beyond PostgreSQL FTS

PostgreSQL's GIN-backed full-text search is good but limited.
Integration with:

- **pgsearch / ParadeDB** for BM25 and faceted search.
- **Apache Solr / Elasticsearch** as a federated text-search backend
  surfaced through SPARQL.
- **pgvector** for semantic search (already done, can be deepened).

…would let pg_ripple be the *single* answer for "search across my
entire knowledge graph" without users assembling 3-4 systems.

### J.5 Knowledge-graph-aware ETL

A close relative of dbt-pg-ripple (v1.1) is a graph-native ETL
framework — an "Airflow for knowledge graphs" with operators for
extract (R2RML, RML, SPARQL Construct from APIs), transform (Datalog,
SHACL repair, SPARQL Update), and load (CDC sink, Kafka, S3 export).
This could plausibly be a separate project that pg_ripple integrates
deeply with.

### J.6 Web of Data primitives

- DID (decentralised identifier) support for pg_ripple-issued
  identifiers.
- Verifiable credentials (W3C VC) as a typed literal kind.
- Solid Pod compatibility (pg_ripple as a Pod backend).

These are speculative but cheap to support if RDF-1.2 normalises
their representation.

### J.7 Ontology engineering tools

A `pg_ripple_ontology` editor (could be part of Studio) supporting:

- Class hierarchy editing.
- Property domain/range constraints.
- SHACL shape authoring with live validation against existing data.
- Diff and merge of ontology versions.
- Round-trip with Protégé `.owl` files.

### J.8 Semantic version of `pg_dump` for data migration

`pg_ripple_migrate(source_url, target_endpoint, mapping)` — moving
data between RDF systems is a chronic pain point. A standardised
migration tool (with mappings for Blazegraph, Virtuoso, Stardog,
GraphDB, Neo4j) makes pg_ripple the *destination* a lot of teams
will reach for when their incumbent triple store is sunset.

### J.9 Streaming table formats

Apache Iceberg, Delta Lake, and Apache Hudi have become the lingua
franca of lakehouse storage. pg_ripple should not become a lakehouse,
but it should understand lakehouse boundaries:

- Export named graphs as partitioned Iceberg tables.
- Maintain RDF-derived feature tables for ML platforms.
- Read slowly-changing dimension tables through FDWs and expose them
  as semantic graph views.
- Publish CDC streams that downstream lakehouse engines can compact.

This connects graph semantics to the analytics platforms enterprises
already operate.

### J.10 Knowledge graph feature store

Feature stores (Feast, Tecton, Databricks Feature Store) are mostly
tabular. Many high-value features are relational/graph-derived: number
of two-hop neighbours, membership in a risky component, shortest path to
a known fraud entity, shared supplier exposure, ontology class depth.
pg_ripple could materialize graph features with point-in-time correctness
and expose them to ML pipelines.

The strongest version would combine temporal RDF, SPARQL materialized
views, provenance, and Arrow export into a graph-native feature store
where every feature has a lineage trail back to triples.

---

## 12. Vector K — Trust, privacy, and governance

**Posture: Invest. Trust is the enterprise buying criterion.**

Many graph systems win technical pilots and lose production deployment
because they cannot satisfy data governance, privacy, consent, audit,
and regulatory requirements. pg_ripple has unusually strong raw material
for this space: PostgreSQL RLS, named graphs, PROV-O, audit logs, SHACL,
temporal queries, and ordinary SQL permissions. The future direction is
to assemble those pieces into a coherent trust layer.

### K.1 Policy-as-graph

Access policies are themselves graph-shaped: users have roles, roles
grant permissions, permissions apply to graph regions, graph regions
contain entities, entities carry labels, labels imply obligations.
Instead of treating policy as scattered SQL grants plus application
logic, pg_ripple could model policy inside a dedicated governance graph
and compile it to PostgreSQL RLS, HTTP authorization checks, and agent
tool policies.

This would let users ask questions like:

- Which roles can see facts about this customer?
- Which named graphs include data subject to export controls?
- Which SHACL shape enforces consent for this data product?
- Which downstream views would be affected if this policy changes?

The implementation should remain conservative: PostgreSQL still enforces
the final permission decision; the graph provides explanation, authoring,
and static analysis.

### K.2 Consent and purpose limitation

GDPR, HIPAA, and similar regimes care not only whether data is true, but
why it may be used. pg_ripple could support purpose-scoped named graphs:

- Facts tagged with allowed purposes (`analytics`, `support`, `fraud`,
  `training`, `research`).
- Queries executed under a declared purpose.
- Policy checks that reject joins across incompatible purposes.
- Audit logs that record purpose at query time.

This is particularly relevant to AI. A fact may be valid for customer
support but not for model training. Encoding that distinction at the
graph layer is far more robust than hoping every application remembers it.

### K.3 Data residency and sovereignty

Multi-region and hosted pg_ripple deployments will face residency rules:
EU data stays in the EU, health data stays in a regulated region, defense
data stays in a national boundary. Named graphs give a natural unit for
residency policy. The federation planner could use residency metadata
to avoid moving data across forbidden boundaries, and `explain_sparql()`
could report residency decisions alongside shard-pruning decisions.

This turns compliance from a deployment spreadsheet into a query-planning
constraint.

### K.4 Fine-grained redaction and partial disclosure

RLS answers "can this role see the row?" Knowledge graphs often need a
more nuanced answer: a user may see that an entity exists, but not a
specific property; or may see a generalized class but not the exact
diagnosis. Future work could include:

- Predicate-level redaction policies.
- Literal masking for sensitive values.
- Class generalization (e.g. exact diagnosis → broader category).
- Prompt-context redaction before LLM calls.
- Audit events when redaction changes a result.

The critical design principle is transparency: redacted results should be
marked as redacted when safe, not silently made to look complete.

### K.5 Confidential computing and encryption boundaries

PostgreSQL already supports TLS, disk encryption through the platform,
and role-based access. Some workloads require more: encrypted backups,
KMS integration, per-tenant keys, or confidential-computing enclaves for
query execution. pg_ripple should not build cryptography, but it can
integrate with the surrounding ecosystem:

- Envelope encryption for graph-aware exports.
- Signed graph snapshots.
- KMS-backed secrets for federation endpoints and LLM providers.
- Optional enclave-compatible deployment guides.
- Integrity proofs for exported RDF datasets.

These features are not glamorous, but they are exactly what turns a
prototype into an enterprise system.

### K.6 Governance evidence packs

Auditors do not want a console tour; they want evidence. pg_ripple could
generate evidence packs that include:

- Active policies and their compiled enforcement state.
- Recent access logs and SPARQL audit entries.
- SHACL validation history.
- Data lineage and provenance summaries.
- Retention and deletion job history.
- Release evidence dashboard snapshots.

This fits perfectly with the existing diagnostic-bundle idea, but the
audience is governance rather than support.

### K.7 Privacy-preserving analytics

Graphs are re-identification machines if mishandled. Future pg_ripple
analytics should include privacy-preserving modes:

- k-anonymity checks for exported subgraphs.
- Differentially-private aggregate SPARQL functions.
- Synthetic graph generation tied to SHACL shapes.
- Query budget accounting for sensitive graphs.

The project should be careful here: bad privacy tooling is worse than no
privacy tooling. But a conservative, well-documented subset would be a
significant differentiator.

---

## 13. Vector L — Domain solution accelerators

**Posture: Invest selectively. Vertical packaging turns technology into
outcomes.**

pg_ripple's generic feature list is impressive, but many buyers do not
buy generic features. They buy a solution to a domain problem. The goal
is not to turn pg_ripple into an industry application; it is to provide
accelerators that make the first production use case obvious.

### L.1 Life sciences and healthcare

The semantic-web stack is unusually strong in biomedical domains:
SNOMED CT, MeSH, UMLS, FHIR RDF, Gene Ontology, UniProt, DrugBank,
clinical trial ontologies. pg_ripple can become a pragmatic biomedical
KG platform by shipping:

- Reference loaders for common public ontologies.
- FHIR RDF import/export helpers.
- OMOP-to-RDF mappings.
- SHACL shape packs for common healthcare data-quality checks.
- RAG templates for clinical-trial matching and literature review.
- Audit and consent patterns aligned with healthcare privacy regimes.

This is a high-value vertical because RDF already has mindshare and the
need for explainable AI is acute.

### L.2 Financial services risk and compliance

Financial services care about entity resolution, beneficial ownership,
sanctions screening, fraud rings, KYC/AML workflows, model-risk evidence,
and auditability. pg_ripple already has the ingredients: `owl:sameAs`,
PROV-O, temporal queries, graph traversal, SHACL, and RLS. A finance
accelerator could include:

- Legal-entity ontology starter pack.
- Beneficial ownership traversal queries.
- Sanctions-list update pipeline.
- Suspicious-network pattern library.
- Explainable risk-score feature generation.
- Immutable evidence export for regulators.

The trap is becoming a compliance product. The opportunity is providing
the graph substrate compliance products want to build on.

### L.3 Manufacturing and supply-chain digital twins

Digital twins are graphs: assets, parts, suppliers, facilities, sensors,
maintenance events, constraints, documents, and locations. pg_ripple can
combine RDF, temporal facts, GeoSPARQL, TimescaleDB integration, and
SHACL to model these systems in one PostgreSQL cluster.

Possible accelerator components:

- Asset hierarchy ontology.
- Bill-of-materials graph model.
- Supplier-risk and dependency path queries.
- Sensor-to-entity mapping via temporal RDF.
- Offline edge-sync profile for factories and ships.
- Maintenance-rule templates using Datalog.

This is one of the clearest use cases for graph + time-series + edge
sync, and it avoids the crowded generic RAG market.

### L.4 Cybersecurity knowledge graphs

Security teams already think in graph terms: identities, hosts,
vulnerabilities, alerts, software packages, network paths, privileges,
attack techniques. pg_ripple could integrate MITRE ATT&CK, CVE/CPE,
SBOM, IAM, and SIEM data into a queryable graph.

High-value templates include:

- "Which internet-facing assets run software affected by this CVE?"
- "Which identities can reach this crown-jewel database?"
- "Which alerts form a plausible attack path?"
- "Which remediation breaks the most risk paths?"

This domain rewards provenance, temporal reasoning, and explainability.
It also naturally aligns with the project's SBOM and supply-chain
hardening work.

### L.5 Public-sector and open-data graphs

Governments publish RDF and linked-data datasets, but operating public
SPARQL endpoints has historically been painful. pg_ripple could offer a
public-data profile:

- Read-heavy deployment guidance.
- Dataset packaging and citation metadata.
- VoID and SPARQL Service Description first-class support.
- Caching and rate-limiting defaults for public endpoints.
- Bulk download endpoints for researchers.
- Accessibility-focused Studio views for non-technical users.

This is not necessarily the largest commercial market, but it produces
visibility, citations, and public-good credibility.

### L.6 Software engineering and observability graphs

Modern software organizations already have graphs: services, owners,
repositories, CI workflows, deploys, incidents, SLOs, dependencies,
feature flags, and vulnerabilities. pg_ripple can become a semantic
control plane for engineering operations:

- Service catalog graph.
- Code ownership and dependency traversal.
- Incident-to-change correlation.
- SBOM and vulnerability impact analysis.
- Architecture drift detection.
- RAG over runbooks with graph-grounded context.

This is a pragmatic adoption path because engineering teams can evaluate
the product on their own metadata before asking the rest of the business
to trust it.

---

## 14. Vector M — Semantic lakehouse and analytics

**Posture: Watch now, invest when the integration path is clear.**

Data platforms are moving toward lakehouse table formats, Arrow-native
execution, and cross-engine analytics. pg_ripple should not abandon
PostgreSQL to chase that world, but it should make graph semantics easy
to carry into it.

### M.1 Graph-to-lakehouse export contracts

Arrow export is a starting point. A richer export contract would define
stable layouts for:

- Triple tables partitioned by named graph and predicate.
- Entity tables generated from SHACL shapes.
- Edge tables compatible with graph data science frameworks.
- Feature tables with point-in-time correctness.
- Provenance tables linked to every export batch.

If those layouts are documented and versioned, downstream systems can
depend on them. That matters more than having yet another ad hoc export
function.

### M.2 DuckDB and local analytics

DuckDB has become the default local analytical engine. A pg_ripple +
DuckDB story could let users pull graph snapshots into notebooks,
perform local analytics, and push derived facts back after validation.
This should stay outside the core extension, probably as a Python SDK
integration using Arrow.

### M.3 Graph analytics algorithms

SPARQL answers pattern queries; many users also want PageRank, connected
components, community detection, centrality, similarity, and path
algorithms. PostgreSQL can run some of these, but dedicated graph
analytics engines are better. pg_ripple should provide adapters rather
than reimplement everything:

- Export to NetworkX / graph-tool for Python users.
- Export to GraphFrames / GraphX for Spark users.
- Export to cuGraph for GPU-heavy users.
- Import algorithm results as triples or feature tables.

The key is round-trip semantics: algorithm outputs should return with
provenance, timestamps, and validation.

### M.4 Semantic metric layer

BI tools increasingly rely on metric layers (dbt Semantic Layer,
Cube, LookML). pg_ripple could make metrics graph-aware: a metric is not
only a SQL expression, but a semantically typed measure with lineage,
applicable dimensions, business definitions, and governance policies.

This is where RDF shines. A metric catalog is a graph of concepts,
owners, definitions, dependencies, dashboards, and source tables. pg_ripple
could be the metadata brain behind an analytics stack even when the facts
being aggregated stay in ordinary warehouse tables.

### M.5 Data contracts and schema negotiation

SHACL shapes are effectively data contracts. A future integration with
streaming and lakehouse systems could treat shapes as contract artifacts:

- Producers publish SHACL contracts.
- Consumers declare which shapes they require.
- CDC streams validate outgoing events against contracts.
- Breaking shape changes require migration approval.

This would bring semantic-web rigor into the data-contract movement,
where many teams currently reinvent weaker versions of SHACL.

---

## 15. Vector O — Novel research bets

**Posture: Selective experimentation. One in five will pay off; that is
enough.**

The vectors above are mostly *known good directions* — extensions of
existing capabilities the team already understands. This section is
deliberately different: it lists bets that are speculative, possibly
wrong, and worth running as small skunkworks experiments rather than
multi-quarter commitments. Each is a one-paragraph sketch; each has the
property that *if it works*, pg_ripple becomes a meaningfully different
product.

### O.1 SPARQL JIT via PL/Rust

PostgreSQL plans cannot be compiled to native code today, but `plrust`
makes Rust available as a procedural language. A SPARQL → `plrust`
backend could compile hot queries to native Rust functions that operate
directly on dictionary IDs, bypassing the SQL planner for a small subset
of patterns where its overhead dominates. The bet: 5–10× speedup on
high-QPS small queries (the kind LLM agents emit constantly), at the
cost of a parallel query path that must be kept honest with the SQL
path through differential testing.

### O.2 Self-tuning indexes via reinforcement learning

Index choice today is operator judgment. A long-running background
worker could observe query workload, model index utility, and propose
or auto-create/drop indexes with a small RL policy. The bet: dictionary
hot tier, BRIN summarisation, named-graph indexes, and `vp_rare`
promotion all become adaptive without human intervention. Risk: RL
systems fail in surprising ways; ship as advisory mode first, with
explicit rollback.

### O.3 Differentiable knowledge graphs

Neural Theorem Provers, Neural-Symbolic Stack Machines, and similar
systems treat triples as fuzzy facts in a learned vector space and
"prove" goals via differentiable rules. A `pg_ripple_neural` companion
could expose Datalog rules over a soft fact base, returning ranked
proofs with attention scores. The bet: knowledge graphs become useful
even when the data is incomplete, because partial proofs are still
informative. Risk: high research overhead, narrow audience initially —
but the AI community is hungry for this.

### O.4 Conversational graph editing

A "co-pilot for ontology curation" that watches user actions, suggests
new shapes when patterns emerge, proposes merges when entities look
duplicated, and flags rules that contradict newly added facts. The bet:
ontology engineering becomes a dialog rather than a one-shot design
exercise. This pairs naturally with the LLM SHACL repair (C.4) and
schema mining (E.5) work, but reframes them as continuous assistance
rather than batch tools.

### O.5 Event-sourced graphs

Treat the triple store as a derived view over an immutable event log:
every assertion, retraction, and rule firing becomes an event with a
monotonic offset. The current VP tables become the "default projection"
of those events, but other projections (point-in-time, per-tenant,
per-purpose, per-policy) can be built and rebuilt cheaply. The bet:
auditability, time travel, replay, and multi-tenancy all collapse into
one mechanism. Risk: storage amplification; mitigated by event log
compaction policies.

### O.6 Causal graphs and intervention queries

Most knowledge graphs encode correlation. Causal graphs encode *why*.
Pearl-style do-calculus over an RDF graph annotated with causal
direction would let users ask "if I intervene on X, what happens to Y?"
— a question current graph systems cannot answer. The bet: pg_ripple
becomes the first triple store usable for counterfactual reasoning in
medicine, economics, and policy modelling. Risk: niche audience, hard
semantics; pilot with a single domain (e.g. epidemiological models)
before generalising.

### O.7 Embedding-native queries

Today vector similarity is a SPARQL function. A bolder design treats
embeddings as a first-class join key: `?x ~~> ?y WITH similarity > 0.8`
becomes part of the algebra, the planner can reorder vector and
symbolic joins together, and the optimizer can push down nearest-
neighbour predicates the way it pushes down filters. The bet: hybrid
retrieval becomes 10× more expressive than RRF post-fusion. Risk: the
SPARQL standard has no place for this; pg_ripple may end up shipping a
non-standard dialect.

### O.8 Knowledge marketplaces

A federation directory plus a shared identifier resolution service plus
a payments primitive (or grant tracking) could turn pg_ripple into the
backbone of a distributed knowledge marketplace: data providers publish
named graphs, consumers subscribe, and queries cross provider
boundaries with provenance and licensing intact. The bet: the open
data community has wanted this for two decades; the missing pieces have
always been operational, not technical, and pg_ripple's PostgreSQL
substrate is unusually well-suited to providing them.

### O.9 Graph-native testing primitives

Property-based testing (`proptest`) is mature for code; graph-based
testing of *applications* is not. A pg_ripple test harness could let
application authors write `assert_graph_property!` macros that generate
random conforming graphs (via SHACL-driven synthesis), run application
queries against them, and check application invariants. The bet:
application correctness against graph data becomes testable the same
way pure functions are testable today. Risk: niche audience initially;
high adoption potential if it lands inside common test frameworks.

### O.10 Ambient knowledge protocol

A small daemon that watches a user's filesystem, browser history, IDE,
and chat logs (with explicit opt-in and local processing), extracts
facts via LLM, validates them with SHACL, and stores them in a personal
pg_ripple instance. The bet: a personal knowledge graph that grows
without manual curation becomes the substrate for future AI assistants.
Risk: privacy, scope creep, ethical concerns. Treat as a research probe
exclusively, never as a product, and make every fact deletable in one
operation.

### O.11 Graph-shaped LLM context windows

Modern LLMs accept structured context — JSON, XML, YAML. A *graph* is
a structured context too, and may be a more efficient representation
when the prompt would otherwise repeat the same entity many times.
pg_ripple could provide context formats designed for token efficiency:
a topological serialization where each entity appears once with a
reference number, a SHACL-shape-conforming JSON-LD that mirrors the
prompt template's expected variables, or a Datalog-style fact list with
implicit closure. The bet: the same prompt size carries 3–5× more
information, materially improving RAG quality.

### O.12 Personal sovereign knowledge graphs

Solid Pods, Bluesky AT Protocol, and Web 3 personal-data projects all
share the same problem: where do users actually store their data? A
pg_ripple "single-user" profile — a Raspberry-Pi-class deployment with
graceful sync, automatic backup, and a Studio that runs entirely in the
browser — could be the answer. The bet: as enterprise SaaS becomes
untrustworthy, individuals and small organisations will want sovereign
knowledge graphs; pg_ripple's PostgreSQL roots make it operationally
plausible.

### O.13 SPARQL over WebAssembly

A WASM build of the pg_ripple SPARQL engine (without storage) could let
SPARQL run inside the browser, in CDN edge workers, and inside other
applications, federating back to a remote pg_ripple for the heavy data.
The bet: SPARQL becomes a portable query language the way SQLite is
a portable storage engine. Risk: the engine is tightly coupled to PG
SPI; would require extracting a pure-Rust core that does not exist
today. Still a useful forcing function.

### O.14 Graph diff and patch as a first-class operation

Two graphs can be `diff`-ed: triples added, removed, restructured. A
patch can be applied. A merge can be three-way. pg_ripple already has
the storage to do this efficiently. Exposing it as `pg_ripple.diff()`,
`pg_ripple.patch()`, and `pg_ripple.merge()` would unlock workflows that
currently require external tools: ontology version control, dataset
release management, configuration drift detection. The bet: knowledge
graphs become as version-controlled as code.

### O.15 Bidirectional knowledge graphs

Most graphs are one-way: facts flow in, queries pull facts out. A
bidirectional graph also pushes derived obligations: when a fact is
added that violates a SHACL shape, the graph proposes the fix; when a
rule changes, the graph reports which downstream applications need to
re-train; when a query result changes, the graph notifies the
application that depended on it. The bet: knowledge graphs become
*active participants* in applications, not passive storage. Risk: the
abstraction is unfamiliar; pilot with one or two SDK callbacks first.

### O.16 Negative knowledge

RDF cannot natively represent "X is *not* a Y". Negation-as-failure
helps in queries but does not let users assert closed-world facts. A
typed *negative triple* — first-class, validated, separately indexed,
queryable as `NOT EXISTS` push-down — would let users represent dis-
agreement, refutation, and explicit absence. The bet: graphs become
honest about what they do not know. Risk: standards friction; this
would be a pg_ripple-specific extension unless it can ride on RDF 1.2.

### O.17 Federated learning over graphs

Multiple pg_ripple instances training a shared embedding model without
exchanging raw triples — using federated averaging or secure
aggregation — would let consortia (hospitals, banks, manufacturers)
benefit from collective intelligence without sharing data. The bet:
the vertical accelerators in §13 become much more compelling when the
domain has many participants who cannot pool data directly. Risk:
substantial cryptographic and operational complexity; partner with an
established federated-learning project rather than reinventing.

### O.18 Knowledge graphs as physical simulations

Many systems we describe as "graphs" are also dynamical systems:
biological pathways, supply chains, traffic networks, social
influence. A pg_ripple integration with a discrete-event or
agent-based simulator (Mesa, AnyLogic, NetLogo) could let users run
*what-if* simulations directly on the graph and write results back as
new named graphs annotated with simulation provenance. The bet:
explainable AI for systems-of-systems becomes a product category.

### O.19 Quantum-resistant identifier scheme

Today's IRIs and dictionary IDs assume classical hash collision
resistance. Long-lived knowledge graphs (decades) may need post-quantum
identifiers and signatures. A pluggable identifier scheme — XXH3-128
today, SPHINCS+ or similar tomorrow — would future-proof graphs whose
provenance must survive the cryptographic transitions of the next 30
years. Speculative now; cheap to design correctly while only one
identifier scheme exists.

### O.20 SPARQL for the planner

The most meta of the bets: expose pg_ripple's *own* operational state
(plans, statistics, locks, replication lag, merge backlog, audit
events) as a queryable RDF graph. Operators write SPARQL against the
running database to find anomalies, compose alerts, and build runbooks.
The bet: pg_ripple becomes the first database that uses its own data
model to manage itself. Risk: low — most of the data is already in
`pg_stat_*` views; this is a packaging exercise.

---

## 16. Vector P — Anti-directions

**Posture: Decline. Saying no is also strategy.**

Note: the anti-directions list is unchanged in spirit from earlier
drafts; renumbered as P after the insertion of Vector O.

A list of things pg_ripple *could* do but probably *should not* — and
why — is as important as the list of investments. Each of these has
been raised in roadmap discussions or community channels; documenting
the no keeps the focus.

### P.1 Becoming a standalone server

The temptation to ship a pg_ripple-only binary that does not require
PostgreSQL is real (some users find PostgreSQL operations heavy). It is
also strategic suicide: every operational advantage pg_ripple has comes
from PostgreSQL. *Decline*.

### P.2 Implementing its own query optimiser

Replacing the PostgreSQL planner with a graph-native cost-based
optimiser is a famous trap — many teams have tried, few have shipped.
The current approach (compile to SQL, lean on the PG planner, add
hints where the planner is wrong) is correct. Continue it. *Decline.*

### P.3 Owning the LLM stack end-to-end

It is tempting to ship a "pg_ripple AI Assistant" with a bundled LLM,
prompt templates, and agent orchestration. This is a separate product
in a fast-moving space. Be a *substrate* for AI applications; do not
become one. *Decline.*

### P.4 Reinventing GraphQL or REST

Ship GraphQL via a separate companion (B.3 above); do not bake a
GraphQL server into the extension. The pg_ripple_http service is
already at the edge of what an extension's companion process should
do. *Decline.*

### P.5 Becoming a triple-store benchmark

Spending engineering budget on optimising for synthetic benchmarks
(BSBM, SP²Bench, WatDiv) beyond the level needed to prove no
regressions is rarely worth it. Real workloads are different. Ship
the benchmark suite as a regression gate, do not shape the architecture
around the benchmark numbers. *Decline.*

### P.6 Replacing the dictionary encoder with a learned alternative

ML-based dictionary compression is an active research area. The
existing XXH3-128 + LRU + tiered hot table design is well-understood,
debuggable, and fast. *Decline* until a learned approach has been
proven by a third party at production scale.

### P.7 Custom storage engine outside PostgreSQL

Bypassing the heap — writing directly to mmap'd files, using
RocksDB/LevelDB/LMDB, building a custom WAL — would accelerate some
workloads but lose every operational benefit. *Decline.*

---

## 17. Time horizons

The vectors above span very different timescales. A first-pass
sequencing might look like:

### 17.1 Next 6 months (post-v1.0)

- v1.1 ships as planned (Cypher, Jupyter, LangChain, Kafka, dbt,
  materialized SPARQL views).
- pg_ripple Studio MVP (read-only schema browser + SPARQL playground).
- PG19-tracking CI matrix.
- Public conformance dashboard.
- First solution template: RAG knowledge base or customer 360.
- Security posture template attached to each release.
- One Vector O probe: choose one of {O.1 SPARQL JIT, O.14 graph diff,
  O.20 SPARQL for the planner} as a low-risk first experiment.

### 17.2 6–18 months

- Cypher / GQL TCK conformance.
- MCP server (C.5).
- First-party Python and TypeScript SDKs.
- VS Code extension.
- Foundation governance discussion begins.
- Multi-tenant SaaS topology (D.1).
- Reference dataset library (G.4).
- Policy-as-graph MVP (K.1).
- Healthcare or financial-services accelerator (L.1/L.2).
- SQL-facing semantic projections (B.5).

### 17.3 18–36 months

- Native property-graph storage option (B.2).
- Native columnar table access method (A.2).
- Storage tiering to object storage (D.3).
- Full Studio (visual query builder, inference inspector, data quality
  dashboard).
- Probabilistic / weighted reasoning (E.2).
- Hosted free-tier playground.
- LDBC participation.
- Graph-aware statistics and cardinality feedback (A.6).
- Edge-sync profile (D.5).
- Semantic lakehouse export contracts (M.1).
- Governance evidence packs (K.6).
- Two further Vector O probes graduated to spec work (O.7 embedding-
  native joins and O.5 event-sourced graphs are obvious candidates).

### 17.4 36+ months

- OWL 2 DL profile (E.1).
- Geographic federation (D.4).
- pg_ripple as a contrib extension to PostgreSQL itself.
- A standardised name and brand reflecting the broader product.
- 100B-triple production deployments as published case studies.
- Confidential-computing deployment profile (K.5).
- Full native property-graph + RDF dual-storage model.
- Cross-region partial-replication marketplace for public datasets.
- Whichever Vector O bets graduated through the funnel: the goal is
  one or two genuinely novel capabilities that no comparable system
  has shipped, not all twenty.

---

## 18. Summary scorecard

The fifteen substantive vectors compared on three axes (1 = low, 5 = high);
Vector P (anti-directions) is shown for completeness:

| Vector | Strategic value | Engineering cost | Risk | Posture |
|---|---|---|---|---|
| A — Deep PG integration | 5 | 3 | 2 | **Invest** |
| B — Graph protocol pluralism | 4 | 4 | 3 | **Invest selectively** |
| C — AI/LLM-native knowledge layer | 5 | 4 | 3 | **Invest hard** |
| D — Distributed scale | 4 | 5 | 4 | **Invest, bound ambition** |
| E — Reasoning frontier | 3 | 4 | 3 | **Opportunistic** |
| F — Developer experience & UI | 5 | 5 | 2 | **Invest** |
| G — Standards leadership | 3 | 2 | 1 | **Opportunistic** |
| H — Operational excellence | 4 | 3 | 1 | **Invest** |
| I — Sustainability | 5 | 3 | 2 | **Required** |
| J — Adjacent absorption | 3 | 4 | 4 | **Watch, selective** |
| K — Trust, privacy, and governance | 5 | 4 | 2 | **Invest** |
| L — Domain solution accelerators | 4 | 3 | 2 | **Invest selectively** |
| M — Semantic lakehouse and analytics | 3 | 4 | 3 | **Watch, then invest** |
| O — Novel research bets | 4 | 4 | 5 | **Selective experimentation** |
| P — Anti-directions | n/a | n/a | n/a | **Decline** |

The recurring pattern: the highest strategic-value, lowest-risk
investments are deep PostgreSQL integration (A), AI/LLM substrate (C),
developer experience and UI (F), operational excellence (H),
sustainability (I), and trust/governance (K). These six form the spine
of any plausible post-v1.0 strategy. Everything else is opportunistic,
optional, domain-packaged, or deliberately deferred.

---

## 19. A concluding observation

pg_ripple's most underrated property is that it has *already done the
hard work*. The dictionary, the VP storage, the SPARQL compiler, the
Datalog reasoner, the SHACL validator, the HTAP merge worker, the
Citus integration, the conformance suites — these are all present,
working, and tested. The unglamorous reality of post-v1.0 is that the
next 5× of impact will not come from another reasoning algorithm or
another standard — it will come from making everything that is
*already there* easier to discover, install, operate, learn, integrate,
and trust.

That framing collapses the fifteen vectors above into a single sentence:
*the next era of pg_ripple is about the surface area, not the engine*.

The engine is good. Now make it easy.
