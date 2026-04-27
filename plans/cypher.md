# Cypher / GQL Architecture Decision Record

**Status:** Draft  
**Date:** 2025  
**Authors:** pg_ripple team  

---

## Context

Property graph query languages — Cypher (Neo4j), openCypher, and the emerging
ISO GQL standard — have become the dominant way data engineers express pattern
matching over graph-shaped data. Many users who discover pg_ripple's knowledge
graph capabilities ask whether they can use Cypher instead of SPARQL.

This ADR captures the design intent for Cypher/GQL support in pg_ripple.

---

## Decision

pg_ripple will implement a **Cypher-to-SPARQL rewrite layer** rather than a
native Cypher execution engine. The target query subset, parser crate choice,
and semantic fidelity notes are documented below.

---

## Target Query Subset

The initial implementation targets the intersection of openCypher and common
RDF property graph patterns:

| Feature | Supported | Notes |
|---|---|---|
| `MATCH (n:Label)` | Yes | Maps to `?n a :Label` |
| `MATCH (n)-[r:Prop]->(m)` | Yes | Maps to `?n :Prop ?m` |
| `WHERE n.prop = value` | Yes | Maps to SPARQL FILTER |
| `RETURN n.prop` | Yes | Maps to SPARQL SELECT |
| `CREATE (n:Label {prop: val})` | Planned | Maps to SPARQL INSERT DATA |
| `SET n.prop = val` | Planned | Maps to SPARQL UPDATE |
| `DELETE n` | Planned | Maps to `pg_ripple.erase_subject()` |
| Path patterns `(a)-[*1..3]->(b)` | Yes | Maps to SPARQL property paths |
| `OPTIONAL MATCH` | Yes | Maps to SPARQL OPTIONAL |
| `WITH … ORDER BY … LIMIT` | Yes | Maps to SPARQL ORDER BY / LIMIT |
| `UNWIND` | Planned | Maps to SPARQL VALUES |
| Subqueries | Planned | Maps to SPARQL subSELECT |

Features not in scope (deferred to a future release):

- Cypher mutations that create blank nodes without IRIs
- `MERGE` (upsert semantics without a natural SPARQL equivalent)
- Index hints (`USING INDEX`)
- Stored procedures

---

## Parser Crate Choice

**Selected:** [`cypher-parser`](https://crates.io/crates/cypher-parser) (Rust)

Rationale:
- Pure Rust, no C dependencies
- Produces an AST that maps cleanly to SPARQL algebra concepts
- Actively maintained and covers openCypher 9 grammar
- MIT licensed

Alternative considered: `neo4j-cypher-parser` (JNI bridge to the Java parser).
Rejected: introduces a JVM dependency that is incompatible with the pgrx
embedding model.

---

## Rewrite-to-SPARQL Strategy

The rewrite pipeline is:

```
Cypher text
    → cypher-parser AST
    → pg_ripple Cypher IR (normalized pattern graph)
    → SPARQL algebra (spargebra types)
    → existing SPARQL → SQL translation (unchanged)
    → SQL → SPI execution
```

Key mapping rules:

1. **Node patterns**: `(n:Label)` → `?n a :Label .`
2. **Relationship patterns**: `(n)-[:Prop]->(m)` → `?n :Prop ?m .`
3. **Property access**: `n.prop` → a fresh variable `?n_prop` with triple `?n :prop ?n_prop .`
4. **Labels as RDF types**: Cypher labels map to `rdf:type` triples using the
   same namespace prefix registry as SPARQL queries.
5. **Named graphs**: `MATCH (n) IN GRAPH 'iri'` → SPARQL `GRAPH <iri> { ... }`.

---

## Semantic Fidelity Notes

The rewrite approach achieves high but not perfect fidelity:

| Semantic Difference | Impact | Mitigation |
|---|---|---|
| Cypher `null` propagation differs from SPARQL `UNBOUND` | Low | Document; emit FILTER(?x != "") where needed |
| Cypher node identity uses internal IDs; pg_ripple uses IRIs | Medium | Expose `id()` as IRI string; users must use full IRIs |
| Cypher allows multi-value properties (lists); RDF does not | Low | Each list element becomes a separate triple |
| Cypher path length `*` (unlimited) | High | Map to SPARQL `+` (1 or more); document limitation |

---

## Entry Point

New module: `src/cypher/` with:

```
src/cypher/
    mod.rs           — public API, Cypher text → spargebra algebra
    parser.rs        — wraps cypher-parser crate
    ir.rs            — Cypher IR types
    rewrite.rs       — Cypher IR → SPARQL algebra
```

New SQL function: `pg_ripple.cypher(query TEXT) → SETOF RECORD` — executes a
Cypher query by rewriting to SPARQL and dispatching to the existing SPARQL
execution stack.

---

## Acceptance Criteria

- `pg_ripple.cypher('MATCH (n:Person) RETURN n.name')` returns the same results
  as the equivalent SPARQL query.
- All test cases in `tests/cypher/` pass.
- `EXPLAIN` output for a Cypher query shows the SPARQL rewrite.
- Documentation includes a Cypher quick-start guide.

---

## Status

Design complete; implementation is planned for a future release. This ADR is
**not** a commitment to a specific release date.
