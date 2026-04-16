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
| `load_rdfxml` | [Bulk Loading](bulk-load.md) |
| `load_rdfxml_file` | [Bulk Loading](bulk-load.md) |
| `load_ntriples_into_graph` | [Bulk Loading](bulk-load.md) |
| `load_turtle_into_graph` | [Bulk Loading](bulk-load.md) |
| `load_rdfxml_into_graph` | [Bulk Loading](bulk-load.md) |
| `load_ntriples_file_into_graph` | [Bulk Loading](bulk-load.md) |
| `load_turtle_file_into_graph` | [Bulk Loading](bulk-load.md) |
| `load_rdfxml_file_into_graph` | [Bulk Loading](bulk-load.md) |
| `create_graph` | [Named Graphs](named-graphs.md) |
| `drop_graph` | [Named Graphs](named-graphs.md) |
| `list_graphs` | [Named Graphs](named-graphs.md) |
| `find_triples_in_graph` | [Named Graphs](named-graphs.md) |
| `triple_count_in_graph` | [Named Graphs](named-graphs.md) |
| `delete_triple_from_graph` | [Named Graphs](named-graphs.md) |
| `clear_graph` | [Named Graphs](named-graphs.md) |
| `sparql` | [SPARQL Queries](sparql-query.md) |
| `sparql_ask` | [SPARQL Queries](sparql-query.md) |
| `sparql_explain` | [SPARQL Queries](sparql-query.md) |
| `sparql_construct` | [SPARQL Queries](sparql-query.md) |
| `sparql_describe` | [SPARQL Queries](sparql-query.md) |
| `sparql_update` | [SPARQL Update](sparql-update.md) |
| `fts_index` | [Full-Text Search](fts.md) |
| `fts_search` | [Full-Text Search](fts.md) |
| `encode_triple` | [RDF-star](rdf-star.md) |
| `decode_triple` | [RDF-star](rdf-star.md) |
| `get_statement` | [RDF-star](rdf-star.md) |
| `encode_term` | [Dictionary](dictionary.md) |
| `decode_id` | [Dictionary](dictionary.md) |
| `decode_id_full` | [Dictionary](dictionary.md) |
| `lookup_iri` | [Dictionary](dictionary.md) |
| `register_prefix` | [Prefix Registry](prefix.md) |
| `prefixes` | [Prefix Registry](prefix.md) |
| `export_ntriples` | [Serialization & Export](serialization.md) |
| `export_nquads` | [Serialization & Export](serialization.md) |
| `export_turtle` | [Serialization & Export](serialization.md) |
| `export_turtle_stream` | [Serialization & Export](serialization.md) |
| `export_jsonld` | [Serialization & Export](serialization.md) |
| `export_jsonld_stream` | [Serialization & Export](serialization.md) |
| `sparql_construct_turtle` | [Serialization & Export](serialization.md) |
| `sparql_construct_jsonld` | [Serialization & Export](serialization.md) |
| `sparql_describe_turtle` | [Serialization & Export](serialization.md) |
| `sparql_describe_jsonld` | [Serialization & Export](serialization.md) |
| `load_shacl` | [SHACL Validation](shacl.md) |
| `validate` | [SHACL Validation](shacl.md) |
| `list_shapes` | [SHACL Validation](shacl.md) |
| `drop_shape` | [SHACL Validation](shacl.md) |
| `process_validation_queue` | [SHACL Validation](shacl.md) |
| `validation_queue_length` | [SHACL Validation](shacl.md) |
| `dead_letter_count` | [SHACL Validation](shacl.md) |
| `dead_letter_queue` | [SHACL Validation](shacl.md) |
| `drain_dead_letter_queue` | [SHACL Validation](shacl.md) |
| `enable_shacl_monitors` | [SHACL Validation](shacl.md) |
| `load_rules` | [Datalog Reasoning](datalog.md) |
| `load_rules_builtin` | [Datalog Reasoning](datalog.md) |
| `infer` | [Datalog Reasoning](datalog.md) |
| `check_constraints` | [Datalog Reasoning](datalog.md) |
| `list_rules` | [Datalog Reasoning](datalog.md) |
| `drop_rules` | [Datalog Reasoning](datalog.md) |
| `enable_rule_set` | [Datalog Reasoning](datalog.md) |
| `disable_rule_set` | [Datalog Reasoning](datalog.md) |
| `prewarm_dictionary_hot` | [Datalog Reasoning](datalog.md) |
| `create_sparql_view` | [Materialized Views](views.md) |
| `drop_sparql_view` | [Materialized Views](views.md) |
| `list_sparql_views` | [Materialized Views](views.md) |
| `create_datalog_view` | [Materialized Views](views.md) |
| `create_datalog_view_from_rule_set` | [Materialized Views](views.md) |
| `drop_datalog_view` | [Materialized Views](views.md) |
| `list_datalog_views` | [Materialized Views](views.md) |
| `create_extvp` | [Materialized Views](views.md) |
| `drop_extvp` | [Materialized Views](views.md) |
| `list_extvp` | [Materialized Views](views.md) |
| `pg_trickle_available` | [Materialized Views](views.md) |
| `compact` | [Administration](admin.md) |
| `stats` | [Administration](admin.md) |
| `subscribe` | [Administration](admin.md) |
| `unsubscribe` | [Administration](admin.md) |
| `htap_migrate_predicate` | [Administration](admin.md) |
| `subject_predicates` | [Administration](admin.md) |
| `object_predicates` | [Administration](admin.md) |
| `deduplicate_predicate` | [Administration](admin.md) |
| `deduplicate_all` | [Administration](admin.md) |
| `vacuum` | [Administration](admin.md) |
| `reindex` | [Administration](admin.md) |
| `vacuum_dictionary` | [Administration](admin.md) |
| `dictionary_stats` | [Administration](admin.md) |
| `enable_graph_rls` | [Administration](admin.md) |
| `grant_graph` | [Administration](admin.md) |
| `revoke_graph` | [Administration](admin.md) |
| `list_graph_access` | [Administration](admin.md) |
| `schema_summary` | [Administration](admin.md) |
| `enable_schema_summary` | [Administration](admin.md) |
| `plan_cache_stats` | [Administration](admin.md) |
| `plan_cache_reset` | [Administration](admin.md) |
