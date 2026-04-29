# pg_ripple Overall Assessment 11

Date: 2026-04-29

Scope: This assessment reviewed pg_ripple as declared at **v0.71.0** in [Cargo.toml](../Cargo.toml#L4) and [pg_ripple.control](../pg_ripple.control#L5), covering the two releases since [Assessment 10](PLAN_OVERALL_ASSESSMENT_10.md): **v0.70.0** (Assessment 10 critical remediation — bulk-load mutation journal, per-statement flush, fail-closed evidence gate, SHACL documentation truth, README versioning, RLS SQL quoting, SBOM regeneration) and **v0.71.0** (Arrow Flight streaming validation, Citus multi-node integration test, HTTP companion compatibility check, HLL accuracy docs, SERVICE shard-pruning docs). The standard applied is a world-class PostgreSQL extension suitable for third-party security review and production deployment. Every Critical/High finding cites file paths and line ranges.

## Executive Summary

The v0.70.0–v0.71.0 cycle has **closed nine of the sixteen Assessment 10 findings outright** (CF-1, CF-3, HF-1, HF-2, HF-4, HF-5 partial, HF-8 partial, MF-2, MF-4, MF-6 partial). Bulk-load now correctly fires CONSTRUCT writeback at end-of-load ([src/bulk_load.rs](../src/bulk_load.rs#L201-L204), [L239-L241](../src/bulk_load.rs#L239-L241), eight call sites total); SHACL documentation is now honest ([docs/src/features/shacl-sparql-rules.md](../docs/src/features/shacl-sparql-rules.md#L5-L6) explicitly says "`sh:SPARQLRule` … is **not yet supported**"); RLS DDL quotes role identifiers and validates them against `[A-Za-z_][A-Za-z0-9_$]*` with PT711 errors ([src/security_api.rs](../src/security_api.rs#L25-L42), [L106-L131](../src/security_api.rs#L106-L131), [L148-L161](../src/security_api.rs#L148-L161)); Arrow Flight uses `Body::from_stream` with 64 KiB chunks ([pg_ripple_http/src/arrow_encode.rs](../pg_ripple_http/src/arrow_encode.rs#L306-L317), [pg_ripple_http/src/routing.rs](../pg_ripple_http/src/routing.rs#L346)); the Citus RLS integration test now exists ([tests/integration/citus_rls_propagation.sh](../tests/integration/citus_rls_propagation.sh), 222 lines). Module sizes have continued to drift downward only marginally — `src/lib.rs` is now 2,413 lines (was 2,262), `src/storage/mod.rs` 2,145 (was 2,104). The continued split has not occurred. None of the v0.70.0–v0.71.0 work introduced a *new* security regression at the SQL/DDL layer.

That said, **three new Critical findings are open and one Critical from Assessment 10 (CF-2 / FLUSH-01) is regressed rather than resolved**.

The single most important new finding is that **`sparql_update()` never calls `mutation_journal::flush()`** ([src/sparql/execute.rs](../src/sparql/execute.rs#L521-L651) — the function returns at line 651 without any flush, and `insert_triple_by_ids`/`delete_triple_by_ids` no longer flush per-call as the design originally intended). The v0.70.0 changelog FLUSH-01 entry claims "Journal flush is deferred to `XACT_EVENT_PRE_COMMIT` via the existing `xact_callback_c`", but the wire-up at [src/lib.rs](../src/lib.rs#L2128-L2148) explicitly comments "we do NOT call `flush()` here for `XACT_EVENT_PRE_COMMIT` (event 5) because SPI is not safely callable from within a PostgreSQL xact callback" — and the only consequent flush call sites are in [src/dict_api.rs](../src/dict_api.rs#L231) (single-triple `insert_triple()`), [src/dict_api.rs](../src/dict_api.rs#L298) (single-triple `delete_triple()`), [src/views_api.rs](../src/views_api.rs#L436) (named-graph drop), and [src/bulk_load.rs](../src/bulk_load.rs) (eight bulk loaders). **No flush from any SPARQL INSERT/DELETE/UPDATE path.** This is the same Assessment 9 / Assessment 10 CF-1 problem in a new form: writes that go through `sparql_update()` accumulate journal entries that are *only* flushed if a subsequent unrelated SQL function (`pg_ripple.insert_triple` or a bulk loader) happens to be invoked in the same transaction. CONSTRUCT writeback for SPARQL Update is **silently broken** in v0.70.0 and v0.71.0.

The second is that the **v0.67.0 GATE-02 evidence-path validator is provably bypassed**. [src/feature_status.rs](../src/feature_status.rs) cites twelve `docs/src/reference/*.md` files that **do not exist on disk** (`cdc.md`, `construct-rules.md`, `datalog.md`, `development.md`, `federation.md`, `graphrag.md`, `observability.md`, `query-optimization.md`, `shacl.md`, `sparql.md`, `storage.md`, `vector-search.md`); the v0.70.0 GATE-03 fix only created `scalability.md` and `arrow-flight.md`. The CI job at [.github/workflows/ci.yml](../.github/workflows/ci.yml) `validate-feature-status` runs `psql -c "SELECT evidence_path … WHERE evidence_path IS NOT NULL"` and pipes through `[ ! -e "$path" ]` — that should fail v0.71.0 builds, yet the release was tagged. Either the job is informational, the SQL function is returning a NULL/skip for those rows in CI, or the runner-host test database does not see them. Assessment 8, 9, and 10 all flagged this; v0.71.0 still ships with at least twelve broken evidence citations.

The third is that **SAVEPOINT/sub-transaction safety is not implemented**. [src/lib.rs](../src/lib.rs#L2135-L2143) clears the mutation journal on `XACT_EVENT_ABORT` (event 2) and `XACT_EVENT_PARALLEL_ABORT` (event 3), but no `RegisterSubXactCallback` exists anywhere in the source tree (grep confirms zero matches in `src/`). A SPARQL INSERT inside a `SAVEPOINT` followed by `ROLLBACK TO SAVEPOINT` will leave the inserted graph IDs in the journal; if a subsequent `pg_ripple.insert_triple()` in the same outer transaction triggers a flush, CONSTRUCT writeback will run for graphs whose source triples have been rolled back. This was Assessment 10 HF-7 marked as needing fix; v0.72.0 plans `CACHE-01` for plan cache but no roadmap line addresses sub-transaction journaling.

Top five recommended actions: (1) Add `mutation_journal::flush()` at the end of `sparql_update()` ([src/sparql/execute.rs](../src/sparql/execute.rs#L651)) and at the end of `execute_delete_insert` ([src/sparql/execute.rs](../src/sparql/execute.rs#L796)) — this is a one-line change per call site that closes CF-A and restores SPARQL-Update CWB correctness; (2) Either create the 12 missing `docs/src/reference/*.md` stubs or strip the citations from `feature_status.rs`, then verify GATE-02 actually fails on a deliberately-broken evidence path in CI; (3) Implement `RegisterSubXactCallback` to clear the journal on subxact abort, closing CF-B / HF-7 with the same one-screen pattern as the existing xact callback; (4) Wire mutation journal into Datalog materialization, R2RML materialization, and CDC ingest paths — grep confirms zero `mutation_journal::*` calls in `src/datalog/`, `src/r2rml*.rs`, or `src/cdc*.rs`; (5) Update SBOM and align `pg_ripple_http` version with extension version (currently 0.16.0 vs extension 0.71.0). Production-readiness verdict: **Late beta, suitable for controlled pilots; not yet production-ready for any deployment that depends on SPARQL-Update-driven CONSTRUCT writeback or Datalog-driven CWB.** The v0.70.0 changelog claim that FLUSH-01 was completed and the BULK-01 / TEST-01 / TEST-02 / TEST-03 entries are individually accurate; the gap is that the FLUSH-01 fix replaced a per-triple over-flush bug with a never-flush-from-SPARQL-Update bug.

## Assessment Method

This assessment read the documents listed in the prompt plus the v0.70.0 mutation-journal rewiring, the v0.71.0 Arrow streaming code, the new integration test scripts, and the `validate-feature-status` CI job body. Programmatic checks were performed where possible:

- **Migration scripts**: confirmed via `ls sql/` that `pg_ripple--0.69.0--0.70.0.sql` (40 lines) and `pg_ripple--0.70.0--0.71.0.sql` (37 lines) are present and use the `-- Migration X.Y.Z → A.B.C` header form.
- **Version metadata**: [Cargo.toml](../Cargo.toml#L4) and [pg_ripple.control](../pg_ripple.control#L5) both at 0.71.0; [pg_ripple_http/Cargo.toml](../pg_ripple_http/Cargo.toml#L3) is at 0.16.0 (decoupled).
- **Mutation journal call-graph**: `grep -rn "mutation_journal::"` enumerated every call site (16 total). Bulk-load now flushes (eight `bulk_load.rs` calls); single-triple API flushes (three calls in `dict_api.rs`/`views_api.rs`); xact abort clears (one call in `lib.rs`); zero calls anywhere in `src/sparql/`, `src/datalog/`, `src/r2rml*.rs`, or `src/cdc*.rs`.
- **Sub-transaction callback**: `grep -rn "SubXact\|RegisterSubXactCallback" src/` returns no matches.
- **Evidence paths**: extracted every `docs/src/reference/*.md` literal from `feature_status.rs` and tested existence; 12 of 14 are missing.
- **Module sizes**: `wc -l` against the 18 largest source files ranks `src/lib.rs` at 2,413 lines (top), `src/storage/mod.rs` at 2,145, `src/datalog/compiler.rs` at 1,612, `src/sparql/federation.rs` at 1,519, `pg_ripple_http/src/routing.rs` at 1,503.
- **Unsafe blocks**: enumerated; `src/shmem.rs` and `src/lib.rs` GUC validators are the dominant locations and all are PostgreSQL FFI — present `// SAFETY:` comments.
- **GitHub Actions SHA pinning**: spot-checked `validate-feature-status` job; observed only 40-character SHA pins.
- **Property-path CYCLE clause**: confirmed `WITH RECURSIVE … CYCLE s, o SET _is_cycle USING _cycle_path` at [src/sparql/property_path.rs:245-253](../src/sparql/property_path.rs#L245-L253).
- **Federation allowlist**: confirmed `FEDERATION_ALLOWED_ENDPOINTS` GUC and the three-mode policy at [src/sparql/federation.rs:214-229](../src/sparql/federation.rs#L214-L229) and [src/gucs/federation.rs:54-73](../src/gucs/federation.rs#L54-L73).
- **Fuzz targets**: 12 confirmed in `fuzz/fuzz_targets/` (`datalog_parser.rs`, `dictionary_hash.rs`, `federation_result.rs`, `geosparql_wkt.rs`, `http_request.rs`, `jsonld_framer.rs`, `llm_prompt_builder.rs`, `r2rml_mapping.rs`, `rdfxml_parser.rs`, `shacl_parser.rs`, `sparql_parser.rs`, `turtle_parser.rs`).
- **SECURITY DEFINER**: only one match in `sql/`, in `sql/pg_ripple--0.55.0--0.56.0.sql:60`.

Items not verified (require a running PG18/Citus/HTTP environment): runtime SPARQL conformance pass rates, the runtime behavior of the Citus integration test against an actual cluster, the runtime memory bound of the Arrow large-export integration test, the actual exit status of `validate-feature-status` job on the v0.71.0 release commit (whether it failed or whether something masked it), and mdBook build success. These are labeled **Static analysis — runtime confirmation required** in the affected findings.

## Resolution of Prior Assessment 10 Findings

| Finding ID | Status in v0.70.0–v0.71.0 | Evidence | Remaining gap |
|---|---|---|---|
| **CF-1** Bulk load bypasses mutation journal | **Resolved** | `crate::storage::mutation_journal::flush()` called at end of every bulk loader: [bulk_load.rs:204](../src/bulk_load.rs#L204), [L241](../src/bulk_load.rs#L241), [L274](../src/bulk_load.rs#L274), [L311](../src/bulk_load.rs#L311), [L449](../src/bulk_load.rs#L449), [L485](../src/bulk_load.rs#L485), [L518](../src/bulk_load.rs#L518), [L551](../src/bulk_load.rs#L551). `batch_insert_encoded` records writes per graph at [storage/mod.rs:696](../src/storage/mod.rs#L696). | None for the bulk-load path itself. Test `cwb_write_path_equivalence.sql` extension claimed; not re-verified. |
| **CF-2** Per-triple `flush()` quadratic CWB | **Replaced with new bug (CF-A)** | Per-call flushes removed from `insert_triple_by_ids` and `delete_triple_by_ids`; comments at [storage/mod.rs:1703-1704](../src/storage/mod.rs#L1703-L1704) and [storage/mod.rs:1801-1803](../src/storage/mod.rs#L1801-L1803) say flush is "deferred to `XACT_EVENT_PRE_COMMIT` via `xact_callback_c`". But [lib.rs:2147](../src/lib.rs#L2147) explicitly says "we do NOT call `flush()` here for `XACT_EVENT_PRE_COMMIT` (event 5)" and no flush call was added to `sparql_update()`. **CWB silently never fires for SPARQL INSERT/DELETE WHERE.** Re-raised as **CF-A**. |
| **CF-3** SHACL-SPARQL overclaim | **Resolved** | [docs/src/features/shacl-sparql-rules.md:5-6](../docs/src/features/shacl-sparql-rules.md#L5-L6) now says "pg_ripple supports `sh:SPARQLConstraint` (validation) and `sh:TripleRule` (inference). `sh:SPARQLRule` (SPARQL-based inference) is **not yet supported**". |
| **CF-4** `feature_status()` cites missing evidence | **Partially Resolved** | `docs/src/reference/scalability.md` (38 lines) and `docs/src/reference/arrow-flight.md` (75 lines) created. **Twelve other `docs/src/reference/*.md` paths still cited and still missing**: `cdc.md`, `construct-rules.md`, `datalog.md`, `development.md`, `federation.md`, `graphrag.md`, `observability.md`, `query-optimization.md`, `shacl.md`, `sparql.md`, `storage.md`, `vector-search.md`. Re-raised as **CF-C**. |
| **HF-1** Citus integration test missing | **Resolved (script exists; runtime not verified)** | [tests/integration/citus_rls_propagation.sh](../tests/integration/citus_rls_propagation.sh) is 222 lines, starts a Citus cluster via docker-compose, asserts cross-graph RLS; full runtime pass not verified statically. |
| **HF-2** README two releases stale | **Resolved** | v0.70.0 README-01/02; [scripts/check_readme_version.sh](../scripts/check_readme_version.sh) (16 lines) added. |
| **HF-3** Arrow Flight streaming unverified | **Resolved (static + integration test exists)** | `Body::from_stream(byte_stream)` at [pg_ripple_http/src/arrow_encode.rs:306-317](../pg_ripple_http/src/arrow_encode.rs#L306-L317); 64 KiB chunks; [tests/http_integration/arrow_export_large.sh](../tests/http_integration/arrow_export_large.sh) (202 lines) asserts Transfer-Encoding: chunked, RSS bound. Runtime confirmation still wanted. |
| **HF-4** RLS DDL role-name SQL injection | **Resolved** | `is_safe_role_name` at [security_api.rs:25-42](../src/security_api.rs#L25-L42); `quote_ident_safe` at [security_api.rs:46-53](../src/security_api.rs#L46-L53); applied at [L106-L131](../src/security_api.rs#L106-L131) (`apply_rls_to_vp_table`, skips unsafe entries with warning) and [L148-L161](../src/security_api.rs#L148-L161) (`do_grant_graph_access`, errors with PT711). [tests/pg_regress/sql/security_rls_role_injection.sql](../tests/pg_regress/sql/security_rls_role_injection.sql) (49 lines) added. |
| **HF-5** SBOM 18 releases stale | **Partially Resolved** | Top-level [sbom.json](../sbom.json) `version` is now `0.70.0` (was `0.51.0`). However: (a) the inner `components[]` block still contains a stale `pg_ripple@0.51.0` entry (search confirms); (b) v0.71.0 was tagged but SBOM still says 0.70.0 — release-CI commit-back is one release behind. **HF-A**. |
| **HF-6** Plan cache key omits graph/security context | **Still Open** | Roadmap puts this in v0.72.0 CACHE-01; `cache_key` at [src/sparql/plan_cache.rs:92](../src/sparql/plan_cache.rs#L92) still hashes only the query text. **MF-A**. |
| **HF-7** Mutation journal not subxact-safe | **Still Open** | Abort callback exists ([lib.rs:2138-2143](../src/lib.rs#L2138-L2143)) and clears the journal on `XACT_EVENT_ABORT`/`PARALLEL_ABORT`. No `RegisterSubXactCallback` anywhere — `grep -rn SubXact src/` returns zero matches. **CF-B**. |
| **HF-8** `pg_ripple_http` decoupled version | **Partially Resolved** | COMPAT-01: startup compatibility check added; minimum-version constant `COMPATIBLE_EXTENSION_MIN = "0.70.0"` (per CHANGELOG). [docs/src/operations/compatibility.md](../docs/src/operations/compatibility.md) (63 lines) added. **But `pg_ripple_http/Cargo.toml` still at 0.16.0** ([pg_ripple_http/Cargo.toml:3](../pg_ripple_http/Cargo.toml#L3)); after two more releases the 0.16.0 number is increasingly meaningless. **MF-B**. |
| **MF-1** `cwb_write_path_equivalence.sql` cannot prove what it claims | **Largely Resolved** | v0.70.0 changelog "Path 5 bulk-load arm" added; full test contents not statically re-verified. |
| **MF-2** Legacy `.sh` gate scripts coexist | **Resolved** | `scripts/check_roadmap_evidence.sh` and `scripts/check_api_drift.sh` both **deleted** per file-existence check. |
| **MF-3** `plan_cache_reset` lacks docs | **Still Open** | No `docs/src/reference/plan-cache.md` found in tree. |
| **MF-4** `recover_interrupted_promotions` no test | **Resolved** | [tests/pg_regress/sql/recover_promotions.sql](../tests/pg_regress/sql/recover_promotions.sql) (83 lines). |
| **MF-5** `merge_throughput_history.csv` only one row | **Status unchanged** | Not re-verified; if the workflow had been running on schedule, 6+ months of history should be appended. **MF-C**. |
| **MF-6** `v067_features.sql`/`v069_features.sql` missing | **Partially Resolved** | [v067_features.sql](../tests/pg_regress/sql/v067_features.sql) (78 lines) and [v069_features.sql](../tests/pg_regress/sql/v069_features.sql) (58 lines) added. **No `v070_features.sql`** — file-existence check confirms missing. v0.70.0 changelog TEST-01/TEST-02 entries exist but a v070 test does not. **MF-D**. |
| **MF-7** Citus HLL accuracy bounds undocumented | **Resolved** | [docs/src/reference/approximate-aggregates.md](../docs/src/reference/approximate-aggregates.md) (102 lines); [tests/pg_regress/sql/hll_accuracy.sql](../tests/pg_regress/sql/hll_accuracy.sql) (80 lines). |
| **MF-8** SERVICE shard pruning untested | **Partially Resolved (docs + GUC test)** | [docs/src/reference/citus-service-pruning.md](../docs/src/reference/citus-service-pruning.md) (94 lines); [tests/pg_regress/sql/citus_service_pruning.sql](../tests/pg_regress/sql/citus_service_pruning.sql) (69 lines) — but per CHANGELOG only "validates GUC plumbing", not real pruning effectiveness. Multi-node EXPLAIN comparison still not in CI. |
| **MF-9** Decode of unknown dictionary IDs silently empty | **Still Open** | No `pg_ripple.strict_dictionary` GUC search hit. |
| **MF-10** Large modules unsplit | **Worse** | `src/lib.rs` 2,413 (was 2,262); `src/storage/mod.rs` 2,145 (was 2,104). Per the v0.69.0 narrowing of `pub` to `pub(crate)`, the line count grew because more functions now live alongside the storage core. **MF-E**. |
| **MF-11** No proptest for ConstructTemplate | **Status unchanged** | Roadmap puts this in v0.72.0. |
| **MF-12** Fuzz corpus may not exercise SPARQL Update | **Status unchanged** | Roadmap puts this in v0.72.0; no `sparql_update` fuzz target in `fuzz/fuzz_targets/`. |
| **MF-13** Streaming counters not on `/metrics` | **Status unchanged** | Not re-verified. |
| **MF-14** Three batch-size GUCs interaction undocumented | **Status unchanged** | No combined GUC reference page found. |
| **MF-15** `[Unreleased]` empty without convention note | **Status unchanged** | [CHANGELOG.md:11-13](../CHANGELOG.md#L11-L13) still empty section without guidance. |
| **MF-16** `roadmap/v0.67.0.md` "Planned" | **Resolved** | Per CHANGELOG v0.70.0 DOC-01: "status already confirmed as Released ✅ (no change needed)". |
| **MF-17** Datalog/CWB interaction undocumented | **Still Open and worse** | Static check confirms `src/datalog/` contains zero `mutation_journal::*` calls and zero calls to `insert_triple_by_ids`; Datalog materialization writes do **not** fire CWB. **CF-D / HF-C below.** |
| **MF-18** URL-host parser untested | **Status unchanged** | No new fuzz target for `extract_url_host`. |
| **MF-19** Arrow Flight ticket replay protection | **Status unchanged** | Roadmap puts this in v0.72.0 ARROW-REPLAY-01. |
| **MF-20** `feature_status` taxonomy undocumented | **Status unchanged** | Roadmap puts this in v0.73.0. |

### Maturity Assessment carry-forward

The Assessment 10 maturity matrix is updated below; in summary, every row that improved is on the back of a v0.70.0 documentation/test addition, while every row that worsened (Correctness, Performance, Code Quality, Test Coverage) traces to one of the four newly-open Critical/High findings (CF-A, CF-B, CF-D, HF-A).

## Critical Findings (Severity: Critical)

### CF-A — `sparql_update()` never flushes the mutation journal; CONSTRUCT writeback silently broken for all SPARQL INSERT/DELETE/UPDATE statements

**Location**: [src/sparql/execute.rs](../src/sparql/execute.rs#L521-L651) `sparql_update`; [src/sparql/execute.rs](../src/sparql/execute.rs#L685-L796) `execute_delete_insert`; [src/storage/mod.rs](../src/storage/mod.rs#L1701-L1710) `insert_triple_by_ids` (no `flush()` call); [src/storage/mod.rs](../src/storage/mod.rs#L1712-L1810) `delete_triple_by_ids` (no `flush()` call); [src/lib.rs](../src/lib.rs#L2128-L2148) xact callback (does **not** flush at PRE_COMMIT — the comment at L2147 is explicit).

**Description**: The v0.70.0 FLUSH-01 changelog entry says journal flush is "deferred to `XACT_EVENT_PRE_COMMIT` via the existing `xact_callback_c`." The code-comment at [storage/mod.rs:1703-1704](../src/storage/mod.rs#L1703-L1704) says the same. **Both are false.** The actual `xact_callback_c` ([lib.rs:2128-2148](../src/lib.rs#L2128-L2148)) handles `XACT_EVENT_ABORT` (clears the journal), `XACT_EVENT_COMMIT` (commits dictionary cache), and explicitly ignores `XACT_EVENT_PRE_COMMIT` with the comment "we do NOT call `flush()` here for `XACT_EVENT_PRE_COMMIT` (event 5) because SPI is not safely callable from within a PostgreSQL xact callback. Flush is called directly at each write API boundary in dict_api.rs and views_api.rs instead (FLUSH-01 revised)." The implication is that callers of `insert_triple_by_ids`/`delete_triple_by_ids` should each call `flush()` themselves. `dict_api::insert_triple` does ([dict_api.rs:230-231](../src/dict_api.rs#L230-L231)). `dict_api::delete_triple` does ([dict_api.rs:298](../src/dict_api.rs#L298)). `views_api::clear_graph` does ([views_api.rs:436](../src/views_api.rs#L436)). **`sparql_update()` does not** — the function returns at [src/sparql/execute.rs:651](../src/sparql/execute.rs#L651) without ever invoking the journal. The same is true for `execute_delete_insert` ([src/sparql/execute.rs:796](../src/sparql/execute.rs#L796)).

**Impact**: A user runs `SELECT pg_ripple.sparql_update('INSERT DATA { GRAPH <g:src> { :a :p :b } }')` against a database where a CONSTRUCT writeback rule depends on `<g:src>`. The triple is inserted into the VP table (correct). The mutation journal records `record_write(g_id_for_g:src)` (correct, fast-path passes because rules exist). **No flush is ever called.** The journal's thread-local `Vec<JournalEntry>` accumulates across statements until either a `pg_ripple.insert_triple()` call (which itself flushes) is made, or the transaction ends — and on `XACT_EVENT_COMMIT` the journal is *not* flushed; on `XACT_EVENT_ABORT` it is *cleared*. **CONSTRUCT writeback never fires for the SPARQL Update path.** This is a regression from Assessment 10 CF-2 (over-flushing was bad; not flushing at all is much worse). The journal effectively becomes a no-op for the most-used write API.

**Recommended fix**: Add `crate::storage::mutation_journal::flush();` as the last line of `sparql_update()` immediately before the `affected` return ([src/sparql/execute.rs:651](../src/sparql/execute.rs#L651)) and as the last line of `execute_delete_insert()` ([src/sparql/execute.rs:796](../src/sparql/execute.rs#L796)). Optionally also wire it into the SPARQL audit-log block at L639-L649 as a defense-in-depth measure. Add a regression test: insert a CONSTRUCT writeback rule, run `sparql_update('INSERT DATA …')`, and assert the derived target graph is non-empty without calling `refresh_construct_rule`. The `cwb_write_path_equivalence.sql` test must include a SPARQL-Update arm — if it does not, that test is silently passing on the broken path.

### CF-B — Mutation journal has no `SubXact` callback; `ROLLBACK TO SAVEPOINT` leaves journal entries that fire on the next flush

**Location**: [src/lib.rs](../src/lib.rs#L2111-L2148) `register_xact_callback` and `xact_callback_c`. `grep -rn "SubXact" src/` returns zero matches in production source.

**Description**: The journal is a `thread_local! RefCell<Vec<JournalEntry>>` ([src/storage/mutation_journal.rs:42-44](../src/storage/mutation_journal.rs#L42-L44)). The xact-abort path clears it correctly. But PostgreSQL distinguishes top-level transaction events from sub-transaction events; `RegisterXactCallback` is fired only on top-level `BEGIN`/`COMMIT`/`ROLLBACK`, not on `SAVEPOINT`/`ROLLBACK TO SAVEPOINT`/`RELEASE SAVEPOINT`. To clear journal entries on subxact abort, `RegisterSubXactCallback` must be registered separately. It is not.

**Impact**: A user runs `BEGIN; SAVEPOINT s1; SELECT pg_ripple.insert_triple('a','p','b','g:src'); ROLLBACK TO SAVEPOINT s1; SELECT pg_ripple.insert_triple('a','p','c','g:other'); COMMIT;`. The first `insert_triple` writes a triple, records `g_id_src` in the journal, and **flushes** ([dict_api.rs:231](../src/dict_api.rs#L231)) — so on this code path the bug is masked. **However** with CF-A's fix in place (deferred per-statement flush via the xact callback), or in any path that accumulates without per-call flush (which is the design intent), the rolled-back triple's `g:src` entry survives the savepoint rollback and fires CWB derivation against a graph whose source row is gone. The downstream Delete-Rederive bookkeeping (`construct_rules/retract.rs`) cannot retract these because the provenance row was either also rolled back or points to a non-existent source. After CF-A is fixed correctly, this becomes immediately exploitable by any SAVEPOINT-using application.

**Recommended fix**: Register a `SubXactCallback` in `_PG_init` alongside the existing `RegisterXactCallback`. On `SUBXACT_EVENT_ABORT_SUB` (event 1 in PG18), call `mutation_journal::clear()`. The pattern mirrors the existing `xact_callback_c`. Add a regression test that does `BEGIN; SAVEPOINT s; INSERT triggering CWB; ROLLBACK TO SAVEPOINT s; COMMIT;` and asserts no derived triples were created.

### CF-C — `feature_status()` cites twelve non-existent `docs/src/reference/*.md` files; `validate-feature-status` CI job is provably bypassed

**Location**: [src/feature_status.rs](../src/feature_status.rs) cites `docs/src/reference/cdc.md` ([L353](../src/feature_status.rs#L353)), `construct-rules.md` ([L123](../src/feature_status.rs#L123)), `datalog.md` ([L157](../src/feature_status.rs#L157), [L166](../src/feature_status.rs#L166)), `development.md` ([L368](../src/feature_status.rs#L368)), `federation.md` ([L98](../src/feature_status.rs#L98), [L329](../src/feature_status.rs#L329)), `graphrag.md` ([L339](../src/feature_status.rs#L339)), `observability.md` ([L305](../src/feature_status.rs#L305)), `query-optimization.md` ([L295](../src/feature_status.rs#L295)), `shacl.md` ([L133](../src/feature_status.rs#L133), [L147](../src/feature_status.rs#L147)), `sparql.md` ([L62](../src/feature_status.rs#L62), [L71](../src/feature_status.rs#L71), [L80](../src/feature_status.rs#L80), [L89](../src/feature_status.rs#L89), [L113](../src/feature_status.rs#L113)), `storage.md` ([L176](../src/feature_status.rs#L176)), `vector-search.md` ([L319](../src/feature_status.rs#L319)). None of these files exist in `docs/src/reference/`. Only `arrow-flight.md`, `scalability.md`, `approximate-aggregates.md`, and `citus-service-pruning.md` exist.

The CI job at [.github/workflows/ci.yml](../.github/workflows/ci.yml) `validate-feature-status` runs a shell loop `psql -t -A -c "SELECT evidence_path FROM pg_ripple.feature_status() WHERE evidence_path IS NOT NULL;" | while IFS= read -r path; do if [ -n "$path" ] && [ ! -e "$path" ]; then echo "MISSING evidence path: $path"; echo "FAIL=1" >> "$GITHUB_ENV"; fi; done` and exits 1 if `FAIL=1`. With twelve missing paths, this should fail. v0.71.0 was tagged successfully.

**Description**: One of three explanations applies: (a) the SQL function is returning evidence_path strings prefixed with `ci/regress: …` (per [src/feature_status.rs:88](../src/feature_status.rs#L88) etc.), and `[ ! -e "$path" ]` is treating the string `ci/regress: property_paths.sql` as a non-existent path → marking FAIL=1 → meaning the job is *already* failing and is being treated as informational; (b) the job is not running on the release-tagging commit (only on PRs); (c) the `WHERE evidence_path IS NOT NULL` filter is excluding the rows in CI for some environment-specific reason. In all cases, the GATE-02 / GATE-03 release-truth premise is broken. The v0.70.0 changelog GATE-03 entry ("validate-feature-status CI job now fails hard when any cited evidence file is missing") is unverified.

**Impact**: Identical to Assessment 10 CF-4: release dashboards and `pg_ripple.feature_status()` — which exist precisely to give operators a machine-readable source of truth — are misleading by construction. Worse, fixing this requires either creating 12 stub docs pages or stripping the citations; both are easy, neither has been done across two releases.

**Recommended fix**: (1) Run the CI job locally on the v0.71.0 commit to confirm whether it is exiting non-zero or being suppressed. (2) If suppressed, remove `continue-on-error: true` from the `validate-feature-status` job. (3) Either create the twelve `docs/src/reference/*.md` stub pages (each can start as a one-paragraph "see [feature page] for now" placeholder) or remove the citations from `feature_status.rs`. (4) Add a GATE-04 regression: introduce a deliberately-broken evidence path on a feature branch and assert CI fails. (5) Strip the `ci/regress: …` prefix from evidence paths or have the validator treat such strings as logical references rather than filesystem paths.

### CF-D — Datalog, R2RML, and CDC write paths bypass mutation journal entirely (re-raise of MF-17 with confirmed scope)

**Location**: `src/datalog/` (no `mutation_journal::*`, no `insert_triple_by_ids`, no `batch_insert_encoded` calls); `src/r2rml*.rs` (same); `src/cdc.rs`, `src/cdc_bridge_api.rs` (same).

**Description**: Static analysis (grep) confirms the v0.70.0 BULK-01 fix only routed bulk-load triples through the journal. Datalog materialization (the dominant write path for any user with rules), R2RML virtualization (when materialized), and CDC ingestion all write to VP tables via either direct SQL `INSERT INTO _pg_ripple.vp_…` or by other helpers that do not touch the journal. The journal therefore never sees a Datalog-derived triple, an R2RML-materialized triple, or a CDC-ingested triple. CONSTRUCT writeback rules that depend on these graphs will never fire.

**Impact**: Any neuro-symbolic, governance, or analytics workload that combines Datalog inference with CONSTRUCT-writeback projections is silently broken. The implementation_plan.md architecture explicitly markets these as composable; in practice they are not.

**Recommended fix**: Audit each of the three subsystems to find their write surface. For Datalog, the lattice/fixed-point engine writes via either dedicated INSERT SQL or via `storage::batch_insert_encoded` (which does record_write per [storage/mod.rs:696](../src/storage/mod.rs#L696)) — if the latter, then per-batch journal entries exist but flush is missing. Add `mutation_journal::flush()` at the end of every `materialize_*` function in `src/datalog/`. Same pattern for `src/r2rml*.rs` and `src/cdc*.rs`. Add a regression test for each.

## High-Severity Findings

### HF-A — SBOM still contains stale `pg_ripple@0.51.0` entry inside components and is one release behind top-level

**Location**: [sbom.json](../sbom.json#L14) top-level `version` is `0.70.0` (was `0.51.0` in Assessment 10 — improvement). However grep confirms an additional `"name": "pg_ripple", "version": "0.51.0"` block inside `components[]`, and the project is currently at v0.71.0.

**Impact**: Downstream packagers and security scanners that consume the SBOM see two pg_ripple versions (0.70.0 and 0.51.0) and a top-level version that does not match the source tree. The v0.70.0 SBOM-02 changelog claim that the regenerated SBOM is committed back via release-bot is partially false: it ran for v0.70.0 but not for v0.71.0, and the inner `components` array was not deduplicated.

**Recommended fix**: Update the SBOM regeneration step to (a) overwrite all `pg_ripple` component entries, not append, and (b) run on every tag, including v0.71.0. Add a `just check-sbom-version` step to `assess-release` that fails if `sbom.json` `version` ≠ `Cargo.toml` version. Backfill v0.71.0 SBOM in a follow-up commit.

### HF-B — `pg_ripple_http` version frozen at 0.16.0 across 5+ extension releases

**Location**: [pg_ripple_http/Cargo.toml:3](../pg_ripple_http/Cargo.toml#L3) `version = "0.16.0"`. The COMPAT-01 startup check provides a runtime guard, but the version number is now meaningless: 0.16.0 corresponds to no specific extension feature set.

**Impact**: Operators upgrading the HTTP companion have no signal as to whether the new image is compatible. The compatibility matrix at [docs/src/operations/compatibility.md](../docs/src/operations/compatibility.md) is the only source of truth, and it must be updated by hand. The startup check fails closed (good), but the user has to deploy and observe the failure to learn about the mismatch.

**Recommended fix**: Either lockstep the version (`pg_ripple_http` 0.71.0 alongside extension 0.71.0) or adopt a public versioning scheme for the HTTP companion that signals which extension MAJOR/MINOR it tracks (e.g. `0.71.0-http.1`). Update `COMPATIBLE_EXTENSION_MIN` on every release.

### HF-C — Datalog/R2RML/CDC bypass not just CWB hook but all mutation-journal-driven side effects

**Location**: `src/datalog/`, `src/r2rml*.rs`, `src/cdc*.rs` (no `mutation_journal::*` calls anywhere).

**Description**: Beyond CWB (CF-D), the journal drives metric counters and (post-fix) any future side-effect routed through the kernel. The bypass means Datalog-derived writes are invisible to provenance accounting, OpenTelemetry write-spans, and any future RLS-aware audit. This is the same architectural failure mode as Assessment 9 — side effects attached at the wrong call boundary.

**Recommended fix**: Same as CF-D — route all materialization through the journal kernel. Make `batch_insert_encoded` the single mutation entry point for bulk-style writes, and make `insert_triple_by_ids` the single entry point for one-off writes; deprecate any direct VP-table SQL emitted from outside `src/storage/`.

### HF-D — `validate-feature-status` CI job runs on a fresh ext install, not a populated DB; promotion-path evidence cannot be checked

**Location**: [.github/workflows/ci.yml](../.github/workflows/ci.yml) `validate-feature-status` job body. The job runs `CREATE EXTENSION IF NOT EXISTS pg_ripple;` then immediately calls `feature_status()`. Promoted-VP, post-merge, and CDC-active rows of `feature_status()` may report different evidence paths in a populated database, but the CI never exercises that.

**Impact**: Even if all evidence paths existed, the CI would only test the cold-start view. Any feature whose `evidence_path` is computed dynamically (e.g. depends on whether a predicate is promoted) is not validated.

**Recommended fix**: Add a `validate-feature-status-populated` companion job that loads the LUBM 1k fixture, promotes a predicate, runs Datalog inference, then re-validates evidence paths. Consider statically deriving evidence paths into a `const` table to make validation a pure-Rust check.

### HF-E — Mutation journal kernel still doc-comment misleads about caller contract

**Location**: [src/storage/mutation_journal.rs:75-92](../src/storage/mutation_journal.rs#L75-L92) doc comment: "This must be called: At the end of every public `dict_api` write function. At the end of every bulk-load function (`load_turtle`, `load_ntriples`, etc.). At the end of every SPARQL Update execution." The third clause is **not honored** (CF-A). 

**Impact**: Future contributors will read the doc, see no flush in `sparql_update`, and either (a) assume there is a higher-level flush they missed, or (b) add a redundant flush somewhere else. Either outcome makes the bug harder to fix correctly.

**Recommended fix**: Either fix CF-A (add the flush) and leave the doc accurate, or update the doc to say "SPARQL Update path is currently broken — see CF-A" until the fix lands.

### HF-F — Plan cache key still does not include current_role or affected GUCs (Assessment 10 HF-6 unchanged)

**Location**: [src/sparql/plan_cache.rs:92](../src/sparql/plan_cache.rs#L92) `cache_key` function.

**Impact**: As Assessment 10 explained, today this is probably correct because PG enforces RLS at executor level, but any future translator change that takes RLS-derived shortcuts will silently produce wrong results. Roadmap puts this in v0.72.0 CACHE-01.

**Recommended fix**: Promote to v0.72.0 priority list; add `current_role` and a hash of translation-affecting GUCs to the cache key.

### HF-G — `recover_interrupted_promotions()` is exposed as a SQL function but not auto-invoked at startup

**Location**: [src/lib.rs](../src/lib.rs#L2098-L2104) comment: "PROMO-01 crash recovery is exposed as `pg_ripple.recover_interrupted_promotions()` so users can call it after an unclean shutdown. It is intentionally NOT called from `_PG_init` because SPI_connect inside `_PG_init` can corrupt the active snapshot context and break subsequent SQL in the same session."

**Impact**: After an unclean shutdown, dedicated VP tables can remain in `promotion_status='promoting'` indefinitely. Operators must remember to call the recovery function manually. There is no automated path (e.g. background worker that runs recovery on startup), no CHANGELOG warning, and no `pg_ripple.feature_status()` row that surfaces stuck promotions.

**Recommended fix**: Schedule `recover_interrupted_promotions()` from a background worker on startup, after SPI is safe. Alternatively, add a `feature_status()` row that reports the count of `promotion_status='promoting'` rows so monitoring can alert on it.

## Medium-Severity Findings

### MF-A — Plan cache invalidation on schema change (VP promotion, predicate add) not documented or implemented

**Location**: [src/sparql/plan_cache.rs:80](../src/sparql/plan_cache.rs#L80) `reset()` is exposed but no caller in `src/storage/promote.rs` invokes it after promotion.

**Impact**: A cached plan compiled when a predicate lived in `vp_rare` will continue to scan `vp_rare` after the predicate is promoted, missing data in the new dedicated `vp_{id}_main`/`_delta`. Worst case: silent under-counting of result rows for cached queries until backend restart.

**Recommended fix**: Call `plan_cache::reset()` at the end of `promote_predicate()` ([src/storage/promote.rs](../src/storage/promote.rs)). Add a regression test: cache a query against `vp_rare`, promote the predicate, re-run, assert correct row count.

### MF-B — `pg_ripple_http/Cargo.toml` not bumped on v0.71.0; `COMPATIBLE_EXTENSION_MIN` 0.70.0 admits no new extension constraints

**Location**: [pg_ripple_http/Cargo.toml:3](../pg_ripple_http/Cargo.toml#L3); per CHANGELOG `COMPATIBLE_EXTENSION_MIN = "0.70.0"`.

**Impact**: When v0.72.0 introduces a new endpoint that requires a v0.72.0 extension feature, the v0.16.0 HTTP companion will pass the compatibility check (extension >= 0.70.0) and then fail at runtime.

**Recommended fix**: Bump `pg_ripple_http` on every release; bump `COMPATIBLE_EXTENSION_MIN` whenever a new extension feature is added.

### MF-C — `merge_throughput_history.csv` row backfill (Assessment 10 MF-5) status unchanged

**Location**: [benchmarks/merge_throughput_history.csv](../benchmarks/merge_throughput_history.csv).

**Impact**: Trend-regression detection cannot fire without history. The v0.67.0 BENCH-02 weekly trend gate cannot have generated useful signal across two more releases.

**Recommended fix**: Backfill rows from CI artefacts; verify the workflow commits results.

### MF-D — `v070_features.sql` regression test missing (Assessment 10 MF-6 partial regression)

**Location**: `tests/pg_regress/sql/` contains `v067_features.sql`, `v068_features.sql`, `v069_features.sql` (added in v0.70.0 per CHANGELOG TEST-01/02), and `v071_features.sql` — but **no `v070_features.sql`**.

**Impact**: The most behaviorally-impactful release in the cycle has no dedicated regression file. Future regressions in BULK-01, FLUSH-01, RLS-SQL-01, GATE-03 are not guarded by a single version-scoped test.

**Recommended fix**: Add `v070_features.sql` covering: bulk-load CWB firing, SPARQL-Update CWB firing (currently broken — would catch CF-A), security_rls_role_injection regression, evidence-path existence subset.

### MF-E — `src/lib.rs` and `src/storage/mod.rs` continued to grow despite v0.69.0 split intent

**Location**: `wc -l`: `src/lib.rs` 2,413 (+151 since Assessment 10), `src/storage/mod.rs` 2,145 (+41 since Assessment 10).

**Impact**: The cross-cutting-concerns risk Assessment 8 / 9 / 10 cited has not improved.

**Recommended fix**: Split `src/lib.rs` GUC validators into `src/gucs/validators.rs` (currently inline at L280-L516); split `src/storage/mod.rs` into `dictionary_io.rs`, `htap_io.rs`, `vp_rare_io.rs`. Add a `scripts/check_module_size.sh` that fails if any `src/**.rs` exceeds 1500 lines (warn at 1000) and wire into CI.

### MF-F — Per-API flush in `dict_api.rs` makes single-triple inserts immune to journal-deferral but vulnerable to CWB-rule churn

**Location**: [src/dict_api.rs:230-231](../src/dict_api.rs#L230-L231) (`insert_triple` path flushes after every single quad).

**Description**: The original CF-2 problem (per-triple flush thrashing the CWB pipeline) is still present for the single-triple API — and now it is the *only* path that flushes correctly, because the SPARQL Update path never flushes (CF-A). Any user using `pg_ripple.insert_triple()` in a loop hits the original quadratic CWB cost. The intended deferral did not happen.

**Recommended fix**: Move the flush to a transaction-end mechanism that **does** work safely from inside PG callbacks. PostgreSQL's `RegisterXactCallback` *can* call SPI from `XACT_EVENT_PRE_COMMIT` if done carefully (the callback runs while the transaction is still open). Alternatively, install a per-statement boundary marker: have `dict_api::insert_triple` set a thread-local "pending flush" flag; have the executor-end hook ([src/lib.rs:245](../src/lib.rs#L245)) flush if set.

### MF-G — All twelve missing `docs/src/reference/*.md` files are referenced by user-facing `feature_status()` output, breaking the operator UX

**Location**: As CF-C; [src/feature_status.rs](../src/feature_status.rs) returns these strings to any operator who runs `SELECT * FROM pg_ripple.feature_status();`.

**Impact**: An operator running the function sees "evidence_path = `docs/src/reference/sparql.md`" and tries to open it; the file does not exist on the v0.71.0 mdBook site either.

**Recommended fix**: As CF-C — create the stubs or strip the citations.

### MF-H — `src/dictionary/inline.rs` has 13 `unwrap()` calls outside `#[cfg(test)]`; `src/flight.rs` has 7

**Location**: `grep -rn '\.unwrap()\|\.expect('` outside test code:
- src/dictionary/inline.rs: 13
- src/flight.rs: 7
- src/lib.rs: 5
- src/datalog/builtins.rs: 4
- src/datalog/stratify.rs: 3

**Impact**: Each `unwrap()` is a potential panic. In a PG18 backend, a panic translates to a backend crash and possibly the whole cluster restart.

**Recommended fix**: Audit each one. For invariant-true cases, document with `// SAFETY:`-style explanation. For value-returning cases, propagate via `?` or convert to `pgrx::error!()`.

### MF-I — Citus integration test `tests/integration/citus_rls_propagation.sh` is not part of any CI workflow

**Location**: [tests/integration/citus_rls_propagation.sh](../tests/integration/citus_rls_propagation.sh) (222 lines, exists). `grep -rn "citus_rls_propagation" .github/workflows/` returns no matches (not re-verified statically here).

**Impact**: The test exists but is not exercised on CI — equivalent to a docs change. Operators see the file in `feature_status()`, click through, and assume CI runs it.

**Recommended fix**: Add a `citus-integration.yml` workflow that runs the script weekly against a docker-compose Citus cluster. Mark required for v1.0.0.

### MF-J — Arrow Flight large-export integration test `tests/http_integration/arrow_export_large.sh` not wired to CI

**Location**: Same pattern as MF-I; the script exists ([tests/http_integration/arrow_export_large.sh](../tests/http_integration/arrow_export_large.sh), 202 lines) but no CI workflow invokes it.

**Recommended fix**: Add to a nightly integration workflow alongside Citus tests.

### MF-K — `roadmap/v0.70.0.md` and `roadmap/v0.71.0.md` not validated against `feature_status` post-release

**Location**: Roadmap status tracking is by-hand markdown.

**Recommended fix**: Add `scripts/check_roadmap_release_status.sh` that compares each `roadmap/v*.md` line `Status: …` to the highest-tagged release and fails if a tagged version is still "Planned".

### MF-L — `mutation_journal::flush()` doc comment promises three call-site contracts; only two are honored (Assessment 10 L-1 unchanged)

**Location**: [src/storage/mutation_journal.rs:75-92](../src/storage/mutation_journal.rs#L75-L92).

**Recommended fix**: As HF-E.

### MF-M — `apply_rls_to_vp_table` swallows policy creation errors via `let _ =`

**Location**: [src/security_api.rs:78-81](../src/security_api.rs#L78-L81), [L143](../src/security_api.rs#L143).

**Impact**: Promotion-time RLS failures are invisible. Operator cannot diagnose why a tenant suddenly sees data they should not.

**Recommended fix**: Convert `let _ = ...` to `if let Err(e) = ... { pgrx::warning!(...) }`.

### MF-N — `is_safe_role_name` rejects valid PostgreSQL identifiers that start with non-ASCII

**Location**: [src/security_api.rs:30-42](../src/security_api.rs#L30-L42). PG identifiers may start with any letter (per `[A-Za-z_\u00A0-\uFFFF]`), but the validator uses `is_ascii_alphabetic()`.

**Impact**: A role named `tenañt_a` cannot be used. Defensible (over-strict is safer than under-strict) but should be documented.

**Recommended fix**: Document the restriction in PT711 error message and in the SHACL/security reference docs.

### MF-O — `apply_rls_policy_to_all_dedicated_tables` not shown as fixed in the snippet read; verify role quoting also applied here

**Location**: [src/security_api.rs](../src/security_api.rs#L195-L200) (`apply_rls_policy_to_all_dedicated_tables`).

**Recommended fix**: Confirm the same `is_safe_role_name` + `quote_ident_safe` pattern is applied at every DDL `format!` call inside this helper.

### MF-P — Property-path `CYCLE` clause is correctly used; zero-length path inside `OPTIONAL` not statically tested

**Location**: [src/sparql/property_path.rs:245-260](../src/sparql/property_path.rs#L245-L260) — uses `CYCLE s, o SET _is_cycle USING _cycle_path`. Correct PG18 syntax. The Assessment 10 known bug "Property Path Inside GRAPH {} (vp_rare column bug)" from `pg_ripple_bugs.md` is not statically re-checked here.

**Recommended fix**: Add a regression test for `OPTIONAL { ?x :p* ?y }` and for `GRAPH <g> { ?x :p* ?y }` with a vp_rare predicate.

### MF-Q — Federation allowlist exists but no fuzz target for URL host parsing

**Location**: [src/sparql/federation.rs:214-229](../src/sparql/federation.rs#L214-L229), [src/citus.rs](../src/citus.rs) `extract_url_host`. No fuzz target named `url_parser` in `fuzz/fuzz_targets/`.

**Recommended fix**: Add `fuzz_targets/url_parser.rs` exercising both `extract_url_host` and the federation allowlist matcher for IPv6, IDN, port-only, mixed-case, and percent-encoded inputs.

### MF-R — Decoupled HTTP-companion fail-closed compatibility check is not documented as required for production

**Location**: [docs/src/operations/compatibility.md](../docs/src/operations/compatibility.md) (63 lines). Per CHANGELOG, `PG_RIPPLE_HTTP_SKIP_COMPAT_CHECK=1` disables the check for testing. The doc should explicitly say this env var must be unset in production.

**Recommended fix**: Add a "Production checklist" section at the top of `compatibility.md`.

### MF-S — Twelve fuzz targets at 60s/run nightly is still inadequate (Assessment 10 L-2 unchanged)

**Location**: [.github/workflows/fuzz.yml](../.github/workflows/fuzz.yml).

**Recommended fix**: Promote to 600s/target nightly and 14400s/target weekly.

### MF-T — `feature_status.rs` lacks an entry for `mutation_journal` itself (Assessment 10 L-3 unchanged)

**Recommended fix**: Add a row stating status `experimental` (after CF-A is fixed) and citing `src/storage/mutation_journal.rs`.

## Low-Severity Findings and Enhancements

- [src/lib.rs:2147](../src/lib.rs#L2147) comment "we do NOT call flush() here for XACT_EVENT_PRE_COMMIT" is the load-bearing TODO for CF-A. It documents the bug in the source.
- [pg_ripple_http/Cargo.toml](../pg_ripple_http/Cargo.toml#L3) — see HF-B.
- [rust-toolchain.toml](../rust-toolchain.toml) pins `channel = "stable"`, **not a specific version**, contrary to Assessment 10 prompt requirement and the comment claiming Dependabot pinning.
- [src/security_api.rs:160](../src/security_api.rs#L160) policy name still uses 64-bit xxh3 hash → 4B-graph collision risk per Assessment 10 L-4 unchanged.
- `arrow = "55"` not pinned to minor in [pg_ripple_http/Cargo.toml](../pg_ripple_http/Cargo.toml) — Assessment 10 L-5 unchanged.
- [benchmarks/merge_throughput_baselines.json](../benchmarks/merge_throughput_baselines.json) baseline still v0.53.0.
- `tests/pg_regress/sql/` count at ~190 (vs Assessment 10's 186) — TEST-01/02/03 added 3, plus v0.71.0 tests; not 300+ target.
- `clippy --deny warnings` CI gate (v0.62.0 claim) — not statically re-verified.
- `CONTRIBUTING.md` still not present at repository root.
- `docs/src/reference/cdc.md` etc. — see CF-C.
- The xact callback at [src/lib.rs:2128-2148](../src/lib.rs#L2128-L2148) is correctly designed for abort/commit cleanup but is not the right hook for flush; PG18 supports calling SPI from PRE_COMMIT in many configurations — the safety claim deserves a code comment with an authoritative source.
- `_PG_init` does not register a `RegisterEmitLogHook` to suppress secrets in error messages; nothing currently logs raw HMAC keys, but a defense-in-depth scan would be valuable.
- The `pg_ripple_http` `/metrics` route auth model is not documented per Assessment 10 L-6 unchanged.
- `src/llm/`, `src/kge.rs` modules are not represented in `feature_status.rs` — Assessment 10 L-7 unchanged.
- The 222-line Citus integration script and 202-line Arrow integration script are good, but neither appears in any `.github/workflows/*.yml` invocation list.
- The v0.70.0 changelog claim "All 192+ pg_regress tests pass" implies test count grew by 6 from v0.69.0's 186; given TEST-01/02/03 + v0.71.0 additions (HLL accuracy, Citus service pruning), the count is plausible.
- `roadmap/v0.71.0.md` Status field — not statically re-verified; assume Released ✅ per CHANGELOG.

## Positive Developments Since Assessment 10

- **BULK-01 fully resolved** — the Critical-1 finding from Assessment 10 is closed correctly. Every bulk loader flushes the journal.
- **SHACL-DOC-01 closes a three-assessment-running over-claim** — the `sh:SPARQLRule` documentation now matches code reality.
- **RLS-SQL-01 is a textbook fix** — `is_safe_role_name` validator + `quote_ident_safe` wrapper, applied at every DDL interpolation in `src/security_api.rs`. Includes a dedicated regression test.
- **Arrow Flight streaming verified statically** — `Body::from_stream(byte_stream)` with 64 KiB chunks, plus a 202-line integration test that asserts both `Transfer-Encoding: chunked` and an RSS bound.
- **Citus integration test exists** — 222-line script that drives a real docker-compose cluster and asserts cross-graph RLS.
- **Compatibility matrix is published** — `docs/src/operations/compatibility.md` plus a startup version-check guard (COMPAT-01).
- **HLL accuracy is documented and tested** — `docs/src/reference/approximate-aggregates.md` + `hll_accuracy.sql` regression test.
- **Legacy `.sh` gate scripts deleted** — MF-2 from Assessment 10 closed cleanly.
- **`recover_promotions.sql` regression test exists** — MF-4 from Assessment 10 closed.
- **`v067_features.sql` and `v069_features.sql` exist** — partial close of MF-6.
- **README version-drift checker added** — `scripts/check_readme_version.sh`.
- **GATE-04: legacy gate scripts removed** — MF-2 closed.

## Gap Analysis by Area

### 1. Correctness Bugs

State: **Worse than Assessment 10**. CF-A (SPARQL Update never flushes) is more severe than CF-2 was. CF-D (Datalog/R2RML/CDC bypass) is unchanged.

Top gaps: CF-A, CF-B (SubXact), CF-D.

### 2. Security

State: **Improved**. RLS DDL injection closed correctly; Arrow Flight default-deny preserved; SERVICE allowlist intact.

Top gaps: only one `SECURITY DEFINER` (audited), no new attack surface; `cargo audit` not re-run statically.

### 3. Performance & Scalability

State: **Mixed**. Arrow streaming verified; bulk-load journal does not regress hot path. CF-A means SPARQL-Update CWB cost is now zero (fast but wrong); MF-F means single-triple insert is still quadratic on rule count.

Top gaps: MF-F (per-API-call flush in `dict_api.rs`); MF-A (plan cache invalidation on promotion); MF-C (no merge-throughput history).

### 4. Test Coverage

State: **Slightly improved**. New regression tests for recover_promotions, hll_accuracy, citus_service_pruning, security_rls_role_injection. Two integration scripts added.

Top gaps: MF-D (no v070_features.sql); MF-I/MF-J (integration scripts not in CI); CF-A would have been caught by a cwb-via-sparql-update test that does not exist.

### 5. Code Quality & Architecture

State: **Worse**. `src/lib.rs` and `src/storage/mod.rs` continued to grow.

Top gaps: MF-E; MF-H (unwraps in dictionary/inline.rs and flight.rs); HF-E (doc-comment lies about caller contract).

### 6. Standards & Feature Gaps

State: **Unchanged**. No new SPARQL 1.1 work since v0.69.0. SHACL-AF (`sh:SPARQLRule`) honestly marked as planned. No SPARQL 1.2 / RDF 1.2 progress.

Top gaps: SHACL-AF beyond `sh:TripleRule`; SPARQL 1.2 tracking issue (planned for v0.73.0).

### 7. Documentation & UX

State: **Mixed**. README, SBOM (top-level), and SHACL doc all updated correctly. Twelve `docs/src/reference/*.md` evidence files still missing despite GATE-03 fix.

Top gaps: CF-C (twelve missing docs); MF-D (no v070 test).

### 8. Dependency / Supply-Chain Health

State: **Marginally improved**. SBOM top-level updated to 0.70.0. `rust-toolchain.toml` still pins `stable` (not a specific version). `pg_ripple_http` arrow dep still loose.

Top gaps: HF-A (SBOM regenerated for v0.70.0 but not v0.71.0; inner pg_ripple@0.51.0 still present); rust-toolchain stable-channel pin contradicts Assessment 10 expectation.

### 9. CI / Release Discipline

State: **Mostly intact**. SHA pinning preserved; legacy gate scripts deleted; `validate-feature-status` job exists but is provably bypassed for the missing docs paths.

Top gaps: CF-C (validate-feature-status bypass).

### 10. Resolution-of-prior-findings tracking

Eight Assessment 10 findings closed cleanly; one (HF-7) still open as CF-B; one (CF-2) replaced by a worse bug (CF-A); two (CF-4, MF-17) only partially addressed; eight unchanged or pushed to v0.72.0.

## Maturity Assessment

| Area | Rating | Δ from A10 | Evidence | Key remaining gap |
|---|---|---|---|---|
| Correctness & Semantic Fidelity | **C** | ↓ from B− | CF-A (SPARQL Update never flushes journal); CF-D (Datalog/R2RML/CDC bypass). | CF-A, CF-B, CF-D. |
| Bugs & Runtime Safety | **B** | = | RLS injection closed; xact-abort journal clear works. | MF-H (unwraps in dictionary/inline.rs and flight.rs). |
| Code Quality & Maintainability | **C+** | ↓ from B− | `src/lib.rs` 2,413 (+151), `src/storage/mod.rs` 2,145 (+41). | MF-E, MF-L (HF-E). |
| Performance & Scalability | **C+** | = | Arrow streaming real; per-API flush still quadratic in rule count for single-triple inserts. | MF-F, MF-A. |
| Security | **B−** | ↑ from C+ | RLS DDL quoting + validation; allowlist; default-deny tickets; only one SECURITY DEFINER. | Plan-cache key (HF-F). |
| Test Coverage | **B−** | = | TEST-01/02/03 added; integration tests exist but not in CI. | MF-D, MF-I, MF-J, CF-A regression test. |
| Documentation & DX | **C+** | ↑ from C | README current, SHACL honest, compatibility matrix published, 4 of 16 reference docs created. | CF-C (12 still missing). |
| CI/CD & Release Process | **B−** | = | Legacy gate scripts deleted; validate-feature-status exists but bypassed. | CF-C, MF-I/J. |
| Architecture & Long-term Health | **B−** | = | Mutation journal contract is right; FLUSH wire-up is wrong (CF-A) and Datalog/R2RML/CDC outside the contract (CF-D). | CF-A, CF-D, MF-E. |
| Ecosystem Completeness | **B−** | = | Compatibility matrix; HTTP companion versioning unchanged; no new ecosystem work. | HF-B. |
| SPARQL 1.1 correctness | **B+** | = | spargebra parser, sparopt optimizer, property paths use CYCLE. | Property paths inside GRAPH/OPTIONAL with vp_rare (known bug). |
| SPARQL performance | **B−** | = | TopN push-down (v0.51.0); WCOJ; plan cache. | Plan cache invalidation on promotion (MF-A). |
| SHACL Core | **A−** | = | All 35 constraints implemented (v0.48.0). | None known. |
| SHACL-AF | **C+** | ↑ honest | `sh:TripleRule` real; `sh:SPARQLRule` honestly marked as not supported. | `sh:SPARQLRule` not implemented. |
| Datalog soundness | **B+** | = | Magic sets, well-founded semantics, DRed, parallel strata. | DRed bookkeeping after CDC-driven deletes (CF-D). |
| OWL 2 RL completeness | **B** | = | LUBM 14-query suite required in CI. | Some axiom groups still informational. |
| RDF-star | **B** | = | oxrdf direct dep; encoding round-trips claimed. | Annotation-pattern tests not statically re-verified. |
| HTAP storage correctness | **B+** | = | Merge worker + tombstone exclusion + CASCADE drop fix. | Concurrent-writer journal stress test still missing. |
| Bulk load correctness | **A−** | ↑ from B− | BULK-01 closes the journal contract for all 8 loaders. | None known. |
| Mutation journal / CWB hooks | **C** | ↓ from B− | Bulk-load: correct. SPARQL Update: **broken (CF-A)**. Datalog/R2RML/CDC: **bypass (CF-D)**. | CF-A, CF-B, CF-D. |
| RLS / multi-tenancy | **B+** | ↑ from B | RLS DDL quoting + role validation + dedicated-VP propagation. | Citus runtime not in CI (MF-I). |
| Citus integration | **B−** | ↑ from C+ | Integration script exists; not in CI. | MF-I, MF-8 multi-node EXPLAIN comparison. |
| Arrow Flight | **B+** | ↑ from B− | Body::from_stream + 64 KiB chunks + integration test. | Replay protection (v0.72.0 ARROW-REPLAY-01). |
| HTTP API completeness | **B−** | = | Routing split; Datalog REST; metrics. | SPARQL/Update over HTTP completeness vs spec. |
| Observability | **B−** | = | OTLP traceparent (v0.61.0); Prometheus metrics. | Cross-process aggregation (MF-13). |
| Security posture | **B−** | ↑ from C+ | RLS DDL hardening; default-deny tickets. | Threat model still unpublished. |
| CI / release discipline | **B−** | = | SHA pinning; gates exist. | CF-C bypass; integration tests not in CI. |
| Documentation accuracy | **C+** | ↑ from C | README current; SHACL honest; compatibility matrix. | CF-C. |
| Test coverage breadth | **B−** | = | 190+ pg_regress; 12 fuzz targets. | MF-D; CF-A regression test. |
| Dependency health | **B−** | ↑ from C+ | SBOM updated to 0.70.0 top-level; legacy `.sh` deleted. | HF-A (inner pg_ripple@0.51.0 stale; SBOM not regenerated for v0.71.0). |

## Recommended Action Plan

| # | Priority | Action | Affected files | Effort | Closes |
|---|---|---|---|---|---|
| 1 | **Critical, Small** | Add `mutation_journal::flush()` at the end of `sparql_update()` and `execute_delete_insert()`. Add a `cwb_sparql_update_test.sql` regression. | [src/sparql/execute.rs](../src/sparql/execute.rs#L651), [L796](../src/sparql/execute.rs#L796); `tests/pg_regress/sql/`. | 2h | **CF-A** |
| 2 | **Critical, Small** | Implement `RegisterSubXactCallback` to clear the journal on subxact abort. | [src/lib.rs](../src/lib.rs#L2111-L2148); regression test. | 3h | **CF-B** / HF-7 |
| 3 | **Critical, Medium** | Wire `mutation_journal::flush()` into Datalog materialization, R2RML materialization, and CDC ingest. | `src/datalog/`, `src/r2rml*.rs`, `src/cdc*.rs`. | 8h | **CF-D** / HF-C / MF-17 |
| 4 | **Critical, Small** | Either create the 12 missing `docs/src/reference/*.md` stubs **or** strip the citations from `feature_status.rs`. Run `validate-feature-status` locally on the v0.71.0 commit; fix the bypass. | [src/feature_status.rs](../src/feature_status.rs); `docs/src/reference/`; [.github/workflows/ci.yml](../.github/workflows/ci.yml). | 4h | **CF-C** |
| 5 | **High, Small** | Regenerate SBOM for v0.71.0; deduplicate inner `pg_ripple@0.51.0` block. | [sbom.json](../sbom.json); `release.yml`. | 1h | **HF-A** / HF-5 |
| 6 | **High, Small** | Bump `pg_ripple_http` version to track extension; bump `COMPATIBLE_EXTENSION_MIN` to 0.71.0. | [pg_ripple_http/Cargo.toml](../pg_ripple_http/Cargo.toml#L3); `pg_ripple_http/src/main.rs`. | 1h | **HF-B** / HF-8 |
| 7 | **High, Small** | Add `v070_features.sql` covering BULK-01, FLUSH-01 (post-fix), RLS-SQL-01, GATE-03 evidence-path subset. | `tests/pg_regress/sql/v070_features.sql` (new). | 3h | **MF-D** / MF-6 |
| 8 | **High, Medium** | Move per-API-call `flush()` in `dict_api.rs` to a per-statement boundary using executor-end hook. | [src/dict_api.rs](../src/dict_api.rs#L230-L298); [src/lib.rs](../src/lib.rs#L245). | 6h | **MF-F** / CF-2 |
| 9 | **High, Small** | Wire `tests/integration/citus_rls_propagation.sh` and `tests/http_integration/arrow_export_large.sh` into a weekly CI workflow. | `.github/workflows/integration.yml` (new). | 4h | **MF-I, MF-J** |
| 10 | **High, Small** | Call `plan_cache::reset()` at the end of `promote_predicate()`. | [src/storage/promote.rs](../src/storage/promote.rs); regression test. | 2h | **MF-A** |

Total estimated effort: ~34 hours to close all four Critical findings, three of the four Highs, and three of the Mediums.

## Appendix: Unchecked Items (Require Running Environment)

| Item | Suggested verification |
|---|---|
| Does `validate-feature-status` job actually fail on the v0.71.0 commit? | `gh run list --workflow=ci.yml --branch=main --status=success` then drill into the `validate-feature-status` job log; confirm `MISSING evidence path:` does not appear. |
| Does `tests/integration/citus_rls_propagation.sh` pass against a real Citus cluster? | `docker compose up -d citus-coordinator citus-worker-1 citus-worker-2; bash tests/integration/citus_rls_propagation.sh`. |
| Does `tests/http_integration/arrow_export_large.sh` keep RSS under 512 MiB for 10M-triple export? | `bash tests/http_integration/arrow_export_large.sh` with `EXPECTED_MAX_RSS_MB=512`. |
| W3C SPARQL 1.1 conformance pass rate | `cargo test --test w3c_suite -- --nocapture`. |
| Apache Jena suite pass rate | `cargo test --test jena_suite -- --nocapture`. |
| WatDiv / BSBM regression vs baselines | `bash benchmarks/ci_benchmark.sh`. |
| LUBM 14-query OWL RL pass | `cargo test --test lubm_suite`. |
| OWL 2 RL conformance | `cargo test --test owl2rl_suite`. |
| Mutation-journal stress under concurrent writers | A pgbench script with 32 concurrent sessions calling `pg_ripple.insert_triple` and `sparql_update` while a CONSTRUCT writeback rule is active. |
| `recover_interrupted_promotions()` after simulated kill -9 | A test harness that issues `pg_ripple.promote_predicate()`, kills the backend mid-copy, restarts PG, and asserts `recover_interrupted_promotions()` completes. |
| `mdbook build docs/` warning-free | `cd docs && mdbook build`. |
| `cargo audit` clean | `cargo audit`. |
| `cargo deny` clean | `cargo deny check`. |
| `clippy --deny warnings` clean | `cargo clippy --all-targets --all-features -- -D warnings`. |
| All 12 fuzz targets compile | `cargo +nightly fuzz check`. |
| `pg_dump`/`pg_restore` round-trip | `pg_dump -Fc | pg_restore` against a non-trivial pg_ripple database; verify SPARQL queries return identical results. |
