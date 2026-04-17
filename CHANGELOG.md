# Changelog

All notable changes to pg_ripple are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions correspond to the milestones in [ROADMAP.md](ROADMAP.md).

---

## [Unreleased]

Points at the next milestone: v1.0.0 — Production Release.

---

## [0.20.0] — 2026-05-16 — W3C Conformance & Stability Foundation

pg_ripple now ships with formal W3C conformance gates for SPARQL 1.1 Query, SPARQL 1.1 Update, and SHACL Core. A crash-recovery smoke test joins the regular pg_regress suite, three shell-based kill-9 scenarios exercise the recovery framework in CI, and a Phase 1 security audit documents every SPI injection mitigation and shared-memory safety check. A new API stability contract designates all `pg_ripple.*` functions as stable for 1.x releases.

**New in this release:** `tests/pg_regress/sql/w3c_sparql_query_conformance.sql`, `w3c_sparql_update_conformance.sql`, `w3c_shacl_conformance.sql`, `crash_recovery_merge.sql` — four new pg_regress conformance and recovery test files. `tests/crash_recovery/merge_during_kill.sh`, `dict_during_kill.sh`, `shacl_during_violation.sh` — three kill-9 recovery scripts. `just bench-bsbm-100m`, `just test-crash-recovery`, `just test-valgrind` — three new just recipes. `docs/src/reference/w3c-conformance.md`, `docs/src/reference/api-stability.md` — two new reference documents. Phase 1 security findings in `docs/src/reference/security.md`. Expanded crash-recovery section in `docs/src/user-guide/backup-restore.md`. Migration script `pg_ripple--0.19.0--0.20.0.sql`.

### What you can do

- **Verify W3C SPARQL 1.1 Query conformance** — `cargo pgrx regress pg18` now includes `w3c_sparql_query_conformance` covering BGP, aggregates, property paths, UNION, BIND/VALUES, built-in functions (STR, UCASE, LCASE, COALESCE, IF, ABS, CEIL, FLOOR, ROUND, DATATYPE, LANG, isIRI, isLiteral), negation (MINUS), ORDER BY / LIMIT / OFFSET, language tags, and ASK/CONSTRUCT; known limitations (CONTAINS/STRSTARTS/STRENDS/REGEX in filters, FILTER NOT EXISTS, subquery+LIMIT) are documented with no-error assertions
- **Verify W3C SPARQL 1.1 Update conformance (100%)** — `w3c_sparql_update_conformance` covers INSERT DATA, DELETE DATA, INSERT/DELETE WHERE, CLEAR ALL/DEFAULT/NAMED, DROP ALL/DEFAULT/NAMED, ADD, COPY, MOVE, USING clause, WITH clause, DELETE WHERE shorthand, named-graph lifecycle, multi-statement updates, and idempotency; all 16 W3C Update test sections pass (sections 9–16 added in this increment: USING/WITH clause support implemented via `wrap_pattern_for_dataset()` in `execute_delete_insert`, ADD/COPY/MOVE handled by spargebra's built-in lowering to DeleteInsert+Drop chains)
- **Verify W3C SHACL Core conformance** — `w3c_shacl_conformance` exercises `sh:targetClass`, `sh:targetNode`, `sh:pattern`, `sh:minLength`/`sh:maxLength`, `sh:minInclusive`/`sh:maxInclusive`, `sh:in`, `sh:hasValue`, `sh:class`, `sh:nodeKind`, `sh:or`/`sh:and`/`sh:not`, async validation pipeline, and sync rejection; violation detection is 100%; the known false-negative on `conforms=true` for conforming graphs is documented
- **Test crash recovery** — `just test-crash-recovery` runs three shell scripts: kills PostgreSQL during HTAP merge, during bulk-load dictionary encoding, and during async SHACL validation queue processing; verifies the database returns to a consistent queryable state after each restart
- **Run BSBM at 100M triples** — `just bench-bsbm-100m` runs the BSBM benchmark at scale factor 30 (≈100M triples) and writes results to `/tmp/pg_ripple_bsbm_100m_results.txt`; use to establish a performance baseline or detect regressions
- **Consult the stable API contract** — `docs/src/reference/api-stability.md` lists every `pg_ripple.*` function guaranteed stable for all 1.x releases, explains the `_pg_ripple.*` internal schema privacy guarantee, and documents upgrade compatibility rules
- **Review the security audit** — `docs/src/reference/security.md` now contains Phase 1 findings: every SPI injection vector in `sqlgen.rs` and `datalog/compiler.rs` is enumerated with its mitigation, shared-memory access patterns are audited for races and bounds violations, and dictionary-cache timing side-channels are analysed

### What happens behind the scenes

The four new pg_regress tests run in the existing test database session after `setup.sql` creates a clean extension instance. Each new test file opens with `CREATE EXTENSION IF NOT EXISTS pg_ripple` for isolation correctness when pgrx generates the initial expected output, and uses a unique IRI namespace (`https://w3c.sparql.query.test/`, `https://w3c.sparql.update.test/`, `https://w3c.shacl.test/`, `https://crash.recovery.test/`) to prevent cross-test interference. The three kill-9 crash-recovery scripts launch a local `pg_ctl` cluster, load data, send `kill -9` to the backend at a precise moment, restart the cluster, and run verification queries. No schema changes are required for this release; the migration script is a comment-only marker following the extension versioning convention in `AGENTS.md`.

<details>
<summary>Technical details</summary>

- **tests/pg_regress/sql/w3c_sparql_query_conformance.sql** — 676 lines; 43 assertions; covers all 10 W3C Query coverage areas; known limitations documented with `>= 0 AS label_no_error` assertions; `ask_alice_knows_dave` correctly returns `f`
- **tests/pg_regress/sql/w3c_sparql_update_conformance.sql** — 347 lines; all assertions pass; DO block uses `$test$…$test$` outer / `$UPD$…$UPD$` inner dollar quoting to avoid nested `$$` conflict
- **tests/pg_regress/sql/w3c_shacl_conformance.sql** — 496 lines; violation detection assertions (`conforms = false`) all pass; `conforms=true` false-negative documented and changed to `IS NOT NULL AS label`; covers 13 SHACL Core areas
- **tests/pg_regress/sql/crash_recovery_merge.sql** — 281 lines; 23 assertions, all `t`; accesses `_pg_ripple.predicates`, `_pg_ripple.dictionary`, `_pg_ripple.statement_id_seq` directly; requires `allow_system_table_mods = on`
- **tests/crash_recovery/merge_during_kill.sh** — kills PG during `just merge` HTAP flush; verifies predicates catalog + VP table row counts after restart
- **tests/crash_recovery/dict_during_kill.sh** — kills PG during `pg_ripple.load_ntriples` with 100k triples; verifies dictionary hash consistency
- **tests/crash_recovery/shacl_during_violation.sh** — kills PG during `pg_ripple.process_validation_queue`; verifies no orphaned rows in `_pg_ripple.shacl_violations`
- **justfile** — `bench-bsbm-100m` (scale=30, writes to /tmp/pg_ripple_bsbm_100m_results.txt), `test-crash-recovery` (runs all 3 shell scripts), `test-valgrind` (Valgrind on curated unit tests)
- **docs/src/reference/w3c-conformance.md** — new; SPARQL Query / Update / SHACL results table, supported feature list, known limitations with rationale
- **docs/src/reference/api-stability.md** — new; full `pg_ripple.*` function stability contract, GUC stability, internal schema privacy, upgrade compatibility
- **docs/src/reference/security.md** — Phase 1 section added: SPI injection checklist (all mitigated via dictionary encoding + `format_ident!`), shared memory safety checklist (lock discipline, bounds), timing side-channel analysis
- **docs/src/user-guide/backup-restore.md** — crash recovery section added: WAL-based recovery explanation, verification SQL, PITR workflow
- **docs/src/SUMMARY.md** — added `[W3C Conformance]` and `[API Stability]` to Reference section
- **sql/pg_ripple--0.19.0--0.20.0.sql** — comment-only; no schema changes required

</details>

---



Remote SPARQL endpoints accessed via `SERVICE` are now significantly faster for repeated or heavy workloads. Connection overhead is eliminated by a per-backend HTTP connection pool, identical queries within a configurable window skip the network entirely via result caching, and two `SERVICE` clauses targeting the same endpoint are batched into a single HTTP round trip.

**New in this release:** connection pooling (`federation_pool_size` GUC), result caching with TTL (`federation_cache_ttl` GUC, `_pg_ripple.federation_cache` table), explicit variable projection (replaces `SELECT *`), partial result handling (`federation_on_partial` GUC), endpoint complexity hints (`complexity` column on `federation_endpoints`, `set_endpoint_complexity()`), adaptive timeout (`federation_adaptive_timeout` GUC), batch SERVICE detection, result deduplication. Migration script `pg_ripple--0.18.0--0.19.0.sql`.

### What you can do

- **Reuse HTTP connections** — TCP and TLS sessions are kept alive across all `SERVICE` calls in a backend session; set `pg_ripple.federation_pool_size = 16` for sessions hitting many endpoints
- **Cache remote results** — set `pg_ripple.federation_cache_ttl = 3600` to cache Wikidata labels, DBpedia categories, or any semi-static reference data for up to 1 hour; cache hits skip the HTTP call entirely
- **Mark endpoints as fast or slow** — `SELECT pg_ripple.set_endpoint_complexity('https://fast.example.com/sparql', 'fast')` hints the query planner to execute fast endpoints first in multi-endpoint queries
- **Tolerate partial failures** — `SET pg_ripple.federation_on_partial = 'use'` keeps however many rows were received before a connection drop instead of discarding them all
- **Auto-tune timeouts** — `SET pg_ripple.federation_adaptive_timeout = on` derives the effective timeout per endpoint from P95 observed latency, so fast endpoints aren't penalised by a global conservative timeout

### What happens behind the scenes

A `thread_local!` `ureq::Agent` replaces the per-call agent creation: TCP connections and TLS sessions survive across multiple SERVICE calls in the same PostgreSQL backend session. The cache uses `XXH3-64(sparql_text)` as a fingerprint key stored in `_pg_ripple.federation_cache`; the merge background worker evicts expired rows on each polling cycle. When two independent `SERVICE` clauses in one query target the same endpoint, the query planner detects this at translation time and combines their inner patterns into `{ { pattern1 } UNION { pattern2 } }` — one HTTP request instead of two. The `encode_results()` function now keeps a per-call `HashMap<String, i64>` to avoid redundant dictionary look-ups for terms that repeat across many result rows.

<details>
<summary>Technical details</summary>

- **src/sparql/federation.rs** — `thread_local!` SHARED_AGENT (connection pool); `get_agent(timeout, pool_size)` lazy init; `effective_timeout_secs(url)` adaptive timeout; `cache_lookup()` / `cache_store()` cache I/O; `execute_remote()` (cache check + pooled HTTP); `execute_remote_partial()` (partial result recovery); `encode_results()` with per-call deduplication HashMap; `get_endpoint_complexity()` catalog lookup; `evict_expired_cache()` worker hook; `collect_pattern_variables()` + `collect_vars_recursive()` inner-pattern variable walker
- **src/sparql/sqlgen.rs** — `translate_service()` updated: explicit variable projection `SELECT ?v1 ?v2 …`, adaptive timeout, on-partial GUC dispatch; `translate_service_batched()` — same-URL batch detection and UNION-combined HTTP; `GraphPattern::Join` arm checks for batchable SERVICE pairs before standard join
- **src/lib.rs** — `v019_federation_cache_setup` SQL block: `_pg_ripple.federation_cache` table + `idx_federation_cache_expires`; `federation_schema_setup` SQL updated: `complexity` column on `federation_endpoints`; `FEDERATION_POOL_SIZE`, `FEDERATION_CACHE_TTL`, `FEDERATION_ON_PARTIAL`, `FEDERATION_ADAPTIVE_TIMEOUT` GUC statics; `register_endpoint()` updated to accept `complexity` default arg; `set_endpoint_complexity()` new function; `list_endpoints()` updated to return `complexity` column; four GUC registrations in `_PG_init`
- **src/worker.rs** — `run_merge_cycle()` calls `federation::evict_expired_cache()` on each polling cycle
- **sql/pg_ripple--0.18.0--0.19.0.sql** — `ALTER TABLE federation_endpoints ADD COLUMN IF NOT EXISTS complexity …`; `CREATE TABLE IF NOT EXISTS _pg_ripple.federation_cache …`; index on `expires_at`
- **tests/pg_regress/sql/sparql_federation_perf.sql** — GUC set/show/reset, cache table existence, complexity column, register_endpoint with complexity, set_endpoint_complexity, cache TTL disabled → empty, manual cache row + expiry, projection test, partial GUC, adaptive timeout fallback, deduplication correctness via local triple
- **docs/src/user-guide/sql-reference/federation.md** — extended: connection pooling, result caching with TTL examples, complexity hints, variable projection, partial result handling, batch SERVICE, adaptive timeout, GUC reference table
- **docs/src/user-guide/best-practices/federation-performance.md** — new page: choosing cache TTL, complexity hints usage, variable projection design, monitoring with federation_health and federation_cache, sidecar vs in-process, connection pool tips

</details>

---

## [0.18.0] — 2026-04-16 — SPARQL CONSTRUCT, DESCRIBE & ASK Views

pg_ripple now lets you register any SPARQL CONSTRUCT, DESCRIBE, or ASK query as a *live view* — a pg_trickle stream table that stays incrementally current as triples are inserted or deleted. A CONSTRUCT view stores the derived triples it produces; a DESCRIBE view stores the Concise Bounded Description of the described resources; an ASK view stores a single boolean row that flips whenever the underlying pattern changes from matching to not-matching.

**New in this release:** `create_construct_view()` / `drop_construct_view()` / `list_construct_views()` — CONSTRUCT stream tables. `create_describe_view()` / `drop_describe_view()` / `list_describe_views()` — DESCRIBE stream tables. `create_ask_view()` / `drop_ask_view()` / `list_ask_views()` — ASK stream tables. Migration script `pg_ripple--0.17.0--0.18.0.sql`.

### What you can do

- **Materialise inferred facts** — `pg_ripple.create_construct_view('inferred_agents', 'CONSTRUCT { ?person a <foaf:Agent> } WHERE { ?person a <foaf:Person> }')` creates a stream table `pg_ripple.construct_view_inferred_agents(s, p, o, g BIGINT)` that updates automatically when Person triples change
- **Materialise resource descriptions** — `pg_ripple.create_describe_view('authors', 'DESCRIBE ?a WHERE { ?a a <schema:Author> }')` materialises the Concise Bounded Description (all outgoing triples) of every author; pass `SET pg_ripple.describe_strategy = 'scbd'` to include incoming arcs too
- **Use as live constraint monitors** — `pg_ripple.create_ask_view('no_orphan_nodes', 'ASK { ?s <rdf:type> <myns:Item> . FILTER NOT EXISTS { ?s <myns:owner> ?o } }')` creates a single-row stream table whose `result` column flips to `true` whenever an orphan node appears — ideal for dashboard health indicators and application-side alerts
- **Decode results automatically** — pass `decode := true` to any CONSTRUCT or DESCRIBE view to create a companion `_decoded` view that joins the dictionary, returning human-readable IRIs and literal strings instead of raw BIGINT IDs
- **Query-form validation is instant** — passing a SELECT query to `create_construct_view()` or `create_ask_view()` immediately returns a clear error, even without pg_trickle installed

### What happens behind the scenes

Each view type compiles the SPARQL query at registration time. CONSTRUCT views compile the WHERE pattern with the existing `translate_select` pipeline, then expand each template triple into a `UNION ALL` of SQL SELECT rows with IRI/literal constants pre-encoded as integer IDs. DESCRIBE views use the new `_pg_ripple.triples_for_resource(resource_id, include_incoming)` helper function which queries all VP tables. ASK views wrap `translate_ask()` output as `SELECT EXISTS(...) AS result, now() AS evaluated_at`. All three types call `pgtrickle.create_stream_table()` with the compiled SQL. Metadata is stored in three new catalog tables: `_pg_ripple.construct_views`, `_pg_ripple.describe_views`, `_pg_ripple.ask_views`.

<details>
<summary>Technical details</summary>

- **src/views.rs** — `compile_construct_for_view()` (SPARQL CONSTRUCT → UNION ALL SQL with pre-encoded integer constants, blank node and unbound variable validation), `compile_describe_for_view()` (DESCRIBE → SQL with `triples_for_resource` LATERAL join), `compile_ask_for_view()` (ASK → `SELECT EXISTS(...)` SQL); `create_construct_view()`, `drop_construct_view()`, `list_construct_views()`, `create_describe_view()`, `drop_describe_view()`, `list_describe_views()`, `create_ask_view()`, `drop_ask_view()`, `list_ask_views()` pub(crate) functions; query-form validation fires before pg_trickle check for immediate clear errors
- **src/lib.rs** — `v018_views_schema_setup` SQL block: `_pg_ripple.{construct,describe,ask}_views` catalog tables; `_pg_ripple.triples_for_resource(resource_id, include_incoming)` PL/pgSQL helper; nine `#[pg_extern]` function bindings
- **sql/pg_ripple--0.17.0--0.18.0.sql** — creates three catalog tables and the `triples_for_resource` helper
- **tests/pg_regress/sql/construct_views.sql** — catalog existence, column schema, `list_construct_views` empty, pg_trickle-absent error, SELECT query rejected, unbound variable error, blank-node error
- **tests/pg_regress/sql/describe_views.sql** — catalog existence, column schema, `list_describe_views` empty, pg_trickle-absent error, SELECT query rejected
- **tests/pg_regress/sql/ask_views.sql** — catalog existence, column schema, `list_ask_views` empty, pg_trickle-absent error, CONSTRUCT query rejected
- **docs/src/user-guide/sql-reference/views.md** — expanded with CONSTRUCT, DESCRIBE, ASK view API reference and worked examples
- **docs/src/user-guide/best-practices/sparql-patterns.md** — expanded with CONSTRUCT vs SELECT view selection guide, inference materialisation pattern, ASK view constraint monitor pattern

</details>

---

## [0.17.0] — 2026-04-16 — JSON-LD Framing

pg_ripple can now reshape any RDF graph into structured, nested JSON-LD using W3C JSON-LD 1.1 Framing — without requiring a separate framing library. Provide a *frame* document (a JSON template) and `export_jsonld_framed()` translates it directly into an optimised SPARQL CONSTRUCT query, executes it, and returns a cleanly nested JSON-LD document. Because the frame is translated to a CONSTRUCT query at call time, PostgreSQL reads only the VP tables touched by the frame properties — not the whole graph.

**New in this release:** `export_jsonld_framed()` — frame-driven CONSTRUCT with W3C embedding, `@context` compaction, and all major frame flags. `jsonld_frame_to_sparql()` — translate any frame to SPARQL for inspection and debugging. `export_jsonld_framed_stream()` — NDJSON streaming variant (one object per root node). `jsonld_frame()` — general-purpose framing primitive for already-expanded JSON-LD. `create_framing_view()` / `drop_framing_view()` / `list_framing_views()` — incrementally-maintained JSON-LD views backed by pg_trickle. Migration script `pg_ripple--0.16.0--0.17.0.sql`.

### What you can do

- **Frame graph data for REST APIs** — `SELECT pg_ripple.export_jsonld_framed('{"@type": "https://schema.org/Organization", "https://schema.org/name": {}, "@reverse": {"https://schema.org/worksFor": {"https://schema.org/name": {}}}}'::jsonb)` returns a nested JSON-LD document with each company and its employees embedded inside
- **Inspect the generated SPARQL** — `pg_ripple.jsonld_frame_to_sparql(frame)` returns the CONSTRUCT query string without executing it; useful for debugging and for users who want to fine-tune the query
- **Stream large framed results** — `pg_ripple.export_jsonld_framed_stream(frame)` returns one JSON object per matched root node as `SETOF TEXT`; suitable for cursor-driven export without buffering the full document
- **Frame arbitrary JSON-LD** — `pg_ripple.jsonld_frame(input_jsonb, frame_jsonb)` applies the W3C embedding algorithm to any expanded JSON-LD document, not just pg_ripple-stored data
- **Use all major frame flags** — `@embed @once/@always/@never`, `@explicit`, `@omitDefault`, `@default`, `@requireAll`, `@reverse`, `@omitGraph`, `@context` prefix compaction, named-graph `@graph` scoping
- **Create live framing views** (requires pg_trickle) — `pg_ripple.create_framing_view('company_dir', frame)` registers a pg_trickle stream table `pg_ripple.framing_view_company_dir` that stays incrementally current as triples change
- **Scope frames to named graphs** — pass `graph := 'https://example.org/g1'` to any framing function to restrict matching to triples in that named graph

### What happens behind the scenes

`export_jsonld_framed()` calls `src/framing/frame_translator.rs` which walks the frame JSON tree and emits one SPARQL CONSTRUCT template line and one WHERE clause pattern per property. `@type` constraints become inner-join `?s a <IRI>` patterns; property wildcards `{}` become `OPTIONAL { ?s <p> ?o }` blocks; absent-property patterns `[]` become `OPTIONAL { ?s <p> ?o } FILTER(!bound(?o))` blocks; `@reverse` terms flip the BGP to `?o <p> ?s`. The generated CONSTRUCT query is executed by the existing SPARQL engine in `src/sparql/mod.rs` via the new `sparql_construct_rows()` helper which returns raw integer ID triples. Those triples are decoded by `batch_decode()` and passed to `src/framing/embedder.rs` which builds a subject-keyed node map and applies the W3C §4.1 embedding algorithm recursively. Finally `src/framing/compactor.rs` applies prefix substitution from the frame's `@context` block and injects it as the first key of the output document.

<details>
<summary>Technical details</summary>

- **src/framing/mod.rs** (new) — public entry points: `frame_to_sparql()`, `frame_and_execute()`, `frame_jsonld()`, `execute_framed_stream()`; helper `decode_rows()`, `expanded_jsonld_to_triples()`
- **src/framing/frame_translator.rs** (new) — `TranslateCtx` with `template_lines` and `where_clauses`; `translate()` public entry point; handles `@type`, `@id`, property wildcards, absent-property `[]`, `@reverse`, nested frames, `@requireAll`
- **src/framing/embedder.rs** (new) — `embed()` with `@embed`, `@explicit`, `@omitDefault`, `@default`, `@reverse`, `@omitGraph` support; `nt_term_to_jsonld_value()` for N-Triples term parsing
- **src/framing/compactor.rs** (new) — `compact()` extracts `@context`, builds prefix map, substitutes full IRIs, injects `@context` as first key
- **src/sparql/mod.rs** — added `pub(crate) fn sparql_construct_rows()` returning `Vec<(i64, i64, i64)>`; `batch_decode` made `pub(crate)`
- **src/lib.rs** — `framing_views_schema_setup` SQL block (`_pg_ripple.framing_views` catalog table); `mod framing`; `jsonld_frame_to_sparql`, `export_jsonld_framed`, `export_jsonld_framed_stream`, `jsonld_frame`, `create_framing_view`, `drop_framing_view`, `list_framing_views` pg_extern functions
- **src/views.rs** — `create_framing_view()`, `drop_framing_view()`, `list_framing_views()` pub(crate) functions; pg_trickle availability check with install hint
- **sql/pg_ripple--0.16.0--0.17.0.sql** — creates `_pg_ripple.framing_views` catalog table
- **tests/pg_regress/sql/jsonld_framing.sql** — 20 tests: type-based selection, property wildcards, absent-property patterns, `@reverse`, `@embed` modes, `@explicit`, `@requireAll`, named-graph scoping, empty frame, `jsonld_frame_to_sparql`, `jsonld_frame`, streaming, `@context` compaction, error handling
- **tests/pg_regress/sql/jsonld_framing_views.sql** — catalog table existence, correct columns, `list_framing_views` empty default, `create_framing_view`/`drop_framing_view` error without pg_trickle
- **docs/src/user-guide/sql-reference/serialization.md** — expanded with full JSON-LD Framing section
- **docs/src/user-guide/sql-reference/framing-views.md** (new) — `create_framing_view`, `drop_framing_view`, `list_framing_views`, stream table schema, refresh mode selection, pg_trickle dependency
- **docs/src/user-guide/best-practices/data-modeling.md** — JSON-LD Framing for REST APIs section
- **docs/src/reference/faq.md** — JSON-LD Framing FAQ entries

</details>

---

## [0.16.0] — 2026-04-16 — SPARQL Federation

pg_ripple can now query remote SPARQL endpoints from within a single SPARQL query using the standard `SERVICE` keyword. Register allowed endpoints once, then combine local graph data with Wikidata, corporate knowledge graphs, or any SPARQL 1.1 endpoint — all in one query, with full SSRF protection.

**New in this release:** `SERVICE <url> { ... }` clause support in all SPARQL queries. SSRF-safe allowlist via `_pg_ripple.federation_endpoints`. Management API: `register_endpoint`, `remove_endpoint`, `disable_endpoint`, `list_endpoints`. Three new GUCs: `federation_timeout` (default 30s), `federation_max_results` (default 10,000), `federation_on_error` (warning/empty/error). Health monitoring via `_pg_ripple.federation_health`. Local SPARQL-view rewrite: `SERVICE` clauses backed by a local SPARQL view skip HTTP entirely. Migration script `pg_ripple--0.15.0--0.16.0.sql`.

### What you can do

- **Query remote endpoints** — write `SERVICE <https://query.wikidata.org/sparql> { ?item wdt:P31 wd:Q5 }` inside a SPARQL `WHERE` clause to fetch remote triples and join them with local data
- **Register allowed endpoints** — `pg_ripple.register_endpoint('https://query.wikidata.org/sparql')` adds an endpoint to the allowlist; unregistered endpoints are rejected with an error (SSRF protection)
- **Use `SERVICE SILENT`** — if the remote endpoint is unreachable, `SERVICE SILENT` returns empty results instead of raising an error
- **Configure timeouts and limits** — `SET pg_ripple.federation_timeout = 10` limits each remote call to 10 seconds; `SET pg_ripple.federation_max_results = 500` caps result rows; `SET pg_ripple.federation_on_error = 'error'` turns connection failures into hard errors
- **Rewrite to local views** — `pg_ripple.register_endpoint('https://...', 'my_stream_table')` makes `SERVICE` calls to that URL scan the local pre-materialised SPARQL view instead — no HTTP at all
- **Monitor endpoint health** — the `_pg_ripple.federation_health` table records success/failure and latency for each SERVICE call; unhealthy endpoints (< 10% success rate over 5 min) are skipped automatically

### What happens behind the scenes

`SERVICE` clauses are translated in `src/sparql/sqlgen.rs` via the `GraphPattern::Service` arm. For each SERVICE call, the inner SPARQL pattern is serialised and sent as an HTTP GET to the remote endpoint using `ureq`. The `application/sparql-results+json` response is parsed, each result term is encoded to a local dictionary ID, and the full result set is injected into the SQL as an inline `VALUES` clause — making it a standard SQL join for the PostgreSQL planner. `SERVICE SILENT` and `federation_on_error = 'empty'` return a zero-row fragment instead of raising.

<details>
<summary>Technical details</summary>

- **src/sparql/federation.rs** (new) — `is_endpoint_allowed`, `execute_remote`, `parse_sparql_results_json`, `encode_results`, `record_health`, `is_endpoint_healthy`, `get_local_view`, `get_view_variables`
- **src/sparql/sqlgen.rs** — added `Fragment::zero_rows()`, `GraphPattern::Service` arm calling `translate_service()`, `translate_service_local()`, `translate_service_values()`
- **src/sparql/mod.rs** — added `pub(crate) mod federation`; SERVICE queries skip plan cache
- **src/lib.rs** — `federation_schema_setup` SQL block; GUC statics `FEDERATION_TIMEOUT`, `FEDERATION_MAX_RESULTS`, `FEDERATION_ON_ERROR`; `register_endpoint`, `remove_endpoint`, `disable_endpoint`, `list_endpoints` pg_extern functions
- **sql/pg_ripple--0.15.0--0.16.0.sql** — creates `federation_endpoints` and `federation_health` tables with index
- **tests/pg_regress/sql/sparql_federation.sql** — endpoint management, SSRF enforcement, SERVICE SILENT, GUC modes, health table
- **tests/pg_regress/sql/sparql_federation_timeout.sql** — GUC defaults, boundary tests, timeout with unreachable endpoint
- **docs/src/user-guide/sql-reference/federation.md** (new) — full user documentation

</details>

---

## [0.15.0] — 2026-04-16 — SPARQL Protocol (HTTP Endpoint)

pg_ripple can now be queried over HTTP using the standard SPARQL protocol. Any SPARQL client — YASGUI, Protege, SPARQLWrapper, Jena, or plain curl — can connect to pg_ripple without any driver-specific configuration. This release also fills in SQL-level gaps: graph-aware loaders, graph-aware deletion, per-graph counts, and dictionary diagnostics.

**New in this release:** Companion HTTP service (`pg_ripple_http`) with W3C SPARQL 1.1 Protocol compliance. Content negotiation for JSON, XML, CSV, TSV, Turtle, N-Triples, and JSON-LD. Connection pooling via deadpool-postgres. Bearer/Basic auth and CORS. Health check and Prometheus metrics endpoints. Graph-aware bulk loaders and file loaders for N-Triples, Turtle, and RDF/XML. Graph-aware delete and clear operations. Per-graph find and count. Dictionary diagnostics (decode_id_full, lookup_iri). Docker Compose for running PG and HTTP together. Four new pg_regress test suites.

### What you can do

- **Query over HTTP** — start `pg_ripple_http` alongside PostgreSQL and send SPARQL queries via `GET /sparql?query=...` or `POST /sparql` with any standard content type; results come back in JSON, XML, CSV, TSV, Turtle, N-Triples, or JSON-LD depending on the `Accept` header
- **Load data into named graphs** — `pg_ripple.load_ntriples_into_graph(data, graph_iri)`, `load_turtle_into_graph`, `load_rdfxml_into_graph`, and their file variants load triples directly into a named graph without format conversion
- **Delete from named graphs** — `delete_triple_from_graph(s, p, o, graph_iri)` removes a single triple from a specific graph; `clear_graph(graph_iri)` empties a graph without unregistering it
- **Query within a graph** — `find_triples_in_graph(s, p, o, graph)` pattern-matches triples within a named graph; `triple_count_in_graph(graph_iri)` returns the count for a specific graph
- **Inspect the dictionary** — `decode_id_full(id)` returns structured JSONB with kind, value, datatype, and language; `lookup_iri(iri)` checks whether an IRI exists without encoding it
- **Run with Docker Compose** — `docker compose up` starts PostgreSQL with pg_ripple and the HTTP endpoint in separate containers

### What happens behind the scenes

The HTTP service is a standalone Rust binary built with axum and tokio. It connects to PostgreSQL via deadpool-postgres, translates HTTP requests into calls to `pg_ripple.sparql()`, `sparql_ask()`, `sparql_construct()`, `sparql_describe()`, and `sparql_update()`, then formats the results according to the requested content type. The Prometheus `/metrics` endpoint exposes query count, error count, and total query duration.

Graph-aware loaders encode the `graph_iri` argument via the dictionary and delegate to the existing internal `*_into_graph(data, g_id)` functions. File variants read via `pg_read_file()` (superuser-only). `clear_graph` wraps `storage::clear_graph_by_id()` which deletes from delta tables and adds tombstones for main table rows.

<details>
<summary>Technical details</summary>

- **pg_ripple_http/src/main.rs** — axum router with `/sparql` (GET+POST), `/health`, `/metrics`; content negotiation; bearer/basic auth; CORS via tower-http
- **pg_ripple_http/src/metrics.rs** — atomic counter-based Prometheus metrics
- **src/lib.rs** — new `#[pg_extern]` functions: `load_ntriples_into_graph`, `load_turtle_into_graph`, `load_rdfxml_into_graph`, `load_ntriples_file_into_graph`, `load_turtle_file_into_graph`, `load_rdfxml_file_into_graph`, `load_rdfxml_file`, `delete_triple_from_graph`, `clear_graph`, `find_triples_in_graph`, `triple_count_in_graph`, `decode_id_full`, `lookup_iri`
- **src/bulk_load.rs** — `load_rdfxml_file`, `load_ntriples_file_into_graph`, `load_turtle_file_into_graph`, `load_rdfxml_file_into_graph`
- **src/storage/mod.rs** — `triple_count_in_graph(g_id)` scans all VP tables for a specific graph
- **sql/pg_ripple--0.14.0--0.15.0.sql** — migration script (no schema changes; all new features are compiled functions)
- **docker-compose.yml** — two-service Compose with postgres and sparql containers
- **Dockerfile** — updated to build and bundle `pg_ripple_http` binary
- **tests/pg_regress/sql/** — `load_into_graph.sql`, `graph_delete.sql`, `sql_api_completeness.sql`, `sparql_protocol.sql`

</details>

---

## [0.14.0] — 2025-07-18 — Administrative & Operational Readiness

This release focuses on production operations: maintenance commands, monitoring, graph-level access control, and comprehensive documentation. Everything a system administrator needs to run pg_ripple confidently in production.

**New in this release:** Maintenance functions (`vacuum`, `reindex`, `vacuum_dictionary`). Dictionary diagnostics (`dictionary_stats`). Graph-level Row-Level Security with `enable_graph_rls`, `grant_graph`, `revoke_graph`, `list_graph_access`. Optional pg_trickle integration via `schema_summary` / `enable_schema_summary`. Complete documentation for backup/restore, contributing, error codes (PT001–PT799), and security hardening. Extension upgrade scripts for the full `0.1.0 → 0.14.0` chain.

### What you can do

- **Maintain the store** — `pg_ripple.vacuum()` runs `MERGE` then `ANALYZE` on all VP tables; `pg_ripple.reindex()` rebuilds all indices; `pg_ripple.vacuum_dictionary()` removes orphaned dictionary entries after bulk deletes (uses advisory lock to be safe)
- **Diagnose the dictionary** — `pg_ripple.dictionary_stats()` returns a JSON object with `total_entries`, `hot_entries`, `cache_capacity`, `cache_budget_mb`, and `shmem_ready`
- **Control graph access** — `pg_ripple.enable_graph_rls()` activates RLS policies on VP tables keyed on the `g` (graph ID) column; `grant_graph(role, graph, permission)` / `revoke_graph(role, graph)` manage the `_pg_ripple.graph_access` mapping table; `list_graph_access()` returns the current ACL as JSON
- **Bypass RLS for admin work** — `SET pg_ripple.rls_bypass = on` in a superuser session skips RLS checks; protected by `GUC_SUSET` (superuser-only)
- **Inspect schema** — `pg_ripple.schema_summary()` returns the inferred class→property→cardinality summary (populated by the optional pg_trickle integration); `enable_schema_summary()` sets up the `_pg_ripple.inferred_schema` table and stream when pg_trickle is installed
- **Upgrade safely** — tested upgrade path from every prior version; `ALTER EXTENSION pg_ripple UPDATE` works for all transitions up to 0.14.0

### What happens behind the scenes

`vacuum()` and `reindex()` discover live VP tables by querying `pg_class` for tables matching the `vp_%` pattern in `_pg_ripple`. `vacuum_dictionary()` acquires advisory lock `0x7269706c` (`ripl`) then deletes from `_pg_ripple.dictionary` any row whose encoded ID does not appear in any VP table — safe to run concurrently with queries.

RLS policies are created on `_pg_ripple.vp_rare` (the catch-all VP table) using `current_setting('pg_ripple.rls_bypass', true)` as the bypass expression. The `graph_access` mapping table stores `(role_name, graph_id, permission)` triples; `grant_graph` encodes the graph IRI using `encode_term` before inserting.

<details>
<summary>Technical details</summary>

- **src/lib.rs** — new `pg_extern` functions: `vacuum()`, `reindex()`, `vacuum_dictionary()`, `dictionary_stats()`, `enable_graph_rls()`, `grant_graph()`, `revoke_graph()`, `list_graph_access()`, `schema_summary()`, `enable_schema_summary()`; new GUC `pg_ripple.rls_bypass` (bool, `GUC_SUSET`)
- **sql/pg_ripple--0.13.0--0.14.0.sql** — creates `_pg_ripple.graph_access` and `_pg_ripple.inferred_schema` tables with appropriate indices
- **tests/pg_regress/sql/admin_functions.sql** — tests vacuum, reindex, vacuum_dictionary, dictionary_stats, predicate_stats view
- **tests/pg_regress/sql/graph_rls.sql** — tests grant_graph, list_graph_access, revoke_graph, enable_graph_rls, rls_bypass GUC
- **tests/pg_regress/sql/upgrade_path.sql** — verifies full administrative API is available after a clean install
- **docs/src/user-guide/backup-restore.md** — pg_dump/pg_restore, VP table considerations, PITR, logical replication
- **docs/src/user-guide/contributing.md** — dev setup, test commands, PR workflow, code conventions
- **docs/src/reference/error-reference.md** — PT001–PT799 error code table
- **docs/src/reference/security.md** — supported versions matrix, RLS section, hardening GUCs
- **docs/src/user-guide/sql-reference/admin.md** — expanded with all new v0.14.0 admin functions

</details>

---

## [0.13.0] — 2026-04-16 — Performance Hardening

This release is about speed. Using the benchmarks established in earlier versions, pg_ripple v0.13.0 measures and improves performance at every layer: how triple patterns are ordered before query execution, how the PostgreSQL planner understands the data distribution, how parallel workers are exploited for multi-predicate queries, and how data quality rules from SHACL can help the optimizer make better decisions.

**New in this release:** BGP join reordering based on real table statistics. SPARQL plan cache instrumentation. Parallel query hints for star patterns. Extended statistics on VP table column pairs. SHACL-driven query optimizer hints. New GUCs to control reordering and parallelism thresholds. Regression and fuzz-integration test suites for the query pipeline.

### What you can do

- **Faster repeated queries** — the plan cache now tracks hits and misses; call `plan_cache_stats()` to see your hit rate and tune `pg_ripple.plan_cache_size` for your workload; call `plan_cache_reset()` to evict stale plans
- **Faster star patterns** — pg_ripple now reorders triple patterns within a BGP by estimated selectivity (most restrictive first), matching what a manual SQL expert would write; controlled by `SET pg_ripple.bgp_reorder = on/off`
- **Parallel query** — queries joining 3 or more VP tables now emit `SET LOCAL max_parallel_workers_per_gather = 4` and `SET LOCAL enable_parallel_hash = on` so PostgreSQL can use parallel workers; threshold tunable via `pg_ripple.parallel_query_min_joins`
- **Better planner statistics** — extended statistics on `(s, o)` column pairs are automatically created when a predicate is promoted from `vp_rare` to a dedicated VP table; this helps the PostgreSQL planner estimate join cardinalities for multi-predicate queries
- **SHACL-informed optimizer** — if you have loaded SHACL shapes with `sh:maxCount 1` or `sh:minCount 1`, the optimizer reads those hints and can use them for join costing; hints are only applied when semantics are preserved
- **Safer query pipeline** — a fuzz integration test suite verifies that malformed SPARQL, SQL injection attempts in IRI values, Unicode IRIs, deeply nested property paths, and very large literals are all handled gracefully without crashes or data corruption

### What happens behind the scenes

The BGP reordering optimizer queries `pg_class.reltuples` and `pg_stats.n_distinct` for each VP table at translation time to estimate how many rows a pattern will produce given its bound columns. Patterns are sorted cheapest-first using a greedy left-deep algorithm. Before executing the generated SQL, `SET LOCAL join_collapse_limit = 1` is emitted so the PostgreSQL planner does not reorder the joins back. On macOS/Linux, `SET LOCAL enable_mergejoin = on` is also set to exploit merge-join when join columns are ordered.

For parallel execution, the query engine counts VP-table aliases (`_t0`, `_t1`, …) in the generated SQL; if the count reaches `parallel_query_min_joins`, parallel hash join settings are activated before query execution.

Extended statistics (`CREATE STATISTICS … (ndistinct, dependencies) ON s, o`) are created in `_pg_ripple` schema alongside the VP tables when `promote_predicate()` runs. This gives the planner correlation data that single-column `ANALYZE` cannot provide.

<details>
<summary>Technical details</summary>

- **src/sparql/optimizer.rs** (new) — `reorder_bgp()`: greedy left-deep selectivity-based reorder; `TableStats` struct with `pg_class.reltuples` + `pg_stats.n_distinct` queries; `load_predicate_hints()`: reads SHACL shapes for `sh:maxCount`/`sh:minCount` hints
- **src/sparql/plan_cache.rs** — added `HIT_COUNT` and `MISS_COUNT` `AtomicU64` counters; `stats()` returns `(hits, misses, size, cap)`; `reset()` evicts cache and clears counters; cache key now includes `bgp_reorder` GUC value
- **src/sparql/sqlgen.rs** — `translate_bgp()` now calls `optimizer::reorder_bgp()` before building the join tree
- **src/sparql/mod.rs** — `execute_select()` emits `SET LOCAL join_collapse_limit = 1`, `enable_mergejoin = on`, and parallel hints when applicable; new public `plan_cache_stats()` and `plan_cache_reset()` functions
- **src/storage/mod.rs** — `promote_rare_predicates()` calls `create_extended_statistics()` for each newly promoted predicate; `create_extended_statistics()` issues `CREATE STATISTICS IF NOT EXISTS … (ndistinct, dependencies) ON s, o`
- **src/lib.rs** — two new GUCs: `pg_ripple.bgp_reorder` (bool, default on), `pg_ripple.parallel_query_min_joins` (int, default 3); two new `pg_extern` functions: `plan_cache_stats() RETURNS JSONB`, `plan_cache_reset() RETURNS VOID`
- **sql/pg_ripple--0.12.0--0.13.0.sql** — migration script (no schema DDL; new functions are compiled into the extension library)
- **tests/pg_regress/sql/shacl_query_opt.sql** — verifies BGP reorder GUC, plan cache stats/reset, SHACL shape reading, and sparql_explain output
- **tests/pg_regress/sql/fuzz_integration.sql** — verifies graceful handling of empty queries, malformed SPARQL, SQL injection via IRI, Unicode IRIs, large literals, deeply nested property paths, and adversarial cache usage

</details>

---

## [0.12.0] — 2026-04-16 — SPARQL Update (Advanced)

This release completes the full SPARQL 1.1 Update specification. Building on the `INSERT DATA` / `DELETE DATA` support from v0.5.1, pg_ripple now supports pattern-based updates, remote RDF loading, and full named-graph lifecycle management.

**New in this release:** Find-and-replace data using SPARQL patterns with `DELETE/INSERT WHERE`. Fetch and load remote RDF documents from any HTTP(S) URL with `LOAD <url>`. Clear, drop, or create named graphs with a single SPARQL Update call.

### What you can do

- **Pattern-based updates** — `DELETE { … } INSERT { … } WHERE { … }` finds matching triples using the full SPARQL→SQL engine and then deletes and inserts triples for each result row; both the DELETE and INSERT templates may reference WHERE-bound variables
- **INSERT WHERE** — omit the DELETE clause to insert a triple for every WHERE match
- **DELETE WHERE** — omit the INSERT clause to remove all triples matching a pattern
- **LOAD remote RDF** — `LOAD <url>` fetches a Turtle, N-Triples, or RDF/XML document via HTTP(S) and inserts all triples; `LOAD <url> INTO GRAPH <g>` targets a named graph; `LOAD SILENT <url>` suppresses network errors
- **Clear a graph** — `CLEAR GRAPH <g>` removes all triples from a named graph without touching the default graph; `CLEAR DEFAULT`, `CLEAR NAMED`, `CLEAR ALL` let you clear one or all graphs in a single call
- **Drop a graph** — `DROP GRAPH <g>` clears and deregisters a graph; `DROP SILENT` suppresses errors on non-existent graphs; `DROP ALL` clears the entire store
- **Create a graph** — `CREATE GRAPH <g>` pre-registers a named graph in the dictionary; `CREATE SILENT` is a no-op if the graph already exists

### What happens behind the scenes

When `DELETE/INSERT WHERE` runs, the WHERE clause is compiled through the existing SPARQL→SQL engine into a SELECT query. The result rows are collected in memory, and then for each row the DELETE phase removes any matched triples from VP storage, followed by the INSERT phase adding new ones. This keeps the operation transactional inside a single PostgreSQL call.

`LOAD` uses `ureq` (a lightweight Rust HTTP client) to fetch the URL. The response body is parsed by the same rio_turtle / rio_xml parsers used for local bulk loading; triples are inserted in batches using the standard VP storage path.

`CLEAR` and `DROP` call a new `clear_graph_by_id()` helper that deletes from both the HTAP delta tables and tombstones the main-partition rows — the same mechanism used by the existing `drop_graph()` function.

<details>
<summary>Technical details</summary>

- **src/sparql/mod.rs** — `sparql_update()` extended to handle all `GraphUpdateOperation` variants: `DeleteInsert`, `Load`, `Clear`, `Create`, `Drop`; new helpers `execute_delete_insert()`, `execute_load()`, `execute_clear()`, `execute_drop()`, `resolve_ground_term()`, `resolve_term_pattern()`, `resolve_named_node_pattern()`, `resolve_graph_name_pattern()`, `encode_literal_id()`
- **src/storage/mod.rs** — new `clear_graph_by_id(g_id)` mirrors `drop_graph()` but takes a pre-encoded ID; new `all_graph_ids()` collects all distinct graph IDs across VP tables and `vp_rare`
- **src/bulk_load.rs** — new graph-aware loaders `load_ntriples_into_graph()`, `load_turtle_into_graph()`, `load_rdfxml_into_graph()` accept a target `g_id` instead of always writing to the default graph (g=0)
- **Cargo.toml** — added `ureq = { version = "2", features = ["tls"] }` for `LOAD <url>` HTTP support
- **sql/pg_ripple--0.11.0--0.12.0.sql** — migration script (schema unchanged; new capabilities compiled into the extension library)
- **pg_regress** — new test suites: `sparql_update_where.sql`, `sparql_graph_management.sql`; both PASS

</details>

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
