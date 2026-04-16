# Changelog

All notable changes to pg_ripple are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions correspond to the milestones in [ROADMAP.md](ROADMAP.md).

---

## [Unreleased]

---

## [0.11.0] — 2026-04-16 — SPARQL & Datalog Views

This release adds always-fresh, incrementally-maintained stream tables for SPARQL and Datalog queries, plus Extended Vertical Partitioning (ExtVP) semi-join tables for multi-predicate star-pattern acceleration. All three features are built on top of [pg_trickle](https://github.com/grove/pg-trickle) and are soft-gated — pg_ripple loads and operates normally without pg_trickle; the new functions detect its absence at call time and return a clear error with an install hint.

**New in this release:** Compile any SPARQL SELECT query into a pg_trickle stream table with `create_sparql_view()`. Bundle a Datalog rule set with a goal pattern into a self-refreshing view with `create_datalog_view()`. Pre-compute semi-joins between frequently co-joined predicate pairs with `create_extvp()` to give 2–10× star-pattern speedups.

### What you can do

- **SPARQL views** — `pg_ripple.create_sparql_view(name, sparql, schedule, decode)` compiles a SPARQL SELECT query to SQL and registers it as a pg_trickle stream table; the table stays incrementally up-to-date on every triple insert/update/delete
- **Datalog views** — `pg_ripple.create_datalog_view(name, rules, goal, schedule, decode)` bundles inline Datalog rules with a goal query into a self-refreshing table; `create_datalog_view_from_rule_set(name, rule_set, goal, schedule, decode)` references a previously-loaded named rule set
- **ExtVP semi-joins** — `pg_ripple.create_extvp(name, pred1_iri, pred2_iri, schedule)` pre-computes the semi-join between two predicate tables; the SPARQL query engine detects and uses ExtVP tables automatically
- **Detect pg_trickle** — `pg_ripple.pg_trickle_available()` returns `true` if pg_trickle is installed, so callers can gate feature usage without catching errors
- **Lifecycle management** — `drop_sparql_view`, `drop_datalog_view`, `drop_extvp` remove both the stream table and the catalog entry; `list_sparql_views()`, `list_datalog_views()`, `list_extvp()` return JSONB arrays of registered objects

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.pg_trickle_available()` | `BOOLEAN` | Returns `true` if pg_trickle is installed |
| `pg_ripple.create_sparql_view(name, sparql, schedule DEFAULT '1s', decode DEFAULT false)` | `BIGINT` | Compile SPARQL SELECT to a pg_trickle stream table; returns column count |
| `pg_ripple.drop_sparql_view(name)` | `BOOLEAN` | Drop the stream table and catalog entry |
| `pg_ripple.list_sparql_views()` | `JSONB` | List all registered SPARQL views |
| `pg_ripple.create_datalog_view(name, rules, goal, rule_set_name DEFAULT 'custom', schedule DEFAULT '10s', decode DEFAULT false)` | `BIGINT` | Compile inline Datalog rules + goal into a stream table |
| `pg_ripple.create_datalog_view_from_rule_set(name, rule_set, goal, schedule DEFAULT '10s', decode DEFAULT false)` | `BIGINT` | Reference an existing named rule set for a Datalog view |
| `pg_ripple.drop_datalog_view(name)` | `BOOLEAN` | Drop the stream table and catalog entry |
| `pg_ripple.list_datalog_views()` | `JSONB` | List all registered Datalog views |
| `pg_ripple.create_extvp(name, pred1_iri, pred2_iri, schedule DEFAULT '10s')` | `BIGINT` | Pre-compute a semi-join stream table for two predicates |
| `pg_ripple.drop_extvp(name)` | `BOOLEAN` | Drop the ExtVP stream table and catalog entry |
| `pg_ripple.list_extvp()` | `JSONB` | List all registered ExtVP tables |

### New catalog tables

| Table | Description |
|-------|-------------|
| `_pg_ripple.sparql_views` | Stores SPARQL view name, original query, generated SQL, schedule, decode mode, stream table name, and variables |
| `_pg_ripple.datalog_views` | Stores Datalog view name, rules, rule set, goal, generated SQL, schedule, decode mode, stream table name, and variables |
| `_pg_ripple.extvp_tables` | Stores ExtVP name, predicate IRIs, predicate IDs, generated SQL, schedule, and stream table name |

<details>
<summary>Technical details</summary>

- **src/views.rs** — new module implementing all v0.11.0 public functions; `compile_sparql_for_view()` wraps `sparql::sqlgen::translate_select()` and renames internal `_v_{var}` columns to plain `{var}` for stream table compatibility; `create_extvp()` generates a parameterized semi-join SQL template over the two predicate VP tables
- **src/lib.rs** — three new catalog tables created at extension load time; eleven new `#[pg_extern]` functions exposed in the `pg_ripple` schema
- **src/datalog/mod.rs** — added `load_and_store_rules(rules_text, rule_set_name) -> i64` helper for Datalog view creation
- **src/sparql/mod.rs** — `sqlgen` module made `pub(crate)` so `views.rs` can call `translate_select()` directly
- **sql/pg_ripple--0.10.0--0.11.0.sql** — migration script adding the three catalog tables for upgrades from v0.10.0
- **pg_regress** — new test suites: `sparql_views.sql`, `datalog_views.sql`, `extvp.sql`; all pass

</details>

---

## [0.10.0] — 2026-04-16 — Datalog Reasoning Engine

This release delivers a full Datalog reasoning engine over the VP triple store. Rules are parsed from a Turtle-flavoured syntax, stratified for evaluation order, and compiled to native PostgreSQL SQL — no external reasoner process needed.

**New in this release:** pg_ripple can now execute RDFS and OWL RL entailment, user-defined inference rules, Datalog constraints, and arithmetic/string built-ins. Inference results are written back into the VP store with `source = 1` so explicit and derived triples are always distinguishable. A hot dictionary tier accelerates frequent IRI lookups, and a SHACL-AF bridge detects `sh:rule` properties in shape graphs and registers them alongside standard Datalog rules.

### What you can do

- **Write custom inference rules** — `pg_ripple.load_rules(rules, rule_set)` parses Turtle-flavoured Datalog and stores the compiled SQL strata
- **Built-in RDFS entailment** — `pg_ripple.load_rules_builtin('rdfs')` loads all 13 RDFS entailment rules; call `pg_ripple.infer('rdfs')` to materialize closure
- **Built-in OWL RL reasoning** — `pg_ripple.load_rules_builtin('owl-rl')` loads ~20 core OWL RL rules covering class hierarchy, property chains, and inverse/symmetric/transitive properties
- **Run inference on demand** — `pg_ripple.infer(rule_set)` runs all strata in order and inserts derived triples with `source = 1`; safe to call repeatedly (idempotent)
- **Declare integrity constraints** — rules with an empty head become constraints; `pg_ripple.check_constraints()` returns all violations as JSONB
- **Inspect and manage rule sets** — `pg_ripple.list_rules()` returns rules as JSONB; `pg_ripple.drop_rules(rule_set)` clears a named set; `enable_rule_set` / `disable_rule_set` toggle a set without deleting it
- **Accelerate hot IRIs** — `pg_ripple.prewarm_dictionary_hot()` loads frequently-used IRIs (≤ 512 B) into an UNLOGGED hot table for sub-microsecond lookups; survives connection pooling but not database restart
- **SHACL-AF bridge** — shapes that contain `sh:rule` entries are detected by `load_shacl()` and registered in the rules catalog; full SHACL-AF rule execution is planned for v0.11.0

### New GUC parameters

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.inference_mode` | `'on_demand'` | `'off'` disables engine; `'on_demand'` evaluates via CTEs; `'materialized'` uses pg_trickle stream tables |
| `pg_ripple.enforce_constraints` | `'warn'` | `'off'` silences violations; `'warn'` logs them; `'error'` raises an exception |
| `pg_ripple.rule_graph_scope` | `'default'` | `'default'` applies rules to default graph only; `'all'` applies across all named graphs |

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.load_rules(rules TEXT, rule_set TEXT DEFAULT 'custom')` | `BIGINT` | Parse, stratify, and store a Datalog rule set; returns the number of rules loaded |
| `pg_ripple.load_rules_builtin(name TEXT)` | `BIGINT` | Load a built-in rule set by name (`'rdfs'` or `'owl-rl'`) |
| `pg_ripple.list_rules()` | `JSONB` | Return all active rules as a JSONB array |
| `pg_ripple.drop_rules(rule_set TEXT)` | `BIGINT` | Delete a named rule set; returns the number of rules deleted |
| `pg_ripple.enable_rule_set(name TEXT)` | `VOID` | Mark a rule set as active |
| `pg_ripple.disable_rule_set(name TEXT)` | `VOID` | Mark a rule set as inactive |
| `pg_ripple.infer(rule_set TEXT DEFAULT 'custom')` | `BIGINT` | Run inference; returns the number of derived triples inserted |
| `pg_ripple.check_constraints(rule_set TEXT DEFAULT NULL)` | `JSONB` | Evaluate integrity constraints; returns violations |
| `pg_ripple.prewarm_dictionary_hot()` | `BIGINT` | Load hot IRIs into UNLOGGED hot table; returns rows loaded |

<details>
<summary>Technical details</summary>

- **src/datalog/mod.rs** — public API and IR type definitions (`Term`, `Atom`, `BodyLiteral`, `Rule`, `RuleSet`); catalog helpers for `_pg_ripple.rules` and `_pg_ripple.rule_sets`
- **src/datalog/parser.rs** — tokenizer and recursive-descent parser for Turtle-flavoured Datalog; variables as `?x`, full IRIs as `<...>`, prefixed IRIs as `prefix:local`, head `:-` body `.` delimiter
- **src/datalog/stratify.rs** — SCC-based stratification via Kosaraju's algorithm; unstratifiable programs (negation cycles) are rejected with a clear error message naming the cyclic predicates
- **src/datalog/compiler.rs** — compiles Rule IR to PostgreSQL SQL; non-recursive strata use `INSERT … SELECT … ON CONFLICT DO NOTHING`; recursive strata use `WITH RECURSIVE … CYCLE` (PG18 native cycle detection); negation compiles to `NOT EXISTS`; arithmetic/string built-ins compile to inline SQL expressions
- **src/datalog/builtins.rs** — RDFS (13 rules: rdfs2–rdfs12, subclass, domain, range) and OWL RL (~20 rules: class hierarchy, property chains, inverse/symmetric/transitive) as embedded Rust string constants
- **src/dictionary/hot.rs** — UNLOGGED hot table `_pg_ripple.dictionary_hot` for IRIs ≤ 512 B; `prewarm_hot_table()` runs at `_PG_init` when `inference_mode != 'off'`; `lookup_hot()` and `add_to_hot()` provide O(1) in-process hash lookups
- **src/shacl/mod.rs** — `parse_and_store_shapes()` now calls `bridge_shacl_rules()` when `inference_mode != 'off'`; the bridge detects `sh:rule` and registers a placeholder in `_pg_ripple.rules`
- **VP store** — `source SMALLINT NOT NULL DEFAULT 0` column present in all VP tables; migration script adds it retroactively to tables created before v0.10.0; `source = 0` means explicit, `source = 1` means derived
- **Migration script** — `sql/pg_ripple--0.9.0--0.10.0.sql` includes all `CREATE TABLE IF NOT EXISTS` and `ALTER TABLE … ADD COLUMN IF NOT EXISTS` statements for zero-downtime upgrades
- New pg_regress tests: `datalog_custom.sql`, `datalog_rdfs.sql`, `datalog_owl_rl.sql`, `datalog_negation.sql`, `datalog_arithmetic.sql`, `datalog_constraints.sql`, `datalog_malformed.sql`, `shacl_af_rule.sql`, `rdf_star_datalog.sql`

</details>

---

## [0.9.0] — 2026-04-15 — Serialization, Export & Interop

This release completes RDF I/O: pg_ripple can now import from and export to all major RDF serialization formats, and SPARQL CONSTRUCT and DESCRIBE queries can return results directly as Turtle or JSON-LD.

**New in this release:** Until now, you could load Turtle and N-Triples but exports were limited to N-Triples and N-Quads. You can now export as Turtle or JSON-LD — formats that are friendlier for human reading and REST APIs respectively. RDF/XML import covers the format that Protégé and most OWL editors produce. Streaming export variants handle large graphs without buffering the full document in memory.

### What you can do

- **Load RDF/XML** — `pg_ripple.load_rdfxml(data TEXT)` parses conformant RDF/XML (Protégé, OWL, most ontology editors); returns the number of triples loaded
- **Export as Turtle** — `pg_ripple.export_turtle()` serializes the default graph (or any named graph) as a compact Turtle document with `@prefix` declarations; RDF-star quoted triples use Turtle-star notation
- **Export as JSON-LD** — `pg_ripple.export_jsonld()` serializes triples as a JSON-LD expanded-form array, ready for REST APIs and Linked Data Platform contexts
- **Stream large graphs** — `pg_ripple.export_turtle_stream()` and `pg_ripple.export_jsonld_stream()` return one line at a time as `SETOF TEXT`, suitable for `COPY … TO STDOUT` pipelines
- **Get CONSTRUCT results as Turtle** — `pg_ripple.sparql_construct_turtle(query)` runs a SPARQL CONSTRUCT query and returns a Turtle document instead of JSONB rows
- **Get CONSTRUCT results as JSON-LD** — `pg_ripple.sparql_construct_jsonld(query)` returns JSONB in JSON-LD expanded form
- **Get DESCRIBE results as Turtle or JSON-LD** — `pg_ripple.sparql_describe_turtle(query)` and `pg_ripple.sparql_describe_jsonld(query)` offer the same format choice for DESCRIBE

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.load_rdfxml(data TEXT)` | `BIGINT` | Parse RDF/XML, load into default graph |
| `pg_ripple.export_turtle(graph TEXT DEFAULT NULL)` | `TEXT` | Export graph as Turtle |
| `pg_ripple.export_jsonld(graph TEXT DEFAULT NULL)` | `JSONB` | Export graph as JSON-LD (expanded form) |
| `pg_ripple.export_turtle_stream(graph TEXT DEFAULT NULL)` | `SETOF TEXT` | Streaming Turtle export |
| `pg_ripple.export_jsonld_stream(graph TEXT DEFAULT NULL)` | `SETOF TEXT` | Streaming JSON-LD NDJSON export |
| `pg_ripple.sparql_construct_turtle(query TEXT)` | `TEXT` | CONSTRUCT result as Turtle |
| `pg_ripple.sparql_construct_jsonld(query TEXT)` | `JSONB` | CONSTRUCT result as JSON-LD |
| `pg_ripple.sparql_describe_turtle(query TEXT, strategy TEXT DEFAULT 'cbd')` | `TEXT` | DESCRIBE result as Turtle |
| `pg_ripple.sparql_describe_jsonld(query TEXT, strategy TEXT DEFAULT 'cbd')` | `JSONB` | DESCRIBE result as JSON-LD |

<details>
<summary>Technical details</summary>

- `rio_xml` crate added as a dependency for RDF/XML parsing (uses rio_api `TriplesParser` interface, consistent with existing rio_turtle parsers)
- `src/export.rs` extended with `export_turtle`, `export_jsonld`, `export_turtle_stream`, `export_jsonld_stream`, `triples_to_turtle`, and `triples_to_jsonld`
- Turtle serialization groups by subject using `BTreeMap` for deterministic output; emits predicate-object lists per subject
- JSON-LD expanded form: each subject is one array entry; predicates become IRI-keyed arrays of `{"@value": …}` / `{"@id": …}` objects
- RDF-star quoted triples: passed through in Turtle-star `<< s p o >>` notation; in JSON-LD emitted as `{"@value": "…", "@type": "rdf:Statement"}`
- Streaming variants avoid buffering the full document; `export_turtle_stream` yields prefix lines then one `s p o .` per row
- SPARQL format functions (`sparql_construct_turtle`, etc.) delegate to the existing SPARQL engine then pass rows through the new serialization layer
- New pg_regress tests: `serialization.sql`, `rdf_star_construct.sql`, expanded `sparql_construct.sql`

</details>

---

## [0.8.0] — 2026-04-15 — Advanced Data Quality Rules

This release rounds out the data quality system with more expressive rules and a background validation mode that never slows down your inserts.

**New in this release:** Until now, each validation rule applied to a single property in isolation. You can now combine rules — "this value must satisfy rule A *or* rule B", "must satisfy *all* of these rules", "must *not* match this rule" — and count how many values on a property actually conform to a sub-rule. A background mode queues violations for later review instead of blocking every write.

### What you can do

- **Combine rules with logic** — use `sh:or`, `sh:and`, and `sh:not` to build validation rules that express complex conditions, such as "a contact must have either a phone number or an email address"
- **Reference another rule from within a rule** — `sh:node <ShapeIRI>` checks that each value on a property also satisfies a separate named rule; rules can reference each other up to 32 levels deep without getting stuck in a loop
- **Count qualifying values** — `sh:qualifiedValueShape` combined with `sh:qualifiedMinCount` / `sh:qualifiedMaxCount` counts only the values that actually pass a sub-rule, so you can say "at least two authors must be affiliated with a university"
- **Validate without blocking writes** — set `pg_ripple.shacl_mode = 'async'` so that inserts complete immediately and violations are collected silently in the background; the background worker drains the queue automatically
- **Inspect collected violations** — `pg_ripple.dead_letter_queue()` returns all async violations as a JSON array; `pg_ripple.drain_dead_letter_queue()` clears the queue once you have reviewed them
- **Drain the queue manually** — `pg_ripple.process_validation_queue(batch_size)` processes violations on demand, useful in test pipelines or batch jobs

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.process_validation_queue(batch_size BIGINT DEFAULT 1000)` | `BIGINT` | Process up to N pending validation jobs |
| `pg_ripple.validation_queue_length()` | `BIGINT` | How many jobs are waiting in the queue |
| `pg_ripple.dead_letter_count()` | `BIGINT` | How many violations have been recorded |
| `pg_ripple.dead_letter_queue()` | `JSONB` | All recorded violations as a JSON array |
| `pg_ripple.drain_dead_letter_queue()` | `BIGINT` | Delete all recorded violations and return how many were removed |

<details>
<summary>Technical details</summary>

- `ShapeConstraint` enum extended with `Or(Vec<String>)`, `And(Vec<String>)`, `Not(String)`, `QualifiedValueShape { shape_iri, min_count, max_count }`
- `validate_property_shape()` refactored to accept `all_shapes: &[Shape]` for recursive nested shape evaluation
- `node_conforms_to_shape()` added: depth-limited recursive conformance check (max depth 32)
- `process_validation_batch(batch_size)` added: SPI-based batch drain of `_pg_ripple.validation_queue`, writes violations to `_pg_ripple.dead_letter_queue`
- Merge worker (`src/worker.rs`) extended with `run_validation_cycle()` called after each merge transaction
- `validate_sync()` now handles `Class`, `Node`, `Or`, `And`, `Not`, and `QualifiedValueShape` (max-count check only for sync)
- `run_validate()` now checks top-level node `Or`/`And`/`Not` constraints in offline validation

</details>

---

## [0.7.0] — 2026-04-15 — Data Quality Rules (Core)

This release adds SHACL — a W3C standard for expressing data quality rules — and on-demand deduplication for datasets that have accumulated duplicate entries.

**What this means in practice:** You define rules like "every Person must have a name, and the name must be a string", load them into the database once, and pg_ripple will check those rules on every insert or on demand. Violations are reported as structured JSON so they can be logged, monitored, or acted on automatically.

### What you can do

- **Define data quality rules** — `pg_ripple.load_shacl(data TEXT)` parses rules written in W3C SHACL Turtle format and stores them in the database; returns the number of rules loaded
- **Check your data** — `pg_ripple.validate(graph TEXT DEFAULT NULL)` runs all active rules against your data and returns a JSON report: `{"conforms": true/false, "violations": [...]}`. Pass a graph name to validate only that graph
- **Reject bad data on insert** — set `pg_ripple.shacl_mode = 'sync'` to have `insert_triple()` immediately reject any triple that violates a `sh:maxCount`, `sh:datatype`, `sh:in`, or `sh:pattern` rule
- **Manage rules** — `pg_ripple.list_shapes()` lists all loaded rules; `pg_ripple.drop_shape(uri TEXT)` removes one rule by its IRI
- **Remove duplicate triples** — `pg_ripple.deduplicate_predicate(p_iri TEXT)` removes duplicate entries for one property, keeping the earliest record; `pg_ripple.deduplicate_all()` deduplicates everything
- **Deduplicate automatically on merge** — set `pg_ripple.dedup_on_merge = true` to eliminate duplicates each time the background worker compacts data (see v0.6.0)

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.load_shacl(data TEXT)` | `INTEGER` | Parse Turtle, store rules, return count loaded |
| `pg_ripple.validate(graph TEXT DEFAULT NULL)` | `JSONB` | Full validation report |
| `pg_ripple.list_shapes()` | `TABLE(shape_iri TEXT, active BOOLEAN)` | All rules in the catalog |
| `pg_ripple.drop_shape(shape_uri TEXT)` | `INTEGER` | Remove a rule by IRI |
| `pg_ripple.deduplicate_predicate(p_iri TEXT)` | `BIGINT` | Remove duplicates for one property |
| `pg_ripple.deduplicate_all()` | `BIGINT` | Remove duplicates across all properties |
| `pg_ripple.enable_shacl_monitors()` | `BOOLEAN` | Create a live violation-count stream table (requires pg_trickle) |

### New configuration options

| Option | Default | Description |
|--------|---------|-------------|
| `pg_ripple.shacl_mode` | `'off'` | When to validate: `'off'`, `'sync'` (block bad inserts), `'async'` (queue for later — see v0.8.0) |
| `pg_ripple.dedup_on_merge` | `false` | Eliminate duplicate triples during each background merge |

### New internal tables

| Table | Description |
|-------|-------------|
| `_pg_ripple.shacl_shapes` | Stores each loaded rule with its IRI, parsed JSON, and active flag |
| `_pg_ripple.validation_queue` | Inbox for inserts when `shacl_mode = 'async'` |
| `_pg_ripple.dead_letter_queue` | Recorded violations with full JSONB violation reports |
| `_pg_ripple.violation_summary` | Live violation counts by rule and severity (created by `enable_shacl_monitors()`) |

### Supported validation constraints (v0.7.0)

`sh:minCount`, `sh:maxCount`, `sh:datatype`, `sh:in`, `sh:pattern`, `sh:class`, `sh:targetClass`, `sh:targetNode`, `sh:targetSubjectsOf`, `sh:targetObjectsOf`. Logical combinators (`sh:or`, `sh:and`, `sh:not`) and qualified constraints are added in v0.8.0.

### Upgrading from v0.6.0

```sql
ALTER EXTENSION pg_ripple UPDATE;
```

The migration creates three new tables (`shacl_shapes`, `validation_queue`, `dead_letter_queue`) and their indexes. No existing tables are modified.

---

## [0.6.0] — 2026-04-15 — High-Speed Reads and Writes at the Same Time

This release separates write traffic from read traffic so both can run at full speed simultaneously. It also adds change notifications so other systems can react to new triples in real time.

**The problem this solves:** In earlier versions, heavy read queries could slow down writes and vice versa. Now, writes go into a small fast table and reads see everything via a transparent view. A background worker periodically merges the write table into an optimised read table without interrupting either operation.

### What you can do

- **Write and read simultaneously without blocking** — inserts land in a fast write buffer; reads see both the buffer and the main read-optimised store through a transparent view
- **Trigger a manual merge** — `pg_ripple.compact()` immediately merges all pending writes into the read store; returns the total number of triples after compaction
- **Subscribe to changes** — `pg_ripple.subscribe(pattern TEXT, channel TEXT)` sends a PostgreSQL `LISTEN/NOTIFY` message to `channel` every time a triple matching `pattern` is inserted or deleted; use `'*'` to receive all changes
- **Unsubscribe** — `pg_ripple.unsubscribe(channel TEXT)` stops notifications on a channel
- **Get storage statistics** — `pg_ripple.stats()` reports total triple count, how many predicates have their own table, how many triples are still in the write buffer, and the background worker's process ID

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.compact()` | `BIGINT` | Merge all pending writes into the read store |
| `pg_ripple.stats()` | `JSONB` | Storage and background worker statistics |
| `pg_ripple.subscribe(pattern TEXT, channel TEXT)` | `BIGINT` | Subscribe to change notifications |
| `pg_ripple.unsubscribe(channel TEXT)` | `BIGINT` | Stop notifications on a channel |
| `pg_ripple.htap_migrate_predicate(pred_id BIGINT)` | `void` | Migrate one property table to the split-storage layout |
| `pg_ripple.subject_predicates(subject_id BIGINT)` | `BIGINT[]` | All properties for a given subject (fast lookup) |
| `pg_ripple.object_predicates(object_id BIGINT)` | `BIGINT[]` | All properties for a given object (fast lookup) |

### New configuration options

| Option | Default | Description |
|--------|---------|-------------|
| `pg_ripple.merge_threshold` | `10000` | Minimum pending writes before background merge starts |
| `pg_ripple.merge_interval_secs` | `60` | Maximum seconds between merge cycles |
| `pg_ripple.merge_retention_seconds` | `60` | How long to keep the previous read table before dropping it |
| `pg_ripple.latch_trigger_threshold` | `10000` | Pending writes needed to wake the merge worker early |
| `pg_ripple.worker_database` | `postgres` | Which database the merge worker connects to |
| `pg_ripple.merge_watchdog_timeout` | `300` | Log a warning if the merge worker is silent for this many seconds |

### Bug fixes in this release

- **Startup race condition** — the extension's shared memory flag is now set inside the correct PostgreSQL startup hook, eliminating a rare crash window during server start
- **GUC registration crash** — configuration parameters requiring postmaster-level access no longer crash when `CREATE EXTENSION pg_ripple` runs without the extension in `shared_preload_libraries`
- **SPARQL aggregate decode bug** — `COUNT`, `SUM`, and similar aggregate results were incorrectly looked up in the string dictionary; they now pass through as plain numbers
- **Merge worker: DROP TABLE without CASCADE** — the merge worker failed if old tables had dependent views; fixed by using `CASCADE` and recreating the view afterwards
- **Merge worker: stale index name** — repeated `compact()` calls failed with "relation already exists" because the old index name survived a table rename; the stale index is now dropped before creating a new one

### Upgrading from v0.5.1

```sql
ALTER EXTENSION pg_ripple UPDATE;
```

The migration script adds a column to the predicate catalog, creates the pattern tables and change-notification infrastructure, and converts every existing property table to the split read/write layout in a single transaction. Existing triples land in the write buffer; call `pg_ripple.compact()` afterwards to move them into the read store immediately.

<details>
<summary>Technical details</summary>

- HTAP split: writes → `vp_{id}_delta` (heap + B-tree); cross-partition deletes → `vp_{id}_tombstones`; query view = `(main EXCEPT tombstones) UNION ALL delta`
- Background merge: sort-ordered insertion into a fresh `vp_{id}_main` (BRIN-indexed) + `ANALYZE`; previous main dropped after `merge_retention_seconds`
- `ExecutorEnd_hook` pokes the merge worker latch when `TOTAL_DELTA_ROWS` reaches `latch_trigger_threshold`
- Subject/object pattern tables (`_pg_ripple.subject_patterns`, `_pg_ripple.object_patterns`) — GIN-indexed `BIGINT[]` columns rebuilt by the merge worker; enable O(1) predicate lookup per node
- CDC notifications fire as `pg_notify(channel, '{"op":"insert|delete","s":...,"p":...,"o":...,"g":...}')` via trigger on each delta table

</details>

---

## [0.5.1] — 2026-04-15 — Compact Number Storage, CONSTRUCT/DESCRIBE, SPARQL Update, Full-Text Search

This release stores common data types (integers, dates, booleans) as compact numbers instead of text, making range comparisons in queries much faster. It also adds the two remaining SPARQL query forms, write support via SPARQL Update, and full-text search on text values.

### What you can do

- **Faster comparisons on numbers and dates** — `xsd:integer`, `xsd:boolean`, `xsd:date`, and `xsd:dateTime` values are stored as compact integers; FILTER comparisons (`>`, `<`, `=`) run as plain integer comparisons with no string decoding
- **SPARQL CONSTRUCT** — `pg_ripple.sparql_construct(query TEXT)` assembles new triples from a template and returns them as a set of `{s, p, o}` JSON objects; useful for transforming or exporting data
- **SPARQL DESCRIBE** — `pg_ripple.sparql_describe(query TEXT, strategy TEXT)` returns the neighbourhood of a resource — all triples directly connected to it (Concise Bounded Description) or both incoming and outgoing triples (Symmetric CBD)
- **SPARQL Update** — `pg_ripple.sparql_update(query TEXT)` executes `INSERT DATA { … }` and `DELETE DATA { … }` statements; returns the number of triples affected
- **Full-text search** — `pg_ripple.fts_index(predicate TEXT)` indexes text values for a property; `pg_ripple.fts_search(query TEXT, predicate TEXT)` searches them using standard PostgreSQL text-search syntax

### Bug fixes

- `fts_index` now accepts N-Triples `<IRI>` notation for the predicate argument
- `fts_index` now uses a correct partial index that does not require PostgreSQL subquery support
- Inline-encoded values (integers, dates) now decode correctly in SPARQL SELECT results instead of returning NULL

### New configuration options

- `pg_ripple.describe_strategy` (default `'cbd'`) — DESCRIBE expansion algorithm: `'cbd'`, `'scbd'` (symmetric), or `'simple'` (subject only)

---

## [0.5.0] — 2026-04-15 — Complete SPARQL 1.1 Query Engine

This release completes SPARQL 1.1 query support. All standard query patterns — graph traversal, aggregates, unions, subqueries, optional matches, and computed values — are now supported.

### What you can do

- **Traverse graph relationships** — property paths (`+`, `*`, `?`, `/`, `|`, `^`) follow chains of relationships; cyclic graphs are handled safely using PostgreSQL's cycle detection
- **Combine results from alternative patterns** — `UNION { ... } UNION { ... }` merges results from two or more patterns; `MINUS { ... }` removes results that match an unwanted pattern
- **Aggregate and group results** — `COUNT`, `SUM`, `AVG`, `MIN`, `MAX`, `GROUP_CONCAT` work with `GROUP BY` and `HAVING` just as in SQL
- **Use subqueries** — nest `{ SELECT … WHERE { … } }` patterns at any depth
- **Compute new values** — `BIND(<expr> AS ?var)` assigns a calculated value to a variable; `VALUES ?x { … }` injects a fixed set of values into a pattern
- **Optional matches** — `OPTIONAL { … }` returns results even when the optional pattern has no data, leaving those variables unbound
- **Limit recursion depth** — `pg_ripple.max_path_depth` caps how deep property-path traversal can go, preventing runaway queries on very large graphs

### Bug fixes

- Sequence paths (`p/q`) no longer produce a Cartesian product when intermediate nodes are anonymous
- `p*` (zero-or-more) paths no longer crash with a PostgreSQL CYCLE syntax error
- `OPTIONAL` no longer produces incorrect results due to an alias collision in the generated SQL
- `GROUP BY` column references no longer go out of scope in the outer query
- `MINUS` join clause now uses the correct column alias
- `VALUES` no longer generates a duplicate alias clause
- `BIND` in aggregate subqueries (`SELECT (COUNT(?p) AS ?cnt)`) now produces the correct SQL expression
- Numbers in FILTER expressions (`FILTER(?cnt >= 2)`) are now emitted as SQL integers instead of dictionary IDs
- Changing `pg_ripple.max_path_depth` mid-session now correctly invalidates the plan cache

<details>
<summary>Technical details</summary>

- Property paths compile to `WITH RECURSIVE … CYCLE` CTEs using PostgreSQL 18's hash-based `CYCLE` clause
- All pg_regress test files are now idempotent — safe to run multiple times against the same database
- `setup.sql` drops and recreates the extension for full isolation between runs
- New tests: `property_paths.sql`, `aggregates.sql`, `resource_limits.sql` — 12/12 pass

</details>

---

## [0.4.0] — 2026-04-14 — Statements About Statements (RDF-star)

This release adds RDF-star: the ability to store facts *about* facts. For example, you can record not just "Alice knows Bob" but also "Alice knows Bob — according to Carol, since 2020". This is essential for provenance tracking, temporal data, and property graph–style edge annotations.

### What you can do

- **Load N-Triples-star data** — `pg_ripple.load_ntriples()` now accepts N-Triples-star, including nested quoted triples in both subject and object position
- **Encode and decode quoted triples** — `pg_ripple.encode_triple(s, p, o)` stores a quoted triple and returns its ID; `pg_ripple.decode_triple(id)` converts it back to JSON
- **Use statement identifiers** — `pg_ripple.insert_triple()` now returns the stable integer identifier of the stored statement; that identifier can itself appear as a subject or object in other triples
- **Look up a statement by its identifier** — `pg_ripple.get_statement(i BIGINT)` returns `{"s":…,"p":…,"o":…,"g":…}` for any stored statement
- **Query with SPARQL-star** — ground (all-constant) quoted triple patterns work in SPARQL `WHERE` clauses: `WHERE { << :Alice :knows :Bob >> :assertedBy ?who }`

### Known limitations in this release

- Turtle-star is not yet supported; use N-Triples-star for RDF-star bulk loading
- Variable-inside-quoted-triple SPARQL patterns (e.g. `<< ?s :knows ?o >> :assertedBy ?who`) are deferred to v0.5.x
- W3C SPARQL-star conformance test suite not yet run (deferred to v0.5.x)

<details>
<summary>Technical details</summary>

- `KIND_QUOTED_TRIPLE = 5` added to the dictionary; quoted triples stored with `qt_s`, `qt_p`, `qt_o` columns via non-destructive `ALTER TABLE … ADD COLUMN IF NOT EXISTS`
- Custom recursive-descent N-Triples-star line parser — avoids the `oxrdf/rdf-12` + `spargebra` feature conflict with no new crate dependencies
- `spargebra` and `sparopt` now use the `sparql-12` feature, enabling `TermPattern::Triple` with correct exhaustiveness guards
- SPARQL-star ground patterns compile to a dictionary lookup + SQL equality condition

</details>

---

## [0.3.0] — 2026-04-14 — SPARQL Query Language

This release introduces SPARQL, the standard W3C query language for RDF data. You can now ask questions over your stored facts using a familiar graph-pattern syntax, with results returned as JSON.

### What you can do

- **Run SPARQL SELECT queries** — `pg_ripple.sparql(query TEXT)` executes a SPARQL SELECT and returns one JSON object per result row, with variable names as keys and values in standard N-Triples format
- **Run SPARQL ASK queries** — `pg_ripple.sparql_ask(query TEXT)` returns `true` if any results exist, `false` otherwise
- **Inspect the generated SQL** — `pg_ripple.sparql_explain(query TEXT, analyze BOOL DEFAULT false)` shows what SQL was generated from a SPARQL query; pass `analyze := true` for a full execution plan with timings
- **Tune the query plan cache** — `pg_ripple.plan_cache_size` (default 256) controls how many SPARQL-to-SQL translations are cached per connection; set to `0` to disable caching

### Supported query features

- Basic graph patterns with bound or wildcard subjects, predicates, and objects
- `FILTER` with comparisons (`=`, `!=`, `<`, `<=`, `>`, `>=`) and boolean operators (`&&`, `||`, `!`, `BOUND()`)
- `OPTIONAL` (left-join)
- `GRAPH <iri> { … }` and `GRAPH ?g { … }` for named graph scoping
- `SELECT` with variable projection, `DISTINCT`, `REDUCED`
- `LIMIT`, `OFFSET`, `ORDER BY`

<details>
<summary>Technical details</summary>

- SPARQL text → `spargebra 0.4` algebra tree → SQL via `src/sparql/sqlgen.rs`; all IRI and literal constants are encoded to `i64` before appearing in SQL — SQL injection via SPARQL constants is structurally impossible
- Per-query encoding cache avoids redundant dictionary lookups for constants appearing multiple times in one query
- Self-join elimination: patterns sharing a subject but using different predicates compile to a single scan, not separate subqueries
- Batch decode: all integer result columns are decoded in a single `SELECT … WHERE id IN (…)` round-trip
- `RUST_TEST_THREADS = "1"` in `.cargo/config.toml` prevents concurrent dictionary upsert deadlocks in the test suite
- New pg_regress tests: `sparql_queries.sql` (10 queries), `sparql_injection.sql` (7 adversarial inputs)

</details>

---

## [0.2.0] — 2026-04-14 — Bulk Loading, Named Graphs, and Export

This release makes it practical to work with large RDF datasets. You can load standard RDF files, organise triples into named collections, export data back to standard formats, and register IRI prefixes for convenience.

### What you can do

- **Load RDF files in bulk** — `pg_ripple.load_ntriples(data TEXT)`, `load_nquads(data TEXT)`, `load_turtle(data TEXT)`, and `load_trig(data TEXT)` accept standard RDF text and return the number of triples loaded
- **Load from a file on the server** — `pg_ripple.load_ntriples_file(path TEXT)` and its siblings read a file directly from the server filesystem (requires superuser); essential for large datasets
- **Organise triples into named graphs** — `pg_ripple.create_graph('<iri>')` creates a named collection; `pg_ripple.drop_graph('<iri>')` deletes it along with its triples; `pg_ripple.list_graphs()` lists all collections
- **Export data** — `pg_ripple.export_ntriples(graph)` and `pg_ripple.export_nquads(graph)` serialise stored triples to standard text; pass `NULL` to export all triples
- **Register IRI prefixes** — `pg_ripple.register_prefix('ex', 'https://example.org/')` records a shorthand; `pg_ripple.prefixes()` lists all registered mappings
- **Promote rare properties manually** — `pg_ripple.promote_rare_predicates()` moves any property that has grown beyond the threshold into its own dedicated table

### How rare properties work

Properties with fewer than 1,000 triples (configurable via `pg_ripple.vp_promotion_threshold`) are stored in a shared table rather than creating a dedicated table for each one. Once a property crosses the threshold it is automatically migrated. This keeps the database tidy for datasets with many rarely-used properties.

### How blank node scoping works

Blank node identifiers (`_:b0`, `_:b1`, etc.) from different load calls are automatically isolated. Loading the same file twice will produce separate, independent blank nodes rather than merging them — which is almost always what you want.

<details>
<summary>Technical details</summary>

- `rio_turtle 0.8` / `rio_api 0.8` added for N-Triples, N-Quads, Turtle, and TriG parsing
- Blank node scoping via `_pg_ripple.load_generation_seq`: each load advances a shared sequence; blank node hashes are prefixed with `"{generation}:"` to prevent cross-load merging
- `batch_insert_encoded` groups triples by predicate and issues one multi-row INSERT per predicate group, reducing round-trips
- `_pg_ripple.statements` range-mapping table created (populated in v0.6.0)
- `_pg_ripple.prefixes` table: `(prefix TEXT PRIMARY KEY, expansion TEXT)`
- GUCs added: `pg_ripple.vp_promotion_threshold` (i32, default 1000), `pg_ripple.named_graph_optimized` (bool, default off)
- New pg_regress tests: `triple_crud.sql`, `named_graphs.sql`, `export_ntriples.sql`, `nquads_trig.sql`

</details>

---

## [0.1.0] — 2026-04-14 — First Working Release

pg_ripple can now be installed into a PostgreSQL 18 database. After installation you can store facts — statements like "Alice knows Bob" — and retrieve them by pattern. This is the foundation that all later releases build on. No query language yet: just the core building blocks.

### What you can do

- **Install the extension** — `CREATE EXTENSION pg_ripple` in any PostgreSQL 18 database (requires superuser)
- **Store facts** — `pg_ripple.insert_triple('<Alice>', '<knows>', '<Bob>')` saves a fact and returns a unique identifier for it
- **Find facts by pattern** — `pg_ripple.find_triples('<Alice>', NULL, NULL)` returns everything about Alice; `NULL` is a wildcard for any position
- **Delete facts** — `pg_ripple.delete_triple(…)` removes a specific fact
- **Count facts** — `pg_ripple.triple_count()` returns how many facts are stored
- **Encode and decode terms** — `pg_ripple.encode_term(…)` converts a text term to its internal numeric ID; `pg_ripple.decode_id(…)` converts it back

### How storage works

Every piece of text — names, URLs, values — is converted to a compact integer before storage. Lookups and joins operate on integers, not strings, which is what makes queries fast. Facts are automatically organised into one table per relationship type, and relationship types with few facts share a single table to avoid creating thousands of tiny tables. Every fact receives a globally unique integer identifier that later versions use for RDF-star.

<details>
<summary>Technical details</summary>

- pgrx 0.17 project scaffolding targeting PostgreSQL 18
- Extension bootstrap creates `pg_ripple` (user-visible) and `_pg_ripple` (internal) schemas; the `pg_` prefix requires `SET LOCAL allow_system_table_mods = on` during bootstrap
- Dictionary encoder (`src/dictionary/mod.rs`): `_pg_ripple.dictionary` table; XXH3-128 hash stored in BYTEA; dense IDENTITY sequence as join key; backend-local LRU encode/decode caches; CTE-based upsert avoids pgrx 0.17 `InvalidPosition` error on empty `RETURNING` results
- Vertical partitioning (`src/storage/mod.rs`): `_pg_ripple.vp_{predicate_id}` tables with dual B-tree indices on `(s,o)` and `(o,s)`; `_pg_ripple.predicates` catalog; `_pg_ripple.vp_rare` consolidation table; `_pg_ripple.statement_id_seq` for globally-unique statement IDs
- Error taxonomy (`src/error.rs`): `thiserror`-based types — PT001–PT099 (dictionary), PT100–PT199 (storage)
- GUC: `pg_ripple.default_graph`
- CI pipeline: fmt, clippy, pg_test, pg_regress (`.github/workflows/ci.yml`)
- pg_regress tests: `setup.sql`, `dictionary.sql`, `basic_crud.sql`

</details>
