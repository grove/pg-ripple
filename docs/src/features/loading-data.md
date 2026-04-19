# §2.2 Loading Data

## What and Why

Getting data into pg_ripple is the first step in building a knowledge graph. pg_ripple
supports every major RDF serialization format and offers three loading strategies tuned
for different scenarios: inline string loading, server-side file loading, and
single-triple insertion.

Choosing the right format and loading mode matters. A 10-million-triple dataset loaded
via `insert_triple()` in a loop takes hours; the same dataset loaded from a server-side
N-Triples file via `load_ntriples_file()` finishes in minutes.

---

## How It Works

### Supported Formats

| Format | Function (string) | Function (file) | Named graphs | Notes |
|---|---|---|---|---|
| **Turtle** | `load_turtle()` | `load_turtle_file()` | No (use `load_turtle_into_graph()`) | Human-readable; supports prefixes, RDF-star |
| **N-Triples** | `load_ntriples()` | `load_ntriples_file()` | No (use `load_ntriples_into_graph()`) | One triple per line; fastest to parse |
| **N-Quads** | `load_nquads()` | `load_nquads_file()` | Yes (inline) | N-Triples + fourth graph column |
| **TriG** | `load_trig()` | `load_trig_file()` | Yes (inline) | Turtle + named graph blocks |
| **RDF/XML** | `load_rdfxml()` | `load_rdfxml_file()` | No (use `load_rdfxml_into_graph()`) | Legacy XML format; widely supported |

### Three Loading Modes

**Mode 1: String loading** — pass RDF text as a SQL string parameter. Best for small-to-medium
datasets (up to a few MB) and interactive use:

```sql
SELECT pg_ripple.load_turtle('
@prefix ex: <https://example.org/> .
ex:paper/1 ex:title "Hello World" .
');
```

**Mode 2: Server-side file loading** — read from a file on the PostgreSQL server's filesystem.
Best for large datasets. Requires superuser privileges:

```sql
SELECT pg_ripple.load_turtle_file('/data/papers.ttl');
```

**Mode 3: Single-triple insertion** — insert one triple at a time. Best for real-time
ingestion from application code:

```sql
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/1>',
    '<https://example.org/title>',
    '"Hello World"'
);
```

### The Loading Pipeline

Regardless of format, every loader follows the same internal pipeline:

1. **Parse** — deserialize the RDF serialization into (subject, predicate, object, graph) quads.
2. **Encode** — dictionary-encode each IRI, blank node, and literal to a `BIGINT` ID using batch `ON CONFLICT DO NOTHING ... RETURNING`.
3. **Route** — look up the predicate in `_pg_ripple.predicates` to find the target VP table (or `vp_rare`).
4. **Insert** — batch-insert encoded `(s, o, g)` rows into the appropriate VP delta table.

```admonish tip
String loaders process the entire input in a single transaction. If any triple fails
to parse with `strict = true`, the entire load is rolled back. With `strict = false`
(the default), malformed triples are skipped and a WARNING is emitted.
```

---

## Worked Examples

### Loading Turtle

The most common format for hand-authored data:

```sql
SELECT pg_ripple.load_turtle('
@prefix ex:    <https://example.org/> .
@prefix dct:   <http://purl.org/dc/terms/> .
@prefix foaf:  <http://xmlns.com/foaf/0.1/> .
@prefix bibo:  <http://purl.org/ontology/bibo/> .
@prefix schema: <https://schema.org/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .

ex:paper/42 a bibo:AcademicArticle ;
    dct:title "Knowledge Graphs in Practice"@en ;
    dct:creator ex:person/alice, ex:person/bob ;
    dct:date "2024-03-15"^^xsd:date ;
    bibo:citedBy ex:paper/99 ;
    schema:keywords "knowledge graph", "RDF", "SPARQL" .

ex:paper/99 a bibo:AcademicArticle ;
    dct:title "Graph Neural Networks for Entity Resolution" ;
    dct:creator ex:person/carol .

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

The function returns the number of triples loaded:

```sql
-- Returns: 15
```

### Loading N-Triples

N-Triples is one triple per line with no abbreviations — optimal for machine-generated data:

```sql
SELECT pg_ripple.load_ntriples('
<https://example.org/paper/42> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://purl.org/ontology/bibo/AcademicArticle> .
<https://example.org/paper/42> <http://purl.org/dc/terms/title> "Knowledge Graphs in Practice" .
<https://example.org/paper/42> <http://purl.org/dc/terms/creator> <https://example.org/person/alice> .
<https://example.org/paper/42> <http://purl.org/dc/terms/creator> <https://example.org/person/bob> .
');
```

### Loading N-Quads (with Named Graphs)

N-Quads extend N-Triples with a fourth field for the graph IRI:

```sql
SELECT pg_ripple.load_nquads('
<https://example.org/paper/42> <http://purl.org/dc/terms/title> "Knowledge Graphs in Practice" <https://example.org/graph/pubmed> .
<https://example.org/paper/99> <http://purl.org/dc/terms/title> "Graph Neural Networks" <https://example.org/graph/arxiv> .
<https://example.org/paper/42> <http://purl.org/dc/terms/creator> <https://example.org/person/alice> <https://example.org/graph/pubmed> .
');
```

### Loading TriG (Turtle with Named Graphs)

TriG wraps Turtle blocks in `GRAPH { }` sections:

```sql
SELECT pg_ripple.load_trig('
@prefix ex:  <https://example.org/> .
@prefix dct: <http://purl.org/dc/terms/> .
@prefix bibo: <http://purl.org/ontology/bibo/> .

GRAPH ex:graph/pubmed {
    ex:paper/100 a bibo:AcademicArticle ;
        dct:title "Drug Interaction Networks" ;
        dct:creator ex:person/dave .
}

GRAPH ex:graph/arxiv {
    ex:paper/200 a bibo:AcademicArticle ;
        dct:title "Transformer Architectures for NLP" ;
        dct:creator ex:person/eve .
}
');
```

### Loading RDF/XML

The original XML serialization of RDF — common in older datasets and OWL ontologies:

```sql
SELECT pg_ripple.load_rdfxml('
<?xml version="1.0" encoding="UTF-8"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:dct="http://purl.org/dc/terms/"
         xmlns:bibo="http://purl.org/ontology/bibo/">
  <bibo:AcademicArticle rdf:about="https://example.org/paper/42">
    <dct:title>Knowledge Graphs in Practice</dct:title>
    <dct:creator rdf:resource="https://example.org/person/alice"/>
  </bibo:AcademicArticle>
</rdf:RDF>
');
```

### Loading from Server-Side Files

For large datasets, server-side file loading avoids transferring data through the SQL
protocol:

```sql
-- Load a large N-Triples dump (superuser required)
SELECT pg_ripple.load_ntriples_file('/data/exports/papers.nt');

-- Load Turtle with strict parsing (abort on any error)
SELECT pg_ripple.load_turtle_file('/data/exports/ontology.ttl', true);

-- Load into a specific named graph
SELECT pg_ripple.load_turtle_file_into_graph(
    '/data/exports/pubmed.ttl',
    'https://example.org/graph/pubmed'
);
```

```admonish warning
File loading functions read from the **PostgreSQL server's** filesystem, not the client's.
The path must be accessible to the `postgres` OS user. These functions require superuser
privileges for security reasons.
```

### Loading Turtle-Star (RDF-Star)

pg_ripple's Turtle parser supports RDF-star quoted triples natively:

```sql
SELECT pg_ripple.load_turtle('
@prefix ex:  <https://example.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

<< ex:paper/42 ex:cites ex:paper/99 >> ex:confidence "0.92"^^xsd:decimal .
<< ex:paper/42 ex:cites ex:paper/99 >> ex:source ex:system/citation-extractor .
');
```

### Loading into Named Graphs

Load data into a specific graph without the TriG/N-Quads format:

```sql
-- Create the graph first (optional — auto-created on load)
SELECT pg_ripple.create_graph('https://example.org/graph/2024');

-- Load Turtle into the named graph
SELECT pg_ripple.load_turtle_into_graph('
@prefix ex:  <https://example.org/> .
@prefix dct: <http://purl.org/dc/terms/> .
@prefix bibo: <http://purl.org/ontology/bibo/> .

ex:paper/300 a bibo:AcademicArticle ;
    dct:title "New Findings in Graph Theory" ;
    dct:creator ex:person/frank .
', 'https://example.org/graph/2024');
```

### Using SPARQL Update for Loading

SPARQL `INSERT DATA` is another way to add triples:

```sql
SELECT pg_ripple.sparql_update('
PREFIX ex:   <https://example.org/>
PREFIX dct:  <http://purl.org/dc/terms/>
PREFIX bibo: <http://purl.org/ontology/bibo/>

INSERT DATA {
    ex:paper/500 a bibo:AcademicArticle ;
        dct:title "SPARQL Performance Tuning" ;
        dct:creator ex:person/alice .
}
');
```

---

## Common Patterns

### Pattern: ETL Pipeline

A typical ETL pipeline loads data in stages:

```sql
-- Step 1: Load the ontology
SELECT pg_ripple.load_turtle_file('/data/ontology.ttl');

-- Step 2: Load reference data
SELECT pg_ripple.load_ntriples_file('/data/institutions.nt');

-- Step 3: Load the main dataset
SELECT pg_ripple.load_ntriples_file('/data/papers.nt');

-- Step 4: Load supplementary data into a named graph
SELECT pg_ripple.load_nquads_file('/data/citations.nq');

-- Step 5: Update statistics
SELECT pg_ripple.vacuum();

-- Step 6: Verify the load
SELECT pg_ripple.triple_count();
SELECT pg_ripple.stats();
```

### Pattern: Incremental Loading

For streaming data ingestion, use `insert_triple()` inside application code:

```sql
-- Application inserts triples as events arrive
SELECT pg_ripple.insert_triple(
    '<https://example.org/paper/new123>',
    '<http://purl.org/dc/terms/title>',
    '"Just Published: A New Study"'
);

-- Periodically compact HTAP tables
SELECT pg_ripple.compact();
```

### Pattern: Strict vs Lenient Parsing

```sql
-- Lenient (default): skip bad triples, emit WARNINGs
SELECT pg_ripple.load_turtle('
@prefix ex: <https://example.org/> .
ex:good ex:rel ex:target .
ex:bad ex:rel "unclosed literal .
ex:also_good ex:rel ex:other .
', false);
-- Returns: 2 (skipped the bad triple)

-- Strict: abort on any parse error
SELECT pg_ripple.load_turtle('
@prefix ex: <https://example.org/> .
ex:good ex:rel ex:target .
ex:bad ex:rel "unclosed literal .
', true);
-- ERROR: Turtle parse error at line 4
```

### Pattern: Loading OWL Ontologies

```sql
-- Auto-detects format from file extension (.ttl, .nt, .xml, .rdf, .owl)
SELECT pg_ripple.load_owl_ontology('/data/ontologies/foaf.rdf');

-- Or load explicitly as RDF/XML
SELECT pg_ripple.load_rdfxml_file('/data/ontologies/dublin_core.rdf');
```

---

## Performance and Trade-offs

### Throughput by Loading Mode

| Mode | Approximate throughput | Use case |
|---|---|---|
| `insert_triple()` | 3,000–8,000 triples/s | Real-time ingestion, single-triple updates |
| `load_turtle()` / `load_ntriples()` | 30,000–80,000 triples/s | Interactive bulk loads up to a few MB |
| `load_ntriples_file()` | 80,000–200,000 triples/s | Large server-side files |
| `load_turtle_file()` | 60,000–150,000 triples/s | Large server-side Turtle files |

```admonish note
N-Triples is consistently faster than Turtle because it requires no prefix expansion
or abbreviation handling. For maximum throughput on large datasets, convert to N-Triples
first: `rapper -i turtle -o ntriples data.ttl > data.nt`
```

### Format Selection Guide

| Scenario | Recommended format |
|---|---|
| Hand-authored data | Turtle (readable, supports prefixes) |
| Machine-generated export | N-Triples (fastest parsing, one line per triple) |
| Data with named graphs | N-Quads or TriG |
| Legacy XML datasets | RDF/XML |
| Maximum load speed | N-Triples via `load_ntriples_file()` |

### ANALYZE After Loads

After loading significant amounts of data, update PostgreSQL planner statistics:

```sql
-- Run ANALYZE on all VP tables
SELECT pg_ripple.vacuum();
```

This ensures the query planner has accurate row-count estimates for join ordering.

### Batch Size Considerations

For string-based loaders, the entire input is processed in one transaction. Very large
strings (hundreds of MB) can cause memory pressure. For datasets over 50 MB, prefer
file-based loading:

```sql
-- Instead of a huge string literal:
-- SELECT pg_ripple.load_ntriples('... 100 million lines ...');

-- Use file loading:
SELECT pg_ripple.load_ntriples_file('/data/huge_dataset.nt');
```

---

## Gotchas and Debugging

### Blank Node Scoping

Each `load_turtle()` call creates a fresh blank-node scope. Two separate calls using
`_:x` produce two different internal IDs:

```sql
-- Call 1: _:x maps to internal ID 12345
SELECT pg_ripple.load_turtle('
@prefix ex: <https://example.org/> .
_:x ex:name "Alice" .
ex:paper/1 ex:author _:x .
');

-- Call 2: _:x maps to internal ID 67890 (different!)
SELECT pg_ripple.load_turtle('
@prefix ex: <https://example.org/> .
_:x ex:name "Bob" .
ex:paper/2 ex:author _:x .
');
```

If you need the same anonymous node across loads, use a stable IRI instead:

```sql
SELECT pg_ripple.insert_triple(
    '<https://example.org/anon/shared-node>',
    '<https://example.org/name>',
    '"Shared Entity"'
);
```

### Character Encoding

All loaders expect UTF-8 input. Non-UTF-8 data causes parse errors:

```sql
-- If your file is Latin-1, convert first:
-- iconv -f ISO-8859-1 -t UTF-8 data.nt > data_utf8.nt
SELECT pg_ripple.load_ntriples_file('/data/data_utf8.nt');
```

### Verifying Loaded Data

After loading, verify with `find_triples()` or `triple_count()`:

```sql
-- Check total triples
SELECT pg_ripple.triple_count();

-- Inspect specific triples
SELECT * FROM pg_ripple.find_triples(
    '<https://example.org/paper/42>', NULL, NULL
);

-- Check per-predicate statistics
SELECT pg_ripple.stats();
```

### File Path Errors

File loaders read from the server filesystem. Common errors:

```sql
-- ERROR: could not open file "/data/papers.nt": No such file or directory
-- Fix: ensure the file exists and is readable by the postgres OS user

-- ERROR: permission denied for function load_turtle_file
-- Fix: file loaders require superuser; use string loaders for non-superusers
```

### Duplicate Handling

Loading the same data twice does not create duplicates — VP tables use `ON CONFLICT DO NOTHING`:

```sql
SELECT pg_ripple.load_turtle('
@prefix ex: <https://example.org/> .
ex:a ex:rel ex:b .
');
-- Returns: 1

SELECT pg_ripple.load_turtle('
@prefix ex: <https://example.org/> .
ex:a ex:rel ex:b .
');
-- Returns: 0 (already exists)
```

---

## Next Steps

- **[§2.1 Storing Knowledge](../features/storing-knowledge.md)** — understand the triple model and named graphs.
- **[§2.3 Querying with SPARQL](../features/querying-with-sparql.md)** — query the data you loaded.
- **[§2.6 Exporting and Sharing](../features/exporting-and-sharing.md)** — export data in various formats.
