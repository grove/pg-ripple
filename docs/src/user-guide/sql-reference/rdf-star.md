# RDF-star

RDF-star (RDF\*) extends RDF by allowing triples to be used as subjects or objects in other triples. This enables statements *about* statements — useful for provenance, temporal annotations, and LPG-style edge properties.

## Overview

In RDF-star a **quoted triple** `<< s p o >>` can appear in subject or object position:

```
<< <ex:alice> <ex:knows> <ex:bob> >> <ex:assertedBy> <ex:carol> .
```

pg_ripple stores quoted triples in the dictionary with `kind = 5` (`KIND_QUOTED_TRIPLE`) and encodes them using their component dictionary IDs.

## encode_triple

```sql
pg_ripple.encode_triple(
    subject   TEXT,
    predicate TEXT,
    object    TEXT
) RETURNS BIGINT
```

Encodes a triple as a dictionary entry and returns its `BIGINT` ID. Idempotent — repeated calls with the same triple return the same ID.

```sql
SELECT pg_ripple.encode_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
);
```

## decode_triple

```sql
pg_ripple.decode_triple(id BIGINT) RETURNS JSONB
```

Returns the triple encoded at a given dictionary ID as a JSONB object `{"s":…,"p":…,"o":…}`.

```sql
SELECT pg_ripple.decode_triple(42);
-- Returns: {"s":"<https://example.org/alice>","p":"<https://example.org/knows>","o":"<https://example.org/bob>"}
```

## insert_triple and SIDs

`insert_triple()` returns a **statement identifier** (SID) — a globally-unique `BIGINT` for the inserted triple. SIDs can be used as subjects or objects in subsequent triples to annotate the statement.

```sql
DECLARE sid BIGINT;
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
) INTO sid;

-- Annotate the statement with provenance
SELECT pg_ripple.insert_triple(
    '<https://example.org/sid-' || sid || '>',
    '<https://example.org/assertedBy>',
    '<https://example.org/carol>'
);
```

## get_statement

```sql
pg_ripple.get_statement(i BIGINT) RETURNS JSONB
```

Looks up a triple by its SID and returns it as `{"s":…,"p":…,"o":…,"g":…}`.

```sql
SELECT pg_ripple.get_statement(1);
-- Returns: {"s":"<ex:alice>","p":"<ex:knows>","o":"<ex:bob>","g":"0"}
```

## Loading RDF-star data

`load_ntriples()` accepts N-Triples-star input with subject-position and object-position quoted triples:

```sql
SELECT pg_ripple.load_ntriples('
<< <https://example.org/alice> <https://example.org/knows> <https://example.org/bob> >>
    <https://example.org/assertedBy>
    <https://example.org/carol> .
');
```

## SPARQL-star patterns

Ground (all-constant) quoted triple patterns are supported in SPARQL WHERE clauses:

```sql
SELECT * FROM pg_ripple.sparql('
  SELECT ?who WHERE {
    << <https://example.org/alice> <https://example.org/knows> <https://example.org/bob> >>
        <https://example.org/assertedBy> ?who
  }
');
```

## LPG edge property mapping

RDF-star is a natural fit for encoding LPG edge properties: a quoted triple represents the edge, and subsequent triples about the quoted triple encode the properties.

```
<< <ex:alice> <ex:knows> <ex:bob> >> <ex:since>   "2023-01-01"^^xsd:date .
<< <ex:alice> <ex:knows> <ex:bob> >> <ex:strength> "strong" .
```


## Variable-inside-quoted-triple patterns (v0.48.0)

As of v0.48.0, variables inside quoted triple patterns are supported.
This allows binding variables to the components of a quoted triple that
appears as the subject or object of another triple:

```sql
-- Bind ?v to the object component of the matching quoted triple
SELECT * FROM pg_ripple.sparql('
  PREFIX ex: <http://example.org/>
  SELECT ?v ?who WHERE {
    << ex:alice ex:age ?v >> ex:assertedBy ?who .
  }
');

-- Bind all three components
SELECT * FROM pg_ripple.sparql('
  PREFIX ex: <http://example.org/>
  SELECT ?s ?p ?o ?who WHERE {
    << ?s ?p ?o >> ex:assertedBy ?who .
  }
');
```

This works by joining the `_pg_ripple.dictionary` table on the `qt_s`,
`qt_p`, and `qt_o` columns (available for entries with `kind = 5`).
