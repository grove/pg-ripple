# Cypher / LPG Support on the pg_triple Foundation

> **Status**: Exploratory analysis — not yet committed to the roadmap.
> This document captures the architectural reasoning from the initial design session (April 2026).
> Further analysis documents in this folder should be added before planning is finalised.

---

## 1. The Question

pg_triple is being built as an RDF triple store with SPARQL query support. RDF Knowledge Graphs
(KGs) and Labeled Property Graphs (LPGs) have historically been distinct models, but the boundary
is dissolving. The question is: should pg_triple also expose a Cypher / GQL interface, and if so,
how?

---

## 2. What the Storage Layer Already Provides

pg_triple's VP table layout turns out to be structurally close to LPG storage in surprising ways:

| LPG concept | VP table mapping |
|---|---|
| Node identity | Dictionary ID (`i64`) for each subject IRI |
| Node label | `rdf:type` triple → `vp_type(s=node_id, o=label_id)` |
| Edge type | Predicate — each VP table *is* a typed edge table |
| Edge endpoint | `s` and `o` columns |
| Node property | `vp_{prop}(s=node_id, o=value_id)` — exactly how VP tables work |
| Edge property | **Requires RDF-star** (see §3) |

The dictionary encoder (IRI → `i64`), HTAP architecture, and BRIN/B-tree index strategy are
all datatype-agnostic. They would be reused verbatim for Cypher support.

### What is free

- Dictionary encoding: 100% reusable
- VP table storage: LPG and RDF triples are the same physical layout
- HTAP merge architecture: unchanged
- Datalog/OWL RL reasoning: maps to graph-reachability and label propagation
- SHACL shapes → node/edge property constraints: directly useful for LPG schemas

### What is not free

- **Edge properties** — the only structural gap (see §3)
- **Cypher parser** — does not exist as a reusable Rust crate (see §5)
- **Cypher algebra compiler** — a new translation pipeline, analogous to `src/sparql/` but for Cypher→SQL
- **Write-path semantics** — `CREATE`, `MERGE`, `SET`, `DELETE` must route through the dictionary encoder and VP table selection logic

---

## 3. The Edge Property Problem and RDF-star

In LPG, edges are first-class objects that carry key-value properties:

```cypher
CREATE (alice)-[:KNOWS {since: 2020, weight: 0.9}]->(bob)
```

VP tables store `(s, o)` integer pairs. There is no natural slot for `since` or `weight` on the
edge. This is the one genuine storage gap.

**RDF-star** (planned for v0.4.0) resolves this cleanly:

```turtle
<<:alice :knows :bob>> :since 2020 .
<<:alice :knows :bob>> :weight 0.9 .
```

An RDF-star annotation triple `<<s p o>> q v` is stored as a quad where the subject is the
hash of the inner triple `(s, p, o)`. This maps to an edge property without any schema changes:

| Cypher | RDF-star | Storage |
|---|---|---|
| `CREATE (a)-[:KNOWS {since: 2020}]->(b)` | `<<:a :knows :b>> :since 2020` | `vp_since(s=hash(a,knows,b), o=2020_id)` |
| `MATCH ()-[r:KNOWS]-() RETURN r.since` | `?s :since ?v . FILTER EXISTS { <<:a :knows :b>> }` | standard VP scan |

The conclusion: **v0.4.0 (RDF-star) is the enabling dependency for full Cypher write support**.
Before that release, read-only Cypher (`MATCH … RETURN`) on triples-without-edge-properties is
feasible; write support for edge-property edges (`CREATE`/`SET` with `[r:TYPE {prop: val}]`) is
not.

---

## 4. Cypher Write Semantics

Cypher has a full mutation surface — it is not read-only:

| Cypher clause | Meaning | Maps to |
|---|---|---|
| `CREATE (n:Label {k: v})` | Create node with properties | `insert_triple(n, rdf:type, Label)` + `insert_triple(n, k, v)` |
| `CREATE (a)-[:TYPE {k: v}]->(b)` | Create edge with properties | RDF-star annotation triples (requires v0.4.0) |
| `MERGE (n:Label {k: v})` | Upsert | `ON CONFLICT DO NOTHING` + conditional insert |
| `SET n.prop = val` | Update property | delete old `vp_prop(n, ?)` row + insert new one |
| `REMOVE n.prop` | Delete property | delete `vp_prop(n, ?)` row |
| `DETACH DELETE n` | Delete node + all incident edges | delete all VP rows where `s = n_id OR o = n_id` |

Every write operation must route through the dictionary encoder (string → `i64`) and the VP table
selector (predicate ID → table OID lookup). A separate `pg_cypher` extension that calls into
these internals is, in practice, so tightly coupled to pg_triple that the extension boundary
provides no real separation. This is the primary argument for keeping Cypher support within the
same Cargo workspace rather than a truly independent project.

---

## 5. The Rust Cypher Parser Ecosystem (April 2026)

The situation is analogous to SPARQL before `spargebra` existed — a real gap in the ecosystem.

| Crate | Downloads | Last updated | Assessment |
|---|---|---|---|
| `open-cypher` | 3,259 all-time | 3 years ago | Abandoned; no algebra output |
| `drasi-query-cypher` | 4,477 all-time | 3 months ago | Hardwired to Microsoft Drasi's runtime types (`ElementMetadata`, `SourceChange`); not a standalone parser |
| `plexus-parser` | 147 all-time | Active | Lowers to Plexus MLIR dialect; project-specific, not general-purpose |
| `uni-cypher` | 250 all-time | 1 day ago | Brand new; Rustic AI project-specific |
| `cypherlite-query` | 152 all-time | Active | Tightly coupled to CypherLite engine |

**Finding**: There is no Rust crate for Cypher that does what `spargebra` does for SPARQL — parse
query text, produce a backend-agnostic algebra IR, and expose it for a custom execution backend.
This is a genuine ecosystem gap.

### Implication

A standalone `cypher-algebra` crate, modelled on `spargebra`'s design, would:

1. Be independently valuable and publishable to crates.io
2. Fill an actual gap in the Rust graph-database ecosystem
3. Give pg_triple a well-tested, separable frontend component

The parser work is substantial but the resulting crate is reusable far beyond pg_triple.

---

## 6. Proposed Architecture

### 6.1 Cargo workspace structure

```
pg-triple/  (workspace root)
  crates/
    cypher-algebra/        ← standalone crate, independently published
    │   src/
    │     grammar/         — openCypher / ISO GQL grammar (pest or nom)
    │     ast.rs           — concrete syntax tree
    │     algebra.rs       — normalized algebra IR (Match, Project, Filter,
    │                         Create, Merge, Set, Remove, Delete)
    │     normalize.rs     — AST → algebra lowering
    │
    pg_triple/             ← pgrx extension
        src/
          sparql/          — existing SPARQL→SQL pipeline
          cypher/          — Cypher algebra → SQL (new; mirrors sparql/)
            mod.rs
            translator.rs  — CypherAlgebra → SQL string
            writer.rs      — CREATE/MERGE/SET/DELETE → VP table DML
```

### 6.2 Translation pipeline

```
Cypher text
    │
    ▼  cypher-algebra (standalone crate)
CypherAlgebra IR
    │
    ▼  src/cypher/translator.rs  (pg_triple)
Encode constants via dictionary encoder
    │
    ▼
SQL SELECT (joins over VP tables, identical to SPARQL path)
    │
    ▼  PostgreSQL / SPI
Result rows (integer-encoded)
    │
    ▼  batch decode
Decoded result set
```

For write operations:

```
CypherAlgebra::Create / Merge / Set / Delete
    │
    ▼  src/cypher/writer.rs
dictionary encode all string values
    │
VP table DML (INSERT / DELETE)  ←  same functions as SPARQL Update writer
```

### 6.3 Shared components — no duplication

| Component | SPARQL path | Cypher path |
|---|---|---|
| Dictionary encode/decode | `src/dictionary/` | same |
| VP table lookup | `src/storage/predicates` | same |
| HTAP write path | `src/storage/insert_triple()` | same |
| Merge worker | `src/storage/merge.rs` | same |
| SHACL validation | `src/shacl/` | same (node/edge schema constraints) |
| RDF-star (edge props) | v0.4.0 | same — required for edge properties |

The Cypher→SQL compiler is the only genuinely new component. Everything else is reused.

---

## 7. Relationship to Apache AGE

Apache AGE is a PostgreSQL extension with openCypher support. It is sometimes cited as the path
to "Cypher on the same infrastructure."

**AGE does not query SQL views.** AGE has its own internal storage in `_ag_label_*` heap tables
using its `agtype` binary format. `cypher('graph', $$ MATCH (n) RETURN n $$)` scans AGE's own
tables, not pg_triple's VP tables.

Using AGE alongside pg_triple would require an ETL sync step (VP tables → AGE internal storage),
resulting in:
- Duplicate data storage
- Stale Cypher reads (lag behind pg_triple writes)
- Loss of dictionary encoding benefits
- No shared transaction boundary

**Conclusion**: AGE interop via ETL copy is a reporting sidecar, not a live query path. It is not
a substitute for native Cypher support in pg_triple.

---

## 8. Sequencing Relative to the Current Roadmap

The current roadmap runs to v1.0.0 at an estimated 95–122 person-weeks. Cypher support is not
in that scope.

Recommended sequencing:

1. **v0.x – v1.0.0**: Build pg_triple as planned. No Cypher scope. Ensure the storage API
   (`insert_triple`, `delete_triple`, VP table selector, dictionary encoder) is stable and
   well-tested — this becomes the write-path contract that the Cypher compiler will target.

2. **v0.4.0 (RDF-star)**: This is the enabling dependency for edge properties. It is now
   available early in the roadmap, well before Cypher write support is needed.

3. **Parallel / post-1.0**: Begin `cypher-algebra` as an independent crate in the workspace.
   The grammar and AST work does not depend on pg_triple's internals and can proceed in parallel
   with later 0.x releases.

4. **Post-1.0**: Integrate `cypher-algebra` into pg_triple as `src/cypher/`. Add `cypher()`
   SQL function mirroring `sparql()`.

### Rough effort estimate (post-1.0 work)

| Component | Estimated effort |
|---|---|
| `cypher-algebra` crate (grammar, AST, algebra IR) | 12–16 pw |
| `src/cypher/translator.rs` (read-only MATCH→SQL) | 8–12 pw |
| `src/cypher/writer.rs` (CREATE/MERGE/SET/DELETE) | 8–10 pw |
| Variable-length paths (already built for SPARQL property paths) | 2–3 pw |
| Test suite + openCypher TCK compliance | 6–8 pw |
| **Total** | **36–49 pw** |

---

## 9. Open Questions

These should be resolved before committing Cypher support to the roadmap:

1. **ISO GQL vs. openCypher scope**: GQL (ISO/IEC 39075:2024) is the formal standard; openCypher
   is the practical dialect used by Neo4j, Memgraph, AGE, etc. They overlap significantly but
   differ in mutation syntax and some pattern semantics. Which does the parser target first?

2. **`MERGE` conflict semantics**: Cypher `MERGE` is conceptually simple but semantically complex
   under concurrent writes. PostgreSQL advisory locks or `ON CONFLICT` strategies need design.

3. **LPG surface naming**: Should `CREATE (n:Person)` store to `rdf:type` using the RDF model
   (interoperable but verbose IRI), or should there be a separate LPG namespace convention?

4. **`cypher-algebra` crate governance**: Publish as a standalone crate from the start, or
   develop inside the workspace first and extract when stable?

5. **OpenCypher TCK compliance**: The openCypher project provides a Technology Compatibility Kit
   (TCK) with ~800 scenarios. What compliance level is required before shipping?

---

## 10. References

- openCypher project and TCK: https://opencypher.org/
- ISO GQL standard (ISO/IEC 39075:2024): https://www.iso.org/standard/76120.html
- `spargebra` (the SPARQL analog): https://crates.io/crates/spargebra
- RDF-star W3C spec: https://www.w3.org/2021/12/rdf-star.html
- W3C RDF 1.2 (formalises RDF-star): https://www.w3.org/TR/rdf12-concepts/
- Apache AGE: https://age.apache.org/
- RDF→LPG mapping: https://www.w3.org/TR/rdf-star-use-cases/
- **Prior art survey** — eight graph systems analysed in detail:
  [plans/cypher/prior_art_graph_systems.md](prior_art_graph_systems.md)
