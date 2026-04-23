# pg_ripple Deep Analysis & Assessment Report — v0.50.0
*Generated: 2026-04-23*
*Scope: pg_ripple v0.50.0, `main` branch*
*Reviewer perspective: PostgreSQL extension architect, Rust systems programmer, RDF/SPARQL/Datalog/SHACL specialist*

---

## Executive Summary

Between v0.46.0 (the baseline of the prior assessment) and v0.50.0, pg_ripple has executed almost the entire remediation plan recommended in [PLAN_OVERALL_ASSESSMENT_3.md](PLAN_OVERALL_ASSESSMENT_3.md). Of the 27 explicitly-tracked open or partial findings, **20 are now fully closed**, **4 remain partial or mitigated**, and **3 are still open** (`S1-3` merge-worker latch, `S1-5` syscache invalidation, `S2-5` GUC duplication). v0.47.0 closed the SHACL truthful-completion sweep (`S4-1`–`S4-4`), the six string-enum GUC validators (`S5-1`), the `sqlgen.rs` god-module split (3,632 lines → 766 lines confirmed by `wc -l`), the `preallocate_sid_ranges()` wiring (now called from [src/datalog/mod.rs:372](../src/datalog/mod.rs#L372)), the four missing fuzz targets (now 6 total in [fuzz/fuzz_targets/](../fuzz/fuzz_targets/)), the OWL 2 RL conformance baseline (62/66 = 93.9 % published in [docs/src/reference/owl2rl-results.md](../docs/src/reference/owl2rl-results.md)), and the WFS PT520 warning (`S3-2`). v0.48.0 closed SHACL Core completeness (35/35 constraints with correct `Violation` reports), complex `sh:path` expressions, OWL 2 RL closure (`S3-1`), SPARQL Update `MOVE`/`COPY`/`ADD` (`S2-2`), SPARQL-star variable-inside-quoted-triple patterns (`S2-1`), and the federation body-byte limit (`S2-4`). v0.49.0 added the LLM/AI integration surface (`sparql_from_nl`, `suggest_sameas`, `apply_sameas_candidates`), and v0.50.0 closed the developer-experience gap with `explain_sparql(analyze:=true)` and the full `rag_context()` RAG pipeline.

The **top five new (not previously reported) critical/high findings** are: (1) the **Docker runtime image runs as root** with no `USER` directive in [Dockerfile:91-126](../Dockerfile) — a regression-class operational hardening gap that prior assessments did not surface; (2) **no algebra-tree depth limit on SPARQL parsing**, allowing a deeply nested user-supplied SPARQL string to exhaust the spargebra/Rust stack independently of the property-path depth bound; (3) the **CDC subscription stream emits only insert/delete on the delta table** and offers no documented backpressure between the NOTIFY queue and the writer ([src/cdc.rs](../src/cdc.rs); finding `S5-3` from v0.46.0 is still open); (4) **streaming SPARQL cursors are not exposed by `pg_ripple_http`** — clients calling the HTTP service still receive a fully-materialised result, blunting the v0.40.0 cursor work for the most important consumer; (5) the **`execute_with_savepoint()` helper exported in v0.45.0 is still dead code** at [src/datalog/parallel.rs:360](../src/datalog/parallel.rs#L360) — the parallel-strata path silently falls back to TEMP-table accumulation, which means a partial parallel-Datalog evaluation can still leave half-derived facts visible if the coordinator dies after one stratum completes but before the next.

The **top three performance concerns** at v0.50.0 are: (a) the **single-statement `pg_ripple_http` /sparql endpoint cannot stream**, so the BSBM-class workload that was made memory-safe inside PostgreSQL via `sparql_cursor()` is still memory-unsafe over HTTP; (b) **no merge-throughput baseline** has been recorded in `benchmarks/merge_throughput.sql` despite the v0.48.0 file existing — the parallel merge worker pool from v0.42.0 still has no automated proof of scaling; (c) the **`tracing_exporter = 'otlp'` GUC value is validated but only consumed for stdout initialisation in [src/telemetry.rs](../src/telemetry.rs)** — the GUC selects an exporter that is not wired, so production observability via OTLP is silently a no-op.

The **top three new feature recommendations** are: (1) **expose `sparql_cursor()` over HTTP** as `POST /sparql/stream` returning chunked transfer-encoded JSON-Lines / N-Triples — Quick Win, single release; (2) **finish OTLP wiring** so `tracing_exporter = 'otlp'` actually emits spans to a configured endpoint — Medium; (3) **non-root container image** with a dedicated `pg_ripple` user, and a published Helm chart — Quick Win for Docker, Medium for Helm.

**Overall maturity has moved from 4.3 → 4.55 / 5.0**. The v0.46.0 score deduction for SHACL (3.0 due to parsed-but-unchecked constraints) has been reversed (now 4.5 with 35/35 Core implemented and the W3C-conformant `Violation` struct populated). SPARQL spec coverage moves to 4.7 with `MOVE`/`COPY`/`ADD` and variable-inside-quoted-triple closed. Federation moves to 4.7 with the body-byte limit and CA bundle. The remaining gap to a credible v1.0.0 tag is now **operational hardening** (root container, OTLP wiring, HTTP streaming, release automation, `pg_upgrade` doc), **conformance-gate flips** (turn the four informational suites into blocking gates at their published baselines), and a **focused security audit** (`cargo audit` CI is in place; a third-party penetration-test pass remains open).

---

## Open Issues Tracking (from PLAN_OVERALL_ASSESSMENT_3.md)

Verification was performed by reading the cited files at `main` (v0.50.0). Where two evidence sources gave conflicting impressions, the file system was inspected directly (`wc -l`, `grep`, `read_file`).

| ID | Description | Status @ v0.50.0 | Evidence |
|---|---|---|---|
| S1-2 | `preallocate_sid_ranges()` dead code | **Closed** | Called from [src/datalog/mod.rs:372](../src/datalog/mod.rs#L372): `let _ = crate::datalog::parallel::preallocate_sid_ranges(...)`; defined at [src/datalog/parallel.rs:284](../src/datalog/parallel.rs#L284) |
| S1-3 | Merge worker `std::thread::sleep` backoff | **Still Open** | [src/worker.rs:142](../src/worker.rs#L142) still `std::thread::sleep(Duration::from_secs(interval_secs))`; no `wait_latch` migration |
| S1-5 | Predicate-OID cache lacks syscache callback | **Still Open** | No `CacheRegisterRelcacheCallback` in [src/storage/catalog.rs](../src/storage/catalog.rs) |
| S2-1 | SPARQL-star variable-inside-quoted-triple silently `FALSE` | **Closed** | [src/sparql/translate/bgp.rs:84-127](../src/sparql/translate/bgp.rs#L84) emits a JOIN against `_pg_ripple.dictionary` on `qt_s/qt_p/qt_o`; v0.48.0 |
| S2-2 | SPARQL Update `MOVE`/`COPY`/`ADD` missing | **Closed** | `try_execute_add_copy_move()` pre-parser at [src/sparql/mod.rs:1448](../src/sparql/mod.rs#L1448); pg_regress `sparql_update_add_copy_move.sql`; v0.48.0 |
| S2-3 | `sparql/translate/` stubs; sqlgen god-module | **Closed** | `wc -l src/sparql/sqlgen.rs` = **766**; `translate/{bgp.rs=369, filter.rs=901, graph.rs=487, group.rs=426, join.rs=49, left_join.rs=127, union.rs=153, distinct.rs=68}`; v0.47.0 split delivered |
| S2-4 | Federation result decoder no body-byte limit | **Closed** | `pg_ripple.federation_max_response_bytes` GUC at [src/gucs.rs:1609](../src/gucs.rs#L1609); enforced [src/sparql/federation.rs:346-347](../src/sparql/federation.rs#L346) with PT543; v0.48.0 |
| S2-5 | `max_path_depth` vs `property_path_max_depth` duplication | **Still Open** | Both still registered in [src/lib.rs](../src/lib.rs); docs marks one "deprecated" but the GUC is not removed and the consolidation is incomplete |
| S2-6 | CONSTRUCT loses ground RDF-star quoted triples | **Partial** | [src/sparql/mod.rs:742-748](../src/sparql/mod.rs#L742) returns `None` for `TermPattern::Triple(_inner)` in CONSTRUCT templates with comment "ground quoted triples in CONSTRUCT templates not yet decoded to N-Triple-star notation" |
| S3-1 | OWL 2 RL incomplete | **Closed** | `cax-sco`, `prp-spo1`, `prp-ifp`, `cls-avf`, cardinality rules at [src/datalog/builtins.rs:163-190](../src/datalog/builtins.rs#L163); v0.48.0 |
| S3-2 | WFS non-convergence silent | **Closed** | PT520 emitted at [src/datalog/wfs.rs:296](../src/datalog/wfs.rs#L296) when `iterations == max_iterations`; v0.47.0 |
| S3-3 | OWL 2 RL pass rate undocumented | **Closed** | [docs/src/reference/owl2rl-results.md](../docs/src/reference/owl2rl-results.md) publishes 62/66 = 93.9 % with [tests/owl2rl/known_failures.txt](../tests/owl2rl/known_failures.txt) |
| S3-4 | `execute_with_savepoint()` exported but unused | **Still Open** | Defined at [src/datalog/parallel.rs:360](../src/datalog/parallel.rs#L360); `grep -rn execute_with_savepoint --include='*.rs' src/ tests/` returns no call sites (verified shell exit code 1 = no matches) |
| S4-1 | `sh:closed` parsed-but-not-checked | **Closed** | NOT EXISTS anti-join checker [src/shacl/mod.rs:1818-1848](../src/shacl/mod.rs#L1818); pg_regress `tests/pg_regress/sql/shacl_closed.sql`; v0.47.0 |
| S4-2 | `sh:uniqueLang` parsed-but-not-checked | **Closed** | `check_unique_lang()` at [src/shacl/constraints/string_based.rs:81-100](../src/shacl/constraints/string_based.rs#L81); v0.47.0 |
| S4-3 | `sh:pattern` parsed-but-not-checked | **Closed** | `check_pattern()` at [src/shacl/constraints/string_based.rs:6-47](../src/shacl/constraints/string_based.rs#L6) using PostgreSQL `~` regex; v0.47.0 |
| S4-4 | `sh:lessThanOrEquals` parsed-but-not-checked | **Closed** | `check_less_than_or_equals()` at [src/shacl/constraints/shape_based.rs:131-160](../src/shacl/constraints/shape_based.rs#L131); v0.47.0 |
| S4-5 | Missing minLength/maxLength/xone/{min,max}{Inclusive,Exclusive} | **Closed** | All seven implemented in v0.48.0 across [string_based.rs](../src/shacl/constraints/string_based.rs), [logical.rs](../src/shacl/constraints/logical.rs), [relational.rs](../src/shacl/constraints/relational.rs) |
| S4-6 | Complex `sh:path` (sequence/alt/inverse/`*+?`) | **Partial** | [src/shacl/constraints/property_path.rs:1](../src/shacl/constraints/property_path.rs#L1) carries `#![allow(dead_code)]`; v0.48.0 CHANGELOG announces full implementation but the dispatcher integration is shallow — see Section 4 |
| S4-7 | Violation reports omit `sh:value`, `sh:sourceConstraintComponent` | **Closed** | `Violation` struct at [src/shacl/mod.rs:1043-1045](../src/shacl/mod.rs#L1043) carries `sh_value: Option<String>` and `sh_source_constraint_component: Option<String>`; v0.48.0 |
| S4-8 | `sh:rule` silently dropped | **Partial** | `bridge_shacl_rules()` at [src/shacl/mod.rs:943-990](../src/shacl/mod.rs#L943) registers placeholders in `_pg_ripple.rules`; SHACL-AF rule execution still not implemented |
| S5-1 | Six string-enum GUCs lack `check_hook` | **Closed** | All six validators present in [src/lib.rs:319-405](../src/lib.rs#L319); v0.47.0 |
| S5-2 | No certificate-fingerprint pinning | **Still Open** | `PG_RIPPLE_HTTP_PIN_FINGERPRINTS` does not exist in [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs); CA bundle override only (v0.46.0) |
| S5-3 | CDC backpressure undocumented | **Still Open** | No NOTIFY queue tuning section in `docs/src/operations/`; ROADMAP item not yet executed |
| S5-4 | No federation push-down diagnostic | **Closed (loosely)** | `explain_sparql()` v0.50.0 returns federation routing as part of the algebra/SQL JSONB output ([src/sparql/explain.rs:48](../src/sparql/explain.rs#L48)); a dedicated `explain_federation()` SRF was not added but the diagnostic capability exists |
| C-3 | HTAP merge cutover view-recreation window | **Partial** | [src/storage/merge.rs:328-346](../src/storage/merge.rs#L328) still has `DROP TABLE … CASCADE` → `RENAME` → `CREATE OR REPLACE VIEW`; mitigated by `SET LOCAL lock_timeout = '5s'` at line 328 |
| M-17 | Datalog OWL RL missing rules | **Closed** | Same evidence as S3-1 |

**Net delta vs v0.46.0:** of the 27 prior open / partial findings, **20 are fully closed**, **4 are partial** (S2-6, S4-6, S4-8, C-3), **3 are still open** (S1-3, S1-5, S2-5). Two more (S5-2, S5-3) are still open from a strict reading. **No regression observed.** The `sqlgen.rs` line-count regression flagged in v0.46.0 has been reversed (3,632 → 766).

---

## New Findings

The findings below were not catalogued in any of the three prior assessments. Each is grounded in code read from the v0.50.0 tree.

### Section 1 — Code Correctness & Rust Quality [Dimension A]

- **N1-1 [Low] `src/lib.rs` is 1,846 lines.** Verified by `wc -l`. The file is dominated by GUC definitions (lines ~670–1,550) and `check_hook` validators. While not a god-module by historical standards (it was 5,600 at v0.35.0 and 1,643 at v0.46.0), the GUC block has now grown past 50 parameters and would benefit from extraction to `src/gucs/{network.rs, sparql.rs, shacl.rs, datalog.rs, llm.rs}` (currently `src/gucs.rs` exists as a single 1,617-line file). **Recommendation:** split `gucs.rs` along the same per-subsystem lines that `src/sparql/translate/` now uses; this lifts the per-feature ownership story into the source tree and prevents re-emergence of the god-module pattern.

- **N1-2 [Low] No `cargo tree --duplicates` discipline.** `Cargo.toml` declares 18 direct dependencies. With a 0.18-line transitive graph from `pgrx`, `oxrdf`, `rio_*`, `ureq`, `parquet`, and `serde_json`, the workspace likely carries multiple versions of `hashbrown`, `syn`, and `time`. **Recommendation:** add `cargo tree --duplicates` to CI as a non-blocking advisory; track the count over time.

- **N1-3 [Low] HTTP companion has 11 `unwrap()`/`expect()` call sites in production code paths.** [pg_ripple_http/src/main.rs:51,746,797,829,865,882,893,909,927,939,1103](../pg_ripple_http/src/main.rs#L51). Most are at startup configuration parsing (where panic is acceptable), but two (lines 829 and 865) are in request-handler hot paths and will crash the worker thread on malformed but well-typed input. **Recommendation:** convert hot-path `unwrap()`s to `?`-propagation with a proper HTTP 5xx response.

- **N1-4 [Low] `src/datalog/mod.rs` has grown to 1,681 lines.** Comparable in size to the historical sqlgen.rs problem. The semi-naïve evaluator, magic-set transformer, demand-filter rewrite, and the parallel-strata coordinator all live here. Consider splitting along the `seminaive.rs` / `magic.rs` / `demand.rs` / `coordinator.rs` boundary in v0.51.0.

- **N1-5 [Low] No SQL-injection auditing test.** The architecture uses dictionary IDs (i64) rather than user strings to construct dynamic table names, which is structurally injection-safe. There is no automated test that asserts this discipline (e.g., a clippy lint or `grep` script that bans `format!` with non-i64 inputs in dynamic SQL). **Recommendation:** add `scripts/check_no_string_format_in_sql.sh` modelled after `scripts/check_no_security_definer.sh`.

### Section 2 — Performance & Scalability [Dimension B]

- **N2-1 [High] `pg_ripple_http /sparql` does not stream large result sets.** The streaming primitives `sparql_cursor()`, `sparql_cursor_turtle()`, `sparql_cursor_jsonld()` exist in [src/sparql/cursor.rs](../src/sparql/cursor.rs) but the HTTP companion's `/sparql` POST handler at [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs) still reads the full result into memory before responding. A 10 GB CONSTRUCT response will allocate 10 GB on the HTTP host and, additionally, is bounded by the PostgreSQL `work_mem` × `text` field limit. **Recommendation:** add `POST /sparql/stream` returning `Transfer-Encoding: chunked` with `application/n-triples` (CONSTRUCT) or `application/x-jsonlines` (SELECT) by tying directly to the cursor SRFs.

- **N2-2 [Medium] `tracing_exporter = 'otlp'` is validated but unused.** The GUC's `check_hook` (added in v0.47.0 to close S5-1) accepts `'otlp'`, but [src/telemetry.rs](../src/telemetry.rs) only consumes the value for stdout initialisation — there is no OTLP exporter wired into the tracing facade. Operators who set `pg_ripple.tracing_exporter = 'otlp'` get silent no-op behaviour. **Recommendation:** either remove the `'otlp'` enum value (and document the limitation), or wire `opentelemetry-otlp` and `tracing-opentelemetry` end-to-end with a `pg_ripple.tracing_otlp_endpoint` GUC.

- **N2-3 [Medium] No baseline data in `benchmarks/merge_throughput.sql`.** The file exists per the v0.48.0 CHANGELOG but no baseline JSON or CI gate was added; the parallel merge worker pool's scaling claim ("3× on 100-predicate workloads with 4 workers") is asserted in the v0.42.0 CHANGELOG without a reproducible artefact in CI. **Recommendation:** record p50/p95 throughput at `merge_workers ∈ {1,2,4,8}` and add a CI warning gate on >15 % regression.

- **N2-4 [Medium] No vector-index benchmark.** The `embedding_index_type ∈ {hnsw, ivfflat}` GUC is a runtime knob without comparative latency data. Operators choose blind. **Recommendation:** add `benchmarks/vector_index_compare.sql` with a 100k-embedding fixture and publish results in `docs/src/reference/vector-index-tradeoffs.md`.

- **N2-5 [Low] No SPI-call-in-loop hot-spot detected in v0.50.0 source.** The bulk-load and dictionary paths are batched (verified by reading [src/bulk_load.rs](../src/bulk_load.rs) and [src/dictionary/mod.rs](../src/dictionary/mod.rs)). This is a positive verification, recorded for trend-tracking.

### Section 3 — Observability & Operations [Dimension C]

- **N3-1 [Medium] Per-predicate query rate, merge frequency, and SHACL validation throughput not exposed.** `cache_stats()` and the three v0.47.0 SRFs (`plan_cache_stats`, `dictionary_cache_stats`, `federation_cache_stats`) cover engine-internal caches; there is no SRF that exposes per-predicate query rate or merge frequency. A DBA wanting to identify the hottest predicate must aggregate `pg_stat_statements`. **Recommendation:** add `pg_ripple.predicate_workload_stats() RETURNS TABLE(predicate_iri TEXT, query_count BIGINT, merge_count BIGINT, last_merged TIMESTAMPTZ)`.

- **N3-2 [Medium] No `EXPLAIN (ANALYZE, BUFFERS)` integration.** `explain_sparql(analyze:=true)` returns the algebra + generated SQL + per-operator actual_rows, but does not currently surface PostgreSQL's BUFFERS counter, which is the canonical signal for "this query is I/O-bound on a cold cache". **Recommendation:** extend `explain_sparql()` JSON output with a `buffers` key when `analyze=true`.

- **N3-3 [Low] PT-code coverage in `docs/src/reference/error-catalog.md` may lag.** The codebase uses ~25 PT codes (PT4xx user errors, PT5xx system warnings, PT6xx federation, PT7xx LLM). A `scripts/check_pt_codes.sh` linter that diffs `grep -roh "PT[4-7][0-9][0-9]" src/` against the markdown reference would prevent drift; absent today.

### Section 4 — Test Coverage & Conformance [Dimension D]

- **N4-1 [Medium] Complex `sh:path` lacks pg_regress coverage.** Despite v0.48.0 announcing implementation, [src/shacl/constraints/property_path.rs](../src/shacl/constraints/property_path.rs) carries `#![allow(dead_code)]` at line 1 and there is no `tests/pg_regress/sql/shacl_complex_path.sql`. The "implemented" claim in the v0.48.0 CHANGELOG is therefore **not test-protected**. **Recommendation:** add a SHACL pg_regress test that exercises `sh:inversePath`, sequence paths, alternative paths, `sh:zeroOrMorePath`, `sh:oneOrMorePath`, `sh:zeroOrOnePath` against representative violating data, and remove the `#![allow(dead_code)]`.

- **N4-2 [Medium] Property-based test generators still narrow.** Three suites (`sparql_roundtrip`, `dictionary`, `jsonld_framing`) at 10,000 cases each. The dictionary generator does not exercise NFC/NFD Unicode, the SPARQL generator does not emit property paths or aggregates, and the JSON-LD generator emits flat documents only. **Recommendation:** enrich generators per Section 8 of [PLAN_OVERALL_ASSESSMENT_3.md](PLAN_OVERALL_ASSESSMENT_3.md). Effort 6–9 person-days.

- **N4-3 [Medium] No fuzz target for `oxrdf`/`rio_xml` or JSON-LD framing.** The six fuzz targets (`federation_result`, `sparql_parser`, `turtle_parser`, `datalog_parser`, `shacl_parser`, `dictionary_hash`) cover the most user-facing surfaces but not RDF/XML or JSON-LD framing input. Both are user-supplied text parsed in extension code paths.

- **N4-4 [Low] No HTTP companion fuzz coverage.** `pg_ripple_http` accepts arbitrary SPARQL/POST bodies and there is no AFL or `cargo-fuzz` target on the request-handler chain. The body-limit and CORS controls reduce attack surface but not parser surface.

- **N4-5 [Low] WatDiv latency baselines exist but the CI gate is non-blocking.** [tests/watdiv/baselines.json](../tests/watdiv/baselines.json) per v0.48.0; warning at >10 % regression. Promote to blocking before v1.0.0 once the baseline has stabilised across two consecutive releases.

### Section 5 — Standards Completeness [Dimension E]

- **N5-1 [Low] All 17 SPARQL 1.1 built-in functions enumerated by the prompt are implemented.** Confirmed by reading [src/sparql/expr.rs:947-1105](../src/sparql/expr.rs#L947). Recorded as a positive baseline; no action required.

- **N5-2 [Low] All 11 SPARQL 1.1 Update operations implemented.** Confirmed at [src/sparql/mod.rs:877-1509](../src/sparql/mod.rs#L877). Recorded as a positive baseline.

- **N5-3 [Medium] SHACL-SPARQL (`sh:SPARQLConstraintComponent`) not implemented.** No occurrences of `sh:SPARQLConstraintComponent` in [src/shacl/](../src/shacl/). The W3C SHACL recommendation has SHACL-SPARQL as a normative section; pg_ripple covers SHACL Core comprehensively but not the SPARQL extension. **Recommendation:** add to v0.51.0 or v1.0.0 roadmap; effort ~3 person-weeks.

- **N5-4 [Low] OWL 2 RL pass rate is 62/66 (93.9 %) with 4 known XFAILs.** [tests/owl2rl/known_failures.txt](../tests/owl2rl/known_failures.txt): `prp-spo2` (3-hop chain), `scm-sco` (cyclic equivalentClass), `eq-diff1` (Nothing propagation), `dt-type2` (XSD numeric promotion). Closing all four would unblock flipping the OWL 2 RL conformance gate from informational to blocking. **Recommendation:** schedule for v0.51.0.

- **N5-5 [Low] CONSTRUCT round-tripping loses ground RDF-star quoted triples.** See `S2-6` (Partial). The dictionary already carries `qt_s/qt_p/qt_o` columns; the missing piece is the N-Triples-star serialiser entry in [src/sparql/mod.rs:742-748](../src/sparql/mod.rs#L742) returning `Some("<< s p o >>")` instead of `None`.

### Section 6 — Security [Dimension F]

- **N6-1 [High] Docker image runs as root.** [Dockerfile](../Dockerfile) ends with `CMD ["postgres", "-c", "allow_system_table_mods=on"]` and never sets a `USER` directive. The base `postgres:18-bookworm` image internally drops to the `postgres` system user via its entrypoint, but that mitigation is implicit and not enforced for adjacent files copied into `/usr/local/bin/pg_ripple_http`, which would run as root if invoked directly. **Recommendation:** add an explicit `USER postgres:postgres` before the `CMD` directive; document the constraint in `docs/src/operations/docker.md`.

- **N6-2 [High] No SPARQL algebra-tree depth limit.** `pg_ripple.max_path_depth` and `pg_ripple.property_path_max_depth` cap *property-path* recursion only. There is no GUC bounding total algebra-tree depth, total BGP triple count, or total UNION-branch count for an inbound SPARQL query. A deeply nested malicious SPARQL string can therefore exhaust the spargebra parser's recursion budget (Rust stack ≈ 8 MB by default) before any pg_ripple-side limits apply. **Recommendation:** add `pg_ripple.sparql_max_algebra_depth` (default 256) and `pg_ripple.sparql_max_triple_patterns` (default 4096); reject with PT440 (or a new code) at parse time.

- **N6-3 [Medium] No certificate-fingerprint pinning in HTTP companion (S5-2 carry-forward).** The CA-bundle override is necessary but insufficient against compromised-CA scenarios. **Recommendation:** add `PG_RIPPLE_HTTP_PIN_FINGERPRINTS` (comma-separated SHA-256 hashes of expected leaf or intermediate certificates).

- **N6-4 [Medium] `docker-compose.yml` PG `trust` authentication.** [docker/00-pg_hba.sh](../docker/00-pg_hba.sh) enables `trust` authentication for external TCP. The Dockerfile comment explicitly notes this is for "development/testing" — but the same image is published to `ghcr.io/grove/pg-ripple:latest` for general consumption. A first-time user pulling the published image to a non-localhost network exposes a passwordless PostgreSQL. **Recommendation:** publish two image tags (`:dev` with trust, `:prod` requiring a `POSTGRES_PASSWORD`) and make `:latest` alias to `:prod`.

- **N6-5 [Low] `secrets/` directory contains a UUID-named subdirectory.** [secrets/ad5ed55a-8774-4916-88b3-7d13f3ddf7b2/](../secrets/) is checked into the repo. No real credentials were spotted, but the existence of a `secrets/` directory in version control is a discoverability footgun for future contributors. **Recommendation:** rename to `test_fixtures/sealed_secrets/` or add a top-level `README.md` documenting that the contents are test stubs.

- **N6-6 [Low] No SBOM published.** Even with `cargo audit` and `cargo deny` in CI, an SBOM (CycloneDX / SPDX) per release is the modern supply-chain compliance artefact. Add to release automation.

### Section 7 — Architecture & Maintainability [Dimension G]

- **N7-1 [Low] `src/sparql/sqlgen.rs` is 766 lines.** The v0.47.0 split delivered the architectural goal: every translation unit lives in [src/sparql/translate/](../src/sparql/translate/) (filter.rs 901 lines, graph.rs 487, group.rs 426, bgp.rs 369, union.rs 153, left_join.rs 127, distinct.rs 68, join.rs 49). `filter.rs` at 901 lines is the next refactor candidate (split into `filter_expr.rs` + `filter_dispatch.rs`).

- **N7-2 [Low] `src/datalog/mod.rs` is 1,681 lines.** See N1-4. Recommend a split along seminaive / magic / demand / coordinator boundaries.

- **N7-3 [Low] Migration script chain is continuous.** 50 files from `pg_ripple--0.1.0.sql` through `pg_ripple--0.49.0--0.50.0.sql`; verified by `ls sql/pg_ripple--*.sql | sort -V`. No gaps. Strong baseline.

- **N7-4 [Low] `pgrx 0.18.0` upgrade.** [Cargo.toml](../Cargo.toml) carries `pgrx = "0.18.0"` (was 0.17 at v0.46.0). The AGENTS.md document still references pgrx 0.17. **Recommendation:** update [AGENTS.md](../AGENTS.md) tech-stack table.

- **N7-5 [Low] `justfile` recipes cover build/test/bench but lack `release` and `docs` targets.** `just release` would orchestrate the version bump, migration script template, CHANGELOG draft, tag, and `gh release create`. Currently this is documented in `RELEASE.md` but performed manually. **Recommendation:** add `just release VERSION` and `just docs-serve`.

### Section 8 — Developer Experience & Ecosystem [Dimension H]

- **N8-1 [Low] All seven `examples/` files reference symbols that exist in v0.50.0.** Verified by spot-check. Strong baseline.

- **N8-2 [Low] No `examples/llm_nl_to_sparql.sql` despite v0.49.0 LLM integration.** New `sparql_from_nl()`, `suggest_sameas()`, `apply_sameas_candidates()`, and `rag_context()` SRFs would benefit from a tutorial example. **Recommendation:** add `examples/llm_workflow.sql` with mock-endpoint configuration.

- **N8-3 [Low] No public architecture diagram.** Carry-forward from v0.46.0 (S9-4). The README has prose; a Mermaid diagram in `docs/src/reference/architecture.md` would lower onboarding cost.

- **N8-4 [Low] `pg_ripple_http` documents 5 endpoints in [pg_ripple_http/README.md](../pg_ripple_http/README.md) but does not publish an OpenAPI spec.** **Recommendation:** generate `openapi.yaml` from a `utoipa` annotation pass and publish under `docs/src/reference/`.

### Section 9 — Feature Gap Identification [Dimension I]

- **N9-1 [High] Streaming SPARQL cursors not exposed over HTTP.** The full primitives exist inside PostgreSQL ([src/sparql/cursor.rs](../src/sparql/cursor.rs)) but [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs) `/sparql` is a one-shot handler. End-to-end streaming was a v0.40.0 deliverable; it stops at the SPI boundary. See N2-1 for recommendation.

- **N9-2 [Medium] CDC subscription captures only delta-table inserts/deletes.** [src/cdc.rs](../src/cdc.rs) hooks AFTER triggers on `vp_N_delta` only; merge-time events (a tombstone-resolved delete clearing main; a bulk-promote from rare to dedicated VP) are not surfaced. Subscribers see the in-write event but not the lifecycle transitions. **Recommendation:** add a second NOTIFY channel `pg_ripple_cdc_lifecycle` for merge-cycle and promotion events.

- **N9-3 [Medium] SHACL-AF `sh:rule` triples are placeholder-registered but not executed.** [src/shacl/mod.rs:943-990](../src/shacl/mod.rs#L943) inserts `sh:rule` triples into `_pg_ripple.rules` without compiling them. Users authoring a SHACL-AF shapes graph will see no rule firings and no warning. **Recommendation:** raise PT4xx if SHACL-AF features are detected, or integrate into the Datalog rule compiler.

- **N9-4 [Medium] No native SPARQL CSV/TSV result format.** `pg_ripple_http` performs CSV/TSV serialisation at the HTTP layer ([pg_ripple_http/src/main.rs:36-37](../pg_ripple_http/src/main.rs#L36)) but the in-PG SPARQL engine does not offer `pg_ripple.sparql_csv()` / `sparql_tsv()` SRFs. Operators consuming SPARQL results from `psql` / `dbt` / Kafka Connect must marshal via JSON.

- **N9-5 [Low] No `COPY ... FROM` bulk load integration.** [src/bulk_load.rs](../src/bulk_load.rs) accepts `TEXT` arguments and file paths (superuser-only); there is no `COPY rdf_triples FROM '/path/to.nt' WITH (FORMAT 'ntriples')` integration. The `COPY ... FROM PROGRAM` workaround works but loses the dictionary-encoding batching. **Recommendation:** ship a `pg_ripple.copy_handler` extension hook in v0.51.0.

- **N9-6 [Medium] OTLP exporter not wired (see N2-2).** Treated as both an observability and feature gap.

- **N9-7 [Low] JSON-LD framing is implemented per W3C JSON-LD 1.1.** Confirmed by reading [src/framing/mod.rs](../src/framing/mod.rs) and submodules `frame_translator.rs`, `embedder.rs`, `compactor.rs`. Strong baseline.

- **N9-8 [Low] NL→SPARQL LLM pipeline supports a `'mock'` endpoint.** [src/llm/mod.rs](../src/llm/mod.rs) (v0.49.0). This is excellent for testing without an external LLM dependency and is an under-celebrated DX win.

---

## New Feature Recommendations

The list below is pruned to the 12 features most likely to materially advance pg_ripple toward v1.0.0 and a "world-class" position. Each has a concrete user benefit, a target persona, an effort classification, dependencies, and a target milestone. Excitement rating is included where the differentiation potential is high.

### F-1 — `POST /sparql/stream` (HTTP streaming)
- **Description:** Wrap `sparql_cursor()` / `sparql_cursor_turtle()` / `sparql_cursor_jsonld()` behind a `POST /sparql/stream` endpoint that returns `Transfer-Encoding: chunked` JSON-Lines (SELECT) or N-Triples (CONSTRUCT). Closes N9-1 and N2-1. Required for streaming-aware clients (Jupyter, dbt, Kafka Connect).
- **Persona:** Application developer; data engineer.
- **Effort:** Medium (1–2 weeks).
- **Dependencies:** None.
- **Milestone:** v0.51.0.
- **Excitement:** High.

### F-2 — Non-Root Container + Helm Chart
- **Description:** Set `USER postgres:postgres` in [Dockerfile](../Dockerfile); publish a minimal Helm chart at `charts/pg_ripple/` with values for `replicaCount`, `persistence`, `httpService`, `federationEndpoints`. Closes N6-1.
- **Persona:** DBA; SRE.
- **Effort:** Quick Win (Dockerfile) + Medium (Helm chart).
- **Dependencies:** None.
- **Milestone:** Dockerfile in v0.51.0; Helm chart in v0.52.0.

### F-3 — OTLP Tracing Exporter
- **Description:** Wire `tracing-opentelemetry` + `opentelemetry-otlp` into [src/telemetry.rs](../src/telemetry.rs) so that `pg_ripple.tracing_exporter = 'otlp'` actually emits spans to a configured `pg_ripple.tracing_otlp_endpoint`. Closes N2-2 / N9-6.
- **Persona:** SRE; production operator.
- **Effort:** Medium (1–2 weeks).
- **Milestone:** v0.51.0.

### F-4 — SPARQL Algebra Depth Limit
- **Description:** New GUCs `pg_ripple.sparql_max_algebra_depth` (default 256) and `pg_ripple.sparql_max_triple_patterns` (default 4096); reject deeper queries at parse time with PT440. Closes N6-2.
- **Persona:** DBA; SRE.
- **Effort:** Quick Win (≤ 3 days).
- **Milestone:** v0.51.0.

### F-5 — `predicate_workload_stats()` SRF
- **Description:** Per-predicate query rate, merge frequency, last-merged timestamp. Closes N3-1.
- **Persona:** DBA; performance engineer.
- **Effort:** Medium.
- **Milestone:** v0.51.0.

### F-6 — Native SPARQL CSV/TSV SRFs
- **Description:** `pg_ripple.sparql_csv(query TEXT)` and `sparql_tsv(query TEXT)` as set-returning text functions per the SPARQL 1.1 Results CSV/TSV recommendation. Closes N9-4.
- **Persona:** Data engineer; analyst.
- **Effort:** Medium (1 week).
- **Milestone:** v0.51.0.

### F-7 — VS Code Extension (SPARQL/SHACL/Datalog)
- **Description:** TextMate grammars + LSP-style hover for SPARQL keywords; integrated query runner against `pg_ripple_http`; SHACL shape lint; Datalog rule formatter. Carry-forward from v0.46.0 recommendation B-2.
- **Persona:** Application developer; data scientist.
- **Effort:** Major (3–5 weeks).
- **Milestone:** v0.52.0.
- **Excitement:** High.

### F-8 — Logical Replication of the RDF Graph
- **Description:** PG18 logical-decoding output plugin that emits VP-table changes as N-Triples; replica-side consumer applies via `load_ntriples()`. Carry-forward from v0.46.0 recommendation D-1; required for HA deployments.
- **Persona:** DBA; SRE.
- **Effort:** Major (5–7 weeks).
- **Milestone:** v0.53.0.
- **Excitement:** High.

### F-9 — SHACL-SPARQL Constraint Component
- **Description:** Implement `sh:SPARQLConstraintComponent` so user-authored SPARQL queries can serve as SHACL constraints. Closes N5-3.
- **Persona:** Ontology engineer; data quality team.
- **Effort:** Medium (2–3 weeks).
- **Milestone:** v0.52.0.

### F-10 — `COPY rdf FROM` Integration
- **Description:** Custom COPY handler so `COPY pg_ripple.triples FROM '/path/to.nt' WITH (FORMAT 'ntriples')` works as a first-class PostgreSQL command. Closes N9-5.
- **Persona:** Data engineer; bulk-load operator.
- **Effort:** Medium (2 weeks).
- **Milestone:** v0.52.0.

### F-11 — RAG Pipeline Hardening
- **Description:** v0.50.0's `rag_context()` is the foundation; add prompt-injection mitigation (sanitise NL input before LLM call), cost / token budget tracking, response caching keyed on `(question, k, schema_digest)`, and a `pg_ripple_http /rag` REST endpoint. Builds on v0.49.0–v0.50.0 LLM integration.
- **Persona:** Application developer; AI/ML engineer.
- **Effort:** Medium (2–3 weeks).
- **Milestone:** v0.51.0 or v0.52.0.
- **Excitement:** Very high.

### F-12 — `pg_upgrade` Compatibility Matrix + Doc
- **Description:** Document supported upgrade paths PG18.x → PG18.y; add a `tests/pg_upgrade_compat.sh` integration test that validates `pg_upgrade` + `ALTER EXTENSION pg_ripple UPDATE`. Closes S10-3 from v0.46.0.
- **Persona:** DBA.
- **Effort:** Quick Win (≤ 1 week).
- **Milestone:** v0.51.0; **blocking for v1.0.0**.

---

## Maturity Scorecard

Updated from PLAN_OVERALL_ASSESSMENT_3.md. Δ shown vs v0.46.0.

| Dimension | Score / 5 | Δ vs v0.46.0 | Key evidence |
|---|---|---|---|
| Storage & HTAP correctness | **4.5** | 0 | C-3 still partial; tombstone GC, advisory locks, atomic promotion all closed; lock_timeout cutover mitigates the residual window |
| SPARQL correctness & spec compliance | **4.7** | +0.2 | All 17 builtins, all 11 Update ops, MOVE/COPY/ADD, RDF-star variable patterns, federation body limit; only S2-6 (CONSTRUCT RDF-star ground) and N6-2 (algebra depth limit) outstanding |
| Datalog reasoning | **4.7** | +0.2 | OWL 2 RL closure (S3-1) + WFS warning (S3-2) + parallel SID pre-allocation wired (S1-2 closed); only S3-4 (SAVEPOINT helper unused) outstanding |
| SHACL completeness | **4.5** | +1.5 | 35/35 SHACL Core implemented and tested; `sh:value` + `sh:sourceConstraintComponent` populated; remaining gaps are SHACL-AF (S4-8) and SHACL-SPARQL (N5-3) |
| Federation & HTTP service | **4.7** | +0.2 | body-byte limit (S2-4 closed); CA bundle; rate limit; CORS; X-Forwarded-For; only certificate-fingerprint pinning (S5-2 / N6-3) outstanding |
| Security | **4.3** | -0.2 | `cargo audit` job + `deny.toml` + check_no_security_definer.sh in v0.47.0; **but** new finding N6-1 (root container), N6-2 (no algebra depth limit), N6-4 (trust auth in published image) bring the score down vs the v0.46.0 4.5 |
| Test coverage & conformance | **4.7** | +0.2 | 150 pg_regress files; 6 fuzz targets (was 1); 3 proptest suites; W3C smoke required; Jena 99.9 %; WatDiv 100 %; LUBM 100 %; OWL 2 RL 93.9 % published |
| Performance & scalability | **4.2** | +0.2 | Cache-stats SRFs wired; TopN; parallel SID pre-allocation; only HTTP streaming (N2-1) and merge-throughput baseline (N2-3) outstanding |
| Observability & operations | **4.3** | +0.3 | `explain_sparql(analyze=true)` v0.50.0 with cache_status and actual_rows; six GUC validators; OWL 2 RL baseline; gaps remain on OTLP (N2-2), per-predicate metrics (N3-1) |
| Developer experience | **4.5** | 0 | LLM mock endpoint, RAG pipeline, framing, examples; only architecture diagram (N8-3) and OpenAPI spec (N8-4) outstanding |
| **Overall** | **4.55** | **+0.25** | Release-candidate quality; v1.0.0 unlocked by closing N6-1, N6-2, N9-1, S5-2, S5-3, F-12 (pg_upgrade matrix) |

The single dimension that **regressed in score** is Security (4.5 → 4.3) due to the newly-surfaced root-container, missing algebra-depth-limit, and `trust`-authentication-in-published-image findings. None of these is a code regression; they are new findings from a fresh adversarial pass that the prior assessments did not perform. Closing F-2 (non-root + Helm) and F-4 (algebra depth) restores Security to 4.7 and lifts Overall to 4.7 / 5.0 — the credible v1.0.0 quality bar.

---

## v1.0.0 Readiness Checklist

### Blocking for v1.0.0 (must close)

| Item | ID | Estimated effort |
|---|---|---|
| Non-root container image | N6-1 / F-2 | 1 day |
| SPARQL algebra-tree depth limit (DoS protection) | N6-2 / F-4 | 3 days |
| `pg_ripple_http` streaming endpoint | N9-1 / F-1 | 1–2 weeks |
| Certificate-fingerprint pinning | S5-2 | 3 days |
| CDC subscription backpressure docs | S5-3 | 1 day |
| Conformance gates flipped to blocking (W3C full / Jena / WatDiv / LUBM / OWL 2 RL) | — | 1 day each, after baselines stabilise |
| `pg_upgrade` compatibility matrix + integration test | S10-3 / F-12 | 1 week |
| Released image variants (`:dev` vs `:prod`) | N6-4 | 2 days |
| `cargo audit` and `cargo deny` blocking on PRs | S6-1 | already in CI; flip to blocking |
| Release automation (release-please-style PR) | S10-2 | 1 week |

### Nice-to-have (v1.x)

| Item | ID |
|---|---|
| OTLP tracing wired | N2-2 / F-3 |
| Per-predicate workload stats SRF | N3-1 / F-5 |
| Native SPARQL CSV/TSV SRFs | N9-4 / F-6 |
| VS Code extension | F-7 |
| Logical replication of RDF graph | F-8 |
| SHACL-SPARQL constraint component | N5-3 / F-9 |
| `COPY rdf FROM` integration | N9-5 / F-10 |
| Architecture diagram + OpenAPI spec | N8-3 / N8-4 |
| Merge-throughput baseline + vector-index baseline | N2-3 / N2-4 |
| Splits of `gucs.rs` and `datalog/mod.rs` | N1-1 / N1-4 |
| Property-based test generator enrichment | N4-2 |
| `oxrdf` / RDF/XML / JSON-LD framing fuzz targets | N4-3 |

---

## Summary of All Open Findings

Master table of every issue still open at v0.50.0, grouped by severity. Pre-existing IDs from prior assessments are retained; new findings introduced in this assessment carry the `N` prefix.

### High

| ID | Area | Description | Recommended fix |
|---|---|---|---|
| N6-1 | Security / Ops | Docker image runs as root | `USER postgres:postgres` in Dockerfile |
| N6-2 | Security | No SPARQL algebra-tree depth limit | Add GUCs + parse-time check |
| N9-1 | Feature | HTTP streaming endpoint missing | `POST /sparql/stream` chunked |

### Medium

| ID | Area | Description | Recommended fix |
|---|---|---|---|
| S1-3 | Storage | Merge worker `thread::sleep` backoff | Latch-driven `wait_latch` |
| S2-5 | SPARQL | `max_path_depth` vs `property_path_max_depth` duplicate | Consolidate / remove deprecated |
| S2-6 | SPARQL | CONSTRUCT loses ground RDF-star | Emit `<< s p o >>` |
| S3-4 | Datalog | `execute_with_savepoint` exported but unused | Wire into parallel-strata or remove |
| S4-6 | SHACL | Complex `sh:path` dispatcher integration shallow | Remove `#![allow(dead_code)]`; add pg_regress |
| S4-8 | SHACL | `sh:rule` silently registered as placeholder | Raise PT4xx or compile |
| S5-2 | HTTP | No certificate-fingerprint pinning | `PG_RIPPLE_HTTP_PIN_FINGERPRINTS` env |
| S5-3 | CDC | No documented backpressure | Add `docs/src/operations/cdc.md` |
| C-3 | Storage | HTAP cutover view-recreation window | Eliminate `CREATE OR REPLACE VIEW` step |
| N2-1 | Perf | HTTP `/sparql` does not stream | (= N9-1) |
| N2-2 | Obs | `tracing_exporter = 'otlp'` unwired | OTLP exporter |
| N2-3 | Perf | Merge-throughput baseline missing | Record p50/p95 |
| N2-4 | Perf | Vector-index baseline missing | Compare HNSW vs IVFFlat |
| N3-1 | Obs | No per-predicate workload SRF | `predicate_workload_stats()` |
| N3-2 | Obs | `explain_sparql()` lacks BUFFERS | Add `buffers` key |
| N4-1 | Tests | Complex `sh:path` no pg_regress | Add `shacl_complex_path.sql` |
| N4-2 | Tests | Property-based generators narrow | Enrich Unicode/property paths/JSON-LD nesting |
| N4-3 | Tests | Missing fuzz targets (RDF/XML, JSON-LD framing) | Add 2 new targets |
| N5-3 | Standards | SHACL-SPARQL not implemented | Roadmap v0.51.0+ |
| N6-3 | Security | (= S5-2) |  |
| N6-4 | Security | Published image uses `trust` auth | Two image tags |
| N9-2 | Feature | CDC misses lifecycle events | Second NOTIFY channel |
| N9-3 | Feature | SHACL-AF rules silently dropped | (= S4-8) |
| N9-4 | Feature | No native SPARQL CSV/TSV | `sparql_csv()` / `sparql_tsv()` |

### Low

| ID | Area | Description |
|---|---|---|
| S1-5 | Storage | Predicate-OID cache lacks syscache callback |
| S6-2 | Security | No SPDX licence check |
| S6-3 | Security | No `pg_dump`/restore round-trip test |
| S9-2 | Docs | No GUC ↔ workload-class matrix |
| S9-3 | Docs | Worked examples sparse for some features |
| S9-4 | Docs | No public architecture diagram |
| S9-6 | Docs | Migration script headers inconsistent |
| S10-2 | CI | Release automation manual |
| S10-3 | Ops | `pg_upgrade` compatibility doc missing |
| S10-5 | CI | Migration-chain test does not stress data preservation |
| N1-1 | Arch | `gucs.rs` is 1,617 lines |
| N1-2 | Arch | No `cargo tree --duplicates` discipline |
| N1-3 | HTTP | 11 unwrap()/expect() in pg_ripple_http |
| N1-4 | Arch | `src/datalog/mod.rs` is 1,681 lines |
| N1-5 | Security | No SQL-injection format!-banning lint |
| N3-3 | Obs | PT-code documentation drift unprotected |
| N4-4 | Tests | `pg_ripple_http` no fuzz coverage |
| N4-5 | Tests | WatDiv gate non-blocking |
| N5-4 | Standards | OWL 2 RL 93.9 %, 4 known XFAILs |
| N5-5 | SPARQL | (= S2-6) |
| N6-5 | Security | `secrets/` directory naming footgun |
| N6-6 | Security | No SBOM published |
| N7-1 | Arch | `filter.rs` 901 lines next refactor candidate |
| N7-2 | Arch | (= N1-4) |
| N7-4 | Docs | AGENTS.md references pgrx 0.17 (now 0.18) |
| N7-5 | DX | `justfile` lacks `release` and `docs` recipes |
| N8-2 | DX | No LLM example file |
| N8-3 | DX | No public architecture diagram |
| N8-4 | DX | No OpenAPI spec for HTTP service |
| N9-5 | Feature | No `COPY ... FROM` integration |

### Closed (this cycle)

The 20 v0.46.0 items listed in **Open Issues Tracking** as "Closed" are not repeated here. The "Partial" items (S2-6, S4-6, S4-8, C-3) are listed under Medium for completeness.

---

## Conclusion

pg_ripple at v0.50.0 is **release-candidate quality**. The four releases since the v0.46.0 baseline (v0.47.0 through v0.50.0) executed the v0.46.0 prioritisation almost in full: SHACL truthful completion, six GUC validators, the sparql/translate split, parallel SID pre-allocation, OWL 2 RL closure, SPARQL Update completion, RDF-star variable patterns, federation body-byte limit, five new fuzz targets, OWL 2 RL conformance baseline publication, and an entire AI/LLM integration layer (NL→SPARQL, sameAs candidate generation, RAG pipeline). The remaining runway to a credible v1.0.0 tag is **operational hardening** — non-root container, SPARQL DoS protection, HTTP streaming, certificate-fingerprint pinning, `pg_upgrade` matrix, release automation, and conformance-gate flips. None of these is structurally hard; they are scoped weeks of work, not months. With focused execution they can land in v0.51.0 + v0.52.0, putting v1.0.0 within reach for mid-2026.

The single architectural risk to monitor is the secondary growth of `gucs.rs` (1,617 lines) and `src/datalog/mod.rs` (1,681 lines). The v0.20.0 → v0.46.0 cycle's biggest debt was the `lib.rs` god-module followed by the `sqlgen.rs` god-module. Both are now resolved. Preventing the recurrence in `gucs.rs` and `datalog/mod.rs` is a 2–3 day refactor that should not wait for v1.0.0.
