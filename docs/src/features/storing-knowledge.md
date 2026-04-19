# §2.1 Storing Knowledge

## What and Why

pg_ripple stores data as **RDF triples** — the W3C standard for representing knowledge.
Every fact is a three-part statement: a **subject**, a **predicate**, and an **object**.
This structure is deceptively simple but powerful enough to model any domain — from
bibliographic records and biomedical ontologies to enterprise knowledge graphs.

Why triples instead of tables?

- **Schema-free evolution**: add new predicates without ALTER TABLE.
- **Natural linking**: every entity is an IRI — links across datasets are free.
- **Standards-based**: SPARQL, SHACL, OWL, and thousands of public vocabularies work out of the box.
- **Provenance-ready**: RDF-star lets you annotate individual facts with confidence scores, sources, and timestamps.

pg_ripple stores triples inside PostgreSQL using **Vertical Partitioning (VP)** — one
internal table per predicate, with all values dictionary-encoded as `BIGINT`. You never
see this machinery directly; you interact through `insert_triple()`, `load_turtle()`,
and SPARQL.

---

## How It Works

### The Triple Model

Every RDF triple has the form:

```
<subject>  <predicate>  <object> .
```

- **Subject**: the thing you are describing (always an IRI or blank node).
- **Predicate**: the relationship or property (always an IRI).
- **Object**: the value — an IRI (another entity), a literal (string, number, date), or a blank node.

```admonish note
IRIs (Internationalized Resource Identifiers) look like URLs but are identifiers, not
necessarily web addresses.  `<https://example.org/paper/42>` identifies a paper —
it does not need to resolve to a web page.
```

### Named Graphs

Triples can be grouped into **named graphs** — logical partitions identified by an IRI.
This is useful for:

- Tracking provenance: "these triples came from PubMed"
- Multi-tenancy: one graph per customer
- Inference output: derived triples go into a separate graph

pg_ripple uses graph ID `0` for the **default graph** (triples with no explicit graph).
Named graphs get a positive integer ID via dictionary encoding.

### Blank Nodes

Blank nodes are anonymous identifiers — they represent "something exists" without
giving it a global IRI. pg_ripple encodes blank nodes with a `_:` prefix:

```sql
SELECT pg_ripple.insert_triple(
    '_:review1',
    '<https://schema.org/author>',
    '<https://example.org/person/alice>'
);
```

```admonish warning
Blank nodes are **document-scoped**. Two separate `load_turtle()` calls that both use
`_:x` will create two different internal identifiers. If you need stable cross-document
identity, use IRIs instead.
```

### RDF-Star (Quoted Triples)

RDF-star lets you make statements **about** other statements. This is essential for
provenance, confidence scores, and temporal annotations.

A quoted triple wraps `<< subject predicate object >>` and can appear as a subject
or object in another triple:

```sql
-- "The fact that Paper42 was authored by Alice has confidence 0.95"
SELECT pg_ripple.insert_triple(
    '<< <https://example.org/paper/42> <https://purl.org/dc/terms/creator> <https://example.org/person/alice> >>',
    '<https://example.org/confidence>',
    '"0.95"^^<http://www.w3.org/2001/XMLSchema#decimal>'
);
```

### Dictionary Encoding

Every IRI, blank node, and literal is mapped to a `BIGINT` (i64) via XXH3-128 hashing
before storage. VP tables contain only integers — this makes joins fast and storage
compact. You never need to think about encoding; pg_ripple handles it transparently.

---

## Worked Examples

The examples in this chapter use a bibliographic dataset: papers, authors, institutions,
journals, and citations.

### Setting Up Prefixes

Register namespace prefixes so SPARQL queries are readable:

```sql
SELECT pg_ripple.register_prefix('ex',    'https://example.org/');
SELECT pg_ripple.register_prefix('dct',   'http://purl.org/dc/terms/');
SELECT pg_ripple.register_prefix('foaf',  'http://xmlns.com/foaf/0.1/');
SELECT pg_ripple.register_prefix('bibo',  'http://purl.org/ontology/bibo/');
SELECT pg_ripple.register_prefix('schema','https://schema.org/');
SELECT pg_ripple.register_prefix('xsd',   'http://www.w3.org/2001/XMLSchema#');
```

### Inserting Individual Triples

```sql
-- Create a paper
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/42>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://purl.org/ontology/bibo/AcademicArticle>'
);

-- Add a title
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/42>',
    '<http://purl.org/dc/terms/title>',
    '"Knowledge Graphs in Practice"'
);

-- Add an author
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/42>',
    '<http://purl.org/dc/terms/creator>',
    '<https://example.org/person/alice>'
);

-- Author metadata
SELECT pg_ripple.insert_triple(
    '<https://example.org/person/alice>',
    '<http://xmlns.com/foaf/0.1/name>',
    '"Alice Johnson"'
);

SELECT pg_ripple.insert_triple(
    '<https://example.org/person/alice>',
    '<https://schema.org/affiliation>',
    '<https://example.org/institution/mit>'
);

-- Institution metadata
SELECT pg_ripple.insert_triple(
    '<https://example.org/institution/mit>',
    '<http://xmlns.com/foaf/0.1/name>',
    '"Massachusetts Institute of Technology"'
);
```

### Loading a Full Dataset with Turtle

For bulk data, Turtle format is more natural:

```sql
SELECT pg_ripple.load_turtle('
@prefix ex:     <https://example.org/> .
@prefix dct:    <http://purl.org/dc/terms/> .
@prefix foaf:   <http://xmlns.com/foaf/0.1/> .
@prefix bibo:   <http://purl.org/ontology/bibo/> .
@prefix schema: <https://schema.org/> .
@prefix xsd:    <http://www.w3.org/2001/XMLSchema#> .

ex:paper/42 a bibo:AcademicArticle ;
    dct:title "Knowledge Graphs in Practice" ;
    dct:creator ex:person/alice, ex:person/bob ;
    dct:date "2024-03-15"^^xsd:date ;
    bibo:citedBy ex:paper/99 ;
    schema:keywords "knowledge graph", "RDF", "SPARQL" .

ex:paper/99 a bibo:AcademicArticle ;
    dct:title "Graph Neural Networks for Entity Resolution" ;
    dct:creator ex:person/carol ;
    bibo:cites ex:paper/42 .

ex:person/alice foaf:name "Alice Johnson" ;
    schema:affiliation ex:institution/mit .

ex:person/bob foaf:name "Bob Smith" ;
    schema:affiliation ex:institution/stanford .

ex:person/carol foaf:name "Carol Williams" ;
    schema:affiliation ex:institution/mit .

ex:institution/mit foaf:name "Massachusetts Institute of Technology" .
ex:institution/stanford foaf:name "Stanford University" .
');
```

### Using Named Graphs

Store triples from different sources in separate graphs:

```sql
-- Create named graphs for different data sources
SELECT pg_ripple.create_graph('https://example.org/graph/pubmed');
SELECT pg_ripple.create_graph('https://example.org/graph/arxiv');

-- Load PubMed data into its graph
SELECT pg_ripple.load_turtle_into_graph('
@prefix ex:   <https://example.org/> .
@prefix dct:  <http://purl.org/dc/terms/> .
@prefix bibo: <http://purl.org/ontology/bibo/> .

ex:paper/100 a bibo:AcademicArticle ;
    dct:title "Drug Interaction Networks" ;
    dct:creator ex:person/dave .
', 'https://example.org/graph/pubmed');

-- Load arXiv data into its graph
SELECT pg_ripple.load_turtle_into_graph('
@prefix ex:   <https://example.org/> .
@prefix dct:  <http://purl.org/dc/terms/> .
@prefix bibo: <http://purl.org/ontology/bibo/> .

ex:paper/200 a bibo:AcademicArticle ;
    dct:title "Transformer Architectures for NLP" ;
    dct:creator ex:person/eve .
', 'https://example.org/graph/arxiv');

-- List all named graphs
SELECT * FROM pg_ripple.list_graphs();
```

### RDF-Star for Provenance and Confidence

Annotate citations with provenance metadata:

```sql
-- Record that Paper 42 cites Paper 99 (the base fact)
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/42>',
    '<http://purl.org/ontology/bibo/cites>',
    '<https://example.org/paper/99>'
);

-- Annotate this citation with a confidence score
SELECT pg_ripple.insert_triple(
    '<< <https://example.org/paper/42> <http://purl.org/ontology/bibo/cites> <https://example.org/paper/99> >>',
    '<https://example.org/confidence>',
    '"0.92"^^<http://www.w3.org/2001/XMLSchema#decimal>'
);

-- Record who asserted this citation
SELECT pg_ripple.insert_triple(
    '<< <https://example.org/paper/42> <http://purl.org/ontology/bibo/cites> <https://example.org/paper/99> >>',
    '<http://purl.org/dc/terms/source>',
    '<https://example.org/system/citation-extractor>'
);
```

### Translating a Relational Schema to RDF

Suppose you have a relational database with tables `papers`, `authors`, and `affiliations`:

| papers.id | papers.title | papers.year |
|---|---|---|
| 42 | Knowledge Graphs in Practice | 2024 |

| authors.id | authors.name | authors.institution_id |
|---|---|---|
| 1 | Alice Johnson | 10 |

The mapping pattern:

1. **Each row becomes a subject IRI**: `<https://example.org/paper/{id}>`
2. **Each column becomes a predicate**: use a standard vocabulary (Dublin Core, Schema.org, FOAF)
3. **Foreign keys become object IRIs**: `authors.institution_id = 10` → `<https://example.org/institution/10>`
4. **Scalar values become literals**: `papers.title` → `"Knowledge Graphs in Practice"`

```sql
-- Row from papers table → triples
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/42>',
    '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>',
    '<http://purl.org/ontology/bibo/AcademicArticle>'
);
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/42>',
    '<http://purl.org/dc/terms/title>',
    '"Knowledge Graphs in Practice"'
);
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/42>',
    '<http://purl.org/dc/terms/date>',
    '"2024"^^<http://www.w3.org/2001/XMLSchema#gYear>'
);

-- Foreign key → IRI link
SELECT pg_ripple.insert_triple(
    '<https://example.org/person/1>',
    '<https://schema.org/affiliation>',
    '<https://example.org/institution/10>'
);
```

---

## Common Patterns

### Pattern: Type Hierarchies

Use `rdf:type` and `rdfs:subClassOf` to create type hierarchies:

```sql
SELECT pg_ripple.load_turtle('
@prefix ex:   <https://example.org/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix bibo: <http://purl.org/ontology/bibo/> .

bibo:AcademicArticle rdfs:subClassOf bibo:Article .
bibo:Article rdfs:subClassOf bibo:Document .

ex:paper/42 a bibo:AcademicArticle .
');
```

With RDFS inference enabled (see [§2.5](../features/reasoning-and-inference.md)),
pg_ripple can automatically derive that `ex:paper/42` is also a `bibo:Article` and
a `bibo:Document`.

### Pattern: Multi-Valued Properties

Unlike relational columns, RDF predicates are naturally multi-valued:

```sql
-- A paper can have multiple authors — just insert multiple triples
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/42>',
    '<http://purl.org/dc/terms/creator>',
    '<https://example.org/person/alice>'
);
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/42>',
    '<http://purl.org/dc/terms/creator>',
    '<https://example.org/person/bob>'
);
```

### Pattern: Typed and Language-Tagged Literals

```sql
-- Typed literal (date)
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/42>',
    '<http://purl.org/dc/terms/date>',
    '"2024-03-15"^^<http://www.w3.org/2001/XMLSchema#date>'
);

-- Language-tagged string
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/42>',
    '<http://purl.org/dc/terms/title>',
    '"Knowledge Graphs in Practice"@en'
);
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/42>',
    '<http://purl.org/dc/terms/title>',
    '"Wissensgraphen in der Praxis"@de'
);
```

### Pattern: Reification with RDF-Star vs Named Graphs

Two approaches for tracking who said what:

**RDF-star** — annotate individual triples:

```sql
SELECT pg_ripple.insert_triple(
    '<< <https://example.org/paper/42> <http://purl.org/dc/terms/creator> <https://example.org/person/alice> >>',
    '<http://purl.org/dc/terms/source>',
    '<https://example.org/dataset/pubmed>'
);
```

**Named graphs** — group triples by source:

```sql
SELECT pg_ripple.load_turtle_into_graph('
@prefix ex:  <https://example.org/> .
@prefix dct: <http://purl.org/dc/terms/> .
ex:paper/42 dct:creator ex:person/alice .
', 'https://example.org/dataset/pubmed');
```

Use RDF-star when different triples about the same entity have different provenance.
Use named graphs when entire batches share the same source.

---

## Performance and Trade-offs

| Approach | Insert rate | Query flexibility | Storage overhead |
|---|---|---|---|
| `insert_triple()` | ~5,000 triples/s | Full | Highest (per-call overhead) |
| `load_turtle()` | ~50,000 triples/s | Full | Low (batch dictionary encoding) |
| `load_turtle_file()` | ~100,000 triples/s | Full | Lowest (server-side streaming) |

- **Dictionary cache**: frequently used IRIs (predicates, common types) stay in the
  shared-memory LRU cache. Check hit rates with `SELECT pg_ripple.cache_stats()`.
- **VP table promotion**: predicates with fewer than 1,000 triples share the `vp_rare`
  consolidation table. Once a predicate crosses the threshold, it gets its own dedicated
  VP table with dual B-tree indexes.
- **Named graph overhead**: the `g` column adds 8 bytes per triple. If you do not need
  named graphs, using the default graph (the default) avoids the cost of graph-ID lookups.

```admonish tip
After large bulk loads, run `ANALYZE` on the internal tables to update PostgreSQL planner statistics:
```sql
SELECT pg_ripple.vacuum();
```
```

---

## Gotchas and Debugging

**IRI formatting**: All IRIs must be wrapped in angle brackets (`<...>`) in function calls.
Forgetting the brackets is the most common error:

```sql
-- WRONG: will be treated as a plain literal
SELECT pg_ripple.insert_triple(
    'https://example.org/paper/42',
    'http://purl.org/dc/terms/title',
    '"Hello"'
);

-- CORRECT: angle brackets around IRIs
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/42>',
    '<http://purl.org/dc/terms/title>',
    '"Hello"'
);
```

**Blank node scoping**: Blank nodes from separate `load_turtle()` calls are independent.
Two calls using `_:x` create two different entities.

**Literal quoting**: Literals must be wrapped in double quotes within the single-quoted SQL
string. Typed literals use `^^<datatype>` suffix:

```sql
-- Plain string
'"Hello"'

-- Typed integer
'"42"^^<http://www.w3.org/2001/XMLSchema#integer>'

-- Language-tagged string
'"Bonjour"@fr'
```

**Checking what is stored**: Use `find_triples()` with wildcards to inspect data:

```sql
-- All triples about Paper 42
SELECT * FROM pg_ripple.find_triples(
    '<https://example.org/paper/42>', NULL, NULL
);

-- All triples with the dct:creator predicate
SELECT * FROM pg_ripple.find_triples(
    NULL, '<http://purl.org/dc/terms/creator>', NULL
);

-- Total triple count
SELECT pg_ripple.triple_count();
```

**Duplicate triples**: Inserting the same (s, p, o, g) twice is idempotent — the second
insert returns the existing SID. Use `deduplicate_all()` to clean up historical duplicates.

---

## Next Steps

- **[§2.2 Loading Data](../features/loading-data.md)** — bulk loading in all RDF formats with performance tuning.
- **[§2.3 Querying with SPARQL](../features/querying-with-sparql.md)** — query the triples you stored.
- **[§2.4 Validating Data Quality](../features/validating-data-quality.md)** — enforce schema constraints with SHACL.
