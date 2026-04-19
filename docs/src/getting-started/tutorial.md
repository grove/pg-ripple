# Guided Tutorial — Build a Knowledge Graph in 30 Minutes

This tutorial picks up where the [Hello World walkthrough](hello-world.md) ends. You will build a bibliographic knowledge graph with papers, authors, institutions, and citations — then validate it, reason over it, and export it as JSON-LD.

The tutorial is organized in four independent segments. Each takes under ten minutes and leaves you with a working, progressively richer knowledge graph. You can stop after any segment.

```admonish note
This tutorial uses an academic bibliographic dataset. The patterns — entity relationships, typed literals, named graphs, inference, validation — apply equally to product catalogs, supply chains, organizational hierarchies, or any domain with interconnected data.
```

## Prerequisites

pg_ripple is installed and you are connected to a PostgreSQL database with the extension created. See [Installation](installation.md).

---

## Segment 1: Load and Explore (10 min)

### Register prefixes

```sql
SELECT pg_ripple.register_prefix('bib', 'http://example.org/bib/');
SELECT pg_ripple.register_prefix('foaf', 'http://xmlns.com/foaf/0.1/');
SELECT pg_ripple.register_prefix('dc', 'http://purl.org/dc/elements/1.1/');
SELECT pg_ripple.register_prefix('dcterms', 'http://purl.org/dc/terms/');
SELECT pg_ripple.register_prefix('schema', 'http://schema.org/');
SELECT pg_ripple.register_prefix('skos', 'http://www.w3.org/2004/02/skos/core#');
```

### Load the bibliographic dataset

```sql
SELECT pg_ripple.load_turtle('
@prefix bib:     <http://example.org/bib/> .
@prefix foaf:    <http://xmlns.com/foaf/0.1/> .
@prefix dc:      <http://purl.org/dc/elements/1.1/> .
@prefix dcterms: <http://purl.org/dc/terms/> .
@prefix schema:  <http://schema.org/> .
@prefix rdf:     <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:     <http://www.w3.org/2001/XMLSchema#> .
@prefix skos:    <http://www.w3.org/2004/02/skos/core#> .

bib:mit       a schema:Organization ; schema:name "MIT" .
bib:stanford  a schema:Organization ; schema:name "Stanford University" .
bib:oxford    a schema:Organization ; schema:name "University of Oxford" .

bib:alice     a foaf:Person ; foaf:name "Alice Chen" ;
              schema:affiliation bib:mit .
bib:bob       a foaf:Person ; foaf:name "Bob Smith" ;
              schema:affiliation bib:stanford .
bib:carol     a foaf:Person ; foaf:name "Carol Martinez" ;
              schema:affiliation bib:oxford .

bib:paper1    a schema:ScholarlyArticle ;
              dc:title "Knowledge Graphs in Practice" ;
              dc:creator bib:alice ; dc:creator bib:bob ;
              dcterms:issued "2024-01-15"^^xsd:date ;
              schema:about <http://example.org/bib/kg> .

bib:paper2    a schema:ScholarlyArticle ;
              dc:title "Efficient SPARQL Query Processing" ;
              dc:creator bib:bob ; dc:creator bib:carol ;
              dcterms:issued "2024-03-22"^^xsd:date .

bib:paper3    a schema:ScholarlyArticle ;
              dc:title "Graph-Enhanced Retrieval for LLMs" ;
              dc:creator bib:alice ;
              dcterms:issued "2024-06-10"^^xsd:date .

bib:paper2    dcterms:references bib:paper1 .
bib:paper3    dcterms:references bib:paper1 .
bib:paper3    dcterms:references bib:paper2 .

bib:alice foaf:knows bib:bob .
bib:bob   foaf:knows bib:carol .
');
```

### Explore: find all papers by Alice

```sql
SELECT * FROM pg_ripple.sparql('
  PREFIX dc: <http://purl.org/dc/elements/1.1/>
  PREFIX bib: <http://example.org/bib/>
  PREFIX foaf: <http://xmlns.com/foaf/0.1/>
  SELECT ?title WHERE {
    ?paper dc:creator bib:alice .
    ?paper dc:title ?title .
  }
');
```

### Explore: citation chains

Find papers that cite papers Alice authored:

```sql
SELECT * FROM pg_ripple.sparql('
  PREFIX dc: <http://purl.org/dc/elements/1.1/>
  PREFIX dcterms: <http://purl.org/dc/terms/>
  PREFIX bib: <http://example.org/bib/>
  SELECT ?citingTitle ?citedTitle WHERE {
    ?citing dcterms:references ?cited .
    ?cited dc:creator bib:alice .
    ?citing dc:title ?citingTitle .
    ?cited dc:title ?citedTitle .
  }
');
```

### Explore: count papers per author

```sql
SELECT * FROM pg_ripple.sparql('
  PREFIX dc: <http://purl.org/dc/elements/1.1/>
  PREFIX foaf: <http://xmlns.com/foaf/0.1/>
  SELECT ?name (COUNT(?paper) AS ?papers) WHERE {
    ?paper dc:creator ?author .
    ?author foaf:name ?name .
  }
  GROUP BY ?name
  ORDER BY DESC(?papers)
');
```

---

## Segment 2: Validate (10 min)

SHACL (Shapes Constraint Language) lets you define data quality rules. You will create a shape that requires every `ScholarlyArticle` to have a title and at least one creator.

### Load a SHACL shape

```sql
SELECT pg_ripple.load_shacl('
@prefix sh:     <http://www.w3.org/ns/shacl#> .
@prefix schema: <http://schema.org/> .
@prefix dc:     <http://purl.org/dc/elements/1.1/> .
@prefix xsd:    <http://www.w3.org/2001/XMLSchema#> .

<http://example.org/shapes/ArticleShape>
  a sh:NodeShape ;
  sh:targetClass schema:ScholarlyArticle ;
  sh:property [
    sh:path dc:title ;
    sh:minCount 1 ;
    sh:maxCount 1 ;
    sh:datatype xsd:string ;
    sh:message "Every article must have exactly one title" ;
  ] ;
  sh:property [
    sh:path dc:creator ;
    sh:minCount 1 ;
    sh:message "Every article must have at least one creator" ;
  ] .
');
```

### Validate the dataset

```sql
SELECT pg_ripple.validate();
```

The result is a JSONB validation report. If all articles conform, the report shows zero violations. Now insert a bad article to see validation catch it:

```sql
SELECT pg_ripple.insert_triple(
  'http://example.org/bib/bad_paper',
  'http://www.w3.org/1999/02/22-rdf-syntax-ns#type',
  'http://schema.org/ScholarlyArticle'
);

SELECT pg_ripple.validate();
```

The report now shows a violation: the article has no title and no creator.

---

## Segment 3: Reason (10 min)

Datalog rules let you derive new facts. You will write a rule that infers transitive co-authorship: if Alice co-authored a paper with Bob, and Bob co-authored with Carol, then Alice and Carol are indirectly connected.

### Write and load a rule

```sql
SELECT pg_ripple.load_rules('
  coauthor(?a, ?b) :- <http://purl.org/dc/elements/1.1/creator>(?paper, ?a),
                      <http://purl.org/dc/elements/1.1/creator>(?paper, ?b),
                      ?a != ?b.
  connected(?a, ?b) :- coauthor(?a, ?b).
  connected(?a, ?b) :- connected(?a, ?c), coauthor(?c, ?b), ?a != ?b.
', 'coauthorship');
```

### Run inference

```sql
SELECT pg_ripple.infer('coauthorship');
```

This returns the number of new facts derived.

### Query the derived facts

```sql
SELECT * FROM pg_ripple.sparql('
  PREFIX bib: <http://example.org/bib/>
  PREFIX foaf: <http://xmlns.com/foaf/0.1/>
  SELECT ?name WHERE {
    bib:alice <http://example.org/bib/connected> ?person .
    ?person foaf:name ?name .
  }
');
```

Alice is now connected to Bob (direct co-author on paper1), Carol (through Bob on paper2), and potentially others through the transitive chain.

---

## Segment 4: Export (10 min)

Export your knowledge graph as JSON-LD, shaped for an API using a frame template.

### Export as Turtle

```sql
SELECT pg_ripple.export_turtle();
```

This returns all triples in human-readable Turtle format.

### Export as JSON-LD with framing

```sql
SELECT pg_ripple.sparql_construct_jsonld('
  PREFIX dc: <http://purl.org/dc/elements/1.1/>
  PREFIX foaf: <http://xmlns.com/foaf/0.1/>
  PREFIX schema: <http://schema.org/>
  CONSTRUCT {
    ?paper dc:title ?title .
    ?paper dc:creator ?author .
    ?author foaf:name ?name .
    ?author schema:affiliation ?org .
    ?org schema:name ?orgName .
  }
  WHERE {
    ?paper a schema:ScholarlyArticle .
    ?paper dc:title ?title .
    ?paper dc:creator ?author .
    ?author foaf:name ?name .
    OPTIONAL {
      ?author schema:affiliation ?org .
      ?org schema:name ?orgName .
    }
  }
');
```

The result is a nested JSON-LD document with papers, their authors, and institutional affiliations — ready to serve from a REST API.

---

## What you built

In 30 minutes, you created a knowledge graph with:

- **Structured data** — papers, authors, institutions, and citations as RDF triples
- **Quality rules** — SHACL shapes that catch incomplete articles
- **Derived knowledge** — Datalog rules that infer transitive co-authorship
- **API-ready export** — JSON-LD output shaped for downstream consumers

## Next steps

- [Storing Knowledge](../features/storing-knowledge.md) — data modeling deep dive
- [Querying with SPARQL](../features/querying-with-sparql.md) — the full query language
- [Validating Data Quality](../features/validating-data-quality.md) — advanced SHACL patterns
- [Reasoning and Inference](../features/reasoning-and-inference.md) — Datalog, RDFS, OWL RL
