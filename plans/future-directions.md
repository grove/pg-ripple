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

The directions are grouped into eleven thematic vectors. Each vector is
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

---

## 12. Vector K — Anti-directions

**Posture: Decline. Saying no is also strategy.**

A list of things pg_ripple *could* do but probably *should not* — and
why — is as important as the list of investments. Each of these has
been raised in roadmap discussions or community channels; documenting
the no keeps the focus.

### K.1 Becoming a standalone server

The temptation to ship a pg_ripple-only binary that does not require
PostgreSQL is real (some users find PostgreSQL operations heavy). It is
also strategic suicide: every operational advantage pg_ripple has comes
from PostgreSQL. *Decline*.

### K.2 Implementing its own query optimiser

Replacing the PostgreSQL planner with a graph-native cost-based
optimiser is a famous trap — many teams have tried, few have shipped.
The current approach (compile to SQL, lean on the PG planner, add
hints where the planner is wrong) is correct. Continue it. *Decline.*

### K.3 Owning the LLM stack end-to-end

It is tempting to ship a "pg_ripple AI Assistant" with a bundled LLM,
prompt templates, and agent orchestration. This is a separate product
in a fast-moving space. Be a *substrate* for AI applications; do not
become one. *Decline.*

### K.4 Reinventing GraphQL or REST

Ship GraphQL via a separate companion (J.3 above); do not bake a
GraphQL server into the extension. The pg_ripple_http service is
already at the edge of what an extension's companion process should
do. *Decline.*

### K.5 Becoming a triple-store benchmark

Spending engineering budget on optimising for synthetic benchmarks
(BSBM, SP²Bench, WatDiv) beyond the level needed to prove no
regressions is rarely worth it. Real workloads are different. Ship
the benchmark suite as a regression gate, do not shape the architecture
around the benchmark numbers. *Decline.*

### K.6 Replacing the dictionary encoder with a learned alternative

ML-based dictionary compression is an active research area. The
existing XXH3-128 + LRU + tiered hot table design is well-understood,
debuggable, and fast. *Decline* until a learned approach has been
proven by a third party at production scale.

### K.7 Custom storage engine outside PostgreSQL

Bypassing the heap — writing directly to mmap'd files, using
RocksDB/LevelDB/LMDB, building a custom WAL — would accelerate some
workloads but lose every operational benefit. *Decline.*

---

## 13. Time horizons

The vectors above span very different timescales. A first-pass
sequencing might look like:

### 13.1 Next 6 months (post-v1.0)

- v1.1 ships as planned (Cypher, Jupyter, LangChain, Kafka, dbt,
  materialized SPARQL views).
- pg_ripple Studio MVP (read-only schema browser + SPARQL playground).
- PG19-tracking CI matrix.
- Public conformance dashboard.

### 13.2 6–18 months

- Cypher / GQL TCK conformance.
- MCP server (C.5).
- First-party Python and TypeScript SDKs.
- VS Code extension.
- Foundation governance discussion begins.
- Multi-tenant SaaS topology (D.1).
- Reference dataset library (G.4).

### 13.3 18–36 months

- Native property-graph storage option (B.2).
- Native columnar table access method (A.2).
- Storage tiering to object storage (D.3).
- Full Studio (visual query builder, inference inspector, data quality
  dashboard).
- Probabilistic / weighted reasoning (E.2).
- Hosted free-tier playground.
- LDBC participation.

### 13.4 36+ months

- OWL 2 DL profile (E.1).
- Geographic federation (D.4).
- pg_ripple as a contrib extension to PostgreSQL itself.
- A standardised name and brand reflecting the broader product.
- 100B-triple production deployments as published case studies.

---

## 14. Summary scorecard

The eleven vectors compared on three axes (1 = low, 5 = high):

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
| K — Anti-directions | n/a | n/a | n/a | **Decline** |

The recurring pattern: the highest strategic-value, lowest-risk
investments are deep PostgreSQL integration (A), AI/LLM substrate (C),
developer experience and UI (F), operational excellence (H), and
sustainability (I). These five form the spine of any plausible
post-v1.0 strategy. Everything else is opportunistic, optional, or
deliberately deferred.

---

## 15. A concluding observation

pg_ripple's most underrated property is that it has *already done the
hard work*. The dictionary, the VP storage, the SPARQL compiler, the
Datalog reasoner, the SHACL validator, the HTAP merge worker, the
Citus integration, the conformance suites — these are all present,
working, and tested. The unglamorous reality of post-v1.0 is that the
next 5× of impact will not come from another reasoning algorithm or
another standard — it will come from making everything that is
*already there* easier to discover, install, operate, learn, integrate,
and trust.

That framing collapses the eleven vectors above into a single sentence:
*the next era of pg_ripple is about the surface area, not the engine*.

The engine is good. Now make it easy.
