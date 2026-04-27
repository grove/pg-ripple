# Comparison: pg_ripple vs Alternatives

A dispassionate side-by-side. Pick the technology that fits your team and constraints, not the one with the loudest blog.

---

## At a glance

|  | **pg_ripple** | Plain PostgreSQL | Virtuoso / Blazegraph / GraphDB | Neo4j / Memgraph | Pinecone / Weaviate / Qdrant |
|---|---|---|---|---|---|
| Deployment | PostgreSQL extension | RDBMS you already run | Standalone (JVM/native) | Standalone | Standalone / SaaS |
| Query language | SPARQL 1.1 | SQL | SPARQL 1.1 | Cypher / GQL | Proprietary |
| Data model | RDF (triples) | Relational | RDF (triples) | Property graph | Vectors + metadata |
| Validation | SHACL (full Core + SPARQL) | CHECK / triggers | Varies | Constraints | None |
| Reasoning | Datalog (RDFS, OWL RL/EL/QL, lattices, WFS, magic sets) | Manual SQL or none | RDFS / OWL (varies) | None built-in | None |
| Vector search | pgvector + KGE | pgvector | Plug-in or none | Limited | Native, only purpose |
| Hybrid graph + vector | One SQL query | Manual joins | Plug-in workarounds | Limited | Not possible |
| ACID | Full PostgreSQL ACID | Full ACID | Varies (often eventual) | ACID | Varies |
| Backup / restore | `pg_dump` / `pg_restore` | Standard | Custom tooling | Custom tooling | Custom tooling |
| HTTP / SPARQL Protocol | `pg_ripple_http` | None | Built-in | Built-in (Cypher over HTTP) | REST |
| Federation | SPARQL `SERVICE` + vector federation | None | SPARQL `SERVICE` | None | Some |
| Operational expertise | PostgreSQL DBA skills transfer | PostgreSQL DBA | Specialised triple-store ops | Specialised graph ops | Vendor-specific |
| Conformance | W3C SPARQL 1.1, SHACL Core, OWL 2 RL: 100 % | n/a | Varies | n/a | n/a |

---

## When pg_ripple is the obvious choice

- You **already operate PostgreSQL** and want to avoid running a second database.
- Your data has **rich, interconnected relationships** — ontologies, supply chains, organisational hierarchies, citation networks.
- You need **SPARQL 1.1** for interoperability with W3C-standard tooling.
- You need to **validate data quality** against formal rules (SHACL).
- You need to **derive new facts** from existing data (Datalog, OWL RL).
- You want to combine **graph traversal with vector similarity** for RAG, recommendations, or record linkage.
- You need **transactional guarantees** spanning graph data, vector data, and ordinary relational data.

---

## When pg_ripple is *not* the right answer

| Situation | Better fit |
|---|---|
| > 1 B triples, single instance | Distributed triple stores (or pg_ripple + Citus, see [Scaling](../operations/scaling.md)) |
| Existing Cypher / GQL codebase, no plans to learn SPARQL | Neo4j / Memgraph |
| Pure vector search, no graph traversal | pgvector by itself, or Pinecone/Qdrant if you need a managed service |
| Streaming graph analytics over append-only event firehose | Apache Flink + a graph library |
| You do not run PostgreSQL anywhere and have no plans to | Pick a tool native to your stack |
| Need SQL-only, allergic to RDF | Plain PostgreSQL with thoughtful schema design |

---

## A specific comparison: hybrid retrieval for RAG

This is the comparison most teams care about today.

|  | pg_ripple | Vector DB only | Graph DB only |
|---|---|---|---|
| Free-text question → similar entities | Native | Native | Manual |
| Multi-hop relationship walk | Native (SPARQL property paths) | Not possible | Native (Cypher) |
| Combined hybrid query | One SQL call | Glue code in app | Glue code in app |
| Atomic write of triple + embedding | Yes (one transaction) | No | No |
| Audit + provenance | PROV-O + audit log | None | Custom |
| Multi-tenant isolation | Graph RLS + quota | Per-namespace, per-tier | Per-database |
| Operational footprint | One PostgreSQL | Two systems | Two systems |
| Cost of an extra vector store you no longer need | $0 | $$$$ | $$$$ |

The dominant trade-off: vector DBs are simpler when your *only* job is "find similar passages". Once you also need precise relationships, provenance, multi-hop reasoning, or transactional consistency, the cost of stitching two systems together quickly exceeds the cost of running pg_ripple.

---

## See also

- [When to Use pg_ripple](when-to-use.md)
- [Architecture at a Glance](architecture-glance.md)
- [Performance & Conformance Results](performance-results.md)
