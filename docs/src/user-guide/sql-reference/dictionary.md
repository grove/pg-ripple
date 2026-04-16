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

## decode_id_full (v0.15.0)

```sql
pg_ripple.decode_id_full(id BIGINT) RETURNS JSONB
```

Returns a structured JSONB object with detailed type information. More informative than `decode_id()` for debugging and introspection.

| Field | Description |
|-------|-------------|
| `kind` | Term type: `"iri"`, `"bnode"`, `"literal"`, `"default_graph"`, `"quoted_triple"` |
| `value` | The term value (IRI string, literal text, blank node label) |
| `datatype` | XSD datatype IRI (for typed literals) or `null` |
| `language` | Language tag (for language-tagged literals) or `null` |

```sql
SELECT pg_ripple.decode_id_full(42);
-- {"kind": "literal", "value": "Alice", "datatype": null, "language": "en"}

SELECT pg_ripple.decode_id_full(1);
-- {"kind": "iri", "value": "https://example.org/alice", "datatype": null, "language": null}
```

## lookup_iri (v0.15.0)

```sql
pg_ripple.lookup_iri(iri TEXT) RETURNS BIGINT
```

Checks whether an IRI exists in the dictionary and returns its ID. Returns `NULL` if the IRI has never been encoded. Unlike `encode_term()`, this never inserts — it is a read-only lookup.

```sql
SELECT pg_ripple.lookup_iri('<https://example.org/alice>');
-- Returns: 1 (or NULL if not in the dictionary)
```

Useful for checking whether a resource exists before querying:

```sql
DO $$
BEGIN
  IF pg_ripple.lookup_iri('<https://example.org/alice>') IS NOT NULL THEN
    RAISE NOTICE 'Alice exists in the store';
  END IF;
END $$;
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
