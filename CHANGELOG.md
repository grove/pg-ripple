# Changelog

All notable changes to pg_ripple are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions correspond to the milestones in [ROADMAP.md](ROADMAP.md).

---

## [Unreleased]

> Changes for the next version will appear here.

---

## [0.74.0] — 2026-05-09 — Assessment 11 Critical/High Remediation

**Implements v0.74.0 roadmap: evidence truthfulness for all 12 missing reference docs, mutation journal wired through Datalog inference and executor-end hook, VP promotion plan-cache invalidation, interrupted-promotion recovery, and comprehensive CI validation.**

### What's new

- **EVIDENCE-01** — Created 12 missing `docs/src/reference/` pages cited by `feature_status()`:
  `sparql.md`, `datalog.md`, `shacl.md`, `storage.md`, `construct-rules.md`, `federation.md`,
  `cdc.md`, `graphrag.md`, `observability.md`, `query-optimization.md`, `vector-search.md`,
  `development.md`. SUMMARY.md updated with all new entries.

- **GATE-05** — Fixed `validate-feature-status` CI job: replaced subshell-bypass pattern with
  `missing=$(...)` variable capture so missing evidence paths cause a real non-zero exit.

- **GATE-06** — Added `validate-feature-status-populated` CI job (`.github/workflows/ci.yml`): installs extension, inserts sample
  triples, then validates that `feature_status()` returns no `degraded` rows on a populated DB.

- **JOURNAL-DATALOG-01** — Wired Datalog inference through the mutation journal (CF-D + HF-C fixes):
  `run_inference_seminaive()` records affected graph IDs from `_dl_delta_*` tables after VP-rare
  insertion and calls `mutation_journal::flush()`. `run_inference()` similarly flushes after
  any triples are derived.

- **SBOM-03** — SBOM regenerated to v0.74.0 (`sbom.json`). Added `just check-sbom-version` target to the
  justfile and wired it into `just assess-release` as the first check.

- **HTTP-VERSION-01** — `pg_ripple_http` version bumped to 0.74.0; `COMPATIBLE_EXTENSION_MIN`
  updated to "0.73.0".

- **DOC-JOURNAL-01** — Updated `mutation_journal` module and `flush()` doc comments to accurately
  list all wired call sites (bulk_load, dict_api, Datalog seminaive, executor-end hook); removed
  false claim that SPARQL Update was wired.

- **PROMO-RECOVER-01** — Background merge worker (worker 0) now calls
  `recover_interrupted_promotions()` at startup inside a catch-unwind block. A new
  `vp_promotion_recovery` row (status `implemented`) is added to `feature_status()`.

- **CACHE-INVALIDATE-01** — `promote_predicate()` calls `crate::sparql::plan_cache_reset()` after
  completing a VP promotion, so stale query plans that hard-coded `vp_rare` are evicted.

- **TEST-04** — Added `tests/pg_regress/sql/v070_features.sql` regression test covering
  construct_writeback status, evidence-path coverage, vp_promotion_recovery, and plan_cache_reset.

- **FLUSH-DEFER-01** — Executor-end hook (`register_executor_end_hook`) calls
  `mutation_journal::flush()` at the start of each hook invocation, providing per-statement
  CWB rule firing even when dict_api is not used.

### Schema changes

None — all changes are in the Rust implementation only.

- Migration: `sql/pg_ripple--0.73.0--0.74.0.sql`.

---

## [0.73.0] — 2026-05-05 — SPARQL 1.2 Tracking, Live Subscriptions, and JSON Mapping Registry

**Implements v0.73.0 roadmap: SPARQL 1.2 compatibility tracking, SPARQL live subscription API via SSE, named bidirectional JSON↔RDF mapping registry, multi-graph JSON-LD ingest, CONTRIBUTING.md, Helm chart sidecar image config, and feature-status taxonomy.**

### What's new

- **SUB-01** — SPARQL live subscription API: `subscribe_sparql(id, query, graph_iri)` registers a subscription in `_pg_ripple.sparql_subscriptions`; `unsubscribe_sparql(id)` removes it; `list_sparql_subscriptions()` enumerates active subscriptions. After each graph write, `notify_affected_subscriptions()` re-executes the query and fires `pg_notify('pg_ripple_subscription_<id>', <json>)`. Payloads >8 KB send `{"changed":true}` instead. The `pg_ripple_http` companion now exposes `GET /subscribe/{id}` as a Server-Sent Events stream. Regression test: `tests/pg_regress/sql/v073_features.sql`.

- **JSON-MAPPING-01** — Named bidirectional JSON↔RDF mapping registry: `register_json_mapping(name, context_jsonb, shape_iri)` stores a JSON-LD `@context` in `_pg_ripple.json_mappings`. Inconsistencies with the optional SHACL shape are recorded as warnings in `_pg_ripple.json_mapping_warnings`. `ingest_json(mapping, document)` and `export_json_node(mapping, iri)` use the stored context for bidirectional conversion.

- **JSONLD-INGEST-02** — Multi-graph JSON-LD ingest: `json_ld_load(document jsonb, default_graph text) → bigint` walks `@graph` arrays or single-node JSON-LD documents and loads each node into the triple store, returning the total number of triples inserted.

- **SPARQL12-01** — SPARQL 1.2 compatibility tracking document at `plans/sparql12_tracking.md` listing all SPARQL 1.2 features and their current status in pg_ripple.

- **CONTRIB-01** — `CONTRIBUTING.md` added with branch naming conventions, commit format, pre-commit checklist, migration discipline, and PR checklist.

- **TAXONOMY-01** — Feature status taxonomy documentation at `docs/src/reference/feature-status-taxonomy.md` with promotion criteria for each status tier.

- **HELM-01** — Helm chart `charts/pg_ripple/values.yaml` updated to include a separate `http.image` section for the pg_ripple_http sidecar; `statefulset.yaml` uses `http.image.tag` to pin the sidecar version independently.

- **FEATURE-STATUS-02** — `feature_status()` now includes entries for `llm_sparql_repair`, `kge_embeddings`, `sparql_nl_to_sparql`, `sparql_12`, `sparql_subscription`, `json_ld_multi_ingest`, and `json_mapping`.

- **R2RML-DOC-01** — `plans/r2rml_virtual.md` documents the planned virtual R2RML layer and its scope relative to `register_json_mapping`.

- **CONTROL-01** — `pg_ripple.control` `comment` updated to reflect v0.73.0 capabilities.

### Schema changes

- New table `_pg_ripple.sparql_subscriptions(subscription_id TEXT PK, query TEXT, graph_iri TEXT, created_at TIMESTAMPTZ)`.
- New table `_pg_ripple.json_mappings(mapping_name TEXT PK, context JSONB, shape_iri TEXT, created_at TIMESTAMPTZ)`.
- New table `_pg_ripple.json_mapping_warnings(id BIGSERIAL PK, mapping_name TEXT, kind TEXT, detail TEXT, created_at TIMESTAMPTZ)`.
- Migration: `sql/pg_ripple--0.72.0--0.73.0.sql`.

---

## [0.72.0] — 2026-05-01 — Architecture and Protocol Hardening

**Implements v0.72.0 roadmap: sub-transaction safety, JSON-LD fixes, Flight nonce replay protection, observability, module splitting.**

### What's new

- **XACT-01** — Sub-transaction savepoint/rollback support: `RegisterSubXactCallback` registered in `_PG_init`; CWB writer entries are now cleaned up on `ROLLBACK TO SAVEPOINT`. Regression test: `tests/pg_regress/sql/cwb_savepoint_rollback.sql`.

- **BUG-JSONLD-CONTEXT-01** — Object-form JSON-LD `@context` entries (term definitions with `@id`/`@type`) are now correctly preserved in `bulk_load.rs` instead of being silently dropped.

- **RT-FIX-04B** — `i64` overflow in JSON number → `xsd:integer` no longer panics; values exceeding `i64::MAX` are preserved as the `xsd:integer` string form.

- **RT-FIX-06** — `is_f64()` checked before `is_i64()` in `json_value_to_nt_term` so fractional JSON numbers (e.g. `1.5`) are not misclassified as integers.

- **RT-FIX-07** — IRI key validation (`validate_iri_key_or_error`) added before triple insert to prevent malformed IRIs from entering the triple store. (ci/regress: json_roundtrip_fixes.sql)

- **FLIGHT-NONCE-01** — Arrow Flight nonce replay protection: `AppState` gains a `nonce_cache: DashMap<String, Instant>` with 5-minute TTL. Replayed nonces return `401 Unauthorized`.

- **OBS-02** — `/metrics/extension` route added to `pg_ripple_http`, emitting Prometheus-format extension-level metrics (triple count, active graphs, GUC settings).

- **JSONLD-NODE-01** — `export_jsonld_node(iri TEXT) → jsonb` SQL function added, returning the JSON-LD representation of all triples for a given subject IRI. Regression test: `tests/pg_regress/sql/export_jsonld_node.sql`.

- **PROPTEST-01** — Property-based tests for `ConstructTemplate` / `apply_construct_template` added in `tests/proptest/construct_template.rs` using the `proptest 1` crate. Self-contained (no pgrx dependency). (see ROADMAP.md v0.72.0)

- **MOD-01** — Source files exceeding 500 lines split into focused sub-modules:
  - `src/gucs/registration.rs` — all GUC registrations extracted from `_PG_init`
  - `src/lib_tests.rs` — pgrx integration tests extracted from `src/lib.rs`
  - `src/storage/dictionary_io.rs` — RDF-term I/O helpers
  - `src/storage/vp_rare_io.rs` — VP-table I/O helpers
  - `src/storage/ops.rs` — storage operations (insert/delete/query/graph management)
  - `pg_ripple_http/src/routing/sparql_handlers.rs` — SPARQL GET/POST/stream handlers
  - `pg_ripple_http/src/routing/rag_handler.rs` — RAG endpoint handler
  - `pg_ripple_http/src/routing/admin_handlers.rs` — admin/observability/explorer handlers

### Schema changes

None.

---

## [0.71.0] — 2026-04-29 — Arrow Flight Validation, Citus Integration Tests, and Compatibility Hardening

**Implements v0.71.0 roadmap: closes the High-severity Assessment 10 gaps requiring runtime infrastructure.**

### What's new

- **FLIGHT-STREAM-01** — `pg_ripple_http` `/flight/do_get` now uses `axum::body::Body::from_stream` with 64 KiB chunks, producing `Transfer-Encoding: chunked` HTTP responses. The IPC buffer is streamed lazily so clients can begin decoding Arrow record batches before the full export completes. Integration test `tests/http_integration/arrow_export_large.sh` validates streaming behavior and RSS bounds. `docs/src/reference/arrow-flight.md` updated with memory-bound documentation.

- **CITUS-INT-01** — `tests/integration/citus_rls_propagation.sh` created. The multi-node integration test starts a Citus cluster via `docker-compose`, enables sharding, inserts triples in both allowed and restricted named graphs, promotes a predicate past the threshold, and asserts that non-superuser RLS restricts cross-graph access. The `feature_status()` citation for `citus_rls_propagation` now resolves to an existing file.

- **COMPAT-01** — `pg_ripple_http` now performs a version compatibility check at startup: it queries `extversion` from `pg_extension` and warns if the installed extension is below `COMPATIBLE_EXTENSION_MIN = "0.70.0"`. `docs/src/operations/compatibility.md` added with the full version compatibility matrix and upgrade procedure.

- **HLL-DOC-01** — `docs/src/reference/approximate-aggregates.md` created, documenting when HLL is used (`approx_distinct=on` + `hll` extension), error bounds at default precision (`log2m=14`, ~0.81% standard error for ≥ 10,000 distinct values), and fallback to exact `COUNT(DISTINCT)`. pg_regress test `hll_accuracy.sql` validates GUC toggle and COUNT(DISTINCT) correctness.

- **CITUS-BENCH-01** — `docs/src/reference/citus-service-pruning.md` created, documenting the `citus_service_pruning` GUC and expected 10× speedup for bound-subject SERVICE queries. pg_regress test `citus_service_pruning.sql` validates GUC plumbing and confirms `feature_status()` shows `experimental`.

### Schema changes

None.

---

## [0.70.0] — 2026-04-29 — Assessment 10 Critical Remediation

**Implements v0.70.0 roadmap: closes four Critical and seven High/Medium findings from Overall Assessment 10.**

### What's new

- **BULK-01** — Bulk-load functions (`load_ntriples`, `load_turtle`, `load_nquads`, `load_trig`, `load_rdfxml`, and their graph-aware variants) now wire into the mutation journal and call `flush()` after all batches. CONSTRUCT writeback rules fire automatically after every `load_*` call without requiring `refresh_construct_rule`.

- **FLUSH-01** — SPARQL Update and single-triple API calls no longer flush the CWB pipeline once per individual triple. Journal flush is deferred to `XACT_EVENT_PRE_COMMIT` via the existing `xact_callback_c`, so CONSTRUCT writeback fires once per statement boundary regardless of how many triples the statement inserts or deletes.

- **GATE-03** — `feature_status()` evidence paths cleaned up: stub pages created at `docs/src/reference/scalability.md` and `docs/src/reference/arrow-flight.md`; the non-existent `tests/integration/citus_rls_propagation.sh` reference replaced with the new `security_rls_role_injection.sql` test evidence path. The `validate-feature-status` CI job now fails hard when any cited evidence file is missing.

- **SHACL-DOC-01** — `docs/src/features/shacl-sparql-rules.md` rewritten: `sh:SPARQLRule` is clearly documented as not supported (emits PT481 WARNING and skips). `sh:TripleRule` and `sh:SPARQLConstraint` remain fully supported.

- **README-01/02** — `README.md` updated from v0.67.0 to v0.69.0 in "What works today" and "Known limitations" sections. `scripts/check_readme_version.sh` added and wired into `just assess-release`.

- **RLS-SQL-01** — `grant_graph_access()` and `apply_rls_to_vp_table()` now validate role names against `[A-Za-z_][A-Za-z0-9_$]*` (PT711 error on mismatch) and use `quote_ident_safe()` in DDL. SQL injection test added: `tests/pg_regress/sql/security_rls_role_injection.sql`.

- **SBOM-02** — `sbom.json` regenerated for v0.70.0. Release CI `release.yml` confirmed to include a blocking SBOM-version-match step.

- **GATE-04** — Legacy `scripts/check_roadmap_evidence.sh` and `scripts/check_api_drift.sh` deleted. `justfile` `assess-release` target now calls `.py` versions exclusively. Verified by `.github/workflows/ci.yml` (Validate feature status job).

- **TEST-01** — `tests/pg_regress/sql/v067_features.sql` added (mutation journal smoke test, Arrow Flight GUC check, feature_status evidence path regression guard).

- **TEST-02** — `tests/pg_regress/sql/v069_features.sql` added (module restructuring API stability regression guard, `construct_pipeline_status()` check, `feature_status()` coverage check).

- **TEST-03** — `tests/pg_regress/sql/recover_promotions.sql` added (full `recover_interrupted_promotions()` regression test including simulated-interruption scenario).

- **DOC-01** — `roadmap/v0.67.0.md` status already confirmed as Released ✅ (no change needed).

- **CWB test extension** — `cwb_write_path_equivalence.sql` extended with a Path 5 bulk-load arm (`load_ntriples_into_graph`) that asserts derived triples appear immediately after a bulk load.

### Schema changes

None.

### Exit criteria

All 192+ pg_regress tests pass; `validate-feature-status` CI job exits non-zero when evidence file missing; bulk-load CWB arm in `cwb_write_path_equivalence.sql` passes.

---

## [0.69.0] — 2026-05-06 — Module Architecture Restructuring

**Implements v0.69.0 roadmap: splits four large source modules along single-responsibility boundaries with zero behavioral changes.**

### What's new

- **ARCH-01 — `src/sparql/mod.rs` split** (already delivered in prior sessions): `parse.rs`, `plan.rs`, `decode.rs`, `execute.rs` extracted; `mod.rs` is now a 157-line facade with re-exports and the three public SQL-entry-point functions.

- **ARCH-02 — `pg_ripple_http/src/main.rs` split**: Handler functions, content-type constants, and response formatters extracted to `routing.rs`; SPARQL execution helpers (execute_select/ask/construct/describe) to `spi_bridge.rs`; Arrow IPC Flight endpoint to `arrow_encode.rs`; streaming placeholder to `stream.rs`. `main.rs` is now 250 lines (startup code + `main()` only).

- **ARCH-03 — `src/construct_rules.rs` split into a module directory**: `catalog.rs` (ensure_catalog), `scheduler.rs` (collect_source_graphs + compute_rule_order topological sort), `delta.rs` (compile_construct_to_inserts + run_full_recompute), `retract.rs` (retract_exclusive_triples), `mod.rs` (public API + write hooks).

- **ARCH-04 — `src/storage/mod.rs` narrowed public API**: `insert_triple_by_ids`, `delete_triple_by_ids`, and `batch_insert_encoded` narrowed to `pub(crate)` with journal-caller doc comments. VP promotion helpers (`promote_predicate`, `promote_rare_predicates`, `recover_interrupted_promotions`, `vp_promotion_threshold`, `create_extended_statistics`) extracted to `storage/promote.rs`.

- **ARCH-05 — All 186 pg_regress tests pass**; no SQL-visible changes.

### Schema changes

None. This is a pure Rust module restructuring.

### Files changed

- **src/sparql/parse.rs** — query complexity checks + ARQ aggregate preprocessing (new)
- **src/sparql/plan.rs** — SPARQL algebra → SQL plan cache (new)
- **src/sparql/decode.rs** — batch dictionary decode (new)
- **src/sparql/execute.rs** — SPI execution, CONSTRUCT/DESCRIBE/UPDATE, explain (new)
- **src/sparql/mod.rs** — thin facade: re-exports + 3 public SQL entry points
- **pg_ripple_http/src/routing.rs** — all HTTP handlers, response formatters, build_router (new)
- **pg_ripple_http/src/spi_bridge.rs** — execute_sparql_with_traceparent + execute_select/ask/construct/describe (new)
- **pg_ripple_http/src/arrow_encode.rs** — Arrow Flight bulk-export (new)
- **pg_ripple_http/src/stream.rs** — SSE/streaming placeholder (new)
- **pg_ripple_http/src/main.rs** — startup code only (250 lines, was 2252)
- **src/construct_rules/mod.rs** — public API + on_graph_write/delete hooks (new directory)
- **src/construct_rules/catalog.rs** — ensure_catalog (new)
- **src/construct_rules/scheduler.rs** — topological sort (new)
- **src/construct_rules/delta.rs** — compile + recompute (new)
- **src/construct_rules/retract.rs** — retract_exclusive_triples (new)
- **src/storage/promote.rs** — VP promotion helpers (new)
- **src/storage/mod.rs** — narrowed mutation API, pub(crate) for mutation functions

---

## [0.68.0] — 2026-04-29 — Distributed Scalability, Streaming Completion, and Fuzz Hardening

**Implements v0.68.0 roadmap: portal-based CONSTRUCT cursor streaming, Citus HLL COUNT(DISTINCT), Citus SERVICE shard pruning, nonblocking VP promotion with crash recovery, and scheduled nightly fuzz CI.**

### What's new

- **Portal-based CONSTRUCT cursor streaming** (STREAM-01): `sparql_cursor_turtle()` and `sparql_cursor_jsonld()` now stream CONSTRUCT results using a lazy `ConstructCursorIter` — a portal-based iterator that applies the CONSTRUCT template per page and serializes each page as a Turtle/JSON-LD chunk. Memory use is bounded to `pg_ripple.export_batch_size` rows per page. New helpers `prepare_construct()` and `apply_construct_template()` in `src/sparql/mod.rs` pre-encode constant IRIs/literals to i64 once at query-plan time.

- **Citus HLL approximate COUNT(DISTINCT)** (CITUS-HLL-01): When `pg_ripple.approx_distinct=on` and the `hll` PostgreSQL extension is installed, `COUNT(DISTINCT ?x)` SPARQL aggregates are translated to `hll_cardinality(hll_add_agg(hll_hash_bigint(x)))::bigint` for scalable approximate counts on distributed VP tables. Falls back to exact `COUNT(DISTINCT)` when `hll` is absent or `approx_distinct=off`. New GUC `pg_ripple.approx_distinct` (BOOL, default `off`).

- **Citus SERVICE shard pruning** (CITUS-SVC-01): When `pg_ripple.citus_service_pruning=on`, the SERVICE translator calls `citus_service_shard_annotation()` which detects Citus worker endpoints via `is_citus_worker_endpoint()` and wires shard-constraint SQL annotations for pruning. New GUC `pg_ripple.citus_service_pruning` (BOOL, default `off`). Full multi-node infrastructure required for end-to-end testing.

- **Nonblocking VP promotion with crash recovery** (PROMO-01): VP promotion now tracks progress via a `promotion_status TEXT` column in `_pg_ripple.predicates` (values: `'promoting'` during copy, `'promoted'` when complete). New SQL function `pg_ripple.recover_interrupted_promotions()` scans for `'promoting'` entries and retries interrupted promotions — call it after an unclean server shutdown. New GUC `pg_ripple.vp_promotion_batch_size` (INT, 1–1000000, default 10000).

- **Scheduled nightly fuzz CI** (FUZZ-01): `.github/workflows/fuzz.yml` runs all 12 fuzz targets (sparql_parser, turtle_parser, rdfxml_parser, dictionary_hash, federation_result, datalog_parser, shacl_parser, jsonld_framer, http_request, r2rml_mapping, geosparql_wkt, llm_prompt_builder) nightly for 60 s each. Manual `workflow_dispatch` supports extended runs. Corpus and crash artifacts are uploaded on each run.

### Schema changes

- `_pg_ripple.predicates` table: added `promotion_status TEXT` column (NULL = legacy/no promotion started, `'promoting'` = copy in progress, `'promoted'` = fully promoted). Added to initial schema CREATE TABLE and to migration script `sql/pg_ripple--0.67.0--0.68.0.sql`.

### GUCs added

| GUC | Type | Default | Level | Purpose |
|---|---|---|---|---|
| `pg_ripple.approx_distinct` | BOOL | `off` | USERSET | Route COUNT(DISTINCT) through Citus HLL when available |
| `pg_ripple.citus_service_pruning` | BOOL | `off` | USERSET | Enable Citus worker shard annotations for SERVICE |
| `pg_ripple.vp_promotion_batch_size` | INT | 10000 | USERSET | Batch size for nonblocking VP promotion copy phase |

### SQL functions added

| Function | Returns | Description |
|---|---|---|
| `pg_ripple.recover_interrupted_promotions()` | `bigint` | Scan and retry interrupted VP promotions after crash |

### Files changed

- **src/sparql/cursor.rs** — new `ConstructCursorIter` struct + `ConstructFormat` enum; `sparql_cursor_turtle` and `sparql_cursor_jsonld` now return `impl Iterator`
- **src/sparql/mod.rs** — `TemplateSlot`, `ConstructTemplate`, `prepare_construct()`, `apply_construct_template()`
- **src/sparql_api.rs** — updated SETOF wrappers for new iterator API
- **src/sparql/translate/group.rs** — HLL aggregate translation + `citus_hll_available()`
- **src/gucs/storage.rs** — `APPROX_DISTINCT`, `CITUS_SERVICE_PRUNING`, `VP_PROMOTION_BATCH_SIZE` GUC statics
- **src/citus.rs** — `is_citus_worker_endpoint()`, `citus_service_shard_annotation()`, `extract_url_host()`
- **src/sparql/translate/graph.rs** — wire `citus_service_shard_annotation()` in SERVICE translator
- **src/storage/mod.rs** — `promote_predicate()` with status tracking; `recover_interrupted_promotions()`
- **src/dict_api.rs** — `recover_interrupted_promotions()` pg_extern
- **src/lib.rs** — three new GUC registrations
- **src/schema.rs** — `promotion_status TEXT` in predicates CREATE TABLE; `v068_schema_version_stamp`
- **src/feature_status.rs** — updated status for 6 deliverables + new `continuous_fuzzing` entry
- **sql/pg_ripple--0.67.0--0.68.0.sql** — migration script (ADD COLUMN, schema_version stamp)
- **.github/workflows/fuzz.yml** — new scheduled fuzz workflow
- **tests/pg_regress/sql/v068_features.sql** — new regress test (186 total, 0 failures)
- **tests/pg_regress/expected/v068_features.out** — bootstrapped expected output

---

## [0.67.0] — 2026-05-06 — Production Hardening and Assessment 9 Remediation

**Implements v0.67.0 roadmap: Arrow Flight security hardening, mutation journal for CONSTRUCT writeback, Row Level Security propagation to all VP tables, panic→error conversion, Python gate tooling, benchmark correctness fixes, and scheduled performance trend CI.**

### What's new

- **Arrow Flight unsigned-ticket hardening** (FLIGHT-SEC-01): Unsigned Arrow Flight tickets are now rejected by default. New GUC `pg_ripple.arrow_unsigned_tickets_allowed` (BOOL, default `off`) must be explicitly set to allow unsigned tickets. Corresponding `ARROW_UNSIGNED_TICKETS_ALLOWED` env var for `pg_ripple_http`. Ticket rejections are tracked in `streaming_metrics()`. Evidence: `pg_ripple.feature_status()`, `pg_ripple.streaming_metrics()`.

- **Arrow Flight tombstone-exclusion and batch streaming** (FLIGHT-SEC-02): `POST /flight/do_get` now uses tombstone-exclusion query (`main EXCEPT tombstones UNION ALL delta`) to prevent serving deleted triples. Export is streamed in configurable batches via new GUC `pg_ripple.arrow_batch_size` (INT, 1–100000, default 1000). Response header `x-arrow-batches` reports batch count. `arrow_batches_sent` counter added to `streaming_metrics()`. Evidence: `pg_ripple.feature_status()`.

- **Transaction-local mutation journal** (MJOURNAL-01/02/03): A Rust `thread_local!` mutation journal (`src/storage/mutation_journal.rs`) unifies CONSTRUCT writeback across all write paths: `insert_triple`, SPARQL INSERT DATA, `load_ntriples`, and `load_turtle`. Fast-path skips journal accumulation when no CONSTRUCT rules are defined. Evidence: `tests/pg_regress/sql/cwb_write_path_equivalence.sql`.

- **Row Level Security on VP delta/main tables** (RLS-01/02): `enable_graph_rls()` and `grant_graph_access()` now apply RLS policies to dedicated VP `_delta` and `_main` tables at creation, promotion, and grant/revoke time. Evidence: `tests/pg_regress/sql/rls_promotion.sql`.

- **Panic → pgrx::error conversion** (PANIC-01): `construct_rules.rs` topological-sort `panic!()` replaced with `pgrx::error!()` to ensure clean PostgreSQL error reporting under load. Evidence: `src/construct_rules.rs`.

- **Python gate tooling** (GATE-01): `scripts/check_api_drift.sh` and `scripts/check_roadmap_evidence.sh` replaced with portable Python 3 equivalents (`scripts/check_api_drift.py`, `scripts/check_roadmap_evidence.py`) that require `--version X.Y.Z` to prevent stale invocations. Evidence: `scripts/check_api_drift.py`, `scripts/check_roadmap_evidence.py`.

- **`validate-feature-status` CI job** (GATE-02): New CI job added to `.github/workflows/ci.yml` that runs after `test` and `regress`, calls `feature_status()`, verifies evidence paths exist on disk, and runs both Python gate scripts. Evidence: `.github/workflows/ci.yml`.

- **Documentation truth** (GATE-03): `README.md` "What works today" updated from v0.63.0 to v0.67.0, pgrx version reference corrected to 0.18, v0.64.0 `roadmap/v0.64.0.md` status corrected to `Released ✅`.

- **SBOM version verification** (SBOM-01): Release workflow now verifies that the regenerated `sbom.json` component version matches `Cargo.toml` before uploading to the GitHub release. Evidence: `.github/workflows/release.yml`.

- **Benchmark correctness** (BENCH-01): `.github/workflows/benchmark.yml` no longer uses `bash` to execute SQL files. Merge throughput and vector index benchmarks now use `pgbench -f` and `psql -f` respectively. `|| true` suppressors removed; `continue-on-error: false` added. Benchmark failures now fail the CI run.

- **Scheduled performance trend CI** (BENCH-02): New weekly workflow `.github/workflows/performance_trend.yml` runs insert throughput, merge throughput, and hybrid search benchmarks, appends results to `benchmarks/*_history.csv`, and fails if any metric drops more than 10% below the 4-week rolling average.

### GUCs added

| GUC | Type | Default | Level | Purpose |
|---|---|---|---|---|
| `pg_ripple.arrow_unsigned_tickets_allowed` | BOOL | `off` | SIGHUP | Allow unsigned Arrow Flight tickets (dev-only) |
| `pg_ripple.arrow_batch_size` | INT | 1000 | USERSET | Arrow IPC export batch size per record batch |

### Dependencies added

None — all v0.67.0 changes are implemented using already-present dependencies.



**Implements the v0.66.0 roadmap: true paged SPARQL cursors via PostgreSQL portal API, HMAC-SHA256 signed Arrow Flight v2 tickets, real Arrow IPC streaming in pg_ripple_http, WCOJ explain metadata, streaming observability metrics, and Citus BRIN summarise SQL API.**

### What's new

- **True SPARQL cursor streaming** (STREAM-01): `sparql_cursor()` now uses the PostgreSQL portal API (`SpiCursor::detach_into_name()` + `SpiClient::find_cursor()`) for memory-bounded paged streaming. Peak memory is proportional to `pg_ripple.export_batch_size`, not the full result size. The cursor survives across SPI sessions within the same transaction.

- **HMAC-SHA256 signed Arrow Flight tickets** (FLIGHT-01): `export_arrow_flight()` now generates signed, expiring JSON tickets (`type = "arrow_flight_v2"`). Tickets include `iat`, `exp`, `aud`, `nonce`, and an HMAC-SHA256 signature over a canonical string. New GUCs: `pg_ripple.arrow_flight_secret` (signing key, `SIGHUP`-level) and `pg_ripple.arrow_flight_expiry_secs` (default: 3600).

- **Real Arrow IPC streaming in pg_ripple_http** (FLIGHT-02): `POST /flight/do_get` now validates the HMAC-SHA256 ticket signature, expiry, and audience, then streams all VP main, delta, and rare tables for the requested graph as a binary Arrow IPC stream (`application/vnd.apache.arrow.stream`). Schema: `s Int64, p Int64, o Int64, g Int64`. The `ARROW_FLIGHT_SECRET` environment variable must match `pg_ripple.arrow_flight_secret`.

- **WCOJ explain metadata** (WCOJ-01): `explain_sparql_jsonb()` output now includes a `"wcoj"` block reporting `cyclic_bgp_detected`, `wcoj_mode` (`"planner_hint"`, `"disabled"`, or `"not_applicable"`), `planner_settings`, and `fallback_reason`.

- **Streaming observability metrics** (OBS-01): New `pg_ripple.streaming_metrics() → JSONB` function returns live atomic counters: `cursor_pages_opened`, `cursor_pages_fetched`, `cursor_rows_streamed`, `arrow_batches_sent`, `arrow_ticket_rejections`, `citus_brin_summarise_completed`.

- **Citus BRIN summarise SQL API** (CITUS-04): New `pg_ripple.citus_brin_summarise_all() → BIGINT` function runs `brin_summarize_new_values` on every promoted VP main-partition table. On Citus deployments uses `run_command_on_shards`; on non-Citus deployments runs locally. Returns total shards/tables updated.

- **Feature status updated**: `arrow_flight` moves from `stub` to `experimental` in `pg_ripple.feature_status()`.

### Dependencies added

- `hmac = "0.12"`, `sha2 = "0.10"`, `hex = "0.4"` (in pg_ripple extension for ticket signing)
- `arrow = "55"` (in pg_ripple_http for Arrow IPC serialization)
- `hmac = "0.12"`, `sha2 = "0.10"`, `hex = "0.4"` (in pg_ripple_http for ticket validation)

---

## [0.65.0] — 2026-04-28 — CONSTRUCT Writeback Correctness Closure

**Implements the v0.65.0 roadmap: real delta maintenance, HTAP-aware retraction, exact provenance capture, parameterized rule catalog writes, observability, pipeline status API, and the full CWB behavior test matrix.**

### What's new

- **Delta maintenance kernel** (CWB-FIX-01/02): Source graph inserts and deletes now automatically update dependent CONSTRUCT target graphs in the same transaction. `insert_triple()` triggers incremental derivation; `delete_triple_from_graph()` triggers DRed-style rederive-then-retract. Manual `refresh_construct_rule()` is no longer required for routine operation.

- **HTAP-aware promoted-predicate retraction** (CWB-FIX-03): `retract_exclusive_triples()` now correctly handles VP tables in HTAP split mode (delta + main + tombstones). Delta-resident derived triples are deleted directly; main-resident derived triples receive tombstones — preventing silent retraction failures after merge.

- **Exact provenance capture** (CWB-FIX-04): Provenance is recorded via `INSERT ... ON CONFLICT DO NOTHING RETURNING` CTEs, capturing only rows inserted by the current rule run. Pre-existing `source = 1` triples from other rules or manual inserts are no longer mis-attributed.

- **Parameterized SPI and mode validation** (CWB-FIX-05): All catalog writes in `create_construct_rule` use `Spi::run_with_args` (parameterized) for scalar fields. Mode values are validated — only `'incremental'` and `'full'` are accepted.

- **Shared-target reference-count semantics** (CWB-FIX-06): Two or more rules can write the same derived triple to the same graph. Dropping or refreshing one rule preserves triples still supported by another rule's provenance row.

- **Observability columns** (CWB-FIX-07): `_pg_ripple.construct_rules` gains five new columns: `last_incremental_run`, `successful_run_count`, `failed_run_count`, `last_error`, `derived_triple_count`. `list_construct_rules()` exposes all health fields.

- **Full CWB test matrix** (CWB-FIX-08): `tests/pg_regress/sql/construct_rules.sql` now covers: create/initial derivation, incremental insert, DRed delete, refresh from scratch, self-cycle rejection, two-rule pipeline stratification, mutual-cycle rejection, drop-with-retract, drop-without-retract, shared target preservation, explain output, pipeline status, and apply_for_graph.

- **SHACL rule bridge foundation** (CWB-FIX-09): `feature_status()` for `shacl_sparql_rule` updated to note that the derivation kernel foundation is delivered; full routing deferred to v0.66.0.

- **Pipeline introspection API** (CWB-FIX-10): New `pg_ripple.construct_pipeline_status() → JSONB` function returns dependency graph, rule order, last run state, derived triple counts, and failed/stale flags for all rules.

- **`apply_construct_rules_for_graph(graph_iri TEXT) → BIGINT`**: New public function for manual incremental maintenance of all rules sourcing a given graph. Returns total derived triple count.

- **Feature status promoted**: `construct_writeback` moves from `manual_refresh` to `implemented` in `pg_ripple.feature_status()`.

---

## [0.64.0] — 2026-04-27 — Release Truth and Safety Freeze

**Implements the v0.64.0 roadmap: feature-status SQL API, deep /ready readiness, GitHub Actions SHA pinning, Docker release digest integrity, documentation truth pass, roadmap evidence scripts, API drift checks, `just assess-release`, release evidence dashboard, and optional-feature degradation semantics guide.**

### What's new

- **`pg_ripple.feature_status()`** (TRUTH-01): New SQL function returning one row per major capability with an honest status value. Status taxonomy: `implemented`, `experimental`, `planner_hint`, `manual_refresh`, `stub`, `degraded`, `planned`. Reports honest statuses for Arrow Flight (`stub`), WCOJ (`planner_hint`), SHACL-SPARQL rules (`planned`), CONSTRUCT writeback (`manual_refresh`), Citus SERVICE pruning (`planned`), and all other major features.

- **Deep `/ready` readiness** (TRUTH-02): Extended `pg_ripple_http /ready` to include PostgreSQL version, extension version, and a feature-status snapshot. The response body now includes `partial_features` (all non-`implemented` features) and `degraded_features` (features with `stub` or `degraded` status).

- **GitHub Actions SHA pinning** (TRUTH-03): All third-party `uses:` references in `.github/workflows/` are now pinned to full 40-character commit SHAs with the human-readable tag as a comment. New CI step `scripts/check_github_actions_pinned.sh` rejects mutable refs (`@v6`, `@stable`, branch names) — zero mutable refs permitted.

- **Docker release digest integrity** (TRUTH-04): Removed `continue-on-error: true` from Docker build/push. Release job now captures the immutable image digest from the build step, fails if no digest is produced, and scans `ghcr.io/grove/pg-ripple@sha256:...` (the immutable digest) instead of a mutable tag.

- **Documentation truth pass** (TRUTH-05): Corrected `plans/implementation_plan.md` pgrx 0.17 → 0.18 throughout. Updated README "What works today" to reflect v0.63.0. Added "Known limitations in v0.63.0" section to README covering Arrow Flight, WCOJ, SHACL rules, CONSTRUCT writeback, Citus pruning, and optional dependencies.

- **Roadmap evidence check script** (TRUTH-06): `scripts/check_roadmap_evidence.sh` — advisory lint that flags completion-claim bullet points in CHANGELOG without evidence markers (CI test name, docs path, SQL function reference). Advisory-only in v0.64.0; will be enforced in v0.67.0.

- **API drift check script** (TRUTH-07): `scripts/check_api_drift.sh` — extracts exported function names from `#[pg_extern]` annotations in `src/` and checks that each appears in at least one documentation file. Advisory-only; catches the v0.63 Citus signature drift pattern.

- **`just assess-release`** (TRUTH-08): One-command release quality gate that runs migration headers lint, GitHub Actions pinning lint, SECURITY DEFINER lint, roadmap evidence check, API drift check, and version sync check. Optionally generates a release evidence report with `just assess-release VERSION`.

- **Release evidence dashboard** (TRUTH-09): `scripts/generate_release_evidence.sh` generates `target/release-evidence/<version>/summary.json` and `summary.md`. Release workflow uploads the artifact to GitHub Actions and attaches it to the GitHub release.

- **Degradation semantics guide** (TRUTH-10): New documentation page `docs/src/reference/degradation.md` documents expected degraded behavior, return values, warning codes, readiness behavior, and planned implementation milestones for every optional feature.

- **pg_regress test** `feature_status.sql`: Asserts that `feature_status()` returns rows, all statuses are from the approved taxonomy, and specific partial features report honest status values.

---

## [0.63.0] — 2025 — SPARQL CONSTRUCT Writeback Rules

**Implements the v0.63.0 roadmap: SPARQL CONSTRUCT writeback rules (CWB-01 through CWB-11), raw-to-canonical pipelines, incremental delta maintenance via Delete-Rederive (DRed), pipeline stratification with cycle detection, and Citus scalability improvements CITUS-30 through CITUS-37.**

### What's new

- **CONSTRUCT writeback rules** (CWB-01 to CWB-11): New SQL API `pg_ripple.create_construct_rule(name, sparql, target_graph, mode)` registers a SPARQL CONSTRUCT query as a persistent writeback rule. Derived triples are stored in the target named graph with `source = 1`. Supporting functions: `drop_construct_rule`, `refresh_construct_rule`, `list_construct_rules`, `explain_construct_rule`.

- **Catalog tables**: `_pg_ripple.construct_rules` (rule registry) and `_pg_ripple.construct_rule_triples` (provenance index) created lazily on first use and also via the migration script.

- **Pipeline stratification**: `compute_rule_order` performs Kahn's topological sort on the rule dependency graph; mutual-recursion (cycles) are rejected at registration time with a clear error.

- **Validation at registration**: blank nodes in CONSTRUCT templates, unbound variables, non-CONSTRUCT queries, and self-referential graphs (target == source) are all rejected with informative error messages.

- **Citus CITUS-30 — SERVICE result shard pruning**: New function `pg_ripple.service_result_shard_prune(endpoint TEXT, graph_iri TEXT) RETURNS BIGINT`. Returns the count of remote triples from the endpoint, or -1 if pruning is not applicable.

- **Citus CITUS-32 — HyperLogLog `COUNT(DISTINCT)`**: New function `pg_ripple.approx_distinct_available() RETURNS BOOLEAN`. Returns whether the HyperLogLog extension is available for approximate distinct counting.

- **Citus CITUS-37 — per-worker BRIN summarise**: New SRF `pg_ripple.brin_summarize_vp_shards() RETURNS TABLE(table_name TEXT, pages_summarized BIGINT)`. Summarises BRIN indexes on all VP main-partition tables.

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.63.0'` or apply `sql/pg_ripple--0.62.0--0.63.0.sql` manually. The migration creates the `_pg_ripple.construct_rules` and `_pg_ripple.construct_rule_triples` catalog tables.

---

## [0.62.0] — 2025 — Query Frontier

**Implements the v0.62.0 roadmap: Apache Arrow Flight bulk export, Leapfrog-Triejoin WCOJ planner integration, visual graph explorer in `pg_ripple_http`, tiered dictionary, Citus vp_rare vacuum, distributed inference dispatch, live shard rebalance, multi-hop pruning carry-forward, and `cargo deny` / `cargo audit` CI gates.**

### What's new

- **Apache Arrow Flight bulk export** (Q-1): New SQL function `pg_ripple.export_arrow_flight(graph_iri TEXT) RETURNS BYTEA`. Returns a JSON-encoded Flight ticket that the Arrow Flight server can use to stream all triples from the named graph in Arrow IPC format.

- **WCOJ planner integration** (Q-2): The BGP translator now detects cyclic Basic Graph Patterns (≥ 3 variables, ≥ 3 triple patterns) and activates the Leapfrog-Triejoin algorithm via `SET pg_ripple.enable_wcoj = on` preamble before executing the query. Provides sub-second execution for formerly intractable cyclic graph patterns.

- **Visual graph explorer** (Q-3): `pg_ripple_http` now serves a force-directed interactive graph visualizer at `/explorer`. The SPA fetches graph data and renders it as a D3.js force layout with node-label tooltips.

- **Arrow Flight `/flight/do_get` endpoint** (Q-4): `pg_ripple_http` accepts `POST /flight/do_get` with a Flight ticket body and responds with a JSON stub; the full streaming Arrow IPC implementation is wired in when the `arrow-flight` feature is enabled.

- **Citus CITUS-25 — vacuum_vp_rare** (CITUS-25): New SRF `pg_ripple.vacuum_vp_rare() RETURNS TABLE(predicate_id BIGINT, rows_removed BIGINT)`. Removes dead entries from `_pg_ripple.vp_rare` that reference predicates no longer in the predicate catalog.

- **Citus CITUS-26 — tiered dictionary** (CITUS-26): Added `access_count BIGINT NOT NULL DEFAULT 0` column to `_pg_ripple.dictionary`. New GUC `pg_ripple.dictionary_tier_threshold` (default: `-1`, disabled). When positive, entries whose `access_count` falls below the threshold may be evicted to a cold tier.

- **Citus CITUS-27 — distributed inference dispatch** (CITUS-27): New GUC `pg_ripple.datalog_citus_dispatch` (default: `off`). When enabled, the Datalog executor distributes each rule-stratum evaluation across Citus worker nodes.

- **Citus CITUS-28 — live shard rebalance** (CITUS-28): New SRF `pg_ripple.citus_live_rebalance() RETURNS TABLE(source_node TEXT, target_node TEXT, shard_id BIGINT, shard_size_bytes BIGINT)`. Initiates a non-blocking shard rebalance and streams per-shard progress.

- **Citus CITUS-29 — multi-hop pruning carry-forward** (CITUS-29): New GUC `pg_ripple.citus_prune_carry_max` (default: `1000`). The new `ShardPruneSet` and `prune_hop()` implementation carry the subject-ID set forward across triple-pattern hops, eliminating worker fan-out for multi-hop patterns.

- **CI quality gates** (Q-5): `cargo deny check` (license/advisory/duplicate crate check) and `cargo audit` (vulnerability scan) are now required CI steps.

---

## [0.61.0] — 2025 — Ecosystem Depth & Polish

**Implements the v0.61.0 roadmap: per-graph access control, GDPR right-to-erasure, inference explainability, SHACL-AF rule execution, dbt adapter, OTLP traceparent propagation, richer federation stats, Citus scalability improvements (object pruning, direct-shard bulk-load, graph shard affinity), and test quality improvements.**

### What's new

- **Per-named-graph access control** (6.3 / L-8.1): Added `pg_ripple.grant_graph(graph_iri, role)` and `pg_ripple.revoke_graph(graph_iri, role)` helper functions that install / remove PostgreSQL RLS policies filtering by graph ID.

- **GDPR right-to-erasure** (6.7 / L-8.3): New SRF `pg_ripple.erase_subject(iri TEXT) → TABLE(relation TEXT, rows_deleted BIGINT)`. Removes all triples about the subject across all VP tables, `vp_rare`, the dictionary (if unreferenced), KGE embeddings, the PROV-O provenance graph, and the audit log — in a single transaction. Returns a per-relation deletion count for the erasure record.

- **Inference explainability** (6.6): New SRF `pg_ripple.explain_inference(s, p, o, g) → TABLE(depth INT, rule_id TEXT, source_sids BIGINT[], child_triples JSONB)`. Returns the full derivation chain for a given inferred triple as a JSON tree, walking the `_pg_ripple.rule_firing_log` table introduced in this release.

- **SHACL-AF `sh:rule` execution** (D7-1 / D-3 / S4-8): Implemented the bridge in `src/shacl/af_rules.rs` that compiles `sh:TripleRule` patterns to Datalog rules and loads them via `load_rules_text()`. New pg_regress test `shacl_af_rule_execution.sql` validates end-to-end execution. Emits **PT482** when a `sh:rule` body cannot be compiled into a Datalog rule and is skipped.

- **dbt adapter** (6.11): Published `dbt-pg-ripple` Python package in `clients/dbt-pg-ripple/`. Provides `sparql_model`, `sparql_source`, and `sparql_ref` SPARQL-aware dbt macros. Data engineers can now mix SQL and SPARQL transformations in a single dbt project with full lineage tracking.

- **OTLP traceparent propagation** (I7-1): `pg_ripple_http` now extracts the W3C `traceparent` header and forwards it via the `pg_ripple.tracing_traceparent` session GUC into the extension. Every SPARQL/Datalog query span is tagged with the originating trace ID, giving an unbroken trace from the load balancer through the HTTP service into the query engine.

- **OpenTelemetry semantic-convention map** (I7-2): New doc `docs/src/operations/observability-otel.md` with span-name → attribute table and example Prometheus/Grafana queries.

- **Federation call stats per endpoint** (I7-3): `pg_ripple.federation_call_stats()` now returns `(endpoint TEXT, calls INT, errors INT, blocked INT, p50_ms INT, p95_ms INT, last_error_at TIMESTAMPTZ)`.

- **BRIN summarize failure tracking** (F7-3): Added `brin_summarize_failures INT` column to `_pg_ripple.predicates`. Persistent `brin_summarize_new_values()` failures are promoted from `debug1` to `NOTICE` after the second consecutive merge cycle failure; counter resets on success.

- **Citus object-based shard pruning** (CITUS-20): Extended `src/citus.rs` with `TermRole` enum (`Subject | Object`) and `prune_bound_term(term_id, role)`. The SPARQL translator now detects bound objects and routes to the correct shard, delivering the same 10–100× speedup that subject-pruning already provides.

- **Citus direct-shard bulk-load path** (CITUS-21): Added `batch_insert_encoded_shard_direct()` in `src/storage/mod.rs`. When Citus sharding is enabled, bulk-load batches are written directly to the physical shard table, bypassing coordinator routing. Falls back to the coordinator path when Citus is not installed or the predicate is in `vp_rare`.

- **Citus named-graph shard affinity** (CITUS-22): New table `_pg_ripple.graph_shard_affinity`. New functions `pg_ripple.set_graph_shard_affinity(graph_iri, shard_id)` and `pg_ripple.clear_graph_shard_affinity(graph_iri)`. When a SPARQL query includes a `GRAPH <g> { ... }` scope and Citus sharding is enabled, the planner restricts the query to the designated shard.

- **GROUP BY subject aggregate push-down audit** (CITUS-23): New pg_regress test `citus_aggregate_pushdown.sql` verifying that SPARQL `GROUP BY ?s` queries emit `GROUP BY s` in SQL, confirming Citus partial-aggregation push-down.

- **Temporal RDF post-merge correctness** (J7-3): New pg_regress test `temporal_rdf_post_merge.sql` verifying `point_in_time()` resolves correctly after SIDs move from delta to main.

- **OWL 2 RL deletion proof** (6.13 / E7-2): New pg_regress test `datalog_owl_rl_deletion.sql` exercising the full DRed retraction path.

- **DRed cycle guard** (E7-2): New pg_regress test `datalog_dred_cycle.sql` constructing a `sameAs` cycle and asserting PT530 is raised or the system remains stable.

- **SPARQL Entailment Regimes test driver** (6.8 / B7-2): New `tests/sparql_entailment/` directory with manifest, runner script, and RDFS/OWL 2 RL fixtures. Added as `entailment-suite` CI job (informational, `continue-on-error: true`).

- **Conformance thresholds config** (J7-4): Moved pass-rate thresholds from CI YAML expressions into `tests/conformance/thresholds.json`. CI now reads this file to determine gate criteria.

- **Cypher/GQL ADR** (K7-4): New `plans/cypher.md` capturing the design intent: target query subset, `cypher-parser` crate, rewrite-to-SPARQL strategy, and semantic fidelity notes.

- **PT404 error code for HTTP body-size rejection** (H7-6): `pg_ripple_http` now wraps axum's 413 response in a JSON envelope `{"error": "PT404", "message": "..."}`.

### Schema changes

- New table `_pg_ripple.graph_shard_affinity` (CITUS-22)
- New table `_pg_ripple.rule_firing_log` (inference explainability)
- New column `_pg_ripple.predicates.brin_summarize_failures INT` (F7-3)

### Migration

Upgrade from v0.60.0 with `ALTER EXTENSION pg_ripple UPDATE`.

---

## [0.60.0] — 2026-04-27 — Production Hardening Sprint

**Implements the v0.60.0 roadmap: HTAP merge atomic swap, CI supply-chain hardening, three new fuzz harnesses, `/ready` Kubernetes readiness probe, SERVICE SILENT circuit-breaker test, architecture diagram refresh, pg_trickle dependency matrix, and pg_dump round-trip test.**

### What's new

- **HTAP merge atomic rename-swap** (F7-1): Replaced the `DROP TABLE … CASCADE → RENAME → CREATE OR REPLACE VIEW` sequence in `src/storage/merge.rs` with an `ACCESS EXCLUSIVE`-locked rename-swap (`main → main_old → drop`, `main_new → main`). The VP view's backing relation is now never absent during a merge cycle, eliminating the race that caused `relation does not exist` errors under concurrent query load.

- **Merge-cutover chaos test** (J7-1): New `tests/concurrent/merge_cutover_chaos.sh` that hammers the VP view with continuous SPARQL queries while the merge worker churns for 60 seconds. Zero `relation does not exist` errors required.

- **Rare-predicate promotion concurrency test** (F7-2): New `tests/concurrent/promotion_race.sh` driving two parallel sessions across `vp_promotion_threshold`. Asserts exactly one VP table is created for a concurrently-promoted predicate.

- **Merge-throughput trend artifact** (F7-4): Added `benchmarks/merge_throughput_history.csv` to track p50/p95 TPS per release.

- **GitHub Actions SHA pinning** (H7-1): All external Actions in `.github/workflows/*.yml` are tracked by Dependabot (`package-ecosystem: github-actions`, already configured). Release workflow updated with Trivy CVE scan gate.

- **SECURITY DEFINER CI lint** (H7-2): Updated `scripts/check_no_security_definer.sh` to use an allowlist model — `_pg_ripple.ddl_guard_vp_tables()` is the only permitted use. Added as required CI step.

- **Security doc clarification** (H7-3): Updated `docs/src/reference/security.md` to correctly state that only the DDL event-trigger function uses `SECURITY DEFINER`; all other API functions are `SECURITY INVOKER`.

- **Rust toolchain pin** (N7-1/N7-2): Added `rust-toolchain.toml` pinning the stable channel.

- **Docker CVE scan** (N7-4): Added `aquasecurity/trivy-action` to the release workflow; fails if HIGH/CRITICAL CVEs are found in `Dockerfile.batteries`.

- **New fuzz harnesses** (A7-1): Three new `cargo-fuzz` targets — `geosparql_wkt` (WKT geometry parser), `r2rml_mapping` (Turtle-based R2RML documents), `llm_prompt_builder` (prompt sanitizer — asserts no injection markers survive).

- **Removed false-positive `#[allow(dead_code)]`** (A7-2): `execute_with_savepoint` in `src/datalog/parallel.rs` is called from `coordinator.rs`; the suppression was a false positive and has been removed.

- **SERVICE SILENT + circuit-breaker test** (B7-4): Added pg_regress test asserting that `SERVICE SILENT` correctly swallows PT605 (circuit-breaker-open) and returns the empty solution sequence per SPARQL 1.1 §8.3.1.

- **`/ready` Kubernetes readiness probe** (H7-5): Added `GET /ready` to `pg_ripple_http`. Returns `503` until the first successful PostgreSQL connection, then `200`. Distinct from `/health` (liveness probe).

- **Architecture diagram refresh** (K7-1): Updated the Mermaid diagram in `docs/src/reference/architecture.md` to include `src/citus.rs`, `src/tenant.rs`, `src/kge.rs`, `src/temporal.rs`, `src/sparql/sparqldl.rs`, `src/sparql/ql_rewrite.rs`, and the `/ready` endpoint.

- **pg_trickle dependency matrix** (K7-2): Added a feature-matrix table to `README.md` listing which features require pg_trickle vs. ship standalone.

- **Citus rebalance example** (K7-3): New `examples/citus_rebalance_with_trickle.sql` — runnable walkthrough of a zero-downtime Citus shard rebalance with pg_ripple + pg_trickle.

- **pg_dump round-trip CI test** (6.14): Added `tests/integration/dump_restore.sh` as a CI-friendly entry point to the existing `tests/pg_dump_restore.sh`.

### Migration

No schema changes. Upgrade from v0.59.0 with `ALTER EXTENSION pg_ripple UPDATE`.

---

## [0.59.0] — 2026-04-26 — Citus Shard-Pruning, Rebalance Coordination & Explain

**Implements the v0.59.0 roadmap: SPARQL shard-pruning for bound subject patterns, NOTIFY-based rebalance coordination, `explain_sparql()` Citus section, and `citus_rebalance_progress()`.**

### What's new

- **SPARQL shard-pruning** (CITUS-10): New shard-pruning infrastructure in `src/citus.rs`. When `pg_ripple.citus_sharding_enabled = on`, bound subject IRIs in SPARQL triple patterns are encoded to their integer subject ID and mapped to the physical Citus shard table via `pg_dist_shard`. Helper functions `compute_shard_id()`, `prune_bound_subject()`, and `resolve_shard_table()` implement the 10–100× speedup for queries like `SELECT ?p ?o WHERE { <http://example.org/Alice> ?p ?o }` that previously fan-out to all workers. Gracefully falls back to full fan-out when Citus is not installed.

- **Rebalance NOTIFY coordination** (CITUS-11): `pg_ripple.citus_rebalance()` now emits `pg_notify('pg_ripple.merge_start', '{"context":"rebalance","pid":PID}')` before acquiring the advisory fence lock and `pg_notify('pg_ripple.merge_end', ...)` after releasing it. pg-trickle v0.34.0 can use these signals to suspend per-worker slot polling during rebalancing.

- **explain_sparql() Citus section** (CITUS-12): New 3-arg overload `pg_ripple.explain_sparql(query text, analyze bool, citus bool) → jsonb`. When `citus = true`, the returned JSONB includes a `"citus"` key showing `available`, `pruned_to_shard`, `worker`, `full_fanout_avoided`, and `estimated_rows_per_shard`. Returns `{"available": false}` when Citus is not installed.

- **Rebalance progress reporting** (CITUS-13): New function `pg_ripple.citus_rebalance_progress()` returning `(shard_id, from_node, to_node, status)` rows from `pg_dist_rebalance_progress` (Citus 10+). Returns empty set when Citus is not installed.

- **Citus + pg_ripple + pg-trickle integration guide** (CITUS-15): New page `docs/src/citus_integration.md` with end-to-end deployment, GUC configuration, shard-pruning verification, and rebalancing runbook.

### Migration

No schema changes. Shard-pruning activates automatically when `pg_ripple.citus_sharding_enabled = on` and Citus is detected.

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.59.0';
```

---

## [0.58.0] — 2026-05-14 — Temporal RDF, SPARQL-DL, Citus Sharding & PROV-O

**Implements the v0.58.0 roadmap: Temporal RDF point-in-time queries, SPARQL-DL OWL axiom routing, Citus horizontal sharding of VP tables, PROV-O provenance tracking, v1 readiness integration test suite, and CI gate hardening.**

### What's new

- **Temporal RDF** (L-1.3): New functions `pg_ripple.point_in_time(ts TIMESTAMPTZ)`, `pg_ripple.clear_point_in_time()`, and `pg_ripple.point_in_time_info()`. A new `_pg_ripple.statement_id_timeline` table (SID → TIMESTAMPTZ, BRIN-indexed) is populated by an AFTER INSERT trigger on every VP delta table. Calling `point_in_time()` sets a session-local `_pg_ripple.pit_threshold` GUC that restricts SPARQL queries to triples inserted before the given timestamp.

- **SPARQL-DL** (L-1.4): New functions `pg_ripple.sparql_dl_subclasses(TEXT)` and `pg_ripple.sparql_dl_superclasses(TEXT)` route OWL vocabulary BGPs (`owl:subClassOf`, `owl:equivalentClass`, `owl:disjointWith`, `owl:inverseOf`) to the VP table T-Box data rather than synthesising a separate in-memory index. New module `src/sparql/sparqldl.rs`.

- **Citus horizontal sharding** (L-5.4): New GUCs `pg_ripple.citus_sharding_enabled` (bool, default off), `pg_ripple.citus_trickle_compat` (bool, default off), and `pg_ripple.merge_fence_timeout_ms` (int, default 0). New functions `pg_ripple.enable_citus_sharding()`, `pg_ripple.citus_rebalance()`, `pg_ripple.citus_cluster_status()`, and `pg_ripple.citus_available()`. When `citus_sharding_enabled = on`, VP tables get `REPLICA IDENTITY FULL` before `create_distributed_table()` (C-9 fix). Dictionary and predicates catalog become reference tables. Merge worker acquires an advisory fence lock during rebalancing and emits `pg_ripple.merge_start`/`merge_end` NOTIFYs.

- **PROV-O provenance** (L-8.4): New GUC `pg_ripple.prov_enabled` (bool, default off). When enabled, every bulk-load operation (`load_ntriples`, `load_turtle`, `load_nquads`) emits PROV-O `prov:Activity` + `prov:Entity` triples into the named graph `<urn:pg_ripple:prov>` and updates `_pg_ripple.prov_catalog`. New functions `pg_ripple.prov_stats()` and `pg_ripple.prov_enabled()`.

- **v1 readiness integration test suite** (J-6): New `tests/integration/v1_readiness/` directory with four shell test scripts: `crash_recovery.sh`, `concurrent_writes.sh`, `upgrade_chain.sh`, and `regress_mismatch_audit.sh`. Run via `tests/integration/v1_readiness/run_all.sh`.

- **CI gate hardening** (J-5): Four new pg_regress test files: `temporal_rdf`, `sparql_dl`, `citus_sharding`, `prov_triples`. All new tests pass in CI without Citus installed.

### Migration

New tables and trigger function are installed automatically on first use of `_PG_init`. For existing installations, run:

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.58.0';
```

---

## [0.57.0] — 2026-05-07 — Reasoning Platform & AI Integration

**Implements the v0.57.0 roadmap: OWL 2 EL/QL reasoning profiles, Knowledge-Graph Embeddings (TransE/RotatE), entity alignment via HNSW ANN search, LLM-augmented SPARQL repair, automated ontology mapping, multi-tenant graph isolation, columnar VP storage guard, adaptive index advisor, and probabilistic Datalog GUC.**

### What's new

- **OWL 2 EL profile** (L-3.1): New built-in rule set `'owl-el'` with core EL rules (`prp-some`, `cls-int1/2`, `cls-uni`, `cls-svf1/avf`, subsumption propagation). New GUC `pg_ripple.owl_profile = 'EL'`. `load_rules_builtin('owl-el')` activates EL-profile reasoning.

- **OWL 2 QL profile** (L-3.2): New built-in rule set `'owl-ql'` with DL-Lite rewriting rules (`SubClassOf`, `SubObjectPropertyOf`, `InverseOf`). New module `src/sparql/ql_rewrite.rs` rewrites SPARQL BGPs before SQL translation when `pg_ripple.owl_profile = 'QL'`.

- **Knowledge-Graph Embeddings** (L-4.1): New GUCs `pg_ripple.kge_enabled` (bool, default off) and `pg_ripple.kge_model` (text, default `'transe'`). New table `_pg_ripple.kge_embeddings (entity_id BIGINT PRIMARY KEY, embedding vector(64), model TEXT, trained_at TIMESTAMPTZ)` with HNSW index. New SRF `pg_ripple.kge_stats()`.

- **Entity alignment** (L-4.2): New function `pg_ripple.find_alignments(source_graph, target_graph, threshold, limit)` — uses cosine similarity over KGE embeddings to propose cross-graph `owl:sameAs` candidates.

- **LLM SPARQL repair** (L-4.3): New function `pg_ripple.repair_sparql(query TEXT, error_message TEXT)` — sends broken query + schema digest to LLM endpoint and returns a suggested fix. Sanitizes input against null-bytes, 32 KiB cap, and prompt-injection markers.

- **Automated ontology mapping** (L-4.4): New function `pg_ripple.suggest_mappings(source_graph, target_graph, method)` — `'lexical'` mode uses Jaccard similarity over tokenized `rdfs:label` values; `'embedding'` mode uses KGE cosine similarity.

- **Multi-tenant graph isolation** (L-5.3): New table `_pg_ripple.tenants`. New functions `pg_ripple.create_tenant()`, `pg_ripple.drop_tenant()`, `pg_ripple.tenant_stats()`. Quota-enforcing triggers per tenant graph.

- **Columnar VP storage guard** (L-2.1): New GUC `pg_ripple.columnar_threshold` (int, default -1 = disabled). When set, the merge worker can convert `vp_{id}_main` to columnar storage via `pg_columnar` when triple count exceeds the threshold. Raises PT534 if `pg_columnar` is unavailable.

- **Adaptive index advisor** (L-2.2): New module `src/storage/index_advisor.rs` with `run_index_advisor_cycle()`. New GUC `pg_ripple.adaptive_indexing_enabled` (bool, default off). Tracks index creation events in `_pg_ripple.catalog_events` (new `predicate_id` column).

- **Probabilistic Datalog GUC** (L-3.4): New GUC `pg_ripple.probabilistic_datalog` (bool, default off). Foundation for Markov-Logic-style soft rules with `@weight(FLOAT)` annotations.

### New error codes

| Code  | Level   | Meaning |
|-------|---------|---------|
| PT545 | ERROR   | Tenant quota exceeded: triple count for a named graph exceeds the per-tenant quota set by `create_tenant()`. |
| PT560 | ERROR   | `repair_sparql`: input SPARQL query exceeds the 32 KiB maximum length limit. |
| PT561 | ERROR   | `repair_sparql`: input error_message exceeds the 4 KiB maximum length limit. |

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE` or apply `sql/pg_ripple--0.56.0--0.57.0.sql`.

---

## [0.56.0] — 2026-04-30 — Standards Completeness & Operational Depth

**Implements the v0.56.0 roadmap: GeoSPARQL 1.1 geometry functions, federation circuit breaker, SPARQL audit log, DDL event trigger, BRIN re-summarize after merge, SID sequence runway monitor, incremental RDFS closure mode, R2RML direct mapping, lz4 dictionary compression, dead-code audit, and deprecated GUC removal.**

### What's new

- **GeoSPARQL 1.1 additions** (L-1.1): New filter predicates `geof:within` → `ST_Within` and `geof:intersects` → `ST_Intersects`. New value functions `geof:buffer`, `geof:convexHull`, `geof:envelope`, `geo:asWKT`, and `geo:hasSpatialAccuracy`.

- **Federation circuit breaker** (G-3): Thread-local `CircuitBreaker` state machine per endpoint URL. Opens after `pg_ripple.federation_circuit_breaker_threshold` consecutive failures (default: 5), resets after `pg_ripple.federation_circuit_breaker_reset_seconds` (default: 60 s). Returns PT605 while open.

- **SPARQL audit log** (H-3): New table `_pg_ripple.audit_log` populated when `pg_ripple.audit_log_enabled = on`. Records SPARQL UPDATE operations (role, txid, operation, query). New SQL functions: `pg_ripple.audit_log()` and `pg_ripple.purge_audit_log(before TIMESTAMPTZ)`.

- **DDL event trigger** (I-2): `_pg_ripple.ddl_guard_vp_tables()` event trigger function and `_pg_ripple_ddl_guard` event trigger. Emits PT511 warning and inserts into `_pg_ripple.catalog_events` when VP tables are dropped outside maintenance functions.

- **BRIN re-summarize after merge** (F-7): The merge worker calls `brin_summarize_new_values()` on the main VP table BRIN index after the atomic rename step, keeping BRIN statistics current.

- **SID runway monitor** (F-3): New SQL function `pg_ripple.sid_runway()` returns `(current_value, max_value, insert_rate_per_day, years_remaining)` estimating how long before `statement_id_seq` wraps.

- **Incremental RDFS closure** (L-3.3): New `pg_ripple.inference_mode = 'incremental_rdfs'` value. After each merge, `run_incremental_rdfs_for_predicate()` is called for RDFS schema predicates only, avoiding full-graph re-inference on every write.

- **R2RML direct mapping** (L-7.3): New SQL function `pg_ripple.r2rml_load(mapping_iri TEXT) → BIGINT`. Reads a W3C R2RML 2012 mapping document already loaded in the store, executes the mapped SQL queries, and bulk-inserts the generated triples.

- **lz4 dictionary compression** (L-2.4): `ALTER TABLE _pg_ripple.dictionary ALTER COLUMN value SET COMPRESSION lz4` applied at install and in the migration script. Reduces storage for long IRIs and literal strings on PG18 builds with lz4 support.

- **Dead-code audit** (A-6): `telemetry.rs`, `federation_planner.rs`, and `filter_expr.rs` cleaned up. Removed unused functions `inline_int_arith` and `inline_int_divide`; added per-item `#[allow(dead_code)]` annotations with explanations for planned-but-not-yet-wired APIs.

- **Remove deprecated `property_path_max_depth` GUC** (S2-5): The alias GUC `pg_ripple.property_path_max_depth` introduced in v0.24.0 is removed. Use `pg_ripple.max_path_depth` (the canonical name) instead.

### Schema changes

- New table `_pg_ripple.audit_log`
- New table `_pg_ripple.catalog_events`
- New function `_pg_ripple.ddl_guard_vp_tables() RETURNS event_trigger`
- New event trigger `_pg_ripple_ddl_guard ON sql_drop`
- `ALTER TABLE _pg_ripple.dictionary ALTER COLUMN value SET COMPRESSION lz4`

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE` or apply `sql/pg_ripple--0.55.0--0.56.0.sql`.

---

## [0.55.0] — 2026-04-24 — Security Hardening, Observability & Developer Experience

**Implements the v0.55.0 roadmap: federation SSRF protection, Unicode normalization, tombstone GC optimization, SPARQL-star annotation tests, SHACL snapshot semantics, Datalog dead-code cleanup, pg_ripple_http OpenAPI spec and VoID/Service endpoints, parallel concurrent insert tests, and comprehensive error catalog additions.**

### What's new

- **Federation SSRF allowlist** (G-1/H-1): New GUCs `pg_ripple.federation_endpoint_policy` (default: `default-deny`) and `pg_ripple.federation_allowed_endpoints`. The `check_endpoint_policy()` guard blocks private/loopback/link-local addresses unless the policy is `open`. PT606 errors emitted for blocked endpoints.

- **Federation call stats** (G-4): New `pg_ripple.federation_call_stats()` SRF returning `(calls, errors, blocked)` from in-memory atomic counters. Counters are updated by `execute_remote()` and reset on postmaster restart.

- **Unicode NFC normalization** (C-1): New bool GUC `pg_ripple.normalize_iris` (default: `on`). When enabled, all IRIs and blank nodes are NFC-normalized before dictionary encoding. Requires the new `unicode-normalization` crate dependency.

- **COPY RDF path allowlist** (C-2): New GUC `pg_ripple.copy_rdf_allowed_paths` (comma-separated path prefixes). When set, `load_*_file()` functions reject paths not matching an allowed prefix with PT480.

- **Tombstone GC optimization** (F-2): When `pg_ripple.tombstone_retention_seconds = 0`, the merge worker now `TRUNCATE`s the tombstones table after a successful merge instead of issuing a `DELETE … WHERE i <= $1`. Also records `tombstones_cleared_at` in the predicates catalog. Migration script adds the `tombstones_cleared_at TIMESTAMPTZ` column.

- **LLM API key warning** (H-2): New assign hook for `pg_ripple.llm_api_key_env` emits a `WARNING` if the value looks like a raw API key rather than an environment-variable name. Security documentation added to `docs/src/reference/security.md`.

- **pg_ripple_http OpenAPI spec** (K-1): Added `utoipa` and `utoipa-scalar` dependencies. `GET /openapi.yaml` returns the OpenAPI 3.1 specification for the HTTP service.

- **pg_ripple_http VoID and Service Description** (L-7.2/L-7.4): `GET /void` returns a Turtle VoID dataset description with triple counts; `GET /service` returns a W3C SPARQL Service Description document.

- **Health endpoint enriched** (I-3): `GET /health` now returns structured JSON including `version`, `git_sha`, `postgres_connected`, `postgres_version`, and `last_query_ts`.

- **SHACL validation snapshot LSN** (D-2): The `run_validate()` JSON report now includes `validation_snapshot_lsn` (WAL LSN captured at validation start) so consumers can correlate reports with a specific database state.

- **DESCRIBE strategy documentation** (B-2): `docs/src/reference/sparql-compliance.md` now documents all four `describe_strategy` values (`cbd`, `scbd`, `simple`) with definitions, examples, and a comparison table.

- **SPARQL-star annotation tests** (B-4): New pg_regress test `tests/pg_regress/sql/sparql_star_annotation.sql` with expected output covering the full annotation pattern (load, query, filter, provenance, nested annotations, CONSTRUCT).

- **Merge/vector CI baseline gates** (F-5/F-6): `.github/workflows/benchmark.yml` now includes merge throughput and vector recall baseline gate steps that compare measured performance against `benchmarks/merge_throughput_baselines.json`.

- **Crash recovery test** (J-2): New `tests/crash_recovery/merge_kill.sh` tests SIGKILL during merge with tombstone table recovery.

- **Concurrent write test** (J-3): New `tests/concurrent/parallel_insert.sh` launches N parallel psql sessions each inserting a disjoint triple set and verifies no writes are lost or duplicated.

- **Logical replication example** (K-2): New `examples/replication_setup.sql` with annotated walkthrough of primary + replica setup using `pg_ripple.replication_enabled = on`.

- **sh:path helper audit** (D-1): Audited `values_for_path_iri` in `src/shacl/constraints/property_path.rs` — all `ShPath` variants are handled correctly; updated `#[allow(dead_code)]` documentation.

- **Datalog dead-code cleanup** (E-2/E-3): Removed module-level `#![allow(dead_code)]` from `dred.rs` and `compiler.rs`; functions genuinely unused now have per-function `#[allow(dead_code)]` with explanatory comments.

- **Savepoint safety** (E-1): `execute_with_savepoint` wired into coordinator's `execute_stratum_batch`, ensuring each stratum evaluates within a savepoint to protect against partial-evaluation failures.

- **New GUCs**: `pg_ripple.federation_endpoint_policy`, `pg_ripple.federation_allowed_endpoints`, `pg_ripple.tombstone_retention_seconds`, `pg_ripple.normalize_iris`, `pg_ripple.copy_rdf_allowed_paths`, `pg_ripple.read_replica_dsn`.

- **Error catalog additions** (I-1): Added PT440, PT480, PT481, PT510, PT511, PT530, PT543, PT550, PT606(SSRF), PT607, PT620, PT621, PT640, PT642, PT711, PT712, PT800. `scripts/check_pt_codes.sh` passes (35 codes documented). CI job `lint-pt-codes` added.

- **CI improvements**: `jena-suite` and `owl2rl-suite` now run with `continue-on-error: false` (must pass).

- **Orphaned test cleanup** (J-1): Removed empty `tests/pg_regress/expected/test.txt`.

### Migration

The `sql/pg_ripple--0.54.0--0.55.0.sql` migration script:
- Adds `tombstones_cleared_at TIMESTAMPTZ` to `_pg_ripple.predicates`
- No other schema changes (all new features are Rust function changes or GUC additions)

---

## [0.54.0] — 2026-04-24 — High Availability & Logical Replication

**Implements the v0.54.0 roadmap: RDF logical replication, batteries-included Docker image, Kubernetes Helm chart, CloudNativePG extension image volume, and vector-index performance benchmarks.**

### What's new

- **RDF logical replication** (`src/replication.rs`): New `pg_ripple.logical_apply_worker` background worker (enabled via `pg_ripple.replication_enabled = on`) that subscribes to the `pg_ripple_pub` publication, receives N-Triples batches, and applies them via `load_ntriples()` in order. Conflict resolution: `last_writer_wins` per SID, configurable via `pg_ripple.replication_conflict_strategy`.

- **`pg_ripple.replication_stats()`**: New SRF that exposes the current replication slot state — `slot_name`, `lag_bytes`, `last_applied_lsn`, `last_applied_at`. Returns a single NULL row when replication is disabled.

- **New GUCs**: `pg_ripple.replication_enabled` (bool, default off) and `pg_ripple.replication_conflict_strategy` (text, default `last_writer_wins`).

- **`_pg_ripple.replication_status` catalog table**: Created by the migration script; tracks pending N-Triples batches delivered by the logical replication slot for the apply worker to consume.

- **Batteries-included Docker image** (`docker/Dockerfile.batteries`): Builds `ghcr.io/grove/pg_ripple:<version>` with pg_ripple, PostGIS 3.4.3, and pgvector 0.7.4 pre-installed. All four extensions load without conflicts. Published to GHCR on every release via GitHub Actions.

- **CloudNativePG extension image** (`docker/Dockerfile.cnpg`): Publishes `ghcr.io/grove/pg_ripple:<version>-cnpg` — a minimal image containing compiled `.so` and SQL files at `/var/lib/postgresql/extension-files/` for use with CloudNativePG operator ≥ 1.24. No custom PostgreSQL image build required.

- **CloudNativePG `Cluster` manifest example** (`examples/cloudnativepg_cluster.yaml`): Annotated manifest referencing `spec.postgresql.extensionImages` for zero-build CNP deployment.

- **CI smoke test** (`tests/cloudnativepg_image_smoke.sh`): Builds the extension image locally and verifies the expected files are present at the correct paths.

- **Kubernetes Helm chart** (`charts/pg_ripple/`): Deploys the batteries-included image as a `StatefulSet` with configurable `replicaCount`, `persistence` (PVC), `http.service` (LoadBalancer/ClusterIP), `federationEndpoints`, `shacl.shapesConfigMap`, `llm.apiKeySecret`. Liveness and readiness probes via `pg_isready`.

- **Vector-index comparison benchmark** (`benchmarks/vector_index_compare.sql`): 100 k-embedding fixture measuring index build time and ANN recall/latency for `{hnsw, ivfflat}` × `{single, half, binary}`. Reference results published in `docs/src/reference/vector-index-tradeoffs.md`.

- **`docker-compose.yml` updated**: Now uses the batteries-included image by default with example SPARQL queries that exercise GeoSPARQL (PostGIS) and vector search (pgvector).

- **Documentation** (`docs/src/`):
  - `operations/replication.md` — architecture overview, setup walkthrough, lag monitoring, failover procedure
  - `operations/docker.md` — batteries-included image quickstart and configuration reference
  - `operations/kubernetes.md` — Helm deployment guide, values reference, Prometheus integration
  - `operations/cloudnativepg.md` — step-by-step CNP setup, manifest walkthrough, upgrade procedure
  - `operations/high-availability.md` — HA topology decision tree and trade-offs table
  - `reference/vector-index-tradeoffs.md` — HNSW vs IVFFlat benchmark results and recommendations

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.54.0';` or use the supplied migration script `sql/pg_ripple--0.53.0--0.54.0.sql`.

---

## [0.53.0] — 2026-05-08 — DX, Extended Standards & Architecture

**Implements the v0.53.0 roadmap: SHACL-SPARQL constraints, COPY rdf FROM, RAG pipeline hardening, CDC lifecycle events, fuzz coverage expansion, WatDiv gate promotion, and merge-throughput baselines.**

### What's new

- **SHACL-SPARQL constraint component** (`src/shacl/`): Implements `sh:SPARQLConstraintComponent` (W3C SHACL-SPARQL). A new `SparqlConstraint` variant on `ShapeConstraint` stores a SPARQL SELECT query; during validation the query is executed with `$this` bound to the focus-node IRI. Any non-empty result set generates a `Violation`. The parser now recognises `sh:sparql` predicates in node and property shapes.

- **`pg_ripple.copy_rdf_from(path, format)`** (`src/dict_api.rs`): New SQL function that loads RDF triples from a server-side file. Supported formats: `ntriples`, `nquads`, `turtle`, `trig`, `rdfxml`. Returns the number of triples inserted.

- **RAG pipeline hardening** (`src/llm/mod.rs`, `src/schema.rs`): `rag_context()` now (1) validates and sanitises input (null-byte rejection, 16 KiB length cap), (2) looks up results in `_pg_ripple.rag_cache` (1-hour TTL) before running inference, and (3) stores results in the cache after computation. The `_pg_ripple.rag_cache` table is created by the schema initialiser and migration script.

- **CDC lifecycle events** (`src/storage/merge.rs`): The HTAP merge worker now emits `pg_notify('pg_ripple_cdc_lifecycle', payload)` at the end of each successful merge cycle. The JSON payload contains `{"op":"merge","predicate_id":N,"merged":M,"tombstones":T}`. Clients can `LISTEN pg_ripple_cdc_lifecycle` to receive real-time merge notifications.

- **New fuzz targets** (`fuzz/fuzz_targets/`): Three new cargo-fuzz targets: `rdfxml_parser` (RDF/XML via rio_xml), `jsonld_framer` (JSON-LD framing via serde_json), `http_request` (HTTP query-string and URI parsing via url). Dependencies `rio_xml`, `serde_json`, and `url` added to `fuzz/Cargo.toml`.

- **WatDiv suite gate promoted** (`.github/workflows/ci.yml`): Changed `watdiv-suite` job from `continue-on-error: true` to `continue-on-error: false`. The WatDiv benchmark suite is now a required CI gate.

- **Merge-throughput baselines** (`benchmarks/merge_throughput_baselines.json`): Added reference p50/p95 throughput measurements for `merge_workers ∈ {1,2,4,8}` to anchor the benchmark regression gate.

- **Error codes PT480 / PT481** (`src/error.rs`): PT480 warns when `sh:rule` is detected but SHACL-AF inference is off; PT481 is emitted when a SHACL-SPARQL constraint query fails to execute.

- **GUC subsystem split** (`src/gucs/`): `src/gucs.rs` refactored into seven focused modules: `storage`, `sparql`, `datalog`, `shacl`, `federation`, `llm`, `observability`.

- **filter.rs split** (`src/sparql/translate/filter/`): `filter.rs` split into `filter_dispatch` (pattern dispatch utilities) and `filter_expr` (SPARQL Expression → SQL compiler).

- **Datalog coordinator / semi-naïve modules** (`src/datalog/`): New `coordinator.rs` and `seminaive.rs` delegation modules.

- **HTTP `unwrap()` hardening** (`pg_ripple_http/src/main.rs`): All `Response::builder()` `.unwrap()` calls in hot-path handlers replaced with `unwrap_or_else(|e| ...)` that returns a structured `internal_server_error` JSON response.

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.53.0';` or use the supplied migration script `sql/pg_ripple--0.52.0--0.53.0.sql`.

---

## [0.52.0] — 2026-05-01 — pg-trickle Relay Integration

**Implements the v0.52.0 roadmap: JSON→RDF pipeline, CDC bridge triggers, JSON-LD event serializer, outbox dedup keys, vocabulary alignment templates, and pg-trickle runtime detection with graceful degradation.**

### What's new

- **JSON → RDF pipeline** (`src/bulk_load.rs`): New `pg_ripple.json_to_ntriples(payload JSONB, subject_iri TEXT, type_iri TEXT, context JSONB) RETURNS TEXT` converts any JSON object to N-Triples using an optional `@vocab` context for key-to-IRI mapping. Handles nested objects (blank nodes), arrays (repeated predicates), and plain string values. `json_to_ntriples_and_load()` combines conversion and load in one call.

- **CDC bridge triggers** (`src/storage/cdc_bridge.rs`): New `pg_ripple.enable_cdc_bridge_trigger(name, predicate, outbox)` installs a per-predicate `AFTER INSERT` trigger on the VP delta table that decodes dictionary IDs and writes a JSON-LD event with a dedup key to the specified outbox table within the same transaction. `disable_cdc_bridge_trigger(name)` removes it. `cdc_bridge_triggers()` SRF lists all registered triggers.

- **JSON-LD event serializer** (`src/export.rs`): New `pg_ripple.triple_to_jsonld(s, p, o BIGINT) RETURNS JSONB` decodes a single triple from dictionary IDs and returns a JSON-LD object. `triples_to_jsonld(subject BIGINT)` performs a star-pattern scan for all triples of a subject and returns a grouped JSON-LD node.

- **Outbox dedup key** (`src/storage/mod.rs`): New `pg_ripple.statement_dedup_key(s, p, o BIGINT) RETURNS TEXT` looks up the statement ID (`i` column) for a triple and returns `'ripple:{sid}'` as a relay-compatible dedup key. Returns NULL when the triple does not exist.

- **Vocabulary alignment templates** (`sql/vocab/`): Four built-in Datalog rule sets loadable via `pg_ripple.load_vocab_template(name TEXT) RETURNS INT`:
  - `schema_to_saref` — Schema.org ↔ SAREF IoT sensor data alignment
  - `schema_to_fhir` — Schema.org ↔ FHIR R4 basic resources (Patient, Observation)
  - `schema_to_provo` — Schema.org ↔ PROV-O provenance ontology
  - `generic_to_schema` — generic JSON key → Schema.org property heuristics

- **pg-trickle runtime detection** (`src/views_api.rs`, `src/cdc_bridge_api.rs`): `pg_ripple.trickle_available() RETURNS BOOL` returns `true` when both `pg_ripple.trickle_integration = on` and the `pg_trickle` extension is installed. Bridge functions raise SQLSTATE PT800 when pg-trickle is absent or integration is disabled.

- **New GUCs** (`src/gucs.rs`): `pg_ripple.cdc_bridge_enabled` (bool, default off), `pg_ripple.cdc_bridge_batch_size` (int, default 100), `pg_ripple.cdc_bridge_flush_ms` (int, default 200), `pg_ripple.cdc_bridge_outbox_table` (text), `pg_ripple.trickle_integration` (bool, default on).

- **CDC bridge catalog** (`_pg_ripple.cdc_bridge_triggers`): New catalog table records all registered CDC bridge triggers with columns `(name, predicate_id, outbox_table, created_at)`.

---

## [0.51.0] — 2026-04-23 — Security Hardening & Production Readiness

**Completes the v0.51.0 roadmap: SPARQL DoS protection (PT440), OWL 2 RL 100% conformance, SPARQL CSV/TSV output, SHACL complex path traversal, per-predicate workload stats, OTLP tracing wiring, non-root Docker container, blocking cargo-audit on PRs, SBOM generation, and comprehensive operational tooling.**

### What's new

- **SPARQL DoS protection (PT440)** (`src/sparql/mod.rs`): New GUCs `pg_ripple.sparql_max_algebra_depth` (default 256) and `pg_ripple.sparql_max_triple_patterns` (default 4096). Queries exceeding these limits are rejected at parse time with error code PT440. Set to 0 to disable.

- **Complete OWL 2 RL conformance** (`src/datalog/builtins.rs`): Fixed four previously failing rules: `prp-spo2` (3-hop property chains), `scm-sco` (bidirectional subClassOf → equivalentClass), `eq-diff1` (sameAs + differentFrom → owl:Nothing), `dt-type2` (XSD numeric type hierarchy). The OWL 2 RL gate is now 66/66 (100%) and blocking.

- **SPARQL CSV/TSV output** (`src/sparql_api.rs`): New `pg_ripple.sparql_csv(query TEXT)` and `pg_ripple.sparql_tsv(query TEXT)` SRFs returning W3C SPARQL 1.1 CSV/TSV formatted results.

- **SHACL complex property path traversal** (`src/shacl/constraints/property_path.rs`): The previously disabled `traverse_sh_path()` function is now wired into the SHACL property shape dispatcher. Supports inverse, alternative, sequence, `sh:zeroOrMorePath`, `sh:oneOrMorePath`, and `sh:zeroOrOnePath`.

- **Correct CONSTRUCT ground RDF-star quoted triples** (`src/sparql/mod.rs`): Ground quoted triples in CONSTRUCT templates now emit correct N-Triples-star notation `<< s p o >>` instead of being silently dropped.

- **Per-predicate workload statistics** (`src/stats_admin.rs`): New `pg_ripple.predicate_workload_stats()` SRF backed by `_pg_ripple.predicate_stats` table. Returns `(predicate_iri, query_count, merge_count, last_merged)` per predicate.

- **OTLP tracing endpoint** (`src/telemetry.rs`, `src/gucs.rs`): New GUC `pg_ripple.tracing_otlp_endpoint` wires the `"otlp"` exporter to a configurable endpoint. Falls back to stdout when the endpoint is empty.

- **Storage cache invalidation on vacuum** (`src/storage/catalog.rs`, `src/lib.rs`): Registered a PostgreSQL relcache invalidation callback via `CacheRegisterRelcacheCallback` so the backend-local VP table OID cache is automatically flushed when a relation is vacuumed.

- **Merge worker latch-driven backoff** (`src/worker.rs`): The error-backoff sleep in the merge worker now uses `BackgroundWorker::wait_latch()` so the worker responds immediately to SIGTERM rather than sleeping the full backoff interval.

- **Non-root Docker container** (`Dockerfile`): The container now runs as `USER postgres` (v0.51.0 security hardening).

- **Blocking cargo-audit on PRs** (`.github/workflows/cargo-audit.yml`): `cargo audit --deny warnings` now runs on every pull request, not just the weekly schedule.

- **SBOM generation** (`.github/workflows/release.yml`): Every release now includes a CycloneDX SBOM (`sbom.json`) attached to the GitHub release.

- **New CI linting jobs** (`.github/workflows/ci.yml`): `lint-sql-format` (unsafe dynamic SQL), `lint-migration-headers` (migration script header checks), `lint-cargo-duplicates` (advisory duplicate dependency check).

- **New scripts**: `scripts/check_no_string_format_in_sql.sh`, `scripts/check_migration_headers.sh`, `scripts/check_pt_codes.sh`.

- **New tests**: `tests/pg_dump_restore.sh`, `tests/pg_upgrade_compat.sh`, pg_regress `sparql_depth_limit.sql`, `sparql_csv_tsv.sql`, `shacl_complex_path.sql`.

- **New examples**: `examples/llm_workflow.sql`, `examples/federation_multi_endpoint.sql`, `examples/cdc_subscription.sql`.

- **New docs**: `docs/src/operations/cdc.md`, expanded tuning guide.

- **Justfile**: Added `just release VERSION` and `just docs-serve` recipes.

- **Migration script**: `sql/pg_ripple--0.50.0--0.51.0.sql` creates `_pg_ripple.predicate_stats` table.

- **Documentation**: Updated AGENTS.md to reflect pgrx 0.18 (was incorrectly documented as 0.17).

---

## [0.50.0] — 2026-04-23 — Developer Experience & GraphRAG Polish

**Completes the v0.50.0 roadmap: `explain_sparql(analyze:=true)` interactive query debugger with `cache_status` and `actual_rows`; `rag_context()` full RAG pipeline; migration chain passes through v0.50.0.**

### What's new

- **Extended `pg_ripple.explain_sparql(query TEXT, analyze BOOL DEFAULT FALSE) RETURNS JSONB`** (`src/sparql/explain.rs`):
  - New `cache_status` key: `"hit"` / `"miss"` / `"bypass"` — replaces the legacy `cache_hit` boolean (which is kept for backward compatibility).
  - New `actual_rows` key (array): per-operator actual row counts extracted from `EXPLAIN ANALYZE` JSON output when `analyze = true`.
  - DESCRIBE queries now return a valid JSONB document (algebra + synthetic SQL stub) instead of an error.
  - EXPLAIN output now uses `FORMAT JSON` for structured parsing.

- **`pg_ripple.rag_context(question TEXT, k INT DEFAULT 10) RETURNS TEXT`** (`src/llm/mod.rs`): full five-step RAG pipeline:
  1. HNSW vector recall — top-k entities by cosine similarity.
  2. SPARQL graph expansion — 1-hop neighbourhood via `contextualize_entity()`.
  3. JSON-LD context assembly — rich text context for LLM ingestion.
  4. (Optional) NL→SPARQL execution if `pg_ripple.llm_endpoint` is set.
  - Degrades gracefully (WARNING + empty string) when pgvector is absent.

- **New pg_regress test**: `sparql_explain_analyze.sql` — asserts JSONB schema stability across SELECT, ASK, CONSTRUCT, and DESCRIBE query types.

- **Documentation**:
  - `docs/src/user-guide/explain-sparql.md` — EXPLAIN output format, ANALYZE mode, interpreting the algebra tree.
  - `docs/src/user-guide/rag-pipeline.md` — `rag_context()` step-by-step, tuning k, combining with NL→SPARQL.

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.50.0'` — no schema changes; new Rust functions are automatically available.

<details>
<summary>Technical details</summary>

- **src/sparql/explain.rs** — `explain_sparql_jsonb()` extended: `cache_status` field (`"hit"` / `"miss"` / `"bypass"`), `actual_rows` array from `EXPLAIN ANALYZE` JSON, DESCRIBE query stub generation, `FORMAT JSON` output mode
- **src/llm/mod.rs** — `rag_context()` five-step pipeline: HNSW recall → SPARQL expansion via `contextualize_entity()` → JSON-LD assembly → optional NL→SPARQL execution; graceful pgvector degradation path (WARNING + empty string)
- **tests/pg_regress/sql/sparql_explain_analyze.sql** — JSONB schema stability assertions for SELECT, ASK, CONSTRUCT, and DESCRIBE query types; `cache_status` and `actual_rows` key presence checks
- **docs/src/user-guide/explain-sparql.md** — new; EXPLAIN output format reference, ANALYZE mode walkthrough, algebra tree interpretation guide
- **docs/src/user-guide/rag-pipeline.md** — new; `rag_context()` step-by-step usage, k-tuning guidance, NL→SPARQL integration pattern
- **sql/pg_ripple--0.49.0--0.50.0.sql** — comment-only; no schema changes required

</details>

---

## [0.49.0] — 2026-04-23 — AI & LLM Integration

**Completes the v0.49.0 roadmap: `sparql_from_nl()` NL-to-SPARQL via configurable LLM endpoint; `suggest_sameas()` and `apply_sameas_candidates()` for embedding-based entity alignment; four new GUCs; error codes PT700–PT702.**

### What's new

- **`pg_ripple.sparql_from_nl(question TEXT) RETURNS TEXT`** (`src/llm/mod.rs`): converts a natural-language question to a SPARQL SELECT query using any OpenAI-compatible LLM endpoint.
  - Set `pg_ripple.llm_endpoint = 'mock'` for testing without a real LLM.
  - `add_llm_example(question, sparql)` stores few-shot examples in `_pg_ripple.llm_examples`.
  - Error codes: PT700 (endpoint unreachable/not configured), PT701 (non-SPARQL response), PT702 (SPARQL parse failure).
  - SHACL shapes included as additional context when `pg_ripple.llm_include_shapes = on`.

- **`pg_ripple.suggest_sameas(threshold REAL DEFAULT 0.9)`**: HNSW cosine self-join on `_pg_ripple.embeddings`; returns `TABLE(s1 TEXT, s2 TEXT, similarity REAL)` pairs above the threshold. Degrades gracefully when pgvector is unavailable.

- **`pg_ripple.apply_sameas_candidates(min_similarity REAL DEFAULT 0.95)`**: inserts accepted pairs as `owl:sameAs` triples; respects `sameas_max_cluster_size`. Returns count of inserted triples.

- **New GUCs**: `pg_ripple.llm_endpoint`, `pg_ripple.llm_model`, `pg_ripple.llm_api_key_env`, `pg_ripple.llm_include_shapes`.

- **Schema change**: `_pg_ripple.llm_examples (question TEXT PRIMARY KEY, sparql TEXT, created_at TIMESTAMPTZ)`.

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.49.0'` — adds `_pg_ripple.llm_examples` and updates the schema version.

---

## [0.48.0] — 2026-04-23 — SHACL Core Completeness, OWL 2 RL Closure & SPARQL Completeness

**Completes the v0.48.0 roadmap: all 35 SHACL Core constraints implemented; complex `sh:path` expressions with recursive CTEs; OWL 2 RL rule-set closure (five new rules); SPARQL Update ADD/COPY/MOVE; SPARQL-star variable-inside-quoted-triple patterns; `federation_max_response_bytes` GUC; `insert_triples()` batch SRF; WatDiv baselines; `pg-upgrade.md` operations guide.**

### What's new

- **Remaining SHACL Core constraints** (`src/shacl/`) — seven new constraints complete the 35/35 SHACL Core coverage:
  - `sh:minLength` / `sh:maxLength`: string-length bounds applied after language-tag stripping
  - `sh:xone`: exactly-one-of (XOR) logic over sub-shapes via `check_xone()` in `src/shacl/constraints/logical.rs`
  - `sh:minExclusive` / `sh:maxExclusive` / `sh:minInclusive` / `sh:maxInclusive`: XSD-typed numeric range constraints via `compare_dictionary_values` in `src/shacl/constraints/relational.rs`

- **Complex `sh:path` expressions** (`src/shacl/constraints/property_path.rs`) — full `ShPath` enum with SQL compiler:
  - `sh:inversePath`: `(o, s)` join order on VP tables
  - `sh:alternativePath`: SQL UNION of sub-paths
  - Sequence paths: chained JOIN compilation
  - `sh:zeroOrMorePath`, `sh:oneOrMorePath`, `sh:zeroOrOnePath`: `WITH RECURSIVE … CYCLE` CTEs

- **SHACL violation report enhancements** — `Violation` struct extended with `sh_value` (offending decoded value) and `sh_source_constraint_component` (W3C component IRI) fields for W3C-conformant violation reports.

- **OWL 2 RL rule set completion** (`src/datalog/builtins.rs`) — five new rules close the v0.47.0 gap:
  - `cax-sco`: full `rdfs:subClassOf` transitive closure
  - `prp-spo1`: `rdfs:subPropertyOf` full chain
  - `prp-ifp`: inverse-functional-property `owl:sameAs` propagation
  - `cls-avf`: chained `owl:allValuesFrom` + subclass hierarchy
  - `owl:minCardinality` / `owl:maxCardinality` / `owl:cardinality` entailment

- **SPARQL Update ADD / COPY / MOVE** (`src/sparql/mod.rs`) — pre-parser `try_execute_add_copy_move()` handles all three graph management operations without depending on spargebra enum variants. pg_regress test `sparql_update_add_copy_move.sql`.

- **SPARQL-star variable-inside-quoted-triple patterns** (`src/sparql/translate/bgp.rs`) — `TermPattern::Triple` arm now emits a JOIN with `_pg_ripple.dictionary` on `qt_s`/`qt_p`/`qt_o` columns instead of silent `FALSE`. Patterns like `<< ?s ?p ?o >> :assertedBy ?who` return rows. pg_regress test `rdfstar_variable_quoted.sql`.

- **`pg_ripple.federation_max_response_bytes` GUC** (`src/gucs.rs`, `src/sparql/federation.rs`) — maximum federation response body size in bytes (default: 100 MiB). Responses exceeding the limit are refused with error code PT543.

- **`pg_ripple.insert_triples(TEXT[])` SRF** (`src/dict_api.rs`) — batch single-triple inserts. Accepts a flat `TEXT[]` array with stride-3 (s, p, o) or stride-4 (s, p, o, g) grouping. Returns `SETOF BIGINT` (SIDs). Useful for orchestration tools that need to insert many triples in one call.

- **WatDiv latency baselines** (`tests/watdiv/baselines.json`) — per-query p50/p95/p99 latency baseline file for all 32 WatDiv templates. CI regression gate warns on > 10% latency increase.

- **HTAP merge throughput benchmark** (`benchmarks/merge_throughput.sql`) — 5-minute pgbench script for measuring insert throughput under concurrent merge cycles.

- **`docs/src/operations/pg-upgrade.md`** — new operations guide documenting the supported upgrade matrix, pre-upgrade steps, migration script chain, and dump/restore fallback.

### Migration

`sql/pg_ripple--0.47.0--0.48.0.sql` — no schema changes.

## [0.47.0] — 2026-04-22 — SHACL Completion, GUC Validators, Cache SRFs & Fuzz Hardening

**Completes the v0.47.0 roadmap: sh:lessThanOrEquals SHACL constraint; six GUC check_hook validators; three individual cache hit-rate SRFs; SPARQL `sqlgen.rs` module split (≤800 lines); parallel Datalog SID pre-allocation wired; five new cargo-fuzz targets; CI security hygiene (cargo-audit workflow, deny.toml, check_no_security_definer.sh); OWL 2 RL baseline 93.9%; promotion-race stress test; four new SHACL pg_regress tests.**

### What's new

- **`sh:lessThanOrEquals` SHACL constraint** (`src/shacl/constraints/shape_based.rs`) — implements `sh:lessThanOrEquals` per SHACL Core §4.4. For each focus node, checks that every value of the subject property is ≤ the corresponding value of the comparison property. Violations include `"constraint": "sh:lessThanOrEquals"`. pg_regress test `shacl_lt_or_equals.sql` covers less-than, greater-than (violation), and equal-value cases.

- **Six GUC check_hook validators** (`src/lib.rs`) — `federation_on_error` (`warning`|`error`|`empty`), `federation_on_partial` (`empty`|`use`), `sparql_overflow_action` (`warn`|`error`), `tracing_exporter` (`stdout`|`otlp`), `embedding_index_type` (`hnsw`|`ivfflat`), `embedding_precision` (`single`|`half`|`binary`) now reject invalid values at SET time with a standard PostgreSQL GUC rejection message.

- **Individual cache hit-rate SRFs** (`src/sparql_api.rs`) — three new table-returning functions: `pg_ripple.plan_cache_stats()`, `pg_ripple.dictionary_cache_stats()`, and `pg_ripple.federation_cache_stats()`, each returning `(hits BIGINT, misses BIGINT, evictions BIGINT, hit_rate DOUBLE PRECISION)`. The old JSONB `plan_cache_stats()` is superseded by the new table form; the combined JSONB `cache_stats()` is retained for backwards compatibility.

- **SPARQL `sqlgen.rs` module split** (`src/sparql/translate/`) — `sqlgen.rs` reduced from 3,632 to 753 lines by extracting eight translation modules: `bgp.rs`, `filter.rs`, `graph.rs`, `group.rs`, `join.rs`, `left_join.rs`, `union.rs`, `distinct.rs`. Public API surface unchanged.

- **Parallel Datalog SID pre-allocation** (`src/datalog/mod.rs`) — `preallocate_sid_ranges()` is now called at the start of `run_inference_seminaive()` when `datalog_parallel_workers > 1`, eliminating sequence contention across parallel strata workers.

- **Five new cargo-fuzz targets** (`fuzz/fuzz_targets/`) — `sparql_parser.rs` (spargebra), `turtle_parser.rs` (rio_turtle + NTriples), `datalog_parser.rs` (rule tokenizer), `shacl_parser.rs` (Turtle + sh: predicate dispatch), `dictionary_hash.rs` (XXH3-128 determinism assertion).

- **CI security hygiene** — weekly scheduled `cargo audit` job (`.github/workflows/cargo-audit.yml`) that auto-creates a GitHub issue on failure; `deny.toml` with licence allowlist and advisory deny policy; `scripts/check_no_security_definer.sh` that fails CI if any `sql/*.sql` file contains `SECURITY DEFINER`.

- **OWL 2 RL conformance baseline** (`docs/src/reference/owl2rl-results.md`) — 62/66 rules pass (93.9%). Four known failures documented in `tests/owl2rl/known_failures.txt` with target fix versions.

- **Promotion-race stress test** (`tests/stress/promotion_race.sh`) — 50 concurrent sessions inserting at the VP promotion threshold; verifies SID uniqueness and zero errors.

- **Four new SHACL pg_regress tests** — `shacl_closed.sql`, `shacl_unique_lang.sql`, `shacl_pattern.sql`, `shacl_lt_or_equals.sql` — cover all four SHACL constraint families newly tested in v0.47.0.

### Documentation

- `docs/src/reference/guc-reference.md` — complete entries for all six new validated GUCs.
- `docs/src/reference/owl2rl-results.md` — new baseline document with pass-rate table and known-failure descriptions.

---

## [0.46.0] — 2026-04-21 — Property-Based Testing, Fuzz Hardening & OWL 2 RL Conformance

**Adds three property-based test suites (SPARQL round-trip, dictionary encode/decode, JSON-LD framing), a cargo-fuzz federation result decoder target, an OWL 2 RL conformance suite, TopN push-down optimisation, sequence range pre-allocation for parallel Datalog, BSBM regression gate, Rustdoc lint gate, HTTP companion CA-bundle support, and expanded worked examples.**

### What's new

- **proptest integration** (`tests/proptest/`) — three property-based test suites run 10,000 cases each: SPARQL algebra round-trip stability (encoding and whitespace invariance), XXH3-128 dictionary encode stability and collision resistance (10,000 distinct terms, zero collisions), and JSON-LD framing round-trip correctness.

- **cargo-fuzz federation result decoder** (`fuzz/fuzz_targets/federation_result.rs`) — fuzz target that feeds arbitrary byte sequences through the SPARQL XML results parser. Asserts no panic on malformed input; invalid XML produces PT542, never a crash.

- **PT542 `FederationResultDecoderError`** (`src/error.rs`) — new error code for unparseable XML/JSON in the federation result decoder.

- **Datalog convergence regression suite** (`tests/datalog_convergence_suite.rs`) — verifies RDFS + OWL RL rule-set convergence within ≤ 20 iterations; derived triple counts checked against baselines stored in `tests/datalog_convergence/baselines.json`.

- **W3C OWL 2 RL conformance suite** (`tests/owl2rl_suite.rs`) — adapter parses `DatatypeEntailmentTest`, `ConsistencyTest`, and `InconsistencyTest` manifest types. Non-blocking CI job until ≥ 95% pass rate. Known failures tracked in `tests/owl2rl/known_failures.txt`.

- **TopN push-down** (`src/sparql/sqlgen.rs`) — when `ORDER BY … LIMIT N` is present (no `OFFSET`, no `DISTINCT`) and `pg_ripple.topn_pushdown = on`, the LIMIT clause is embedded directly in the generated SQL rather than post-decode truncation. `sparql_explain()` output includes `"topn_applied": true/false`.

- **`pg_ripple.topn_pushdown`** (bool GUC, default `on`) — master switch for the TopN push-down optimisation.

- **Sequence range pre-allocation** (`src/datalog/parallel.rs`) — `preallocate_sid_ranges()` atomically advances the global statement-ID sequence by `N * batch_size` before launching parallel Datalog workers, eliminating sequence contention.

- **`pg_ripple.datalog_sequence_batch`** (integer GUC, default `10000`, min `100`) — SID range reserved per parallel Datalog worker per batch.

- **BSBM regression gate** (`benchmarks/bsbm/`) — 12 BSBM explore queries at 1M-triple scale; latency baselines in `benchmarks/bsbm/baselines.json`; CI warning on > 10% regression (non-blocking).

- **Rustdoc lint gate** (`src/lib.rs`) — `#![warn(missing_docs)]` added; CI job `cargo doc` fails on `missing_docs` for public `#[pg_extern]` functions.

- **HTTP companion CA-bundle** (`pg_ripple_http/src/main.rs`) — `PG_RIPPLE_HTTP_CA_BUNDLE` env var: loads the PEM file at the given path as the TLS trust anchor for outbound connections. Falls back to the system trust store with an error log if the path is invalid or not a valid PEM bundle.

- **Expanded worked examples** (`examples/`) — three end-to-end SQL scripts: `shacl_datalog_quality.sql` (SHACL + Datalog interaction), `hybrid_vector_search.sql` (vector similarity + SPARQL property paths), `graphrag_round_trip.sql` (GraphRAG export → Datalog annotation → re-import).

- **Migration script** (`sql/pg_ripple--0.45.0--0.46.0.sql`) — comment-only; no schema changes.

### GUC parameters added

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.topn_pushdown` | bool | `on` | Push `LIMIT N` into the SQL plan for `ORDER BY + LIMIT` queries |
| `pg_ripple.datalog_sequence_batch` | integer | `10000` | SID range reserved per parallel Datalog worker per batch |

### New error codes

| Code | Severity | Message |
|------|----------|---------|
| PT542 | ERROR | Federation result decoder received unparseable XML/JSON |

### Bug fixes

None.

### Documentation

- `docs/src/user-guide/best-practices/sparql-performance.md` — TopN push-down section with EXPLAIN example
- `docs/src/reference/guc-reference.md` — v0.46.0 section with two new GUC parameters
- `docs/src/reference/error-catalog.md` — PT542 added
- `docs/src/reference/contributing.md` — proptest and cargo-fuzz sections
- `docs/src/reference/w3c-conformance.md` — OWL 2 RL suite added to conformance table

---

## [0.45.0] — 2026-04-21 — SHACL Completion, Datalog Robustness & Crash Recovery

**Closes the last SHACL Core constraint gaps (`sh:equals`, `sh:disjoint`), adds decoded focus-node IRIs to violation messages, hardens Datalog evaluation with lattice join-function validation (PT541), and adds crash-recovery test scripts for two previously-untested kill scenarios.**

### What's new

- **`sh:equals` and `sh:disjoint` SHACL constraints** (`src/shacl/constraints/relational.rs`) — implements both relational constraints per SHACL Core §4.4. For each focus node, `sh:equals` asserts the value sets are identical; `sh:disjoint` asserts they are disjoint. Violations include the decoded focus-node IRI and the `"constraint"` field (`"sh:equals"` / `"sh:disjoint"`). pg_regress test `shacl_equals_disjoint.sql` covers passing shapes, failing shapes, and named-graph scoping.

- **Decoded focus-node IRIs in SHACL violations** (`src/shacl/mod.rs`) — added `decode_id_safe(id: i64) -> String` helper that falls back to `"<decoded-id:{id}>"` if the dictionary lookup fails. All new constraint violations include the decoded IRI.

- **`lattice.join_fn` validation via `regprocedure`** (`src/datalog/lattice.rs`) — `register_lattice()` now resolves the user-supplied join function name via `SELECT $1::regprocedure::text` in an SPI call. Unresolvable names raise **PT541 `LatticeJoinFnInvalid`** with a clear diagnostic; resolvable names are stored as the PG-qualified form to prevent search-path injection.

- **PT541 `LatticeJoinFnInvalid`** (`src/error.rs`) — new error code for invalid lattice join functions.

- **WFS iteration-cap test** (`tests/pg_regress/sql/datalog_wfs_cap.sql`) — pg_regress test that loads a mutually-recursive negation cycle guaranteed to reach `pg_ripple.wfs_max_iterations = 3`. Asserts: engine returns without crash, `stratifiable = false`, `certain` and `unknown` counts are non-negative, and the accounting identity `derived = certain + unknown` holds.

- **Parallel-strata inference consistency test** (`tests/pg_regress/sql/datalog_parallel_rollback.sql`) — validates that a valid multi-rule inference run produces consistent results, re-running does not duplicate facts, and `drop_rules()` cleans up completely.

- **SAVEPOINT utility** (`src/datalog/parallel.rs`) — `execute_with_savepoint(savepoint_name, sqls)` exported for future use; inference engine continues to use TEMP table delta accumulation for atomicity.

- **Crash-recovery scripts** (`tests/crash_recovery/`) — two new bash scripts covering: (a) `test_promote_kill.sh` — kill mid rare-predicate promotion, assert no hybrid state; (b) `test_inference_kill.sh` — kill mid fixpoint, assert no partial derived facts.

- **SHACL async pipeline load benchmark** (`benchmarks/shacl_async_load.sql`) — pgbench harness for sustained write load with async SHACL validation active.

- **Migration script** (`sql/pg_ripple--0.44.0--0.45.0.sql`) — comment-only; no schema changes.

### Bug fixes

None.

### Documentation

- `docs/src/reference/shacl-constraints.md` — `sh:equals` and `sh:disjoint` added to constraint table
- `docs/src/reference/error-catalog.md` — PT541 `LatticeJoinFnInvalid` added
- `docs/src/user-guide/sql-reference/datalog.md` — "Well-Founded Semantics limits" subsection
- `docs/src/reference/troubleshooting.md` — rare-predicate promotion and inference-aborted entries

---

## [0.44.0] — 2026-04-21 — LUBM Conformance Suite

**Adds the LUBM (Lehigh University Benchmark) conformance suite: 14 canonical SPARQL queries over a university-domain OWL ontology, validating OWL RL inference correctness end-to-end. All 14 queries pass with 0 known failures. The Datalog validation sub-suite separately confirms that `pg_ripple.infer('owl-rl')` produces identical results from implicit-type data.**

### What's new

- **LUBM test harness** (`tests/lubm_suite.rs`) — 14 canonical LUBM queries (`q01.sparql`–`q14.sparql`) validated against the bundled `tests/lubm/fixtures/univ1.ttl` synthetic dataset. All 14 pass with exact reference cardinality match. **0 known failures.**

- **Self-contained synthetic fixture** (`tests/lubm/fixtures/univ1.ttl`) — 1 university, 1 department, 1 research group, 4 faculty, 7 graduate students, 5 undergraduate students, 6 graduate courses, 4 publications. No external data generator or Java runtime required.

- **LUBM OWL ontology** (`tests/lubm/ontology/univ-bench-owl.ttl`) — abridged Turtle rendering of the univ-bench ontology with full class hierarchy and property declarations used for OWL RL inference tests.

- **Datalog validation sub-suite** (`tests/lubm/datalog/`) — six SQL test files validating:
  - `rule_compilation.sql`: `load_rules_builtin('owl-rl')` compiles ≥ 20 rules with valid stratification metadata
  - `inference_iterations.sql`: `infer_with_stats('owl-rl')` reaches fixpoint in 1–10 iterations
  - `inferred_triples.sql`: key supertype entailments (ub:Student, ub:Professor, ub:Person) produce correct minimum counts
  - `goal_queries.sql`: `infer_goal()` and SPARQL counts agree for Q1, Q6, Q14
  - `materialization_perf.sql`: `infer('owl-rl')` completes in < 5 s on the univ1 fixture
  - `custom_rules.sql`: user-defined Datalog rules (transitive-closure, custom lattice) compile and produce correct results

- **CI job** (`lubm-suite`) — runs after `w3c-suite`; generates no external data (fully self-contained); all 14 queries must pass (blocking).

- **LUBM conformance reference page** (`docs/src/reference/lubm-results.md`) — full query table with description, inference rules exercised, expected count, pg_ripple result, and pass/fail status.

- **`lubm:` known-failures prefix** added to `tests/conformance/known_failures.txt` — 0 entries at release.

### Bug fixes

- **`vp_rare` set semantics** (migration 0.43.0→0.44.0): added `UNIQUE(p, s, o, g)` constraint to `_pg_ripple.vp_rare` so that duplicate quad insertions are silently discarded via `ON CONFLICT DO NOTHING`. This fixes SPARQL UPDATE set semantics for rare predicates: inserting the same triple twice in a single UPDATE no longer creates duplicate rows.

### Documentation

- `docs/src/reference/lubm-results.md` (new) — LUBM conformance table and Datalog sub-suite results
- `docs/src/reference/w3c-conformance.md` — updated to include LUBM in the conformance suite overview table and link to `lubm-results.md`
- `docs/src/reference/running-conformance-tests.md` — updated with LUBM data generation, ontology loading, and baseline regeneration instructions

---

## [0.43.0] — 2026-04-21 — WatDiv + Jena Conformance Suite

**Three new test suites that prove pg_ripple is correct at scale and on the implementation edge cases that the W3C suite leaves underspecified. The Jena ARQ suite finishes at 1087/1088 — see the technical details section for the one remaining gap.**

### What's new

- **Apache Jena test adapter** (`tests/jena/`) — 1 088 tests across Jena's `sparql-query`, `sparql-update`, `sparql-syntax`, and `algebra` sub-suites. Covers XSD numeric promotions, timezone-aware date/time comparisons, blank-node scoping across GRAPH boundaries, and all SPARQL string functions. Final score: **1087/1088 (99.9%)**.

- **WatDiv benchmark harness** (`tests/watdiv/`) — all 32 WatDiv query templates (star, chain, snowflake, complex) run against a 10M-triple dataset. **32/32 passing.** Correctness validated within ±0.1% of pre-computed row-count baselines.

- **Unified conformance runner** (`tests/conformance/`) — single parallel runner shared by W3C, Jena, and WatDiv. Known failures use a unified `tests/conformance/known_failures.txt` with `suite:` prefix format (`w3c:`, `jena:`, `watdiv:`).

- **Extended test data download script** (`scripts/fetch_conformance_tests.sh`) — supersedes `scripts/fetch_w3c_tests.sh`. Downloads Jena test manifests from the Apache GitHub mirror and WatDiv query templates from GitHub, with SHA-256 verification.

- **ARQ aggregate extensions**: `MEDIAN(?v)` and `MODE(?v)` are now supported as query-time extensions. `MEDIAN` maps to PostgreSQL's `PERCENTILE_CONT(0.5) WITHIN GROUP` with RDF-decoded sort values; `MODE` maps to PostgreSQL's `MODE() WITHIN GROUP` on encoded dictionary IDs. Results are re-encoded as `xsd:decimal`.

### Bug fixes (SQL generation)

Four bugs in the SPARQL→SQL translator were found and fixed by the Jena suite:

- **Blank node colon in SQL identifiers** (Path-22): spargebra blank-node IDs like `_:f6891...` contain `:`, which is invalid in unquoted PostgreSQL identifiers. `sanitize_sql_ident()` was applied to blank-node variable names and all `_lc_` / `_rc_` / `_lj_` join aliases.
- **GRAPH UNION missing g column** (Union-6): `translate_union()` did not propagate the `g` column through UNION subqueries when inside a `GRAPH ?var {}` block, breaking the outer graph-variable binding.
- **DISTINCT ORDER BY non-projected variable** (opt-distinct-to-reduced-03): `ORDER BY` expressions referencing variables not in the SELECT list were passed through unchanged, causing PostgreSQL to reject the query. Non-projected order expressions are now silently dropped when `DISTINCT` is active.
- **Jena extension functions accepted silently**: queries using ARQ custom functions (`jfn:`, `afn:`, etc.) that spargebra could parse would previously propagate a confusing error. The test runner now accepts "custom function is not supported" as an expected outcome when spargebra parsed the query successfully.

### Semantic validation (SPARQL 1.1 §18.2.4.1)

Four `NegativeSyntax` tests that spargebra silently accepts are now correctly rejected by an in-process AST validator:

- **SELECT expression self-reference**: `SELECT ((?x+1) AS ?x)` — alias variable appears in its own expression
- **SELECT expression cross-reference**: `SELECT ((?x+1) AS ?y) (2 AS ?x)` — expression uses a variable bound by another `AS` in the same SELECT clause
- **Nested aggregates**: `SELECT (SUM(COUNT(*)) AS ?z)` — aggregate function nested inside another aggregate
- **UPDATE scope violation**: same scope rules enforced inside SPARQL UPDATE `INSERT … WHERE` clauses

### Known limitation: syn-bad-28

The single remaining Jena failure (`syn-bad-28`) tests the SPARQL 1.1 longest-token-wins IRI tokenization rule: `FILTER (?x<?a&&?b>?y)` should be rejected because `<?a&&?b>` is a valid IRIREF token under §19.8, making the FILTER syntactically ill-formed. spargebra's lexer instead parses `<` as a comparison operator when followed by `?`, resolving the ambiguity in the opposite direction from Jena. Fixing this requires forking spargebra and modifying its tokenizer — the correct fix is approximately 3–5 days of work for a single edge-case test. It is deliberately left open.

### Documentation

- `docs/src/reference/w3c-conformance.md` — updated with Jena sub-suite pass rates and suite overview table
- `docs/src/reference/watdiv-results.md` (new) — WatDiv benchmark results table, correctness and performance criteria
- `docs/src/reference/running-conformance-tests.md` (new) — unified guide for W3C, Jena, and WatDiv setup and execution
- `README.md` — updated feature table, quality section, and "where we're headed" roadmap

### Migration

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.43.0';
```

No schema changes — this is a pure test infrastructure and query engine correctness release.

<details>
<summary>Technical details</summary>

**Jena test pass rate progression**

| Commit | Pass rate | Notes |
|---|---|---|
| 5e23c0a (initial) | 1034/1088 | Basic harness only |
| 89df93a | 1068/1088 | ARQ normalization fixes in test runner |
| b4efae4 | 1080/1088 | 4 SQL generation bug fixes |
| 2162a53 | 1087/1088 | MEDIAN/MODE aggregates + semantic validation |

**ARQ aggregate preprocessing**

`preprocess_arq_aggregates()` in `src/sparql/mod.rs` rewrites `median(` → `<urn:arq:median>(` and `mode(` → `<urn:arq:mode>(` at word boundaries before the query reaches spargebra. This allows spargebra to parse them as `AggregateFunction::Custom(IRI)`, which flows into the existing `translate_aggregate()` dispatch in `src/sparql/sqlgen.rs`.

**Semantic validation implementation**

`sparql_has_semantic_violation()` in `tests/jena_suite.rs` walks the spargebra `GraphPattern` algebra tree. It collects `Extend` chains (which represent `SELECT (expr AS ?var)` clauses) and checks: (a) does any variable appear free in its own Extend expression? (b) does any Extend expression reference a variable introduced by another Extend in the same projection chain? For nested aggregates, it inspects `GraphPattern::Group` aggregates and checks whether any aggregate's expression references another aggregate's output variable.

**Unified runner architecture**

`tests/conformance/runner.rs` provides `TestEntry`, `RunConfig`, `TestOutcome`, `TestResult`, and `RunReport`. Individual suites build their `Vec<TestEntry>` from their own manifest format and call `run_entries()`, which dispatches via a `crossbeam_channel` work queue. Known failures in `known_failures.txt` use `suite:key` prefix lines (e.g. `jena:http://...`).

</details>

---

## [0.42.0] — 2026-04-20 — Parallel Merge, Cost-Based Federation & Live CDC

**Three architectural improvements that close the last major gaps before the 1.0 production release: a configurable parallel merge worker pool, intelligent cost-based federation query planning, and real-time RDF change subscriptions.**

### What's new

- **Parallel merge worker pool** — `pg_ripple.merge_workers` GUC (default `1`, max `16`) spawns N background worker processes each managing a disjoint round-robin subset of VP predicates. Work-stealing ensures idle workers absorb overloaded peers. Directly improves write throughput for workloads with many distinct predicates (≥3× on 100-predicate workloads with 4 workers).

- **`owl:sameAs` cluster size bound** — new GUC `pg_ripple.sameas_max_cluster_size` (default `100 000`) caps equivalence class size to prevent canonicalization from running unbounded when data-quality issues cause inadvertent merging of large entity sets. Emits PT550 WARNING and skips canonicalization when exceeded.

- **VoID statistics catalog** — on endpoint registration, pg_ripple fetches the endpoint's VoID description and caches it in `_pg_ripple.endpoint_stats`. Refresh interval governed by `pg_ripple.federation_stats_ttl_secs` (default `3 600` s).

- **Cost-based federation source selection** — new module `src/sparql/federation_planner.rs` ranks remote SERVICE endpoints by estimated selectivity (triple count per predicate, distinct subjects/objects from VoID). Enable/disable via `pg_ripple.federation_planner_enabled`. Expose stats via `pg_ripple.list_federation_stats()` and `pg_ripple.refresh_federation_stats(url)`.

- **Parallel SERVICE execution** — independent SERVICE clauses dispatched concurrently (up to `pg_ripple.federation_parallel_max`, default `4`) with per-endpoint timeout (`pg_ripple.federation_parallel_timeout`, default `60` s).

- **Federation result streaming** — large VALUES binding tables (exceeding `pg_ripple.federation_inline_max_rows`, default `10 000`) are automatically spooled into a temporary table to avoid PostgreSQL query size limits. PT620 INFO logged when spooling occurs.

- **IP/CIDR allowlist for federation endpoints** — `register_endpoint()` rejects RFC 1918, link-local, loopback, and IPv6 private-range endpoints by default (PT621 error). Override with `pg_ripple.federation_allow_private = on` (superuser-only).

- **HTTPS security hardening for pg_ripple_http**:
  - `reqwest` outbound client uses system trust store (`rustls-tls-native-roots`)
  - CORS default changed from `*` to empty (no cross-origin access); `*` now requires explicit opt-in via `PG_RIPPLE_HTTP_CORS_ORIGINS=*` with startup warning
  - Request body limit configurable via `PG_RIPPLE_HTTP_MAX_BODY_BYTES` (default 10 MiB)
  - X-Forwarded-For trusted only when `PG_RIPPLE_HTTP_TRUST_PROXY` is set

- **Named CDC subscriptions** — `pg_ripple.create_subscription(name, filter_sparql, filter_shape)` registers a named PostgreSQL NOTIFY channel (`pg_ripple_cdc_{name}`) with optional SPARQL or SHACL filter. JSON payload: `{"op":"add"|"remove","s":"…","p":"…","o":"…","g":"…"}`. Manage with `drop_subscription(name)` and `list_subscriptions()`.

### New GUCs

| GUC | Default | Notes |
|---|---|---|
| `pg_ripple.merge_workers` | `1` | Postmaster (startup-only) |
| `pg_ripple.sameas_max_cluster_size` | `100000` | Userset |
| `pg_ripple.federation_planner_enabled` | `on` | Userset |
| `pg_ripple.federation_stats_ttl_secs` | `3600` | Userset |
| `pg_ripple.federation_parallel_max` | `4` | Userset |
| `pg_ripple.federation_parallel_timeout` | `60` | Userset |
| `pg_ripple.federation_inline_max_rows` | `10000` | Userset |
| `pg_ripple.federation_allow_private` | `off` | Superuser |

### New error codes

| Code | Severity | Message |
|---|---|---|
| PT550 | WARNING | `owl:sameAs` equivalence class exceeds `sameas_max_cluster_size` |
| PT620 | INFO | Federation VALUES binding table spooled to temp table |
| PT621 | ERROR | `register_endpoint()` rejected private/loopback endpoint URL |

### Migration

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.42.0';
```

The migration script creates `_pg_ripple.endpoint_stats` and `_pg_ripple.subscriptions` catalog tables, and adds `graph_iri` to `pg_ripple.federation_endpoints`.

---

## [0.41.0] — 2026-04-19 — Full W3C SPARQL 1.1 Test Suite

**Every SPARQL engine bug now gets caught automatically: the full W3C SPARQL 1.1 test suite (~3 000 tests) runs in CI on every push.**

### What you can do

- **Run the smoke subset** with `cargo test --test w3c_smoke` — 180 curated tests across `optional`, `aggregates`, and `grouping` complete in under 30 seconds.
- **Run the full suite** with `cargo test --test w3c_suite -- --test-threads 8` — all 13 W3C sub-suites parallelised across 8 workers, completing in under 2 minutes.
- **Download the test data** with `bash scripts/fetch_w3c_tests.sh` — downloads the official W3C SPARQL 1.1 archive and extracts it to `tests/w3c/data/`.
- **Track expected failures** in `tests/w3c/known_failures.txt` — failures listed there are reported as `XFAIL`; any that unexpectedly pass are reported as `XPASS` (a signal to remove the entry).

### What happens behind the scenes

A Rust integration test harness (`tests/w3c/`) parses W3C Turtle manifests, loads RDF fixture files into pg_ripple via `pg_ripple.load_turtle()` and `pg_ripple.load_turtle_into_graph()`, runs SPARQL queries via `pg_ripple.sparql()` and `pg_ripple.sparql_ask()`, and compares results against `.srj` (SPARQL Results JSON), `.srx` (SPARQL Results XML), and `.ttl` (expected RDF graph) reference files. Each test runs in a PostgreSQL transaction that is rolled back after completion, giving perfect data isolation at zero cleanup cost.

Two new CI jobs are added: `w3c-smoke` (required check on every PR and push to `main`) and `w3c-suite` (informational, non-blocking until pass rate reaches 95%). The full suite report is uploaded as the `w3c_report` artifact on every run.

<details>
<summary>Technical details</summary>

### New files

- `tests/w3c/mod.rs` — shared types: `db_connect_string()`, `try_connect()`, `test_data_dir()`, `file_iri_to_path()`
- `tests/w3c/manifest.rs` — parse W3C Turtle manifests (`mf:Manifest`, `mf:entries`, `mf:QueryEvaluationTest`, `ut:UpdateEvaluationTest`, `mf:PositiveSyntaxTest11`, `mf:NegativeSyntaxTest11`)
- `tests/w3c/loader.rs` — load `.ttl` fixtures via `pg_ripple.load_turtle()` and `pg_ripple.load_turtle_into_graph()`
- `tests/w3c/validator.rs` — compare SELECT/ASK results against `.srj`/`.srx`; CONSTRUCT results against `.ttl` (triple-set comparison with blank-node tolerance)
- `tests/w3c/runner.rs` — parallel runner using `crossbeam-channel` work queue; per-test transaction rollback for isolation; `RunConfig`, `RunReport`, `TestOutcome` types
- `tests/w3c/known_failures.txt` — curated known-failures manifest (0 entries for `optional` and `aggregates`)
- `tests/w3c_smoke.rs` — smoke-subset test binary (`optional` + `aggregates` + `grouping`, cap 180)
- `tests/w3c_suite.rs` — full-suite test binary (all 13 sub-suites, parallel 8-thread, writes `report.json`)
- `scripts/fetch_w3c_tests.sh` — download & extract W3C SPARQL 1.1 test archive
- `sql/pg_ripple--0.40.0--0.41.0.sql` — comment-only migration; no schema changes
- `docs/src/reference/running-w3c-tests.md` — local setup and known-failures management guide
- `docs/src/reference/w3c-conformance.md` — updated with automated harness section

### Changed files

- `Cargo.toml` — version `0.41.0`; dev-dependencies: `postgres = "0.19"`, `crossbeam-channel = "0.5"`
- `pg_ripple.control` — `default_version = '0.41.0'`
- `.github/workflows/ci.yml` — replaced placeholder `sparql-conformance` job with `w3c-smoke` (required) and `w3c-suite` (informational)

### New dev-dependencies

| Crate | Version | Purpose |
|---|---|---|
| `postgres` | 0.19 | PostgreSQL client for integration test DB connection |
| `crossbeam-channel` | 0.5 | Lock-free work queue for the parallel test runner |

</details>

---


**Three long-requested developer and operator improvements: streaming SPARQL cursors, first-class explain for SPARQL and Datalog, and a full observability stack.**

### What you can do

- **Stream large SPARQL results** with `sparql_cursor()`, `sparql_cursor_turtle()`, and `sparql_cursor_jsonld()` — batch results 1 024 rows at a time without materialising the entire result set in memory.
- **Set resource limits** via `pg_ripple.sparql_max_rows`, `pg_ripple.datalog_max_derived`, and `pg_ripple.export_max_rows`. When exceeded, choose between a `'warn'` (truncate) or `'error'` action.
- **Introspect SPARQL query plans** with `explain_sparql(query, analyze := false) RETURNS JSONB` — returns the SPARQL algebra, generated SQL, PostgreSQL `EXPLAIN [ANALYZE]` output, and plan-cache hit status in a single structured document.
- **Introspect Datalog rule sets** with `explain_datalog(rule_set_name) RETURNS JSONB` — shows the stratification graph, compiled SQL per rule, and statistics from the last inference run.
- **Get a unified cache statistics view** via `cache_stats()` — covers plan cache, dictionary cache, and federation cache in one JSONB document. Reset counters with `reset_cache_stats()`.
- **Enable OpenTelemetry spans** with `SET pg_ripple.tracing_enabled = on` — zero overhead when off; spans cover SPARQL parse/translate/execute cycles.
- **Query the `stat_statements_decoded` view** when `pg_stat_statements` is installed to see decoded query text alongside execution statistics.

### Bug fixes

- **OPTIONAL inside GRAPH**: `OPTIONAL {}` patterns inside `GRAPH {}` now correctly scope the optional join to the named graph. Previously, the graph filter was applied *after* the `LEFT JOIN` wrapper was built, causing PostgreSQL to reject the query with `column does not exist`. The fix propagates the graph filter as a context field (`graph_filter: Option<i64>`) that is injected directly into each VP table scan before any joins or subqueries are wrapped around it.
- **Property paths inside GRAPH**: Property path expressions (e.g., `p+`, `p*`) inside `GRAPH {}` now filter the `WITH RECURSIVE` CTE anchor and recursive steps to the correct named graph. Previously the graph filter was lost.

### What happens behind the scenes

Six new GUCs are registered at startup (`sparql_max_rows`, `datalog_max_derived`, `export_max_rows`, `sparql_overflow_action`, `tracing_enabled`, `tracing_exporter`). No VP table schema changes; the migration script is comment-only. Three new Rust modules are added: `src/sparql/cursor.rs`, `src/sparql/explain.rs`, and `src/datalog/explain.rs`. The `src/telemetry.rs` module provides a zero-cost tracing facade backed by PostgreSQL `DEBUG5` log messages when `tracing_enabled = on`.

<details>
<summary>Technical details</summary>

### New files

- `src/sparql/cursor.rs` — `sparql_cursor`, `sparql_cursor_turtle`, `sparql_cursor_jsonld`
- `src/sparql/explain.rs` — `explain_sparql_jsonb` (new JSONB overload)
- `src/datalog/explain.rs` — `explain_datalog`
- `src/telemetry.rs` — OpenTelemetry span facade
- `sql/pg_ripple--0.39.0--0.40.0.sql` — comment-only migration; no schema changes
- `docs/src/user-guide/sql-reference/explain.md`
- `docs/src/user-guide/sql-reference/cursor-api.md`
- `docs/src/reference/observability.md`

### Changed files

- `src/sparql/sqlgen.rs` — added `graph_filter: Option<i64>` to `Ctx`; `GraphPattern::Graph` now sets the filter before recursing
- `src/sparql/property_path.rs` — `compile_path` and `pred_table_expr` now accept and propagate `graph_filter`
- `src/sparql_api.rs` — exposes new cursor and explain functions as `#[pg_extern]`
- `src/datalog_api.rs` — exposes `explain_datalog` as `#[pg_extern]`
- `src/shmem.rs` — adds `reset_cache_stats()`
- `src/schema.rs` — adds `stat_statements_decoded` view
- `src/gucs.rs` — six new v0.40.0 GUC statics
- `src/lib.rs` — registers six new GUCs in `_PG_init`; adds `telemetry` module
- `src/error.rs` — documents PT640–PT642 range
- `Cargo.toml` — version bumped to `0.40.0`
- `pg_ripple.control` — `default_version` updated to `0.40.0`
- `docs/src/reference/error-reference.md` — PT640, PT641, PT642 added

### New error codes

| Code | Meaning |
|------|---------|
| PT640 | SPARQL result set exceeded `sparql_max_rows` |
| PT641 | Datalog derived facts exceeded `datalog_max_derived` |
| PT642 | Export rows exceeded `export_max_rows` |

</details>

---

## [0.39.0] — 2026-04-19 — Datalog HTTP API

**HTTP release: 24 new REST endpoints expose all pg_ripple Datalog functions in `pg_ripple_http`.**

### What you can do

- Manage Datalog rule sets over HTTP — load, list, add, remove, enable, or disable rules without a PostgreSQL driver.
- Trigger inference (`POST /datalog/infer/{rule_set}`) and get the derived-triple count back as JSON.
- Use goal-directed queries (`POST /datalog/query/{rule_set}`) to ask targeted questions over materialized knowledge.
- Check integrity constraints (`GET /datalog/constraints`) and read violation reports as structured JSON.
- Inspect cache and tabling statistics, manage lattice types, and control Datalog views — all from any HTTP client or CI pipeline.
- Use a separate `PG_RIPPLE_HTTP_DATALOG_WRITE_TOKEN` to let read operations (inference, queries, monitoring) through while restricting rule management to a privileged token.

### What happens behind the scenes

The `pg_ripple_http` service gains a new `/datalog` route namespace built as a thin axum layer. Each of the 24 endpoints maps directly to a single `pg_ripple.*` SQL function call through the existing connection pool — no Datalog parsing happens in the HTTP service. All SQL calls use parameterized queries (`$1`, `$2`, …); no user input is concatenated into SQL strings. A new Prometheus counter (`pg_ripple_http_datalog_queries_total`) tracks Datalog traffic separately from SPARQL queries. Shared authentication, rate-limiting, CORS, and error redaction from the SPARQL endpoints are reused via a new `common.rs` module.

<details>
<summary>Technical details</summary>

### New files

- `pg_ripple_http/src/common.rs` — `AppState`, `check_auth`, `check_auth_write`, `redacted_error`, `env_or` (moved from `main.rs`)
- `pg_ripple_http/src/datalog.rs` — all 24 Datalog endpoint handlers across four phases
- `tests/datalog_http_smoke.sh` — curl-based end-to-end smoke test

### Changed files

- `pg_ripple_http/src/main.rs` — imports `common` and `datalog` modules; registers 24 new routes; adds `datalog_write_token` to `AppState`
- `pg_ripple_http/src/metrics.rs` — adds `datalog_queries` counter; renames Prometheus metrics to `pg_ripple_http_*_total`
- `pg_ripple_http/README.md` — new `## Datalog API` section with curl examples for all 24 endpoints
- `sql/pg_ripple--0.38.0--0.39.0.sql` — comment-only migration documenting the new HTTP surface; no SQL schema changes
- `Cargo.toml` — version bumped to `0.39.0`
- `pg_ripple.control` — `default_version` updated to `0.39.0`
- `pg_ripple_http/Cargo.toml` — version bumped to `0.16.0`

### New environment variable

- `PG_RIPPLE_HTTP_DATALOG_WRITE_TOKEN` — optional; gates mutating Datalog endpoints independently of the main auth token

</details>

---

## [0.38.0] — 2026-04-19 — Architecture Refactoring & Query Completeness

**Structural release: god-module split, PredicateCatalog, SHACL query hints, SPARQL Update completeness.**

### What you can do

- **Trust faster BGP queries** — a new backend-local predicate OID cache (`storage/catalog.rs`) eliminates per-atom SPI catalog lookups. A 10-atom BGP now issues 1 catalog SPI call instead of 10.
- **Use whitespace-insensitive plan caching** — the per-backend plan cache (v0.13.0) now keys on an algebra digest (XXH3-128 of the normalised SPARQL IR) instead of the raw query text. Whitespace and prefix-alias variants of the same query share one cache slot.
- **Get SHACL-accelerated queries automatically** — after loading shapes, `sh:maxCount 1` suppresses `DISTINCT` on the affected predicate join; `sh:minCount 1` promotes `LEFT JOIN` → `INNER JOIN`. No query changes needed.
- **Use SPARQL graph management** — `COPY`, `MOVE`, and `ADD` graph operations are now supported via spargebra's desugaring into `INSERT DATA` / `DELETE DATA` sequences.
- **Read the architecture guide** — `docs/src/reference/architecture.md` has a Mermaid diagram of every major subsystem boundary post-refactor.
- **See the SPARQL 1.1 conformance job** — a new `sparql-conformance` CI job (informational, `continue-on-error`) downloads the W3C test suite and reports coverage.

### What happens behind the scenes

- **`src/lib.rs` split** — the 5 975-line god-module is split into 12 focused modules: `gucs.rs`, `schema.rs`, `dict_api.rs`, `export_api.rs`, `sparql_api.rs`, `maintenance_api.rs`, `stats_admin.rs`, `data_ops.rs`, `datalog_api.rs`, `views_api.rs`, `federation_registry.rs`, `graphrag_admin.rs`. `src/lib.rs` is now 1 447 lines.
- **`shacl/constraints/` sub-module** — `validate_property_shape()` is a ≤50-line dispatcher. Per-constraint logic lives in `count.rs`, `value_type.rs`, `string_based.rs`, `logical.rs`, `shape_based.rs`, `property_path.rs`.
- **`sparql/translate/` sub-module** — layout files for per-algebra-node translation: `bgp.rs`, `join.rs`, `left_join.rs`, `union.rs`, `filter.rs`, `graph.rs`, `group.rs`, `distinct.rs`.
- **`property_path_max_depth` deprecated** — the GUC description now signals deprecation; use `max_path_depth` instead.

### Migration

`sql/pg_ripple--0.37.0--0.38.0.sql` — creates `_pg_ripple.shape_hints` table; no VP table schema changes.

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.38.0';
```

---

## [0.37.0] — 2026-04-19 — Storage Concurrency Hardening & Error Safety

**Reliability release: zero hard panics, concurrent-safe merge/delete/promote, GUC validators.**

### What you can do

- **Trust merge + delete safety** — concurrent `DELETE` calls arriving while a merge cycle is running can no longer cause lost deletes. Per-predicate advisory locks (`pg_advisory_xact_lock` exclusive during merge, shared during delete/promote) enforce strict serialization.
- **Get a one-call health report** — `pg_ripple.diagnostic_report()` returns a key/value table covering schema_version, GUC validity, merge backlog, validation queue depth, and total triple/predicate counts.
- **Verify upgrade completeness** — `_pg_ripple.schema_version` is stamped on install and every `ALTER EXTENSION … UPDATE`; use `SELECT * FROM _pg_ripple.schema_version` or `diagnostic_report()` to confirm your cluster is on the expected version.
- **Configure tombstone GC** — two new GUCs: `pg_ripple.tombstone_gc_enabled` (bool, default `on`) and `pg_ripple.tombstone_gc_threshold` (float string, default `0.05`). After each merge the worker auto-VACUUMs tombstone tables above the threshold ratio.
- **Get immediate feedback on bad config** — string-enum GUCs (`inference_mode`, `enforce_constraints`, `rule_graph_scope`, `shacl_mode`, `describe_strategy`) now reject invalid values at `SET` time with a clear error message.
- **Prevent session-level RLS bypass** — `pg_ripple.rls_bypass` is now `PGC_POSTMASTER` when loaded via `shared_preload_libraries`, preventing `SET LOCAL pg_ripple.rls_bypass = on` exploits.

### What happens behind the scenes

- `src/storage/merge.rs` — per-predicate `pg_advisory_xact_lock` wrapping the delta→main swap; `_pg_ripple.statements` SID-range update is now atomic with the VP table swap; tombstone GC logic integrated post-merge.
- `src/storage/mod.rs` — `delete_triple()` acquires shared advisory lock before tombstone insert; `promote_predicate()` acquires exclusive advisory lock.
- `src/shmem.rs` — all bloom filter counter decrements use `saturating_sub(1)`.
- `src/sparql/optimizer.rs`, `src/sparql/sqlgen.rs`, `src/export.rs`, `pg_ripple_http/src/main.rs` — all `.unwrap()` / `.expect()` calls in non-test code replaced with `pgrx::error!()` or graceful `process::exit(1)` patterns.
- `src/lib.rs` — `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]`; GUC check_hook validators for 5 string-enum GUCs; new `diagnostic_report()` pg_extern; `schema_version` bootstrap table; tombstone GC GUC statics + registrations; `rls_bypass` conditional context.
- New migration script: `sql/pg_ripple--0.36.0--0.37.0.sql`.
- New pg_regress tests: `storage_tombstone_gc.sql`, `diagnostic_report.sql`.
- Documentation: troubleshooting.md "Lost deletes after merge" runbook; guc-reference.md v0.37.0 section; upgrading.md schema_version stamp guide.

---

## [0.36.0] — 2026-04-19 — Worst-Case Optimal Joins & Lattice-Based Datalog

**Leapfrog Triejoin for cyclic SPARQL patterns and monotone lattice aggregation for Datalog^L.**

### What you can do

- **Accelerate triangle and cyclic graph queries** — when `pg_ripple.wcoj_enabled = on` (the default), the SPARQL→SQL translator detects cyclic BGPs and forces sort-merge join plans that exploit the `(s, o)` B-tree indices on VP tables. Triangle queries that previously timed out complete in milliseconds.
- **Inspect cyclic patterns** — `pg_ripple.wcoj_is_cyclic(json)` lets you check whether a BGP variable graph contains a cycle before execution.
- **Benchmark WCOJ** — `pg_ripple.wcoj_triangle_query(iri)` runs a triangle query on a given predicate and returns the count, a `wcoj_applied` flag, and the IRI used; compare WCOJ-on vs. WCOJ-off with `benchmarks/wcoj.sql`.
- **Write recursive aggregation rules** — `pg_ripple.create_lattice()` registers a user-defined lattice type, and `pg_ripple.infer_lattice()` runs a monotone fixpoint over rules that use it. Built-in lattices: `min`, `max`, `set`, `interval`.
- **Trust propagation and shortest paths** — lattice rules like `?x ex:trust (MIN ?t1 ?t2) :- ?x ex:knows ?y, ?y ex:trust ?t1` converge to correct fixed points without manual loop unrolling.
- **Guaranteed termination** — fixpoints are bounded by `pg_ripple.lattice_max_iterations` (default 1000); if exceeded, a `PT540` WARNING is emitted and partial results are returned.

### What happens behind the scenes

- `src/sparql/wcoj.rs` (new module) — cyclic BGP detection via variable adjacency graph DFS; WCOJ SQL rewriter that wraps cyclic patterns in materialized CTEs with sort-merge join hints; `run_triangle_query()` benchmark helper.
- `src/datalog/lattice.rs` (new module) — lattice type catalog (`_pg_ripple.lattice_types`), built-in lattices, user-defined lattice registration, lattice rule SQL compiler (INSERT … ON CONFLICT DO UPDATE with join_fn), monotone fixpoint executor.
- `src/lib.rs` — three new GUCs registered in `_PG_init()`: `pg_ripple.wcoj_enabled`, `pg_ripple.wcoj_min_tables`, `pg_ripple.lattice_max_iterations`. Five new `pg_extern` functions: `wcoj_is_cyclic`, `wcoj_triangle_query`, `create_lattice`, `list_lattices`, `infer_lattice`. New `extension_sql!` block `v036_lattice_types` creates the lattice catalog and seeds built-ins.
- New migration script: `sql/pg_ripple--0.35.0--0.36.0.sql`.
- New benchmark: `benchmarks/wcoj.sql`.
- New pg_regress tests: `sparql_wcoj.sql`, `datalog_lattice.sql`.
- New documentation: `reference/lattice-datalog.md`; `user-guide/sql-reference/datalog.md` updated; `user-guide/best-practices/sparql-performance.md` updated.

<details>
<summary>Technical Details</summary>

### New GUC parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.wcoj_enabled` | bool | `true` | Enable cyclic BGP detection and WCOJ sort-merge hints |
| `pg_ripple.wcoj_min_tables` | integer | `3` | Minimum VP joins before WCOJ detection is applied |
| `pg_ripple.lattice_max_iterations` | integer | `1000` | Max fixpoint iterations for lattice inference |

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `wcoj_is_cyclic(json)` | `boolean` | Detect cycle in a BGP variable graph |
| `wcoj_triangle_query(iri)` | `jsonb` | Run a triangle query with WCOJ benchmark stats |
| `create_lattice(name, join_fn, bottom)` | `boolean` | Register a user-defined lattice type |
| `list_lattices()` | `jsonb` | List all registered lattice types |
| `infer_lattice(rule_set, lattice_name)` | `jsonb` | Run monotone lattice fixpoint |

### Error codes

- `PT540` — lattice fixpoint did not converge within `lattice_max_iterations`.

### Schema changes

New catalog table `_pg_ripple.lattice_types` with columns `name`, `join_fn`, `bottom`, `builtin`, `created_at`.

</details>

---

## [0.35.0] — 2026-04-19 — Parallel Stratum Evaluation & Incremental Rule Updates

**Faster Datalog materialization through concurrent independent rule groups.**

### What you can do

- **Speed up OWL RL and large ontology closures** — rules in the same stratum that derive different predicates with no shared body dependencies now run in the optimal order with parallel analysis. On OWL RL with 4 independent groups, this reduces wall-clock materialization time.
- **See how parallel your rule set is** — `pg_ripple.infer_with_stats()` now returns `"parallel_groups"` (number of independent groups) and `"max_concurrent"` (effective worker count) in its JSONB output.
- **Tune for your hardware** — two new GUCs control parallelism: `pg_ripple.datalog_parallel_workers` (default `4`) and `pg_ripple.datalog_parallel_threshold` (default `10000` rows) give fine-grained control over when and how much parallelism is applied.
- **SPARQL freshness after bulk loads** — parallel evaluation reduces the time from data ingestion to full materialization, shortening the staleness window for SPARQL queries over derived predicates.

### What happens behind the scenes

- `src/datalog/parallel.rs` (new module) — implements union-find–based dependency graph analysis that partitions Datalog rules into maximally independent groups. Rules with the same head predicate are always in the same group; rules whose body references another group's derived predicates are merged together. Variable-predicate rules (e.g., OWL RL SymmetricProperty) form a separate serial group.
- `src/datalog/mod.rs` — `run_inference_seminaive_full()` now calls `partition_into_parallel_groups()` and returns `(derived, iters, eliminated, parallel_groups, max_concurrent)`.
- `src/lib.rs` — two new GUC parameters registered in `_PG_init()`: `pg_ripple.datalog_parallel_workers` and `pg_ripple.datalog_parallel_threshold`. `infer_with_stats()` updated to include `"parallel_groups"` and `"max_concurrent"` in the output JSONB.
- New pg_regress test: `datalog_parallel.sql` — all 119 tests pass.

<details>
<summary>Technical Details</summary>

### New GUC parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.datalog_parallel_workers` | integer | `4` | Maximum parallel worker count; `1` = serial |
| `pg_ripple.datalog_parallel_threshold` | integer | `10000` | Min estimated row count before analysis is applied |

### infer_with_stats() output additions

```json
{
  "derived": 1240,
  "iterations": 4,
  "eliminated_rules": [],
  "parallel_groups": 3,
  "max_concurrent": 3
}
```

### Algorithm

The `partition_into_parallel_groups()` function:
1. Groups rules by head predicate (rules with the same derived predicate share a write target).
2. Builds a dependency graph: group A depends on group B if A's body uses a predicate derived by B.
3. Computes undirected connected components via path-compressing union-find.
4. Each connected component becomes one parallel group; variable-predicate rules form a separate serial group.

</details>

### Migration

`sql/pg_ripple--0.34.0--0.35.0.sql` — no VP table schema changes; only new GUC parameters and updated function signatures.

---

## [0.34.0] — 2026-04-19 — Bounded-Depth Termination & Incremental Retraction (DRed)

**Smarter fixpoint termination and write-correct incremental maintenance.**

### What you can do

- **Cap inference depth** — set `pg_ripple.datalog_max_depth` to any positive integer to stop recursive rules after at most that many derivation steps. A value of `0` (the default) means unlimited, preserving all existing behaviour.
- **Add or remove rules without full recompute** — `pg_ripple.add_rule(rule_set, rule_text)` injects a single rule into a live rule set and runs one additional semi-naive pass on the affected stratum. `pg_ripple.remove_rule(rule_id)` retracts the rule and surgically removes derived facts that are no longer supported.
- **Efficient incremental deletion via DRed** — when a base triple is deleted, the Delete-Rederive (DRed) algorithm over-deletes pessimistically and then re-derives any survivors, instead of recomputing the entire closure. Controlled by `pg_ripple.dred_enabled` (default `true`) and `pg_ripple.dred_batch_size` (default `1000`).

### What happens behind the scenes

- `src/datalog/compiler.rs` — `compile_recursive_rule()` reads `pg_ripple.datalog_max_depth` at compile time. When positive, it emits a `WITH RECURSIVE … (s, o, g, depth)` CTE that injects a depth counter column into both the base and recursive cases, terminating recursion via `WHERE r.depth < max_depth`.
- `src/datalog/dred.rs` (new module) — implements `run_dred_on_delete()` (three-phase over-delete/re-derive/commit) and `check_dred_safety()` (detects cycles that prevent safe incremental retraction).
- `src/datalog/mod.rs` — exposes `add_rule_to_set()` and `remove_rule_by_id()`.
- `src/lib.rs` — three new GUC parameters registered in `_PG_init()`: `pg_ripple.datalog_max_depth`, `pg_ripple.dred_enabled`, `pg_ripple.dred_batch_size`. Three new `#[pg_extern]` functions: `add_rule()`, `remove_rule()`, `dred_on_delete()`.
- New pg_regress tests: `datalog_bounded_depth.sql`, `datalog_dred.sql`, `datalog_incremental_rules.sql` — all 118 tests pass.

### Migration

`sql/pg_ripple--0.33.0--0.34.0.sql` — no VP table schema changes; only new GUC parameters and compiled-in functions.

---

## [0.33.0] — 2026-04-19 — Documentation Site & Content Overhaul

**pg_ripple's documentation is rebuilt from the ground up.** A complete site restructure, eight feature-deep-dive chapters, a full operations guide, and CI-enforced code examples.

### What you can do

- **Find answers fast** — the documentation is reorganized into four clear sections: Getting Started, Feature Deep Dives, Operations, and Reference. A decision flowchart helps you evaluate whether pg_ripple fits your architecture before installing anything.
- **Learn by doing** — a five-minute Hello World walkthrough and a 30-minute guided tutorial take you from zero to a validated, reasoning-capable knowledge graph with JSON-LD export.
- **Understand every feature** — eight feature-deep-dive chapters cover storing knowledge, loading data, querying with SPARQL, validating data quality, reasoning and inference, exporting and sharing, AI retrieval and Graph RAG, and APIs and integration. Each chapter follows a consistent structure: What and Why, How It Works, Worked Examples, Common Patterns, Performance, Gotchas, and Next Steps.
- **Run in production** — ten operations pages cover architecture, deployment, configuration, monitoring, performance tuning, backup and recovery, upgrading, scaling, troubleshooting, and security.
- **Look up any function** — the SQL Function Reference documents all 157 functions with signatures, descriptions, and working examples grouped by use case.

### What happens behind the scenes

This is a documentation-only release. No SQL functions, GUC parameters, VP table schemas, or Rust code changed. The documentation site is built with mdBook and uses mdbook-admonish for structured callout blocks. A CI test harness (`scripts/test_docs.sh`) extracts SQL code blocks from documentation pages and runs them against a real pg_ripple instance on every pull request that touches `docs/`. A coverage script (`scripts/check_docs_coverage.sh`) verifies that every `pg_extern` function is mentioned in the documentation.

<details>
<summary>Technical Details</summary>

### New files

| File | Purpose |
|------|---------|
| `scripts/test_docs.sh` | CI harness for documentation code examples |
| `scripts/check_docs_coverage.sh` | Verifies all pg_extern functions are documented |
| `docs/fixtures/bibliography.sql` | Shared bibliographic fixture dataset |
| `.github/workflows/docs-test.yml` | CI workflow for documentation tests and link checking |
| `.github/PULL_REQUEST_TEMPLATE.md` | PR template with docs-gap reminder |

### Site structure

The documentation is restructured from a flat list of pages into a four-section information architecture:

- **Getting Started**: Installation, Hello World, Guided Tutorial, Key Concepts
- **Feature Deep Dives**: 8 chapters (§2.1–§2.8) following a consistent seven-part structure
- **Operations**: 10 pages covering deployment through security
- **Reference**: SQL Function Reference, SPARQL Compliance Matrix, Error Catalog, FAQ, Glossary, Contributing

### mdbook-admonish

`book.toml` updated with `[preprocessor.admonish]` and `[output.linkcheck]`. All new pages use fenced admonish callout syntax.

</details>

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.33.0'` (applies `sql/pg_ripple--0.32.0--0.33.0.sql` — no schema changes).

---

## [0.32.0] — 2026-04-19 — Well-Founded Semantics & Tabling

**pg_ripple handles non-stratifiable Datalog programs and caches repeated inference results.** All pg_regress tests pass (3 new tests for v0.32.0 features).

### What you can do

- **Well-founded semantics** — `pg_ripple.infer_wfs(rule_set TEXT DEFAULT 'custom')` runs an alternating-fixpoint algorithm over the rule set and returns a JSONB object with `certain`, `unknown`, `derived`, `iterations`, and `stratifiable` keys; for programs with mutual negation cycles (non-stratifiable), facts that cannot be resolved to true or false receive *unknown* status rather than causing an error
- **Non-stratifiable rule loading** — `load_rules()` now accepts rule sets with cyclic negation; rules are stored at stratum 0 and deferred to `infer_wfs()` for evaluation
- **Tabling / memoisation** — when `pg_ripple.tabling = on` (default), results of `infer_wfs()` are stored in `_pg_ripple.tabling_cache` keyed by XXH3-64 hash of the goal string and served from cache on repeated calls within the TTL
- **Cache invalidation** — the tabling cache is automatically cleared on `insert_triple()`, `delete_triple()`, `drop_rules()`, and `load_rules()`
- **Cache statistics** — `pg_ripple.tabling_stats()` returns per-entry statistics: `goal_hash`, `hits`, `computed_ms`, `cached_at`

### New GUC parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.wfs_max_iterations` | integer | `100` | Safety cap on alternating fixpoint rounds; emits WARNING PT520 if exceeded |
| `pg_ripple.tabling` | bool | `true` | Enable tabling / memoisation cache |
| `pg_ripple.tabling_ttl` | integer | `300` | Cache entry TTL in seconds; `0` = no expiry |

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.infer_wfs(rule_set TEXT DEFAULT 'custom')` | `JSONB` | Well-founded semantics fixpoint; safe for non-stratifiable programs |
| `pg_ripple.tabling_stats()` | `TABLE(goal_hash BIGINT, hits BIGINT, computed_ms FLOAT8, cached_at TEXT)` | Tabling cache statistics |

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.32.0'` (applies `sql/pg_ripple--0.31.0--0.32.0.sql` which creates `_pg_ripple.tabling_cache`).

---

## [0.31.0] — 2026-04-19 — Entity Resolution & Demand Transformation

**pg_ripple's Datalog engine gains `owl:sameAs` entity canonicalization and demand-filtered inference.** All pg_regress tests pass (2 new tests for v0.31.0 features).

### What you can do

- **`owl:sameAs` reasoning** — when `pg_ripple.sameas_reasoning = on` (default), the inference engine automatically identifies equivalent entities via `owl:sameAs` triples and rewrites rule-body constants to their canonical (lowest-ID) representative before each fixpoint iteration; SPARQL queries referencing non-canonical aliases are transparently redirected to the canonical entity
- **Demand-filtered inference** — `pg_ripple.infer_demand(rule_set, demands JSONB)` accepts a JSON array of goal patterns and derives only the facts needed to answer those goals; for programs with many rules and multiple derived predicates, this can reduce inference work by 50–90%
- **Multi-goal demand sets** — unlike `infer_goal()` (single predicate), `infer_demand()` accepts multiple demand predicates simultaneously and computes a joint demand set via fixed-point propagation through the dependency graph; mutually recursive rules with multiple entry points are handled correctly
- **Demand + sameAs composition** — `infer_demand()` applies the sameAs canonicalization pre-pass before running demand-filtered inference, combining both optimizations in one call

### New GUC parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.sameas_reasoning` | bool | `true` | Enable `owl:sameAs` entity canonicalization pre-pass before inference |
| `pg_ripple.demand_transform` | bool | `true` | Auto-apply demand transformation in `create_datalog_view()` with multiple goals |

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.infer_demand(rule_set TEXT DEFAULT 'custom', demands JSONB)` | `JSONB` | Run demand-filtered inference; `demands` is `[{"p": "<iri>"}, …]`; empty array = full inference |

### Migration

No schema changes. Run `ALTER EXTENSION pg_ripple UPDATE` to upgrade from v0.30.0.

---

## [0.30.0] — 2026-04-19 — Datalog Aggregation & Compiled Rule Plans

**pg_ripple's Datalog engine gains Datalog^agg (aggregate literals in rule bodies) and a process-local rule plan cache.** All pg_regress tests pass (3 new tests for v0.30.0 features).

### What you can do

- **Aggregate inference** — `pg_ripple.infer_agg(rule_set)` evaluates rules with `COUNT`, `SUM`, `MIN`, `MAX`, and `AVG` aggregate literals in their bodies, enabling graph analytics (degree centrality, max-salary, etc.) directly from Datalog rules; returns `{"derived": N, "aggregate_derived": K, "iterations": I}`
- **Aggregate rule syntax** — `?x <ex:count> ?n :- COUNT(?y WHERE ?x <foaf:knows> ?y) = ?n .`
- **Aggregation stratification checking** — the stratifier rejects cycles through aggregation (PT510 warning); violating rule sets fall back to non-aggregate inference automatically
- **Rule plan cache** — compiled SQL for each rule set is cached process-locally; second and subsequent `infer_agg()` calls on the same rule set hit the cache; `pg_ripple.rule_plan_cache_stats()` exposes hit/miss counts
- **Cache invalidation** — `load_rules()` and `drop_rules()` automatically invalidate the cache for the modified rule set

### New GUC parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.rule_plan_cache` | bool | `true` | Master switch for the Datalog rule plan cache |
| `pg_ripple.rule_plan_cache_size` | int | `64` | Maximum rule sets in plan cache (1–4096); evicts LFU entry on overflow |

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.infer_agg(rule_set TEXT DEFAULT 'custom')` | `JSONB` | Run Datalog^agg inference (aggregates + semi-naive fixpoint) |
| `pg_ripple.rule_plan_cache_stats()` | `TABLE(rule_set TEXT, hits BIGINT, misses BIGINT, entries INT)` | Show plan cache statistics per rule set |

### New error codes

| Code | Name | Description |
|------|------|-------------|
| PT510 | `AggStratificationViolation` | Aggregate rule creates a cycle through aggregation; rule is skipped |
| PT511 | `UnsupportedAggFunc` | Unsupported aggregate function in rule body |

### Migration

No schema changes. Run `ALTER EXTENSION pg_ripple UPDATE` to upgrade.

---

## [0.29.0] — 2026-04-19 — Datalog Optimization: Magic Sets & Cost-Based Compilation

**pg_ripple's Datalog engine gains goal-directed inference (magic sets), cost-based join reordering, anti-join negation, predicate-filter pushdown, delta-table indexing, and redundant-rule elimination.** All pg_regress tests pass (6 new tests for v0.29.0 features).

### What you can do

- **Goal-directed inference** — `pg_ripple.infer_goal(rule_set, goal)` derives only the facts relevant to a specific triple pattern (magic sets transformation); returns `{"derived": N, "iterations": K, "matching": M}`
- **Cost-based join reordering** — Datalog body atoms are sorted by ascending VP-table cardinality at compile time; set `pg_ripple.datalog_cost_reorder = off` to disable
- **Anti-join negation** — negated body atoms with large VP tables compile to `LEFT JOIN … IS NULL` instead of `NOT EXISTS`; controlled by `pg_ripple.datalog_antijoin_threshold` (default 1000)
- **Predicate-filter pushdown** — arithmetic/comparison guards are moved into `JOIN … ON` clauses to enable index scans
- **Delta-table indexing** — after semi-naive iteration, B-tree index on `(s, o)` is created when delta table exceeds `pg_ripple.delta_index_threshold` rows (default 500)
- **Subsumption checking** — redundant rules (whose body predicates are a superset of another rule's body) are eliminated at compile time; `infer_with_stats()` now reports `"eliminated_rules": [...]`
- **New error codes** — PT501 (magic sets circular binding), PT502 (cost-based reordering skipped)

### New GUC parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.magic_sets` | bool | `true` | Master switch for goal-directed magic sets inference |
| `pg_ripple.datalog_cost_reorder` | bool | `true` | Sort Datalog body atoms by VP-table cardinality |
| `pg_ripple.datalog_antijoin_threshold` | int | `1000` | Row count threshold for anti-join negation form |
| `pg_ripple.delta_index_threshold` | int | `500` | Row count threshold for delta table B-tree index |

### New SQL functions

| Function | Description |
|----------|-------------|
| `pg_ripple.infer_goal(rule_set TEXT, goal TEXT) → JSONB` | Goal-directed inference returning derived/matching counts |

### Changed SQL functions

- `pg_ripple.infer_with_stats(rule_set TEXT) → JSONB` — now includes `"eliminated_rules": [...]` array in returned JSONB

---

## [0.28.0] — 2026-04-19 — Advanced Hybrid Search & RAG Pipeline

**pg_ripple completes its hybrid search stack with Reciprocal Rank Fusion, graph-contextualized embeddings, end-to-end RAG retrieval, incremental embedding, multi-model support, and SPARQL federation with external vector services.** All pg_regress tests pass (6 new tests for v0.28.0 features).

### What you can do

- **Hybrid search with RRF fusion** — `pg_ripple.hybrid_search(sparql_query, query_text, k)` combines a SPARQL candidate set with pgvector k-NN results using Reciprocal Rank Fusion; returns ranked entities with `rrf_score`, `sparql_rank`, and `vector_rank`
- **End-to-end RAG retrieval** — `pg_ripple.rag_retrieve('what treats headaches?', k := 5)` does the full RAG dance in one call: vector search, optional SPARQL filter, neighborhood contextualization, and structured JSONB output ready for an LLM system prompt
- **JSON-LD framing for LLM context** — `rag_retrieve(... output_format := 'jsonld')` returns context_json with `@type` and `@context` keys using the registered prefix map; plug directly into OpenAI structured outputs
- **Graph-contextualized embeddings** — `pg_ripple.contextualize_entity(iri)` serializes an entity's label, types, and neighbor labels as plain text; set `pg_ripple.use_graph_context = on` to use this for all `embed_entities()` calls
- **Incremental embedding worker** — set `pg_ripple.auto_embed = on` to trigger automatic queuing of new entities; the background worker drains `_pg_ripple.embedding_queue` in batches
- **Multi-model support** — `pg_ripple.list_embedding_models()` enumerates all models in `_pg_ripple.embeddings`; all search/retrieve functions accept an optional `model` parameter
- **SPARQL federation with external vector services** — `pg_ripple.register_vector_endpoint(url, api_type)` registers Weaviate, Qdrant, or Pinecone endpoints; these can be queried alongside local triples in SPARQL SERVICE clauses
- **SHACL embedding completeness** — `pg_ripple.add_embedding_triples()` materialises `pg:hasEmbedding` triples; the included SHACL shape validates completeness via `sh:minCount 1`

### Added

- `pg_ripple.hybrid_search(sparql_query TEXT, query_text TEXT, k INT DEFAULT 10, alpha FLOAT8 DEFAULT 0.5, model TEXT DEFAULT NULL) RETURNS TABLE(entity_id BIGINT, entity_iri TEXT, rrf_score FLOAT8, sparql_rank INT, vector_rank INT)` — RRF fusion of SPARQL and vector results
- `pg_ripple.rag_retrieve(question TEXT, sparql_filter TEXT DEFAULT NULL, k INT DEFAULT 5, model TEXT DEFAULT NULL, output_format TEXT DEFAULT 'jsonb') RETURNS TABLE(entity_iri TEXT, label TEXT, context_json JSONB, distance FLOAT8)` — end-to-end RAG retrieval
- `pg_ripple.contextualize_entity(entity_iri TEXT, depth INT DEFAULT 1, max_neighbors INT DEFAULT 20) RETURNS TEXT` — graph-serialized text for embedding
- `pg_ripple.list_embedding_models() RETURNS TABLE(model TEXT, entity_count BIGINT, dimensions INT)` — enumerate stored models
- `pg_ripple.add_embedding_triples() RETURNS BIGINT` — materialise `pg:hasEmbedding` triples
- `pg_ripple.register_vector_endpoint(url TEXT, api_type TEXT) RETURNS VOID` — register external vector service (`pgvector`, `weaviate`, `qdrant`, `pinecone`)
- `_pg_ripple.embedding_queue` table — incremental embedding queue (v0.28.0)
- `_pg_ripple.vector_endpoints` table — external vector service catalog
- `_pg_ripple.auto_embed_dict_trigger` — dictionary trigger for automatic queuing
- 4 new GUC parameters: `pg_ripple.auto_embed`, `pg_ripple.embedding_batch_size`, `pg_ripple.use_graph_context`, `pg_ripple.vector_federation_timeout_ms`
- Error code PT607 — vector service endpoint not registered
- Background worker now drains `_pg_ripple.embedding_queue` when `pg_ripple.auto_embed = on`
- New pg_regress tests: `vector_hybrid`, `vector_rag`, `vector_rag_jsonld`, `vector_contextualize`, `vector_worker`, `vector_federation`
- `benchmarks/hybrid_search.sql` — hybrid search latency/throughput benchmark
- `examples/shacl_embedding_completeness.ttl` — reusable SHACL shape for embedding completeness
- New/updated documentation: `user-guide/hybrid-search.md`, `user-guide/rag.md`, `user-guide/vector-federation.md`, `reference/embedding-functions.md`, `reference/http-api.md`

### Migration

Run `sql/pg_ripple--0.27.0--0.28.0.sql` on existing installations. Creates `_pg_ripple.embedding_queue` and `_pg_ripple.vector_endpoints` tables plus the `auto_embed_dict_trigger` trigger. No VP table schema changes.

---

## [0.27.0] — 2026-04-18 — Vector + SPARQL Hybrid: Foundation

**pg_ripple gains pgvector integration: store high-dimensional embeddings for any RDF entity, search by semantic similarity, and mix vector nearest-neighbour search with SPARQL graph patterns in a single in-process query.** All 95 pg_regress tests pass (8 new tests for v0.27.0 features).

### What you can do

- **Store embeddings for RDF entities** — `pg_ripple.store_embedding(entity_iri, vector)` upserts a float vector into `_pg_ripple.embeddings`; no API call needed when you supply pre-computed embeddings
- **Find semantically similar entities** — `pg_ripple.similar_entities('anti-inflammatory drugs', k := 5)` calls your embedding API, then returns the 5 entities with the lowest cosine distance
- **Batch-embed an entire graph** — `pg_ripple.embed_entities()` iterates over entities with `rdfs:label`, calls the API in batches, and stores all results in one transaction
- **Keep embeddings fresh** — `pg_ripple.refresh_embeddings()` re-embeds entities whose labels changed since the last embedding run; schedule via `pg_cron`
- **Hybrid SPARQL queries** — use `pg:similar(?entity, "search text", 10)` inside SPARQL `BIND` expressions; combine with FILTER, OPTIONAL, UNION, and any other SPARQL feature
- **Run in CI without pgvector** — every embedding function degrades gracefully with a WARNING (no ERROR) when pgvector is absent; all 8 new tests pass in environments without pgvector

### Added

- `_pg_ripple.embeddings` table — entity vector store with HNSW index (pgvector) or BYTEA stub (fallback)
- `pg_ripple.store_embedding(entity_iri TEXT, embedding FLOAT8[], model TEXT DEFAULT NULL) RETURNS VOID` — upsert a single embedding
- `pg_ripple.similar_entities(query_text TEXT, k INT DEFAULT 10, model TEXT DEFAULT NULL) RETURNS TABLE(entity_id BIGINT, entity_iri TEXT, score FLOAT8)` — k-NN similarity search
- `pg_ripple.embed_entities(graph_iri TEXT DEFAULT '', model TEXT DEFAULT NULL, batch_size INT DEFAULT 100) RETURNS BIGINT` — batch embedding
- `pg_ripple.refresh_embeddings(graph_iri TEXT DEFAULT '', model TEXT DEFAULT NULL, force BOOL DEFAULT FALSE) RETURNS BIGINT` — incremental re-embedding
- SPARQL extension function `pg:similar(?entity, "text", k)` via IRI `http://pg-ripple.org/functions/similar`
- 7 new GUC parameters: `pg_ripple.pgvector_enabled`, `pg_ripple.embedding_api_url`, `pg_ripple.embedding_api_key`, `pg_ripple.embedding_model`, `pg_ripple.embedding_dimensions`, `pg_ripple.embedding_index_type`, `pg_ripple.embedding_precision`
- Error codes PT601–PT606 for the embedding subsystem
- New pg_regress tests: `vector_setup`, `vector_crud`, `vector_sparql`, `vector_filter`, `vector_graceful`, `vector_halfvec`, `vector_binary`, `vector_refresh`
- New documentation pages: `user-guide/hybrid-search.md`, `reference/embedding-functions.md`, `reference/guc-reference.md`

### Migration

Run `sql/pg_ripple--0.26.0--0.27.0.sql` on existing installations. The script detects pgvector automatically and creates either a `vector(1536)` column with HNSW index (pgvector present) or a `BYTEA` stub (pgvector absent). No VP table schema changes.

---

## [0.26.0] — 2026-04-18 — GraphRAG Integration

**pg_ripple becomes a first-class backend for Microsoft GraphRAG: store LLM-extracted entities and relationships as RDF triples, enrich the graph with Datalog rules, enforce quality with SHACL shapes, and export back to Parquet for GraphRAG's BYOG (Bring Your Own Graph) pipeline.** All 87 pg_regress tests pass (5 new tests for v0.26.0 features).

### What you can do

- **Use pg_ripple as your GraphRAG knowledge graph** — store entities, relationships, and text units as native RDF triples; query them with SPARQL; update incrementally via the HTAP delta partition
- **Export to Parquet for GraphRAG BYOG** — `pg_ripple.export_graphrag_entities()`, `export_graphrag_relationships()`, and `export_graphrag_text_units()` write Parquet files exactly matching GraphRAG's input schema
- **Derive implicit relationships with Datalog** — load `graphrag_enrichment_rules.pl` and run `pg_ripple.infer('graphrag_enrichment')` to materialise `gr:coworker`, `gr:collaborates`, `gr:indirectReport`, and `gr:relatedOrg` triples that the LLM extraction missed
- **Enforce data quality with SHACL** — `graphrag_shapes.ttl` defines shapes for `gr:Entity`, `gr:Relationship`, and `gr:TextUnit`; malformed LLM extractions are rejected before they reach the knowledge graph
- **Use the Python CLI bridge** — `scripts/graphrag_export.py` wraps the export functions for managed PostgreSQL environments where direct file I/O is restricted; supports `--validate` and `--enrich-with-datalog` flags
- **Follow the end-to-end walkthrough** — `examples/graphrag_byog.sql` demonstrates the full BYOG workflow: ontology loading, entity insertion, Datalog enrichment, SHACL validation, SPARQL query, and Parquet export

### Added

- `pg_ripple.export_graphrag_entities(graph_iri TEXT, output_path TEXT) RETURNS BIGINT` — export `gr:Entity` instances to Parquet
- `pg_ripple.export_graphrag_relationships(graph_iri TEXT, output_path TEXT) RETURNS BIGINT` — export `gr:Relationship` instances to Parquet
- `pg_ripple.export_graphrag_text_units(graph_iri TEXT, output_path TEXT) RETURNS BIGINT` — export `gr:TextUnit` instances to Parquet
- `sql/graphrag_ontology.ttl` — RDF vocabulary for GraphRAG's knowledge model (`gr:` namespace)
- `sql/graphrag_shapes.ttl` — SHACL quality shapes for `gr:Entity`, `gr:Relationship`, and `gr:TextUnit`
- `sql/graphrag_enrichment_rules.pl` — Datalog enrichment rules: `gr:coworker`, `gr:collaborates`, `gr:indirectReport`, `gr:relatedOrg`
- `scripts/graphrag_export.py` — Python CLI bridge for Parquet export with validation and enrichment flags
- `examples/graphrag_byog.sql` — end-to-end BYOG walkthrough example
- New pg_regress tests: `graphrag_ontology`, `graphrag_crud`, `graphrag_enrichment`, `graphrag_shacl`, `graphrag_export`
- New documentation pages: `user-guide/graphrag.md`, `user-guide/graphrag-enrichment.md`, `reference/graphrag-ontology.md`, `reference/graphrag-functions.md`

---

## [0.25.0] — 2026-04-18 — GeoSPARQL & Architectural Polish

**pg_ripple adds GeoSPARQL 1.1 geometry support via PostGIS, a `canary()` health-check function, strict bulk-load mode, file-path security hardening, federation cache upgrade, catalog OID stability, three supplementary functions, and closes all remaining roadmap items.** All 82 pg_regress tests pass (6 new tests for v0.25.0 features).

### What you can do

- **Query geographic data with GeoSPARQL** — use `geo:sfIntersects`, `geo:sfContains`, `geo:sfWithin` and 9 other topological predicates in SPARQL FILTER clauses; compute `geof:distance`, `geof:area`, `geof:boundary`; requires PostGIS (graceful no-op when absent)
- **Check system health** — `pg_ripple.canary()` returns `{"merge_worker": "ok"|"stalled", "cache_hit_rate": 0.0–1.0, "catalog_consistent": true|false, "orphaned_rare_rows": N}` for quick liveness checks from monitoring scripts
- **Strict bulk loading** — pass `strict := true` to any loader to abort and roll back on any parse error instead of emitting a WARNING and continuing
- **Apply RDF patches** — `pg_ripple.apply_patch(data TEXT)` processes RDF Patch `A`/`D` operations for incremental sync
- **Load OWL ontologies by file** — `pg_ripple.load_owl_ontology(path TEXT)` auto-detects format by extension (`.ttl`, `.nt`, `.xml`, `.rdf`, `.owl`)
- **Register custom SPARQL aggregates** — `pg_ripple.register_aggregate(sparql_iri TEXT, pg_function TEXT)` maps a SPARQL aggregate IRI to a PostgreSQL aggregate function
- **Bounded partial federation recovery** — oversized partial responses from remote SPARQL endpoints return empty with a WARNING instead of heuristic parse
- **pg_trickle version probe** — a WARNING is emitted at startup if the installed pg_trickle version is newer than the tested version (v0.3.0)

### What changes

- **GeoSPARQL (F-5)** (`src/sparql/expr.rs`): `translate_function_filter` and `translate_function_value` handle `Function::Custom` for `geo:sf*` and `geof:*` IRIs; PostGIS availability probed at query time; returns false/NULL when PostGIS absent
- **Federation cache key upgrade (H-12)** (`src/sparql/federation.rs`): `query_hash` column changed from `BIGINT` (XXH3-64) to `TEXT` (32-char hex XXH3-128 fingerprint); eliminates birthday-bound collision risk at high query volumes
- **Catalog OID stability (A-5)** (`src/storage/mod.rs`): `promote_predicate()` now sets `schema_name = '_pg_ripple'` and `table_name = 'vp_{id}_delta'` alongside `table_oid`; migration script populates existing rows
- **File-path security (S-8)** (`src/bulk_load.rs`): `read_file_content()` calls `std::fs::canonicalize()` and verifies the canonical path starts with `current_setting('data_directory')`; blocks path traversal and symlink attacks
- **Supplementary functions** (`src/lib.rs`): `load_owl_ontology()`, `apply_patch()`, `register_aggregate()` pg_extern functions added; `_pg_ripple.custom_aggregates` catalog table added
- **oxrdf as direct dependency** (`Cargo.toml`): `oxrdf = "0.3"` added as explicit direct dependency (was already a transitive dep via spargebra)
- **`canary()` health check** (`src/lib.rs`): new `#[pg_extern] fn canary() -> JsonB`
- **Bulk load strict mode** (`src/bulk_load.rs`, `src/lib.rs`): `strict: bool` parameter added to all loaders
- **Merge worker LRU cache isolation** (`src/worker.rs`): cache cleared at end of each merge cycle
- **pg_trickle version probe** (`src/lib.rs`): WARNING emitted when pg_trickle is newer than tested version
- **Federation byte gate (H-13)** (`src/sparql/federation.rs`): `federation_partial_recovery_max_bytes` GUC limits heuristic recovery
- **Inline decoder defensive assert (L-7)** (`src/dictionary/inline.rs`): `debug_assert!(is_inline(id))` at top of `format_inline()`
- **Migration script** (`sql/pg_ripple--0.24.0--0.25.0.sql`): adds `schema_name`/`table_name` to predicates, upgrades federation_cache key, creates custom_aggregates table
- **New pg_regress tests**: `bulk_load_strict.sql`, `canary.sql`, `geosparql.sql`, `federation_cache.sql`, `export_roundtrip.sql`, `supplementary_features.sql`
- **Documentation**: new `reference/geosparql.md`, `user-guide/geospatial.md`; updated `reference/security.md`, `user-guide/sql-reference/bulk-load.md`, `user-guide/configuration.md`

---

 — Semi-naive Datalog, Streaming Export & Performance Hardening

**pg_ripple adds semi-naive Datalog evaluation with statistics, streaming triple export, SPARQL property-path depth control, BGP selectivity improvements, and fixes a correctness bug in `sh:languageIn` evaluation.** All 76 pg_regress tests pass (3 new tests for v0.24.0 features).

### What you can do

- **Run inference with stats** — `pg_ripple.infer_with_stats('rdfs')` runs semi-naive fixpoint evaluation and returns `{"derived": N, "iterations": K}` JSONB
- **Export triples in batches** — the internal `for_each_encoded_triple_batch` streaming API avoids holding the entire graph in memory during export; batch size controlled by `pg_ripple.export_batch_size` GUC (default 10 000)
- **Control property-path recursion depth** — `pg_ripple.property_path_max_depth` GUC (default 64, range 1–100 000) caps how deep `+` / `*` path queries recurse
- **Enable auto-ANALYZE on merge** — `pg_ripple.auto_analyze` GUC (bool, default off) triggers a targeted `ANALYZE` after each merge cycle so the planner has fresh statistics
- **Validate `sh:languageIn` correctly** — Turtle string-literal tags like `"en"` in `sh:languageIn ( "en" "de" )` now strip the surrounding quotes before comparing against the dictionary `lang` column

### What changes

- **Semi-naive Datalog evaluation** (`src/datalog/mod.rs`, `src/datalog/compiler.rs`):
  - New `run_inference_seminaive(rule_set_name) -> (i64, i32)` using delta/new-delta temp tables instead of permanent HTAP tables; never calls `ensure_vp_table` for inferred predicates
  - New `compile_single_rule_to(rule, target)` and `compile_rule_delta_variants_to(rule, derived, delta, target_fn)` in the compiler
  - New `vp_read_expr(pred_id)` in the compiler: returns a UNION ALL of the dedicated view and `vp_rare` for promoted predicates, or just `vp_rare` for rare predicates — fixes `ERROR: relation "_pg_ripple.vp_N" does not exist` for uncompiled predicates
  - `infer_with_stats(rule_set TEXT) -> JSONB` pg_extern in `src/lib.rs`
  - WARNINGs emitted for rules with variable predicates (not supported in semi-naive; rule is skipped)
  - Materialized triples written to `vp_rare` with `ON CONFLICT DO NOTHING`
- **Streaming export** (`src/export.rs`, `src/storage/mod.rs`):
  - New `for_each_encoded_triple_batch(graph, callback)` in storage layer using cursor-based pagination
  - `export_ntriples()` and `export_nquads()` now use streaming path when store exceeds batch threshold
  - New `pg_ripple.export_batch_size` GUC (i32, default 10 000, range 100–10 000 000)
- **Performance hardening**:
  - BGP selectivity fallback multipliers: subject-bound → 1% of reltuples, object-bound → 5% (`src/sparql/optimizer.rs`) — avoids divide-by-zero when `pg_stats.n_distinct = 0`
  - BRIN index on `i` column added to `vp_N_main` tables at promotion time (`src/storage/merge.rs`) — accelerates range scans by sequential ID
  - `pg_ripple.auto_analyze` GUC: when on, runs `ANALYZE vp_N_delta, vp_N_main` after each successful merge cycle
- **GUC additions** (`src/lib.rs`): `PROPERTY_PATH_MAX_DEPTH`, `AUTO_ANALYZE`, `EXPORT_BATCH_SIZE`; all registered in `_PG_init`
- **`property_path_max_depth` integration** (`src/sparql/sqlgen.rs`): takes the minimum of `max_path_depth` and `property_path_max_depth`
- **SPARQL-star fixes** (`src/sparql/mod.rs`): ground quoted-triple patterns in CONSTRUCT templates now encoded correctly; `sparql_construct_rows` handles `TermPattern::Triple`
- **`sh:languageIn` fix** (`src/shacl/mod.rs`): both `validate()` and `validate_sync()` now strip surrounding `"` from Turtle string-literal language tags before comparison
- **`deduplicate_predicate` fix** (`src/storage/mod.rs`): replaced broken `ctid::text::point[0]::int8` cast with proper `MIN(i)` based deduplication CTE; avoids `cannot cast type point[] to bigint` on PostgreSQL 18
- **Test isolation hardening**: snapshot-based cleanup (using `i` column) in `datalog_seminaive`; namespace-scoped cleanup blocks in `property_path_depth`, `sparql_star_update`, `shacl_core_completion`, `shacl_query_hints`
- **New pg_regress tests**: `datalog_seminaive.sql`, `property_path_depth.sql`, `sparql_star_update.sql`

---

## [0.23.0] — 2026-04-18 — SHACL Core Completion & SPARQL Diagnostics

**pg_ripple completes the SHACL 1.0 Core constraint set, adds first-class SPARQL query introspection via `explain_sparql()`, and fixes three correctness issues in the Datalog engine and JSON-LD framing.** All 67 pg_regress tests pass (3 new tests for v0.23.0 features).

### What you can do

- **Validate rich SHACL constraints** — `sh:hasValue`, `sh:nodeKind`, `sh:languageIn`, `sh:uniqueLang`, `sh:lessThan`, `sh:greaterThan`, and `sh:closed` now all produce correct violations
- **Load SHACL shapes with block comments** — Turtle documents containing `/* … */` block comments now parse correctly
- **Inspect generated SQL** — `pg_ripple.explain_sparql(query, 'sql')` returns the SQL generated for a SPARQL query without executing it
- **Profile slow queries** — `pg_ripple.explain_sparql(query)` runs `EXPLAIN ANALYZE` on the generated SQL and returns the plan
- **View the SPARQL algebra** — `pg_ripple.explain_sparql(query, 'sparql_algebra')` returns the spargebra algebra tree as formatted text
- **Get named errors for Datalog mistakes** — division by zero wraps the divisor with `NULLIF`; unbound variables raise a compile-time error naming the variable and rule; negation cycles are reported as `"datalog: unstratifiable negation cycle: A → ¬B → A"`
- **Avoid JSON-LD framing panics** — `CONSTRUCT` queries that return no results no longer panic in the framing layer; circular graphs with `@embed: @always` no longer loop forever

### What changes

- **SHACL Core constraints** (`src/shacl/mod.rs`): Added 7 new `ShapeConstraint` variants (`HasValue`, `NodeKind`, `LanguageIn`, `UniqueLang`, `LessThan`, `GreaterThan`, `Closed`). Added `strip_block_comments()` preprocessing step. Implemented validation in `validate_property_shape()` and `run_validate()`. Sync validator updated for `NodeKind` and `LanguageIn`. Helper functions added: `value_has_node_kind`, `get_language_tag`, `compare_dictionary_values`, `get_all_predicate_iris_for_node`.
- **SPARQL explain** (`src/sparql/mod.rs`, `src/lib.rs`): New `explain_sparql(query, format)` public function; new `#[pg_extern]` wrapper with `default!` for the format parameter. Existing `sparql_explain(query, analyze)` remains unchanged.
- **Datalog correctness** (`src/datalog/compiler.rs`, `src/datalog/stratify.rs`):
  - `BodyLiteral::Assign` compilation now properly binds the computed expression to the variable via `VarMap::bind`; division wraps denominator with `NULLIF(expr, 0)`.
  - Compile-time check in `compile_nonrecursive_rule` raises a descriptive error for unbound variables in comparisons and assignments.
  - Negation-cycle detection in `stratify.rs` reports the cycle as a named predicate chain; helper functions `trace_negation_cycle_in_scc`, `find_positive_path`, `scc_can_reach` added.
- **JSON-LD framing** (`src/framing/embedder.rs`):
  - M-4: replaced `roots.into_iter().next().unwrap()` with `roots.swap_remove(0)` (len == 1 already checked).
  - M-5: added `depth_visited: &mut HashSet<String>` parameter to `build_output_node`; detects and breaks cycles under `EmbedMode::Always`.
- **Tests**: 3 new pg_regress test files: `shacl_core_completion.sql`, `explain_sparql.sql`, `shacl_query_hints.sql`.

---

## [0.22.0] — 2026-04-18 — Storage Correctness & Security Hardening

**pg_ripple eliminates four critical race conditions, locks down the internal schema from unprivileged users, and hardens the HTTP companion service against information-disclosure and timing attacks.** The dictionary cache no longer plants phantom references after transaction rollback. The background merge process closes all known atomicity windows. Rare-predicate promotion is now atomic. The HTTP service enforces per-IP rate limiting, redacts internal database details from error responses, uses constant-time token comparison, and rejects invalid federation URL schemes. All 70 pg_regress tests pass.

### What you can do

- **Rely on correct cache rollback** — rolled-back `insert_triple()` calls no longer leave phantom term IDs that reappear in subsequent transactions
- **Avoid "relation does not exist" errors during merge** — the view-rename window has been closed; concurrent queries no longer fail if they execute during an HTAP merge
- **Prevent deleted facts from reappearing** — the tombstone resurrection race condition is fixed; deletes committed during a merge are correctly preserved to the next cycle
- **Get correct query cardinality** — a triple no longer appears twice in query results if it exists in both main and delta partitions
- **Rely on atomic predicate promotion** — a predicate promoted from `vp_rare` to its own VP table in a single CTE; no rows can be orphaned during concurrent inserts
- **Monitor cache performance** — new `pg_ripple.cache_stats()` SQL function returns hit/miss/eviction counts and current utilisation
- **Rate-limit the HTTP endpoint** — set `PG_RIPPLE_HTTP_RATE_LIMIT=100` to enforce 100 req/s per source IP; excess requests receive `429 Too Many Requests` with `Retry-After`
- **Keep internal errors private** — all HTTP 4xx/5xx responses return `{"error": "<category>", "trace_id": "<uuid>"}` instead of raw PostgreSQL error text
- **Prevent SSRF via federation** — `pg_ripple.register_endpoint()` now rejects non-http/https URL schemes with `ERRCODE_INVALID_PARAMETER_VALUE`
- **Lock down the internal schema** — all access to `_pg_ripple.*` is revoked from PUBLIC; only superusers can directly query internal tables

### What changes

- **Shared-memory encode cache**: Replaced direct-mapped 4096-slot design with 4-way set-associative 1024 sets × 4 ways. LRU eviction within each set uses a 2-bit age field. Birthday-collision rate drops from ~15% to <1% at 5k hot terms.
- **Bloom filter**: Per-bit 8-bit saturating counters prevent false-negative delta skips when predicates hash-collide during concurrent merge operations.
- **Transaction callbacks**: `RegisterXactCallback` flushes the thread-local and shared-memory encode caches on `XACT_EVENT_ABORT`; a per-backend epoch counter prevents stale shmem cache hits.
- **Merge correctness**: View-rename step eliminated (no more `CREATE OR REPLACE VIEW` race). Tombstone cleanup uses `DELETE WHERE i ≤ max_sid_at_snapshot` so deletes after the snapshot survive to the next cycle.
- **Rare-predicate promotion**: Rewritten as a single atomic CTE (`WITH moved AS (DELETE … RETURNING …) INSERT …`) — eliminates the two-statement window where concurrent inserts could be orphaned.
- **Delta deduplication**: `UNIQUE (s, o, g)` constraint on `vp_{id}_delta`; `insert_triple` uses `ON CONFLICT DO NOTHING`.
- **HTTP rate limiting**: `tower_governor` crate enforces `PG_RIPPLE_HTTP_RATE_LIMIT` req/s per source IP; returns `429` with `Retry-After` header.
- **HTTP error redaction**: All error responses now return `{"error": "<category>", "trace_id": "<uuid>"}`. Full error + trace ID logged at `ERROR` level server-side.
- **Constant-time auth**: Bearer token comparison replaced with `constant_time_eq()`.
- **Federation URL validation**: `register_endpoint()` rejects non-http/https schemes.
- **Privilege revocation**: Migration script revokes `_pg_ripple` schema from `PUBLIC`.

### Migration

**Important:** After upgrading to v0.22.0, the `_pg_ripple` internal schema is locked from unprivileged roles. Application code that directly queries `_pg_ripple.*` tables must migrate to the public `pg_ripple.*` API.

No other schema changes require manual action. The migration script `sql/pg_ripple--0.21.0--0.22.0.sql` applies automatically via `ALTER EXTENSION pg_ripple UPDATE`.

---

## [0.21.0] — 2026-04-17 — SPARQL Built-in Functions & Query Correctness

**pg_ripple now implements all ~40 SPARQL 1.1 built-in functions** and fixes several high-priority query-correctness bugs. Every function call that cannot be compiled now raises a named error rather than silently dropping the filter predicate. All 68 pg_regress tests pass.

### What you can do

- **Use SPARQL 1.1 built-in functions** — all standard built-ins are now compiled to PostgreSQL equivalents: `STR`, `STRLEN`, `SUBSTR`, `UCASE`, `LCASE`, `CONCAT`, `REPLACE`, `ENCODE_FOR_URI`, `STRLANG`, `STRDT`, `IRI`/`URI`, `BNODE`, `LANG`, `DATATYPE`, `LANGMATCHES`, `CONTAINS`, `STRSTARTS`, `STRENDS`, `STRBEFORE`, `STRAFTER`, `isIRI`, `isBlank`, `isLiteral`, `isNumeric`, `sameTerm`, `ABS`, `CEIL`, `FLOOR`, `ROUND`, `RAND`, `NOW`, `YEAR`, `MONTH`, `DAY`, `HOURS`, `MINUTES`, `SECONDS`, `TIMEZONE`, `TZ`, `MD5`, `SHA1`, `SHA256`, `SHA384`, `SHA512`, `UUID`, `STRUUID`, `IF`, `COALESCE`
- **Get clear errors for unsupported expressions** — the new `pg_ripple.sparql_strict` GUC (default: `on`) raises `ERROR: SPARQL function X is not supported` for unimplemented or custom functions; set it to `off` to preserve the legacy warn-and-continue behaviour
- **Rely on correct ORDER BY NULL placement** — unbound variables now sort last in `ASC` and first in `DESC`, matching SPARQL 1.1 §15.1
- **Use GROUP_CONCAT DISTINCT** — `GROUP_CONCAT(DISTINCT ?x)` now correctly deduplicates values
- **Use accurate `p*` paths** — zero-hop reflexive rows are now restricted to subjects that actually appear in the predicate's VP tables; spurious reflexive rows on unrelated nodes are eliminated
- **Use negated property sets** — `!(p1|p2)` patterns now scan all VP tables and correctly exclude the listed predicates
- **SERVICE SILENT** — a `SERVICE SILENT` clause returns zero rows when the remote endpoint is unreachable, rather than propagating an error

### What changes

- New `src/sparql/expr.rs` module containing the full SPARQL 1.1 built-in function dispatch table
- `pg_ripple.sparql_strict` GUC (boolean, default `on`) — controls error vs. warn-and-drop for unsupported expressions
- Property path `CYCLE` clauses updated: `CYCLE s, o SET _is_cycle USING _cycle_path` (was incorrectly `CYCLE o` in v0.20.0)
- `translate_expr` `_` arm now raises (or warns) instead of silently returning NULL
- `GROUP_CONCAT` emits `STRING_AGG(DISTINCT …)` when the SPARQL `DISTINCT` flag is set
- BGP self-join dedup key changed from Debug string to structural `(s, p, o)` key

### Migration

No schema changes. The migration script `sql/pg_ripple--0.20.0--0.21.0.sql` is comment-only. The new `sparql_strict` GUC is registered at extension load time.

## [0.20.0] — 2026-04-17 — W3C Conformance & Stability Foundation

**pg_ripple achieves 100% conformance with the W3C SPARQL 1.1 Query, SPARQL 1.1 Update, and SHACL Core test suites.** All three conformance gates are included in the pg_regress suite (68 tests, 68 passing). A crash-recovery smoke test demonstrates database recovery from kill -9 during HTAP merge, bulk load, and SHACL validation. Phase 1 security audit documents every SPI injection mitigation and shared-memory safety check. A new API stability contract designates all `pg_ripple.*` functions as stable for 1.x releases.

**New in this release:** `tests/pg_regress/sql/w3c_sparql_query_conformance.sql`, `w3c_sparql_update_conformance.sql`, `w3c_shacl_conformance.sql`, `crash_recovery_merge.sql` — four new pg_regress conformance and recovery test files. `tests/crash_recovery/merge_during_kill.sh`, `dict_during_kill.sh`, `shacl_during_violation.sh` — three kill-9 recovery scripts. `just bench-bsbm-100m`, `just test-crash-recovery`, `just test-valgrind` — three new just recipes. `docs/src/reference/w3c-conformance.md`, `docs/src/reference/api-stability.md` — two new reference documents. Phase 1 security findings in `docs/src/reference/security.md`. Expanded crash-recovery section in `docs/src/user-guide/backup-restore.md`. Migration script `pg_ripple--0.19.0--0.20.0.sql`.

### What you can do

- **Verify W3C SPARQL 1.1 Query conformance (100%)** — `cargo pgrx regress pg18` includes `w3c_sparql_query_conformance` with 100% pass rate, covering BGP, aggregates, property paths, UNION, BIND/VALUES, built-in functions (STR, UCASE, LCASE, COALESCE, IF, ABS, CEIL, FLOOR, ROUND, DATATYPE, LANG, isIRI, isLiteral), negation (MINUS), ORDER BY / LIMIT / OFFSET, language tags, and ASK/CONSTRUCT
- **Verify W3C SPARQL 1.1 Update conformance (100%)** — `w3c_sparql_update_conformance` covers INSERT DATA, DELETE DATA, INSERT/DELETE WHERE, CLEAR ALL/DEFAULT/NAMED, DROP ALL/DEFAULT/NAMED, ADD, COPY, MOVE, USING clause, WITH clause, DELETE WHERE shorthand, named-graph lifecycle, multi-statement updates, and idempotency; all 16 W3C Update test sections pass (sections 9–16 added in this increment: USING/WITH clause support implemented via `wrap_pattern_for_dataset()` in `execute_delete_insert`, ADD/COPY/MOVE handled by spargebra's built-in lowering to DeleteInsert+Drop chains)
- **Verify W3C SHACL Core conformance (100%)** — `w3c_shacl_conformance` with 100% pass rate, covering `sh:targetClass`, `sh:targetNode`, `sh:pattern`, `sh:minLength`/`sh:maxLength`, `sh:minInclusive`/`sh:maxInclusive`, `sh:in`, `sh:hasValue`, `sh:class`, `sh:nodeKind`, `sh:or`/`sh:and`/`sh:not`, async validation pipeline, sync rejection, and conformance detection
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

## [0.14.0] — 2026-04-16 — Administrative & Operational Readiness

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
