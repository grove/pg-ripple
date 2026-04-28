[← Back to Blog Index](README.md)

# From Flat Triples to Nested JSON

## JSON-LD framing turns CONSTRUCT results into the JSON your API needs

---

RDF is a graph. APIs speak JSON. The gap between them is why most knowledge graph projects have a "JSON serialization layer" that someone hand-coded, nobody wants to maintain, and everyone curses when the schema changes.

JSON-LD framing bridges this gap with a declarative approach: describe the shape of the JSON you want, and let the framing algorithm extract it from the graph. pg_ripple implements this since v0.17.0, producing nested JSON-LD directly from CONSTRUCT queries.

---

## The Problem: Flat vs. Nested

A SPARQL CONSTRUCT query returns triples:

```sparql
CONSTRUCT {
  ?person foaf:name ?name .
  ?person foaf:knows ?friend .
  ?friend foaf:name ?friendName .
}
WHERE {
  ?person rdf:type foaf:Person .
  ?person foaf:name ?name .
  ?person foaf:knows ?friend .
  ?friend foaf:name ?friendName .
}
```

The result is a flat set of triples:

```
ex:alice foaf:name "Alice"
ex:alice foaf:knows ex:bob
ex:bob   foaf:name "Bob"
ex:alice foaf:knows ex:carol
ex:carol foaf:name "Carol"
```

What your React frontend wants is:

```json
{
  "@id": "ex:alice",
  "name": "Alice",
  "knows": [
    { "@id": "ex:bob", "name": "Bob" },
    { "@id": "ex:carol", "name": "Carol" }
  ]
}
```

Getting from flat triples to nested JSON requires understanding the graph structure: which triples are "top-level" entities, which are nested objects, and how deep the nesting goes. This is the tree extraction problem — pulling a tree shape out of a graph.

---

## JSON-LD Framing

A JSON-LD frame is a template that describes the desired JSON shape:

```json
{
  "@context": {
    "foaf": "http://xmlns.com/foaf/0.1/",
    "name": "foaf:name",
    "knows": "foaf:knows"
  },
  "@type": "foaf:Person",
  "knows": {
    "@embed": "@always"
  }
}
```

This frame says:
- Start with entities of type `foaf:Person`.
- For the `knows` property, embed the referenced objects inline (`@embed: @always`) instead of using a flat reference.

When applied to the CONSTRUCT result, the framing algorithm produces exactly the nested JSON above.

---

## Using Framing in pg_ripple

```sql
SELECT pg_ripple.construct_jsonld(
  query => '
    CONSTRUCT {
      ?person foaf:name ?name .
      ?person foaf:knows ?friend .
      ?friend foaf:name ?friendName .
    }
    WHERE {
      ?person rdf:type foaf:Person .
      ?person foaf:name ?name .
      ?person foaf:knows ?friend .
      ?friend foaf:name ?friendName .
    }
  ',
  frame => '{
    "@context": {
      "foaf": "http://xmlns.com/foaf/0.1/",
      "name": "foaf:name",
      "knows": "foaf:knows"
    },
    "@type": "foaf:Person",
    "knows": { "@embed": "@always" }
  }'
);
```

The result is a PostgreSQL `JSONB` value — a nested JSON object that you can return from a REST endpoint, store in a cache, or process with any JSON-aware tool.

---

## Frame Directives

JSON-LD framing supports several directives that control the output shape:

### @embed

Controls whether referenced objects are inlined or left as references.

- `@always`: Always embed the full object.
- `@once`: Embed the first occurrence, use `@id` references for subsequent.
- `@never`: Always use `@id` references, never embed.

```json
{
  "knows": { "@embed": "@once" }
}
```

With `@once`, if Alice knows Bob and Carol also knows Bob, Bob is embedded in Alice's object and referenced by `@id` in Carol's:

```json
[
  { "name": "Alice", "knows": [{ "@id": "ex:bob", "name": "Bob" }] },
  { "name": "Carol", "knows": [{ "@id": "ex:bob" }] }
]
```

### @reverse

Include reverse relationships — edges pointing *to* an entity rather than *from* it.

```json
{
  "@type": "foaf:Person",
  "@reverse": {
    "knows": { "@embed": "@always" }
  }
}
```

This produces:

```json
{
  "name": "Bob",
  "isKnownBy": [
    { "name": "Alice" },
    { "name": "Carol" }
  ]
}
```

### @explicit

When `true`, only properties mentioned in the frame are included in the output. Unlisted properties are omitted.

```json
{
  "@type": "foaf:Person",
  "name": {},
  "@explicit": true
}
```

This returns only the `name` property, even if the CONSTRUCT returned other properties.

---

## Practical Patterns

### API Response Shaping

A common pattern: one CONSTRUCT query, multiple frames for different API endpoints.

```sql
-- Detailed view (full profile with friends)
SELECT pg_ripple.construct_jsonld(
  query => :person_construct,
  frame => :detailed_frame
);

-- List view (name and ID only)
SELECT pg_ripple.construct_jsonld(
  query => :person_construct,
  frame => '{
    "@type": "foaf:Person",
    "name": {},
    "@explicit": true
  }'
);

-- Graph view (connections only)
SELECT pg_ripple.construct_jsonld(
  query => :person_construct,
  frame => '{
    "@type": "foaf:Person",
    "knows": { "@embed": "@never" },
    "@explicit": true
  }'
);
```

The same CONSTRUCT query, three different JSON shapes. The frame is the API contract; the query is the data source.

### Hierarchical Data

Framing handles nested hierarchies naturally:

```json
{
  "@type": "skos:Concept",
  "prefLabel": {},
  "narrower": {
    "@embed": "@always",
    "narrower": {
      "@embed": "@always"
    }
  }
}
```

This produces a tree of SKOS concepts, nested by the `narrower` relationship:

```json
{
  "prefLabel": "Science",
  "narrower": [
    {
      "prefLabel": "Physics",
      "narrower": [
        { "prefLabel": "Quantum Mechanics" },
        { "prefLabel": "Thermodynamics" }
      ]
    },
    {
      "prefLabel": "Chemistry",
      "narrower": [
        { "prefLabel": "Organic Chemistry" }
      ]
    }
  ]
}
```

### Materialized JSON-LD Views

Since v0.18.0, you can create CONSTRUCT views that automatically re-evaluate when the underlying data changes:

```sql
SELECT pg_ripple.create_construct_view(
  name => 'person_profiles',
  query => '
    CONSTRUCT {
      ?person foaf:name ?name .
      ?person foaf:knows ?friend .
      ?friend foaf:name ?friendName .
    }
    WHERE {
      ?person rdf:type foaf:Person .
      ?person foaf:name ?name .
      OPTIONAL {
        ?person foaf:knows ?friend .
        ?friend foaf:name ?friendName .
      }
    }
  '
);
```

The view materializes the CONSTRUCT results as a table. Combine this with framing for a JSON-LD endpoint that's always fresh:

```sql
-- Serve the API response
SELECT pg_ripple.frame_graph(
  graph => 'person_profiles',
  frame => :api_frame
)
WHERE person_id = 'ex:alice';
```

---

## Why Not Just Write JSON in the Application?

You could fetch SPARQL SELECT results and build JSON in Python, JavaScript, or whatever your API layer speaks. Many teams do this.

The problem is coupling. Your application code encodes assumptions about the graph structure: "a person always has exactly one name, knows zero or more friends, each friend has a name." When the ontology changes — a person can now have multiple names, or the `knows` relationship is renamed to `hasFriend` — the JSON serialization code needs to change.

With framing, the JSON shape is declared in the frame, not coded in the application. Change the ontology? Update the frame. The CONSTRUCT query adapts (or a new one is written), and the JSON output changes without touching application code.

This is the same benefit that XSLT provided for XML — a declarative transformation layer between the data model and the output format. JSON-LD framing is XSLT for the graph age, except it actually works.
