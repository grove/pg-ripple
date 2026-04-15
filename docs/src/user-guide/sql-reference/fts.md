# Full-Text Search

pg_ripple provides full-text search on RDF literal objects using PostgreSQL's built-in GIN `tsvector` indexes. This enables fast free-text queries on string-valued predicates without scanning the entire dictionary.

## fts_index

```sql
pg_ripple.fts_index(predicate TEXT) RETURNS BIGINT
```

Creates a GIN `tsvector` index on the `_pg_ripple.dictionary` table for string-literal objects. Returns the dictionary ID of the predicate.

The `predicate` argument accepts both raw IRI strings and N-Triples notation with angle brackets.

```sql
-- Create FTS index for the dc:description predicate
SELECT pg_ripple.fts_index('http://purl.org/dc/elements/1.1/description');

-- N-Triples notation also accepted
SELECT pg_ripple.fts_index('<http://purl.org/dc/elements/1.1/description>');
```

The index is created as `IF NOT EXISTS`, so calling `fts_index` multiple times for the same predicate is safe.

> **Index scope**: The GIN index covers all plain string literals (`kind = 2`) in the dictionary. The `fts_search` function restricts results by predicate via a VP table JOIN.

### Example: index all abstract predicates

```sql
-- Load some data
SELECT pg_ripple.load_ntriples('
    <https://example.org/paper1> <https://schema.org/abstract> "A study of RDF triplestores" .
    <https://example.org/paper2> <https://schema.org/abstract> "PostgreSQL extensions for graph data" .
');

-- Index the abstract predicate
SELECT pg_ripple.fts_index('<https://schema.org/abstract>');

-- Now search
SELECT * FROM pg_ripple.fts_search('RDF | triplestore', '<https://schema.org/abstract>');
```

---

## fts_search

```sql
pg_ripple.fts_search(query TEXT, predicate TEXT)
    RETURNS TABLE(s TEXT, p TEXT, o TEXT)
```

Executes a full-text search against literal objects of the specified predicate. Returns matching triples as N-Triples–formatted strings.

- `query` — a PostgreSQL `tsquery` expression (see below)
- `predicate` — the predicate IRI, with or without angle brackets

```sql
SELECT s, o
FROM pg_ripple.fts_search('semantic & query', '<https://schema.org/abstract>');
```

### tsquery syntax

PostgreSQL `tsquery` uses `&` (AND), `|` (OR), `!` (NOT), and `<->` (phrase proximity):

| Query | Matches |
|---|---|
| `'rdf'` | documents containing "rdf" |
| `'rdf & sparql'` | documents containing both |
| `'rdf | sparql'` | documents containing either |
| `'!relational'` | documents not containing "relational" |
| `'rdf <-> store'` | documents with "rdf" immediately followed by "store" |

All terms are automatically stemmed (English stemmer by default) — searching for `"querying"` also matches `"query"` and `"queries"`.

### Searching without a prior fts_index call

`fts_search` works even without a prior `fts_index` call by performing a sequential scan of the dictionary joined to the VP table. For large stores, call `fts_index` first for each predicate you search frequently.

### Return columns

| Column | Content |
|---|---|
| `s` | Subject IRI in N-Triples notation |
| `p` | Predicate IRI in N-Triples notation |
| `o` | Literal value in N-Triples notation |

---

## Language configuration

The default text configuration is `'english'`, which applies English stemming and stop-word removal. If your data is in another language you can create the index manually:

```sql
-- Example: French language index on a custom predicate
CREATE INDEX my_fr_fts ON _pg_ripple.dictionary
    USING GIN (to_tsvector('french', value))
    WHERE kind = 2;
```

The built-in `fts_index` and `fts_search` always use `'english'`. Multi-language support is planned for a future release.
