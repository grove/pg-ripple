# Architecture at a Glance

A two-minute overview of how pg_ripple is built. The full architectural reference lives at [Reference → Architecture](../reference/architecture.md); this page is the summary you can hand to an architect during evaluation.

---

## Where pg_ripple sits

```
┌────────────────────────────────────────────────────────────────────┐
│                          Your application                           │
│                                                                     │
│   psql / asyncpg / JDBC          HTTP / SPARQL Protocol             │
└──────────────┬───────────────────────────┬─────────────────────────┘
               │ SQL                       │ HTTP
               ▼                           ▼
┌────────────────────────┐    ┌────────────────────────┐
│   PostgreSQL 18        │    │  pg_ripple_http        │
│                        │◄───┤  (companion service)   │
│  ┌──────────────────┐ │    │  Rust + Axum + deadpool│
│  │   pg_ripple       │ │    └────────┬───────────────┘
│  │   extension       │ │             │
│  │  (Rust + pgrx)    │ │             │ pg_ripple.sparql(...)
│  └──────────────────┘ │             │
└────────────────────────┘             │
            │                          │
            ▼                          │
┌────────────────────────────────────────┘
│  Storage layer (PostgreSQL tables, all integer-encoded)
│
│   _pg_ripple.dictionary        — IRI / blank / literal → BIGINT
│   _pg_ripple.vp_<predicate_id> — one VP table per predicate
│   _pg_ripple.vp_rare           — long-tail predicates
│   _pg_ripple.embeddings        — pgvector vectors, HNSW indexed
│   _pg_ripple.kge_embeddings    — graph-structural embeddings
│   _pg_ripple.audit_log         — every SPARQL UPDATE
│   …
```

Everything is plain PostgreSQL. There is no separate graph store, no separate vector store, no separate cache. A single `pg_dump` captures the whole thing.

---

## The five subsystems

| Subsystem | What it owns | Implemented in |
|---|---|---|
| **Dictionary** | Encoding every IRI / blank node / literal as a stable `BIGINT` | `src/dictionary/` |
| **Storage** | VP tables, HTAP delta + main split, the merge background worker | `src/storage/` |
| **SPARQL engine** | SPARQL text → algebra → SQL → SPI execution → decode | `src/sparql/` |
| **Datalog engine** | Rule parser, stratifier, SQL compiler, OWL RL/EL/QL profiles | `src/datalog/` |
| **SHACL validator** | Shapes → DDL constraints + async validation pipeline | `src/shacl/` |

All five live inside the same PostgreSQL process and the same SQL transaction. This is the property that makes hybrid retrieval, atomic record-linkage, and audit-grade provenance possible.

---

## Three architectural choices that shape everything

### 1. **Vertical Partitioning (VP) — one table per predicate**

Every unique predicate gets its own table. `vp_<id>(s, o, g, i, source)` — all integers. Star patterns (same subject, multiple predicates) collapse into one self-join over a single subject value. There is no `triples(s, p, o, g)` mega-table.

The result: SPARQL star-pattern queries match the speed of a hand-written SQL query against an analogously-shaped relational schema. Often faster, because every join is integer-equality.

### 2. **Dictionary encoding — BIGINTs everywhere**

Every triple, even at the parser boundary, is converted to a tuple of `BIGINT` IDs before it touches storage. Joins are integer equality. Filters are integer comparison. Decoding to text happens only at query output. The dictionary uses XXH3-128 for collision-free hashing and an LRU shared-memory cache for hot terms.

### 3. **HTAP storage — delta + main + merge worker**

Heavy ingest goes into per-predicate **delta** tables (regular B-tree heap). Read queries see `(main EXCEPT tombstones) UNION ALL delta`. A background worker merges delta into BRIN-indexed main asynchronously. Writers never block readers; readers never see partial loads.

---

## What you do *not* see in the diagram

These are deliberately invisible to the user but have outsized impact:

- **Statement-ID timeline** — every triple carries a globally-unique SID from a shared sequence. Powers `point_in_time()` queries.
- **Predicate catalog** — `_pg_ripple.predicates(id, table_oid, triple_count)`. Cached in shared memory; survives extension reloads.
- **Plan cache** — translated SPARQL → SQL plans are cached by query text hash, with invalidation on schema change.
- **Background workers** — merge worker, embedding worker (when `auto_embed = on`), CDC publisher.

---

## How it scales

| Axis | Mechanism |
|---|---|
| **More CPU on one machine** | Parallel merge workers, parallel Datalog stratum evaluation, PostgreSQL parallel scans on VP tables |
| **More disk on one machine** | BRIN indexes on `vp_<id>_main`, dictionary cache sized by GUC |
| **Many small concurrent queries** | PostgreSQL connection pool + `pg_ripple_http` deadpool pool |
| **Read replicas** | `pg_ripple.read_replica_dsn` routes read queries automatically |
| **Across many machines** | Citus integration: VP tables become distributed tables; bound-subject SPARQL patterns are pruned to one shard |

For deep coverage of each axis see [Operations → Scaling](../operations/scaling.md) and [Operations → Citus integration](../operations/citus-integration.md).

---

## Where to read more

- [Reference → Architecture](../reference/architecture.md) — full, code-linked architecture description.
- [Operations → Architecture overview](../operations/architecture.md) — the operations-team view of the same architecture.
- [Plans → Implementation plan](https://github.com/grove/pg-ripple/blob/main/plans/implementation_plan.md) — the authoritative description of the *eventual target architecture*.
