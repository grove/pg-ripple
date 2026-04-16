# Introduction

**pg_ripple** is a high-performance RDF triple store implemented as a native PostgreSQL 18 extension. It stores RDF data using a *Vertical Partitioning* (VP) strategy — each unique predicate gets its own table — and executes SPARQL 1.1 queries by compiling them to SQL that runs inside the PostgreSQL engine.

## What makes pg_ripple different?

Most RDF stores are standalone systems that speak SPARQL over HTTP. pg_ripple lives *inside* PostgreSQL, which means:

- **Integer joins everywhere** — every IRI, blank node, and literal is dictionary-encoded to `BIGINT` before storage. VP table joins never touch strings, giving them the same speed as ordinary relational joins.
- **SPARQL as SQL** — `SELECT * FROM pg_ripple.sparql('…')` returns results as a relational table. Combine SPARQL with ordinary SQL using `JOIN`, `WHERE`, window functions, and CTEs.
- **Transactional writes** — `INSERT`, `DELETE`, and bulk loads participate in PostgreSQL transactions. Rollback works exactly as expected.
- **Property paths via `WITH RECURSIVE`** — path queries compile to PostgreSQL 18's native recursive CTEs with hash-based cycle detection (`CYCLE` clause), avoiding the per-level array scans required by earlier PostgreSQL versions.

## LPG compatibility

pg_ripple's VP storage model is structurally compatible with Labeled Property Graph (LPG) systems: each VP table corresponds to a property edge type, and multi-valued predicates fall out naturally. A Cypher/GQL compatibility layer is on the roadmap; see the [Roadmap](../reference/roadmap.md) for timing.

## When to use pg_ripple

| Use case | pg_ripple | Standalone triple store |
|---|---|---|
| SPARQL inside a PostgreSQL application | ✅ | requires HTTP client |
| Transactional RDF writes with rollback | ✅ | usually not supported |
| Mixing SPARQL results with relational data | ✅ | complex federation |
| SHACL validation and Datalog reasoning | ✅ | rarely built-in |
| SPARQL federation across remote endpoints | ✅ | mature support |
| Hundreds of billions of triples | future (post-1.0) | some mature stores |

## System requirements

- PostgreSQL 18.x
- Rust 1.85+ with pgrx 0.17
- macOS, Linux (x86_64, aarch64)

See [Installation](installation.md) for the full setup procedure.
