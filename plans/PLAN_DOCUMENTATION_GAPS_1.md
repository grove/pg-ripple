# pg_ripple Documentation Gap Analysis (Report #1)

> **Date:** 2026-04-27
> **Scope:** mdBook site under `docs/src/` (rendered at the project documentation site)
> **Audience:** pg_ripple maintainers, technical writers, product owners
> **Status:** Strategic review — recommends a moderate-to-major restructure
> **Companion files:** `docs/src/SUMMARY.md`, `README.md`, `ROADMAP.md`, `examples/`

---

## 1. Executive Summary

After 59 releases pg_ripple ships an enormous feature surface — full SPARQL 1.1, SHACL Core, OWL 2 RL, Datalog (with magic sets, lattices, well-founded semantics, DRed, WCOJ), HTAP storage, a vector + RAG pipeline, NL→SPARQL, knowledge-graph embeddings, GraphRAG export, federation, CDC subscriptions, Citus sharding, temporal queries, PROV-O, R2RML, multi-tenant graphs, geospatial, full-text search, an HTTP service, and more.

The documentation has not kept pace. The `docs/src/` tree contains **good prose for ~30 % of features**, **reference-only stubs for another ~30 %**, and **nothing at all (or only an example .sql file) for the remaining ~40 %**. In addition, the navigation tree (`SUMMARY.md`) leaves **~40 already-written pages orphaned** — they exist on disk but are not reachable from the sidebar, so users cannot discover them.

The gap pattern is consistent: **discovery and conceptual on-ramps are weak**, **AI/LLM/record-linkage features are dramatically under-documented relative to their strategic importance** (they are the differentiators most likely to convince a new user to adopt pg_ripple), and **the information architecture mixes three competing organisations** (`features/` deep-dives, `user-guide/` how-tos, `reference/` lookups) without a clear contract between them.

**Recommendation:** a **moderate-scope restructure** (one release worth of documentation work, not a from-scratch rewrite). Keep the mdBook stack, keep the `features/` deep-dives, but: (a) collapse `user-guide/` and `reference/` into a clean two-tier structure, (b) wire up the orphaned files, (c) add **eight new top-level user guides** for the under-documented strategic capabilities (record linkage, KGE, NL→SPARQL, RAG, AI agents, temporal, multi-tenant, GraphRAG end-to-end), and (d) ship a new "Use Case Cookbook" section with end-to-end recipes that string features together.

---

## 2. Method

We reviewed:

1. The mdBook entry points: `docs/src/SUMMARY.md`, `docs/src/landing.md`, `docs/book.toml`.
2. Every directory under `docs/src/` (`evaluate/`, `getting-started/`, `features/`, `user-guide/`, `operations/`, `reference/`, `research/`).
3. The `README.md` "What works today" feature inventory (the most authoritative human-readable list of capabilities).
4. The `ROADMAP.md` to confirm released vs planned features.
5. The `examples/` directory — every `.sql` example was checked for a corresponding documentation page.
6. The `plans/neuro-symbolic-record-linkage.md` strategy document (the most detailed external statement of pg_ripple's positioning on record linkage).

For each capability we asked four questions:

- **A. Conceptual orientation** — does a new user learn *what it is* and *when to use it* before being shown SQL?
- **B. End-to-end walkthrough** — is there a complete, copy-pasteable worked example?
- **C. Reference completeness** — are all functions, GUCs, error codes, and data-model details documented?
- **D. Discoverability** — is the page reachable from `SUMMARY.md`, the landing page, and from related pages?

A capability scores **green** only when all four hold.

---

## 3. Inventory: Where every feature stands

### 3.1 Legend

| Mark | Meaning |
|---|---|
| ✅ | Conceptual + walkthrough + reference + discoverable |
| 🟡 | Documented but with one of: weak orientation, no end-to-end recipe, or orphaned from `SUMMARY.md` |
| 🟠 | Reference-only (function signatures, no narrative) **or** narrative exists but is hidden |
| 🔴 | Effectively undocumented (no user-facing page; at best an `examples/*.sql` file) |

### 3.2 Feature scoring

| # | Capability | Status | Where it lives today | Where the gap is |
|---|---|---|---|---|
| 1 | Install / first triple / first SPARQL | ✅ | `getting-started/` | — |
| 2 | Key concepts (RDF for SQL users) | ✅ | `getting-started/key-concepts.md` | — |
| 3 | Storing knowledge (triples, IRIs, blank nodes, RDF-star, named graphs, dictionary) | ✅ | `features/storing-knowledge.md` | — |
| 4 | Loading data (Turtle / N-Triples / N-Quads / TriG / RDF-XML) | ✅ | `features/loading-data.md` + `user-guide/sql-reference/bulk-load.md` | — |
| 5 | SPARQL querying (SELECT, CONSTRUCT, ASK, DESCRIBE) | ✅ | `features/querying-with-sparql.md` | — |
| 6 | SHACL validation (sync + async) | ✅ | `features/validating-data-quality.md` | — |
| 7 | Datalog rules + RDFS / OWL RL | ✅ | `features/reasoning-and-inference.md` | — |
| 8 | Export (Turtle / N-Triples / JSON-LD / RDF-XML / framing) | ✅ | `features/exporting-and-sharing.md` | — |
| 9 | EXPLAIN SPARQL | ✅ | `user-guide/explain-sparql.md` (linked) | — |
| 10 | RAG pipeline (`rag_context()`) | ✅ | `user-guide/rag-pipeline.md` (linked) | — |
| 11 | Vector + SPARQL hybrid (`hybrid_search`, `pg:similar`) | 🟡 | `features/ai-retrieval-graph-rag.md` covers most; **separate** `user-guide/hybrid-search.md` is **orphaned** | Conflicting / duplicated content; pick one |
| 12 | NL→SPARQL (`sparql_from_nl`) | 🟡 | `features/nl-to-sparql.md` | No end-to-end demo with real graph; no failure-mode discussion (PT702); no recipe for prompt engineering / few-shot tuning |
| 13 | Entity alignment (`suggest_sameas`, `apply_sameas_candidates`) | 🟡 | `features/entity-alignment.md` | Title/positioning hides the *real* use case — **record linkage / entity resolution**; no precision/recall guidance from real data; no integration with SHACL gates |
| 14 | GraphRAG integration | 🟡 | `user-guide/graphrag.md` + `user-guide/graphrag-enrichment.md` (both **orphaned**) + `reference/graphrag-functions.md` + `reference/graphrag-ontology.md` (both **orphaned**) | Four separate pages, none in `SUMMARY.md`; no end-to-end "ingest → enrich → validate → export → query in Microsoft GraphRAG" recipe; only mentioned in passing on `features/ai-retrieval-graph-rag.md` |
| 15 | CDC subscriptions | ✅ | `features/cdc-subscriptions.md` | — |
| 16 | Federation (SERVICE) | 🟡 | `user-guide/sql-reference/federation.md` (**orphaned** from `SUMMARY.md`); `features/apis-and-integration.md` covers only the basics | No tutorial; no recipe for combining Wikidata + local; no troubleshooting for federation circuit-breaker / timeouts |
| 17 | Vector federation (Weaviate / Qdrant / Pinecone / pgvector) | 🟡 | `user-guide/vector-federation.md` (**orphaned**) | Strategically important (separates pg_ripple from pure vector DBs) but not surfaced in SUMMARY |
| 18 | pg_ripple_http service | ✅ | `features/apis-and-integration.md` + `reference/http-api.md` (orphaned) | OK on conceptual side, but the API reference is orphaned; OpenAPI spec mentioned but not linked |
| 19 | Materialized SPARQL/Datalog views (with pg-trickle) | 🟠 | `user-guide/sql-reference/views.md` (**orphaned**) | No conceptual page in `features/`; users will not discover this is even possible |
| 20 | Full-text search | 🟠 | `user-guide/sql-reference/fts.md` (**orphaned**) | No `features/` deep dive; mentioned once in landing page |
| 21 | Geospatial / GeoSPARQL | 🟠 | `user-guide/geospatial.md` (**orphaned**) + `reference/geosparql.md` (**orphaned**) | Two pages, neither linked from `SUMMARY.md`; no link from `features/` |
| 22 | Streaming cursors | 🟠 | `user-guide/sql-reference/cursor-api.md` (**orphaned**) | Only place this critical large-result feature is documented |
| 23 | Knowledge Graph Embeddings (`kge_enabled`, TransE, RotatE, `find_alignments`) | 🔴 | **Not documented anywhere in `docs/src/`** | Mentioned in README; no user-facing page at all |
| 24 | Temporal RDF (`point_in_time`) | 🔴 | **Not documented anywhere** | One-line README mention; no recipe, no FAQ, no caveats |
| 25 | PROV-O provenance (`prov_enabled`, `prov_stats`) | 🔴 | **Not documented anywhere** | One-line README mention |
| 26 | Multi-tenant graphs (`create_tenant`, `tenant_stats`) | 🔴 | Mentioned only in `operations/security.md` example | No conceptual page; no quota/eviction guide |
| 27 | SPARQL audit log (`audit_log_enabled`, `purge_audit_log`) | 🔴 | **Not documented anywhere** | Compliance feature, hidden |
| 28 | R2RML direct mapping (`r2rml_load`) | 🔴 | **Not documented anywhere** | Major interop story (relational → RDF), invisible |
| 29 | OWL 2 EL / QL profiles | 🔴 | **Not documented anywhere** | Only the OWL RL profile has a page |
| 30 | SPARQL-DL (`sparql_dl_subclasses`, `sparql_dl_superclasses`) | 🔴 | **Not documented anywhere** | |
| 31 | SHACL-SPARQL rules (`sh:SPARQLRule`, `sh:SPARQLConstraint`) | 🔴 | Mentioned in v0.53 release notes | No user page |
| 32 | Citus sharding (separate from operations/citus-integration) | 🟠 | `operations/citus-sharding.md` (**orphaned**) | Operations page lacks a tutorial; no SPARQL-author-facing page on what shard pruning means for queries |
| 33 | Probabilistic rules | 🔴 | `examples/probabilistic_rules.sql` only | No documentation page |
| 34 | SPARQL repair | 🔴 | `examples/sparql_repair.sql` only | No documentation page |
| 35 | Ontology mapping | 🔴 | `examples/ontology_mapping.sql` only | No documentation page |
| 36 | LLM workflow (compound) | 🔴 | `examples/llm_workflow.sql` only | No documentation page |
| 37 | Lattice Datalog (Datalog^L) | 🟡 | `reference/lattice-datalog.md` (linked) | Reference only — no narrative "when do I need a lattice" page; no worked use-case |
| 38 | Magic sets / demand inference | 🟡 | Buried inside `features/reasoning-and-inference.md` and `user-guide/best-practices/datalog-optimization.md` | No standalone "speed up your inference" guide |
| 39 | Worst-case optimal joins (Leapfrog Triejoin) | 🟠 | One paragraph in release notes | No user-facing explanation |
| 40 | DRed (incremental retraction) | 🟠 | One paragraph in release notes | No user-facing explanation |
| 41 | Cypher / LPG mapping | 🟠 | `research/index.md` references it | No user-facing how-to (RDF-star → LPG); a frequent on-boarding question |
| 42 | Best practices (data modelling, bulk load, SPARQL patterns, SHACL patterns, federation perf, Datalog optimisation, update patterns, sparql perf) | 🟠 | `user-guide/best-practices/*.md` — only `index.md` lists three; the **other six are written but not linked** | High-quality content currently invisible |
| 43 | Migration / upgrade between pg_ripple versions | 🟡 | `operations/upgrading.md` + `operations/pg-upgrade.md` (**orphaned**) | Two upgrade pages, naming is confusing |
| 44 | Error catalog | 🟡 | `reference/error-catalog.md` linked; `reference/error-reference.md` (**orphaned, near-duplicate**) | Two near-identical pages |
| 45 | Troubleshooting | 🟡 | `operations/troubleshooting.md` linked; `reference/troubleshooting.md` (**orphaned**) | Two pages, unclear ownership |
| 46 | Embedding-function reference | 🟠 | `reference/embedding-functions.md` (**orphaned**) | Function-level reference exists, not in TOC |
| 47 | API stability promise | 🟠 | `reference/api-stability.md` (**orphaned**) | Important for adopters; hidden |
| 48 | Observability (Prometheus, OTLP) | 🟡 | `operations/monitoring.md` linked; `reference/observability.md` (**orphaned**) | Duplicated effort |
| 49 | Security | 🟡 | `operations/security.md` linked; `reference/security.md` (**orphaned**) | Duplicated effort |
| 50 | Release process | 🟠 | `reference/release-process.md` (**orphaned**) | Contributor doc; should be linked from contributing |
| 51 | Conformance — running W3C tests | 🟡 | `reference/running-conformance-tests.md` linked; `reference/running-w3c-tests.md` (**orphaned**) | Two pages, near-duplicates |
| 52 | LUBM results | 🟠 | `reference/lubm-results.md` (**orphaned**) | Linked from README badge but not from SUMMARY |
| 53 | AI agent / tool-use integration (LangChain, LlamaIndex, future) | 🔴 | Not documented | Planned for v1.1.0 — but the v0.50 `rag_context()` already powers this; no tutorial |

### 3.3 Aggregate

| Status | Count of capabilities |
|---|---|
| ✅ green | 12 |
| 🟡 yellow | 14 |
| 🟠 orange | 13 |
| 🔴 red | 14 |

Of the 53 surveyed capabilities, **only 23 % are fully documented** end-to-end and discoverable. **26 % are completely or near-completely missing** as user-facing prose.

---

## 4. Cross-cutting problems

### 4.1 Three competing organisations in `docs/src/`

| Top-level section | Intended purpose | Reality |
|---|---|---|
| `evaluate/` | Decision-making before install | One page only (`when-to-use.md`) |
| `getting-started/` | First success path | Healthy: install / hello world / tutorial / key concepts |
| `features/` | Conceptual deep-dives ("§2.x" headings) | 11 chapters, well-written, but inconsistent (some are reference, some are tutorial) |
| `user-guide/` | Task-oriented how-to | Contains a *parallel* set of feature pages (e.g. `rag.md`, `hybrid-search.md`, `graphrag.md`, `geospatial.md`) plus best-practices and a SQL reference |
| `operations/` | Day-2 admin | Mostly healthy — some duplicates with `reference/` |
| `reference/` | Function-level lookups | Mixed — some are pure reference (good), some are narrative (overlap with `features/`) |
| `research/` | Background reading | Two pages |

The `features/` ↔ `user-guide/` overlap is the single biggest problem: the same topic has *two* pages with different depth, and a user landing on either one cannot tell whether they are reading the canonical version. Examples: vector / hybrid search (3 pages), GraphRAG (4 pages), RAG (2 pages).

### 4.2 The orphaned-pages problem

Counting files on disk vs files referenced from `SUMMARY.md`:

| Section | Files on disk | Linked in SUMMARY.md | Orphaned |
|---|---|---|---|
| `user-guide/` (top level) | 21 | 2 | **19** |
| `user-guide/sql-reference/` | 21 | 0 | **21** |
| `user-guide/best-practices/` | 10 | 0 (only the parent `index.md` is implicit) | **10** |
| `user-guide/performance/` | 1 | 0 | **1** |
| `reference/` | 30 | 16 | **14** |
| `operations/` | 22 | 19 | **3** |

**~68 already-written documentation pages are not reachable from the sidebar.** This is the single highest-leverage fix in the entire project: simply wiring these into `SUMMARY.md` would close more than half of the perceived documentation debt.

### 4.3 The "AI/LLM" story is fragmented

pg_ripple's strongest commercial differentiator is "knowledge graph + vectors + LLMs in one PostgreSQL transaction". The relevant content is currently scattered across **at least seven pages**:

- `features/ai-retrieval-graph-rag.md` (canonical, well-written)
- `features/nl-to-sparql.md`
- `features/entity-alignment.md`
- `user-guide/rag-pipeline.md` (linked)
- `user-guide/rag.md` (orphaned, older)
- `user-guide/hybrid-search.md` (orphaned)
- `user-guide/graphrag.md`, `graphrag-enrichment.md`, `vector-federation.md` (all orphaned)
- `reference/embedding-functions.md` (orphaned)
- `reference/graphrag-functions.md`, `graphrag-ontology.md` (orphaned)

A new user evaluating pg_ripple for a RAG project will see only **one** of these pages from the sidebar and conclude that the AI story is shallow.

### 4.4 Record linkage / entity resolution is hidden under a technical name

`plans/neuro-symbolic-record-linkage.md` (a 30k-word strategic document) makes it clear that pg_ripple has a **best-in-class** record-linkage story: KGE candidate generation + SHACL hard rules + Datalog provenance + `owl:sameAs` canonicalization + audit log. This is a *category-defining* combination that no other PostgreSQL extension offers.

In the user-facing docs, this entire story is reduced to one page titled **"Entity Alignment with `owl:sameAs`"** — a name only an RDF practitioner would recognise. A data-integration architect searching for "record linkage" or "entity resolution" or "MDM" will find nothing.

### 4.5 No "use-case cookbook"

Every feature page is organised by *what the feature does*, not by *what the user is trying to accomplish*. There is no:

- "Build a product knowledge graph from a relational catalogue" recipe (R2RML → SHACL → SPARQL views).
- "Build a chatbot grounded in a knowledge graph" recipe (Turtle load → embed → `rag_context` → LLM).
- "Deduplicate customer records across two systems" recipe (KGE → suggest_sameas → SHACL gate → apply).
- "Audit who changed what fact when" recipe (PROV-O → audit log → temporal point_in_time).
- "Fan out CDC events to Kafka via a JSON-LD outbox" recipe.

These end-to-end recipes are how potential adopters evaluate whether to adopt the technology.

### 4.6 The `features/` chapter naming mixes audiences

The `features/` chapters are titled with §-style headings (`§2.1`, `§2.4`, `§2.5`, `§2.7`, `§2.8`) — consistent with a textbook-style document — but other chapters in the same folder (`exporting-and-sharing.md`, `nl-to-sparql.md`, `entity-alignment.md`) do **not** carry section numbers. The result reads as half-finished.

### 4.7 No "release-aware" docs

Many features were added in specific versions (e.g. `rag_context()` in v0.50.0) and the feature pages start with "pg_ripple v0.X.Y adds…". This is useful but inconsistent — some pages do not say which release introduced the feature, leaving users to guess whether their version supports it.

### 4.8 Versioning of the docs site itself

`book.toml` builds a single mdBook for the latest commit on `main`. There is no archived prior-version site, so a user on v0.50.0 reading the docs site sees v0.59.0 features with no version warning. (Outside the scope of this report to fix, but worth noting.)

---

## 5. Specific bugs and inconsistencies found in `SUMMARY.md`

(Cross-referenced against the file list under `docs/src/`.)

| Issue | Detail |
|---|---|
| **Duplicate "Architecture" links** | `operations/architecture.md` and `reference/architecture.md` both appear in the sidebar under different sections |
| **`user-guide/rag-pipeline.md` is in "Feature Deep Dives"** | But the parent folder `user-guide/` is otherwise unused in SUMMARY |
| **`user-guide/explain-sparql.md` is in "Feature Deep Dives"** | Same anomaly |
| **`research/` section is not in SUMMARY** | Two pages, both orphaned |
| **`evaluate/` only has `when-to-use.md`** | The original intent of an "Evaluate" tier (comparisons, benchmarks, ROI) is unrealised |
| **`reference/error-catalog.md` and `reference/error-reference.md`** | Two near-identical files — one must be deleted |
| **`reference/running-conformance-tests.md` and `reference/running-w3c-tests.md`** | Same — two near-identical files |
| **`operations/upgrading.md` and `operations/pg-upgrade.md`** | Two upgrade pages — naming should distinguish "pg_ripple upgrade" vs "PostgreSQL major version upgrade" |
| **`features/exporting-and-sharing.md` covers JSON-LD framing** | But there is no separate "JSON-LD Framing" entry in TOC, which is the most-asked-about feature for LLM users |

---

## 6. Recommendations

### 6.1 Phase 1 — Quick wins (no new content, ~1 day of work)

These are pure information-architecture changes; no new prose required.

1. **Wire all 68 orphaned files into `SUMMARY.md`** — every existing page becomes discoverable. This alone closes ~30 % of the gap.
2. **Delete or merge duplicates**: `error-catalog.md` ↔ `error-reference.md`; `running-conformance-tests.md` ↔ `running-w3c-tests.md`; `operations/security.md` ↔ `reference/security.md`; `operations/monitoring.md` ↔ `reference/observability.md`; `operations/troubleshooting.md` ↔ `reference/troubleshooting.md`.
3. **Resolve the `features/` ↔ `user-guide/` overlap**: pick one canonical location per topic. Recommendation: `features/` is the single conceptual chapter; `user-guide/sql-reference/*.md` becomes the function-level reference for that topic; orphaned `user-guide/*.md` pages either merge into `features/` or are deleted.
4. **Add `point_in_time`, `prov_enabled`, `audit_log_enabled`, `r2rml_load`, KGE functions to `reference/sql-functions.md`** — they are missing from the alphabetical reference even though they are exposed.
5. **Section-number every `features/` chapter** (or remove all numbers) — pick a convention.

### 6.2 Phase 2 — Restructure the navigation (~2 days of writing, no new SQL examples needed)

Proposed sidebar:

```
What Is pg_ripple? (landing)

EVALUATE
  When to Use pg_ripple
  Architecture at a Glance
  Performance & Conformance Results       (new — pulls in WatDiv, LUBM, OWL2RL)
  Comparison: vs Triple Stores / Graph DBs / Vector DBs / Plain SQL  (new)

GETTING STARTED
  Installation
  Five-Minute Walkthrough
  30-Minute Tutorial
  Key Concepts (RDF for SQL Users)

FEATURE DEEP DIVES
  Storing Knowledge
  Loading Data
  Querying with SPARQL
  Validating Data Quality (SHACL)
  Reasoning & Inference (Datalog, RDFS, OWL RL/EL/QL)    (expand)
  Exporting & Sharing
  Live Views & Subscriptions                              (merge views + CDC)
  Federation
  Geospatial                                              (promote)
  Full-Text Search                                        (promote)
  Temporal & Provenance                                   (NEW)
  Multi-Tenant Graphs                                     (NEW)

AI, RAG & RECORD LINKAGE                                   (NEW top-level section)
  AI Overview & Decision Tree                             (NEW)
  Vector Embeddings & Hybrid Search
  RAG Pipeline (rag_context)
  Natural Language to SPARQL
  Knowledge Graph Embeddings (KGE)                        (NEW)
  Record Linkage & Entity Resolution                      (RENAMED from Entity Alignment)
  GraphRAG End-to-End                                     (consolidate 4 pages)
  Vector Federation
  AI Agent Integration (LangChain, LlamaIndex)            (NEW, even if just a stub for v1.1.0)

USE CASE COOKBOOK                                          (NEW section)
  Knowledge Graph from a Relational Catalogue (R2RML)
  Chatbot Grounded in a Knowledge Graph
  Deduplicate Customer Records Across Systems
  Audit Trail with PROV-O + Temporal Queries
  CDC → Kafka via JSON-LD Outbox
  Probabilistic Rules for Soft Constraints
  SPARQL Repair Workflow
  Ontology Mapping & Alignment

APIs & INTEGRATION
  pg_ripple_http
  SPARQL Protocol Reference
  HTTP API Reference (OpenAPI)
  Streaming Cursors
  Materialized Views with pg-trickle
  Apache Arrow / Flight Bulk Export

OPERATIONS
  (existing operations/ section, unchanged structure)

REFERENCE
  SQL Function Reference
  GUC Reference
  Error Catalog
  SPARQL Compliance Matrix
  W3C / Jena / WatDiv / LUBM / OWL 2 RL Conformance      (one page each)
  Lattice Datalog
  GeoSPARQL Function Catalog
  Embedding Function Catalog
  GraphRAG Ontology
  HTTP API Reference
  Glossary
  FAQ
  Release Notes
  API Stability Promise                                   (promote)
  Roadmap

CONTRIBUTING
  Contributing
  Release Process                                         (promote)
  Running Conformance Tests
```

This structure (a) eliminates the `features/` ↔ `user-guide/` overlap, (b) elevates the AI story to a peer of the engine features, and (c) gives strategic capabilities (record linkage, KGE, temporal, multi-tenant) the visibility their commercial importance warrants.

### 6.3 Phase 3 — Write the missing pages

Estimated new prose, ranked by strategic impact:

| # | New page | Estimated length | Priority |
|---|---|---|---|
| 1 | **Record Linkage & Entity Resolution** (KGE → suggest_sameas → SHACL gate → apply, with worked precision/recall walkthrough) | 4–6 k words | **P0** |
| 2 | **Knowledge Graph Embeddings** (TransE/RotatE, training cost, when to use, `find_alignments`) | 2–3 k words | **P0** |
| 3 | **GraphRAG End-to-End** (consolidate 4 existing pages + add ingest→enrich→export→query in Microsoft GraphRAG) | 3–4 k words | **P0** |
| 4 | **AI Overview & Decision Tree** (when do I need: hybrid search? RAG? NL→SPARQL? KGE? Each box links to the right deep dive) | 1–2 k words | **P0** |
| 5 | **R2RML / Relational → RDF** | 2–3 k words | P1 |
| 6 | **Temporal & Provenance** (point_in_time + PROV-O + audit log, one consolidated chapter) | 2–3 k words | P1 |
| 7 | **Multi-Tenant Graphs** (tenant + RLS + quota + audit) | 1–2 k words | P1 |
| 8 | **Use Case Cookbook** (eight recipes, ~1 k words each) | 8 k words | P1 |
| 9 | **OWL 2 EL / QL profiles** | 1 k words | P2 |
| 10 | **SHACL-SPARQL rules** | 1 k words | P2 |
| 11 | **Lattice Datalog narrative guide** (when do I need a lattice; cookbook of the four built-in lattices) | 1–2 k words | P2 |
| 12 | **Worst-Case Optimal Joins / DRed / Tabling** (one operator-mental-model page that explains all three) | 2 k words | P2 |
| 13 | **Cypher / LPG → RDF mapping** (for Neo4j users) | 1–2 k words | P2 |
| 14 | **Probabilistic Rules** | 1 k words | P3 |
| 15 | **SPARQL Repair Workflow** | 1 k words | P3 |
| 16 | **AI Agent Integration** (placeholder + LangChain example) | 1 k words | P3 |

Total new prose: **~35 k words** — roughly the size of two of the existing `features/` chapters. This is achievable in a single dedicated documentation release (e.g. align with v0.60.0 or v1.0.0).

### 6.4 Phase 4 — Convert every `examples/*.sql` into a documentation page

Every file in `examples/` is currently invisible from the docs. They should each become an "Example" entry, either as:

- A standalone Use Case Cookbook recipe (for the multi-step ones), **or**
- An "Examples" appendix on the relevant feature page.

### 6.5 Phase 5 — Cross-cutting polish

1. **Add a "Since version vX.Y.Z" badge** to every feature page header — automated from the release notes.
2. **Add a "Related" footer** to every page linking to: prerequisite concepts, the relevant SQL function reference, and follow-up recipes.
3. **Rewrite the landing page** to mention the AI/RAG/record-linkage story in the first paragraph, not buried in a table.
4. **Add a `evaluate/comparison.md`** — pull the table from `when-to-use.md` into a standalone page so prospects can find it via search.
5. **Add a search-engine-friendly synonym list** — "record linkage", "entity resolution", "MDM", "deduplication" should all surface the relevant pages.

---

## 7. Out-of-scope follow-ups

These were noticed during the review but are not addressed by the recommendations above:

1. **Versioned docs site**. Long-term, the docs site should be built per release tag (`docs.pgripple.io/v0.59/`, `docs.pgripple.io/latest/`) rather than always reflecting `main`. mdBook does not handle this natively; would require a CI script.
2. **Tutorial videos / screen-casts**. The 30-minute tutorial would benefit from a recorded walk-through; outside the scope of mdBook content.
3. **Interactive playground**. `user-guide/playground.md` exists but only documents Docker. A web-based "try in browser" via WASM PostgreSQL or a hosted demo would dramatically lower the evaluation barrier.
4. **API-stability promise vs reality**. The `reference/api-stability.md` page exists but is orphaned. As pg_ripple approaches v1.0 this needs to be a first-class part of the navigation.
5. **`pg_ripple_http` has its own README** that is not surfaced anywhere in the mdBook.

---

## 8. Suggested delivery plan

| Release | Documentation work |
|---|---|
| **v0.60.0** | Phase 1 (wire orphans, delete duplicates) + Phase 2 (restructure SUMMARY.md). Zero new prose. Net effect: a new user discovers ~3× more features. |
| **v0.61.0** | Phase 3 P0 pages: Record Linkage, KGE, GraphRAG End-to-End, AI Overview. |
| **v0.62.0** | Phase 3 P1 pages: R2RML, Temporal & Provenance, Multi-Tenant, Use Case Cookbook recipes 1–4. |
| **v1.0.0** | Phase 3 P2/P3 + Phase 4 (examples → docs) + Phase 5 (polish, badges, related-links). The 1.0 release is the natural moment for an "API stability + completed docs" promise. |

This sequencing matches the ROADMAP themes already published (v0.60.0 documentation polish, v1.0.0 production hardening) and adds documentation work as a first-class deliverable rather than an afterthought.

---

## 9. Conclusion

pg_ripple's **engine** is mature; pg_ripple's **documentation** is roughly two releases behind. The good news is that most of the gap is *organisational*, not *missing content* — about 68 already-written pages simply need to be linked, and the duplicated `features/` ↔ `user-guide/` content needs to be consolidated. The remaining ~35 k words of net-new prose are concentrated in the strategic AI / record-linkage / GraphRAG areas where pg_ripple's competitive position is strongest and most under-sold today.

A focused, two- to three-release documentation effort would convert the docs site from a partial reference into a credible front door for the project, well in time for the v1.0.0 production-hardening release.
