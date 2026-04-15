# FAQ

## General

### Why VP tables instead of one big triple table?

A single `(s, p, o, g)` table with 100M triples requires a B-tree index that touches all four columns for any useful predicate-specific query. Each query must scan rows for all predicates regardless of the filter.

Vertical Partitioning (one table per predicate) means a query for `<ex:knows>` triples only scans the `vp_{knows_id}` table — typically a fraction of the total data. The two B-tree indexes on `(s, o)` and `(o, s)` are small and cache-friendly. SPARQL star-patterns (same subject, multiple predicates) become simple multi-way joins between small tables.

### Why PostgreSQL 18?

pg_ripple uses the `CYCLE` clause in `WITH RECURSIVE` CTEs for hash-based cycle detection in property path queries. The `CYCLE` clause was introduced in PostgreSQL 14 but the hash-based variant (as opposed to array-based) first became performant in PG 17/18. PG 18 is also the first version where pgrx 0.17 has stable support.

### Is pg_ripple compatible with LPG tools?

Not yet. A Cypher/GQL compatibility layer is on the v0.13.0 roadmap. The VP storage structure is architecturally aligned with LPG — each VP table is a property edge type — so the mapping will be natural.

### What RDF formats does pg_ripple support?

- **Load**: N-Triples, N-Triples-star, Turtle, N-Quads, TriG
- **Export**: N-Triples, Turtle (v0.9.0), JSON-LD (v0.9.0), RDF/XML (v0.9.0)

---

## SPARQL

### What SPARQL 1.1 features are supported?

As of v0.5.0:

- SELECT, ASK
- BGP (basic graph patterns)
- OPTIONAL (LeftJoin)
- UNION / MINUS
- FILTER (comparison, string, boolean operators)
- Property paths: `+`, `*`, `?`, `/`, `|`, `^`
- GROUP BY / HAVING / aggregates (COUNT, SUM, AVG, MIN, MAX, GROUP_CONCAT)
- Subqueries
- BIND / VALUES
- Named graphs via `GRAPH`
- ORDER BY, LIMIT, OFFSET, DISTINCT

Not yet: CONSTRUCT, DESCRIBE, SPARQL Update, SERVICE (federation).

### Does pg_ripple support SPARQL 1.1 property paths?

Yes, as of v0.5.0. All standard path operators are supported: `+`, `*`, `?`, `/` (sequence), `|` (alternative), `^` (inverse). Negated property sets `!(p1|p2)` are partially supported via `vp_rare`.

Property path queries compile to `WITH RECURSIVE` CTEs with PostgreSQL 18's `CYCLE` clause for hash-based cycle detection.

### What is the maximum traversal depth for property paths?

Controlled by the `pg_ripple.max_path_depth` GUC (default: 100). Set it lower to prevent runaway queries on dense graphs:

```sql
SET pg_ripple.max_path_depth = 10;
```

### Why does my FILTER not match a number?

SPARQL FILTER comparisons on numeric literals (`FILTER(?age >= 18)`) require the literal to be typed with an XSD numeric type:

```
"18"^^<http://www.w3.org/2001/XMLSchema#integer>
```

Plain string literals like `"18"` are compared as strings. Use typed literals when inserting numeric data, or cast in the FILTER expression.

---

## Data modeling

### What's the difference between a named graph and a blank node?

A **named graph** is a set of triples identified by an IRI. It is used for partitioning data by source, time, or topic. You can query across all named graphs, query within a specific graph, or count triples per graph.

A **blank node** is a resource without a global IRI identity — it has identity only within a document load scope. Blank nodes are used for anonymous resources (e.g. intermediate nodes in a structure) that don't need a stable identifier.

### What is an RDF-star quoted triple?

A quoted triple `<< s p o >>` is a triple that can appear in subject or object position in another triple. It enables statements *about* triples — useful for provenance (`<< alice knows bob >> :assertedBy :carol`), temporal annotations, and confidence scores.

pg_ripple stores quoted triples as dictionary entries of `kind = 5`. See [RDF-star](../sql-reference/rdf-star.md) for details.

---

## Performance

### How fast is bulk load?

On a modern server with an NVMe SSD, `load_ntriples()` processes approximately 50,000–150,000 triples per second (single connection, default settings). Performance depends on predicate diversity (more unique predicates → more VP tables created), hardware, and PostgreSQL configuration.

### When should I use SPARQL vs `find_triples`?

`find_triples()` only matches a single (s, p, o, g) pattern — it is equivalent to a SPARQL BGP with exactly one triple pattern. Use it for single-pattern lookups.

Use `sparql()` for anything more complex: multi-pattern joins, OPTIONAL, FILTER, aggregates, property paths, or when you want the ergonomics of SPARQL's variable-binding model.
