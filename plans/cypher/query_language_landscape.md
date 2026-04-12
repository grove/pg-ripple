# Graph Query Language Landscape — openCypher, GQL, Gremlin, and SPARQL

> **Status**: Exploratory analysis — not yet committed to the roadmap.
> Companion to [cypher_lpg_analysis.md](cypher_lpg_analysis.md) and
> [prior_art_graph_systems.md](prior_art_graph_systems.md).
> Written April 2026.

---

## 1. The Four Languages at a Glance

| Language | Standard body | Model | Style | Status |
|---|---|---|---|---|
| **SPARQL 1.1** | W3C | RDF quads | Declarative, set-based | W3C Recommendation 2013; SPARQL 1.2 in progress |
| **openCypher** | openCypher.org (Neo4j) | LPG | Declarative, pattern-based | Last standalone release 2024.2; project now tracks GQL |
| **GQL** | ISO/IEC SC32 WG3 | LPG | Declarative, pattern-based | ISO/IEC 39075:2024, published April 2024 |
| **Gremlin** | Apache TinkerPop | LPG | **Imperative**, traversal-based | Apache community standard; TinkerPop 3.7.x |

The critical observation: openCypher and GQL are **converging**. The openCypher
project website describes openCypher 9 as "the last release before the openCypher
project hit the road to GQL." Neo4j's Cypher 25 (released 2025) is explicitly
aligned with GQL. For practical purposes, **openCypher is a dialect of GQL**, and
supporting GQL's read/write core subsumes openCypher.

Gremlin is categorically different from the other three: it is an imperative
traversal pipeline, not a declarative pattern-matching language.

---

## 2. What "Transpile to SPARQL" Means — and Whether It Works

### 2.1 SPARQL algebra as the common IR

SPARQL 1.1 defines a formal algebra (BGP, Join, LeftJoin, Filter, Union, Extend,
Minus, Project, Distinct, Reduced, Slice, ToMultiSet, ToList, OrderBy, Group,
Having). This algebra is expressive enough to represent all the operators in
openCypher's and GQL's read path. Stardog's Cypher endpoint, before it was
deprecated, compiled Cypher → SPARQL algebra → execute. This is technically viable.

However, the right framing for pg_triple is **not** "transpile to SPARQL" but
rather "transpile to a common graph algebra IR that maps to VP-table SQL." Whether
that IR is literally SPARQL algebra or pg_triple's own defined algebra is an
implementation choice, not a correctness requirement. The SQL/SPI layer is the
true common target.

Using SPARQL algebra as the canonical IR has one concrete advantage: spargebra
already exists, is well-tested, and pg_triple's `src/sparql/` pipeline already
lowers it to SQL. A Cypher/GQL front-end that produces SPARQL algebra IR would
reuse the entire SQL emission path at zero extra cost.

### 2.2 openCypher / GQL → SPARQL algebra mapping (read path)

Most of openCypher/GQL's read surface maps cleanly:

| openCypher / GQL | SPARQL algebra equivalent |
|---|---|
| `MATCH (n:Label)` | BGP: `?n rdf:type :Label` |
| `MATCH (a)-[:TYPE]->(b)` | BGP: `?a :type ?b` |
| `WHERE n.age > 30` | Filter(BGP, `?age > 30`) + BGP `?n :age ?age` |
| `OPTIONAL MATCH` | LeftJoin |
| `RETURN n.name AS name` | Project(Extend(...)) |
| `WITH x, count(*) AS c` | Group + Having + Project |
| `UNION` | Union (set) |
| `UNION ALL` | Union (bag / ToMultiSet) |
| `ORDER BY / SKIP / LIMIT` | OrderBy + Slice |
| `UNWIND list AS x` | Extend (lateral unnest) |
| `[*m..n:TYPE]` | Property path with quantified repeat |
| `shortestPath(...)` | Not in SPARQL 1.1 algebra; needs extension operator |

**What does not map cleanly (read path):**

- **Cypher/GQL variable scoping**: `WITH` in Cypher resets the variable scope;
  only explicitly projected variables are carried forward. SPARQL scoping is
  different within subqueries. This is a semantic difference the translation layer
  must compensate for.
- **Null semantics**: Cypher null propagates through expressions; SPARQL uses
  UNDEF which has different coercion behaviour in FILTER.
- **List literals and list comprehensions**: `[x IN list WHERE ... | expr]` has
  no direct SPARQL algebra operator (SPARQL uses VALUES/BIND/UNNEST).
- **`shortestPath()` / `allShortestPaths()`**: SPARQL property paths do not
  expose path objects or path lengths. This requires an extension operator in
  the algebra IR.
- **`MERGE`**: No algebra equivalent in SPARQL algebra. Belongs to the write path.

**Conclusion on read-path transpilation**: A `cypher-algebra` crate that produces
an IR *closely related to* SPARQL algebra (with a small set of extensions for
Cypher-specific operators) can reuse the `src/sparql/` SQL emission path for
most operators. The extension operators need new SQL templates, not a new pipeline.

### 2.3 openCypher / GQL → write semantics

SPARQL Update (INSERT DATA, DELETE DATA, INSERT/DELETE WHERE) is a separate
standard from SPARQL query algebra. Cypher's write surface (`CREATE`, `MERGE`,
`SET`, `REMOVE`, `DELETE`) has no SPARQL algebra representation and must route
through a separate write compiler (`src/cypher/writer.rs`, documented in
[cypher_lpg_analysis.md](cypher_lpg_analysis.md)).

The "transpile to SPARQL Update" option exists but is awkward: `MERGE (n:Person
{name: 'Alice'})` would need to become a SPARQL `INSERT ... WHERE NOT EXISTS {
... }` conditional, which is semantically closer but still not atomic under
concurrent writes. The safer path is native `INSERT ... ON CONFLICT DO NOTHING
... RETURNING` via SPI, as documented in the prior analysis.

---

## 3. openCypher and GQL — Will They Diverge Again?

GQL (ISO/IEC 39075:2024) was heavily influenced by openCypher, PGQL (Oracle), GSQL
(TigerGraph), and G-CORE (academic consortium). It absorbs and standardizes them.

### Syntax overlap

GQL's **Graph Pattern Matching (GPM)** layer — the `MATCH ... WHERE ... RETURN`
core — is documented by the standards committee to be "essentially identical" to
GPM in SQL/PGQ. Neo4j designed Cypher 25 to be GQL-compatible; the openCypher
project stopped independent releases and defers to GQL going forward.

For pg_triple: a `cypher-algebra` parser written against the GQL grammar will
accept all openCypher queries without modification for the GPM subset, which
covers ~90% of practical read workloads.

### Where GQL diverges from openCypher

| Feature | openCypher 9 | GQL (ISO/IEC 39075:2024) |
|---|---|---|
| Node/edge identity requirement | Implicit (no mandatory ID) | Mandatory element identity (IDs required) |
| Schema / graph types | Optional, schema-free | Formal `GRAPH TYPE` DDL |
| Path pattern quantification | `[*m..n]` (Cypher extension) | `{m,n}` in quantified path pattern (GPC) — formally standardised |
| NEXT clause | Not present; uses `WITH` | Explicit `NEXT` for sequential step composition |
| Mutation syntax | `CREATE`, `MERGE`, `SET`, `DELETE` | `INSERT`, `SET`, `REMOVE`, `DELETE` (similar but formal) |
| Composite queries | `UNION` at result level | `UNION`, `INTERSECT`, `EXCEPT` as first-class query combinators |
| `RETURN *` | Supported | `RETURN *` — all bound variables |
| Multi-graph queries | Via `USE` in some extensions | `MATCH` from multiple named graphs is built-in |

**The GQL mutation syntax difference is the only practical parser-level
divergence**: GQL uses `INSERT` where openCypher uses `CREATE`, and GQL omits
`MERGE` from the core standard (leaving upsert semantics to implementations).
Everything else is syntactic sugar or additive.

For a `cypher-algebra` crate: parse both `CREATE ...` and `INSERT INTO ...` as
the same `Statement::Insert` AST node. The grammar has two branches; the algebra
IR is the same. This is not hard.

### SQL/PGQ — the silent fifth option

SQL:2023 Part 16 (SQL/PGQ — Property Graph Queries in SQL) embeds GQL's GPM
inside SQL via a `GRAPH_TABLE()` function:

```sql
SELECT person_name, friend_name
FROM GRAPH_TABLE(
    social_graph
    MATCH (p:Person)-[:KNOWS]->(f:Person)
    COLUMNS (p.name AS person_name, f.name AS friend_name)
);
```

This is **native SQL**, and PostgreSQL 18 may eventually implement it. For
pg_triple, `GRAPH_TABLE()` over a view layer on VP tables is a plausible future
path. It is not a query language pg_triple needs to implement (PostgreSQL itself
would handle it), but it is relevant to how pg_triple exposes its LPG structure.

SQL/PGQ is worth tracking but not implementing. It is mentioned here for
completeness.

---

## 4. Gremlin — The Outlier

### 4.1 Why Gremlin is fundamentally different

Gremlin is Apache TinkerPop's **graph traversal language** (GTL). Unlike SPARQL,
openCypher, and GQL — which are declarative — Gremlin is **imperative and
functional**. A Gremlin query is a sequence of step-by-step traversal operations
applied to a stream of graph elements:

```groovy
g.V()                              // all vertices
 .has('name', 'Alice')             // filter to Alice
 .out('KNOWS')                     // traverse outgoing KNOWS edges
 .out('KNOWS')                     // hop again
 .dedup()                          // deduplicate endpoints
 .values('name')                   // project the name property
```

This looks translatable. For simple patterns it is. For complex patterns it is not.

### 4.2 What can be translated to declarative SQL

| Gremlin step | SQL equivalent | Translatable? |
|---|---|---|
| `g.V().has('label', val)` | `SELECT s FROM vp_label WHERE o = val_id` | Yes |
| `.out('TYPE')` | `JOIN vp_type ON ...` | Yes |
| `.in('TYPE')` | Reverse join | Yes |
| `.both('TYPE')` | UNION of forward + reverse | Yes |
| `.filter(...)` | WHERE clause | Yes (limited predicates) |
| `.dedup()` | DISTINCT | Yes |
| `.order().by(...)` | ORDER BY | Yes |
| `.limit(n)` | LIMIT | Yes |
| `.count()` | COUNT(*) | Yes |
| `.group().by('x').by(count())` | GROUP BY + COUNT | Yes |
| `.repeat(out(...)).times(n)` | WITH RECURSIVE + depth counter | Yes |
| `.repeat(out(...)).until(has('x', val))` | WITH RECURSIVE + termination condition | Yes (bounded) |
| `.path()` | path materialization alongside result | Complex but feasible |

### 4.3 What cannot be translated

| Gremlin step | What it does | Translatable? |
|---|---|---|
| `.sack(assign).by('age')` | Per-path state variable | **No** — no SPARQL/SQL equivalent |
| `.store('x')` / `.aggregate('x')` | Lazy/eager side-effect collection into a named list | **No** |
| `.cap('x')` | Flush a side-effect accumulator | **No** |
| `.branch(...).option(...)` | Multi-way conditional traversal split | **Partial** (CASE, but not recursive) |
| `.choose(pred, true-step, false-step)` | Inline conditional execution | **Partial** |
| `.subgraph('x')` | Extract a subgraph as a side-effect | **No** |
| `.inject(values...)` | Inject literal values into the pipeline | **Partial** (VALUES in SQL) |

The `sack()` step is the clearest example of Gremlin's fundamentally procedural
nature: it maintains a per-traverser (per-path) mutable variable that can be read
and written at each step. There is no set-based or relational equivalent.

**Academic confirmation**: Hartig and Hidders (2019, "Defining Schemas for
Property Graphs") demonstrated that Gremlin's traversal semantics with multiplicity
(paths with repeated nodes/edges are counted separately) differ semantically from
SPARQL's bag semantics. A faithful translation requires preserving path identity,
which standard SQL does not expose.

### 4.4 The Gremlin user base — is it worth partial support?

Gremlin's primary users are JanusGraph, Amazon Neptune (original interface),
CosmosDB (Gremlin API), and TinkerPop-based systems. These users are migrating to
openCypher at a significant rate (Neptune added openCypher in 2023 precisely
because Gremlin was seen as difficult). JanusGraph has an openCypher adapter.

**The translatable subset of Gremlin** (the "declarative Gremlin" used by most
practical queries) covers ~70–80% of real-world workloads. The untranslatable
features (`sack`, `store`, `aggregate`, `cap`) are advanced traversal patterns
used by specialists.

### 4.5 Recommendation on Gremlin support

**Defer entirely and do not plan for it.** The translatable subset is covered by
openCypher/GQL. The untranslatable features require a novel execution semantics.
The user base is migrating away from Gremlin. The effort of implementing even a
partial Gremlin frontend is not justified relative to the value.

If a Gremlin-compatible interface is ever needed, the correct implementation path
is a **Gremlin→GQL compiler** (translate the declarative Gremlin subset to GQL
patterns, then use the GQL pipeline) — not a Gremlin→SQL compiler built directly
into pg_triple. This is a separate external project.

---

## 5. Recommended Architecture — One Pipeline, Multiple Front-Ends

### 5.1 The common algebra IR

Rather than transpiling to SPARQL and then to SQL, the cleanest architecture is a
**unified graph algebra IR** that is a strict superset of SPARQL algebra and adds
the small set of operators needed for Cypher/GQL:

```
SPARQL algebra operators (all existing):
  BGP, Join, LeftJoin, Filter, Union, Extend, Minus,
  Project, Distinct, Reduced, Slice, ToMultiSet, ToList,
  OrderBy, Group, Having

Additional operators for Cypher/GQL:
  NodeScan(label_id, var)              -- MATCH (n:Label)
  EdgeExpand(var, type_id, dir, var)   -- -[:TYPE]->
  PathRepeat(pattern, min, max, mode)  -- [*m..n], trail vs. walk vs. simple
  ShortestPath(src, dst, type, dir)    -- shortestPath(...)
  Unwind(expr, var)                    -- UNWIND list AS x
  MergeNode(label_id, key_props)       -- MERGE (n:Label {key: val})
  CreateNode(label_id, props)          -- CREATE (n:Label {props})
  SetProp(var, prop_id, expr)          -- SET n.prop = val
  DeleteNode(var, detach)              -- DELETE / DETACH DELETE
```

These operators are a small extension. Their SQL templates are documented in
[prior_art_graph_systems.md §9](prior_art_graph_systems.md).

### 5.2 Front-end crates and translation chains

```
SPARQL text ──► src/sparql/ (spargebra) ──────────────────────────────────┐
                                                                            │
openCypher text ──► cypher-algebra crate ──► translate to IR ─────────────┤
                                                                            │
GQL text ──────► cypher-algebra crate ──► translate to IR ─────────────────┤
                  (same crate, two grammar                                  │
                   branches; same AST nodes)                                │
                                                                            ▼
Gremlin text ──► [NOT SUPPORTED / external Gremlin→GQL tool]        graph algebra IR
                                                                            │
                                                                 src/sparql/algebra.rs
                                                                 (extended with graph ops)
                                                                            │
                                                               src/sparql/translator.rs
                                                               (SQL emission; extended
                                                                for new operators)
                                                                            │
                                                                      SPI → PostgreSQL
```

**Key properties of this design:**

- `cypher-algebra` is a standalone published Rust crate. It understands both
  openCypher 9 syntax and GQL syntax and produces **the same IR** for equivalent
  queries. A `MATCH`/`RETURN` query in openCypher and the equivalent `MATCH`/`RETURN`
  in GQL produce identical algebra IR nodes.
- The SQL emitter in `src/sparql/translator.rs` is extended once with the new
  operators. There is no second SQL emitter.
- SPARQL queries continue to flow through `spargebra` unchanged.

### 5.3 What the `cypher-algebra` crate must handle for both openCypher and GQL

| Construct | openCypher syntax | GQL syntax | IR node |
|---|---|---|---|
| Create node | `CREATE (n:Label {k:v})` | `INSERT (n:Label {k:v})` | `CreateNode` |
| Pattern match | `MATCH (n:Label)` | `MATCH (n:Label)` | `NodeScan` |
| Edge match | `MATCH (a)-[:T]->(b)` | `MATCH (a)-[e:T]->(b)` | `EdgeExpand` |
| Optional | `OPTIONAL MATCH` | `OPTIONAL MATCH` | `LeftJoin` |
| Filter | `WHERE` | `WHERE` | `Filter` |
| Projection | `RETURN` | `RETURN` | `Project` |
| Aggregation | `WITH x, count(*) AS c` | `WITH x, count(*) AS c` | `Group` |
| Chaining | `WITH x ...` | `NEXT` or `WITH x ...` | `Sequence` |
| Variable path | `[*m..n:T]` | `(n WHERE …)-[e:T]->{m,n}(m)` | `PathRepeat` |
| Union | `UNION` / `UNION ALL` | `UNION` / `UNION ALL` | `Union` |
| Upsert | `MERGE (n:Label {k:v})` | Implementation-defined | `MergeNode` |

The grammar has two branches (openCypher and GQL); the AST normalises both to the
same IR. This is about 30% more grammar work but zero additional translator work.

---

## 6. Effort Estimate — All Three Languages Together

Implementing GQL + openCypher together (shared `cypher-algebra` crate with two
grammar branches):

| Component | openCypher only | + GQL grammar branch | Total |
|---|---|---|---|
| `cypher-algebra` crate (grammar + AST + IR) | 12–16 pw | +3–4 pw | 15–20 pw |
| `src/cypher/translator.rs` (read path) | 8–12 pw | 0 pw (shared IR) | 8–12 pw |
| `src/cypher/writer.rs` (write path) | 8–10 pw | +1–2 pw (INSERT syntax) | 9–12 pw |
| TCK compliance (openCypher TCK ~2000 scenarios) | 6–8 pw | — | 6–8 pw |
| GQL basic conformance testing | — | 3–4 pw | 3–4 pw |
| **Total** | **34–46 pw** | **+7–10 pw** | **41–56 pw** |

vs. the openCypher-only estimate from the core analysis (~36–49 pw): adding GQL
costs 5–7 pw on top. That is the grammar extension and an additional conformance
test suite.

**Gremlin**: not recommended. Estimated 30–40 pw for the translatable subset, with
fundamental correctness gaps in the untranslatable features. Not worth it.

---

## 7. Verdict

| Language | Support? | Why | When |
|---|---|---|---|
| **SPARQL** | Yes | Core feature; already planned | v0.3.0 onward |
| **openCypher** | Yes | Practical dialect; most tooling targets it | Post-1.0, Phase 1 |
| **GQL** | Yes, together with openCypher | Same crate, 2 grammar branches, trivial marginal cost | Post-1.0, Phase 1 |
| **SQL/PGQ** | Tracking only | PostgreSQL will implement; pg_triple should ensure VP tables are exposable as PGQ graph views | V2.x |
| **Gremlin** | No | Imperative model; untranslatable features; user base migrating to openCypher | Never / external tool |

The answer to "can all be transpiled to SPARQL" is: **openCypher and GQL can be
compiled to a superset of SPARQL algebra, and that IR lowers to the same VP-table
SQL.** The phrase "transpile to SPARQL" is correct directionally but the precise
answer is "translate to the unified graph algebra IR that pg_triple's SQL emitter
already understands, with a small number of new operators added."

Gremlin cannot be faithfully transpiled to any declarative algebra due to its
per-path state (`sack`, `store`, `cap`) and side-effect model.

---

## 8. Open Questions

1. **`cypher-algebra` crate scope**: should the crate parse both openCypher 9 and
   GQL in its 1.0 release, or ship openCypher 9 first and add GQL in a minor
   version? Given GQL is the live standard and openCypher 9 is frozen, targeting
   GQL as the primary grammar with openCypher 9 as a compatibility dialect may be
   more future-proof.

2. **`MERGE` in GQL**: GQL's ISO standard does not include MERGE in the core
   language (it is implementation-defined). Should pg_triple support `MERGE` as an
   openCypher compatibility feature but mark it as non-standard in GQL mode?

3. **SQL/PGQ `GRAPH_TABLE()` integration**: PostgreSQL 18 may add `GRAPH_TABLE()`
   in a future point release. Should pg_triple expose VP tables as PGQ graphs via
   a DDL helper (`pg_triple.create_pgq_graph(graph_name TEXT)`)? This would give
   users SQL/PGQ support for free without pg_triple implementing a GQL parser at
   all — worth evaluating if PostgreSQL adds support before pg_triple's Cypher work
   starts.

4. **TCK compliance target for GQL**: The openCypher TCK has ~2000 scenarios.
   There is no equivalent published GQL conformance suite yet (the standard is
   only 2 years old). What compliance level is sufficient for a production release?

---

## 9. References

- ISO/IEC 39075:2024 GQL standard: https://www.iso.org/standard/76120.html
- GQL standards progress: https://www.gqlstandards.org/
- openCypher specification and TCK: https://opencypher.org/resources/
- openCypher → GQL transition: https://opencypher.org/ ("last release before GQL")
- Neo4j Cypher 25 (GQL-aligned): https://neo4j.com/docs/cypher-manual/current/introduction/
- SQL/PGQ (ISO/IEC 9075-16:2023): part of SQL:2023 standard
- GQL vs SQL/PGQ GPM layer: https://dl.acm.org/doi/10.1145/3514221.3526057
- Apache TinkerPop Gremlin reference: https://tinkerpop.apache.org/docs/current/reference/
- Hartig & Hidders on Gremlin/SPARQL semantics: "Defining Schemas for Property Graphs" (2019)
- Companion documents:
  - [cypher_lpg_analysis.md](cypher_lpg_analysis.md) — core LPG/RDF storage analysis
  - [prior_art_graph_systems.md](prior_art_graph_systems.md) — pure LPG system lessons
  - [prior_art_hybrid_systems.md](prior_art_hybrid_systems.md) — Stardog, AnzoGraph, Neptune
