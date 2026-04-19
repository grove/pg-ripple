# Hello World — Five-Minute Walkthrough

This walkthrough takes you from an empty database to working SPARQL queries in five minutes. You will load ten triples about people and movies, then run three queries of increasing complexity.

## Prerequisites

pg_ripple is installed and you are connected to a PostgreSQL database with the extension created. See [Installation](installation.md) if you have not done this yet.

## Step 1: Register prefixes

Prefixes are shortcuts for long IRIs. Register a few common ones:

```sql
SELECT pg_ripple.register_prefix('ex', 'http://example.org/');
SELECT pg_ripple.register_prefix('foaf', 'http://xmlns.com/foaf/0.1/');
SELECT pg_ripple.register_prefix('schema', 'http://schema.org/');
```

## Step 2: Load data

Load ten triples about people and the movies they directed or acted in:

```sql
SELECT pg_ripple.load_turtle('
  @prefix ex:     <http://example.org/> .
  @prefix foaf:   <http://xmlns.com/foaf/0.1/> .
  @prefix schema: <http://schema.org/> .

  ex:alice   foaf:name     "Alice" .
  ex:alice   schema:knows  ex:bob .
  ex:bob     foaf:name     "Bob" .
  ex:bob     schema:knows  ex:carol .
  ex:carol   foaf:name     "Carol" .
  ex:movie1  schema:name   "The Graph" .
  ex:movie1  schema:director ex:alice .
  ex:movie1  schema:actor    ex:bob .
  ex:movie2  schema:name   "Linked Data" .
  ex:movie2  schema:director ex:bob .
');
```

The function returns the number of triples loaded (10).

## Step 3: Query — basic pattern

Find all movies and their directors:

```sql
SELECT * FROM pg_ripple.sparql('
  PREFIX schema: <http://schema.org/>
  PREFIX foaf: <http://xmlns.com/foaf/0.1/>
  SELECT ?movie ?director WHERE {
    ?movie schema:director ?person .
    ?movie schema:name ?movie .
    ?person foaf:name ?director .
  }
');
```

Each row in the result is a JSONB object with the variable bindings. You should see "The Graph" directed by "Alice" and "Linked Data" directed by "Bob".

## Step 4: Query — OPTIONAL

Find all movies with their directors, and actors if they have any:

```sql
SELECT * FROM pg_ripple.sparql('
  PREFIX schema: <http://schema.org/>
  PREFIX foaf: <http://xmlns.com/foaf/0.1/>
  SELECT ?movieName ?directorName ?actorName WHERE {
    ?movie schema:name ?movieName .
    ?movie schema:director ?director .
    ?director foaf:name ?directorName .
    OPTIONAL {
      ?movie schema:actor ?actor .
      ?actor foaf:name ?actorName .
    }
  }
');
```

"The Graph" has an actor (Bob), while "Linked Data" does not — the `actorName` column is null for that row. The `OPTIONAL` keyword works like a SQL `LEFT JOIN`.

## Step 5: Query — property path

Find everyone Alice is connected to, directly or indirectly, through `schema:knows` links:

```sql
SELECT * FROM pg_ripple.sparql('
  PREFIX ex: <http://example.org/>
  PREFIX schema: <http://schema.org/>
  PREFIX foaf: <http://xmlns.com/foaf/0.1/>
  SELECT ?name WHERE {
    ex:alice schema:knows+ ?person .
    ?person foaf:name ?name .
  }
');
```

The `+` operator follows the `schema:knows` relationship one or more times. Alice knows Bob directly, and Bob knows Carol, so the query returns both "Bob" and "Carol".

## What you just learned

- **Triples** are facts with three parts: subject, predicate, object
- **Prefixes** are shortcuts for long IRIs
- **`load_turtle()`** loads data in Turtle format
- **`sparql()`** runs SPARQL queries and returns results as JSONB
- **OPTIONAL** is like a SQL `LEFT JOIN`
- **Property paths** (`+`, `*`) follow chains of relationships

## Next steps

- [Guided Tutorial](tutorial.md) — build a complete knowledge graph with validation and reasoning
- [Key Concepts](key-concepts.md) — understand RDF concepts using PostgreSQL analogies
- [Querying with SPARQL](../features/querying-with-sparql.md) — the full SPARQL feature set
