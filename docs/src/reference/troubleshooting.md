# Troubleshooting

## SPARQL query returns 0 rows

**Symptom**: A SPARQL query returns no results even though you expect data to be there.

**Common causes**:

1. **Wrong IRI prefix** — IRIs are case-sensitive and must match exactly. `<https://example.org/Alice>` and `<https://example.org/alice>` are different resources.

   Debug: query the dictionary directly.
   ```sql
   SELECT id, value FROM _pg_ripple.dictionary
   WHERE value LIKE '%alice%';
   ```

2. **Unregistered IRI** — the IRI you used in the query was never loaded into the dictionary. Check `triple_count()` and `find_triples()` to confirm data is present.

3. **Case mismatch in literals** — `"Alice"` and `"alice"` are different literals.

4. **Wrong namespace** — verify the exact predicate IRI used at load time.

**General approach**: use `sparql_explain()` to see the generated SQL, then run it directly to understand what the planner is doing.

```sql
SELECT pg_ripple.sparql_explain(
    'SELECT ?name WHERE { ?p <https://example.org/name> ?name }',
    false
);
```

---

## Property path hangs or times out

**Symptom**: A query using `+` or `*` paths runs forever or is killed by `statement_timeout`.

**Cause**: An unbounded path query on a dense graph can generate millions of recursive CTE iterations before the depth guard fires.

**Fix**:

```sql
-- Cap recursion depth for the current session
SET pg_ripple.max_path_depth = 10;

-- Or set a statement timeout
SET statement_timeout = '5s';
```

**Also check** whether the graph contains a cycle. The `CYCLE` clause in PG18 prevents infinite loops, but very wide fan-out before a cycle is detected still generates many rows.

---

## Aggregate returns unexpected results

**Symptom**: `COUNT(?x)` returns a larger number than expected.

**Cause**: SPARQL aggregate functions count all solution bindings, including duplicates, unless `DISTINCT` is used.

```sparql
-- Counts all bindings, including duplicates
SELECT (COUNT(?x) AS ?n) WHERE { ?s <ex:p> ?x }

-- Counts distinct values only
SELECT (COUNT(DISTINCT ?x) AS ?n) WHERE { ?s <ex:p> ?x }
```

**Also**: if data was loaded multiple times (e.g. in tests), duplicate triples will inflate counts. pg_ripple VP tables do not enforce uniqueness constraints.

---

## load_ntriples returns fewer triples than expected

**Symptom**: `load_ntriples()` returns a count smaller than the number of lines in your file.

**Cause**: Lines with syntax errors are silently skipped by the parser. Blank lines and comment lines (starting with `#`) are also not counted.

**Debug**: check the PostgreSQL server log for parse warnings, or validate the file with an external tool such as Apache Jena `riot`:

```bash
riot --validate input.nt
```

---

## find_triples returns `f` for found_literal

**Symptom**: A triple with a literal object was inserted but `find_triples(..., '"Alice"', ...)` returns no results.

**Cause**: The literal might have been inserted with a language tag or type annotation that you are not including in the search term.

```sql
-- Insert with type
SELECT pg_ripple.insert_triple('<ex:p>', '<ex:name>', '"Alice"^^<xsd:string>');

-- Search must include the type
SELECT * FROM pg_ripple.find_triples(NULL, '<ex:name>', '"Alice"^^<http://www.w3.org/2001/XMLSchema#string>');
```

---

## extension "pg_ripple" has no update path

**Symptom**: `ALTER EXTENSION pg_ripple UPDATE` fails with:
```
ERROR: extension "pg_ripple" has no update path from version "X" to version "Y"
```

**Cause**: The migration script `sql/pg_ripple--X--Y.sql` is missing from the extension directory.

**Fix**: reinstall the extension from the target version's source tree:

```bash
cargo pgrx install --pg-config $(which pg_config)
```

Then retry `ALTER EXTENSION pg_ripple UPDATE`.
