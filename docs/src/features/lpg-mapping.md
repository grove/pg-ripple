# Cypher / LPG → RDF Mapping

If you are coming from Neo4j, Memgraph, or any other property-graph database, you already have a mental model of your data. This page shows how **property graph concepts map to RDF**, how to load a property graph schema into pg_ripple, and how to query it in SPARQL.

Nothing about your graph needs to change. The mapping is mechanical and reversible.

---

## Two models, one idea

Both **Labeled Property Graphs (LPG)** and **RDF** represent knowledge as a graph. The terminology differs:

| Property graph | RDF / pg_ripple |
|---|---|
| Node | Subject (an IRI or blank node) |
| Node label | `rdf:type` triple |
| Node property key + value | Predicate + object triple |
| Relationship type | Predicate IRI |
| Relationship property | RDF-star annotation `<< s p o >> key value` |
| `id()` internal identifier | IRI (you choose the naming scheme) |
| Named graph | Named graph (same concept) |
| No equivalent | Datatype-annotated literal (`"42"^^xsd:integer`) |

The only material difference is **relationship properties** (properties *on an edge* in LPG). In RDF these become **RDF-star quoted triples**.

---

## Naming scheme for IRIs

LPG nodes have internal integer IDs. You need to convert them to IRIs. A simple and robust scheme:

```
https://example.org/node/{label}/{id}
```

For example, a Neo4j node `(:Person {id: 42, name: "Alice"})` becomes:

```
<https://example.org/node/Person/42>  rdf:type  <https://example.org/vocab/Person> .
<https://example.org/node/Person/42>  <https://example.org/vocab/name>  "Alice" .
```

If your nodes have a domain-meaningful unique key (email, UUID, slug), use that instead of the internal ID — it makes the IRIs stable across re-imports.

---

## Translating a Cypher schema to RDF

### Nodes and node properties

**Cypher:**
```cypher
CREATE (:Person {name: "Alice", age: 30, active: true})
```

**RDF (Turtle):**
```turtle
@prefix ex:  <https://example.org/node/Person/> .
@prefix voc: <https://example.org/vocab/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:alice  a              voc:Person ;
          voc:name       "Alice" ;
          voc:age        30 ;
          voc:active     true .
```

### Relationships without properties

**Cypher:**
```cypher
MATCH (a:Person {name: "Alice"}), (b:Person {name: "Bob"})
CREATE (a)-[:KNOWS]->(b)
```

**RDF:**
```turtle
ex:alice  voc:KNOWS  ex:bob .
```

### Relationships *with* properties (RDF-star)

**Cypher:**
```cypher
CREATE (a)-[:KNOWS {since: 2020, strength: 0.9}]->(b)
```

**RDF-star (Turtle-star):**
```turtle
ex:alice  voc:KNOWS  ex:bob .

<< ex:alice  voc:KNOWS  ex:bob >>
    voc:since     2020 ;
    voc:strength  "0.9"^^xsd:decimal .
```

Store this in pg_ripple:

```sql
SELECT pg_ripple.load_turtle($TTL$
@prefix ex:  <https://example.org/node/Person/> .
@prefix voc: <https://example.org/vocab/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:alice  voc:KNOWS  ex:bob .
<< ex:alice  voc:KNOWS  ex:bob >>
    voc:since    "2020"^^xsd:integer ;
    voc:strength "0.9"^^xsd:decimal .
$TTL$);
```

---

## Translating common Cypher queries to SPARQL

### Node by property

```cypher
MATCH (p:Person {name: "Alice"}) RETURN p.age
```
```sparql
PREFIX voc: <https://example.org/vocab/>
SELECT ?age WHERE {
    ?p  a       voc:Person ;
        voc:name "Alice" ;
        voc:age  ?age .
}
```

### Traverse a relationship

```cypher
MATCH (a:Person {name: "Alice"})-[:KNOWS]->(b) RETURN b.name
```
```sparql
PREFIX ex:  <https://example.org/node/Person/>
PREFIX voc: <https://example.org/vocab/>
SELECT ?name WHERE {
    ex:alice  voc:KNOWS  ?b .
    ?b        voc:name   ?name .
}
```

### Multi-hop traversal (variable depth)

```cypher
MATCH (a:Person {name: "Alice"})-[:KNOWS*1..]->(b) RETURN b.name
```
```sparql
PREFIX ex:  <https://example.org/node/Person/>
PREFIX voc: <https://example.org/vocab/>
SELECT ?name WHERE {
    ex:alice  voc:KNOWS+  ?b .   # + = one or more hops
    ?b        voc:name    ?name .
}
```

Or `voc:KNOWS*` for zero-or-more.

### Relationship property filter

```cypher
MATCH (a)-[r:KNOWS]->(b) WHERE r.strength > 0.8 RETURN a.name, b.name
```
```sparql
PREFIX voc: <https://example.org/vocab/>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
SELECT ?aName ?bName WHERE {
    ?a voc:KNOWS ?b .
    << ?a voc:KNOWS ?b >> voc:strength ?str .
    FILTER(?str > "0.8"^^xsd:decimal)
    ?a voc:name ?aName .
    ?b voc:name ?bName .
}
```

### Aggregation

```cypher
MATCH (p:Person)-[:KNOWS]->(friend) RETURN p.name, count(friend) AS friends
```
```sparql
PREFIX voc: <https://example.org/vocab/>
SELECT ?name (COUNT(?friend) AS ?friends) WHERE {
    ?p  a           voc:Person ;
        voc:name    ?name ;
        voc:KNOWS   ?friend .
}
GROUP BY ?name
ORDER BY DESC(?friends)
```

---

## Bulk migration from Neo4j

The recommended path for migrating a Neo4j database:

1. **Export with `neo4j-admin dump`** or the APOC `export.graphml` procedure to GraphML or CSV.
2. **Convert to Turtle** with a small Python script (or use R2RML if the source is a JDBC view).
3. **Load into pg_ripple** with `pg_ripple.load_turtle_file()`.

A minimal Python translator for the CSV export:

```python
import csv, sys

BASE   = "https://example.org/node/"
VOCAB  = "https://example.org/vocab/"

# nodes.csv: nodeId, label, propKey1, propKey2, ...
with open("nodes.csv") as f:
    for row in csv.DictReader(f):
        iri = f"<{BASE}{row['label']}/{row['nodeId']}>"
        print(f"{iri} <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{VOCAB}{row['label']}> .")
        for k, v in row.items():
            if k not in ("nodeId", "label") and v:
                print(f"{iri} <{VOCAB}{k}> {_literal(v)} .")
```

For large dumps (> 100 M triples), use `COPY` via `load_ntriples_file()` rather than `load_turtle()` — N-Triples is streamed in bulk without parsing overhead.

---

## What you gain over LPG

Once your graph is in pg_ripple, you have access to capabilities that most LPG databases lack:

- **SHACL validation** — define and enforce a schema on the graph with formal guarantees.
- **OWL reasoning** — automatically derive `rdf:type` assertions from `owl:equivalentClass` axioms across multiple schemas.
- **Federated queries** — join your local graph with Wikidata, DBpedia, or any other SPARQL endpoint in a single query.
- **Vector + graph hybrid** — embed entities and run HNSW similarity search combined with SPARQL graph traversal.
- **Transactional writes** — graph writes, vector index updates, and relational table updates in a single PostgreSQL transaction.

---

## See also

- [Storing Knowledge — RDF-star](storing-knowledge.md) — relationship properties in detail.
- [Querying with SPARQL](querying-with-sparql.md) — full SPARQL 1.1 reference.
- [Record Linkage](record-linkage.md) — useful if migrating two systems with overlapping entities.
