# Prefix Registry

The prefix registry maps short prefixes (e.g. `ex`) to IRI expansions (e.g. `https://example.org/`). Registered prefixes are used by the export functions when serializing triples to Turtle or N-Triples.

> **SPARQL queries do not use the prefix registry.** SPARQL queries must use either full IRIs in angle brackets (`<https://…>`) or declare prefixes inline with `PREFIX ex: <https://…>`.

## register_prefix

```sql
pg_ripple.register_prefix(prefix TEXT, expansion TEXT) RETURNS VOID
```

Registers or replaces a prefix–expansion mapping. Idempotent.

```sql
SELECT pg_ripple.register_prefix('ex', 'https://example.org/');
SELECT pg_ripple.register_prefix('rdf', 'http://www.w3.org/1999/02/22-rdf-syntax-ns#');
SELECT pg_ripple.register_prefix('xsd', 'http://www.w3.org/2001/XMLSchema#');
```

## prefixes

```sql
pg_ripple.prefixes() RETURNS TABLE(prefix TEXT, expansion TEXT)
```

Returns all registered prefix–expansion mappings.

```sql
SELECT * FROM pg_ripple.prefixes();
```

## Example: prefixes in export

After registering `ex: <https://example.org/>`, export functions will use the short form:

```sql
SELECT pg_ripple.register_prefix('ex', 'https://example.org/');
SELECT pg_ripple.export_ntriples();
-- Output uses <https://example.org/…> fully qualified
-- Turtle output uses ex:… short form when available
```
