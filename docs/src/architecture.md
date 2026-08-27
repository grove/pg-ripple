# Architecture: Subsystem Dependency Graph

This document describes the major subsystems of pg_ripple and their dependencies.
It was introduced in v0.114.0 as part of the module-decomposition effort (A16).

## Subsystem Map

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Query Layer                                  │
│  src/sparql/                    src/sparql/wcoj/                     │
│  ├─ parse.rs                    ├─ mod.rs (coordinator)              │
│  ├─ plan.rs                     ├─ executor.rs                       │
│  ├─ execute.rs                  ├─ trie.rs                           │
│  └─ decode.rs                   └─ leapfrog.rs                       │
│                                                                      │
│  src/sparql/embedding/                                               │
│  ├─ mod.rs                                                           │
│  ├─ index.rs  (API client, pgvector)                                 │
│  ├─ hybrid.rs (hybrid SPARQL+vector search)                          │
│  └─ rag.rs    (RAG retrieval)                                        │
└──────────────────────────────────┬──────────────────────────────────┘
                                   │ uses
┌──────────────────────────────────▼──────────────────────────────────┐
│                         Inference Layer                              │
│                                                                      │
│  src/datalog/        src/datalog_api/     src/skos/                  │
│  (rule engine,       ├─ parse.rs          ├─ mod.rs                  │
│   stratifier,        ├─ validate.rs       ├─ bundle.rs               │
│   SQL compiler,      ├─ explain.rs        ├─ inference.rs            │
│   RDFS/OWL RL)       └─ conflict.rs       ├─ broader_narrower.rs     │
│                                           └─ export.rs               │
│                                                                      │
│  src/shacl/          src/shacl/validator/                            │
│  (shapes→DDL,        ├─ mod.rs                                       │
│   async pipeline)    ├─ node.rs                                      │
│                      ├─ property.rs                                  │
│                      ├─ sparql.rs                                    │
│                      └─ severity.rs                                  │
└──────────────────────────────────┬──────────────────────────────────┘
                                   │ uses
┌──────────────────────────────────▼──────────────────────────────────┐
│                         Storage Layer                                │
│                                                                      │
│  src/storage/        src/dictionary/      src/citus/                 │
│  (VP tables,         (IRI/BNode/Lit →     ├─ mod.rs (detection)      │
│   HTAP delta/main,   i64 via XXH3-128)    ├─ shard_pruning.rs        │
│   merge worker)                           ├─ ddl_hooks.rs            │
│                                           ├─ query_rewriting.rs      │
│                                           └─ rebalance.rs            │
└─────────────────────────────────────────────────────────────────────┘
```

## Key Dependency Relationships

| Subsystem | Depends On | Notes |
|-----------|-----------|-------|
| SKOS (`src/skos/`) | Datalog (`src/datalog/`) | Bundle loading injects Datalog rules for SKOS closure |
| OWL RL | Datalog (`src/datalog/`) | OWL 2 RL entailment rules compiled to Datalog strata |
| NS-RL (neuro-symbolic) | `sparql/embedding/` + Datalog + `datalog_api/conflict` | Combines vector similarity with rule-based inference |
| Conflict detection | SHACL (`src/shacl/`) + Datalog | Lattice-based conflict checks use both shape validation and Datalog constraints |
| Hypothetical inference | Storage (`src/storage/`) | Creates temporary VP table snapshots for what-if queries |
| Views (`src/views/`) | SPARQL + Datalog + Storage | CONSTRUCT/DESCRIBE/ASK views wrap SPARQL queries as SQL views |
| Citus (`src/citus/`) | Storage | Shard pruning reads VP predicate catalog and VP table OIDs |
| PageRank (`src/pagerank/`) | Datalog + Storage | Datalog-native PageRank with IVM |

## Module Size Policy

Each `.rs` file is reported at **800 LOC** and bounded at **1,200 LOC** (CI hard failure).
The gate runs via `scripts/check_module_size.sh` on every PR. Files over 800
lines are reported for review; files over 1,200 lines fail CI.

When a module grows beyond the limit, decompose it into a `src/<module>/` directory:

```
src/mymodule.rs                     # before (>1500 LOC)
  ↓
src/mymodule/mod.rs                 # coordinator + public re-exports (< 400 LOC)
src/mymodule/submodule_a.rs         # focused sub-module (< 400 LOC)
src/mymodule/submodule_b.rs         # focused sub-module (< 400 LOC)
```

Reference implementations: `src/datalog/`, `src/sparql/`, `src/views/`, `src/skos/`.
