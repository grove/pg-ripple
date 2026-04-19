# Key Concepts — RDF for PostgreSQL Users

If you know PostgreSQL, you already understand most of what you need to work with pg_ripple. This page maps RDF concepts to their PostgreSQL equivalents.

## Triples

A **triple** is the atomic unit of data in RDF. It has three parts:

| Part | What it is | PostgreSQL analogy |
|---|---|---|
| **Subject** | The entity being described | A row's primary key |
| **Predicate** | The relationship or attribute | A column name |
| **Object** | The value or related entity | A cell value or foreign key |

For example, the fact "Alice knows Bob" is the triple:

```
<http://example.org/alice> <http://xmlns.com/foaf/0.1/knows> <http://example.org/bob> .
```

In pg_ripple, this triple is stored in a VP table named after the predicate (`foaf:knows`), with integer-encoded subject and object columns.

## IRIs

An **IRI** (Internationalized Resource Identifier) is a globally unique identifier for an entity or relationship. Think of it as a namespaced primary key that is guaranteed unique across all datasets in the world.

```
http://example.org/alice          -- an entity
http://xmlns.com/foaf/0.1/knows   -- a relationship
```

**Prefixes** are shortcuts to avoid writing full IRIs repeatedly:

```sql
SELECT pg_ripple.register_prefix('ex', 'http://example.org/');
-- Now ex:alice means http://example.org/alice
```

## Blank nodes

A **blank node** is an anonymous entity — like a row with no primary key. It exists only within the document where it was created.

```turtle
ex:alice foaf:address [ foaf:city "Boston" ; foaf:country "US" ] .
```

The address has no IRI. It is a blank node, identified internally by a system-generated label. Blank nodes from different `load_turtle()` calls are always distinct entities, even if they share the same label.

```admonish warning
Blank nodes cannot be referenced from outside their originating load call. If you need to reference an entity from multiple places, give it an IRI.
```

## Literals

A **literal** is a data value — a string, number, date, or boolean. Literals can have a datatype or a language tag.

| Literal | Type | PostgreSQL equivalent |
|---|---|---|
| `"Alice"` | Plain string | `TEXT` |
| `"42"^^xsd:integer` | Typed integer | `INTEGER` |
| `"2024-01-15"^^xsd:date` | Typed date | `DATE` |
| `"Bonjour"@fr` | Language-tagged string | No direct equivalent |

In pg_ripple, all literals are dictionary-encoded to compact integer IDs for storage. The original string representation is preserved and decoded on query output.

## Predicates and VP tables

In a relational database, a table groups all attributes of a single entity type. In pg_ripple, data is organized by **predicate** — each unique predicate gets its own table (a Vertical Partitioning, or VP, table).

```
Relational:  persons(id, name, email, knows_id)
pg_ripple:   vp_foaf_name(s, o)      -- subject → name
             vp_foaf_knows(s, o)     -- subject → object
             vp_schema_email(s, o)   -- subject → email
```

This structure makes join-heavy SPARQL queries fast because each predicate's data is co-located and indexed.

## Named graphs

A **named graph** is a labeled collection of triples — like a PostgreSQL schema that groups related tables.

```sql
-- Create a named graph
SELECT pg_ripple.create_graph('http://example.org/publications');

-- Load data into it
SELECT pg_ripple.load_turtle_into_graph(
  '<http://example.org/paper1> <http://purl.org/dc/elements/1.1/title> "My Paper" .',
  'http://example.org/publications'
);
```

Named graphs are useful for:

- **Multi-source data**: keep data from different sources separate
- **Access control**: grant read access to specific graphs per role
- **Versioning**: load new data into a fresh graph, validate, then swap

All triples without an explicit graph belong to the **default graph** (graph ID = 0).

## RDF-star

Standard RDF says "Alice knows Bob." But what if you want to say *when* Alice met Bob, or *who* recorded that fact? **RDF-star** lets you make statements about statements:

```
<< ex:alice foaf:knows ex:bob >> ex:since "2020"^^xsd:gYear .
```

This says: "The fact that Alice knows Bob has been true since 2020." In pg_ripple, each triple has a statement identifier (SID) that can be used as the subject or object of other triples, enabling edge properties similar to labeled property graphs.

## SPARQL

**SPARQL** is the standard query language for RDF data — the equivalent of SQL for relational databases. Where SQL queries tables, SPARQL queries graph patterns.

| SQL | SPARQL |
|---|---|
| `SELECT name FROM persons WHERE id = 1` | `SELECT ?name WHERE { ex:person1 foaf:name ?name }` |
| `JOIN` | Graph pattern matching (implicit) |
| `LEFT JOIN` | `OPTIONAL { }` |
| `WHERE x IN (...)` | `VALUES (?x) { ... }` |
| `GROUP BY ... HAVING` | `GROUP BY ... HAVING` |
| `WITH RECURSIVE` | Property paths (`foaf:knows+`) |

In pg_ripple, SPARQL queries are compiled to SQL and executed via PostgreSQL's query engine. You call them through `pg_ripple.sparql()`:

```sql
SELECT * FROM pg_ripple.sparql('
  PREFIX foaf: <http://xmlns.com/foaf/0.1/>
  SELECT ?name WHERE { ?person foaf:name ?name }
');
```

## Dictionary encoding

pg_ripple does not store raw strings in its data tables. Every IRI, blank node, and literal is mapped to a compact `BIGINT` (i64) by the dictionary encoder. VP tables contain only integer columns, making joins and comparisons fast.

You never need to interact with dictionary IDs directly — `sparql()` and `find_triples()` handle encoding and decoding automatically. For advanced use cases, `encode_term()` and `decode_id()` are available.

## Summary of analogies

| RDF concept | PostgreSQL analogy |
|---|---|
| Triple | Row in a table |
| Subject | Primary key value |
| Predicate | Column name / table name (VP) |
| Object | Cell value or foreign key |
| IRI | Globally unique identifier |
| Blank node | Row with system-generated ID |
| Literal | Typed column value |
| Named graph | Schema |
| SPARQL | SQL |
| SHACL shape | CHECK constraint / trigger |
| Datalog rule | Materialized view definition |

## Next steps

- [Storing Knowledge](../features/storing-knowledge.md) — data modeling with triples
- [Loading Data](../features/loading-data.md) — all import formats and methods
- [Querying with SPARQL](../features/querying-with-sparql.md) — the full query language
