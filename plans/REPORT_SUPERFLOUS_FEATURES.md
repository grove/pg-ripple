# Slimming Down for v1.0 — What Can We Drop?

> **What is this?** pg_ripple has grown a lot of features over 78 releases.
> Some of those features are half-finished, rarely used, or duplicated by
> other parts of the system. This document is a prioritized list of things we
> could remove to make the v1.0 release smaller, simpler, and more honest
> about what it actually does well.
>
> **Nothing here is decided yet.** Every item is a *candidate* for discussion.

---

## The big picture

We found roughly **20 000 lines of code** and **100 documentation files** that
are candidates for removal or relocation. That's a meaningful slice of the
project. The items fall into six priority tiers, from "delete today with zero
risk" through "think about it after v1.0."

| Priority | What it is | How much weight |
|---|---|---|
| **Tier 0** | Leftover build logs and scratch files | Trivial (just delete) |
| **Tier 1** | Features that are advertised but not actually working | ~3 500 lines |
| **Tier 2** | Big experimental features that depend on external tools | ~7 000 lines |
| **Tier 3** | Small niche features with little proven demand | ~1 500 lines |
| **Tier 4** | Outdated planning documents, blog posts, and examples | ~100 files |
| **Tier 5** | Side-projects bundled in the same repo (Helm chart, dbt, etc.) | Several components |
| **Tier 6** | Housekeeping and reorganisation | Small but worthwhile |

---

## Tier 0 — Leftover scratch files (safe to delete immediately)

During development, some temporary files were accidentally saved into the
repository. They serve no purpose for users or contributors.

| File | What it is | Why drop it |
|---|---|---|
| `build_output.txt` | A saved copy of compiler output from April 21 | Stale; CI produces this fresh on every run |
| `cargo_check_output.txt` | Another saved compiler log | Same reason |
| `clippy_all.txt` / `clippy_output.txt` | Linting results (one is empty) | Regenerated on demand |
| `check_test_output.sh` | A tiny throwaway script | Not used by any workflow |
| `sbom.json` / `sbom_diff.md` | Software bill-of-materials snapshot | Should be generated automatically in CI, not stored |
| `DEEP_ANALYSIS_PROMPT.md` | An internal working document | Not relevant to users or contributors |

**What to do:** Delete them all and add the filenames to `.gitignore` so they
don't come back.

---

## Tier 1 — Features that look real but aren't finished

These are the most important candidates because they appear in the product's
feature list, but when you look under the hood, they either do nothing or do
far less than advertised. Shipping them in v1.0 would mislead users.

### 1.1 SPARQL-DL (querying OWL class hierarchies directly)

**What it is:** A way to ask questions directly about the "vocabulary" of your
knowledge graph — things like "which classes are subclasses of Animal?" —
without having to store those relationships as regular data triples.

**What value it would bring:** Faster and more natural queries over ontology
structure for users who work with complex OWL vocabularies.

**Why consider dropping it:** The code is entirely marked as unused. It was
started in v0.58.0 but never wired into the actual query engine. If you call
it, nothing happens. Rather than ship dead code, we should remove it and
honestly say "not supported in v1.0."

---

### 1.2 Worst-Case Optimal Joins (the "Leapfrog" algorithm)

**What it is:** A cutting-edge algorithm for queries that have cycles — for
example, "find all triangles where A knows B, B knows C, and C knows A." The
traditional approach produces huge intermediate results; this algorithm avoids
that by intersecting sorted data streams.

**What value it would bring:** Dramatic speedups (10–100×) on cyclic graph
patterns, which are common in social-network and fraud-detection queries.

**Why consider dropping it:** We only implemented the easy part — reordering
which tables are joined first. The actual leapfrog execution engine was never
built. Our release notes claim a big speedup that isn't really delivered. We
should either rename this to what it actually is (a join-order hint) or merge
the small useful piece into the existing optimizer module and drop the
misleading name.

---

### 1.3 SHACL SPARQL Rules (deriving new data from SHACL shapes)

**What it is:** SHACL is our data-quality system — it lets you define what
"valid" data looks like. SHACL *rules* go a step further: they can
automatically generate new data when certain patterns are found. Think of it
like database triggers, but for knowledge graphs.

**What value it would bring:** Users could express complex derivation logic
using the same SHACL language they already use for validation, without
learning Datalog.

**Why consider dropping it:** The system accepts and stores these rules, but
never actually runs them. The execution engine hasn't been built. This is
confusing for users who define rules and then wonder why nothing happens.

---

### 1.4 SPARQL 1.2 support

**What it is:** The next version of the SPARQL query language standard,
currently being drafted by the W3C working group.

**What value it would bring:** Future-proofing — when the standard is
finalized, we'd already support it.

**Why consider dropping it:** The upstream library we depend on hasn't shipped
SPARQL 1.2 grammar support yet, so our "support" is just a configuration
flag that does nothing. We're paying a small maintenance cost for zero
functionality. We can easily re-add this once the standard is actually ready.

---

### 1.5 Unused code in the Datalog engine

**What it is:** The Datalog module (our rule-based reasoning engine) contains
over 20 places where code is explicitly marked as "not currently used."

**What value it would bring:** Some of it may be forward-looking scaffolding
for future features.

**Why consider dropping it:** Much of it appears to be leftover from
refactoring — functions that were replaced by newer versions but never
cleaned up. Every unused function is a maintenance burden and a source of
confusion. We should audit each one: if it's needed, wire it up; otherwise,
delete it.

---

## Tier 2 — Big experimental features (largest weight savings)

These are substantial features — each hundreds or thousands of lines of code
— that were added in recent development cycles. They're all marked
"experimental" and most require additional software to be installed alongside
pg_ripple. They represent the largest opportunity to slim down the extension.

### 2.1 Bidirectional sync (two-way data exchange between systems)

**What it is:** A comprehensive framework for synchronizing data between
pg_ripple and external systems. It handles conflicts (what happens when both
sides change the same data?), deduplication, upserts, deletes, and
back-references. Think of it like two-way Google Drive sync, but for
knowledge-graph data.

**What value it would bring:** Enterprise deployments where pg_ripple needs
to both consume data from and publish data to other systems in real time, with
no data loss or duplication.

**Why consider dropping it:** At 2 500 lines, this is the single largest
source file in the entire project. It implements 15 separate sub-features, many
of which depend on our sibling project pg-trickle. It's a complex, hard-to-test
subsystem that hasn't been battle-tested in production. The core pg_ripple
value proposition (store RDF, run SPARQL) doesn't require it. It could live as
a separate add-on module shipped after v1.0.

---

### 2.2 Knowledge-Graph Embeddings (machine-learning on graph structure)

**What it is:** A way to train mathematical vector representations ("embeddings")
of entities in the knowledge graph using algorithms called TransE and RotatE.
These embeddings can then be used to find entities that are structurally
similar — for example, finding companies that look like other companies based
on their relationships.

**What value it would bring:** Entity alignment (matching the same entity
across different datasets), link prediction (guessing missing relationships),
and similarity search.

**Why consider dropping it:** The entire module is marked as unused code.
Training machine-learning models inside a database extension is unusual —
users who need this capability typically run it in Python with PyTorch or
similar tools where they have GPU access and better tooling. The simpler
"store and search pre-computed embeddings" feature (which we'd keep) covers
the common use case.

---

### 2.3 LLM / AI integration (natural-language to SPARQL)

**What it is:** Three AI-powered features: (a) translating plain English
questions into SPARQL queries, (b) automatically fixing broken SPARQL queries
by asking an LLM to repair them, and (c) using embeddings to suggest that
two entities in different datasets might be the same thing.

**What value it would bring:** Makes the system accessible to non-technical
users who can't write SPARQL, and provides intelligent data-matching
capabilities.

**Why consider dropping it:** This is essentially an HTTP client that talks
to external AI services (like OpenAI). Putting that inside a database
extension means we have to handle TLS certificates, rate limiting, prompt
injection attacks, API key management, and model changes — all inside the
database process. This is better handled by the HTTP companion service
(`pg_ripple_http`) or by application code outside the database. When no AI
endpoint is configured, all three features silently do nothing.

---

### 2.4 Citus distributed database support

**What it is:** Integration with Citus, a PostgreSQL extension that
distributes tables across multiple servers for horizontal scaling. Our Citus
support would shard the triple store across many machines.

**What value it would bring:** The ability to handle very large knowledge
graphs (billions of triples) by spreading the work across a cluster of
database servers.

**Why consider dropping it:** Of the five Citus features we've implemented,
only one is fully working. The other four are marked "experimental" with
notes saying they've never been tested on an actual multi-server cluster.
We're shipping 1 300 lines of code that was only ever tested with mocked-up
single-server scenarios. The recommendation is to keep the one working piece
and move the rest to a separate companion module that's loaded only when
Citus is detected.

---

### 2.5 Arrow Flight bulk export (high-speed data streaming)

**What it is:** Apache Arrow Flight is a high-performance protocol for
streaming large datasets. Our implementation lets analytics tools pull
millions of triples out of pg_ripple very quickly in a columnar format.

**What value it would bring:** Fast data export for data science and
analytics workloads — think "dump everything into a Jupyter notebook."

**Why consider dropping it:** It requires cryptographic ticket signing,
secret management, and a dedicated streaming encoder. Much of this
overlaps with work already done in our PG18 logical-decoding bridge and with
the GraphRAG Parquet export. It adds complexity without a clear user asking
for it specifically. We could defer it to v1.1.

---

### 2.6 RDF logical replication (keeping replicas in sync)

**What it is:** A background worker that subscribes to PostgreSQL's built-in
change stream and replays triple inserts/deletes onto a read-only replica of
the knowledge graph.

**What value it would bring:** High availability — if the primary database
goes down, a replica has an up-to-date copy of the knowledge graph.

**Why consider dropping it:** We actually have *two* replication approaches
in the codebase. This older one (`replication.rs`) and a newer one from
v0.54.0 that uses PG18's native logical decoding. Maintaining two parallel
replication systems doubles the testing and support burden. We should pick
one and drop the other.

---

### 2.7 Live SPARQL subscriptions (push notifications for query results)

**What it is:** Register a SPARQL query and get notified every time its
results change. The notification is delivered via PostgreSQL's built-in
NOTIFY/LISTEN mechanism.

**What value it would bring:** Real-time dashboards that update automatically
when the underlying data changes, without polling.

**Why consider dropping it:** The implementation hits PostgreSQL's 8 KB
notification payload limit, so for any non-trivial query result, it just sends
`{"changed": true}` and the client has to re-query anyway. It's a neat
concept that doesn't scale to production use. The HTTP companion service has
Server-Sent Events (SSE) which is a more robust delivery mechanism — keep it
there only if anywhere.

---

### 2.8 CDC bridge (change-data-capture outbox)

**What it is:** A bridge that captures every write to the knowledge graph and
publishes it as a structured event to pg-trickle's outbox system, suitable
for downstream consumers like Kafka, Debezium, or webhook endpoints.

**What value it would bring:** Event-driven architectures where other systems
need to react to knowledge-graph changes in near real-time.

**Why consider dropping it:** It overlaps heavily with both the bidirectional
sync (Tier 2.1, which has its own outbox) and the replication worker (Tier
2.6). We have three different systems for "tell the outside world about
changes." We should pick one and commit to it.

---

### 2.9 JSON Mapping (named JSON-LD context registry)

**What it is:** A way to register a named mapping between JSON fields and RDF
properties, so you can import plain JSON documents and export graph data as
familiar JSON structures using a single registered template.

**What value it would bring:** Simplifies integration with REST APIs that
speak JSON — you define the mapping once and use it in both directions.

**Why consider dropping it:** The functionality largely overlaps with our
existing JSON-LD framing engine (which already translates between flat RDF
and nested JSON) and with the JSON-LD ingest path. Having two slightly
different approaches to "JSON ↔ RDF" confuses users. We should fold the
useful bits into the framing module and remove the separate registry.

---

## Tier 3 — Small niche features with little proven demand

These are small, real features — each implements a recognized standard or
pattern — but they serve narrow use cases and overlap with other capabilities
that already exist in pg_ripple.

### 3.1 R2RML (mapping relational tables to RDF)

**What it is:** A W3C standard for defining how rows in a relational SQL
table should be transformed into RDF triples. You write a mapping document
that says "column X becomes predicate Y" and the system generates triples.

**Value:** Useful for organizations that want to expose their existing SQL
databases as a knowledge graph without moving data.

**Why consider dropping:** Only about 290 lines and the more advanced "virtual
mapping" plan was never implemented. Most users who want this workflow use
pg-trickle or JSON mapping instead, which are more flexible.

---

### 3.2 Provenance tracking (PROV-O)

**What it is:** Automatically records *who* loaded *what* data and *when*,
using the W3C PROV-O vocabulary. Every bulk-load operation generates a small
audit trail describing the activity, the person responsible, and what was
produced.

**Value:** Compliance and auditing — "show me the lineage of this data."

**Why consider dropping:** The feature defaults to "off" and adds overhead to
every load. The same information can be recorded by application code or by a
cookbook recipe without building it into the extension core.

---

### 3.3 Temporal queries (time-travel)

**What it is:** The ability to ask "what did the knowledge graph look like at
3pm yesterday?" by setting a point-in-time threshold that filters out
triples that were inserted after that moment.

**Value:** Reproducibility, debugging, and regulatory compliance (some
industries require the ability to reconstruct historical states).

**Why consider dropping:** The implementation is fragile — it relies on
internal statement IDs which can shift when data is reorganized during
maintenance operations. It's only 194 lines, but it promises more than it
can reliably deliver.

---

### 3.4 Multi-tenancy helpers

**What it is:** A convenience wrapper that creates a PostgreSQL role, assigns
it to a named graph, and optionally enforces a quota on how many triples that
tenant can store.

**Value:** SaaS-style deployments where multiple customers share one database
but each can only see their own data.

**Why consider dropping:** It's just a thin wrapper around `grant_graph_access()`
which already exists and works. Users can achieve the same result with two SQL
calls. The wrapper adds 220 lines of code for minimal convenience.

---

### 3.5 OWL 2 QL query rewriting

**What it is:** The OWL standard defines several "profiles" (subsets of the
language). Our main reasoning engine implements OWL 2 RL (Rule Language).
This module adds a second profile — OWL 2 QL (Query Language) — which works
by rewriting SPARQL queries rather than materializing inferences.

**Value:** Faster queries for users who have OWL 2 QL ontologies, without the
storage overhead of pre-computing all inferences.

**Why consider dropping:** We're positioning pg_ripple around OWL 2 RL
(Datalog-based reasoning). Supporting a second OWL profile adds cognitive
load for users and testing burden for us. Very few users have asked for
OWL 2 QL specifically.

---

### 3.6 Embedding client inside the SPARQL module

**What it is:** An HTTP client that calls OpenAI-compatible embedding APIs,
bundled inside the query engine module.

**Value:** Powers the `pg:similar()` function in SPARQL queries for
vector-similarity search.

**Why consider dropping:** An HTTP client for calling external AI services
doesn't belong inside the query-translation layer. It should live in the HTTP
companion service or be a standalone utility. This is a code-organization
issue more than a feature-removal issue.

---

## Tier 4 — Outdated documents, blog posts, and examples

Over 78 releases, we've accumulated a lot of written material. Some of it
describes features that were never built, promotes capabilities that are
misleading, or duplicates content in other locations.

### 4.1 Old assessment documents

We have **eleven** numbered "overall assessment" files from past code-quality
reviews. Only the latest one (#11) is still relevant. The older ten are
historical records that clutter the `plans/` directory.

**What to do:** Archive #1–#10 into a subfolder or delete them.

### 4.2 Plans for features we're not building

Several planning documents describe features that are *not* on the roadmap:

- **Cypher/GQL query language** — We have three documents exploring Cypher
  support, but it's not planned. This is noise.
- **Storage tiering (SlateDB/DuckDB)** — Speculative architecture research
  that won't happen for v1.0.
- **Link prediction, neuro-symbolic record linkage** — Research topics, not
  product plans.
- **Competitive landscape notes** — Great content, but it belongs in research
  docs, not in `plans/`.

**What to do:** Move to a `docs/research/` folder or consolidate into a
single "future directions" document.

### 4.3 Blog posts that describe unimplemented features

Several blog posts market features that don't work as described:

- The **Leapfrog Triejoin** post describes a full custom executor that was
  never built (only the join-reorder hint exists).
- The **Probabilistic Datalog** post describes a feature that doesn't exist
  in the code at all.
- Posts about **Citus shard pruning**, **temporal queries**, **R2RML**, and
  **neuro-symbolic entity resolution** all depend on experimental or niche
  modules we're considering removing.

**What to do:** If we remove a feature, we must also remove or update its
blog post. A post promoting non-existent functionality is worse than no post.

### 4.4 Examples and documentation pages

Seventeen example SQL scripts and 153 documentation pages exist. Several are
tightly coupled to the features above. When a feature is removed, its
examples and docs pages should go with it.

---

## Tier 5 — Side-projects that could live in their own repositories

These are separate tools that happen to be bundled in the same Git repository.
Each one has its own release cadence and its own users. Bundling them with
the core extension means every release of pg_ripple implicitly "releases"
all of them too, even if they haven't changed.

### 5.1 dbt adapter (`clients/dbt-pg-ripple/`)

**What it is:** An adapter for the dbt (data build tool) ecosystem that lets
dbt manage SPARQL views and transformations.

**Why consider separating:** It's tiny (one file, one test) with no evidence
of active users. It would be better served as its own repository with its own
versioning.

### 5.2 Helm chart (`charts/pg_ripple/`)

**What it is:** A Kubernetes deployment template for running pg_ripple in
cloud-native environments.

**Why consider separating:** Helm charts evolve on a different cadence from
the database extension. Keeping them together means chart updates require
tagging a new extension release.

### 5.3 CloudNativePG Docker image

**What it is:** A specialized container image for the CloudNativePG Kubernetes
operator (used for running PostgreSQL on OpenShift/K8s).

**Why consider separating:** We already have a standard Dockerfile. Two
container images doubles the build and testing surface for each release.

### 5.4 GraphRAG vocabulary files in the wrong directory

**What it is:** Ontology and shape files for the GraphRAG export feature,
currently placed in the `sql/` directory.

**Why consider moving:** PostgreSQL's extension installer copies *everything*
in `sql/` when the extension is installed. These vocabulary files aren't SQL
and shouldn't be installed alongside migration scripts.

---

## Tier 6 — Housekeeping

| What | Why it matters |
|---|---|
| Old test results still tracked in Git | Adds noise to the repo history |
| 128 configuration knobs | Many are for experimental features; hard for users to know which ones matter |
| 81 SQL migration files | Users upgrading from v0.1 must walk through 81 steps; we should offer a direct path |
| 12.5 MB of third-party test data in the repo | Makes cloning slow; could be downloaded on demand in CI |
| Developer-facing docs at the top level | `AGENTS.md`, `RELEASE.md`, etc. are for contributors, not users; could live under `.github/` |
| Duplicate file patterns (e.g., `views.rs` + `views_api.rs`) | Eight modules have this split where one file is just a thin wrapper around the other |

---

## Recommended order of work

1. **Immediate (zero risk):** Delete scratch files (Tier 0) and do
   housekeeping (Tier 6). This has no effect on functionality.

2. **Before v1.0 (no user-facing API change):** Remove dead code and
   unfinished stubs (Tier 1). This makes the feature list honest.

3. **Deprecation period (announce in v0.79):** Mark the smaller Tier 2
   features as deprecated (Arrow Flight, old replication, subscriptions, CDC
   bridge, JSON mapping). Remove them in v1.0.0.

4. **At the v1.0.0 release:** Move the large Tier 2 features (bidirectional
   sync, KGE, LLM, Citus) into separate companion packages. They can evolve
   on their own schedule and ship for v1.1 when ready.

5. **After v1.0:** Evaluate Tier 3 modules based on user feedback. Extract
   side-projects (Tier 5) into their own repos. Clean up documentation and
   blog posts (Tier 4).

---

## Things to keep in mind

**Removing features has trade-offs:**

- **Marketing vs. reality.** Some of these features exist because they tell a
  compelling story ("we have AI integration!" or "we scale to billions of
  triples!"). Removing them makes the product sound smaller, even if it makes
  it more solid. That's a legitimate tension to weigh.

- **We can always bring things back.** Code deleted from v1.0 lives on in Git
  history and can be restored. But database tables that are dropped require a
  migration to re-add, so we need to plan the removal carefully.

- **Companion modules vs. deletion.** For big features like Citus and
  bidirectional sync, moving them to a separate package is better than
  deleting them. Users who need them can install the add-on; users who don't
  get a smaller, simpler core.

- **The HTTP companion is tightly coupled.** Several experimental features
  have web endpoints in the HTTP service. When we remove a feature from the
  core, we must also remove its HTTP routes at the same time.

---

## Suggested first step

A single pull request that is completely safe and immediately beneficial:

1. Delete the eight scratch files (Tier 0)
2. Remove the dead SPARQL-DL module (Tier 1.1)
3. Archive old planning documents (Tier 4.1)
4. Move demo SQL scripts out of the extension install directory (Tier 5.4/5.5)

This gives us a smaller, cleaner project with no behaviour change and no risk
of breaking anything.

---

*This document is a discussion starter, not a decision. Each item needs an
explicit go/no-go before anyone starts deleting code.*
