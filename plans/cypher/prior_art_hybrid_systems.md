# Hybrid RDF + Property Graph Systems — Architectural Lessons

> **Scope**: Lessons from three production systems that support both RDF/SPARQL
> and LPG/Cypher on shared or closely coupled storage. Written April 2026.
> These systems are distinct from the pure LPG systems covered in
> [prior_art_graph_systems.md](prior_art_graph_systems.md) because they had to
> solve the same core problem pg_triple faces: how do you expose two graph models
> without duplicating data?

---

## Table of Contents

1. [Stardog — unified quad store, Cypher projection (now deprecated)](#1-stardog)
2. [AnzoGraph — columnar RDF* store, dual Bolt + SPARQL endpoints](#2-anzograph)
3. [Amazon Neptune — parallel separate stores, two-model silo](#3-amazon-neptune)
4. [Cross-cutting synthesis](#4-cross-cutting-synthesis)

---

## 1. Stardog

**Current name**: Stardog (v10.x, Stardog Union Inc.)
**License**: Commercial; Community Edition available

### Storage model

Stardog's canonical storage layer is a **custom persistent quad store** (S, P, O, G)
backed by **six full-permutation B-tree indices** (SPOG, SOPR, OPSG, POSG, GPOS,
GOPS). Every IRI, blank node, and literal is dictionary-encoded to a 64-bit
integer ID at ingest time — conceptually identical to pg_triple's XXH3-128
dictionary encoding. The store writes quads as four integer columns; string
payloads exist only in the dictionary. There is **no VP (vertical partitioning)
by predicate** — Stardog uses full permutation indices so any access pattern
(by S, by P, by O, by G, or combination) is efficient.

A differential index layer holds uncommitted/recent writes separately from the
main compacted index — architecturally analogous to pg_triple's planned HTAP
delta/main split.

### Dual-model approach

**RDF is the canonical model; property graph was a query-time projection.**

Stardog historically exposed a Cypher endpoint (`property-graph-queries`). Cypher
queries were internally rewritten to SPARQL algebra and executed against the same
quad store. As of Stardog 10.x, **the native Cypher endpoint has been deprecated
and removed**. Stardog now exposes SPARQL, GraphQL, SQL, and path queries; Cypher
is no longer a first-class interface.

The lesson is documented as a deliberate choice: maintaining a Cypher execution
engine alongside SPARQL proved too costly relative to the benefit. The translation
approach (Cypher → SPARQL algebra → execute) was simpler to maintain and allowed
one optimizer to handle both workloads.

Stardog's **Virtual Graph** mechanism is a separate concept: a federation layer
that allows SPARQL queries to include `GRAPH <virtual://source>` clauses
translated at runtime to SQL/NoSQL queries over external sources. Virtual graphs
are never stored; they are query-time projections. This is fundamentally different
from dual-model storage.

### Edge property handling

Stardog 7.1+ supports **RDF-star** for edge properties:

```turtle
<< :Pete :worksAt :Stardog >> :source :HR .
```

This is stored using a **triple-identifier (TID) side store**: the base triple
`(:Pete, :worksAt, :Stardog)` hashes to a TID, and then `(TID, :source, :HR)` is
stored in a separate structure keyed by that hash.

**Critical documented constraints:**
- The `edge.properties` database flag is **immutable** — it must be set at DB
  creation and cannot be enabled after the fact.
- Only the *subject* position of a triple can be a quoted triple (no nested
  annotations).
- Edge properties must be in the same named graph as the base edge.
- When `edge.properties` is enabled, transactions must use `ABORT_ON_CONFLICT`
  strategy rather than the default `LAST_COMMIT_WINS` — documented as problematic
  for clustered write workloads.
- Stardog explicitly states this feature "has several known performance problems
  and is **not recommended for new projects**." They are waiting for the finalized
  RDF 1.2 / SPARQL 1.2 standard before committing to a permanent implementation.

### Lessons for pg_triple

**Lesson S1 — Single unified store beats parallel stores.**
Stardog's approach of compiling Cypher to SPARQL and executing against one quad
store eliminates dual-write overhead and keeps one optimizer. For pg_triple,
implementing Cypher as a translation layer onto VP tables is strictly better than
maintaining a parallel LPG storage path.

**Lesson S2 — Cypher as a native execution target is expensive to sustain; translation is viable.**
Stardog dropped its native Cypher endpoint. The Cypher → SPARQL algebra translation
path proved more maintainable. For pg_triple, a standalone `cypher-algebra` crate
that lowers to the same SPARQL-derived SQL the `sparql/` pipeline uses is
architecturally sound and has the precedent of a production system validating the
approach.

**Lesson S3 — RDF-star via TID side store is feasible but costly; defer until RDF 1.2 finalizes.**
Stardog's TID-keyed side store requires a different transaction conflict resolution
strategy (ABORT_ON_CONFLICT) that limits write concurrency — a significant
regression. For pg_triple, the pragmatic path is a dedicated
`_pg_triple.edge_props(triple_hash BIGINT, prop_id BIGINT, val BIGINT, g BIGINT)`
table with an index on `triple_hash`, deferring full RDF 1.2 annotation semantics
until the W3C standard finalizes and Stardog's own second attempt is observable.

**Lesson S4 — Immutable storage flags signal correctness requirements, not arbitrary constraints.**
Stardog requires `edge.properties` at creation time because enabling it mid-life
would require rewriting the conflict-resolution strategy for already-committed
transactions. For pg_triple, the analogous decision is to document that enabling
`pg_triple.enable_edge_props` after data has been loaded requires a migration
step — it should never silently alter the behavior of existing VP tables.

**Lesson S5 — ICV guard mode is SHACL at commit time, not a separate system.**
Stardog's Integrity Constraint Validation stores SHACL constraints as RDF data and
runs a reduced validation pass against the write delta before the commit completes.
For pg_triple's SHACL module, this maps to: generate deferred trigger functions
(or `CHECK` constraints via `CONSTRAINT TRIGGER`) from `sh:NodeShape` definitions,
evaluated within the same transaction as the write.

---

## 2. AnzoGraph DB (Altair Graph Lakehouse)

**Current name**: Altair Graph Lakehouse (acquired from Cambridge Semantics)
**License**: Commercial

### Storage model

AnzoGraph is an **MPP in-memory columnar graph database**, partitioned across
cluster nodes. The storage format is fundamentally columnar — queries are
parallelized across predicates/columns and across data nodes simultaneously.

The canonical physical representation is **RDF* (RDF-star) as the unified
format**. All data — whether loaded via SPARQL Update or via a Cypher `CREATE`
— ends up in the same columnar RDF* store. There is no separate property graph
structure. Property graph data is mapped to RDF* at load time:

```
Cypher node: (n:Person {name: 'Alice', born: 1990})
→ RDF:  <Alice> rdf:type <Person> .
        <Alice> <name> 'Alice' .
        <Alice> <born> 1990 .

Cypher edge: (Alice)-[:KNOWS {since: 2020}]->(Bob)
→ RDF*: <Alice> <KNOWS> <Bob> .
        << <Alice> <KNOWS> <Bob> >> <since> 2020 .
```

### Dual-model approach

**Unified store; dual protocol endpoints on the same data.**

AnzoGraph exposes:
- SPARQL endpoint (port 7098): SPARQL 1.1 + SPARQL* (RDF-star)
- Bolt endpoint (port 7088): openCypher v9 (subset)

Both endpoints operate against the **same physical columnar RDF* store**. A node
loaded via Cypher `CREATE` is immediately queryable via SPARQL without any ETL.
The duality is entirely at the query interface level.

The predicate catalog must be pre-populated before Cypher data loads
(`auto_predicate` mode); node labels must be registered as predicates (analogous
to `_pg_triple.predicates`) before VP routing can work.

### Edge property handling

Edge properties use RDF* annotations on base triples (the `<< ... >>` quoted
triple syntax). The base triple is implicitly asserted; the annotation adds the
edge property as a separate fact referencing the triple's identity. This is
AnzoGraph's native edge-property mechanism — no reification, no `rdf:Statement`
pattern.

### Cross-model querying

Because the storage is unified, **a SPARQL query and a Cypher query both see
the same underlying data**. You cannot mix SPARQL and Cypher syntax in a single
query (separate endpoints), but there is no data barrier between models.

### Write semantics

**`MERGE` is explicitly not supported** in the current release. Cypher write
support is limited to partial `CREATE`, `DELETE`, `SET`, `REMOVE` — parameters
in `CREATE`/`SET` are not supported, and multiple UPDATE clauses in one statement
are not supported. **MERGE is absent because implementing upsert semantics over a
columnar MPP store requires distributed locking + read-modify-write**, which is
antithetical to MPP analytics architecture.

Bulk loading via flat files is the primary write path for large datasets.

### Lessons for pg_triple

**Lesson A1 — RDF* as the unified canonical format is the cleanest architecture.**
AnzoGraph's mapping of all Cypher constructs to RDF* means exactly one storage
model to maintain, optimize, and index. For pg_triple, the mapping is already
complete for nodes and edges; the only gap is edge properties, which map to
RDF*-annotated triples. The `_pg_triple.edge_props` side table (Lesson S3) is the
minimal implementation of this pattern.

**Lesson A2 — Predicate catalog registration must be explicit on the write path.**
AnzoGraph's `auto_predicate` mode registers Cypher labels as predicates in the
catalog before data loads. For pg_triple, when Cypher `CREATE (n:Person)` arrives,
the label must be resolved to a predicate ID and registered in `_pg_triple.predicates`
before VP table routing occurs. This should be an explicit step, not silent
fallback to `vp_rare`.

**Lesson A3 — No MERGE in MPP analytical systems confirms PostgreSQL's structural advantage.**
AnzoGraph omits MERGE because distributed OCC is expensive over columnar storage.
pg_triple's VP tables use PostgreSQL MVCC: `INSERT ... ON CONFLICT DO NOTHING ...
RETURNING` is atomic at zero extra cost. This is a concrete reason to keep Cypher
writes within the PostgreSQL SPI execution path rather than delegating to an
external graph engine.

**Lesson A4 — Columnar storage is analytically faster but OLTP-restricted.**
AnzoGraph's MPP columnar format outperforms row-store triple stores on aggregation
workloads but cannot handle transactional write patterns. pg_triple's HTAP split
(delta heap + BRIN main + merge worker) is the hybrid that achieves both. Cypher
analytics workloads will naturally benefit from the merge worker having consolidated
rows into the BRIN-indexed main partition.

**Lesson A5 — Separate Bolt and SPARQL endpoints over shared storage is the right UX.**
AnzoGraph demonstrates that users accept two separate protocol endpoints (one for
analysts running SPARQL, one for developers running Cypher) as long as the data
is the same. pg_triple should expose `pg_triple.sparql(...)` and
`pg_triple.cypher(...)` as separate SQL functions with the same storage backing —
no need for a unified query language.

---

## 3. Amazon Neptune

**Managed service**: AWS Neptune Database (graph OLTP) + Neptune Analytics (in-memory analytics)
**License**: Commercial (AWS managed)

### Storage model

Neptune Database uses a **custom distributed graph storage engine** on an
Aurora-style shared storage volume (3-AZ replication, up to 128 TiB). The byte
format is not publicly documented, but from AWS documentation and public
presentations:

**Neptune has two completely separate logical data stores within one cluster:**

1. **Property Graph store** (Gremlin / openCypher): vertices with label and
   property bags, edges with label, direction, and property bags. Physically
   organized as adjacency lists. Each vertex has an internal UUID; edges are
   first-class entities with their own identity and property bags.

2. **RDF store** (SPARQL): a quad store (S, P, O, G) with six permutation
   indices (similar to Jena/Blazegraph). Integer identifiers for all terms
   (dictionary-encoded), separate from the property graph store's ID space.

These two stores are **completely separate and isolated**. Loading data via
SPARQL Update populates the RDF store; loading via CSV bulk load or Gremlin write
populates the property graph store. There is no automatic bridging.

**Neptune Analytics** (a separate AWS product) is an in-memory graph analytics
engine supporting **only openCypher** (no SPARQL). It can ingest from Neptune
Database (property graph store only) or from S3. It is optimized for traversal
and graph algorithm execution, not OLTP.

### Dual-model approach

**Parallel separate stores with no cross-reference capability.**

Neptune accepts Gremlin, openCypher, and SPARQL from the same endpoint (port 8182)
via different HTTP paths (`/gremlin`, `/openCypher`, `/sparql`). However:

- A Gremlin or openCypher query **cannot reference RDF triples.**
- A SPARQL query **cannot reference property graph vertices/edges.**
- **No single query can span both models.**
- **There is no cross-model join or federation.**

Neptune chose this architecture to avoid impedance mismatch at the query level.
Each model remains independently consistent. Customers who need to query across
both models must ETL data from one store to the other.

### Edge property handling

**Property graph store**: edges are first-class entities, so edge properties are
stored directly as key-value pairs on the edge entity. No reification required —
this is the LPG model's structural advantage over RDF for edge properties.

**RDF store**: Neptune supports SPARQL 1.1 but **not RDF-star** (as of the last
published documentation). Edge properties in the RDF store require standard
`rdf:Statement` reification or named-graph-per-edge patterns, which are expensive.

### Cross-model querying

Not supported. Neptune's stated guidance is: "choose the model that fits your data
best." The Neptune ML feature (graph neural networks) operates on the **property
graph store only** — not on RDF data.

### Write semantics

Neptune supports full ACID writes for all three query languages:
- SPARQL Update (INSERT DATA, DELETE DATA, INSERT/DELETE WHERE) — ACID
- Gremlin mutations — ACID
- openCypher DML (CREATE, MERGE, SET, DELETE) — ACID, **including MERGE**
- Bulk loading via S3 — at-least-once semantics (not transactional)

Neptune's MERGE implementation under concurrent writes is production-hardened.
How Neptune implements MERGE concurrency safety is not publicly documented, but
the feature is confirmed reliable by AWS customer case studies.

### Lessons for pg_triple

**Lesson N1 — Two-store parallel architectures are an architectural dead end for unified semantics.**
Neptune's customers who want to reason over both their RDF knowledge graph and their
LPG social graph must ETL. For pg_triple, a single unified VP table layer (where
Cypher edges are simply triples/quads) avoids this permanently. The decision to
use RDF as the canonical form must be made at the foundation and never revisited.

**Lesson N2 — Neptune Analytics vs Neptune Database is a lesson in OLTP/OLAP separation.**
Neptune Analytics (in-memory, columnar, algorithm-optimized) is a separate product
because the Neptune Database engine cannot efficiently execute analytical workloads
at scale. pg_triple's HTAP split mirrors this separation within a single PostgreSQL
instance: delta tables handle OLTP, BRIN-indexed main tables handle analytics.
The merge worker is what makes both efficient — it is not optional.

**Lesson N3 — Native LPG edge properties are structurally cheaper than RDF-star.**
Neptune's property graph store carries edge properties at zero extra cost (edges
have their own identity and property bags). If pg_triple receives write-heavy
Cypher workloads with dense edge properties, a dedicated
`_pg_triple.edge_props(triple_hash BIGINT, prop_id BIGINT, val BIGINT, g BIGINT)`
table with an index on `triple_hash` may be more efficient than routing through
RDF-star annotation VP tables, because it separates the index structures from the
base triple scan.

**Lesson N4 — Neptune ML's export–external-compute–reimport pattern is the template for future ML integration.**
Neptune ML exports the property graph to SageMaker in DGL/PyTorch Geometric format,
trains node/link prediction models, and imports predictions back as vertex/edge
properties. For pg_triple, this maps to: reserve a named graph (e.g., `g_id` for
the ML-derived graph) for imported prediction triples, keeping them queryable
alongside the base knowledge graph without polluting the explicit triple store.

**Lesson N5 — "openCypher, Gremlin, and SPARQL on the same cluster" does not mean "on the same data."**
Neptune's marketing suggests multi-model capability, but the implementation is
three separate query engines over two isolated stores. Any future pg_triple
documentation of Cypher support should be explicit: **SPARQL and Cypher are two
interfaces to the same VP table data**, not two separate databases cohabiting a
server. This is a fundamental architectural differentiator from Neptune.

---

## 4. Cross-cutting synthesis

### 4.1 The universal consensus: dictionary encoding

All three systems — Stardog (B-tree quad store), AnzoGraph (columnar MPP),
Neptune (distributed adjacency + quad store) — independently encode all IRIs,
blank nodes, and literals to integer IDs before storage. This is not accidental.
Integer joins are 2–4× faster than string joins at the storage layer; all three
systems measured this and arrived at the same conclusion.

pg_triple's XXH3-128 dictionary encoding is validated by all three production
systems as the baseline correct design.

### 4.2 The consensus on unified vs. parallel storage

**Unified RDF-first storage with LPG as a structural view is the correct architecture.**

| System | Architecture | Outcome |
|---|---|---|
| Stardog | Unified RDF quad store; Cypher → SPARQL translation | Successful; Cypher endpoint later deprecated as too costly to maintain |
| AnzoGraph | Unified columnar RDF* store; Cypher mapped to RDF* at load | Successful; operationally clean; MERGE not supported |
| Neptune | Parallel separate stores (RDF + LPG) | Creates hard query barrier; ETL required for cross-model queries |

The Stardog and AnzoGraph experiences confirm: a single integer-encoded quad/triple
store can serve both SPARQL and Cypher workloads. The only question is whether
the edge-property extension (RDF-star / a triple-hash side table) adds acceptable
overhead.

Neptune is the counter-evidence: parallel stores give operational isolation but
create a permanent semantic barrier. The fact that Neptune Analytics was built as
a *separate product* to handle analytics that Neptune Database cannot do is an
implicit admission that the property graph store was never enough for analytical
workloads — exactly the split pg_triple's HTAP architecture prevents.

### 4.3 RDF-star: necessary but not yet stable

| System | RDF-star status | Lesson |
|---|---|---|
| Stardog | Built 2019; deprecated 2024 pending RDF 1.2 | Premature commitment = tech debt |
| AnzoGraph | Built-in from the start; production-stable | Works if the store is designed for it from day 0 |
| Neptune RDF store | Not supported; standard reification only | No RDF-star = unusable edge properties in RDF |
| Neptune LPG store | Native edge identity (no RDF-star needed) | LPG gets edge properties for free |

The synthesis: **implement the minimal edge-property extension now** using a
`triple_hash`-keyed side table, design its external API to match emerging RDF 1.2
semantics, and plan a migration to native RDF-star VP tables when the W3C standard
finalizes and pgrx/oxrdf provide stable support (planned in pg_triple v0.4.0).

The Stardog deprecation is a warning not to over-commit to a specific RDF-star
surface syntax before RDF 1.2 finalizes. The AnzoGraph experience confirms that
the underlying storage concept (edge annotation via triple identity) is sound.

### 4.4 Cypher as translation target vs. native execution engine

| Approach | Seen in | Outcome |
|---|---|---|
| Cypher → SPARQL algebra → execute | Stardog (historically) | Maintainable; operator-sharing; Stardog chose this |
| Cypher → native execution engine | Neo4j, Memgraph | Highest performance; highest maintenance cost |
| Cypher → SQL via algebra IR | pg_triple (proposed) | Viable; validated by Stardog's choice; PostgreSQL optimizer replaces both algebraic optimizer AND execution engine |

For pg_triple, the translation path (Cypher algebra → SQL → PostgreSQL executor)
is architecturally equivalent to Stardog's Cypher → SPARQL → execute path, but
targets SQL instead of SPARQL. This is strictly less work than a native Cypher
executor and shares the PostgreSQL optimizer's full power (join reordering,
statistics, parallel query, AIO) at no extra cost.

**The `cypher-algebra` crate (standalone, independently published) produces the
algebra IR; `src/cypher/translator.rs` lowers it to SQL. This is validated by
two production systems as the correct separation.**

### 4.5 What these systems regret — avoid in pg_triple

| Anti-pattern | Seen in | Why to avoid |
|---|---|---|
| Parallel separate stores | Neptune | Hard query barrier; ETL required between models; two schema registries |
| RDF* implemented before standard finalizes | Stardog | Deprecated 5 years later; migration cost |
| Edge-property store requiring immutable DB flag | Stardog | Limits operational flexibility for live systems |
| Cypher MERGE without MVCC or uniqueness constraint | Widespread | Race condition under concurrent writes (see prior_art_graph_systems.md §9.2) |
| Columnar-first storage without row-oriented write path | AnzoGraph | MERGE becomes unsupportable; bulk-load-only write model |
| `auto_predicate` silent fallback on unknown predicates | AnzoGraph | Silently drops data to vp_rare instead of failing; correctness hazard |

### 4.6 Recommended sequence for pg_triple dual-model support

Based on all three hybrid systems:

1. **No changes needed to VP table schema** for read-only Cypher (`MATCH`/`RETURN`).
   VP tables already encode all structural elements of an LPG (nodes via
   `vp_rdf_type`, edges via predicate VP tables, node properties via property VP
   tables). The `cypher-algebra` crate + `translator.rs` is sufficient for Phase 1.

2. **Cypher DML without edge properties** (`CREATE`, `SET`, `DELETE`, `MERGE`)
   requires no schema changes either. All writes route through the existing
   `insert_triple` / `delete_triple` API with dictionary encoding. This is
   validated by AnzoGraph's RDF-first unified store approach.

3. **Cypher edge properties** require a single schema addition:
   `_pg_triple.edge_props(triple_hash BIGINT, prop_id BIGINT, val BIGINT, g BIGINT)`
   with an index on `(triple_hash, prop_id)`. This is a net-new table with no
   impact on existing VP tables. Defer until v0.4.0 (RDF-star release) or a
   confirmed use-case requirement.

4. **Never build a parallel LPG store.** Neptune proves this is a dead end.

---

## 5. References

- Stardog documentation: https://docs.stardog.com/
- Stardog property graph queries (archived): https://docs.stardog.com/query-stardog/property-graph-queries
- Stardog ICV (SHACL guard mode): https://docs.stardog.com/data-quality-constraints
- AnzoGraph / Altair Graph Lakehouse documentation: https://docs.cambridgesemantics.com/anzograph/
- AnzoGraph openCypher support: https://docs.cambridgesemantics.com/anzograph/userdoc/property-graphs.htm
- Neptune feature overview: https://docs.aws.amazon.com/neptune/latest/userguide/feature-overview-data-model.html
- Neptune openCypher: https://docs.aws.amazon.com/neptune/latest/userguide/access-graph-opencypher.html
- Neptune Analytics: https://docs.aws.amazon.com/neptune-analytics/latest/userguide/what-is-neptune-analytics.html
- RDF 1.2 concepts (W3C working draft): https://www.w3.org/TR/rdf12-concepts/
- openCypher specification: https://opencypher.org/
- Companion document (pure LPG systems): [prior_art_graph_systems.md](prior_art_graph_systems.md)
- Core analysis: [cypher_lpg_analysis.md](cypher_lpg_analysis.md)
