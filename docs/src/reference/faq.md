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

**Import (loading):**
- N-Triples and N-Triples-star (`load_ntriples`)
- N-Quads (`load_nquads`)
- Turtle and Turtle-star (`load_turtle`)
- TriG (`load_trig`)
- RDF/XML (`load_rdfxml`, v0.9.0)

**Export:**
- N-Triples (`export_ntriples`)
- N-Quads (`export_nquads`)
- Turtle (`export_turtle`, v0.9.0) — including Turtle-star for RDF-star data
- JSON-LD expanded form (`export_jsonld`, v0.9.0)
- Streaming Turtle or JSON-LD for large graphs (`export_turtle_stream`, `export_jsonld_stream`, v0.9.0)

SPARQL CONSTRUCT and DESCRIBE results can be serialized directly to Turtle or JSON-LD via `sparql_construct_turtle`, `sparql_construct_jsonld`, `sparql_describe_turtle`, and `sparql_describe_jsonld` (v0.9.0).

### Can I use pg_ripple with JSON-LD for REST APIs?

Yes.  Use `export_jsonld()` or `sparql_construct_jsonld()` to produce JSON-LD responses:

```sql
-- Full graph as JSON-LD
SELECT pg_ripple.export_jsonld('https://myapp.example.org/graph/users');

-- SPARQL-driven selection as JSON-LD
SELECT pg_ripple.sparql_construct_jsonld('
  CONSTRUCT { ?s ?p ?o }
  WHERE { ?s a <https://schema.org/Person> ; ?p ?o }
');
```

The output is JSON-LD in expanded form — each subject is one array entry with IRI keys and typed value arrays.

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

---

## HTAP & Operations (v0.6.0)

### Does pg_ripple require shared_preload_libraries?

For full HTAP functionality (background merge worker, latch-poke hook, shared-memory statistics) you must add pg_ripple to `shared_preload_libraries`:

```ini
shared_preload_libraries = 'pg_ripple'
```

Without this, the extension still works for reads and writes — but all writes stay in delta tables and are never automatically merged into main. Queries on predicates with large deltas will be slower than expected.

See the [Pre-Deployment Checklist](../user-guide/pre-deployment.md) for the complete setup sequence.

### What is the difference between compact() and the merge worker?

| | `compact()` | Merge worker |
|---|---|---|
| **Trigger** | Manual SQL call | Automatic (latch poke or timer) |
| **Blocks caller** | Yes | No — runs in background |
| **When to use** | Maintenance windows, tests | Production continuous operation |

Both produce the same result: delta rows are moved into main, tombstones are cleared, and a fresh BRIN index is built.

### How do I know if the merge worker is keeping up?

```sql
-- Check unmerged row count
SELECT pg_ripple.stats() -> 'unmerged_delta_rows';

-- Watch it over time
SELECT now(), (pg_ripple.stats() -> 'unmerged_delta_rows')::int AS lag
FROM generate_series(1, 10) g,
     pg_sleep(5) AS _s
WHERE true;  -- run this manually in a loop
```

A healthy deployment shows `unmerged_delta_rows` rising during writes and falling after merges. If it only rises, the worker is behind — lower `merge_threshold` or increase server I/O capacity.

### Can I subscribe to triple changes in real time?

Yes. CDC (Change Data Capture) is available in v0.6.0 via PostgreSQL `NOTIFY`:

```sql
-- Subscribe to a specific predicate
SELECT pg_ripple.subscribe('<https://schema.org/name>', 'name_changes');

-- In another session
LISTEN name_changes;

-- Notifications arrive when triples are inserted or deleted
SELECT pg_ripple.insert_triple(
    '<https://example.org/Alice>',
    '<https://schema.org/name>',
    '"Alice"'
);
```

Subscriptions are stored in `_pg_ripple.cdc_subscriptions` and persist across reconnects (but must be re-registered after a server restart). See the [Administration](../user-guide/sql-reference/admin.md#subscribepattern-channel) reference for details.

### Why does my query not see recently inserted triples?

If you inserted triples and immediately queried with SPARQL, the results should include those triples — delta tables are always queried alongside main tables.

If triples are missing, check:
1. The triple was committed (not inside an uncommitted transaction)
2. The correct graph is being queried (default graph vs named graph)
3. The correct predicate IRI spelling was used
