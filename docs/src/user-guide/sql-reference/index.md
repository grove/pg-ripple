# SQL Reference

This section documents all SQL functions and views exported by pg_ripple.

## Conventions

**Term format** — all IRI, blank node, and literal arguments use N-Triples notation:

| Kind | Format | Example |
|---|---|---|
| IRI | `<…>` | `<https://example.org/alice>` |
| Blank node | `_:…` | `_:b0` |
| Plain literal | `"…"` | `"Alice"` |
| Language-tagged literal | `"…"@lang` | `"Alice"@en` |
| Typed literal | `"…"^^<type>` | `"30"^^<http://www.w3.org/2001/XMLSchema#integer>` |

**NULL wildcards** — in find/delete functions, `NULL` matches any value for that position.

**Schema** — all public functions are in the `pg_ripple` schema. Internal tables are in `_pg_ripple`.

## Function index

| Function | Section |
|---|---|
| `insert_triple` | [Triple CRUD](triple-crud.md) |
| `delete_triple` | [Triple CRUD](triple-crud.md) |
| `find_triples` | [Triple CRUD](triple-crud.md) |
| `triple_count` | [Triple CRUD](triple-crud.md) |
| `load_ntriples` | [Bulk Loading](bulk-load.md) |
| `load_ntriples_file` | [Bulk Loading](bulk-load.md) |
| `load_turtle` | [Bulk Loading](bulk-load.md) |
| `load_turtle_file` | [Bulk Loading](bulk-load.md) |
| `load_nquads` | [Bulk Loading](bulk-load.md) |
| `load_nquads_file` | [Bulk Loading](bulk-load.md) |
| `load_trig` | [Bulk Loading](bulk-load.md) |
| `load_trig_file` | [Bulk Loading](bulk-load.md) |
| `create_graph` | [Named Graphs](named-graphs.md) |
| `drop_graph` | [Named Graphs](named-graphs.md) |
| `list_graphs` | [Named Graphs](named-graphs.md) |
| `sparql` | [SPARQL Queries](sparql-query.md) |
| `sparql_ask` | [SPARQL Queries](sparql-query.md) |
| `sparql_explain` | [SPARQL Queries](sparql-query.md) |
| `encode_triple` | [RDF-star](rdf-star.md) |
| `decode_triple` | [RDF-star](rdf-star.md) |
| `get_statement` | [RDF-star](rdf-star.md) |
| `encode_term` | [Dictionary](dictionary.md) |
| `decode_id` | [Dictionary](dictionary.md) |
| `register_prefix` | [Prefix Registry](prefix.md) |
| `prefixes` | [Prefix Registry](prefix.md) |
