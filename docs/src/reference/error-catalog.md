# Error Message Catalog

pg_ripple uses structured error codes in the range **PT001–PT799**, organized by subsystem. Error messages follow PostgreSQL conventions: lowercase first word, no trailing period.

```admonish tip title="Finding the error code"
Error codes appear in the `DETAIL` field of PostgreSQL error messages. Use `\errverbose` in psql to see the full error context including the code.
```

---

## PT001–PT099: Dictionary

Errors from the IRI/literal/blank-node → integer encoding subsystem.

| Code | Message | Cause | Fix |
|---|---|---|---|
| PT001 | dictionary encode failed: hash collision detected | Two distinct terms produced the same XXH3-128 hash (extremely rare) | Report to maintainers with the two colliding terms |
| PT002 | dictionary decode failed: id not found | The integer ID does not exist in `_pg_ripple.dictionary` | Data may be corrupt; run `pg_ripple.vacuum_dictionary()` and check VP tables |
| PT003 | invalid term kind: expected 0 (IRI), 1 (literal), 2 (blank node) | Wrong `kind` integer passed to `encode_term()` | Use 0 for IRIs, 1 for literals, 2 for blank nodes |
| PT004 | quoted triple components not found | A quoted-triple ID references `qt_s`/`qt_p`/`qt_o` values that are missing from the dictionary | Re-load the RDF-star data; may indicate a partial load failure |
| PT005 | inline-encoded literal decode failed | Internal decoding error for small inline-encoded literals | Report to maintainers with the literal value |
| PT006 | dictionary batch insert failed | The `ON CONFLICT DO NOTHING … RETURNING` batch insert encountered an unexpected error | Check PostgreSQL logs for disk space or permission issues |
| PT007 | dictionary lookup: NULL term | A NULL value was passed where an IRI, literal, or blank node was expected | Ensure all arguments are non-NULL |
| PT008 | malformed IRI: `<detail>` | The IRI string does not conform to RFC 3987 | Fix the IRI syntax; IRIs must be wrapped in angle brackets `<…>` |
| PT009 | malformed literal: `<detail>` | The literal string cannot be parsed | Use N-Triples syntax: `"value"`, `"value"@lang`, or `"value"^^<datatype>` |
| PT010 | malformed blank node: `<detail>` | The blank node label is invalid | Blank nodes must start with `_:` followed by a valid label |
| PT011 | dictionary cache full, eviction failed | The LRU cache could not evict entries | Increase `pg_ripple.dictionary_cache_size` |
| PT012 | prewarm_dictionary_hot: table not found | The dictionary table does not exist | Run `CREATE EXTENSION pg_ripple` first |

---

## PT100–PT199: Storage

Errors from the VP table storage layer, HTAP partitions, and rare-predicate management.

| Code | Message | Cause | Fix |
|---|---|---|---|
| PT100 | insert_triple: predicate IRI required | The predicate argument is NULL or empty | Provide a valid predicate IRI |
| PT101 | VP table creation failed | DDL error when creating a new VP table | Check `pg_log` for the underlying PostgreSQL error |
| PT102 | htap_migrate_predicate: predicate not found | The predicate ID does not exist in `_pg_ripple.predicates` | Verify the predicate IRI and that triples exist for it |
| PT103 | merge: lock_timeout exceeded during main table swap | Another transaction held a lock on the VP table for too long | Retry; consider increasing `lock_timeout` for maintenance windows |
| PT104 | rare-predicate promotion failed | Error promoting a predicate from `vp_rare` to a dedicated VP table | Check disk space and user permissions |
| PT105 | delete_triple: predicate not found in catalog | The triple's predicate has no VP table | The predicate may never have been used, or was already compacted |
| PT106 | VP table not found: `<table_name>` | The VP table referenced in `_pg_ripple.predicates` does not exist on disk | Run `pg_ripple.compact()` to reconcile the catalog |
| PT107 | delta table insert failed | Error writing to the HTAP delta partition | Check PostgreSQL logs for tablespace or permission issues |
| PT108 | tombstone insert failed | Error recording a deletion in the tombstones table | Check PostgreSQL logs |
| PT109 | merge worker: unexpected state | The background merge worker encountered an inconsistent state | Restart PostgreSQL; check `pg_log` for crash details |
| PT110 | statement_id_seq: sequence exhausted | The global statement ID sequence has reached its maximum | This is unlikely with `BIGINT`; contact maintainers |
| PT111 | vp_rare: row limit exceeded | The rare-predicate table has too many rows for a single predicate | Manually promote with `promote_rare_predicates()` or lower `pg_ripple.vp_promotion_threshold` |
| PT112 | deduplicate: advisory lock not acquired | Another deduplication operation is already running | Wait and retry |
| PT113 | create_graph: invalid graph IRI | The graph IRI is malformed | Graph IRIs must be valid absolute IRIs in angle brackets |
| PT114 | drop_graph: graph not found | The named graph does not exist | Use `list_graphs()` to check available graphs |

---

## PT200–PT299: SPARQL

Errors from the SPARQL parser, algebra optimizer, and SQL code generator.

| Code | Message | Cause | Fix |
|---|---|---|---|
| PT200 | SPARQL parse error: `<detail>` | The SPARQL query has a syntax error | Fix the syntax; use `sparql_explain()` to validate without executing |
| PT201 | unsupported SPARQL algebra node: `<type>` | The query uses a feature not yet implemented | Check the [compliance matrix](sparql-compliance.md) for supported features |
| PT202 | SPARQL SELECT: no projected variables | The SELECT clause has no variables | Add at least one `?variable` to the SELECT clause |
| PT203 | property path depth exceeded `max_path_depth` | A recursive property path exceeded the configured depth limit | Increase `pg_ripple.max_path_depth` or simplify the path expression |
| PT204 | SPARQL federated SERVICE: endpoint not reachable | The remote SPARQL endpoint did not respond | Check the endpoint URL and network connectivity |
| PT205 | SPARQL VALUES clause: column count mismatch | The number of values in a VALUES row does not match the variable list | Ensure each VALUES row has the same number of columns as variables |
| PT206 | SPARQL type error: `<detail>` | A type mismatch in a FILTER expression | Check operand types; e.g., comparing a string to an integer |
| PT207 | SPARQL CONSTRUCT: template variable not in WHERE | A variable in the CONSTRUCT template is not bound in the WHERE clause | Bind all template variables in the WHERE clause |
| PT208 | SPARQL DESCRIBE: no resource specified | DESCRIBE requires at least one resource or variable | Add a resource IRI or variable to the DESCRIBE clause |
| PT209 | SPARQL aggregate: variable not grouped | A non-aggregated variable is used outside GROUP BY | Add the variable to GROUP BY or wrap it in an aggregate function |
| PT210 | SPARQL HAVING: refers to non-aggregate | The HAVING clause references a variable that is not an aggregate result | Use an aggregate function in HAVING |
| PT211 | generated SQL execution failed: `<detail>` | The SQL generated from SPARQL failed to execute | Check `pg_log` for the underlying error; report if reproducible |
| PT212 | plan cache: entry evicted during execution | A cached plan was evicted while the query was still running | Increase `pg_ripple.plan_cache_size` |
| PT213 | SPARQL SERVICE: response parse error | The federated endpoint returned malformed results | Check the remote endpoint's response format |
| PT214 | SPARQL SERVICE: timeout after `<N>` ms | The federated request exceeded `pg_ripple.federation_timeout` | Increase the timeout or simplify the SERVICE query |
| PT215 | SPARQL UPDATE parse error: `<detail>` | The SPARQL Update statement has a syntax error | Fix the syntax |
| PT216 | SPARQL UPDATE: LOAD failed for `<url>` | The LOAD operation could not retrieve the remote resource | Check URL, network, and `pg_ripple.federation_timeout` |
| PT217 | SPARQL UPDATE: unsupported content type `<type>` | The LOAD target serves an unrecognized RDF format | The URL must serve Turtle, N-Triples, N-Quads, TriG, or RDF/XML |
| PT218 | SPARQL UPDATE: CREATE GRAPH already exists | The graph already exists | Use `CREATE SILENT GRAPH` to suppress this error |
| PT219 | SPARQL UPDATE: DROP GRAPH not found | The graph does not exist | Use `DROP SILENT GRAPH` to suppress this error |

---

## PT300–PT399: SHACL

Errors from the SHACL shapes loader, validator, and async monitoring pipeline.

| Code | Message | Cause | Fix |
|---|---|---|---|
| PT300 | SHACL parse error: `<detail>` | The Turtle-encoded SHACL shapes have a syntax error | Fix the Turtle syntax in the shapes definition |
| PT301 | SHACL sync validation failed: `<shape>` — `<message>` | A triple violates a SHACL constraint during synchronous validation | Fix the data to conform to the shape, or modify the shape |
| PT302 | SHACL shape not found: `<iri>` | The referenced shape has not been loaded | Load the shape with `load_shacl()` first |
| PT303 | SHACL DAG monitor: pg_trickle not installed | DAG-aware monitors require the pg_trickle extension | Install pg_trickle or use `enable_shacl_monitors()` for trigger-based validation |
| PT304 | SHACL: unsupported constraint component `<type>` | The shape uses a SHACL-AF or SHACL-JS constraint | Only SHACL Core constraints are supported |
| PT305 | SHACL: sh:path too complex | The property path in the shape exceeds supported complexity | Simplify the `sh:path` expression |
| PT306 | SHACL: validation queue overflow | The async validation queue has exceeded its capacity | Process the queue with `process_validation_queue()` or increase the queue size |
| PT307 | SHACL: dead letter queue threshold reached | Too many validation failures have accumulated | Inspect with `dead_letter_queue()` and address the failures |
| PT308 | SHACL: sh:targetClass not found | The target class IRI is not present in the data | Load data with the target class, or fix the class IRI |
| PT309 | SHACL: circular shape reference | A shape references itself through `sh:node` or `sh:qualifiedValueShape` | Break the circular reference |
| PT310 | SHACL: drop_shape: shape has active monitors | Cannot drop a shape that has active monitors | Disable monitors first with `disable_shacl_dag_monitors()` |

---

## PT400–PT499: Datalog — Rules

Errors from the Datalog rule parser, stratifier, and rule management.

| Code | Message | Cause | Fix |
|---|---|---|---|
| PT400 | rule parse error: `<detail>` | The Datalog rule has a syntax error | Fix the rule syntax; see [Reasoning and Inference](../features/reasoning-and-inference.md) for syntax reference |
| PT401 | rule stratification failed: unstratifiable program | The rule set contains a cycle through negation that prevents stratification | Rewrite rules to break the negation cycle, or use `infer_wfs()` for well-founded semantics |
| PT402 | rule set not found: `<name>` | The referenced rule set has not been loaded | Load it with `load_rules()` or `load_rules_builtin()` |
| PT403 | inference: maximum iteration depth exceeded | Semi-naive evaluation did not converge within the iteration limit | Simplify the rule set or increase `statement_timeout` |
| PT404 | constraint violation detected: `<rule>` | A constraint rule (`:- body.`) fired | Check the data against the constraint body |
| PT405 | rule set already exists: `<name>` | A rule set with this name is already loaded | Drop it first with `drop_rules()`, or choose a different name |
| PT406 | rule: unsafe variable `<var>` | A variable appears in the head but not in a positive body literal | Ensure every head variable also appears in a positive body literal |
| PT407 | rule: built-in predicate not recognized: `<name>` | An unknown built-in predicate was used | Check available built-ins: `=`, `!=`, `<`, `>`, `<=`, `>=`, `+`, `-`, `*`, `/` |
| PT408 | rule: aggregation variable not in group-by | An aggregated variable is used outside the grouping context | Add the variable to the group-by list |

---

## PT500–PT599: Datalog — Inference Engine

Errors from the materialization engine, magic sets optimizer, WFS evaluator, and tabling.

| Code | Message | Cause | Fix |
|---|---|---|---|
| PT500 | infer: no enabled rule sets | `infer()` was called with no rule sets enabled | Enable at least one rule set with `enable_rule_set()` |
| PT501 | infer: SPI execution failed during iteration `<N>` | The SQL generated for a rule body failed | Check `pg_log` for the underlying error |
| PT502 | infer_demand: magic set rewriting failed | The demand transformation could not be applied | Simplify the goal pattern or rule set |
| PT503 | infer_demand: goal pattern too broad | The goal has no bound arguments, defeating the purpose of demand-driven evaluation | Bind at least one argument in the goal |
| PT504 | infer_wfs: unfounded set computation exceeded limit | The well-founded semantics fixpoint did not converge | Simplify the rule set or check for unusual negation patterns |
| PT505 | infer_wfs: three-valued model contains undefined atoms | Some atoms could not be classified as true or false | This is expected in WFS; query the `undefined` result set to see which atoms |
| PT506 | tabling: memo store overflow | The tabling memo store exceeded its size limit | Increase `pg_ripple.tabling_memo_size` |
| PT507 | infer_agg: aggregation cycle detected | An aggregation rule depends on its own aggregate result | Rewrite to break the cycle |
| PT508 | infer_goal: predicate not in any rule set | The goal predicate is not defined by any loaded rule | Load a rule set that defines the predicate |
| PT509 | owl:sameAs canonicalization: cycle limit exceeded | The `owl:sameAs` equivalence class merging exceeded the iteration limit | Check for very large `owl:sameAs` clusters |

---

## PT600–PT699: Export / HTTP

Errors from export serializers, GraphRAG export, and the HTTP companion service.

| Code | Message | Cause | Fix |
|---|---|---|---|
| PT600 | export: serialization failed for triple `<sid>` | A triple could not be serialized to the target format | Check that the triple's dictionary entries are intact |
| PT601 | export: unsupported format `<format>` | An unrecognized export format was requested | Use `ntriples`, `nquads`, `turtle`, or `jsonld` |
| PT602 | export_turtle_stream: batch_size must be > 0 | Invalid batch size | Use a positive integer |
| PT603 | export_jsonld: framing failed | The JSON-LD framing algorithm encountered an error | Check the frame structure; see [JSON-LD Framing](../features/exporting-and-sharing.md) |
| PT604 | export_graphrag_entities: no entities found | No entities match the GraphRAG export criteria | Load data or adjust the GraphRAG ontology |
| PT605 | jsonld_frame_to_sparql: invalid frame | The JSON-LD frame could not be converted to SPARQL | Check the frame JSON structure |
| PT606 | export: streaming interrupted | The streaming export was cancelled or the client disconnected | Retry the export |

---

## PT700–PT799: Configuration / Startup

Errors from extension initialization, GUC validation, and background workers.

| Code | Message | Cause | Fix |
|---|---|---|---|
| PT700 | _PG_init: cache_budget exceeds shared_memory_size | `pg_ripple.cache_budget` is larger than `pg_ripple.shared_memory_size` | Reduce `cache_budget` or increase `shared_memory_size` |
| PT701 | _PG_init: shmem initialization failed | Shared memory allocation failed | Increase system shared memory (`kern.sysv.shmmax` on macOS, `kernel.shmmax` on Linux) |
| PT702 | worker_database not set; merge worker defaulting to 'postgres' | The `pg_ripple.worker_database` GUC is not set | Set it to the correct database name in `postgresql.conf` |
| PT703 | merge worker watchdog: worker has been silent for `<N>` seconds | The background merge worker may have crashed | Check `pg_log` for crash details; restart PostgreSQL |
| PT704 | extension version mismatch: binary `<v1>`, control `<v2>` | The compiled extension version does not match `pg_ripple.control` | Rebuild and reinstall the extension |
| PT705 | GUC validation: `<param>` out of range | A GUC parameter was set to an invalid value | Check the [GUC Reference](guc-reference.md) for valid ranges |
| PT706 | shared_preload_libraries: pg_ripple not loaded | pg_ripple is not in `shared_preload_libraries` | Add `pg_ripple` to `shared_preload_libraries` in `postgresql.conf` and restart |
| PT707 | pg_trickle not installed | A feature requiring pg_trickle was called | Install pg_trickle or use the non-trickle alternative |
| PT708 | pgvector not installed | A vector/embedding function was called without pgvector | Install pgvector or disable with `pg_ripple.pgvector_enabled = off` |
| PT709 | enable_graph_rls: RLS policy creation failed | Row-level security policy could not be created | Check superuser privileges |
| PT710 | grant_graph: invalid permission | Permission must be `'read'`, `'write'`, or `'admin'` | Use one of the three valid permission strings |

```admonish warning title="Reporting bugs"
If you encounter an error code not listed here, or a message that says "contact maintainers", please open a GitHub issue with the full error output, your pg_ripple version (`SELECT pg_ripple.canary()`), and a minimal reproducer.
```
