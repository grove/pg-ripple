# API Stability

This page defines the stability contract for the pg_ripple public API, effective from v0.20.0.

---

## Stability tiers

pg_ripple functions and tables are divided into three tiers:

| Tier | Schema prefix | Contract |
|---|---|---|
| **Stable** | `pg_ripple.*` | Will not change in any incompatible way within the 1.x release series. |
| **Internal** | `_pg_ripple.*` | Private implementation detail. May change at any minor version. Do not depend on it in application code. |
| **Experimental** | Marked `-- EXPERIMENTAL` in source | Subject to change without notice. |

---

## Stable public API (`pg_ripple.*`)

The following functions are **stable** as of v0.20.0 and will be maintained without incompatible change through v1.x:

### Triple store core

| Function | Signature | Notes |
|---|---|---|
| `insert_triple` | `(s text, p text, o text) → bigint` | Returns the statement ID (SID). |
| `delete_triple` | `(s text, p text, o text) → bigint` | Returns deleted row count. |
| `find_triples` | `(s text, p text, o text) → setof jsonb` | NULL = wildcard. |
| `triple_count` | `() → bigint` | Total explicit triples in default graph. |
| `load_ntriples` | `(data text) → bigint` | Bulk load from N-Triples string. |
| `load_turtle` | `(data text) → bigint` | Bulk load from Turtle string. |
| `load_rdf_xml` | `(data text) → bigint` | Bulk load from RDF/XML string. |

### SPARQL

| Function | Signature | Notes |
|---|---|---|
| `sparql` | `(query text) → setof jsonb` | SPARQL 1.1 SELECT. Returns one JSONB object per row. |
| `sparql_ask` | `(query text) → boolean` | SPARQL 1.1 ASK. |
| `sparql_construct` | `(query text) → setof jsonb` | SPARQL 1.1 CONSTRUCT. |
| `sparql_describe` | `(query text, strategy text) → setof jsonb` | SPARQL 1.1 DESCRIBE. |
| `sparql_update` | `(update text) → bigint` | SPARQL 1.1 Update. Returns affected triple count. |
| `sparql_explain` | `(query text, analyze boolean) → text` | Query plan / execution trace. |

### Dictionary

| Function | Signature | Notes |
|---|---|---|
| `encode_term` | `(value text, kind smallint) → bigint` | Encode a term to dictionary ID. |
| `decode_id` | `(id bigint) → text` | Decode a dictionary ID to its string form. |
| `encode_triple` | `(s text, p text, o text) → bigint` | Encode a quoted triple to dictionary ID. |
| `decode_triple` | `(id bigint) → jsonb` | Decode a quoted-triple ID to `{s, p, o}`. |
| `dictionary_stats` | `() → jsonb` | Statistics about the dictionary table. |
| `vacuum_dictionary` | `() → bigint` | Remove orphaned dictionary entries. |

### SHACL validation

| Function | Signature | Notes |
|---|---|---|
| `load_shacl` | `(shapes text) → bigint` | Parse and store a SHACL shapes graph. |
| `validate` | `() → jsonb` | Run offline validation. Returns `{conforms, violations}`. |
| `list_shapes` | `() → setof record` | Enumerate stored shapes. |
| `drop_shape` | `(shape_iri text) → bigint` | Remove a shape. |
| `process_validation_queue` | `() → bigint` | Process the async validation queue. |
| `dead_letter_queue` | `() → setof record` | Items that failed validation processing. |

### Datalog reasoning

| Function | Signature | Notes |
|---|---|---|
| `load_rules` | `(rules text, rule_set text DEFAULT 'custom') → bigint` | Parse and store a Datalog rule set. |
| `load_rules_builtin` | `(name text) → bigint` | Load a built-in rule set (`'rdfs'` or `'owl-rl'`). |
| `add_rule` | `(rule_set text, rule_text text) → bigint` | Add a single rule; returns new rule catalog ID. |
| `remove_rule` | `(rule_id bigint) → bigint` | Remove a rule by ID; returns triples retracted. |
| `drop_rules` | `(rule_set text) → bigint` | Drop all rules in a named rule set. |
| `infer` | `(rule_set text DEFAULT 'custom') → bigint` | Materialise inferences; returns triple count. |
| `infer_with_stats` | `(rule_set text DEFAULT 'custom') → jsonb` | Semi-naive inference with statistics. |
| `infer_goal` | `(rule_set text, goal text) → jsonb` | Goal-directed magic-sets inference. |
| `retract_inferred` | `(rule_set text) → bigint` | Delete all materialised triples for a rule set. |
| `list_rules` | `() → jsonb` | List all stored rules with metadata. |
| `list_rule_sets` | `() → table` | List all named rule sets with rule counts. |

### Administration

| Function | Signature | Notes |
|---|---|---|
| `vacuum` | `() → bigint` | Remove stale tombstones and orphaned delta rows. |
| `reindex` | `() → bigint` | Rebuild VP table indices. |
| `promote_rare_predicates` | `() → bigint` | Promote predicates from `vp_rare` to dedicated VP tables. |
| `trigger_merge` | `() → void` | Signal the background merge worker to run immediately. |
| `get_statement` | `(sid bigint) → jsonb` | Retrieve a triple by its statement ID. |

### Named graphs

| Function | Signature | Notes |
|---|---|---|
| `insert_triple_graph` | `(s text, p text, o text, g text) → bigint` | Insert into a named graph. |
| `delete_triple_graph` | `(s text, p text, o text, g text) → bigint` | Delete from a named graph. |
| `find_triples_graph` | `(s text, p text, o text, g text) → setof jsonb` | Query a named graph. |

### Graph RLS

| Function | Signature | Notes |
|---|---|---|
| `enable_graph_rls` | `() → void` | Enable graph-level Row-Level Security. |
| `grant_graph` | `(graph_iri text, role text) → void` | Grant RLS SELECT access to a graph for a role. |
| `revoke_graph` | `(graph_iri text, role text) → void` | Revoke RLS access for a role. |
| `grant_graph_access` | `(graph_iri text, role text, privilege text DEFAULT 'SELECT') → void` | Grant with explicit privilege level. |
| `revoke_graph_access` | `(graph_iri text, role text) → void` | Revoke access (detailed). |

### Full-text search

| Function | Signature | Notes |
|---|---|---|
| `fts_index` | `(predicate text) → bigint` | Add a predicate to the FTS index. |
| `fts_search` | `(query text) → setof jsonb` | Full-text search over indexed literals. |

### Export and serialisation

| Function | Signature | Notes |
|---|---|---|
| `export_ntriples` | `() → text` | Export all triples as N-Triples. |
| `export_turtle` | `() → text` | Export as Turtle. |
| `export_jsonld` | `() → jsonb` | Export as JSON-LD. |

### Plan cache

| Function | Signature | Notes |
|---|---|---|
| `plan_cache_stats` | `() → jsonb` | Hit/miss statistics for the SPARQL plan cache. |
| `plan_cache_reset` | `() → void` | Flush the plan cache. |

---

## Internal schema (`_pg_ripple.*`)

Tables, sequences, and functions in the `_pg_ripple` schema are **private implementation details**. They may be renamed, restructured, or removed at any minor version.

Do not write application code that queries `_pg_ripple.*` tables directly. Use the `pg_ripple.*` API instead.

Examples of internal objects (non-exhaustive):

- `_pg_ripple.dictionary` — the term encoding table
- `_pg_ripple.predicates` — the VP table catalog
- `_pg_ripple.vp_rare` — the rare-predicate consolidation table
- `_pg_ripple.vp_{id}_main`, `_pg_ripple.vp_{id}_delta`, `_pg_ripple.vp_{id}_tombstones` — HTAP partition tables
- `_pg_ripple.statement_id_seq` — the global SID sequence
- `_pg_ripple.shacl_shapes`, `_pg_ripple.validation_queue`, `_pg_ripple.dead_letter_queue` — SHACL internals

---

## GUC stability

The following GUCs are part of the stable public API:

| GUC | Default | Type |
|---|---|---|
| `pg_ripple.dictionary_cache_size` | `65536` | integer |
| `pg_ripple.vp_promotion_threshold` | `1000` | integer |
| `pg_ripple.merge_threshold` | `10000` | integer |
| `pg_ripple.shacl_mode` | `'async'` | enum: `'async'`, `'sync'`, `'off'` |
| `pg_ripple.enforce_constraints` | `'warn'` | enum: `'error'`, `'warn'`, `'off'` |
| `pg_ripple.rls_bypass` | `off` | boolean |
| `pg_ripple.federation_timeout_ms` | `5000` | integer |
| `pg_ripple.enable_plan_cache` | `on` | boolean |

Internal GUCs (names starting with `pg_ripple._`) may change without notice.

---

## Upgrade compatibility

pg_ripple provides a SQL migration script for every minor release (`sql/pg_ripple--X.Y.Z--X.Y.(Z+1).sql`). Upgrading is always possible via:

```sql
ALTER EXTENSION pg_ripple UPDATE;
```

No data migration is ever required for stable API changes. If a future release modifies the `_pg_ripple.*` schema, the migration script handles it automatically.
