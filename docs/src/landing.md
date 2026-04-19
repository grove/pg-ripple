# What Is pg_ripple?

**pg_ripple** turns your PostgreSQL database into a knowledge graph store. Store facts as triples, query them with SPARQL, validate data quality with SHACL, derive new facts with Datalog rules, and serve results over HTTP — all inside PostgreSQL, with no extra infrastructure for the data store itself.

```sql
-- Load facts about people and relationships
SELECT pg_ripple.load_turtle('
  @prefix ex: <http://example.org/> .
  @prefix foaf: <http://xmlns.com/foaf/0.1/> .
  ex:alice foaf:name "Alice" .
  ex:alice foaf:knows ex:bob .
  ex:bob   foaf:name "Bob" .
  ex:bob   foaf:knows ex:carol .
  ex:carol foaf:name "Carol" .
');

-- Ask: who does Alice know, directly or indirectly?
SELECT * FROM pg_ripple.sparql('
  PREFIX ex: <http://example.org/>
  PREFIX foaf: <http://xmlns.com/foaf/0.1/>
  SELECT ?name WHERE {
    ex:alice foaf:knows+ ?person .
    ?person foaf:name ?name .
  }
');
```

The query follows the `foaf:knows` relationship through any number of hops and returns the names of everyone Alice is connected to — Bob and Carol.

---

## Why pg_ripple?

Knowledge graphs represent information as a network of relationships rather than rows in flat tables. This structure naturally captures complex, interconnected data — organizational hierarchies, supply chains, research citations, product catalogs — that would require dozens of join tables in a relational model.

pg_ripple brings this capability to PostgreSQL. You get the expressiveness of a dedicated graph database while keeping your existing PostgreSQL infrastructure, tooling, backup procedures, and operational expertise.

### Key capabilities

| Capability | What it does |
|---|---|
| **SPARQL queries** | Ask complex relationship questions using the W3C standard query language |
| **SHACL validation** | Define and enforce data quality rules — reject bad data on insert |
| **Datalog reasoning** | Automatically derive new facts from rules and logic |
| **Vector + graph hybrid** | Combine SPARQL graph traversal with pgvector similarity search |
| **JSON-LD framing** | Export nested JSON documents shaped for your API contract |
| **SPARQL Protocol** | Serve queries over a standard HTTP endpoint via `pg_ripple_http` |
| **Federation** | Query remote SPARQL endpoints alongside local data |

### Key numbers

| Metric | Value |
|---|---|
| Bulk load throughput | >100K triples/sec (commodity hardware) |
| SPARQL query latency | <10ms for typical patterns |
| W3C SPARQL 1.1 | Full conformance |
| W3C SHACL Core | Full conformance |
| PostgreSQL version | 18 |

---

## Architecture at a glance

```
┌─────────────────────────────────────────────────┐
│                  PostgreSQL 18                   │
│  ┌───────────────────────────────────────────┐  │
│  │              pg_ripple extension           │  │
│  │  ┌─────────┐  ┌────────┐  ┌───────────┐  │  │
│  │  │Dictionary│  │ SPARQL │  │  Datalog   │  │  │
│  │  │ Encoder  │  │ Engine │  │  Engine    │  │  │
│  │  └────┬─────┘  └───┬────┘  └─────┬─────┘  │  │
│  │       │             │             │         │  │
│  │  ┌────┴─────────────┴─────────────┴─────┐  │  │
│  │  │     VP Tables (one per predicate)     │  │  │
│  │  │   HTAP: delta + main + merge worker   │  │  │
│  │  └──────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
         ▲                          ▲
         │ SQL                      │ HTTP
    Application              pg_ripple_http
```

Every IRI, literal, and blank node is mapped to a compact integer ID by the dictionary encoder. Data is stored in Vertical Partitioning (VP) tables — one table per unique predicate — with integer-only joins for fast query execution. The HTAP architecture separates read and write paths so that heavy loads do not block queries.

---

## Next steps

- **Evaluating?** Read [When to Use pg_ripple](evaluate/when-to-use.md) for an honest comparison with alternatives.
- **Ready to try it?** Start with [Installation](getting-started/installation.md) and then the [Five-Minute Walkthrough](getting-started/hello-world.md).
- **Want the full picture?** The [Guided Tutorial](getting-started/tutorial.md) takes you from loading data to inference in 30 minutes.
- **Want to contribute?** See [Contributing](reference/contributing.md).
