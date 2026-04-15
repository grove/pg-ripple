# Dictionary

The dictionary maps every IRI, blank node, and literal to a `BIGINT` ID using XXH3-128 hashing. VP tables store only `BIGINT` values — no raw strings appear in data tables.

These functions are for advanced users who need to interact with the dictionary directly. Most applications should use the higher-level SPARQL and CRUD functions instead.

## encode_term

```sql
pg_ripple.encode_term(term TEXT) RETURNS BIGINT
```

Encodes an IRI, blank node, or literal (in N-Triples notation) and returns its dictionary ID. If the term is not yet in the dictionary it is inserted.

```sql
SELECT pg_ripple.encode_term('<https://example.org/alice>');
-- Returns the BIGINT ID

SELECT pg_ripple.encode_term('"Alice"@en');
-- Returns the BIGINT ID for the language-tagged literal
```

## decode_id

```sql
pg_ripple.decode_id(id BIGINT) RETURNS TEXT
```

Looks up a dictionary ID and returns the original term in N-Triples notation.

```sql
SELECT pg_ripple.decode_id(1);
-- Returns: '<https://example.org/alice>'
```

## Internal table: _pg_ripple.dictionary

```sql
TABLE _pg_ripple.dictionary (
    id    BIGINT PRIMARY KEY,
    kind  SMALLINT NOT NULL,  -- 1=IRI, 2=BNode, 3=Literal, 4=Default, 5=QuotedTriple
    value TEXT NOT NULL,
    hash_hi BIGINT,
    hash_lo BIGINT,
    qt_s BIGINT,  -- for kind=5: subject component
    qt_p BIGINT,  -- for kind=5: predicate component
    qt_o BIGINT   -- for kind=5: object component
)
```

The `kind` column encodes the term type:

| Kind | Meaning |
|---|---|
| 1 | IRI |
| 2 | Blank node |
| 3 | Literal |
| 4 | Default graph (ID 0) |
| 5 | Quoted triple (RDF-star) |
