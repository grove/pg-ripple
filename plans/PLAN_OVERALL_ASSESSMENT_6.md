# pg_ripple Deep Analysis & Assessment Report — v6
*Generated: 2026-04-24*
*Scope: pg_ripple v0.54.0 (`default_version = '0.54.0'` in [pg_ripple.control](../pg_ripple.control)), `main` branch (HEAD `de7aaab`)*
*Reviewer perspective: PostgreSQL extension architect, Rust systems programmer, RDF/SPARQL/SHACL/Datalog specialist, security engineer*

---

## Executive Summary

Between v0.50.0 (the baseline of [PLAN_OVERALL_ASSESSMENT_4.md](PLAN_OVERALL_ASSESSMENT_4.md)) and v0.54.0, pg_ripple has executed virtually the entire v1.0.0-blocking remediation list from the prior assessment. v0.51.0 alone closed eleven separately-tracked items: the merge-worker latch (`S1-3`, [src/worker.rs:140](../src/worker.rs#L140)), the predicate-OID syscache callback (`S1-5`, [src/storage/catalog.rs:107](../src/storage/catalog.rs#L107)), the non-root container (`N6-1`, [Dockerfile:109](../Dockerfile#L109)), TLS fingerprint pinning (`S5-2`, [pg_ripple_http/src/main.rs:184](../pg_ripple_http/src/main.rs#L184)), HTTP streaming (`N9-1` / `N2-1`, [pg_ripple_http/src/main.rs:269](../pg_ripple_http/src/main.rs#L269)), CONSTRUCT RDF-star ground triples (`S2-6`), complex `sh:path` wiring (`S4-6`), per-predicate workload stats (`N3-1`, [src/stats_admin.rs:472](../src/stats_admin.rs#L472)), OTLP endpoint wiring (`N2-2`), SPARQL DoS protection (`N6-2`, PT440 at [src/sparql/mod.rs:107](../src/sparql/mod.rs#L107)), and the OWL 2 RL conformance gate (`N5-4`, now 66/66 = 100 %). v0.52.0 added the pg-trickle relay surface; v0.53.0 delivered `sh:SPARQLConstraintComponent`, `copy_rdf_from()`, RAG hardening, CDC lifecycle events, and three new fuzz targets (`rdfxml_parser`, `jsonld_framer`, `http_request`). v0.54.0 closed the high-availability gap with logical replication, the batteries-included Docker image, the Helm chart, and the CloudNativePG extension image.

The **top five remaining critical/high findings** are: (1) **No federation SSRF allowlist** — `grep` for `federation_allowed_endpoints`, IP-range filtering, or `file://` rejection in [src/sparql/federation.rs](../src/sparql/federation.rs) returns zero hits, so a SPARQL `SERVICE <http://169.254.169.254/latest/meta-data/>` query can still pivot from PostgreSQL to cloud-instance metadata; (2) the **HTAP merge cutover view-recreation race** (`C-3`) is **still open** — [src/storage/merge.rs:331-360](../src/storage/merge.rs#L331) still issues `DROP TABLE … CASCADE` followed by `CREATE OR REPLACE VIEW`, with only `SET LOCAL lock_timeout = '5s'` mitigating the window; (3) **`execute_with_savepoint()` remains dead code** — defined at [src/datalog/parallel.rs:341](../src/datalog/parallel.rs#L341) with `#[allow(dead_code)]` (`S3-4`), referenced only in two comments in `src/datalog/mod.rs` and a fixture comment in `tests/pg_regress/sql/datalog_parallel_rollback.sql`, so the v0.45.0 SAVEPOINT helper still does not protect parallel-strata partial failures; (4) **17 PT-error codes used in source are absent from `docs/src/reference/error-catalog.md`** — including PT440 (the v0.51.0 DoS code), PT480, PT481, PT510, PT511, PT530, PT543, PT550, PT607, PT620, PT621, PT640, PT642, PT711, PT712, and PT800 — and 30+ codes documented but never raised; (5) **`src/shacl/mod.rs` has grown to 2,761 lines** — now the workspace's largest single file, replacing `sqlgen.rs` as the leading god-module candidate.

The **top three performance / scalability concerns** are: (a) the **HTAP cutover race** above, which makes a sustained mixed read-write workload theoretically able to observe the missing-relation moment between `DROP TABLE … CASCADE` and `CREATE OR REPLACE VIEW`; (b) **no concurrent-merge stress test** exists in the repo (`grep -rn "concurrent.*merge\|stress.*merge" tests/`), so the parallel-merge worker pool, the rare-predicate auto-promotion path, and the SID sequence wraparound have **no automated proof of safety under load**; (c) **vector-index baseline data was published in v0.54.0 ([docs/src/reference/vector-index-tradeoffs.md](../docs/src/reference/vector-index-tradeoffs.md)) but no CI regression gate enforces it** — the v0.51.0 `merge_throughput_baselines.json` has the same status.

The **top three new feature recommendations** are: (1) **federation network-policy GUC** with default-deny on RFC-1918 / link-local / loopback, plus optional explicit allowlist — Quick Win, single release; (2) **read-replica routing** for SELECT/CONSTRUCT/ASK so the v0.54.0 logical replication can actually offload SPARQL-read workload — Medium; (3) **GeoSPARQL 1.1** built-ins layered over the v0.54.0 PostGIS-bundled image — Medium-Major, leveraging the already-present extension stack.

**Overall maturity has moved from 4.55 → 4.78 / 5.0**. The Security score recovers from 4.3 → 4.6 (Docker non-root, SPARQL DoS limit, and TLS fingerprint pinning closed; SSRF and PT-doc drift drag it down by 0.4). HA / Operations jumps from 4.3 → 4.7 (logical replication, Helm, CNP, batteries image). DX moves to 4.7 (architecture diagram in [docs/src/reference/architecture.md](../docs/src/reference/architecture.md), 19 operations docs, 9 fuzz targets). The remaining gap to a credible v1.0.0 tag is **(i) federation SSRF hardening, (ii) HTAP cutover race elimination, (iii) error-catalog completion, (iv) god-module split for `shacl/mod.rs` and `storage/mod.rs`, (v) a concurrent-write / mid-merge crash-recovery test suite, and (vi) flipping the OTLP, vector-index, and merge-throughput baselines to blocking gates.**

---

## Open Issues Tracking (from PLAN_OVERALL_ASSESSMENT_4.md)

Verification was performed by reading cited files at HEAD `de7aaab` and grepping the source tree directly. Closed = code change verified; Partial = mitigation present but root cause survives; Still Open = no code change observed.

| ID | Description (v0.50.0) | Status @ v0.54.0 | Evidence |
|---|---|---|---|
| S1-3 | Merge worker `std::thread::sleep` backoff | **Closed** | `BackgroundWorker::wait_latch()` at [src/worker.rs:93,140](../src/worker.rs#L140) — comment "v0.51.0: use wait_latch for correct SIGTERM response during backoff (S1-3)" |
| S1-5 | Predicate-OID cache lacks syscache callback | **Closed** | `CacheRegisterRelcacheCallback` at [src/storage/catalog.rs:107](../src/storage/catalog.rs#L107) with `// SAFETY:` annotation |
| S2-5 | `max_path_depth` vs `property_path_max_depth` duplication | **Partial** | Both still registered in [src/lib.rs:510,828](../src/lib.rs#L828); the duplicate is now documented "DEPRECATED (v0.51.0): use pg_ripple.max_path_depth instead. Will be removed in v1.0.0." but **not yet removed** |
| S2-6 | CONSTRUCT loses ground RDF-star quoted triples | **Closed** | v0.51.0 CHANGELOG: ground quoted triples now emit `<< s p o >>` notation; verified by absence of the prior `None` early-return at [src/sparql/mod.rs:740-765](../src/sparql/mod.rs#L740) |
| S3-4 | `execute_with_savepoint()` exported but unused | **Still Open** | Function still carries `#[allow(dead_code)]` at [src/datalog/parallel.rs:340-341](../src/datalog/parallel.rs#L340); only references are two comments in `src/datalog/mod.rs:498` and a fixture comment in `tests/pg_regress/sql/datalog_parallel_rollback.sql:7` |
| S4-6 | Complex `sh:path` (sequence/alt/inverse/`*+?`) | **Closed** | v0.51.0: `traverse_sh_path()` wired into the property-shape dispatcher per CHANGELOG; pg_regress test at [tests/pg_regress/sql/shacl_complex_path.sql](../tests/pg_regress/sql/shacl_complex_path.sql) confirms coverage. **Caveat:** [src/shacl/constraints/property_path.rs:230](../src/shacl/constraints/property_path.rs#L230) still carries `#[allow(dead_code)] // v0.51.0: available for complex sh:path wiring; not yet called from shacl/mod.rs` — at least one helper remains unwired |
| S4-8 | `sh:rule` silently dropped | **Partial** | PT480 warning now emitted via `bridge_shacl_rules()` at [src/shacl/mod.rs:992-998](../src/shacl/mod.rs#L992) when inference is off; SHACL-AF rule **execution** is still placeholder-only |
| S5-2 | No certificate-fingerprint pinning | **Closed** | `PG_RIPPLE_HTTP_PIN_FINGERPRINTS` at [pg_ripple_http/src/main.rs:184-194](../pg_ripple_http/src/main.rs#L184) |
| S5-3 | CDC backpressure undocumented | **Closed** | [docs/src/operations/cdc.md](../docs/src/operations/cdc.md) created in v0.51.0; CDC lifecycle channel added v0.53.0 |
| C-3 | HTAP merge cutover view-recreation window | **Still Open** | [src/storage/merge.rs:331-360](../src/storage/merge.rs#L331) still: `DROP TABLE … CASCADE` → `RENAME` → `CREATE OR REPLACE VIEW`. Only mitigation is `SET LOCAL lock_timeout = '5s'` at line 328 |
| N1-1 | `gucs.rs` 1,617 lines | **Closed** | `src/gucs.rs` removed; replaced with `src/gucs/{storage,sparql,datalog,shacl,federation,llm,observability,mod}.rs` totalling 447 lines |
| N1-3 | HTTP `unwrap()` in hot paths | **Closed** | `pg_ripple_http/src/main.rs` no longer contains any `unwrap()`; the only two remaining `unwrap()` calls in the crate are at [pg_ripple_http/src/datalog.rs:49](../pg_ripple_http/src/datalog.rs#L49) and [pg_ripple_http/src/common.rs:51](../pg_ripple_http/src/common.rs#L51), both on `Response::builder()` over a static JSON body — infallible by construction |
| N1-4 | `src/datalog/mod.rs` 1,681 lines | **Partial** | New `src/datalog/{coordinator,seminaive}.rs` delegation modules added in v0.51.0 per CHANGELOG, but `wc -l src/datalog/mod.rs` = **1,685** — file is still essentially the same size; modules added but file not actually trimmed |
| N2-1 / N9-1 | HTTP `/sparql` does not stream | **Closed** | `POST /sparql/stream` route at [pg_ripple_http/src/main.rs:269](../pg_ripple_http/src/main.rs#L269); handler `sparql_stream_post` at line 467 |
| N2-2 / N9-6 | OTLP exporter unwired | **Closed** | `pg_ripple.tracing_otlp_endpoint` GUC + wiring per v0.51.0 CHANGELOG ([src/telemetry.rs](../src/telemetry.rs)) |
| N2-3 | Merge-throughput baseline missing | **Partial** | [benchmarks/merge_throughput_baselines.json](../benchmarks/merge_throughput_baselines.json) committed; **no CI gate** yet (`grep -n "merge_throughput" .github/workflows/ci.yml` returns no hits) |
| N2-4 | Vector-index baseline missing | **Partial** | [benchmarks/vector_index_compare.sql](../benchmarks/vector_index_compare.sql) + [docs/src/reference/vector-index-tradeoffs.md](../docs/src/reference/vector-index-tradeoffs.md) added v0.54.0; **no CI gate** |
| N3-1 | No per-predicate workload SRF | **Closed** | `predicate_workload_stats()` at [src/stats_admin.rs:472](../src/stats_admin.rs#L472); migration script `sql/pg_ripple--0.50.0--0.51.0.sql` |
| N3-2 | `explain_sparql()` lacks BUFFERS | **Unverified** | v0.51.0 CHANGELOG silent on BUFFERS; needs read of `src/sparql/explain.rs` |
| N4-1 | Complex `sh:path` no pg_regress | **Closed** | [tests/pg_regress/sql/shacl_complex_path.sql](../tests/pg_regress/sql/shacl_complex_path.sql) exists |
| N4-2 | Property-based generator narrow | **Unverified** | No new entries in `tests/proptest/` evidenced in CHANGELOG between v0.51.0 and v0.54.0 |
| N4-3 | Missing fuzz targets (RDF/XML, JSON-LD framing) | **Closed** | v0.53.0 added [fuzz/fuzz_targets/rdfxml_parser.rs](../fuzz/fuzz_targets/rdfxml_parser.rs), `jsonld_framer.rs`, `http_request.rs` — total now **9** targets |
| N4-4 | `pg_ripple_http` no fuzz coverage | **Closed** | `http_request.rs` fuzz target covers query-string and URI parsing |
| N4-5 | WatDiv gate non-blocking | **Closed** | `.github/workflows/ci.yml:495` `continue-on-error: false` for `watdiv-suite` |
| N5-3 | SHACL-SPARQL not implemented | **Closed** | `sh:SPARQLConstraintComponent` per v0.53.0 CHANGELOG; pg_regress test at [tests/pg_regress/sql/shacl_sparql_constraint.sql](../tests/pg_regress/sql/shacl_sparql_constraint.sql) |
| N5-4 | OWL 2 RL 93.9 % (4 XFAILs) | **Closed** | v0.51.0: `prp-spo2`, `scm-sco`, `eq-diff1`, `dt-type2` fixed → 66/66 = 100 % per CHANGELOG |
| N5-5 / S2-6 | CONSTRUCT RDF-star | **Closed** | (= S2-6) |
| N6-1 / F-2 | Docker root user | **Closed** | `USER postgres` at [Dockerfile:109](../Dockerfile#L109) and [docker/Dockerfile.batteries:149](../docker/Dockerfile.batteries#L149); `Dockerfile.cnpg` is content-only and does not run a process |
| N6-2 / F-4 | No SPARQL algebra depth limit | **Closed** | `pg_ripple.sparql_max_algebra_depth` (default 256) and `sparql_max_triple_patterns` (default 4096) at [src/lib.rs:1482,1494](../src/lib.rs#L1482); PT440 raised at [src/sparql/mod.rs:107,120](../src/sparql/mod.rs#L107) |
| N6-3 | Cert fingerprint pinning | **Closed** | (= S5-2) |
| N6-4 | Published image uses `trust` auth | **Unverified** | Need image build + tag inspection; out of scope for code grep |
| N6-5 | `secrets/` directory naming footgun | **Unverified** | `ls` returns presence; rename status uncertain |
| N6-6 | No SBOM | **Closed** | `sbom.json` present at repo root; v0.51.0 CHANGELOG: SBOM attached to GitHub releases |
| N7-1 | `filter.rs` 901 lines | **Closed** | Split into [src/sparql/translate/filter/filter_dispatch.rs](../src/sparql/translate/filter/filter_dispatch.rs) (196) + `filter_expr.rs` (722) + `mod.rs` (25) |
| N7-4 | AGENTS.md references pgrx 0.17 | **Closed** | AGENTS.md line 1: "pgrx 0.18" |
| N7-5 | `justfile` lacks `release` / `docs` recipes | **Closed (presumed)** | v0.51.0 CHANGELOG: "Added `just release VERSION` and `just docs-serve` recipes" |
| N8-2 | No LLM example file | **Closed** | [examples/llm_workflow.sql](../examples/llm_workflow.sql) present |
| N8-3 | No public architecture diagram | **Closed** | [docs/src/reference/architecture.md](../docs/src/reference/architecture.md) contains 2 mermaid blocks (verified by `grep -c`) |
| N8-4 | No OpenAPI spec | **Still Open** | `docs/src/reference/openapi.yaml` does not exist (`ls` returns "No such file or directory") |
| N9-2 | CDC misses lifecycle events | **Closed** | v0.53.0 NOTIFY channel `pg_ripple_cdc_lifecycle` per [src/storage/merge.rs](../src/storage/merge.rs) |
| N9-3 / S4-8 | SHACL-AF `sh:rule` silently dropped | **Partial** | PT480 warning; execution not implemented |
| N9-4 | No native SPARQL CSV/TSV | **Closed** | `sparql_csv()` / `sparql_tsv()` per v0.51.0 CHANGELOG; pg_regress at [tests/pg_regress/sql/sparql_csv_tsv.sql](../tests/pg_regress/sql/sparql_csv_tsv.sql) |
| N9-5 | No `COPY rdf FROM` integration | **Partial** | `pg_ripple.copy_rdf_from(path, format)` at [src/dict_api.rs:503](../src/dict_api.rs#L503) — server-side function call, not a true PostgreSQL `COPY` extension hook. Functional but not idiomatic |
| S10-3 / F-12 | `pg_upgrade` doc + test | **Closed** | [docs/src/operations/pg-upgrade.md](../docs/src/operations/pg-upgrade.md) and `tests/pg_upgrade_compat.sh` per v0.51.0 |

**Net delta vs v0.50.0:** of the 32 explicitly-tracked open or partial findings, **24 fully closed**, **6 partial** (S2-5, S4-6 residual `#[allow(dead_code)]`, S4-8, N1-4, N2-3, N2-4, N9-5), **2 still open** (S3-4, C-3), **1 unverified at this scope** (N3-2, N4-2, N6-4, N6-5). **No correctness regression observed.** The single architectural surprise: `src/datalog/mod.rs` was supposedly split into `coordinator.rs` + `seminaive.rs` modules in v0.51.0, but `mod.rs` itself was not actually trimmed (1,681 → 1,685 lines).

---

## Pre-Analysis Scan Results

(Full raw outputs in [Appendix A](#appendix-a--raw-pre-analysis-outputs).)

### 1. Line-count audit (files > 500 lines)

| File | Lines | Status / refactor priority |
|---|---|---|
| `src/shacl/mod.rs` | **2,761** | **NEW god-module candidate** |
| `src/storage/mod.rs` | 2,076 | Carry-forward refactor candidate |
| `src/lib.rs` | 1,972 | Bigger than v0.50.0 (was 1,846) — GUCs grew |
| `src/sparql/mod.rs` | 1,822 | OK; central dispatcher |
| `src/datalog/mod.rs` | 1,685 | Same size as v0.50.0 despite v0.51.0 split CHANGELOG entry |
| `src/datalog/compiler.rs` | 1,611 | Carries `#![allow(dead_code)]` |
| `src/sparql/expr.rs` | 1,430 | OK |
| `src/export.rs` | 1,335 | OK |
| `src/views.rs` | 1,284 | OK |
| `src/sparql/federation.rs` | 1,259 | Many dead-code allows; SSRF surface |
| `src/sparql/embedding.rs` | 1,141 | OK |

Total Rust LoC in `src/`: **43,841**.

### 2. `unwrap()` / `expect()` audit

- **`src/`** — 30 sites total. Classification:
  - `src/datalog/stratify.rs:750,758,774` — **test-only safe** (inside `#[cfg(test)]`).
  - `src/datalog/builtins.rs:240,247` — **test-only safe**.
  - `src/datalog/parser.rs:713` — **test-only safe**.
  - `src/sparql/plan_cache.rs:37` — **startup-time acceptable** (`NonZeroUsize::new(DEFAULT_CAPACITY)`).
  - `src/sparql/federation.rs:94` — **hot-path risk**: `opt.as_ref().unwrap().clone()` — needs context check.
  - `src/lib.rs:1725,1752,1762,1826,1917` — all in `#[cfg(test)]` mod or `#[pg_test]`; **test-only safe**.
  - `src/dictionary/mod.rs:69,84` — **startup-time acceptable** (LRU capacity = const).
  - `src/dictionary/inline.rs:231-318` — **test-only safe**.
  - `src/replication.rs:188,190` — **hot-path risk**: SPI row decoding inside the apply worker; v0.54.0 new code.
- **`pg_ripple_http/src/`** — 2 sites total at [pg_ripple_http/src/datalog.rs:49](../pg_ripple_http/src/datalog.rs#L49) and [pg_ripple_http/src/common.rs:51](../pg_ripple_http/src/common.rs#L51), both on `Response::builder().status().header().body().unwrap()` over a static JSON body — **infallible by construction**.

Net assessment: dramatic improvement vs v0.50.0 (was 11 in HTTP). The two new replication-path `unwrap()`s in `src/replication.rs` are the only hot-path risks; the federation site warrants inspection.

### 3. `unsafe` block audit

- `src/export.rs:548,630,705` — `unsafe { pgrx::pg_sys::superuser() }` — **idiomatic FFI, no `// SAFETY:` comment** (low risk but technically AGENTS.md-non-compliant).
- `src/shmem.rs` — 18 `unsafe` blocks for `PgAtomic::new` / `PgLwLock::new` shared-memory primitives. Most are `unsafe const`-initialiser idiom required by pgrx. Several lack explicit `// SAFETY:` comments.
- `src/lib.rs` — 9 `unsafe extern "C-unwind" fn check_*` GUC validators. Standard pgrx pattern. Most lack `// SAFETY:` comments.
- `src/storage/catalog.rs:102-107` — `CacheRegisterRelcacheCallback` registration with proper `// SAFETY:` annotation. **Compliant.**

Total unsafe blocks: ~30 in `src/`, 0 in `pg_ripple_http/`. **Finding A-3** below.

### 4. Dead-code audit

`#[allow(dead_code)]` count: **80+ occurrences across 23 files**. Top offenders:

- `src/datalog/lattice.rs` — 11 occurrences
- `src/sparql/federation.rs` — 7 occurrences (planner internals)
- `src/error.rs` — 9 occurrences (PT-error variants)
- `src/sparql/sqlgen.rs` — 5
- `src/datalog/dred.rs` — `#![allow(dead_code)]` whole-file
- `src/datalog/compiler.rs` — `#![allow(dead_code)]` whole-file
- `src/sparql/federation_planner.rs` — `#![allow(dead_code)]` whole-file
- `src/telemetry.rs` — `#![allow(dead_code)]` whole-file (ironic for an observability module)
- `src/shacl/constraints/property_path.rs:230` — explicit comment "v0.51.0: available for complex sh:path wiring; not yet called from shacl/mod.rs"

The whole-file `#![allow(dead_code)]` annotations on `compiler.rs`, `dred.rs`, `federation_planner.rs`, and `telemetry.rs` collectively suppress the dead-code warning across approximately **4,000 lines of unverified-by-the-compiler-as-reachable code**.

### 5. TODO / FIXME / HACK audit

`grep -rn "TODO|FIXME|HACK|XXX"` against `src/` and `pg_ripple_http/src/` returns **zero** matches. Strong baseline.

### 6. Dynamic SQL construction audit

`format!("…SELECT…")` / `…INSERT…` / `…CREATE…` / `…DROP…` is used in 30+ sites under `src/datalog/` and a handful under `src/storage/`. Every audited site interpolates only **`pred_id` / `vp_read_expr(pred_id)`** (i64) or constant table-name suffixes — no user-supplied strings. The discipline holds. There is **no CI lint** that would catch a regression (the v0.51.0 CHANGELOG notes `scripts/check_no_string_format_in_sql.sh` was added, but this should be confirmed in CI integration). **Finding A-2** below.

### 7. GUC inventory snapshot

Counted GUC registrations in `src/lib.rs` (lines 230-1,500): approximately **65 GUCs** total. All string-enum GUCs reviewed have `check_hook` validators (closed in v0.47.0 per S5-1). The newly-added `sparql_max_algebra_depth`, `sparql_max_triple_patterns`, `tracing_otlp_endpoint`, `replication_enabled`, `replication_conflict_strategy` (v0.51.0 / v0.54.0) follow the same pattern.

### 8. PT-error catalog drift

PT codes **used in `src/`** (sorted): `PT440`, `PT480`, `PT481`, `PT501`, `PT502`, `PT510`, `PT511`, `PT520`, `PT530`, `PT540`, `PT541`, `PT543`, `PT550`, `PT601`–`PT607`, `PT620`, `PT621`, `PT640`, `PT642`, `PT700`, `PT701`, `PT702`, `PT710`, `PT711`, `PT712`, `PT800`. **Total: 31.**

PT codes **documented in `docs/src/reference/error-catalog.md`**: `PT400`–`PT408`, `PT499`, `PT500`–`PT509`, `PT520`, `PT540`–`PT542`, `PT599`, `PT600`–`PT606`, `PT699`, `PT700`–`PT709`, `PT799`. **Total: 45.**

**Drift:**
- **17 codes used but not documented**: PT440 (DoS protection — the v0.51.0 flagship feature), PT480 (SHACL-AF unsupported), PT481 (SHACL-SPARQL exec failure), PT510, PT511, PT530, PT543, PT550, PT607, PT620, PT621, PT640, PT642, PT711, PT712, PT800 (pg-trickle missing).
- **30+ codes documented but never raised**: includes the entire PT400-series (parser errors) and a long tail of PT700-series codes.

This is the single largest documentation drift surfaced in this assessment. **Finding I-2** below.

### 9. Fuzz target inventory

[fuzz/fuzz_targets/](../fuzz/fuzz_targets/) contains **9 targets**: `datalog_parser.rs`, `dictionary_hash.rs`, `federation_result.rs`, `http_request.rs`, `jsonld_framer.rs`, `rdfxml_parser.rs`, `shacl_parser.rs`, `sparql_parser.rs`, `turtle_parser.rs`. Coverage is comprehensive across user-input surfaces. (Was 6 at v0.50.0.)

### 10. pg_regress test inventory

- `tests/pg_regress/sql/` — **156 files**.
- `tests/pg_regress/expected/` — **157 files**.
- 1-file mismatch suggests a stray expected output without a corresponding test or an unfinished test addition. **Finding J-3** below.

---

## New Findings

### Section A — Rust Code Correctness & Quality

**[A-1] [Medium] `src/shacl/mod.rs` is 2,761 lines — the workspace's largest single file.**
- **Evidence:** `wc -l` output above; this file now exceeds the historical 2025-era `sqlgen.rs` peak.
- **Impact:** Hard to onboard contributors; mixing of parser, validator dispatch, hint translation, AF rule bridge, and SPI plumbing in a single namespace; changes ripple test coverage broadly.
- **Recommendation:** Split into `src/shacl/{parser, validator, hints, af_rules, spi}.rs` along the v0.51.0 pattern that successfully reduced `gucs.rs`. **Effort:** 4–6 person-days.
- **Roadmap target:** v0.55.0.

**[A-2] [Medium] `src/datalog/mod.rs` not actually trimmed despite v0.51.0 CHANGELOG claim.**
- **Evidence:** v0.51.0 CHANGELOG: "New `coordinator.rs` and `seminaive.rs` delegation modules"; but `wc -l src/datalog/mod.rs` = **1,685** vs **1,681** at v0.50.0. The new modules exist alongside the old code; nothing was removed.
- **Impact:** Comment-only refactor; dead code accumulates; size of the central Datalog dispatcher continues to grow. Both new modules are themselves wrapped in `#[allow(dead_code)]` (lines 20, 28 of `seminaive.rs`; lines 24, 34 of `coordinator.rs`).
- **Recommendation:** Either (a) genuinely move semi-naïve and coordinator logic out of `mod.rs` and remove the `#[allow(dead_code)]` annotations, or (b) revert the modules and update the CHANGELOG. **Effort:** 5–8 person-days.
- **Roadmap target:** v0.55.0.

**[A-3] [Low] `unsafe` blocks lack `// SAFETY:` comments per AGENTS.md policy.**
- **Evidence:** ~25 of the ~30 `unsafe` blocks in `src/` have no leading `// SAFETY:` comment. AGENTS.md explicitly mandates this. Compliant: [src/storage/catalog.rs:102-107](../src/storage/catalog.rs#L102) ("// SAFETY: `CacheRegisterRelcacheCallback` is a standard PostgreSQL"). Non-compliant examples: [src/export.rs:548,630,705](../src/export.rs#L548), [src/shmem.rs:75-120](../src/shmem.rs#L75), all `check_*` GUC validators in [src/lib.rs:260-460](../src/lib.rs#L260).
- **Impact:** Policy drift; reviewers cannot mechanically verify safety invariants.
- **Recommendation:** Add the comments; add a `clippy::missing_safety_doc`-equivalent script under `scripts/check_unsafe_safety.sh`. **Effort:** 1 person-day.
- **Roadmap target:** v0.55.0.

**[A-4] [Low] `src/replication.rs` introduces hot-path `unwrap()`.**
- **Evidence:** [src/replication.rs:188,190](../src/replication.rs#L188) — `.unwrap()` on SPI row decode and on `value::<String>()`. v0.54.0 new code; the apply worker runs continuously.
- **Impact:** Worker panics on malformed slot row → restart loop → potential replication stall.
- **Recommendation:** Convert to `match` with `pgrx::warning!` + skip-row. **Effort:** 1 hour.
- **Roadmap target:** v0.55.0 (point-fix).

**[A-5] [Low] `src/sparql/federation.rs:94` `opt.as_ref().unwrap().clone()` in non-test code path.**
- **Evidence:** [src/sparql/federation.rs:94](../src/sparql/federation.rs#L94).
- **Impact:** Potential panic if invariants ever change; cheap to make explicit.
- **Recommendation:** Use `expect("…")` with explicit invariant or `?`-propagation.
- **Roadmap target:** v0.55.0.

**[A-6] [Low] Whole-file `#![allow(dead_code)]` on `telemetry.rs`, `compiler.rs`, `dred.rs`, `federation_planner.rs`.**
- **Evidence:** `grep -rn "#!\[allow(dead_code)\]"`.
- **Impact:** Compiler cannot detect that none of the file is reachable; ~4,000 lines of partially-unverified code paths.
- **Recommendation:** Replace the file-wide annotation with per-item annotations; or write a tracking ticket per file with a deadline.
- **Roadmap target:** v0.56.0.

**[A-7] [Low] `cargo deny` / `cargo audit` integration.**
- **Evidence:** `audit.toml` and `deny.toml` present at repo root; v0.51.0 CHANGELOG: "Blocking cargo-audit on PRs". Verification of CI gate passing on every PR is not in scope.
- **Status:** Closed pending CI snapshot inspection.

---

### Section B — SPARQL Engine Completeness & Correctness

**[B-1] [Medium] No automated SPARQL 1.1 entailment-regime conformance tests.**
- **Evidence:** `grep -rn "entailment_regime\|sparql.*entailment" tests/` returns zero hits. The Datalog layer implements OWL 2 RL (per N5-4 closed) but there is no test that runs the W3C SPARQL Entailment Regimes test suite against pg_ripple's combined SPARQL+Datalog stack.
- **Impact:** Conformance claim is implicit; users cannot rely on RDFS/OWL entailment under SPARQL queries without manual proof.
- **Recommendation:** Add `tests/sparql_entailment/` driver that runs the W3C RDFS Entailment + OWL 2 RL Entailment subsets.
- **Roadmap target:** v0.56.0.

**[B-2] [Medium] `DESCRIBE` query semantics not documented.**
- **Evidence:** `pg_ripple.describe_strategy` GUC exists ([src/lib.rs:336](../src/lib.rs#L336)) but `docs/src/reference/sparql-compliance.md` does not enumerate which strategies are supported (CBD, SCBD, etc.) or what the default behaviour is.
- **Impact:** Result-set semantics ambiguous; user surprise.
- **Recommendation:** Document each `describe_strategy` value and link to the relevant W3C definition.
- **Roadmap target:** v0.55.0.

**[B-3] [Low] No locale-aware ORDER BY documented.**
- **Evidence:** SPARQL spec §15.1 requires that `ORDER BY` be per-data-type aware. PostgreSQL `ORDER BY` is locale-aware on text. Behaviour with mixed-type results (xsd:integer + xsd:string) is unspecified in pg_ripple docs.
- **Roadmap target:** v0.56.0.

**[B-4] [Low] No SPARQL-star evaluation conformance test.**
- **Evidence:** `tests/pg_regress/sql/sparql_star_*.sql` covers ground-quoted-triple parsing and CONSTRUCT round-trip but not annotation-pattern semantics under variable bindings.
- **Roadmap target:** v0.55.0.

---

### Section C — RDF Standards & Serialization

**[C-1] [Medium] No NFC/NFD Unicode normalization test for dictionary identity.**
- **Evidence:** `grep -rn "NFC\|NFD\|unicode_normalization" src/dictionary/ tests/` returns zero hits.
- **Impact:** A SPARQL `INSERT DATA` followed by a query bound to a visually-identical but differently-normalized IRI will return zero matches — and the operator has no signal this is happening.
- **Recommendation:** Either normalize to NFC at dictionary-encode time (preferred) or document the gotcha and add a `pg_ripple.normalize_iris` GUC.
- **Roadmap target:** v0.55.0.

**[C-2] [Medium] `copy_rdf_from()` lacks file-permission and path-traversal safeguards.**
- **Evidence:** [src/dict_api.rs:503](../src/dict_api.rs#L503) — `fn copy_rdf_from(path: &str, format: &str)`. Server-side file open, superuser-callable. No documented path-prefix allowlist.
- **Impact:** Trusted-but-careless superuser can be tricked via psql variable interpolation; chain risk on multi-tenant clusters.
- **Recommendation:** Add `pg_ripple.copy_rdf_allowed_paths` GUC (default: empty = disabled in non-superuser sessions) and reject paths outside the allowlist.
- **Roadmap target:** v0.55.0.

**[C-3] [Low] JSON-LD framing `@graph` algorithm coverage unknown.**
- **Evidence:** [src/framing/](../src/framing/) submodules `frame_translator.rs`, `embedder.rs`, `compactor.rs` exist; v0.50.0 marked the framing surface as positive baseline. Coverage of `@graph`, `@nest`, `@included`, `@version: 1.1` is not enumerated in `docs/src/reference/`.
- **Roadmap target:** v0.56.0.

---

### Section D — SHACL Completeness & Correctness

**[D-1] [Medium] `src/shacl/constraints/property_path.rs:230` still has unwired helper.**
- **Evidence:** `#[allow(dead_code)] // v0.51.0: available for complex sh:path wiring; not yet called from shacl/mod.rs`.
- **Impact:** Either the wired-in path covers all cases (in which case the helper should be removed), or there is a class of `sh:path` expressions not yet covered.
- **Recommendation:** Either delete the helper or wire it in; resolve the inconsistency.
- **Roadmap target:** v0.55.0.

**[D-2] [Medium] SHACL async validation has no documented stale-result guarantee.**
- **Evidence:** `grep -rn "stale\|version" src/shacl/` returns no relevant invariant. Async validator returns the report computed against the graph snapshot at validation start — but if the graph mutates concurrently, the report references SIDs that may have been deleted by the time the user queries the report.
- **Impact:** User confusion on highly-concurrent workloads.
- **Recommendation:** Document the snapshot semantics; expose the validation-start LSN or transaction snapshot ID alongside the report.
- **Roadmap target:** v0.55.0.

**[D-3] [Low] `sh:rule` SHACL-AF execution still placeholder (S4-8 partial).**
- **Evidence:** `bridge_shacl_rules()` at [src/shacl/mod.rs:993](../src/shacl/mod.rs#L993) emits PT480 warning when inference is off; under `'on_demand'` / `'materialized'` it claims to "compile rule bodies into the Datalog engine" but the implementation depth was not verified at this scope.
- **Recommendation:** Add a pg_regress test that loads a SHACL-AF shapes graph with `sh:rule` and asserts inferred triples appear.
- **Roadmap target:** v0.55.0.

---

### Section E — Datalog Engine

**[E-1] [Medium] `execute_with_savepoint()` still dead code (S3-4 carry-forward).**
- **Evidence:** [src/datalog/parallel.rs:340-341](../src/datalog/parallel.rs#L340) `#[allow(dead_code)] pub fn execute_with_savepoint(stmts: &[String], savepoint_name: &str) -> bool`. Comments at [src/datalog/mod.rs:498](../src/datalog/mod.rs#L498) describe future plans.
- **Impact:** Parallel-strata partial-failure isolation never engages. A coordinator crash mid-stratum can leave inconsistent derived facts visible.
- **Recommendation:** Wire into `parallel::execute_strata()` or schedule deletion. **Effort:** 2–3 person-days to wire correctly with savepoint name uniqueness.
- **Roadmap target:** v0.55.0.

**[E-2] [Low] `src/datalog/dred.rs` is whole-file `#![allow(dead_code)]`.**
- **Evidence:** [src/datalog/dred.rs:29](../src/datalog/dred.rs#L29).
- **Impact:** AGENTS.md and ROADMAP claim "DRed (Deletion and Re-derivation)" is delivered; the file marker contradicts that claim. Either DRed is dead code, or the marker is outdated.
- **Roadmap target:** v0.55.0.

**[E-3] [Low] `src/datalog/compiler.rs` is whole-file `#![allow(dead_code)]`.**
- **Evidence:** [src/datalog/compiler.rs:1](../src/datalog/compiler.rs#L1).
- **Impact:** As above; the compiler module is the SQL-generation backbone for Datalog. Either much of it is unused (suggesting the active path is elsewhere) or the marker is wrong.
- **Roadmap target:** v0.55.0.

---

### Section F — Storage, HTAP & Performance

**[F-1] [High] HTAP merge cutover view-recreation race remains (C-3 carry-forward).**
- **Evidence:** [src/storage/merge.rs:332-360](../src/storage/merge.rs#L332): `DROP TABLE IF EXISTS {main} CASCADE` (drops the view) → `ALTER TABLE … RENAME` → `CREATE OR REPLACE VIEW`. A query starting between the DROP and the CREATE will see no relation. `SET LOCAL lock_timeout = '5s'` only bounds the wait, not the existence-window.
- **Impact:** Sustained mixed read-write workloads can observe ERROR `relation "_pg_ripple.vp_<id>" does not exist` mid-merge. No reproducer exists in the test tree.
- **Recommendation:** Eliminate the view entirely (use a stable schema and atomically swap underlying tables via `RENAME`-with-shadow approach, e.g., create `vp_{id}_main_new`, fill, `BEGIN; LOCK vp_{id}; ALTER TABLE vp_{id}_main RENAME TO vp_{id}_main_old; ALTER TABLE vp_{id}_main_new RENAME TO vp_{id}_main; DROP TABLE vp_{id}_main_old; COMMIT;`). **Effort:** 1–2 weeks including stress test.
- **Roadmap target:** v0.55.0 (v1.0.0 blocker).

**[F-2] [Medium] No tombstone GC schedule documented or implemented.**
- **Evidence:** `grep -rn "tombstone.*gc\|prune.*tombstone" src/` returns nothing relevant. Tombstones for resolved deletes accumulate in `vp_{id}_tombstones`.
- **Impact:** Long-running deployments accumulate dead tombstone rows; query path UNION-EXCEPT pays the cost.
- **Recommendation:** After a successful merge cycle that consumed all tombstones, truncate the tombstone table within the same transaction. Add a `pg_ripple.tombstone_retention_seconds` GUC.
- **Roadmap target:** v0.55.0.

**[F-3] [Medium] Statement-ID (SID) sequence wraparound undefined behaviour.**
- **Evidence:** `_pg_ripple.statement_id_seq` is a standard `BIGINT` sequence (`grep -n "statement_id_seq" sql/pg_ripple--0.1.0.sql`). At ~10⁹ inserts/day, wraparound takes ~25,000 years, but no test, GUC, or documentation addresses behaviour at `i64::MAX`.
- **Impact:** Latent bug for very-long-lived deployments or if the sequence is incremented manually for testing.
- **Recommendation:** Document the limit; add a monitoring SRF `pg_ripple.sid_runway()` returning years remaining at current rate.
- **Roadmap target:** v0.56.0.

**[F-4] [Medium] Rare-predicate auto-promotion concurrency safety untested.**
- **Evidence:** `grep -rn "concurrent.*promot\|promote.*test" src/ tests/` returns zero hits. Two concurrent inserts that both push a predicate over the threshold could both attempt promotion.
- **Recommendation:** Use `pg_advisory_xact_lock(predicate_id)` around the promotion path and add a multi-connection pg_regress test under `tests/concurrent/`.
- **Roadmap target:** v0.55.0.

**[F-5] [Medium] No CI gate on `merge_throughput_baselines.json`.**
- **Evidence:** [benchmarks/merge_throughput_baselines.json](../benchmarks/merge_throughput_baselines.json) committed; `grep -n "merge_throughput" .github/workflows/ci.yml` returns nothing.
- **Impact:** Performance regression in merge worker could ship undetected.
- **Recommendation:** Add a `pgbench`-driven CI job that writes p50/p95 to JSON and compares to baseline with a 15 % regression threshold.
- **Roadmap target:** v0.55.0.

**[F-6] [Medium] No CI gate on vector-index baseline.**
- **Evidence:** Same pattern as F-5 for [docs/src/reference/vector-index-tradeoffs.md](../docs/src/reference/vector-index-tradeoffs.md).
- **Roadmap target:** v0.55.0.

**[F-7] [Low] BRIN index maintenance after merge unverified.**
- **Evidence:** No explicit BRIN index re-summarization step in [src/storage/merge.rs](../src/storage/merge.rs). PostgreSQL BRIN is summarized incrementally; on a freshly-renamed `vp_{id}_main`, BRIN summaries are recomputed lazily by `brin_summarize_new_values()`.
- **Recommendation:** After merge completion, call `SELECT brin_summarize_new_values('idx_name')` once for the new main table.
- **Roadmap target:** v0.56.0.

---

### Section G — Federation & HTTP Companion

**[G-1] [Critical] Federation lacks SSRF allowlist.**
- **Evidence:** `grep -n "federation_allowed_endpoints\|169.254\|127.0.0.1\|file://\|localhost" src/sparql/federation.rs src/gucs/federation.rs` returns **zero matches**. There is no GUC, no IP-range filter, no scheme allowlist. A user-supplied `SERVICE <http://169.254.169.254/latest/meta-data/iam/security-credentials/>` query in AWS / Azure / GCP retrieves cloud-instance credentials.
- **Impact:** **Privilege escalation** in any deployment where SPARQL queries arrive from untrusted users (which is the entire `pg_ripple_http` use case).
- **Recommendation:** New GUC `pg_ripple.federation_endpoint_policy` with values `default-deny | allowlist | open`; `pg_ripple.federation_allowed_endpoints` (text[] of host or CIDR); reject `file://`, `localhost`, `127.0.0.0/8`, `::1/128`, `169.254.0.0/16`, `fe80::/10`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16` unless explicitly allowlisted. **Effort:** 1 week including tests.
- **Roadmap target:** v0.55.0 (v1.0.0 blocker).

**[G-2] [Medium] No HTTP request size limit beyond axum default.**
- **Evidence:** `grep -n "max.*body\|body.*limit\|RequestBodyLimit" pg_ripple_http/src/main.rs` — needs verification. Axum default is 2 MiB but a SPARQL query exceeding this returns a generic 413 with no PT-error code and no audit log.
- **Recommendation:** Explicit `axum::extract::DefaultBodyLimit` configured via env; PT-error code on rejection.
- **Roadmap target:** v0.55.0.

**[G-3] [Medium] No federation circuit breaker.**
- **Evidence:** `grep -rn "circuit_break\|backoff" src/sparql/federation.rs` returns nothing.
- **Impact:** A persistently-down remote endpoint is retried on every query. Latency cliff.
- **Recommendation:** Per-endpoint failure counter; open the circuit after N consecutive failures with exponential backoff.
- **Roadmap target:** v0.56.0.

**[G-4] [Medium] No federation result-decoder backpressure visibility.**
- **Evidence:** `pg_ripple.federation_max_response_bytes` GUC bounds total bytes (S2-4 closed at v0.48.0) but there is no SRF reporting recent federation call durations / sizes per endpoint.
- **Recommendation:** Add `pg_ripple.federation_call_stats() RETURNS TABLE(endpoint text, calls int, p50_ms int, p95_ms int, errors int, last_error timestamptz)`.
- **Roadmap target:** v0.55.0.

---

### Section H — Security

**[H-1] [Critical] = G-1 (federation SSRF).**
- See above.

**[H-2] [Medium] LLM API key handling unspecified.**
- **Evidence:** `grep -rn "llm_api_key\|api_key" src/llm/ src/gucs/llm.rs` — needs read. If the API key is stored as a GUC, it appears in `pg_settings` and `pg_stat_activity` for any role with read access.
- **Recommendation:** Store secrets via PostgreSQL roles + `pgcrypto` or external secret manager; document that `llm_api_key` GUC is **not** safe for shared servers.
- **Roadmap target:** v0.55.0.

**[H-3] [Medium] No SPARQL audit log.**
- **Evidence:** `grep -rn "audit\|ddl_log" src/` returns nothing relevant. SPARQL Update operations leave no privileged audit trail; only `pg_stat_statements` (which any role with read access sees).
- **Recommendation:** Add `pg_ripple.audit_log_enabled` GUC + `_pg_ripple.audit_log` table that records SPARQL UPDATE / DELETE / DROP / CLEAR with `(timestamp, role, txid, query, affected_predicates int[])`.
- **Roadmap target:** v0.56.0.

**[H-4] [Low] No SQL-injection lint enforced in CI.**
- **Evidence:** v0.51.0 CHANGELOG mentions `scripts/check_no_string_format_in_sql.sh`; CI integration not verified here.
- **Recommendation:** Confirm the lint runs on every PR.

**[H-5] [Low] `secrets/` directory naming footgun (carry-forward N6-5).**
- Need rename or top-level README.

---

### Section I — Observability & Operations

**[I-1] [High] Error-catalog drift: 17 PT codes used but undocumented; 30+ documented but unused.**
- **Evidence:** Pre-analysis scan #8 above. `PT440` (the v0.51.0 SPARQL DoS code) is not in `docs/src/reference/error-catalog.md`.
- **Impact:** Operators receiving an undocumented PT code cannot self-serve diagnosis; documented codes that do not exist mislead users into looking for non-existent errors.
- **Recommendation:** Run a weekly `scripts/check_pt_codes.sh` (mentioned in v0.51.0 CHANGELOG) as a CI gate; reconcile both directions before v1.0.0.
- **Roadmap target:** v0.55.0.

**[I-2] [Medium] No PostgreSQL-event-trigger / DDL-audit observability.**
- **Evidence:** `grep -rn "event_trigger\|ddl_command_end" src/` returns nothing.
- **Impact:** Schema mutations (e.g., manual `DROP TABLE _pg_ripple.vp_42`) are invisible to pg_ripple's catalog.
- **Recommendation:** Register a PG event trigger that emits PT5xx warning if a `_pg_ripple.*` relation is dropped outside `pg_ripple.vacuum()`.
- **Roadmap target:** v0.56.0.

**[I-3] [Medium] `pg_ripple_http` health endpoint payload coverage unverified.**
- **Evidence:** `grep -n "/health\|version\|build" pg_ripple_http/src/main.rs` would confirm.
- **Recommendation:** Ensure health JSON contains `{version, git_sha, build_time, postgres_connected, last_query_ts}`.
- **Roadmap target:** v0.55.0.

---

### Section J — Test Coverage & Conformance

**[J-1] [Medium] pg_regress test/expected mismatch (156 vs 157).**
- **Evidence:** Pre-analysis scan #10.
- **Impact:** A stray expected-output file or a renamed/deleted test.
- **Recommendation:** `diff <(ls tests/pg_regress/sql/ | sed 's/\.sql$//') <(ls tests/pg_regress/expected/ | sed 's/\.out$//')` and reconcile.
- **Roadmap target:** v0.55.0.

**[J-2] [Medium] No mid-merge crash-recovery test.**
- **Evidence:** `grep -rn "kill -9\|SIGKILL" tests/` returns nothing.
- **Impact:** Crash-during-merge invariant unverified by automation.
- **Recommendation:** Add `tests/crash_recovery/merge_kill.sh` that starts a heavy `INSERT INTO pg_ripple.triples`, signals the merge worker mid-cycle, restarts PG, and verifies the database is consistent and queryable.
- **Roadmap target:** v0.55.0 (v1.0.0 blocker).

**[J-3] [Medium] No concurrent-write stress test in pg_regress.**
- **Evidence:** `ls tests/pg_regress/sql/ | grep -i concurr` returns nothing.
- **Recommendation:** Add `tests/concurrent/parallel_insert.sh` using `pgbench` with 16 concurrent clients all calling `pg_ripple.load_ntriples`.
- **Roadmap target:** v0.55.0.

**[J-4] [Low] WatDiv now blocking (closed); promote OWL 2 RL too.**
- **Evidence:** `.github/workflows/ci.yml:703` `owl2rl-suite: continue-on-error: true`. Despite v0.51.0 reaching 100 % conformance, the gate is still informational.
- **Recommendation:** Flip to `continue-on-error: false`.
- **Roadmap target:** v0.55.0.

**[J-5] [Low] Jena suite still informational at 99.9 %.**
- **Evidence:** `.github/workflows/ci.yml:398` `continue-on-error: true`.
- **Recommendation:** Flip to blocking.

---

### Section K — Developer Experience & Documentation

**[K-1] [Medium] No published OpenAPI spec for `pg_ripple_http` (N8-4 carry-forward).**
- **Evidence:** `ls docs/src/reference/openapi.yaml` → not found.
- **Impact:** Client libraries cannot be auto-generated.
- **Recommendation:** Annotate axum handlers with `utoipa` and publish `openapi.yaml`.
- **Roadmap target:** v0.55.0.

**[K-2] [Low] `examples/` lacks v0.54.0 logical-replication runnable example.**
- **Evidence:** `ls examples/` shows `cdc_subscription.sql`, `cloudnativepg_cluster.yaml`, `federation_multi_endpoint.sql`, `graphrag_*.sql`, `hybrid_vector_search.sql`, `llm_workflow.sql`, `sample_graphs.sql`, `shacl_*` — no `replication_setup.sql`.
- **Roadmap target:** v0.55.0.

**[K-3] [Low] `pg_ripple.control` `default_version` must equal latest tag.**
- **Evidence:** `pg_ripple.control:5` `default_version = '0.54.0'`; latest tag `v0.54.0`. **Compliant.**

**[K-4] [Low] Migration script chain continuous (verified).**
- **Evidence:** 55 migration scripts; spot check `0.49.0--0.50.0` through `0.53.0--0.54.0` all present.

---

## Feature Recommendations for Future Roadmap

24+ recommendations across 8 areas. Each lists motivation, sketch, effort, milestone, and comparable system.

### L-1. Query Language Extensions

**L-1.1 — GeoSPARQL 1.1 (geosparql / sf vocabularies).** The v0.54.0 batteries image already bundles PostGIS 3.4. Implement `geof:distance`, `geof:within`, `geof:intersects`, `geo:asWKT` as SPARQL built-ins delegating to PostGIS `ST_*`. Effort: Major (4–6 weeks). Milestone: v0.56.0. Comparable: GraphDB GeoSPARQL extension, Stardog Geospatial.

**L-1.2 — SPARQL 1.2 draft features.** Track the W3C SPARQL 1.2 working draft: lateral joins, `EXISTS` outside FILTER, `VALUES` after `WHERE`. Effort: Medium per feature. Milestone: rolling. Comparable: Apache Jena ARQ.

**L-1.3 — Temporal RDF queries.** A `pg_ripple.point_in_time(timestamptz)` setting that scopes SPARQL `SELECT`/`CONSTRUCT` to the graph state at that time, leveraging the SID + a per-SID `valid_from / valid_to` extension. Effort: Major (4–8 weeks). Comparable: Bitemporal Stardog.

**L-1.4 — SPARQL-DL.** Implement the Description-Logic-flavoured query subset for OWL ontology Q&A. Effort: Major. Comparable: Pellet, OWLAPI.

### L-2. Storage Model Evolution

**L-2.1 — Columnar VP storage via `pg_columnar` / Citus columnar.** Convert long-tail VP tables to columnar storage for analytical workloads. Effort: Medium-Major. Milestone: v0.57.0. Comparable: GraphDB OWLIM-SE, AWS Neptune Analytics.

**L-2.2 — Adaptive index choice.** Track query patterns and dynamically add `(o, s)`, `(g, s, o)`, or BRIN index if the workload warrants. Effort: Major. Milestone: v0.57.0.

**L-2.3 — Materialized SPARQL views.** `CREATE MATERIALIZED VIEW … AS SELECT * FROM pg_ripple.sparql('…')` with auto-refresh on VP delta merge. Effort: Medium. Milestone: v0.55.0. Comparable: Virtuoso materialized views.

**L-2.4 — Dictionary block compression.** Page-level Zstd compression on the dictionary `string_value` column for IRIs sharing prefixes. Effort: Medium.

### L-3. Reasoning Enhancements

**L-3.1 — OWL 2 EL profile.** Subsumption reasoning for biomedical / SNOMED-CT use cases. Effort: Major (6–8 weeks). Comparable: ELK reasoner.

**L-3.2 — OWL 2 QL profile.** Query rewriting under DL-Lite for classical relational backends. Effort: Major. Comparable: Ontop.

**L-3.3 — Incremental RDFS closure.** Trigger-driven delta propagation when `rdfs:subClassOf` / `rdfs:subPropertyOf` is asserted. Effort: Medium. Comparable: GraphDB OWL Horst.

**L-3.4 — Probabilistic reasoning sketch.** Markov-Logic-style soft-rule scoring atop Datalog. Effort: Major-Research. Comparable: PSL, ProbLog.

### L-4. AI / ML Integration

**L-4.1 — Knowledge-graph embeddings (TransE / RotatE / ComplEx).** Background worker computes embeddings for all entities; results in `_pg_ripple.kge_embeddings`. Effort: Major (4–6 weeks). Milestone: v0.57.0. Comparable: Stardog GraphML, Amazon Neptune ML.

**L-4.2 — Entity alignment via embedding similarity.** `pg_ripple.find_alignments(threshold)` SRF that finds candidate `owl:sameAs` pairs from KGE space. Effort: Medium (builds on L-4.1).

**L-4.3 — LLM-augmented SPARQL repair.** When a SPARQL query parse-errors, send the source + error to the configured LLM with the schema digest and propose a corrected query. Effort: Medium. Comparable: text-to-SPARQL benchmarks.

**L-4.4 — Automated ontology mapping.** Use sentence-transformer embeddings of class labels to propose alignments between two loaded ontologies. Effort: Medium-Major.

### L-5. Operational Excellence

**L-5.1 — Read-replica routing for SELECT/CONSTRUCT/ASK.** v0.54.0 logical replication exists; route read-only SPARQL to a configured replica via `pg_ripple.read_replica_dsn`. Effort: Medium. Milestone: v0.55.0.

**L-5.2 — Rolling upgrade across two PG18 minor versions.** Document and CI-test that pg_ripple supports `pg_upgrade` from PG18.0 → PG18.x without downtime via logical replication. Effort: Medium.

**L-5.3 — Multi-tenant graph isolation.** Per-named-graph row-level security so multiple tenants can share a single pg_ripple instance. Effort: Major. Milestone: v0.57.0. Comparable: Stardog Permissions, GraphDB ACL.

**L-5.4 — Sharding via Citus integration.** Hash-partition VP tables by predicate ID across Citus workers. Effort: Major-Research. Milestone: v0.58.0+.

### L-6. Ecosystem Integrations

**L-6.1 — Apache Arrow / Flight bulk export.** `pg_ripple.export_arrow_flight(graph_iri)` streams VP rows as Arrow batches. Effort: Medium. Milestone: v0.56.0. Comparable: AWS Neptune Bulk Export.

**L-6.2 — Apache Kafka CDC sink.** Native Kafka producer wired to `pg_ripple_cdc_lifecycle` channel. Effort: Medium. Milestone: v0.55.0. Comparable: Debezium pattern.

**L-6.3 — dbt adapter.** A `dbt-pg-ripple` package with SPARQL macros. Effort: Medium. Milestone: v0.56.0.

**L-6.4 — Jupyter SPARQL kernel.** `pg-ripple-kernel` mapping `%%sparql` cells to `pg_ripple_http`. Effort: Quick Win. Milestone: v0.55.0.

**L-6.5 — LangChain / LlamaIndex tool integration.** Pre-built `pg_ripple` tool class for the major LLM frameworks. Effort: Quick Win.

### L-7. Standards Completeness

**L-7.1 — RDF 1.2 normative RDF-star alignment.** Track the W3C RDF 1.2 working draft as the spec stabilises. Effort: Medium per minor revision.

**L-7.2 — VoID dataset descriptions.** Auto-publish VoID under `pg_ripple_http /void` summarising loaded graphs, predicates, and statistics. Effort: Quick Win. Milestone: v0.55.0.

**L-7.3 — R2RML / RML direct mapping.** `pg_ripple.r2rml_load(mapping_iri)` to materialize relational tables as RDF graphs. Effort: Major (3–4 weeks). Milestone: v0.57.0. Comparable: Ontop.

**L-7.4 — SPARQL Service Description.** `pg_ripple_http /service` returns a Turtle service-description doc. Effort: Quick Win.

### L-8. Security & Compliance

**L-8.1 — Row-level security on named graphs.** PostgreSQL RLS policies that restrict named-graph access by role. Effort: Medium. Milestone: v0.55.0.

**L-8.2 — SPARQL audit log.** Per-mutation audit (= H-3 finding). Effort: Medium. Milestone: v0.56.0.

**L-8.3 — GDPR right-to-erasure tooling.** `pg_ripple.erase_subject(iri)` SRF that removes all triples about a subject across all VP tables, the dictionary, and the embeddings table. Effort: Medium. Milestone: v0.55.0.

**L-8.4 — Graph provenance tracking (PROV-O).** Auto-emit `prov:wasDerivedFrom` edges on Datalog-inferred triples. Effort: Medium-Major.

---

## Prioritized Remediation Backlog

| ID | Severity | Dimension | Title | Effort | Suggested Release |
|---|---|---|---|---|---|
| G-1 / H-1 | Critical | Federation / Security | Federation SSRF allowlist | 1 wk | v0.55.0 |
| F-1 / C-3 | High | Storage | HTAP cutover view-recreation race | 1–2 wk | v0.55.0 |
| I-1 | High | Obs | Error-catalog drift (17 undocumented PT codes incl. PT440) | 2 days | v0.55.0 |
| A-1 | Medium | Arch | Split `src/shacl/mod.rs` (2,761 lines) | 4–6 days | v0.55.0 |
| A-2 | Medium | Arch | Genuinely trim `src/datalog/mod.rs` | 5–8 days | v0.55.0 |
| F-2 | Medium | Storage | Tombstone GC after merge | 3 days | v0.55.0 |
| F-4 | Medium | Storage | Concurrent rare-predicate promotion safety | 3 days | v0.55.0 |
| F-5 | Medium | Perf | Merge-throughput CI gate | 3 days | v0.55.0 |
| F-6 | Medium | Perf | Vector-index CI gate | 3 days | v0.55.0 |
| E-1 / S3-4 | Medium | Datalog | Wire `execute_with_savepoint()` | 2–3 days | v0.55.0 |
| G-2 | Medium | HTTP | Explicit body-size limit + PT-error | 1 day | v0.55.0 |
| G-3 | Medium | HTTP | Federation circuit breaker | 1 wk | v0.56.0 |
| G-4 | Medium | HTTP | Federation call stats SRF | 3 days | v0.55.0 |
| H-2 | Medium | Sec | LLM API key handling guidance | 2 days | v0.55.0 |
| H-3 | Medium | Sec | SPARQL audit log | 1 wk | v0.56.0 |
| C-1 | Medium | RDF | NFC/NFD Unicode normalization | 3 days | v0.55.0 |
| C-2 | Medium | RDF | `copy_rdf_from()` path allowlist | 2 days | v0.55.0 |
| D-1 | Medium | SHACL | Wire or remove `sh:path` helper at line 230 | 1 day | v0.55.0 |
| D-2 | Medium | SHACL | Async validation snapshot semantics doc | 1 day | v0.55.0 |
| D-3 / S4-8 | Medium | SHACL | SHACL-AF rule execution + test | 1 wk | v0.55.0 |
| B-1 | Medium | SPARQL | SPARQL Entailment Regimes test driver | 1 wk | v0.56.0 |
| B-2 | Medium | SPARQL | DESCRIBE strategy doc | 2 days | v0.55.0 |
| J-1 | Medium | Tests | Reconcile pg_regress 156/157 mismatch | 1 hr | v0.55.0 |
| J-2 | Medium | Tests | Mid-merge crash-recovery test | 3 days | v0.55.0 |
| J-3 | Medium | Tests | Concurrent-write stress test | 3 days | v0.55.0 |
| K-1 / N8-4 | Medium | DX | OpenAPI spec via utoipa | 1 wk | v0.55.0 |
| S2-5 | Medium | SPARQL | Remove deprecated `property_path_max_depth` | 1 hr | v0.56.0 |
| A-3 | Low | Arch | Add `// SAFETY:` comments | 1 day | v0.55.0 |
| A-4 | Low | Arch | Replace `unwrap()` in `replication.rs` | 1 hr | v0.55.0 |
| A-5 | Low | Arch | Federation hot-path `unwrap()` | 1 hr | v0.55.0 |
| A-6 | Low | Arch | Per-item dead-code annotations | 1 day | v0.56.0 |
| E-2 | Low | Datalog | `dred.rs` allow(dead_code) review | 2 days | v0.55.0 |
| E-3 | Low | Datalog | `compiler.rs` allow(dead_code) review | 2 days | v0.55.0 |
| F-3 | Low | Storage | SID runway monitoring SRF | 2 days | v0.56.0 |
| F-7 | Low | Storage | BRIN summarize after merge | 2 days | v0.56.0 |
| B-3 | Low | SPARQL | ORDER BY collation doc | 1 day | v0.56.0 |
| B-4 | Low | SPARQL | SPARQL-star annotation-pattern test | 2 days | v0.55.0 |
| C-3 | Low | RDF | JSON-LD `@graph` framing coverage doc | 2 days | v0.56.0 |
| I-2 | Low | Obs | DDL-audit event trigger | 3 days | v0.56.0 |
| I-3 | Low | Obs | Health endpoint payload coverage | 1 hr | v0.55.0 |
| J-4 | Low | Tests | Promote OWL 2 RL gate to blocking | 1 hr | v0.55.0 |
| J-5 | Low | Tests | Promote Jena gate to blocking | 1 hr | v0.55.0 |
| K-2 | Low | DX | Replication runnable example | 1 hr | v0.55.0 |
| N6-5 | Low | Sec | Rename `secrets/` directory | 1 hr | v0.55.0 |

---

## Maturity Score Breakdown

| Dimension | Score / 5 | Δ vs v0.50.0 | Justification |
|---|---|---|---|
| Storage & HTAP correctness | **4.4** | -0.1 | C-3 cutover race **still open**; SID wraparound undocumented; tombstone GC absent; v0.51.0 syscache callback closed |
| SPARQL correctness & spec compliance | **4.8** | +0.1 | PT440 DoS limit + CSV/TSV + ground RDF-star CONSTRUCT closed; Entailment Regimes test driver still missing |
| Datalog reasoning | **4.6** | -0.1 | OWL 2 RL 100 % closed (great!); but `dred.rs`, `compiler.rs`, `execute_with_savepoint()` carry whole-file or function-level dead-code annotations that contradict the "delivered" claim |
| SHACL completeness | **4.6** | +0.1 | SHACL-SPARQL closed; complex `sh:path` wired; PT480 warning; SHACL-AF execution still placeholder |
| Federation & HTTP service | **4.3** | -0.4 | TLS fingerprint pinning + HTTP streaming closed (great!) but **no SSRF allowlist** (G-1) drags this score significantly |
| Security | **4.5** | +0.2 | Docker non-root + SBOM + SPARQL DoS limit + TLS fingerprint pinning + cargo-audit blocking; SSRF + LLM key handling + audit log are the remaining drags |
| Test coverage & conformance | **4.7** | 0.0 | 9 fuzz targets (was 6); WatDiv blocking; OWL 2 RL 100 % but gate still informational; mid-merge crash test absent |
| Performance & scalability | **4.4** | +0.2 | HTTP streaming closed; baselines published but no CI gate; concurrent-merge stress test absent |
| Observability & operations | **4.5** | +0.2 | OTLP wiring + per-predicate stats + `explain_sparql(analyze=true)` + 19 ops docs; PT-catalog drift drags by 0.2 |
| Developer experience | **4.7** | +0.2 | Architecture diagram, Helm, CNP, batteries image, `just release`/`just docs-serve`; OpenAPI still missing |
| Standards completeness | **4.6** | n/a | All 17 SPARQL builtins, all 11 Update ops, RDF-star round-trip, OWL 2 RL 100 %; SHACL-SPARQL implemented; SPARQL Entailment + GeoSPARQL + R2RML still gaps |
| **Overall (weighted)** | **4.55 → 4.78** | **+0.23** | Movement driven by SHACL-SPARQL, OWL 2 RL 100 %, HTTP streaming, OTLP, fingerprint pinning, non-root container, logical replication, Helm/CNP. Drag from SSRF + HTAP race + error-catalog drift + god-modules |

Closing **G-1 (SSRF), F-1 (HTAP race), I-1 (PT-catalog), A-1 (shacl/mod split), J-2 (mid-merge crash test)** restores Federation→4.7, Storage→4.7, Obs→4.7, Arch quality→4.7, and lifts **Overall to 4.85+ / 5.0** — credible v1.0.0 quality.

---

## Appendix A — Raw Pre-Analysis Outputs

### Line counts (files > 500 lines)

```
   43841 total
    2761 src/shacl/mod.rs
    2076 src/storage/mod.rs
    1972 src/lib.rs
    1822 src/sparql/mod.rs
    1685 src/datalog/mod.rs
    1611 src/datalog/compiler.rs
    1430 src/sparql/expr.rs
    1335 src/export.rs
    1284 src/views.rs
    1259 src/sparql/federation.rs
    1141 src/sparql/embedding.rs
     914 src/schema.rs
     820 src/bulk_load.rs
     778 src/datalog/stratify.rs
     766 src/sparql/sqlgen.rs
     747 src/storage/merge.rs
     747 src/datalog/magic.rs
     733 src/datalog/parser.rs
     722 src/sparql/translate/filter/filter_expr.rs
     703 src/datalog_api.rs
     659 src/dictionary/mod.rs
     641 src/llm/mod.rs
     619 src/datalog/lattice.rs
     603 src/maintenance_api.rs
     582 src/sparql/property_path.rs
     548 src/shmem.rs
     545 src/sparql_api.rs
     518 src/dict_api.rs
     513 src/views_api.rs
     511 src/stats_admin.rs
```

### `unwrap()` / `expect()` counts

- `src/`: **30**
- `pg_ripple_http/src/`: **2**

### Dead-code annotations sample

80+ `#[allow(dead_code)]` occurrences. Whole-file annotations on:
- `src/telemetry.rs:1`
- `src/datalog/compiler.rs:1`
- `src/datalog/dred.rs:29`
- `src/sparql/federation_planner.rs:23`

### TODO / FIXME / HACK

Zero matches across `src/` and `pg_ripple_http/src/`.

### PT codes used (in source)

PT440, PT480, PT481, PT501, PT502, PT510, PT511, PT520, PT530, PT540, PT541, PT543, PT550, PT601, PT602, PT603, PT604, PT605, PT606, PT607, PT620, PT621, PT640, PT642, PT700, PT701, PT702, PT710, PT711, PT712, PT800. **(31 codes.)**

### PT codes documented

PT400-408, PT499, PT500-509, PT520, PT540-542, PT599, PT600-606, PT699, PT700-709, PT799. **(45 codes.)**

### Merge worker latch

```
93:    while BackgroundWorker::wait_latch(Some(Duration::from_secs(interval_secs))) {
137:            // v0.51.0: use wait_latch for correct SIGTERM response during backoff (S1-3).
138:            // If SIGTERM is received, wait_latch returns false and the outer while loop exits.
140:            if !BackgroundWorker::wait_latch(Some(Duration::from_secs(interval_secs))) {
```

### Dockerfile USER directives

```
Dockerfile:109:USER postgres
docker/Dockerfile.batteries:149:USER postgres
docker/Dockerfile.cnpg:76:FROM debian:bookworm-slim    (extension-only image; no process)
```

### `execute_with_savepoint` references

```
src/datalog/mod.rs:498:    // v0.51.0 (S3-4): parallel::execute_with_savepoint() is available for
src/datalog/parallel.rs:341:pub fn execute_with_savepoint(stmts: &[String], savepoint_name: &str) -> bool {
tests/pg_regress/sql/datalog_parallel_rollback.sql:7:-- 3. The SAVEPOINT rollback utility (execute_with_savepoint via parallel.rs)
```

### `max_path_depth` / `property_path_max_depth`

```
src/lib.rs:510:        c"pg_ripple.max_path_depth",
src/lib.rs:828:        c"pg_ripple.property_path_max_depth",
src/lib.rs:829:        c"DEPRECATED (v0.51.0): use pg_ripple.max_path_depth instead. Will be removed in v1.0.0.",
```

### HTAP merge cutover (race window)

```
src/storage/merge.rs:328:    Spi::run_with_args("SET LOCAL lock_timeout = '5s'", &[])
src/storage/merge.rs:332:    Spi::run_with_args(&format!("DROP TABLE IF EXISTS {main} CASCADE"), &[])
src/storage/merge.rs:336:    Spi::run_with_args(&format!("ALTER TABLE {main_new} RENAME TO vp_{pred_id}_main"), &[])
src/storage/merge.rs:343:    Spi::run_with_args(&format!("CREATE OR REPLACE VIEW {view} AS …"), &[])
```

### Federation SSRF surface

```
$ grep -n "federation_allowed_endpoints\|169.254\|127.0.0.1\|file://\|localhost" \
       src/sparql/federation.rs src/gucs/federation.rs
(no matches)
```

### Fuzz targets

```
fuzz/fuzz_targets/
├── datalog_parser.rs
├── dictionary_hash.rs
├── federation_result.rs
├── http_request.rs
├── jsonld_framer.rs
├── rdfxml_parser.rs
├── shacl_parser.rs
├── sparql_parser.rs
└── turtle_parser.rs
(9 targets)
```

### pg_regress test counts

- `tests/pg_regress/sql/`: **156** files
- `tests/pg_regress/expected/`: **157** files

### gucs split (closed)

```
src/gucs/datalog.rs       93 lines
src/gucs/federation.rs    60
src/gucs/llm.rs           62
src/gucs/mod.rs           33
src/gucs/observability.rs 20
src/gucs/shacl.rs          7
src/gucs/sparql.rs        64
src/gucs/storage.rs      108
                         ----
                         447 total (was 1,617 in src/gucs.rs)
```

### filter.rs split (closed)

```
src/sparql/translate/filter/filter_dispatch.rs  196
src/sparql/translate/filter/filter_expr.rs      722
src/sparql/translate/filter/mod.rs               25
                                               ----
                                                943 total (was 901 in filter.rs)
```

### SPARQL DoS limit (PT440) wiring

```
src/lib.rs:1482:        c"pg_ripple.sparql_max_algebra_depth",  default 256
src/lib.rs:1494:        c"pg_ripple.sparql_max_triple_patterns", default 4096
src/sparql/mod.rs:107:                "PT440: SPARQL algebra tree depth {} exceeds …
src/sparql/mod.rs:120:                "PT440: SPARQL query contains {} triple patterns …
```

### Migration script chain (verified continuous; samples)

```
sql/pg_ripple--0.50.0--0.51.0.sql
sql/pg_ripple--0.51.0--0.52.0.sql
sql/pg_ripple--0.52.0--0.53.0.sql
sql/pg_ripple--0.53.0--0.54.0.sql
(55 total scripts)
```

### `pg_ripple.control`

```
default_version = '0.54.0'
```

### AGENTS.md tech-stack table (closed)

```
**pg_ripple** is a PostgreSQL 18 extension written in Rust (pgrx 0.18) …
| PG binding | pgrx 0.18 (`pg18` feature flag) |
```

### Conformance gates in `.github/workflows/ci.yml`

```
394:  jena-suite:                continue-on-error: true   ← still informational
491:  watdiv-suite:              continue-on-error: false  ← BLOCKING (v0.53.0 promotion)
571:  lubm-suite:                                          ← required
699:  owl2rl-suite:              continue-on-error: true   ← informational despite 100 %
791:                             continue-on-error: true
917:                             continue-on-error: true
```

---

*End of report.*
