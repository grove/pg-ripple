# Triple CRUD

## insert_triple

```sql
pg_ripple.insert_triple(
    subject   TEXT,
    predicate TEXT,
    object    TEXT,
    graph     TEXT DEFAULT NULL
) RETURNS BIGINT
```

Inserts a single triple and returns its globally-unique **statement identifier** (SID). If `graph` is `NULL` the triple is inserted into the default graph (ID 0).

If the predicate has fewer than `pg_ripple.vp_promotion_threshold` (default: 1000) distinct triples it is stored in the shared `_pg_ripple.vp_rare` table; otherwise it gets its own VP table `_pg_ripple.vp_{predicate_id}`.

**Example:**

```sql
SELECT pg_ripple.insert_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
);
-- Returns: 1  (SID)
```

---

## delete_triple

```sql
pg_ripple.delete_triple(
    subject   TEXT,
    predicate TEXT,
    object    TEXT,
    graph     TEXT DEFAULT NULL
) RETURNS BIGINT
```

Deletes all triples matching the given (subject, predicate, object, graph) pattern where `NULL` is a wildcard. Returns the number of triples deleted.

**Examples:**

```sql
-- Delete a specific triple
SELECT pg_ripple.delete_triple(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
);

-- Delete all triples with a given subject
SELECT pg_ripple.delete_triple('<https://example.org/alice>', NULL, NULL);
```

---

## find_triples

```sql
pg_ripple.find_triples(
    subject   TEXT DEFAULT NULL,
    predicate TEXT DEFAULT NULL,
    object    TEXT DEFAULT NULL,
    graph     TEXT DEFAULT NULL
) RETURNS TABLE(subject TEXT, predicate TEXT, object TEXT, graph_id BIGINT)
```

Returns all triples matching the pattern. `NULL` is a wildcard for any position.

**Examples:**

```sql
-- Find by subject
SELECT * FROM pg_ripple.find_triples('<https://example.org/alice>', NULL, NULL);

-- Find by predicate
SELECT * FROM pg_ripple.find_triples(NULL, '<https://example.org/knows>', NULL);

-- Find exact triple
SELECT * FROM pg_ripple.find_triples(
    '<https://example.org/alice>',
    '<https://example.org/knows>',
    '<https://example.org/bob>'
);
```

---

## triple_count

```sql
pg_ripple.triple_count() RETURNS BIGINT
```

Returns the total number of triples across all graphs and all VP tables (both dedicated and `vp_rare`).

```sql
SELECT pg_ripple.triple_count();
-- Returns 0 for an empty store
```
