# Error Reference

pg_ripple uses a structured error code system with codes in the range PT001–PT799. Error messages follow PostgreSQL style (lowercase first word, no trailing period).

---

## PT001–PT099: Dictionary errors

| Code | Message | Resolution |
|---|---|---|
| PT001 | dictionary encode failed: hash collision detected | Hash collision in XXH3-128; contact maintainers |
| PT002 | dictionary decode failed: id not found | The integer ID is not in the dictionary; data may be corrupt |
| PT003 | invalid term kind: expected 0 (IRI), 1 (literal), 2 (blank node) | Pass the correct kind integer to `encode_term()` |
| PT004 | quoted triple components not found | The quoted triple ID references components that are not in the dictionary |
| PT005 | inline-encoded literal decode failed | Internal error in inline encoding; contact maintainers |

---

## PT100–PT199: Storage errors

| Code | Message | Resolution |
|---|---|---|
| PT100 | insert_triple: predicate IRI required | The predicate argument must not be NULL or empty |
| PT101 | VP table creation failed | Check `pg_log` for the underlying PostgreSQL DDL error |
| PT102 | htap_migrate_predicate: predicate not found | The predicate ID does not exist in `_pg_ripple.predicates` |
| PT103 | merge: lock_timeout exceeded during main table swap | Another transaction held a lock for too long; retry |
| PT104 | rare-predicate promotion failed | Check available disk space and user permissions |
| PT105 | delete_triple: predicate not found in catalog | The triple's predicate has no VP table; nothing to delete |

---

## PT200–PT299: SPARQL parser errors

| Code | Message | Resolution |
|---|---|---|
| PT200 | SPARQL parse error: `<detail>` | Fix the SPARQL syntax; use `sparql_explain()` to validate |
| PT201 | unsupported SPARQL algebra node: `<type>` | The query uses a SPARQL feature not yet supported |
| PT202 | SPARQL SELECT: no projected variables | Add at least one `?variable` to the SELECT clause |
| PT203 | property path depth exceeded `max_path_depth` | Increase `pg_ripple.max_path_depth` or simplify the path |
| PT204 | SPARQL federated SERVICE not supported in this version | Federation requires v0.16.0 |
| PT205 | SPARQL VALUES clause: column count mismatch | VALUES row length must match the variable list length |

---

## PT300–PT399: SPARQL update errors

| Code | Message | Resolution |
|---|---|---|
| PT300 | SPARQL update parse error: `<detail>` | Fix the SPARQL Update syntax |
| PT301 | LOAD: HTTP request failed: `<url>` | Check URL, network connectivity, and `pg_ripple.federation_timeout` |
| PT302 | LOAD: unsupported content type: `<type>` | The remote URL must serve Turtle, N-Triples, or RDF/XML |
| PT303 | CREATE GRAPH: graph already exists | Use `CREATE SILENT GRAPH` to suppress the error |
| PT304 | DROP GRAPH: graph not found | Use `DROP SILENT GRAPH` to suppress the error |

---

## PT400–PT499: SHACL errors

| Code | Message | Resolution |
|---|---|---|
| PT400 | SHACL parse error: `<detail>` | Fix the Turtle-formatted SHACL shapes input |
| PT401 | SHACL sync validation failed: `<shape>` — `<message>` | The triple violates a SHACL constraint; fix the data or the shape |
| PT402 | SHACL shape not found: `<iri>` | Load the shape with `load_shacl()` before referencing it |
| PT403 | SHACL DAG monitor: pg_trickle not installed | Install pg_trickle to use `enable_shacl_dag_monitors()` |

---

## PT500–PT599: Datalog errors

| Code | Message | Resolution |
|---|---|---|
| PT500 | rule parse error: `<detail>` | Fix the Datalog rule syntax |
| PT501 | rule stratification failed: unstratifiable program | The rule set contains a cycle through negation; rewrite the rules |
| PT502 | rule set not found: `<name>` | Load the rule set with `load_rules()` before enabling it |
| PT503 | inference: maximum iteration depth exceeded | Simplify the rule set or increase statement_timeout |
| PT504 | constraint violation detected: `<rule>` | A constraint rule fired; check the data or adjust `enforce_constraints` |

---

## PT600–PT699: Administrative errors

| Code | Message | Resolution |
|---|---|---|
| PT600 | vacuum_dictionary: advisory lock not acquired | Another `vacuum_dictionary()` is already running; wait and retry |
| PT601 | reindex: VP table not found | The predicates catalog references a table that no longer exists; run `compact()` and retry |
| PT602 | enable_graph_rls: RLS policy creation failed | Check superuser privileges and `_pg_ripple.graph_access` table existence |
| PT603 | grant_graph: invalid permission | Permission must be `'read'`, `'write'`, or `'admin'` |
| PT604 | enable_schema_summary: pg_trickle not installed | Install pg_trickle or use `schema_summary()` for a one-shot scan |
| PT640 | SPARQL result set exceeded sparql_max_rows limit | Raise `pg_ripple.sparql_max_rows` or set `pg_ripple.sparql_overflow_action = 'warn'` to truncate instead of error |
| PT641 | Datalog derived facts exceeded datalog_max_derived | Raise `pg_ripple.datalog_max_derived` or 0 (unlimited) |
| PT642 | Export rows exceeded export_max_rows | Raise `pg_ripple.export_max_rows` or 0 (unlimited) |

---

## PT700–PT799: Configuration and startup errors

| Code | Message | Resolution |
|---|---|---|
| PT700 | _PG_init: cache_budget exceeds shared_memory_size | Set `pg_ripple.cache_budget` ≤ `pg_ripple.shared_memory_size` |
| PT701 | _PG_init: shmem initialization failed | Check available shared memory (`kern.sysv.shmmax` on macOS) |
| PT702 | worker_database not set; merge worker defaulting to 'postgres' | Set `pg_ripple.worker_database` to the correct database name |
| PT703 | merge worker watchdog: worker has been silent for `<N>` seconds | Check `pg_log` for worker crash details; restart PostgreSQL |
