[← Back to Blog Index](README.md)

# Why RDF Inside PostgreSQL?

## The case for a triple store that lives where your data already is

---

You have a knowledge graph problem. Maybe it's an ontology for a healthcare system. Maybe it's a product taxonomy with 200,000 SKUs and relationships that don't fit in a relational schema. Maybe it's a compliance graph that needs to link regulations, entities, audits, and findings across a dozen data sources.

You've looked at the options: Neo4j, Amazon Neptune, Blazegraph, Virtuoso, Stardog. They all work. They also all mean standing up a separate database, building an ETL pipeline to feed it, and maintaining two sources of truth.

Here's the question nobody asks early enough: does the knowledge graph need its own database?

---

## The Impedance Mismatch Tax

Every dedicated graph database or triple store creates the same operational pattern:

1. Your operational data lives in PostgreSQL (or MySQL, or SQL Server).
2. Your knowledge graph lives in a separate system.
3. You build a pipeline to synchronize them.
4. The pipeline breaks. Silently, usually, and always on a Friday.

The pipeline is the tax. It's not the graph query language that's hard — SPARQL is well-specified and powerful. It's the synchronization. Getting data from where it's written to where it's queried, consistently, with acceptable latency, without losing updates or double-counting.

Teams that run both PostgreSQL and a dedicated triple store spend more engineering time on the pipeline than on the graph queries themselves. That's the impedance mismatch tax: the cost of maintaining two systems that both think they're the source of truth.

---

## What If the Triple Store *Is* PostgreSQL?

pg_ripple is a PostgreSQL extension that implements an RDF triple store with native SPARQL execution. There's no separate process, no external database, no synchronization pipeline. The triples live in PostgreSQL tables. The SPARQL queries compile to SQL and execute through PostgreSQL's query planner.

This means:

- **One database.** Your operational tables and your knowledge graph share the same PostgreSQL instance. A transaction that inserts a row into `orders` can also insert triples into the graph. ACID guarantees apply to both.

- **One backup strategy.** `pg_dump`, PITR, logical replication — they all work on the triple store because it's just tables.

- **One access control model.** PostgreSQL roles, row-level security, `GRANT`/`REVOKE` — they apply to the graph data the same way they apply to your relational tables.

- **One connection pool.** Your application talks to one database. No second connection string, no second connection pool, no second failure domain.

---

## But Can PostgreSQL Actually Do Graph Queries?

This is the reasonable objection. PostgreSQL is a relational database. Graph queries involve traversals, pattern matching, recursive paths, and joins across an unpredictable number of tables. Relational databases aren't built for that.

Except they are — if you give the optimizer the right SQL.

pg_ripple's storage model is called Vertical Partitioning (VP). Instead of one giant `(subject, predicate, object)` table, each unique predicate gets its own table with columns `(s, o, g)` — subject, object, graph. The predicate is implicit in the table name.

This means a SPARQL triple pattern like `?person foaf:knows ?friend` compiles to a scan of a single, narrow, well-indexed table — not a filter over a billion-row SPO table. The PostgreSQL optimizer sees a simple two-column index lookup, and it plans it accordingly.

A star pattern — "find someone's name, age, and email" — becomes a three-way join across three narrow tables, each indexed on subject. PostgreSQL's hash joins handle this efficiently.

A path query — "find all ancestors of X" — compiles to `WITH RECURSIVE` with PostgreSQL 18's hash-based cycle detection. The optimizer handles the recursion; pg_ripple handles the compilation.

The result is that most SPARQL queries execute in the same order of magnitude as hand-written SQL. You lose a constant factor for the translation overhead. You gain the ability to write queries in a language designed for graph patterns instead of contorting SQL into shapes it wasn't meant for.

---

## What You Get That Dedicated Triplestores Don't

### Full SQL Interop

Your SPARQL query results are PostgreSQL result sets. You can wrap them in a view. You can join them with relational tables. You can feed them into `pg_trickle` stream tables for incremental materialization. You can expose them via PostgREST or Hasura.

```sql
-- SPARQL result joined with a relational table
SELECT s.employee_name, s.department,
       r.salary, r.hire_date
FROM sparql('
  SELECT ?employee_name ?department WHERE {
    ?emp foaf:name ?employee_name ;
         org:memberOf ?dept .
    ?dept rdfs:label ?department .
  }
') AS s(employee_name text, department text)
JOIN hr.employees r ON r.name = s.employee_name;
```

Try doing that in Blazegraph.

### PostgreSQL Ecosystem

Every tool that works with PostgreSQL works with pg_ripple's data: pgAdmin, DBeaver, Metabase, Grafana, dbt, Airflow, any JDBC/ODBC driver. The triples are stored in real PostgreSQL tables. Any tool that can query PostgreSQL can query the triple store.

### Transactional Guarantees

Insert 10,000 triples in a transaction. If it fails, none of them are visible. If it succeeds, all of them are visible atomically. Dedicated triplestores have varying levels of transactional support — some offer it, some don't, and few match PostgreSQL's battle-tested MVCC implementation.

### Extensions

pgvector for semantic search over your graph entities. PostGIS for geospatial RDF data. pg_cron for scheduled inference. pg_stat_statements for query performance monitoring. The PostgreSQL extension ecosystem works because pg_ripple is just another extension in the same instance.

---

## When pg_ripple Is Not the Right Choice

Honesty matters more than marketing. pg_ripple is not the right tool when:

- **You need a public SPARQL endpoint with millions of concurrent users.** Dedicated triplestores like Virtuoso are built for this workload. pg_ripple is designed for application-integrated use, not internet-scale public query endpoints.

- **Your graph is billions of triples and your queries are full-graph analytics.** At that scale, distributed systems like Amazon Neptune or Apache Jena TDB2 with bulk-loading optimizations will outperform a single PostgreSQL instance. pg_ripple scales vertically and works best up to hundreds of millions of triples.

- **You don't use PostgreSQL.** If your stack is MySQL or MongoDB, pg_ripple can't help. It's a PostgreSQL extension, not a standalone service.

- **You need SPARQL 1.1 Entailment Regimes beyond OWL RL.** pg_ripple implements OWL 2 RL through Datalog. Full OWL DL reasoning requires a different architecture (tableau-based reasoners like HermiT or Pellet).

The sweet spot is teams that already run PostgreSQL, have a knowledge graph use case (ontologies, taxonomies, compliance, data integration, master data), and don't want to introduce a second database to serve it. That's a surprisingly large number of teams.

---

## The 5-Minute Test

```sql
-- Install the extension
CREATE EXTENSION pg_ripple;

-- Load some triples
SELECT pg_ripple.load_turtle('
  @prefix foaf: <http://xmlns.com/foaf/0.1/> .
  @prefix ex:   <http://example.org/> .

  ex:alice foaf:name "Alice" ;
           foaf:knows ex:bob .
  ex:bob   foaf:name "Bob" ;
           foaf:knows ex:carol .
  ex:carol foaf:name "Carol" .
');

-- Query them with SPARQL
SELECT * FROM pg_ripple.sparql('
  SELECT ?name ?friend_name WHERE {
    ?person foaf:name ?name ;
            foaf:knows ?friend .
    ?friend foaf:name ?friend_name .
  }
');
```

That's it. No separate server. No configuration file. No REST endpoint to set up. The triples are in PostgreSQL tables, indexed and queryable, inside the transaction that loaded them.

If you're already running PostgreSQL and you have a graph problem, the fastest path to a working solution is the one that doesn't require standing up a new database.
