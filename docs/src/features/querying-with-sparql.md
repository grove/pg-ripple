# §2.3 Querying with SPARQL

## What and Why

SPARQL is the W3C standard query language for RDF data — the SQL of the knowledge graph
world. pg_ripple translates SPARQL queries into optimized PostgreSQL SQL behind the
scenes, so you get the expressiveness of SPARQL with the performance of a mature
relational engine.

Why SPARQL instead of raw SQL against VP tables?

- **Graph pattern matching**: find paths, cycles, and subgraph shapes naturally.
- **Property paths**: traverse variable-length relationships with `+`, `*`, `?`.
- **Federation**: query remote SPARQL endpoints alongside local data.
- **Standards compliance**: queries are portable across triple stores.
- **Update support**: `INSERT DATA` and `DELETE DATA` for programmatic modifications.

pg_ripple supports all four SPARQL query forms (SELECT, CONSTRUCT, DESCRIBE, ASK) and
SPARQL Update (`INSERT DATA`, `DELETE DATA`, `DELETE/INSERT WHERE`).

---

## How It Works

### The SPARQL Pipeline

1. **Parse** — `spargebra` parses the SPARQL text into an algebra tree.
2. **Optimize** — `sparopt` applies algebraic optimizations (filter pushdown, join reordering).
3. **Translate** — pg_ripple's SQL generator converts the algebra to PostgreSQL SQL with integer-only VP table joins.
4. **Cache** — the plan cache stores translated SQL keyed by SPARQL text hash.
5. **Execute** — SPI executes the SQL; results are batch-decoded from integer IDs back to IRIs and literals.
6. **Return** — each result row is returned as a JSONB object.

### Key Functions

| Function | Purpose |
|---|---|
| `sparql(query)` | Execute SELECT or ASK; returns JSONB rows |
| `sparql_ask(query)` | Execute ASK; returns boolean |
| `sparql_construct(query)` | Execute CONSTRUCT; returns triple JSONB rows |
| `sparql_construct_turtle(query)` | CONSTRUCT → Turtle text |
| `sparql_construct_jsonld(query)` | CONSTRUCT → JSON-LD JSONB |
| `sparql_describe(query)` | DESCRIBE with CBD; returns triple JSONB rows |
| `sparql_describe_turtle(query)` | DESCRIBE → Turtle text |
| `sparql_update(query)` | INSERT DATA / DELETE DATA; returns affected count |
| `sparql_explain(query, analyze)` | Show generated SQL or EXPLAIN ANALYZE output |
| `explain_sparql(query, format)` | Extended explain with SQL, text, JSON, or algebra output |

---

## Worked Examples

All examples assume the bibliographic dataset from [§2.1](../features/storing-knowledge.md) and [§2.2](../features/loading-data.md) has been loaded.

### Basic Triple Patterns

Find all papers and their titles:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct:  <http://purl.org/dc/terms/>
PREFIX bibo: <http://purl.org/ontology/bibo/>

SELECT ?paper ?title
WHERE {
    ?paper a bibo:AcademicArticle .
    ?paper dct:title ?title .
}
');
```

Each row is a JSONB object like `{"paper": "<https://example.org/paper/42>", "title": "\"Knowledge Graphs in Practice\""}`.

### Filtering Results

Find papers published after 2023:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct:  <http://purl.org/dc/terms/>
PREFIX bibo: <http://purl.org/ontology/bibo/>
PREFIX xsd:  <http://www.w3.org/2001/XMLSchema#>

SELECT ?paper ?title ?date
WHERE {
    ?paper a bibo:AcademicArticle ;
           dct:title ?title ;
           dct:date ?date .
    FILTER (?date > "2023-01-01"^^xsd:date)
}
');
```

### OPTIONAL Patterns

Include authors even if they have no affiliation:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct:    <http://purl.org/dc/terms/>
PREFIX foaf:   <http://xmlns.com/foaf/0.1/>
PREFIX schema: <https://schema.org/>

SELECT ?paper ?authorName ?instName
WHERE {
    ?paper dct:creator ?author .
    ?author foaf:name ?authorName .
    OPTIONAL {
        ?author schema:affiliation ?inst .
        ?inst foaf:name ?instName .
    }
}
');
```

### UNION

Find entities that are either papers or people:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX bibo: <http://purl.org/ontology/bibo/>
PREFIX foaf: <http://xmlns.com/foaf/0.1/>
PREFIX dct:  <http://purl.org/dc/terms/>

SELECT ?entity ?label
WHERE {
    {
        ?entity a bibo:AcademicArticle .
        ?entity dct:title ?label .
    }
    UNION
    {
        ?entity a foaf:Person .
        ?entity foaf:name ?label .
    }
}
');
```

### MINUS

Find papers that have no citations:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX bibo: <http://purl.org/ontology/bibo/>
PREFIX dct:  <http://purl.org/dc/terms/>

SELECT ?paper ?title
WHERE {
    ?paper a bibo:AcademicArticle ;
           dct:title ?title .
    MINUS {
        ?paper bibo:citedBy ?other .
    }
}
');
```

### Aggregation

Count papers per institution:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct:    <http://purl.org/dc/terms/>
PREFIX schema: <https://schema.org/>
PREFIX foaf:   <http://xmlns.com/foaf/0.1/>

SELECT ?instName (COUNT(DISTINCT ?paper) AS ?paperCount)
WHERE {
    ?paper dct:creator ?author .
    ?author schema:affiliation ?inst .
    ?inst foaf:name ?instName .
}
GROUP BY ?instName
ORDER BY DESC(?paperCount)
');
```

### Subqueries

Find the most prolific author and all their papers:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct:  <http://purl.org/dc/terms/>
PREFIX foaf: <http://xmlns.com/foaf/0.1/>

SELECT ?authorName ?paper ?title
WHERE {
    {
        SELECT ?author (COUNT(?p) AS ?count)
        WHERE {
            ?p dct:creator ?author .
        }
        GROUP BY ?author
        ORDER BY DESC(?count)
        LIMIT 1
    }
    ?author foaf:name ?authorName .
    ?paper dct:creator ?author ;
           dct:title ?title .
}
');
```

### Property Paths

Property paths let you traverse variable-length relationships.

**Transitive closure (`+`)** — find all classes an entity belongs to through the subclass hierarchy:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
PREFIX rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#>

SELECT ?entity ?superClass
WHERE {
    ?entity rdf:type/rdfs:subClassOf+ ?superClass .
}
');
```

**Zero-or-more (`*`)** — include the starting node:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>

SELECT ?class ?ancestor
WHERE {
    ?class rdfs:subClassOf* ?ancestor .
}
');
```

**Optional step (`?`)** — zero or one hops:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX schema: <https://schema.org/>
PREFIX foaf:   <http://xmlns.com/foaf/0.1/>

SELECT ?person ?nameOrInst
WHERE {
    ?person schema:affiliation? ?target .
    ?target foaf:name ?nameOrInst .
}
');
```

**Sequence path (`/`)** — chain properties:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct:    <http://purl.org/dc/terms/>
PREFIX schema: <https://schema.org/>
PREFIX foaf:   <http://xmlns.com/foaf/0.1/>

SELECT ?paper ?instName
WHERE {
    ?paper dct:creator/schema:affiliation/foaf:name ?instName .
}
');
```

**Alternative path (`|`)** — match either property:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct:    <http://purl.org/dc/terms/>
PREFIX schema: <https://schema.org/>

SELECT ?entity ?label
WHERE {
    ?entity (dct:title | schema:name) ?label .
}
');
```

**Inverse path (`^`)** — traverse in reverse:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct: <http://purl.org/dc/terms/>

SELECT ?author ?paper
WHERE {
    ?author ^dct:creator ?paper .
}
');
```

### GRAPH Patterns

Query data in specific named graphs:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct: <http://purl.org/dc/terms/>

SELECT ?paper ?title ?graph
WHERE {
    GRAPH ?graph {
        ?paper dct:title ?title .
    }
}
');
```

Query a specific named graph:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct: <http://purl.org/dc/terms/>

SELECT ?paper ?title
WHERE {
    GRAPH <https://example.org/graph/pubmed> {
        ?paper dct:title ?title .
    }
}
');
```

### ASK Queries

Check if something exists:

```sql
SELECT pg_ripple.sparql_ask('
PREFIX dct:  <http://purl.org/dc/terms/>
PREFIX bibo: <http://purl.org/ontology/bibo/>

ASK {
    ?paper a bibo:AcademicArticle ;
           dct:title "Knowledge Graphs in Practice" .
}
');
-- Returns: true
```

### CONSTRUCT Queries

Build new triples from query results:

```sql
SELECT * FROM pg_ripple.sparql_construct('
PREFIX dct:    <http://purl.org/dc/terms/>
PREFIX schema: <https://schema.org/>
PREFIX foaf:   <http://xmlns.com/foaf/0.1/>
PREFIX ex:     <https://example.org/>

CONSTRUCT {
    ?author ex:worksOn ?paper .
    ?paper ex:authoredAt ?inst .
}
WHERE {
    ?paper dct:creator ?author .
    ?author schema:affiliation ?inst .
}
');
```

Get CONSTRUCT results as Turtle:

```sql
SELECT pg_ripple.sparql_construct_turtle('
PREFIX dct:    <http://purl.org/dc/terms/>
PREFIX foaf:   <http://xmlns.com/foaf/0.1/>
PREFIX ex:     <https://example.org/>

CONSTRUCT {
    ?author ex:wrote ?paper .
}
WHERE {
    ?paper dct:creator ?author .
}
');
```

Get CONSTRUCT results as JSON-LD:

```sql
SELECT pg_ripple.sparql_construct_jsonld('
PREFIX dct:  <http://purl.org/dc/terms/>
PREFIX foaf: <http://xmlns.com/foaf/0.1/>
PREFIX ex:   <https://example.org/>

CONSTRUCT {
    ?author ex:wrote ?paper .
}
WHERE {
    ?paper dct:creator ?author .
}
');
```

### DESCRIBE Queries

Get everything about an entity using Concise Bounded Description:

```sql
SELECT * FROM pg_ripple.sparql_describe('
DESCRIBE <https://example.org/paper/42>
');
```

Get the description as Turtle:

```sql
SELECT pg_ripple.sparql_describe_turtle('
DESCRIBE <https://example.org/person/alice>
');
```

Choose the describe strategy:

```sql
-- Symmetric CBD: include triples where the entity is the object too
SELECT * FROM pg_ripple.sparql_describe(
    'DESCRIBE <https://example.org/person/alice>',
    'scbd'
);
```

### SPARQL Update

Insert new triples:

```sql
SELECT pg_ripple.sparql_update('
PREFIX ex:  <https://example.org/>
PREFIX dct: <http://purl.org/dc/terms/>

INSERT DATA {
    ex:paper/600 a <http://purl.org/ontology/bibo/AcademicArticle> ;
        dct:title "Emerging Trends in Knowledge Graphs" ;
        dct:creator ex:person/alice .
}
');
-- Returns: 3
```

Delete specific triples:

```sql
SELECT pg_ripple.sparql_update('
PREFIX ex:  <https://example.org/>
PREFIX dct: <http://purl.org/dc/terms/>

DELETE DATA {
    ex:paper/600 dct:title "Emerging Trends in Knowledge Graphs" .
}
');
-- Returns: 1
```

### Query Debugging with EXPLAIN

View the generated SQL without executing:

```sql
SELECT pg_ripple.sparql_explain('
PREFIX dct:  <http://purl.org/dc/terms/>
PREFIX bibo: <http://purl.org/ontology/bibo/>

SELECT ?paper ?title
WHERE {
    ?paper a bibo:AcademicArticle ;
           dct:title ?title .
}
', false);
```

Run EXPLAIN ANALYZE to see execution times:

```sql
SELECT pg_ripple.sparql_explain('
PREFIX dct:  <http://purl.org/dc/terms/>
PREFIX bibo: <http://purl.org/ontology/bibo/>

SELECT ?paper ?title
WHERE {
    ?paper a bibo:AcademicArticle ;
           dct:title ?title .
}
', true);
```

Use the extended explain with format options:

```sql
-- Show just the generated SQL
SELECT pg_ripple.explain_sparql('
PREFIX dct: <http://purl.org/dc/terms/>
SELECT ?paper ?title
WHERE { ?paper dct:title ?title }
', 'sql');

-- Show EXPLAIN ANALYZE as JSON (for programmatic consumption)
SELECT pg_ripple.explain_sparql('
PREFIX dct: <http://purl.org/dc/terms/>
SELECT ?paper ?title
WHERE { ?paper dct:title ?title }
', 'json');

-- Show the spargebra algebra tree
SELECT pg_ripple.explain_sparql('
PREFIX dct: <http://purl.org/dc/terms/>
SELECT ?paper ?title
WHERE { ?paper dct:title ?title }
', 'sparql_algebra');
```

---

## Common Patterns

### Pattern: Star Queries (Multiple Predicates on the Same Subject)

The optimizer detects star patterns and collapses them into efficient multi-way joins:

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct:    <http://purl.org/dc/terms/>
PREFIX schema: <https://schema.org/>
PREFIX bibo:   <http://purl.org/ontology/bibo/>

SELECT ?paper ?title ?date
WHERE {
    ?paper a bibo:AcademicArticle ;
           dct:title ?title ;
           dct:date ?date ;
           schema:keywords ?kw .
    FILTER (CONTAINS(?kw, "knowledge"))
}
');
```

### Pattern: Existence Checks with FILTER EXISTS

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct:  <http://purl.org/dc/terms/>
PREFIX bibo: <http://purl.org/ontology/bibo/>

SELECT ?paper ?title
WHERE {
    ?paper a bibo:AcademicArticle ;
           dct:title ?title .
    FILTER EXISTS {
        ?paper bibo:citedBy ?other .
    }
}
');
```

### Pattern: VALUES Clause for Parameterized Queries

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct: <http://purl.org/dc/terms/>

SELECT ?paper ?title
WHERE {
    VALUES ?paper {
        <https://example.org/paper/42>
        <https://example.org/paper/99>
    }
    ?paper dct:title ?title .
}
');
```

### Pattern: BIND and Computed Values

```sql
SELECT * FROM pg_ripple.sparql('
PREFIX dct: <http://purl.org/dc/terms/>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

SELECT ?paper ?title ?yearLabel
WHERE {
    ?paper dct:title ?title ;
           dct:date ?date .
    BIND(YEAR(?date) AS ?year)
    BIND(CONCAT("Published in ", STR(?year)) AS ?yearLabel)
}
');
```

---

## Performance and Trade-offs

### Plan Cache

pg_ripple caches translated SQL by SPARQL query hash. Repeated queries skip the parse
and translate steps:

```sql
-- Check cache statistics
SELECT pg_ripple.plan_cache_stats();
-- Returns: {"hits": 42, "misses": 5, "size": 5, "capacity": 128, "hit_rate": 0.89}

-- Reset the cache (e.g., after schema changes)
SELECT pg_ripple.plan_cache_reset();
```

### Filter Pushdown

SPARQL FILTERs on bound constants are encoded to integers before SQL generation.
This means the database compares integers, not strings:

```sql
-- This FILTER is pushed down as an integer comparison:
SELECT * FROM pg_ripple.sparql('
PREFIX dct: <http://purl.org/dc/terms/>
SELECT ?paper WHERE {
    ?paper dct:creator <https://example.org/person/alice> .
}
');
```

### Property Path Depth Limit

Recursive property paths (`+`, `*`) compile to `WITH RECURSIVE ... CYCLE`. The GUC
`pg_ripple.max_path_depth` (default: 50) prevents runaway recursion:

```sql
-- Increase depth for deep hierarchies
SET pg_ripple.max_path_depth = 100;
```

```admonish warning
Setting `max_path_depth` very high on cyclic graphs can cause slow queries. pg_ripple
uses PostgreSQL 18's `CYCLE` clause for hash-based cycle detection, but wide graphs
still accumulate many intermediate rows.
```

### Full-Text Search Integration

Create a GIN index for fast text search on specific predicates:

```sql
-- Index the dct:title predicate for full-text search
SELECT pg_ripple.fts_index('<http://purl.org/dc/terms/title>');

-- Then CONTAINS() and REGEX() filters on dct:title objects use the GIN index
SELECT * FROM pg_ripple.sparql('
PREFIX dct: <http://purl.org/dc/terms/>
SELECT ?paper ?title
WHERE {
    ?paper dct:title ?title .
    FILTER (CONTAINS(?title, "Knowledge"))
}
');
```

Or use the direct full-text search function:

```sql
SELECT * FROM pg_ripple.fts_search(
    'knowledge & graph',
    '<http://purl.org/dc/terms/title>'
);
```

---

## Gotchas and Debugging

### SPARQL Syntax Errors

pg_ripple uses the `spargebra` parser, which gives precise error messages:

```sql
SELECT * FROM pg_ripple.sparql('
SELECT ?x WHERE { ?x ?p }
');
-- ERROR: SPARQL parse error: Expected '.' or '}' at line 2
```

Check the query compiles before running:

```sql
SELECT pg_ripple.explain_sparql('
PREFIX dct: <http://purl.org/dc/terms/>
SELECT ?paper WHERE { ?paper dct:title ?title }
', 'sql');
```

### No Results When Expected

Common causes:

1. **Missing angle brackets**: `dct:title` in SPARQL requires a PREFIX declaration. Without it, the parser treats it as a relative IRI.
2. **Wrong literal format**: `"42"` is a string, not a number. Use `"42"^^xsd:integer`.
3. **Case sensitivity**: IRIs are case-sensitive. `<https://Example.org/X>` and `<https://example.org/x>` are different.

Debug by checking what is stored:

```sql
-- Check if the predicate exists
SELECT * FROM pg_ripple.find_triples(
    NULL, '<http://purl.org/dc/terms/title>', NULL
);
```

### Slow Queries

1. Check the generated SQL with `sparql_explain()`.
2. Look for sequential scans on large VP tables — run `pg_ripple.vacuum()` to update statistics.
3. For property paths, check `max_path_depth` — lower it if the query is exploring too many paths.
4. Check the plan cache hit rate — a low hit rate means many unique queries are being parsed repeatedly.

```sql
-- Step 1: See the execution plan
SELECT pg_ripple.sparql_explain('
PREFIX dct: <http://purl.org/dc/terms/>
SELECT ?paper ?title
WHERE { ?paper dct:title ?title }
', true);

-- Step 2: Update statistics
SELECT pg_ripple.vacuum();

-- Step 3: Check plan cache
SELECT pg_ripple.plan_cache_stats();
```

### SPARQL Update Limitations

`sparql_update()` supports `INSERT DATA` and `DELETE DATA` (ground triples only).
Pattern-based `DELETE/INSERT WHERE` with variables is also supported for flexible
graph modifications. Use `delete_triple()` for programmatic single-triple deletion.

---

## Next Steps

- **[§2.4 Validating Data Quality](../features/validating-data-quality.md)** — enforce constraints on your data with SHACL.
- **[§2.5 Reasoning and Inference](../features/reasoning-and-inference.md)** — derive new facts with Datalog rules.
- **[§2.8 APIs and Integration](../features/apis-and-integration.md)** — access SPARQL from application code via the HTTP endpoint.
