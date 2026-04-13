# Graph System Prior Art — Cypher/LPG Architectural Lessons

> **Scope**: Architectural lessons extracted from eight graph databases for
> implementing Cypher/LPG query support on top of pg_ripple's dictionary-encoded
> integer VP table storage. Written April 2026.
>
> Each section follows the same structure: storage model → Cypher compilation
> approach → MERGE semantics → label indexing → edge identity/edge properties →
> variable-length paths → LPG↔RDF convergence. Lessons are numbered within each
> section and cross-referenced where relevant.

---

## Table of Contents

1. [Neo4j — Cypher inventor, Cypher 25, execution plan anatomy](#1-neo4j)
2. [Kuzu — columnar embeddable, Volcano plan, academic rigor](#2-kuzu)
3. [FalkorDB — GraphBLAS sparse matrix, dictionary interning](#3-falkordb)
4. [Memgraph — in-memory delta storage, access-tier concurrency](#4-memgraph)
5. [Apache AGE — PostgreSQL Cypher extension, agtype format](#5-apache-age)
6. [JanusGraph — distributed Bigtable adjacency, vertex-centric indexes](#6-janusgraph)
7. [Dgraph — posting list storage, UID dictionary, GraphQL+DQL](#7-dgraph)
8. [NebulaGraph — strong-schema, nGQL/openCypher, VID sharding](#8-nebulagraph)
9. [Cross-cutting synthesis](#9-cross-cutting-synthesis)

---

## 1. Neo4j

**Cypher version**: Cypher 25 (post-ISO GQL alignment, versioned release cycle starting 2025).
The prior dialect is now called Cypher 5. The split is analogous to SQL:199x versus SQL:2023.

### Storage model

Neo4j maintains a **property record store** (legacy) and a more recent
**block format** (5.x+). Under both, the unit of storage is:

- A *node record* with: an in-use flag, a chain head pointer to its property
  records and to its first relationship, and a packed label bitfield.
- A *relationship record* with: source node id, target node id, relationship
  type id, next/prev pointers in the doubly-linked adjacency lists for both
  endpoints, and a chain head pointer to its property records.
- *Label token index* (previously "node label index", now "token lookup index"):
  a separate index from property indexes; maps `label → [node_id, …]` and
  `rel_type → [rel_id, …]`.

This is fundamentally different from VP table storage. Neo4j's record store is
a fully normalized entity-centric structure (one physical record per node, one
per edge, one per property value chain). VP tables are predicate-centric; there
is no "node record" — a node exists as the aggregate of all VP rows that share
its subject ID.

### Cypher compilation

Neo4j uses a three-phase pipeline:

```
Cypher text
    │
    ▼  Parser (Antlr grammar)
AST
    │
    ▼  Planner / Optimizer
    │    • cost-model over: label cardinalities, index selectivities,
    │      relationship type counts (maintained in a "count store")
    │    • produces a Logical Plan (operator tree)
    ▼
Logical Plan (operator tree)
    │
    ▼  Runtime selection
    │    • Interpreted (safe, slow)
    │    • Slotted (typed slots per variable, JIT-friendly)
    │    • Pipelined (fused operators, morsel-driven parallel)
    │    • Parallel (morsel-based multi-threaded)
    ▼
Physical Plan (same operators, different execution engine)
    │
    ▼  SPI-equivalent (storage calls)
Result rows
```

`EXPLAIN` outputs the logical plan tree; `PROFILE` runs the physical plan and
reports actual rows and database hits per operator.

Key operators relevant to pg_ripple:

| Operator | Meaning | pg_ripple equivalent |
|---|---|---|
| `NodeByLabelScan` | Full scan of token lookup index for label | `SELECT s FROM vp_rdf_type WHERE o = label_id` |
| `NodeIndexSeek` | Point lookup in a property index | B-tree seek on a VP table |
| `NodeIndexScan` | Range scan over a property index | Range scan on a VP table |
| `Expand(All)` | Traverse all edges from a node by type | Join to the appropriate VP table |
| `Expand(Into)` | Check whether edge exists between two bound nodes | `EXISTS (SELECT 1 FROM vp_x WHERE s = a AND o = b)` |
| `VarLengthExpand(All)` | `[*1..n]` traversal, path materialization | `WITH RECURSIVE … CYCLE` |
| `VarLengthExpand(Pruning)` | `[*1..n]` returning only unique endpoints | Pruning-BFS recursive CTE |
| `Repeat(Trail)` | ISO GQL quantified path pattern `(…){m,n}` | `WITH RECURSIVE … CYCLE` |
| `Merge` | MATCH-or-CREATE | `ON CONFLICT DO NOTHING` + conditional insert |
| `LockingMerge` | MERGE when creating a rel between already-matched nodes | PG advisory locks or `FOR UPDATE` |
| `MergeUniqueNode` | MERGE backed by a uniqueness constraint (2026.02+) | Unique constraint + `ON CONFLICT` |
| `IntersectionNodeByLabelsScan` | `MATCH (n:A:B)` — nodes with all labels | JOIN `vp_type` twice |
| `UnionNodeByLabelsScan` | `MATCH (n:A\|B)` — nodes with any label | UNION scan over `vp_type` |
| `TriadicSelection` | Friends-of-friends-not-already-friends | Two VP joins + anti-join |
| `ShortestPath` / `StatefulShortestPath` | Bidirectional BFS shortest path | Not in SPARQL; new requirement |

Execution plan is a **binary tree read bottom-up**. Leaf operators read data;
inner operators transform or join rows; root operator (`ProduceResults`) gives
the client-facing result set.

Lazy evaluation by default — operators stream rows rather than materializing
full intermediate results. *Eager* operators (sort, aggregation, `DISTINCT`,
`Eager` barrier for write-isolation) materialize their full input before
producing any output.

### MERGE semantics

Neo4j exposes three distinct operator variants for MERGE depending on context:

1. **`Merge`** — general-purpose: try to match; if no match, create. May produce
   a TOCTOU race under concurrent writes in a single-writer system only. Neo4j's
   MVCC means two concurrent transactions can both observe "no match" and both
   insert, violating uniqueness unless a constraint exists.
2. **`LockingMerge`** — used when the MERGE pattern includes a relationship: locks
   both endpoint nodes before creating the relationship. Prevents the race at the
   cost of reduced parallelism.
3. **`MergeUniqueNode`** — used when a property uniqueness constraint covers the
   MERGE key: delegates to the constraint violation path, which is inherently safe.

**Lesson 1**: pg_ripple's MERGE implementation should follow the same three-tier
approach. The equivalent of `MergeUniqueNode` is an `ON CONFLICT DO NOTHING … RETURNING`
pattern against a VP table with a `UNIQUE(s, o)` constraint; this is the safe,
non-racy path. When no uniqueness constraint covers the MERGE key, the extension
must serialize via `pg_advisory_xact_lock(predicate_id, subject_id)` or equivalent
to prevent concurrent double-inserts.

### Label indexing

Neo4j uses a dedicated **token lookup index** (one global B-tree from token_id
to a sorted list of node IDs) that is always maintained. Property indexes are
separate and optional.

`MATCH (n:Person)` uses `NodeByLabelScan` against the token lookup index ONLY
if no property predicate can be pushed into a property index. With a predicate
`WHERE n.age > 30`, the planner chooses between `NodeByLabelScan + Filter` vs
`NodeIndexSeek` depending on estimated selectivity.

**Lesson 2**: In pg_ripple, `rdf:type` VP table is the label index. For
`MATCH (n:Person)`:

```sql
SELECT s AS node_id
FROM _pg_ripple.vp_rdf_type   -- or vp_rare if below threshold
WHERE o = @Person_id
```

This is already present. The label-property compound access (e.g. `MATCH (n:Person {age: 30})`)
compiles to a JOIN between `vp_rdf_type` and `vp_age`:

```sql
SELECT t.s
FROM _pg_ripple.vp_rdf_type t
JOIN _pg_ripple.vp_age a ON t.s = a.s
WHERE t.o = @Person_id
  AND a.o = @30_id
```

The PostgreSQL planner will choose the access order based on statistics
exactly as Neo4j's cost model does.

### Variable-length paths

`[*1..n]` produces `VarLengthExpand(All)`. Pruning variants return unique end
nodes only (useful for DISTINCT reachability). Shortest-path uses bidirectional
BFS (StatefulShortestPath, introduced with Cypher 25's ISO GQL quantified path
patterns).

**Lesson 3**: Use `WITH RECURSIVE … CYCLE` (PostgreSQL 18 native) for
`[*1..n]` traversals. The `CYCLE` clause provides O(1) hash-based cycle
detection — do not implement manual visited-set in the Cypher→SQL translator.
For bounded `[*m..n]`, add a depth counter column to the recursive CTE and
terminate when depth = n.

The `Repeat(Trail)` operator (ISO GQL quantified path patterns) is different
from variable-length expand: trail semantics require that no *edge* is repeated
(as opposed to no *node* repeated in simple path semantics). Implement these as
separate CTE shapes.

### openCypher TCK compliance

Neo4j is the reference implementation. The openCypher TCK (Technology
Compatibility Kit) test suite at https://github.com/opencypher/openCypher contains
~2000 Gherkin scenarios. Neo4j passes all of them by definition.

**Lesson 4**: The TCK is the ground truth. The pg_ripple Cypher compiler should
target TCK compliance incrementally: start with the `Match` and `Create` feature
files, then add `Merge`, `Delete`, `Set`, and `With`. Tracking TCK pass rate
publicly provides a credible signal of completeness.

---

## 2. Kuzu

Kuzu is an embeddable analytical graph database (C++, Apache-2.0). It is the
most carefully engineered open-source graph database from a query compiler
standpoint and the closest academic analog to what pg_ripple needs.

### Storage model

Kuzu uses **columnar chunked storage**:

- **Node table** (one per node label): a column-store table with a fixed set of
  typed property columns. Node identity = sequential uint64 internal id.
- **Edge table** (one per edge type): a CSR (Compressed Sparse Row)-like structure
  storing `(src, dst)` adjacency lists, plus property columns for edge properties.
  Forward and backward CSR lists are both maintained.
- **Chunked array format**: columns stored in fixed-size chunks (2^16 rows by
  default). This enables vectorized scans and SIMD comparisons.

Structural comparison to pg_ripple VP tables:

| Kuzu concept | pg_ripple equivalent |
|---|---|
| Node table `Person(name: STRING, age: INT64)` | `vp_rdf_type(s, o)` + `vp_name(s, o)` + `vp_age(s, o)` |
| Edge table `KNOWS(src: Person, dst: Person, since: INT64)` | `vp_knows(s, o)` + `vp_since(s=triple_hash, o)` via RDF-star |
| Forward CSR adjacency | B-tree index on `(s)` in each VP table |
| Backward CSR adjacency | B-tree index on `(o, s)` in each VP table (already present) |

**Key difference**: Kuzu's node table has a fixed, declared schema per label.
pg_ripple's VP layout is schema-free — a "Person" node can have any predicates.
This difference manifests in how MATCH patterns compile: Kuzu can emit a single
columnar scan with typed projection; pg_ripple must JOIN multiple VP tables.

### Cypher compilation

Kuzu uses a Volcano/iterator physical model with a clean logical→physical split:

```
Cypher text
    │  cypher-parser (custom recursive descent or Antlr)
    ▼
AST
    │  binder (variable resolution, type checking, schema binding)
    ▼
Logical Plan (relational algebra extended with graph operators)
    │  planner (cardinality estimation using ChiSquare statistics)
    │  rewriter passes:
    │    • projection pullup
    │    • filter pushdown
    │    • SemiJoin elimination
    │    • cross-product to join conversion
    ▼
Physical Plan (Volcano operators)
    │
    ▼  morsel-based vectorized execution
```

Operators include: `TableScan`, `IndexScan`, `HashJoin`, `HashAggregate`,
`RecursiveJoin` (for `[*m..n]` paths), `ShortestPathScan`, `FilterProject`.

**Lesson 5**: Kuzu's clean binder/planner/executor separation is the right
architecture. The analog for pg_ripple is:

```
cypher-algebra crate (standalone)
  parse → AST → algebra IR (schema-generic)
      ↓
pg_ripple src/cypher/translator.rs
  algebra IR + schema binding (dictionary lookup for constanted IRIs, VP table OIDs)
  → SQL text emitted via SPI
```

The schema binding step (resolving variable names to VP table OIDs and encoding
literal constants) is analogous to Kuzu's binder. This step belongs in the
pg_ripple layer, not in the standalone `cypher-algebra` crate.

### MERGE semantics

Kuzu implements MERGE as:

1. Attempt a hash probe on the primary key of the node table.
2. If found, bind the existing record to the MERGE variable.
3. If not found, insert and bind the new record.

Kuzu's node tables have a mandatory primary key (unlike Neo4j where uniqueness
constraints are optional). This means every MERGE in Kuzu is implicitly backed
by the `MergeUniqueNode` pattern — the safe path.

**Lesson 6**: pg_ripple should require that any `MERGE (n:Label {key: val})`
pattern have a uniqueness constraint on `(label, key)` to be safe under
concurrent writes. Without it, warn the user (similar to how Neo4j warns that
Merge without a constraint is not concurrency-safe in its documentation).

### Variable-length paths

Kuzu does NOT compile `[*m..n]` to SQL recursive CTEs. Instead, it uses a
dedicated `RecursiveJoin` physical operator that executes iterative BFS/DFS
internally, storing the frontier and path state in buffers. This avoids the
overhead of the SQL engine's CTE recursion infrastructure.

For pg_ripple — which delegates to PostgreSQL's execution engine — `WITH RECURSIVE`
is the correct translation. But the bounded-depth optimization matters: `[*1..3]`
should produce a CTE with a depth counter that terminates at 3 hops, not an
unbounded recursion with a LIMIT on output rows.

**Lesson 7**: Distinguish four path pattern shapes in the Cypher → SQL translator:

| Pattern | SQL form |
|---|---|
| `[r:TYPE]` (single hop) | Direct VP table join |
| `[*n]` (exact n hops) | n nested joins or a depth-bounded recursive CTE |
| `[*m..n]` (bounded) | Recursive CTE with depth counter `BETWEEN m AND n` + `CYCLE` |
| `[*]` (unbounded) | Recursive CTE with `CYCLE` and no depth bound — **danger zone** |

Unbounded `[*]` should be rejected or require explicit `WITH CYCLE` acknowledgment
from the user, since it can produce arbitrarily large result sets.

### openCypher TCK compliance

Kuzu maintains the highest known openCypher TCK compliance rate among open-source
systems (~95%+ of non-enterprise-specific tests as of early 2026). They track
compliance publicly in the repository.

---

## 3. FalkorDB

FalkorDB (formerly RedisGraph) is a sparse-matrix graph database using GraphBLAS
for graph operations. It is implemented as a Redis module (standalone in v4+).

### Storage model

FalkorDB's core novelty is that it represents graphs as **sparse adjacency
matrices** using GraphBLAS `GrB_Matrix` objects:

- One boolean sparse matrix per edge type: rows = source vertex IDs, columns =
  destination vertex IDs.
- Matrix-vector multiply = one-hop traversal (fan-out from a set of roots).
- Iterated matrix multiply = multi-hop; GraphBLAS BFS algorithms handle `[*]`.
- Vertex and edge properties stored in a separate property dictionary, not in
  the matrix itself.

FalkorDB also maintains a **dictionary encoder** for string values:

```
string → integer id   (global dictionary)
```

From v4.10, FalkorDB exposes string interning explicitly via an `intern()`
function in Cypher. Before this, string values were stored as separate heap
objects; after `intern()`, all identical strings share a single memory instance.
Equality checks on interned strings become O(1) pointer comparisons.

**Lesson 8 (direct validation)**: pg_ripple's dictionary encoder (XXH3-128 hash,
LRU cache) is architecturally equivalent to FalkorDB's `intern()` but automatic
and always-on. Every string is encoded before storage; equality is integer
comparison. pg_ripple's approach is strictly stronger: FalkorDB requires the user
to call `intern()` explicitly, meaning un-interned strings get no benefit.

The FalkorDB pattern confirms that dictionary encoding at insert time (not query
time) is the correct architecture for a high-frequency query workload.

### Cypher compilation

FalkorDB compiles openCypher to a sequence of **matrix/vector algebra
operations**:

- `MATCH (n)-[:KNOWS]->(m)` → multiply the KNOWS adjacency matrix against a
  source vector.
- `MATCH (n:Person)` → intersection of the KNOWS result vector with the Person
  label vector.
- Filters on properties → property dictionary lookups.

This is not directly applicable to pg_ripple — pg_ripple will compile to SQL
joins, not matrix multiplications. However, the conceptual mapping is analogous:
VP tables are the same abstraction as FalkorDB's per-edge-type matrices (one
relation/matrix per predicate).

**Lesson 9**: The analogy between VP tables and adjacency matrices holds cleanly.
Both are predicate-partitioned edge stores. The traversal algebra that works for
FalkorDB (expand one hop = one table access; multi-hop = recursive) is exactly
what the pg_ripple Cypher→SQL compiler should produce.

### MERGE semantics

FalkorDB does not fully document its MERGE concurrency model (it inherits Redis's
single-threaded command execution within a global lock, making TOCTOU impossible
by construction — a luxury pg_ripple does not have).

**Lesson 10**: The single-threaded Redis execution model conceals many concurrency
bugs that will manifest in pg_ripple's multi-backend PostgreSQL environment.
Do not borrow FalkorDB's MERGE design without accounting for concurrent SPI access
under PostgreSQL MVCC.

### Variable-length paths

GraphBLAS matrix exponentiation directly implements `[*n]` in O(nnz) per level.
For `[*m..n]`, FalkorDB accumulates results between iteration `m` and `n`.
For directed traversal, the forward adjacency matrix is used; for undirected, a
symmetrized matrix (OR of the forward and transposed matrices).

The undirected path `(a)-[*1..3]-(b)` in pg_ripple requires a UNION of the
forward VP scan and the reverse VP scan at each recursion step:

```sql
WITH RECURSIVE path(n, depth, visited) AS (
    -- base: one hop forward
    SELECT o, 1, ARRAY[s] FROM vp_knows WHERE s = @start_id
    UNION ALL
    -- base: one hop backward
    SELECT s, 1, ARRAY[o] FROM vp_knows WHERE o = @start_id
    UNION ALL
    -- recursive forward
    SELECT vp.o, p.depth+1, p.visited || vp.o
    FROM path p JOIN vp_knows vp ON p.n = vp.s
    WHERE p.depth < 3 AND NOT vp.o = ANY(p.visited)
    UNION ALL
    -- recursive backward
    SELECT vp.s, p.depth+1, p.visited || vp.s
    FROM path p JOIN vp_knows vp ON p.n = vp.o
    WHERE p.depth < 3 AND NOT vp.s = ANY(p.visited)
)
SELECT DISTINCT n FROM path WHERE depth BETWEEN 1 AND 3
```

**Lesson 11**: Generate separate recursive CTE arms for directed vs undirected
patterns. Undirected paths double the CTE branches. Avoid materializing the full
path array (`visited`) unless the query requests path objects — for pure
reachability (`MATCH p=(a)-[*1..3]->(b) RETURN b`) the visited-list can be
replaced with a `CYCLE` clause which uses O(1) per row.

---

## 4. Memgraph

Memgraph is a disk-durable in-memory graph database (C++, openCypher).

### Storage model

Memgraph uses a **delta-based in-memory storage** model:

- Graph data held entirely in RAM (vertices + edges as linked objects).
- WAL (write-ahead log) for durability; snapshots for recovery.
- Storage access is governed by *accessors*:
  - **Shared access** (most queries): multiple Cypher queries run in parallel, each
    marked read or write.
  - **Read-only access**: only allows reads; used for consistent snapshots.
  - **Unique access**: exclusive lock; used for DDL (CREATE INDEX, CREATE
    CONSTRAINT, TTL setup, DROP GRAPH).

**Lesson 12**: Memgraph's three-tier access model maps cleanly to PostgreSQL's
lock granularity:

| Memgraph | pg_ripple equivalent |
|---|---|
| Shared access (read) | SELECT on VP tables — no explicit lock needed, MVCC handles it |
| Shared access (write) | INSERT/UPDATE/DELETE on VP tables — row-level locks via MVCC |
| Read-only access | `SET TRANSACTION READ ONLY` |
| Unique access | `LOCK TABLE vp_… IN EXCLUSIVE MODE` or advisory locks |

For MERGE under pg_ripple: use shared write access (the default), plus a
targeted advisory lock on `(predicate_id, subject_encoded_id)` if no uniqueness
constraint covers the MERGE key (see Lesson 1).

### Label indexing

Memgraph supports two index types:

```cypher
CREATE INDEX ON :Person;              -- label index
CREATE INDEX ON :Person(surname);     -- label + property compound index
```

Label-only indexes map `label_token → [vertex_ptr, …]` in sorted order.
Label-property indexes additionally filter and sort by property value.

**Key for pg_ripple**: Memgraph's label-only index is exactly `vp_rdf_type` with
a B-tree on `(o, s)` — the index that maps label_id → set of subject IDs. The
label-property compound index is a JOIN between `vp_rdf_type` and `vp_{property}`
where the PostgreSQL planner can choose the join order based on statistics.

Memgraph explicitly does NOT create indexes automatically when creating
constraints (unlike Neo4j, which creates a range index on `CREATE INDEX …`).
pg_ripple follows the same discipline — constraints and indexes are separately
managed.

**Lesson 13**: For the read-path, the `rdf:type` VP table is both the label
catalog and the label scan index. The ANALYZE GRAPH equivalent in pg_ripple is
`ANALYZE _pg_ripple.vp_rdf_type` — this should be called periodically (or
triggered by the merge background worker) to keep the planner's row estimates
accurate.

### Variable-length paths

Memgraph compiles `[*1..4]` to a built-in DFS/BFS traversal implemented in C++,
not to SQL recursion. Memgraph exposes `*BFS` (breadth-first), `*DFS`
(depth-first), `*WSHORTEST` (weighted shortest), and `*KSHORTEST` (k-shortest)
as Cypher syntax extensions — these are not part of openCypher but are used when
the built-in path algorithms are needed.

Notably, Memgraph does not support Neo4j's newer quantified path pattern
syntax `--{2}` (exact n hops); it uses `[*2]` instead.

**Lesson 14**: The Memgraph divergence list is valuable for TCK compliance
planning. The following constructs are not yet supported in Memgraph (and
therefore commonly omitted in openCypher implementations):

- COUNT/COLLECT subqueries (`COUNT { MATCH … }`)
- Type predicate expressions (`val IS :: INTEGER`)
- `shortestPath` function directly in MATCH patterns (workaround: `MATCH p = (a)-[*BFS]-(b)`)
- Multi-value WHEN in CASE (`WHEN 'a', 'b' THEN …`)

These are also potential deferred features for pg_ripple's first release.

### MERGE concurrency

Memgraph's storage access model means MERGE runs with shared write access, which
does allow parallel Cypher queries. However, Memgraph's in-memory design allows
it to use fine-grained vertex locking (pessimistic) for MERGE, avoiding the
TOCTOU window. Under PostgreSQL's MVCC, there is no equivalent mechanism unless:
1. A unique constraint covers the MERGE key (safe via `ON CONFLICT`), or
2. An advisory lock serializes concurrent MERGEs for the same key.

---

## 5. Apache AGE

Apache AGE (A Graph Extension) is a PostgreSQL extension (C, Apache-2.0) that
adds Cypher query support. It is the most directly comparable prior art since it
also lives inside PostgreSQL.

### Storage model

AGE creates a per-graph PostgreSQL namespace. Inside that namespace:

```sql
-- Parent tables (inheritance roots)
new_graph._ag_label_vertex  (id agtype, properties agtype)
new_graph._ag_label_edge    (id agtype, start_id agtype, end_id agtype,
                              label agtype, properties agtype)

-- Per-label child tables (via PostgreSQL table inheritance)
new_graph."Person"          (id agtype, properties agtype)    -- inherits _ag_label_vertex
new_graph."FATHER_OF"       (id agtype, start_id agtype, end_id agtype,
                              label agtype, properties agtype) -- inherits _ag_label_edge
```

The `agtype` is a custom PostgreSQL type: a superset of JSONB. Every row's
`properties` column is a JSONB-like blob containing all properties as key-value
pairs. This means:

- Property access requires JSONB path extraction: `properties->>'name'`
- No typed property columns; the planner cannot use B-tree indexes on scalar
  properties without additional expression indexes.
- Identity is a 64-bit `graphid` = packed (label_id, sequence_number).

### Why pg_ripple cannot simply query AGE views

AGE's `agtype` blob properties are the opposite of pg_ripple's VP table design.
In AGE, all properties of a node are stored together in one JSONB cell. In
pg_ripple, each property is a separate row in its own VP table.

A conceptual Cypher `MATCH (n:Person) WHERE n.age > 30 RETURN n.name` would
translate in AGE to:

```sql
SELECT properties->>'name'
FROM new_graph."Person"
WHERE (properties->>'age')::int > 30;
```

And in pg_ripple to:

```sql
SELECT dict_decode(vn.o) AS name
FROM _pg_ripple.vp_name vn
WHERE vn.s IN (
    SELECT t.s FROM _pg_ripple.vp_rdf_type t WHERE t.o = @Person_id
)
AND EXISTS (
    SELECT 1 FROM _pg_ripple.vp_age va
    WHERE va.s = vn.s AND va.o > @30_encoded_id
);
```

These are fundamentally different storage shapes. AGE cannot query pg_ripple
views, and pg_ripple cannot query AGE tables, without a translation layer that
crosses the `agtype` ↔ `integer-dictionary` boundary.

**Lesson 15**: AGE's `agtype` approach is a design dead-end for performance. Storing
all properties as a JSON blob prevents:
- B-tree range scans on property values (unless expression indexes are created).
- Efficient cardinality estimation (the planner cannot see inside the JSON blob).
- Typed arithmetic pushdown (all math must be done after JSON extraction).

pg_ripple's VP table design is strictly superior for analytical queries. The
`cypher-algebra` crate must target the VP table model, not `agtype`.

### MERGE semantics

AGE inherits PostgreSQL semantics for writes. Its MERGE implementation calls
`cypher()` → parsed → a Cypher plan → SPI calls. The TOCTOU problem exists in
AGE exactly as it does in any naïve MERGE implementation: two transactions can
both observe "no match" and both insert.

AGE does not document a mitigation for this. From investigating the AGE source
code, MERGE is implemented as a sequential OPTIONAL MATCH → conditional CREATE
without advisory locks — the naive approach.

**Lesson 16**: Do not copy AGE's MERGE implementation. It is racy under concurrent
writes. Use the `ON CONFLICT DO NOTHING … RETURNING` or advisory lock approach
described in Lesson 1.

### Cypher/SQL boundary

AGE exposes Cypher inside PostgreSQL via:

```sql
SELECT * FROM cypher('graph_name', $$ MATCH (n) RETURN n $$) AS (n agtype);
```

The `cypher()` function is a SQL set-returning function. This means:
- Cypher queries cannot be planned jointly with surrounding SQL.
- The PostgreSQL optimizer cannot push predicates from the outer SQL into
  the Cypher subquery.
- Result materialization happens at the `cypher()` boundary — the result set
  is fully materialized before being joined to any SQL.

pg_ripple's Cypher→SQL compiler will NOT use this pattern. Instead, it generates
native SQL that the PostgreSQL planner can optimize holistically. This is a
fundamental architectural advantage over AGE.

**Lesson 17**: The key design decision that separates pg_ripple from AGE is:
*translate Cypher to SQL before the planner sees the query, not at execution time*.
AGE parses and "executes" Cypher inside a SQL function call. pg_ripple translates
Cypher to SQL text, which PostgreSQL's optimizer then plans and executes. This
gives pg_ripple access to:
- Index selection based on VP table statistics.
- Join order optimization across multiple VP tables.
- Predicate pushdown from outer SQL into the generated inner SQL.
- Parallel query execution via PostgreSQL's parallel scan infrastructure.

---

## 6. JanusGraph

JanusGraph is a distributed property graph database (Java, Apache-2.0) that uses
Gremlin (TinkerPop) rather than Cypher. It is included here for its vertex-centric
index model, which maps directly to VP tables.

### Storage model

JanusGraph stores graphs as **adjacency lists in a Bigtable-model**:

- Each vertex = one row in the KV store, keyed by the vertex's 64-bit ID.
- Each edge and property = one cell in that row.
- Cells are sorted by column value (the column encodes edge label + sort key +
  adjacent vertex delta-id + edge ID).

This is the complete opposite of VP tables:

| JanusGraph | pg_ripple VP tables |
|---|---|
| Vertex-centric: all edges leave one row | Predicate-centric: all edges of one type in one table |
| Edge lookup by vertex = row scan | Edge lookup by predicate = table scan |
| Find all knows-edges of Alice = Alice's row | Find all knows-edges = `SELECT * FROM vp_knows WHERE s = alice_id` |
| Property = one cell per vertex | Property = one row per subject in VP table |

**Lesson 18**: JanusGraph's "vertex-centric index" concept is directly analogous
to what VP tables provide. A vertex-centric index in JanusGraph is a sorted
ordering of a vertex's adjacency list by (edge label, property value) — allowing
efficient range queries such as "find all KNOWS edges from Alice where `since > 2020`".
In pg_ripple, this is:
```sql
SELECT * FROM vp_knows WHERE s = alice_id;  -- already efficient via B-tree(s,o)
-- Filter on edge property (requires RDF-star)
SELECT k.s, k.o
FROM vp_knows k
JOIN vp_since si ON si.s = edge_hash(k.s, knows_id, k.o)
WHERE k.s = alice_id AND si.o > @2020_id;
```
The VP table is already a kind of vertex-centric index for each edge type.

### Edge properties

JanusGraph stores edge properties in the cell value of the edge cell (compressed
serialization). The column encodes the sort key; the value carries unsigned
properties.

Every edge in JanusGraph has intrinsic identity (a unique edge ID stored in the
column). This allows edges to carry properties natively. VP tables store `(s, o)`
pairs — no intrinsic edge identity.

**Lesson 19**: JanusGraph's edge-identity model confirms that edge properties are
fundamentally dependent on an explicit edge identifier. RDF-star maps cleanly to
this: `hash(s, predicate_id, o)` → the edge identifier = what JanusGraph calls
the edge ID. The edge hash is deterministic and reproducible, matching JanusGraph's
edge ID semantics (stored in the cell column).

### Vertex-centric index strategy

JanusGraph's `vertex-centric index` allows sorting the adjacency list within a
vertex row by properties of the outgoing edges — for example, sort KNOWS edges by
`since` date. This enables range queries on edge properties without a global scan.

```
mgmt.buildEdgeIndex(knows, 'knowsByDate', Direction.OUT,
    Order.desc, since)
```

With this index: find all KNOWS edges from Alice since 2020 = O(log n) scan of
Alice's sorted KNOWS list.

**Lesson 20**: In pg_ripple, the dual B-tree indexes on each VP table (one on `(s, o)`,
one on `(o, s)`) already provide O(log n) forward and backward edge lookup. For
edge property filtering (via RDF-star), a compound index on the annotation VP table:
`CREATE INDEX ON vp_since (s, o)` — where `s = edge_hash` — provides the same
effect as JanusGraph's edge-centric index.

---

## 7. Dgraph

Dgraph is a distributed graph database (Go, BSL) with its own DQL (Dgraph Query
Language) based on GraphQL syntax. No Cypher support.

*Note: dgraph.io documentation returned 404 for the requested URLs.*

From published papers and prior knowledge:

### Storage model

Dgraph uses **posting lists** — a structure identical in concept to an inverted
index:

- Each predicate (edge type or property name) has its own posting list.
- A posting list for predicate `knows` contains, for each subject, the list of
  objects: `{alice: [bob, carol], bob: [dave, …], …}`.
- This is a subject-grouped vertical partition by predicate.
- **This is exactly what VP tables are.**

Dgraph encodes all subject and object IRIs/values as 64-bit UIDs using a hash
function — **exactly what pg_ripple's XXH3-128 dictionary encoder does**.

**Lesson 21 (direct validation)**: Dgraph independently arrived at the same
storage architecture as pg_ripple: dictionary-encoded integer IDs + vertical
partitioning by predicate. This is strong architectural validation. The VP table
design is not novel; it is the convergent solution to the graph storage
problem that multiple systems have independently discovered.

Dgraph's posting list corresponds to:
```
vp_{predicate}(s BIGINT, o BIGINT)  -- pg_ripple VP table
```

The primary difference: Dgraph stores posting lists as sorted byte arrays
(delta-compressed UIDs) in a RocksDB KV store, while pg_ripple stores them as
heap rows in PostgreSQL with B-tree indexes. The access patterns are the same;
the physical representation differs.

### Concurrency

Dgraph uses distributed MVCC (based on Raft consensus + timestamped transactions).
Not directly applicable to single-node pg_ripple, but confirms that the
dictionary-encoded VP approach scales to distributed settings.

---

## 8. NebulaGraph

NebulaGraph is a distributed property graph database (C++, Apache-2.0) with a
hybrid query language: native nGQL + openCypher-compatible clauses.

### Storage model

NebulaGraph uses a **strong-schema partitioned** model:

- **TAG** = vertex type with declared property columns (mandatory schema definition
  before insert).
- **EDGE TYPE** = edge type with declared property columns.
- Vertex identity = **VID** — globally unique, user-specified (int64 or fixed-length
  string), NOT auto-assigned.
- **Sharding**: data is partitioned by `hash(VID) mod num_parts`. All edges of a
  vertex are co-located with the vertex's shard.
- Backend: RocksDB per storage node.

VID-based sharding is analogous to pg_ripple's subject-based dictionary grouping.
The critical difference: VIDs are user-visible and structural (used in edge
definitions directly). pg_ripple's dictionary IDs are internal and transparent.

**Lesson 22**: NebulaGraph's experience with VID-based sharding shows that using
the encoded integer as the primary routing key works well for locality. pg_ripple's
dictionary IDs are already integers. If distributed pg_ripple were built on top of
pg_ripple partitioning, the subject dictionary ID would be a natural shard key —
consistent with NebulaGraph's validated approach.

### Schema and label indexing

NebulaGraph separates two concepts that other systems conflate:

- **TAG**: defines the vertex type and its property schema (like a class definition).
- **LABEL** (in native nGQL): a runtime label used for filtering — must have an
  explicit index created before use.

In openCypher-compatible clauses, `MATCH (n:Person)` uses a tag lookup — but
the "label" (tag) cannot be used as an ad-hoc filter without an explicit index.

This is critically different from Neo4j's model where labels are always indexed
via the token lookup index.

**Lesson 23**: NebulaGraph's separation of schema type (TAG) from label index is
a useful model for pg_ripple's schema layer. A SHACL NodeShape defines the vertex
type (analogous to TAG); the `rdf:type` VP table serves as the label index.
`MATCH (n:Person)` always works in pg_ripple because `vp_rdf_type` is always
present and indexed. No separate label index creation step is required.

### nGQL vs openCypher compatibility

NebulaGraph takes a hybrid approach: native nGQL for DDL and DML; openCypher for
DQL only (read queries). The openCypher subset is explicit:

- `MATCH`, `OPTIONAL MATCH`, `WITH`, `RETURN`, `WHERE`, `ORDER BY`, `SKIP`, `LIMIT` → supported
- `CREATE`, `MERGE`, `SET`, `DELETE` → NOT supported in openCypher compat mode;
  use native nGQL `INSERT VERTEX`, `UPSERT VERTEX`, etc.

**Lesson 24**: NebulaGraph's hybrid approach is a viable phased strategy for
pg_ripple. Phase 1: implement Cypher DQL (read-only `MATCH`/`RETURN`/`WITH`/`WHERE`).
Phase 2 (requires RDF-star v0.4.0): add Cypher DML (`CREATE`, `MERGE`, `SET`,
`DELETE`). This maps to the ROADMAP boundary and avoids blocking the initial
release on write semantics.

### Edge rank

NebulaGraph introduces an "edge rank" concept: between the same `(src, dst, type)`
triple, multiple edges can coexist, distinguished by a 64-bit rank value. This is
used for multi-temporal edges (e.g., multiple employment periods at the same
company).

In RDF, this corresponds to named graphs or RDF-star reification with a
timestamp property. pg_ripple's quad store (`s, p, o, g`) already supports named
graphs (the `g BIGINT` column), which covers the rank use case.

**Lesson 25**: Edge rank in NebulaGraph = named graph in pg_ripple. The `g` column
in VP quads maps naturally to this. A Cypher edge with an explicit rank property:
```cypher
MATCH (a)-[r:WORKED_AT {year: 2022}]->(c)
```
maps under RDF-star to:
```sparql
?edge :year 2022 .
FILTER (STRSTARTS(STR(?edge), "..."))  # edge is the triple hash
```
The named graph `g` can also serve as the rank directly if the user wishes.

---

## 9. Cross-cutting synthesis

### 9.1 Node label indexing — convergent solution

Every system surveyed independently arrives at the same solution: a separate
label-keyed index (or a predicate-partitioned structure) that maps
`label_id → [node_id, …]`.

| System | Label index mechanism |
|---|---|
| Neo4j | Token lookup index (B-tree: `label_id → sorted(node_id)`) |
| Kuzu | Separate node table per label; primary index on PK |
| FalkorDB | Sparse label vector (one column per label, rows = vertex IDs) |
| Memgraph | Explicit `CREATE INDEX ON :Label` (hash or range) |
| AGE | Per-label PostgreSQL table (child of `_ag_label_vertex`) |
| JanusGraph | Global vertex index: `(label, vertex_id)` in external index store |
| NebulaGraph | TAG lookup by VID hash + explicit native index for range queries |
| pg_ripple | `vp_rdf_type(s, o)` with B-tree on `(o, s)` — already present |

**pg_ripple is already correct.** No new label index infrastructure is needed.
The `MATCH (n:Person)` pattern compiles to:
```sql
SELECT s FROM _pg_ripple.vp_rdf_type WHERE o = @Person_id
```
and benefits from the existing `(o, s)` B-tree.

For multi-label intersection (`n:A:B`): JOIN `vp_rdf_type` twice:
```sql
SELECT t1.s
FROM _pg_ripple.vp_rdf_type t1
JOIN _pg_ripple.vp_rdf_type t2 ON t1.s = t2.s
WHERE t1.o = @A_id AND t2.o = @B_id
```

For multi-label union (`n:A|B`): UNION:
```sql
SELECT s FROM _pg_ripple.vp_rdf_type WHERE o = @A_id
UNION
SELECT s FROM _pg_ripple.vp_rdf_type WHERE o = @B_id
```

### 9.2 MERGE under concurrent writes — the consensus

Every system that operates under concurrent write access (Neo4j, Memgraph,
Dgraph, NebulaGraph) independently documents that MERGE without a uniqueness
constraint is not safe.

The safe patterns, ranked by preference for pg_ripple:

1. **Uniqueness constraint on the MERGE key** → translate to:
   ```sql
   INSERT INTO _pg_ripple.vp_{predicate}(s, g, o)
   VALUES (@s, @g, @o)
   ON CONFLICT (s, g) DO NOTHING
   RETURNING s;
   -- Then read back the existing row if nothing returned
   ```
   This is atomic and requires no external locking.

2. **No uniqueness constraint** → use advisory lock to serialize:
   ```sql
   SELECT pg_advisory_xact_lock(@predicate_id, @subject_id);
   -- then OPTIONAL MATCH + conditional INSERT
   ```

3. **Never**: SELECT to check existence, then INSERT in a separate step without
   locking. This is the race condition that AGE has and that every documented
   system warns against.

### 9.3 Variable-length path compilation — cross-system comparison

| System | `[*1..n]` implementation |
|---|---|
| Neo4j | `VarLengthExpand` physical operator (custom BFS/DFS in Java) |
| Kuzu | `RecursiveJoin` physical operator (BFS/DFS in C++) |
| FalkorDB | GraphBLAS sparse matrix power iteration |
| Memgraph | Built-in DFS/BFS traversal (`*BFS`, `*DFS`) |
| AGE | Compiled to SQL recursive CTE (via `cypher()` function) |
| JanusGraph | TinkerPop `repeat().times(n)` → Gremlin traversal steps |
| pg_ripple | `WITH RECURSIVE … CYCLE` (PostgreSQL 18 native) |

The consensus is that purpose-built traversal engines (Neo4j, Kuzu, Memgraph,
FalkorDB) outperform SQL recursion for deep traversals (depth > 5-6).
**For pg_ripple, this means bounded short paths (`[*1..3]`) will be efficient;
deep traversals (`[*1..50]`) will be slower than a native graph engine.**

This is an acceptable tradeoff for an RDF/SPARQL-first system that also
supports Cypher. Document this limitation explicitly.

**pg_ripple's CTE template for `(a)-[*m..n:TYPE]->(b)`**:
```sql
WITH RECURSIVE rpath(n, depth) AS (
  -- base case
  SELECT o AS n, 1 AS depth
  FROM _pg_ripple.vp_{type_id}
  WHERE s = @a_encoded

  UNION ALL

  -- recursive case
  SELECT vp.o AS n, rp.depth + 1 AS depth
  FROM rpath rp
  JOIN _pg_ripple.vp_{type_id} vp ON rp.n = vp.s
  WHERE rp.depth < @n_bound
)
CYCLE n SET is_cycle USING path
SELECT DISTINCT n
FROM rpath
WHERE depth >= @m_bound
  AND NOT is_cycle
  AND (@b_encoded IS NULL OR n = @b_encoded);
```

### 9.4 Compiler separation — lessons from each system

| System | Separation | Lesson |
|---|---|---|
| Neo4j | Parser → Logical Plan → Compiler → Runtime (4 stages, internal) | The logical/physical split is essential |
| Kuzu | cypher-parser → binder → planner → physical plan (clean module boundary in C++ source) | Binder is storage-aware; planner is algebraic |
| FalkorDB | Parser tightly coupled to GraphBLAS execution model | Do not do this |
| Memgraph | openCypher parser + plan tree (C++ separate modules) | Readable reference for operator model |
| AGE | Parser in C, plan in PostgreSQL SRF context (tightly coupled) | Do not do this |
| pg_ripple (proposed) | `cypher-algebra` crate (pure algebra, no storage) + `src/cypher/` (SQL emission) | Clean separation confirmed as correct |

The universal lesson is: **the algebraic layer must be storage-agnostic**. Storage
details (which VP table, which OID, encoded constants) belong in the binding/translation
layer, not in the parser or the algebra representation.

### 9.5 LPG ↔ RDF convergence points

All eight systems confirm the same convergence observation:

| LPG concept | RDF equivalent in pg_ripple |
|---|---|
| Node label | `rdf:type` triple |
| Node property | VP table row `(node_id, property_value_id)` |
| Edge type | VP table identifier / predicate IRI |
| Edge endpoint | `s` (subject) and `o` (object) columns |
| Edge property | RDF-star annotation: `<<s p o>> q v` |
| Node identity | Dictionary-encoded IRI (`i64`) |
| Multi-label | Multiple `rdf:type` triples for the same subject |
| Typed edge with properties | RDF-star + one VP table per edge property |

The only genuine gap (already identified in `cypher_lpg_analysis.md`) is edge
properties, which require RDF-star. Every system surveyed that supports edge
properties uses a dedicated edge identity mechanism:

- Neo4j: relationship record with unique ID
- Kuzu: edge table with explicit property columns
- JanusGraph: edge cell column encodes a unique edge ID
- AGE: `id` field in the `agtype` edge blob
- NebulaGraph: `(src, dst, type, rank)` composite key

All of these are logically equivalent to `hash(s, predicate_id, o)` = the
RDF-star edge identifier used by pg_ripple.

### 9.6 Systems NOT to emulate

The following anti-patterns should be explicitly avoided:

| Anti-pattern | Seen in | Reason to avoid |
|---|---|---|
| Store properties as JSONB blob | AGE (`agtype`) | Prevents B-tree pushdown, obscures cardinality |
| MERGE without uniqueness constraint or advisory lock | AGE | Race condition under concurrent writes |
| Cypher query boundary that prevents cross-optimization | AGE (`cypher()` SRF) | Prevents join order optimization with outer SQL |
| String comparison in storage scans | — | All values must be dictionary-encoded before query execution (AGENTS.md constraint) |
| `[*]` unbounded path without CYCLE detection | — | Infinite loop risk on graphs with cycles |

---

## 10. Implications for pg_ripple implementation priorities

Based on all eight systems, the following implementation order is recommended:

### Phase 1: Read-only Cypher (no RDF-star dependency)

1. `cypher-algebra` crate: parser + algebra IR for `MATCH`, `RETURN`, `WHERE`, `WITH`, `OPTIONAL MATCH`, `ORDER BY`, `SKIP`, `LIMIT`, `UNWIND`.
2. `src/cypher/translator.rs`: algebra → SQL compilation for:
   - Node pattern `(n:Label {k: v})` → `vp_rdf_type` JOIN + VP property JOINs
   - Edge pattern `(a)-[:TYPE]->(b)` → VP table JOIN
   - Variable-length path `[*m..n:TYPE]` → recursive CTE with CYCLE
   - Filter pushdown (encode constants at translation time)
   - Star-pattern optimization (same subject, multiple predicates → single join chain)
3. openCypher TCK pass rate target: ≥80% of `Match` and `Return` feature files.

### Phase 2: Write Cypher (requires RDF-star, v0.4.0)

4. `src/cypher/writer.rs`: `CREATE`, `MERGE` (with advisory lock), `SET`, `REMOVE`, `DELETE`, `DETACH DELETE`.
5. Edge property support via RDF-star annotation VP tables.

### Phase 3: Advanced features

6. Shortest path (`shortestPath`, `allShortestPaths`)
7. ISO GQL quantified path patterns (Cypher 25 `{m,n}`)
8. Full openCypher TCK compliance target ≥95%.

---

*Sources: Neo4j Cypher Manual (April 2026), Kuzu documentation and VLDB 2023 paper, FalkorDB documentation and blog, Memgraph documentation, Apache AGE documentation, JanusGraph documentation, Dgraph design documentation (prior knowledge), NebulaGraph documentation.*
