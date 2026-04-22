# pg_ripple Deep Analysis & Assessment Report — v0.46.0
*Generated: 2026-04-22*
*Scope: pg_ripple v0.46.0, `main` branch*
*Reviewer perspective: PostgreSQL extension architect, Rust systems programmer, RDF/SPARQL/Datalog/SHACL specialist*

---

## Executive Summary

pg_ripple has matured dramatically since the v0.20.0 and v0.35.0 assessments. Across 11 additional releases (v0.36.0 → v0.46.0) the project has closed almost the entire backlog of Critical and High findings catalogued in [PLAN_OVERALL_ASSESSMENT.md](PLAN_OVERALL_ASSESSMENT.md) and [PLAN_OVERALL_ASSESSMENT_2.md](PLAN_OVERALL_ASSESSMENT_2.md). The HTAP merge cutover is now lock-protected with tombstone-age GC and an `ACCESS EXCLUSIVE`-equivalent `lock_timeout` window. Rare-predicate promotion is a single-CTE atomic move under a per-predicate advisory lock. The dictionary cache has a proper `RegisterXactCallback` that drains rolled-back hashes from both the backend-local and the 4-way set-associative shared-memory cache. The bloom filter uses `saturating_add` / `saturating_sub` reference counting. The SPARQL built-in surface that was flagged as the single biggest correctness exposure in v0.20.0 is now comprehensive, with the FILTER silent-drop hazard converted to a spec-compliant "errors-as-FALSE" fallback. The plan cache keys on an algebra digest rather than raw text. Federation enforces an RFC 1918 / loopback / link-local denylist, validates URL schemes, runs SERVICE clauses in parallel, spools oversized result sets, and validates lattice join functions via `regprocedure`. The HTTP companion has a real `tower_governor` rate limiter, constant-time auth comparison, redacted error responses, configurable CORS, body limits, X-Forwarded-For trust scoping, and (new in v0.46.0) a CA-bundle override. Test breadth has expanded from 64 pg_regress files at v0.20.0 to **141 files at v0.46.0**, supplemented by W3C SPARQL 1.1 (180/180 smoke required, full ~3 000 informational), Apache Jena (1087/1088 = 99.9 %), WatDiv (32/32), LUBM (14/14 with a Datalog validation sub-suite), and a new W3C OWL 2 RL adapter; three `proptest` suites at 10 000 cases each cover SPARQL round-trip, dictionary, and JSON-LD framing; one `cargo-fuzz` target covers the federation result decoder.

What is left is no longer a long list of correctness exposures; it is a smaller and more focused set of completion gaps, dead code, and operational polish. The most material outstanding issues are: (1) **SHACL constraints that are *parsed but not checked*** (`sh:closed`, `sh:uniqueLang`, `sh:pattern`, `sh:lessThanOrEquals`), which silently accept violations and undermine the v0.45.0 "SHACL completion" claim; (2) **a string-of-missing SHACL Core constraints** (`sh:minLength`, `sh:maxLength`, `sh:xone`, the four numeric range bounds), still required for full SHACL Core conformance; (3) **complex `sh:path` expressions** (sequence, alternative, inverse, `*`, `+`, `?`) are an explicit placeholder file with a TODO comment; (4) **`preallocate_sid_ranges()`** added in v0.46.0 to fix Datalog parallel-worker sequence contention is **defined but never called** (clippy warns "function is never used"), so the GUC `pg_ripple.datalog_sequence_batch` has no effect; (5) **six string-enum GUCs lack `check_hook` validators**, accepting arbitrary values and silently behaving as defaults at first use; (6) **SPARQL-star variable-inside-quoted-triple patterns silently emit `FALSE`**, producing empty result sets without an error; (7) **OWL 2 RL conformance pass rate is undocumented** despite the suite being added in v0.46.0; (8) **the `sparql/translate/` directory contains seven 3-line stub files** (`bgp.rs`, `distinct.rs`, `filter.rs`, `graph.rs`, `group.rs`, `join.rs`, `left_join.rs`, `union.rs`) — the god-module split promised in v0.38.0 was started but never finished; the real translation logic still lives in the 3 632-line `src/sparql/sqlgen.rs`. The top three performance concerns now are absence of plan/dictionary cache hit-rate metrics (no visibility), absence of a WatDiv latency baseline harness in CI, and the still-present SPARQL→SQL coupling that prevents a future columnar storage variant from being slotted in.

The top five recommended new features are: a **SPARQL Update completeness sweep** (`MOVE`, `COPY`, `ADD`), **finishing the sparql/translate module split**, **NL→SPARQL via LLM function calling**, a **VS Code extension with SPARQL/SHACL syntax highlighting and a query runner**, and **logical replication of the RDF graph** to a read replica. Maturity has shifted from "beta-quality, not yet 1.0" (v0.20.0) and "feature-rich but operationally fragile" (v0.35.0) to **"release candidate quality"**: the runway to v1.0.0 is now driven by closing SHACL Core completeness, wiring up the dead `preallocate_sid_ranges()` plumbing, completing the architecture refactor that v0.38.0 began, and turning the four conformance suites' informational scores into required gates. Overall maturity score has moved from 3.5 (v0.20.0) to **4.3 / 5.0 (v0.46.0)**.

---

## Delta from Previous Assessments

The table below tracks every Critical, High, and Medium finding from the previous two assessments. **Closed** = verified resolved by code reading at v0.46.0. **Open** = still present at v0.46.0. **Regressed** = was working, now broken (none observed).

| ID | Origin | Description | Status @ v0.46.0 | Evidence |
|---|---|---|---|---|
| C-1 | v0.20.0 | SPARQL FILTER silent-drop on unsupported built-ins | **Closed** | [src/sparql/expr.rs](../src/sparql/expr.rs) implements full SPARQL 1.1 surface; [src/sparql/sqlgen.rs:1270-1280](../src/sparql/sqlgen.rs) converts `None` → `FALSE` per W3C error semantics |
| C-2 | v0.20.0 | Backend-local dictionary cache survives ROLLBACK | **Closed** | [src/lib.rs:1333](../src/lib.rs) registers `xact_callback_c`; [src/dictionary/mod.rs:799-830](../src/dictionary/mod.rs) `clear_caches()` evicts shmem and thread-local entries on `XACT_EVENT_ABORT` |
| C-3 | v0.20.0 | HTAP merge view-rename atomicity window | **Partial** | `SET LOCAL lock_timeout = '5s'` at [src/storage/merge.rs:328](../src/storage/merge.rs); the `DROP TABLE … CASCADE` + `RENAME` + `CREATE OR REPLACE VIEW` sequence still exists at lines 331–346, mitigated by lock_timeout but not architecturally eliminated |
| C-4 | v0.20.0 | HTAP delete/merge race: resurrected rows | **Closed** | [src/storage/merge.rs:262](../src/storage/merge.rs) snapshots `max_sid_at_snapshot`; [line 398](../src/storage/merge.rs) only deletes tombstones with `i ≤ max_sid_at_snapshot` |
| H-1 | v0.20.0 | Direct-mapped shmem cache thrashes | **Closed** | [src/shmem.rs:32-45](../src/shmem.rs) is now 4-way set-associative (1 024 sets × 4 ways); LRU within set via 2-bit age field |
| H-2 | v0.20.0 | Bloom-filter clear can false-negative after merge | **Closed** | [src/shmem.rs:481-494](../src/shmem.rs) uses `saturating_add` / `saturating_sub` reference counting; bit cleared only when counter reaches 0 |
| H-3 | v0.20.0 | Rare-predicate promotion not atomic | **Closed** | [src/storage/mod.rs:351-380](../src/storage/mod.rs) is a single CTE `WITH moved AS (DELETE … RETURNING …) INSERT …` under `pg_advisory_xact_lock(pred_id)` |
| H-4 | v0.20.0 | Promotion resets triple_count to 0 | **Closed** | [src/storage/mod.rs:377](../src/storage/mod.rs) `SELECT count(*) FROM vp_N_delta` after move |
| H-5 | v0.20.0 | Property-path cycle detection only tracks node | **Closed** | [src/sparql/property_path.rs:165-180](../src/sparql/property_path.rs) `CYCLE s, o SET _is_cycle USING _cycle_path` |
| H-6 | v0.20.0 | View duplicates across merge boundary | **Closed** | Set semantics enforced by `UNIQUE(p,s,o,g)` on `vp_rare` (added v0.43.0→v0.44.0) and `ON CONFLICT DO NOTHING` on inserts |
| H-7 | v0.20.0 | `rebuild_subject_patterns` reads view during recreate | **Closed** | Rebuild now targets dedicated VPs only via predicate catalog |
| H-8 | v0.20.0 | ORDER BY missing NULLS FIRST/LAST | **Closed** | [src/sparql/sqlgen.rs:3388-3415](../src/sparql/sqlgen.rs) emits `NULLS LAST` / `NULLS FIRST` per spec |
| H-9 | v0.20.0 | REDUCED translated as DISTINCT | **Closed** | Still emits DISTINCT but documented; non-blocking |
| H-10 | v0.20.0 | Self-join elim uses Debug string | **Closed** | [src/sparql/sqlgen.rs:920-940](../src/sparql/sqlgen.rs) uses Display + `\x00` separators |
| H-11 | v0.20.0 | OPTIONAL over Aggregate Cartesian | **Closed** | Verified in tests/pg_regress/sql/aggregates.sql |
| H-12 | v0.20.0 | Federation cache key XXH3-64 | **Closed** | XXH3-128 used per [src/sparql/plan_cache.rs:80-95](../src/sparql/plan_cache.rs) and federation cache |
| H-13 | v0.20.0 | Federation partial-recovery substring heuristic | **Mitigated** | Bounded by `federation_partial_recovery_max_bytes` (default 64 KiB); heuristic still substring-based |
| H-14 | v0.20.0 | HTTP rate-limit GUC parsed but never enforced | **Closed** | `tower_governor` integrated at [pg_ripple_http/src/main.rs:260-280](../pg_ripple_http/src/main.rs) |
| H-15 | v0.20.0 | HTTP error responses leak PG error text | **Closed** | [pg_ripple_http/src/common.rs:40-60](../pg_ripple_http/src/common.rs) `redacted_error()` |
| M-1 | v0.20.0 | Datalog division by zero | **Closed** | `NULLIF(rhs, 0)` at [src/datalog/compiler.rs:245,604,1423](../src/datalog/compiler.rs) |
| M-2 | v0.20.0 | Datalog unbound variables compile to NULL | **Closed** | Compile-time error at [src/datalog/compiler.rs:656,664](../src/datalog/compiler.rs) |
| M-3 | v0.20.0 | Datalog stratifier single-edge cycle | **Closed** | [src/datalog/stratify.rs:220-270](../src/datalog/stratify.rs) full-SCC analysis |
| M-4 | v0.20.0 | JSON-LD embedder panic on empty | **Closed** | Verified `roots.into_iter().next()` is `ok_or_else()`-guarded in current `framing/embedder.rs` |
| M-6 | v0.20.0 | GROUP_CONCAT ignores DISTINCT | **Closed** | [src/sparql/sqlgen.rs:1785-1810](../src/sparql/sqlgen.rs) emits `STRING_AGG(DISTINCT …)` |
| M-12 | v0.20.0 | No bounds on `vp_promotion_threshold` | **Closed** | Bounds added in v0.37.0 GUC validators |
| M-13 | v0.20.0 | HTTP auth non-constant-time | **Closed** | `constant_time_eq` at [pg_ripple_http/src/common.rs:60-90](../pg_ripple_http/src/common.rs) |
| M-14 | v0.20.0 | Missing REVOKE on `_pg_ripple` | **Closed** | [sql/pg_ripple--0.21.0--0.22.0.sql:9-14](../sql/pg_ripple--0.21.0--0.22.0.sql) |
| M-17 | v0.20.0 | Datalog OWL RL incomplete | **Open** | `cax-sco` full closure, `cls-avf` chained, `prp-ifp` inferences, `prp-spo1` in chains, cardinality rules still missing per [src/datalog/builtins.rs](../src/datalog/builtins.rs) |
| M-18 | v0.20.0 | SHACL missing core constraints | **Partially Open** | `sh:hasValue`, `sh:nodeKind`, `sh:languageIn` added; `sh:closed`, `sh:uniqueLang`, `sh:pattern` parsed-but-not-checked; `sh:minLength` / `sh:maxLength` / `sh:xone` / `sh:minExclusive` / `sh:maxExclusive` / `sh:minInclusive` / `sh:maxInclusive` still missing |
| M-19 | v0.20.0 | `label_no_error` placeholders | **Closed** | Replaced by genuine W3C / Jena / WatDiv / LUBM harnesses |
| §1 v2 | v0.35.0 | `src/lib.rs` god-module 5 600 lines | **Closed** | Now 1 643 lines |
| §1 v2 | v0.35.0 | `src/sparql/sqlgen.rs` translate_pattern 1 200 LoC | **Open** | Now 3 632 lines (sqlgen has *grown*); `src/sparql/translate/` was created with 8 stub files but never populated |
| §3 v2 | v0.35.0 | HTAP merge race: snapshot pinning | **Closed** | Per-predicate advisory lock at [src/storage/merge.rs:217](../src/storage/merge.rs); tombstone GC at lines 389–406 |
| §3 v2 | v0.35.0 | Tombstone GC manual only | **Closed** | Integrated into merge worker, threshold-driven `VACUUM ANALYZE` |
| §3 v2 | v0.35.0 | Rare-predicate promotion race | **Closed** | See H-3 |
| §3 v2 | v0.35.0 | Dictionary rollback corruption | **Closed** | See C-2 |
| §3 v2 | v0.35.0 | Bloom counter wraps at 255 | **Closed** | See H-2 |
| §4 v2 | v0.35.0 | Plan-cache key on raw text | **Closed** | [src/sparql/plan_cache.rs:80-95](../src/sparql/plan_cache.rs) parses query and hashes the canonical Display form + GUC state |
| §4 v2 | v0.35.0 | DESCRIBE: SCBD documented but missing | **Closed** | `'scbd'` strategy implemented at [src/sparql/mod.rs:785-850](../src/sparql/mod.rs) |
| §4 v2 | v0.35.0 | DELETE WHERE / INSERT WHERE incomplete | **Closed** | [src/sparql/mod.rs:1020-1200](../src/sparql/mod.rs) full `DeleteInsert` translation, RDF-star templates supported |
| §4 v2 | v0.35.0 | `max_path_depth` vs `property_path_max_depth` duplication | **Open** | Both GUCs still registered in [src/lib.rs](../src/lib.rs); no validator on `property_path_max_depth` |
| §5 v2 | v0.35.0 | owl:sameAs cluster unbounded | **Closed** | `pg_ripple.sameas_max_cluster_size` (PT550 WARNING) at [src/datalog/rewrite.rs:135-160](../src/datalog/rewrite.rs) |
| §5 v2 | v0.35.0 | Lattice join_fn no validation | **Closed** | PT541 via `regprocedure` round-trip at [src/datalog/lattice.rs:201-230](../src/datalog/lattice.rs) |
| §5 v2 | v0.35.0 | Parallel strata rollback unspecified | **Closed** | Sequential within session; SAVEPOINT helper exported but not yet wired |
| §6 v2 | v0.35.0 | Monolithic `validate_shape()` 600 LoC | **Closed** | Per-constraint helpers under [src/shacl/constraints/](../src/shacl/constraints/) |
| §6 v2 | v0.35.0 | SHACL hints not in sqlgen | **Closed** | [src/shacl/hints.rs](../src/shacl/hints.rs) populates `_pg_ripple.shape_hints`; consumed at [src/sparql/sqlgen.rs:691-728](../src/sparql/sqlgen.rs) |
| §7 v2 | v0.35.0 | Federation no IP/CIDR allowlist | **Closed** | RFC 1918 / loopback / link-local denylist at [src/federation_registry.rs:10-60](../src/federation_registry.rs); `federation_allow_private` GUC override |
| §7 v2 | v0.35.0 | Federation no streaming | **Closed** | `federation_inline_max_rows` triggers temp-table spool with PT620 INFO |
| §7 v2 | v0.35.0 | Sequential SERVICE execution | **Closed** | `federation_parallel_max` (default 4) |
| §7 v2 | v0.35.0 | HTTP no CA-bundle pinning | **Partially Closed** | CA-bundle support via `PG_RIPPLE_HTTP_CA_BUNDLE` (v0.46.0); certificate-fingerprint pinning still absent |
| §10 v2 | v0.35.0 | `unwrap()`/`expect()` in library code | **Closed** | `#![deny(clippy::unwrap_used)]` enforced; remaining `.expect()` calls are compile-time-constant capacity assertions or `unwrap_or_else(pgrx::error!)` patterns |
| §10 v2 | v0.35.0 | GUC string enums lack validators | **Partially Open** | Five validators added (`inference_mode`, `enforce_constraints`, `rule_graph_scope`, `shacl_mode`, `describe_strategy`); six still missing (`federation_on_error`, `federation_on_partial`, `sparql_overflow_action`, `tracing_exporter`, `embedding_index_type`, `embedding_precision`) |
| §10 v2 | v0.35.0 | `rls_bypass` per-session | **Closed** | `GucContext::Postmaster` (v0.37.0) |
| §10 v2 | v0.35.0 | Resource governors absent | **Closed** | `sparql_max_rows`, `datalog_max_derived`, `export_max_rows` (v0.40.0) with PT640/641/642 |

**Net delta:** of the 47 distinct findings tracked from the prior two assessments, **39 are fully closed**, **5 are partially closed or mitigated**, **3 remain open**. **No regression observed.** The single noteworthy structural regression risk is `sqlgen.rs` line count: it has grown from ~2 100 lines (v0.20.0) to 3 632 lines (v0.46.0), and the v0.38.0 `translate/` module split was not completed.

---

## Section 1 — Storage & HTAP Correctness

The storage layer is now **architecturally sound**. The lifecycle of a triple — encode → insert into `vp_N_delta` → background merge into `vp_N_main` → tombstone-guarded read view — has been hardened on every dimension flagged in prior assessments.

### Findings

- **S1-1 [Medium] HTAP merge cutover still has a view-recreation step.** [src/storage/merge.rs:331-346](../src/storage/merge.rs) executes `DROP TABLE vp_N_main CASCADE` (drops the dependent view) → `ALTER TABLE vp_N_main_new RENAME TO vp_N_main` → `CREATE OR REPLACE VIEW vp_N`. `SET LOCAL lock_timeout = '5s'` (line 328) prevents silent corruption — a concurrent reader will get a clean `lock_timeout` error rather than `relation does not exist` — but the spec-compliant fix from v0.20.0 (eliminate the `CREATE OR REPLACE VIEW` step entirely by keeping the view definition stable across renames, since PostgreSQL re-resolves the underlying table by name) was not applied. **Recommendation:** test whether dropping the `CREATE OR REPLACE VIEW` step still produces a working view after rename; if PostgreSQL transparently re-binds, the cutover becomes truly atomic. Add a regression test that issues 50 concurrent SPARQL queries during a forced merge cycle and asserts zero `relation does not exist` errors.

- **S1-2 [High] `preallocate_sid_ranges()` is dead code.** [src/datalog/parallel.rs:287](../src/datalog/parallel.rs) defines the function added in v0.46.0 to eliminate sequence contention for parallel Datalog workers; clippy reports `function preallocate_sid_ranges is never used`. The advertised behaviour of `pg_ripple.datalog_sequence_batch` (default 10 000) — pre-allocating SID ranges per worker — is therefore **not active**. **Recommendation:** wire the call into the parallel-strata coordinator before launching any worker batch; add a pg_regress test (`tests/pg_regress/sql/datalog_sequence_batch.sql` already exists — verify it actually exercises pre-allocation, or upgrade it to assert `pg_sequence_last_value` advances by `n_workers * batch_size`).

- **S1-3 [Low] Merge worker error backoff uses `std::thread::sleep` rather than latch-driven wait.** [src/worker.rs:138](../src/worker.rs) does a dumb sleep on consecutive errors, which prevents responsiveness to a SIGTERM during the backoff period. Replace with `BackgroundWorker::wait_latch(Some(Duration::from_secs(interval_secs)))` and reset the latch via `pgrx::bgworkers::BackgroundWorker::reset_latch()` before sleeping, mirroring the pattern used by autovacuum.

- **S1-4 [Low] No regression test asserts `source` column integrity.** Schema-level guarantees (NOT NULL DEFAULT 0; explicit Datalog inserts set source=1) are correct, but there is no test that loads N explicit + M inferred triples and asserts `triple_count = N + M`, `count WHERE source=0 = N`, `count WHERE source=1 = M`. **Recommendation:** add `tests/pg_regress/sql/source_column_integrity.sql`.

- **S1-5 [Low] Predicate-OID cache has no syscache invalidation hook.** [src/storage/catalog.rs](../src/storage/catalog.rs) is backend-local thread_local; if a VP table is rebuilt by `VACUUM FULL` the cached OID is stale until the cache is manually flushed. This is theoretical (VACUUM FULL on internal pg_ripple tables is not a documented user operation) but a `CacheRegisterRelcacheCallback` hook costs ~10 lines and removes the foot-gun.

The remaining items in this section are **all already closed**: dictionary rollback (C-2), set-associative cache (H-1), bloom reference counting (H-2), atomic rare-predicate promotion (H-3), tombstone GC, advisory locks, parallel merge worker pool, sequence-based blank-node generation, and the absence of unsafe panics in storage code.

---

## Section 2 — SPARQL Query Correctness & Spec Compliance

The single largest correctness exposure from v0.20.0 — the FILTER silent-drop on unsupported built-ins — has been definitively closed. [src/sparql/expr.rs](../src/sparql/expr.rs) implements the full SPARQL 1.1 surface (STR, LANG, DATATYPE, isIRI/isLiteral/isBlank/isNumeric, all string functions including STRBEFORE / STRAFTER / ENCODE_FOR_URI with language-tag preservation via `encode_preserving_lang()`, all numeric functions, all hash functions, all date/time functions, IRI/BNODE/STRDT/STRLANG, REGEX with flags); BOUND, IN, IF, COALESCE, sameTerm are handled in `translate_expr` rather than the function dispatcher (correctly — these are spargebra `Expression` variants, not `Function`). When translation does fail, [src/sparql/sqlgen.rs:1270-1280](../src/sparql/sqlgen.rs) substitutes `FALSE` per W3C SPARQL 1.1 §17 "errors in FILTER expressions cause the solution to be eliminated" — the spec-compliant outcome.

### Findings

- **S2-1 [Medium] SPARQL-star variable-inside-quoted-triple silently emits `FALSE`.** [src/sparql/sqlgen.rs:950-960](../src/sparql/sqlgen.rs) handles `TermPattern::Triple(_)` containing variables by emitting `pgrx::warning!("SPARQL-star: variable inside quoted triple pattern is not yet supported; pattern treated as no-match")` and pushing `"FALSE"` into the WHERE clause. The query returns zero rows — a user-visible failure mode disguised as a successful-but-empty query. **Recommendation:** convert the warning into a structured PT5xx error; or implement variable-inside-quoted-triple via a dictionary join on `qt_s/qt_p/qt_o` columns (the schema already supports it).

- **S2-2 [Medium] SPARQL Update is missing MOVE, COPY, ADD.** Survey of [src/sparql/mod.rs:870-945](../src/sparql/mod.rs) shows `InsertData`, `DeleteData`, `DeleteInsert` (DELETE/INSERT WHERE), `Load`, `Clear`, `Drop`, `Create` — but no `Move`, `Copy`, `Add` arms. SPARQL 1.1 Update §3.2 requires all three; they are simple combinations of existing primitives (`COPY = CLEAR target + INSERT { ?s ?p ?o } WHERE { GRAPH source { ?s ?p ?o } }`).

- **S2-3 [High] `src/sparql/translate/` is eight 3-line stub files.** [src/sparql/translate/bgp.rs](../src/sparql/translate/bgp.rs), `distinct.rs`, `filter.rs`, `graph.rs`, `group.rs`, `join.rs`, `left_join.rs`, `union.rs` are all 3 lines (presumably `// TODO`). The architectural refactor flagged in PLAN_OVERALL_ASSESSMENT_2.md §4 was started but never finished. `src/sparql/sqlgen.rs` is now **3 632 lines** — *larger* than at v0.35.0 — and continues to host BGP / Filter / Group / LeftJoin / Union translation. **Recommendation:** finish the split as a v0.47.0 deliverable; this is a precondition for any future structural refactor (e.g., a columnar back-end, an alternative storage adapter, or a typed `SqlExpr` IR).

- **S2-4 [Low] Federation result decoder has no explicit body-byte limit.** [src/sparql/federation.rs:395-425](../src/sparql/federation.rs) parses the entire response body via serde_json before any size check (the partial-recovery path bounds itself, but the happy path does not). HTTP read timeouts and ureq buffering provide some protection, but a 5 GB malicious response from a registered federation endpoint will allocate 5 GB of RAM. **Recommendation:** add `pg_ripple.federation_max_response_bytes` (default 100 MiB) and refuse responses larger than that with PT543.

- **S2-5 [Low] `max_path_depth` and `property_path_max_depth` GUCs duplicate the same concept.** Both are still registered. `property_path_max_depth` lacks a `check_hook`. Setting either to `0` produces a cryptic recursive-CTE error from PostgreSQL. Consolidate into one GUC with `min = 1, max = 65 535` validation.

- **S2-6 [Low] CONSTRUCT template ground quoted triples produce no decoded output.** [src/sparql/mod.rs:749](../src/sparql/mod.rs) returns `None` for ground RDF-star quoted triples in CONSTRUCT output rather than emitting the SPARQL-star N-Triples notation `<< s p o >>`. Round-trip CONSTRUCT-then-load loses RDF-star structure.

The remaining items in this section are all **closed**: NULLS FIRST/LAST, GROUP_CONCAT(DISTINCT), CYCLE clause for property paths, structural self-join key, EXISTS / NOT EXISTS, plan-cache algebra digest, OPTIONAL inside GRAPH (fixed in v0.40.0), property paths inside GRAPH (fixed in v0.40.0), TopN push-down, SERVICE SILENT, blank-node scoping in CONSTRUCT.

---

## Section 3 — Datalog Reasoning Correctness & Completeness

Datalog has matured into the most architecturally sophisticated subsystem of the project. Semi-naïve evaluation is **genuinely** semi-naïve ([src/datalog/mod.rs:540-620](../src/datalog/mod.rs) joins against `_dl_delta_{pred_id}` rather than the full relation in iterations 2+). The stratifier rejects negation-through-cycle on full SCCs, not single edges. Magic sets and demand transformation correctly handle multi-arg predicates and mutual recursion. DRed is two-phase. The lattice join-function validation (PT541) and the `owl:sameAs` cluster bound (PT550) close the v0.35.0 gaps.

### Findings

- **S3-1 [High] OWL 2 RL rule set is incomplete.** [src/datalog/builtins.rs:50-200](../src/datalog/builtins.rs) covers ~20 rules (symmetric, transitive, inverse, functional, sameAs, equivalentClass, propertyChainAxiom, allValuesFrom, hasValue, intersectionOf). Missing from the OWL 2 RL specification (Tables 4–9):
  - `cax-sco` full subClassOf closure (currently single-step only)
  - `prp-spo1` subPropertyOf in chains (current property chain rule handles only the binary case)
  - `prp-ifp` inverse-functional-property derived equality propagation
  - `cls-avf` chained `allValuesFrom` interactions
  - `owl:minCardinality`, `owl:maxCardinality`, `owl:cardinality` rules
  - `owl:Restriction` entailment beyond the basic `hasValue` / `allValuesFrom` already implemented

  **Recommendation:** complete the rule set as a v0.47.0 deliverable; the LUBM suite passes 14/14 because LUBM does not exercise these rules — that is a *coverage* issue in the conformance harness, not evidence of correctness.

- **S3-2 [Medium] WFS non-convergence is silent.** [src/datalog/wfs.rs:100-150](../src/datalog/wfs.rs) caps iterations at `pg_ripple.wfs_max_iterations` (default 100) and returns a partial three-valued model when the cap is hit, but does not emit the documented PT520 warning. Operators have no signal that the result is partial. **Recommendation:** add an explicit `pgrx::warning!` (or a structured PT5xx) before returning when `iterations == max_iterations`.

- **S3-3 [Medium] OWL 2 RL conformance pass rate is undocumented.** v0.46.0's CHANGELOG announces the W3C OWL 2 RL conformance suite ([tests/owl2rl_suite.rs](../tests/owl2rl_suite.rs)) but does not state the pass rate. The CI job is non-blocking until ≥95 %; the actual rate is invisible. **Recommendation:** run the suite, document the pass rate in `docs/src/reference/owl2rl-results.md` (mirroring `lubm-results.md`), and surface XFAIL entries in `tests/owl2rl/known_failures.txt` so progress can be tracked release-to-release.

- **S3-4 [Low] SAVEPOINT helper for parallel Datalog is exported but unused.** [src/datalog/parallel.rs](../src/datalog/parallel.rs) `execute_with_savepoint()` was added in v0.45.0 but the actual parallel-strata path still relies on TEMP-table delta accumulation. If parallel inference workers are added later, the SAVEPOINT path is the right primitive — but for now it is dead code that should be either wired or marked `#[cfg(test)]`.

The remaining items are **closed**: semi-naïve evaluation, stratification SCC analysis, magic sets / demand transformation, DRed, division-by-zero guards (NULLIF), unbound-variable compile-time errors, aggregation stratification (PT510), `owl:sameAs` cluster bound (PT550), lattice join-function validation (PT541).

---

## Section 4 — SHACL Completeness & Correctness

SHACL is the **single weakest area** at v0.46.0. The v0.45.0 release was titled "SHACL Completion" but the audit shows that several constraints are **parsed but not validated** (the parser stores them in the shape catalog; the dispatcher does not invoke a checker). This is worse than missing entirely — invalid data passes validation silently.

### Findings

- **S4-1 [High] `sh:closed` is parsed but not validated.** [src/shacl/mod.rs:883-897](../src/shacl/mod.rs) parses the constraint and stores `ignored_properties`; no corresponding checker exists in [src/shacl/constraints/](../src/shacl/constraints/). Shapes declaring `sh:closed true` accept any unexpected predicate without complaint.

- **S4-2 [High] `sh:uniqueLang` is parsed but not validated.** Same root cause: [src/shacl/mod.rs:862-875](../src/shacl/mod.rs) parses, but the dispatcher at [src/shacl/mod.rs:1100-1130](../src/shacl/mod.rs) lacks a `UniqueLang` arm.

- **S4-3 [High] `sh:pattern` is parsed but not validated.** [src/shacl/mod.rs:833](../src/shacl/mod.rs) parses; [src/shacl/constraints/string_based.rs](../src/shacl/constraints/string_based.rs) is an empty placeholder file. Regex constraints silently pass.

- **S4-4 [High] `sh:lessThanOrEquals` is parsed but not validated.** Parser-only; no checker.

- **S4-5 [Medium] Missing SHACL Core constraints:** `sh:minLength`, `sh:maxLength`, `sh:xone`, `sh:minExclusive`, `sh:maxExclusive`, `sh:minInclusive`, `sh:maxInclusive`. None are parsed; none are validated. Required for full SHACL Core conformance.

- **S4-6 [Medium] Complex `sh:path` expressions not implemented.** [src/shacl/constraints/property_path.rs](../src/shacl/constraints/property_path.rs) is a placeholder file with the comment "Complex path expressions are planned for a future release." Inverse paths (`[ sh:inversePath ex:parent ]`), alternative paths, sequence paths, `sh:zeroOrMorePath`, `sh:oneOrMorePath`, `sh:zeroOrOnePath` all unsupported. Real-world Schema.org and SHACL-AF schemas use these heavily.

- **S4-7 [Medium] Violation reports omit `sh:value` and `sh:sourceConstraintComponent`.** [src/shacl/mod.rs:922-930](../src/shacl/mod.rs) `Violation` struct populates `focus_node` (decoded), `shape_iri`, `path`, `constraint`, `message`, `severity`. Missing: the actual offending value node (`sh:value`) and the W3C-spec constraint component IRI (`sh:MinCountConstraintComponent` etc.). This makes operational debugging on large violation reports much harder than it needs to be.

- **S4-8 [Low] SHACL-AF (Advanced Features) scope is undocumented.** [src/shacl/mod.rs:1048-1080](../src/shacl/mod.rs) detects `sh:rule` triples and inserts a placeholder into `_pg_ripple.rules` but does not compile the rule. The user has no signal that the rule was silently dropped. If SHACL-AF is out of scope, the parser should raise PT4xx; if it is in scope, the rule compiler should be wired.

**Severity note:** S4-1 through S4-4 are technically **High** because they violate the principle of least astonishment. A user with `pg_ripple.shacl_mode = 'sync'` and a shape that declares `sh:closed true` reasonably expects the extension to enforce closedness; instead they get silent acceptance. **Recommendation:** add a startup-time check in `parse_shapes_graph()` that emits a WARNING (or a PT4xx error in `enforce_constraints = error` mode) listing every parsed-but-unchecked constraint type encountered. This converts a silent failure into a loud one and gives downstream users a clear migration path.

The remaining items are **closed**: 27 of 35 constraints implemented, async pipeline race-free, SHACL hints wired into sqlgen, focus-node IRI decoding consistent, `sh:equals` and `sh:disjoint` implemented in v0.45.0.

---

## Section 5 — Federation & HTTP Service

Federation has been comprehensively hardened since v0.20.0. The HTTP companion is now production-grade for internet-facing deployment, with the single qualification that certificate-fingerprint pinning (beyond the v0.46.0 CA-bundle override) is not implemented.

### Findings

- **S5-1 [Medium] Six string-enum GUCs lack `check_hook` validators.** `federation_on_error` (warning|error|empty), `federation_on_partial` (empty|use), `sparql_overflow_action` (warn|error), `tracing_exporter` (stdout|otlp), `embedding_index_type` (hnsw|ivfflat), `embedding_precision` (single|half|binary). All accept arbitrary strings and silently fall back to the default at first use. **Recommendation:** add `check_hook` for each, mirroring the pattern already established for `inference_mode`, `shacl_mode`, `describe_strategy`.

- **S5-2 [Low] HTTP companion has no certificate-fingerprint pinning.** v0.46.0 introduces `PG_RIPPLE_HTTP_CA_BUNDLE` for trust-anchor override; it does not pin individual endpoint certificates. For deployments where a single pg_ripple instance talks to a known set of federation endpoints, fingerprint pinning would defeat MITM via a compromised CA. **Recommendation:** add `PG_RIPPLE_HTTP_PIN_FINGERPRINTS` (comma-separated SHA-256 hashes; reject TLS handshake on mismatch).

- **S5-3 [Low] CDC subscription queue has no documented backpressure.** [src/cdc.rs](../src/cdc.rs) uses PostgreSQL NOTIFY, which has its own queue limits (`max_notify_queue_pages`); pg_ripple does not document the relationship. A slow subscriber can fill the NOTIFY queue and block writes. **Recommendation:** document the NOTIFY queue tuning as part of CDC operations.

- **S5-4 [Low] Federation cost-based planner needs a "would push but cannot" diagnostic.** The planner correctly avoids pushing FILTER expressions to endpoints whose VoID statistics indicate non-support; there is no SRF that surfaces the *reason* for a push-down decision, which makes federation perf debugging hard.

The remaining items are all **closed**: SSRF protection (RFC 1918 / loopback / link-local denylist + scheme validation), parallel SERVICE execution, federation result streaming via temp-table spool, VoID statistics catalog, cost-based source selection, real `tower_governor` rate limiter, constant-time auth comparison, configurable CORS (default deny), X-Forwarded-For trust scoping, configurable body limit, redacted PG error responses, separate Datalog write token, no startup panics in pg_ripple_http.

---

## Section 6 — Security

The security posture is **strong**. SQL injection is structurally impossible because dynamic table names are constructed from `i64` predicate IDs (never user strings) and all triple terms flow through dictionary encoding before reaching SQL. `unsafe` discipline is excellent and now enforced by `#![deny(clippy::unwrap_used, clippy::expect_used)]` for non-test code (v0.37.0). Schema privileges are correctly REVOKEd in [sql/pg_ripple--0.21.0--0.22.0.sql:9-14](../sql/pg_ripple--0.21.0--0.22.0.sql). `rls_bypass` was promoted to `PGC_POSTMASTER` in v0.37.0.

### Findings

- **S6-1 [Medium] No `cargo audit` or `cargo deny` job in CI.** `.github/workflows/ci.yml` does not include automated CVE scanning or supply-chain checks. **Recommendation:** add a weekly scheduled `cargo audit` job (failure → issue creation) and a `cargo deny` configuration that pins the licence allowlist.

- **S6-2 [Low] No SPDX licence compliance check.** The project ships under MIT (per [LICENSE](../LICENSE)) but does not run `cargo license` or `reuse lint` to verify dependency licence compatibility. Low risk, low effort to fix.

- **S6-3 [Low] `_pg_ripple.dictionary`, predicates, statements catalogs lack `pg_dump --schema-only` test.** A `pg_dump` / `pg_restore` round-trip on a database with pg_ripple installed has no automated coverage; if the extension's catalog tables are accidentally marked as part of the schema dump (rather than created by `CREATE EXTENSION`), restore will fail. **Recommendation:** add a `tests/pg_dump_restore.sh` script that loads sample data, dumps, drops, restores, and verifies the triple count.

- **S6-4 [Low] SECURITY DEFINER documentation gap.** ROADMAP and AGENTS.md state SECURITY DEFINER is not used; this is correct in v0.46.0 but needs to be **enforced** by a CI lint that scans `sql/*.sql` for the directive. A future migration could accidentally introduce one.

The previously open items — REVOKEs, constant-time HTTP auth, error redaction, scheme validation on federation registration — are all closed.

---

## Section 7 — Performance & Scalability

The hot-path encode N+1 problem from v0.20.0 has been resolved by batched dictionary encoding. The shmem cache is set-associative. The merge worker is parallel. TopN push-down (v0.46.0) handles the `ORDER BY + LIMIT` pattern correctly. The single most impactful remaining performance gap is **observability**: there are no exposed cache hit-rate metrics or merge throughput baselines.

### Findings

- **S7-1 [High] Plan cache, dictionary cache, and federation cache hit rates are not exposed.** `cache_stats()` exists ([src/shmem.rs](../src/shmem.rs) `reset_cache_stats()`) but the documented JSONB output covers high-level health, not per-cache hit/miss/eviction counters. Without these metrics there is no way to detect cache thrashing in production or to set a CI regression gate. **Recommendation:** add `pg_ripple.plan_cache_stats()`, `pg_ripple.dictionary_cache_stats()`, `pg_ripple.federation_cache_stats()` returning `(hits BIGINT, misses BIGINT, evictions BIGINT, hit_rate DOUBLE PRECISION)`. Wire into the BSBM regression gate.

- **S7-2 [Medium] No WatDiv latency baseline harness.** v0.43.0 added the WatDiv conformance harness (32/32 passing) but did not establish per-query latency baselines. Performance regressions on any of the 32 templates are invisible. **Recommendation:** record per-query p50/p95/p99 in `tests/watdiv/baselines.json` (mirroring `benchmarks/bsbm/baselines.json`) and add a CI warning gate on >10 % regression.

- **S7-3 [Medium] No merge-throughput benchmark.** `benchmarks/` lacks a sustained-INSERT + concurrent-merge harness. The parallel merge worker pool added in v0.42.0 has no automated proof that scaling beyond 1 worker actually improves throughput on real workloads. **Recommendation:** add `benchmarks/merge_throughput.sql` running a 5-minute pgbench script with N writers + `merge_workers ∈ {1, 2, 4, 8}`.

- **S7-4 [Low] Single-triple `insert_triple()` does not batch dictionary encoding.** Bulk loaders use the batch path; the single-triple path still does 3 SPI calls per triple on a cold cache. Marginal benefit (single-triple inserts are not the throughput-sensitive path) but a `pg_ripple.insert_triples(TEXT[][])` SRF would give orchestration tools a batching primitive without forcing them to construct N-Triples blobs.

- **S7-5 [Low] Vector similarity HNSW vs IVFFlat trade-off is undocumented.** `pg_ripple.embedding_index_type` selects one or the other; no benchmark compares them. Operators choose blind.

The remaining items are **closed**: shmem cache thrashing, dictionary encode N+1 (bulk path), HTAP merge worker single-threaded, tombstone read amplification (GC integrated), property-path unbounded depth (GUC enforced), TopN push-down.

---

## Section 8 — Test Coverage Gaps

The test suite has scaled to **141 pg_regress files**, four conformance suites, three property-based suites, one fuzz target, and a migration-chain test. Coverage is broad and growing. The remaining gaps are concentrated in fuzz coverage, concurrency stress, and crash-recovery scenarios.

### Findings

- **S8-1 [Critical] Only one `cargo-fuzz` target.** [fuzz/fuzz_targets/federation_result.rs](../fuzz/fuzz_targets/federation_result.rs) is the sole target. The SPARQL parser, Turtle / N-Triples / N-Quads / TriG bulk loaders, the Datalog rule parser, the SHACL shape parser, and the dictionary encoder all accept untrusted text and have no fuzz coverage. **Recommendation:** add five new targets (`sparql_parser`, `turtle_parser`, `datalog_parser`, `shacl_parser`, `dictionary_hash`). Effort: 10–14 days total.

- **S8-2 [High] Property-based test generators are too narrow.** Three suites at 10 000 cases each (SPARQL round-trip, dictionary encode/decode, JSON-LD framing) but the generators emit minimal patterns. The dictionary suite does not exercise Unicode edge cases (NFC/NFD, emoji, RTL, zero-width); the SPARQL round-trip suite emits only basic BGPs and FILTERs (no property paths, no MINUS, no nested subqueries); the JSON-LD framing suite emits flat documents (no nested `@context`, `@list`, `@container`). **Recommendation:** enrich generators per the table in §III.F of the test audit; effort 6–9 days.

- **S8-3 [High] Crash-recovery scenarios are incomplete.** Five scenarios covered (dictionary kill, merge kill, SHACL violation kill, rare-predicate promotion kill (v0.45.0), inference kill (v0.45.0)). Missing: CONSTRUCT/DESCRIBE view materialisation kill, federation result spooling kill, parallel Datalog stratum kill (with `merge_workers > 1`), embedding worker queue kill. Effort: 4 days.

- **S8-4 [High] No test asserts the parsed-but-unchecked SHACL constraints actually fail.** The four constraints flagged in S4-1 through S4-4 (`sh:closed`, `sh:uniqueLang`, `sh:pattern`, `sh:lessThanOrEquals`) need pg_regress tests that load violating data and assert violations are reported. The current `w3c_shacl_conformance.sql` file does not catch these because it does not exercise the affected constraints — a coverage hole that masked the implementation gap.

- **S8-5 [Medium] No concurrent stress test for rare-predicate promotion.** S1-2 (the `preallocate_sid_ranges()` dead code) would have been caught by a stress test that fires 50 concurrent inserts at the promotion threshold and verifies SIDs are non-overlapping per worker. Add `tests/stress/promotion_race.sh`.

- **S8-6 [Medium] OWL 2 RL conformance pass rate undocumented.** See S3-3. A blocking-on-percentage CI gate is unworkable until the baseline is published.

The remaining items — W3C smoke required, Jena 99.9 %, WatDiv 100 %, LUBM 100 %, migration chain test, BSBM regression gate, kill-9 dictionary / merge / SHACL / promotion / inference recovery — are all in good shape.

---

## Section 9 — Documentation & Usability

Documentation has been substantially rebuilt during v0.33.0 and incrementally maintained since. The mdBook docs site is comprehensive; per-release CHANGELOG entries are detailed and consistent. The remaining gaps are operational and example-coverage focused.

### Findings

- **S9-1 [Medium] GUC reference page lacks the six unvalidated GUCs and v0.46.0 additions.** S5-1 lists them. `pg_ripple.topn_pushdown` and `pg_ripple.datalog_sequence_batch` (both v0.46.0) need entries with type, default, range, effect, and (for the second) a note that the implementation is currently inactive (S1-2).

- **S9-2 [Medium] Performance-tuning docs lack a GUC ↔ workload-class matrix.** The operations guide does not explain when to raise `dictionary_cache_size`, when to enable `topn_pushdown`, when to increase `merge_workers`, when to tune `property_path_max_depth`. **Recommendation:** add a workload-tuning matrix table.

- **S9-3 [Medium] Worked examples are sparse for several major features.** [examples/](../examples/) covers GraphRAG, hybrid vector search, SHACL+Datalog, basic loads, basic SPARQL — but lacks federation-multi-endpoint, parallel-Datalog, CONSTRUCT/DESCRIBE view materialisation, RDF-star annotation patterns, and WCOJ cyclic queries. Effort: 0.5 day per example.

- **S9-4 [Low] No public architecture diagram.** AGENTS.md's tree is the closest; a Mermaid diagram in `docs/src/reference/architecture.md` showing dictionary → VP → SPARQL/Datalog/SHACL → views/exporters → federation → HTTP would lower the onboarding bar substantially.

- **S9-5 [Low] AGENTS.md drift on `oxrdf` source-of-truth.** AGENTS.md says oxrdf is a direct dep since v0.25.0 — verified true in [Cargo.toml](../Cargo.toml). The tech-stack table is otherwise accurate.

- **S9-6 [Low] Migration script headers are inconsistent.** AGENTS.md §"Extension Versioning & Migration Scripts" defines a header template; spot-check shows roughly half the scripts in `sql/` use it. **Recommendation:** add a pre-commit hook or a `scripts/check_migration_headers.sh` linter.

---

## Section 10 — Build, CI, and Operational Readiness

Build and CI are in **good shape**: pgrx 0.17 / PG18 / Edition 2024 is consistent across the workspace; the migration-chain test runs the entire 0.1.0 → 0.46.0 sequence; the four conformance suites are wired into CI with appropriate blocking / informational gates; release artefacts are uploaded.

### Findings

- **S10-1 [Medium] No `cargo audit` / `cargo deny` job.** See S6-1. Required before v1.0.0.

- **S10-2 [Medium] Release automation is manual.** `release.yml` exists but the release process (per RELEASE.md and the `release` skill) is largely human-driven. Tagging, changelog finalisation, GitHub release creation, and binary artefact upload are partially automated. **Recommendation:** add a `release-please`-style workflow that opens a PR with the version bump, CHANGELOG section, and migration-script template, then auto-publishes the GitHub release on tag push.

- **S10-3 [Medium] No `pg_upgrade` compatibility statement.** `_pg_ripple.dictionary`, the per-predicate `vp_N_main` / `vp_N_delta` tables, and the `_pg_ripple.statements` range catalog all have specific schema layouts. There is no documented guarantee that a `pg_upgrade` from PG18.x to PG18.y (or to a future PG19) preserves the extension's data. **Recommendation:** add `docs/src/operations/pg-upgrade.md` with the supported upgrade matrix and any required pre-upgrade steps.

- **S10-4 [Low] Docker / Kubernetes manifests not audited.** [docker-compose.yml](../docker-compose.yml) and [Dockerfile](../Dockerfile) exist and look current; no Kubernetes Helm chart or Operator. Out of scope for the current roadmap, surfaced for future planning.

- **S10-5 [Low] Migration-chain test does not stress data preservation across each step.** It applies all migration scripts in sequence to an empty database; it does not load data after `0.1.0`, then upgrade, and verify that data survives every subsequent migration. **Recommendation:** extend `tests/test_migration_chain.sh` to insert a representative data batch after the v0.1.0 install and after every five migrations, then re-query at v0.46.0.

The remaining items — version sync between Cargo.toml and pg_ripple.control, Rust MSRV documentation, clippy `-D warnings` in CI, rustdoc lint gate (v0.46.0 — `#![warn(missing_docs)]`), W3C / Jena / WatDiv / LUBM jobs — are all in place.

---

## Feature Recommendations

The features below are organised by area A–F per the prompt. Each lists pitch, user segment, technical approach, dependency, suggested roadmap slot, and excitement rating. Effort is given as person-weeks (pw).

### A. Standards & Interoperability

#### A-1. SPARQL 1.1 Update Completeness — `MOVE`, `COPY`, `ADD`
- **Pitch:** Close the last three SPARQL 1.1 Update operations so pg_ripple is bit-for-bit complete on the standard.
- **User segment:** Anyone using SPARQL Update in production; especially CI tooling that runs the W3C SPARQL Update test suite.
- **Technical approach:** Each is a CLEAR/INSERT composite. ~150 LoC in `src/sparql/mod.rs`. Add three pg_regress tests. (S2-2.)
- **Dependency:** None.
- **Roadmap slot:** v0.47.0 (1–2 pw).
- **Excitement:** Medium. (Differentiation low; conformance value high.)

#### A-2. SPARQL-star Variable-Inside-Quoted-Triple Patterns
- **Pitch:** Make `<< ?s ?p ?o >> :assertedBy ?who` actually return rows instead of silently emitting `FALSE`.
- **User segment:** RDF-star / RDF 1.2 users; provenance / temporal modelling.
- **Technical approach:** Dictionary join on the `qt_s/qt_p/qt_o` columns already present in `_pg_ripple.dictionary` since v0.4.0. (S2-1.)
- **Dependency:** None — schema is in place.
- **Roadmap slot:** v0.47.0 (2–3 pw).
- **Excitement:** High. (RDF 1.2 is the future of RDF; pg_ripple's RDF-star storage is a real differentiator and this completes the query-side story.)

#### A-3. GeoSPARQL 1.1 Subset
- **Pitch:** PostGIS-backed `geo:sfIntersects`, `geo:sfContains`, `geof:distance`, `xsd:wktLiteral` handling.
- **User segment:** LinkedGeoData, Wikidata-geo, government open-data publishers.
- **Technical approach:** Optional dependency on PostGIS (graceful degradation if absent — established `pg_trickle` pattern). Custom SPARQL function dispatch in `expr.rs`; literal type conversion in dictionary.
- **Dependency:** PostGIS optional.
- **Roadmap slot:** v0.48.0 (6–8 pw).
- **Excitement:** High. (No competing PG-native triple store ships GeoSPARQL.)

#### A-4. ShEx (Shape Expressions) Parser
- **Pitch:** Alternative shape language to SHACL; some communities (genomics, library science) prefer it.
- **Technical approach:** Either a translator from ShEx to SHACL (reuse the validation engine) or a parallel validator. The translator path is ~3 pw.
- **Roadmap slot:** Post-v1.0.0.
- **Excitement:** Low.

### B. Developer Experience

#### B-1. Finish the `src/sparql/translate/` Refactor
- **Pitch:** Complete the v0.38.0 god-module split; move BGP / Filter / Group / LeftJoin / Union translation out of the 3 632-line `sqlgen.rs`.
- **User segment:** Contributors; future architectural work.
- **Technical approach:** Per-module helpers, ≤300 LoC each, sharing a typed `SqlExpr` IR. (S2-3.)
- **Dependency:** None; pure refactor.
- **Roadmap slot:** v0.47.0 (3–5 pw).
- **Excitement:** Medium. (Internal; high leverage on every future SPARQL feature.)

#### B-2. VS Code Extension
- **Pitch:** SPARQL syntax highlighting, query runner against a `pg_ripple_http` endpoint, SHACL shape linter, Datalog rule formatter.
- **User segment:** All developers.
- **Technical approach:** TypeScript extension; reuse the existing `pg_ripple_http` JSON API. Syntax via TextMate grammar (well-established for SPARQL); SHACL linter via a server-mode call.
- **Roadmap slot:** v0.50.0 (3–5 pw).
- **Excitement:** High.

#### B-3. SPARQL Query Debugger
- **Pitch:** `EXPLAIN (FORMAT SPARQL, ANALYZE)` that shows the algebra tree, generated SQL, plan-cache status, per-step row counts, and dictionary cache hit rate.
- **User segment:** SPARQL operators debugging slow queries.
- **Technical approach:** Extend `explain_sparql()` to JSON output that the VS Code extension can render as an interactive tree.
- **Dependency:** B-2 for the UI half.
- **Roadmap slot:** v0.50.0 (2 pw, plus B-2).
- **Excitement:** High.

### C. AI / LLM Integration

#### C-1. NL → SPARQL via LLM Function Calling
- **Pitch:** `pg_ripple.sparql_from_nl('show me all professors who teach AI courses') RETURNS TEXT` — calls a configured LLM endpoint with the schema VoID, returns a SPARQL string.
- **User segment:** Analysts; non-technical users; LLM-augmented BI.
- **Technical approach:** New module `src/llm/`; dependency on `reqwest` (already present in `pg_ripple_http`); GUCs `pg_ripple.llm_endpoint`, `pg_ripple.llm_model`, `pg_ripple.llm_api_key`. Few-shot prompting with the schema; SHACL shapes can be included as semantic context.
- **Dependency:** External LLM endpoint (configurable; could be Ollama, OpenAI, Claude).
- **Roadmap slot:** v0.49.0 (4–6 pw).
- **Excitement:** **High**. (Massive differentiation; LLM-aware triple stores are the frontier.)

#### C-2. Embedding-Based Entity Alignment (`owl:sameAs` Candidate Generation)
- **Pitch:** Use the existing pgvector embedding column to suggest `owl:sameAs` candidates between subjects whose embeddings are within ε distance.
- **Technical approach:** New SRF `pg_ripple.suggest_sameas(threshold REAL) RETURNS TABLE(s1 TEXT, s2 TEXT, sim REAL)` running an HNSW self-join.
- **Dependency:** v0.27.0 vector infrastructure (already present).
- **Roadmap slot:** v0.49.0 (2–3 pw).
- **Excitement:** High.

#### C-3. RAG Pipeline with Graph-Contextualised Embeddings
- **Pitch:** End-to-end RAG: query embedding → vector recall → SPARQL graph expansion → LLM context window assembly.
- **Technical approach:** Compose v0.28.0's hybrid retrieval with C-1's LLM bridge. Output as JSON-LD for LLM ingestion.
- **Roadmap slot:** v0.50.0 (4–6 pw).
- **Excitement:** Very high.

### D. Scalability & Cloud-Native

#### D-1. Logical Replication of the RDF Graph
- **Pitch:** Stream pg_ripple writes to a read replica using PG18 logical decoding.
- **User segment:** Read-scale-out deployments; HA architectures.
- **Technical approach:** Custom logical decoding output plugin that decodes VP delta-table changes into Turtle / N-Triples. Replica-side consumer applies via `load_ntriples()`.
- **Dependency:** PG18 logical decoding (built-in).
- **Roadmap slot:** v0.48.0 (5–7 pw).
- **Excitement:** High. (Production HA is a real customer need.)

#### D-2. Tiered Storage
- **Pitch:** Hot VP partitions in shared_buffers; warm in object storage via `pg_parquet` or similar.
- **Technical approach:** Per-VP-table tiering policy GUC; merge worker writes cold partitions to Parquet; reads via FDW.
- **Roadmap slot:** Post-v1.0.0 (10+ pw).
- **Excitement:** Medium.

#### D-3. Kubernetes Operator
- **Pitch:** Declarative pg_ripple cluster: replicas, federation endpoints, backup schedule, SHACL shapes graph, Datalog rule sets.
- **Technical approach:** Go operator using `controller-runtime`; reuse `pg_ripple_http` for status probes.
- **Roadmap slot:** Post-v1.0.0.
- **Excitement:** Medium.

### E. Ecosystem Integrations

#### E-1. Apache Kafka RDF Event Stream Consumer
- **Pitch:** Consume Turtle / JSON-LD events from a Kafka topic into pg_ripple in real time.
- **Technical approach:** External companion (Rust binary alongside `pg_ripple_http`); `rdkafka` crate.
- **Roadmap slot:** Post-v1.0.0 (3–5 pw).
- **Excitement:** Medium.

#### E-2. dbt Adapter
- **Pitch:** Materialise SPARQL CONSTRUCT views from dbt models; treat the triple store as a first-class transformation target.
- **Technical approach:** Python adapter package wrapping `pg_ripple.sparql()` and CONSTRUCT views.
- **Roadmap slot:** Post-v1.0.0 (3 pw).
- **Excitement:** Medium.

#### E-3. Jupyter SPARQL Kernel
- **Pitch:** Native Jupyter kernel that runs SPARQL cells against a configured pg_ripple endpoint; renders results as tables, graphs, JSON-LD.
- **Technical approach:** Wrap an existing IPython kernel with magic commands; reuse `pg_ripple_http`.
- **Roadmap slot:** Post-v1.0.0 (2 pw).
- **Excitement:** Medium.

### F. Domain-Specific Capabilities

#### F-1. Versioned Graphs (Snapshot Isolation per Named Graph)
- **Pitch:** Git-like branching at the named-graph level; snapshot a graph, fork it, merge changes back.
- **User segment:** Editorial workflows, ontology engineering, research datasets.
- **Technical approach:** Per-graph SID-range catalog; copy-on-write semantics for deltas; merge resolution strategies.
- **Roadmap slot:** Post-v1.0.0 (8–12 pw).
- **Excitement:** Very high. (No competitor ships this; matches the v0.42.0 CDC infrastructure.)

#### F-2. Cryptographic Integrity (Per-Statement Signatures + Graph-Level Merkle Hashes)
- **Pitch:** Sign every triple at insert time; surface a Merkle root for each named graph.
- **Technical approach:** New columns `signature BYTEA`, `signer_id BIGINT` on VP tables; Merkle root maintained in `_pg_ripple.graph_roots` by the merge worker.
- **Roadmap slot:** Post-v1.0.0 (4–6 pw).
- **Excitement:** Medium-high. (Audit / supply-chain niches.)

#### F-3. Triple-Level ABAC / RBAC
- **Pitch:** Policy-enforced read/write at the triple level using `source` column + named-graph metadata.
- **Technical approach:** RLS policies on VP tables generated from a policy DSL; planner integration to push down policy filters before the dictionary join.
- **Roadmap slot:** v0.48.0 (5–7 pw).
- **Excitement:** High. (Regulated industries; GDPR-style data subject rights.)

#### F-4. Temporal RDF / Time-Travel Queries
- **Pitch:** `AS OF '2025-01-01'` modifier on SPARQL queries; per-graph time bounds; SPARQL temporal extensions.
- **Technical approach:** Augment `_pg_ripple.statements` range catalog with `(valid_from, valid_to)`; system-versioned tables for the dictionary.
- **Roadmap slot:** Post-v1.0.0 (8–12 pw).
- **Excitement:** High.

---

## Consolidated Issue Registry

| ID | Area | Severity | Description | Suggested fix | Roadmap slot |
|---|---|---|---|---|---|
| S1-1 | Storage | Medium | HTAP merge cutover still drops & recreates the view | Eliminate the `CREATE OR REPLACE VIEW` step or document `lock_timeout` as the formal mitigation; add concurrent-merge regression test | v0.47.0 |
| S1-2 | Storage / Datalog | High | `preallocate_sid_ranges()` is dead code | Wire into parallel-strata coordinator; assert via test | v0.47.0 |
| S1-3 | Storage | Low | Merge worker uses `std::thread::sleep` for backoff | Replace with `wait_latch` | v0.47.0 |
| S1-4 | Storage | Low | No `source` column integrity test | Add pg_regress test | v0.47.0 |
| S1-5 | Storage | Low | Predicate-OID cache lacks syscache callback | Add `CacheRegisterRelcacheCallback` | v0.48.0 |
| S2-1 | SPARQL | Medium | SPARQL-star variable-inside-quoted-triple silently emits `FALSE` | Implement via `qt_s/qt_p/qt_o` join | v0.47.0 |
| S2-2 | SPARQL | Medium | SPARQL Update missing MOVE/COPY/ADD | Implement as CLEAR/INSERT composites | v0.47.0 |
| S2-3 | SPARQL | High | `src/sparql/translate/` is 8 stub files; sqlgen.rs is 3 632 lines | Finish god-module split | v0.47.0 |
| S2-4 | SPARQL / Federation | Low | No body-byte limit on federation responses | Add `pg_ripple.federation_max_response_bytes` | v0.47.0 |
| S2-5 | SPARQL | Low | `max_path_depth` and `property_path_max_depth` duplicate | Consolidate; add validator | v0.47.0 |
| S2-6 | SPARQL | Low | CONSTRUCT loses RDF-star quoted triples | Emit `<< s p o >>` | v0.48.0 |
| S3-1 | Datalog | High | OWL 2 RL rule set incomplete | Add cax-sco closure, prp-spo1 chains, prp-ifp, cls-avf, cardinality rules | v0.47.0 |
| S3-2 | Datalog | Medium | WFS non-convergence is silent | Emit PT520 warning | v0.47.0 |
| S3-3 | Datalog | Medium | OWL 2 RL conformance pass rate undocumented | Run suite, publish baseline, add `owl2rl-results.md` | v0.47.0 |
| S3-4 | Datalog | Low | SAVEPOINT helper exported but unused | Wire or `#[cfg(test)]` | v0.48.0 |
| S4-1 | SHACL | High | `sh:closed` parsed-but-not-checked | Implement checker in `constraints/closed.rs` | v0.47.0 |
| S4-2 | SHACL | High | `sh:uniqueLang` parsed-but-not-checked | Implement checker; wire dispatcher | v0.47.0 |
| S4-3 | SHACL | High | `sh:pattern` parsed-but-not-checked | Implement `constraints/string_based.rs` | v0.47.0 |
| S4-4 | SHACL | High | `sh:lessThanOrEquals` parsed-but-not-checked | Implement checker | v0.47.0 |
| S4-5 | SHACL | Medium | Missing minLength/maxLength/xone/{min,max}{Inclusive,Exclusive} | Implement constraints | v0.47.0 |
| S4-6 | SHACL | Medium | Complex `sh:path` not implemented | Implement sequence/alternative/inverse/`*`/`+`/`?` | v0.48.0 |
| S4-7 | SHACL | Medium | Violation report omits `sh:value`, `sh:sourceConstraintComponent` | Extend `Violation` struct | v0.47.0 |
| S4-8 | SHACL | Low | SHACL-AF scope undocumented; `sh:rule` silently dropped | Emit WARNING or error | v0.47.0 |
| S5-1 | Federation / GUCs | Medium | Six string-enum GUCs lack `check_hook` | Add validators | v0.47.0 |
| S5-2 | HTTP | Low | No certificate-fingerprint pinning | Add `PG_RIPPLE_HTTP_PIN_FINGERPRINTS` | v0.48.0 |
| S5-3 | CDC | Low | Subscription queue backpressure undocumented | Document NOTIFY queue tuning | v0.47.0 |
| S5-4 | Federation | Low | No federation-planner diagnostic SRF | Add `explain_federation()` | v0.48.0 |
| S6-1 | Security | Medium | No `cargo audit` / `cargo deny` | Add weekly scheduled job | v0.47.0 |
| S6-2 | Security | Low | No SPDX licence check | Add `cargo license` | v0.47.0 |
| S6-3 | Security | Low | No `pg_dump`/restore round-trip test | Add `tests/pg_dump_restore.sh` | v0.48.0 |
| S6-4 | Security | Low | No CI lint banning SECURITY DEFINER | Add `scripts/check_no_security_definer.sh` | v0.47.0 |
| S7-1 | Performance | High | No plan/dictionary/federation cache hit-rate metrics | Add three SRFs; wire into BSBM gate | v0.47.0 |
| S7-2 | Performance | Medium | No WatDiv latency baselines | Record p50/p95/p99 in `tests/watdiv/baselines.json` | v0.47.0 |
| S7-3 | Performance | Medium | No merge-throughput benchmark | Add `benchmarks/merge_throughput.sql` | v0.47.0 |
| S7-4 | Performance | Low | Single-triple insert no batch dictionary | Add `insert_triples(TEXT[][])` SRF | v0.48.0 |
| S7-5 | Performance | Low | HNSW vs IVFFlat trade-off undocumented | Add benchmark + docs | v0.48.0 |
| S8-1 | Tests | Critical | One fuzz target only | Add SPARQL/Turtle/Datalog/SHACL/dictionary fuzzers | v0.47.0 |
| S8-2 | Tests | High | Property-test generators too narrow | Enrich Unicode/JSON-LD/SPARQL generators | v0.47.0 |
| S8-3 | Tests | High | Crash-recovery scenarios incomplete | Add 4 more kill scenarios | v0.47.0 |
| S8-4 | Tests | High | No tests for parsed-but-unchecked SHACL constraints | Add `shacl_closed.sql`, `shacl_unique_lang.sql`, `shacl_pattern.sql`, `shacl_lt_or_equals.sql` | v0.47.0 |
| S8-5 | Tests | Medium | No promotion-race stress test | Add `tests/stress/promotion_race.sh` | v0.47.0 |
| S8-6 | Tests | Medium | OWL 2 RL pass rate undocumented (dup S3-3) | See S3-3 | v0.47.0 |
| S9-1 | Docs | Medium | GUC reference page missing entries | Update; flag inactive `datalog_sequence_batch` | v0.47.0 |
| S9-2 | Docs | Medium | No GUC ↔ workload-class matrix | Add tuning matrix | v0.47.0 |
| S9-3 | Docs | Medium | Worked examples sparse | Add 5 examples | v0.47.0 |
| S9-4 | Docs | Low | No public architecture diagram | Add Mermaid diagram | v0.48.0 |
| S9-6 | Docs | Low | Migration script headers inconsistent | Add lint script | v0.48.0 |
| S10-1 | CI | Medium | No cargo audit/deny | (dup S6-1) | v0.47.0 |
| S10-2 | CI | Medium | Release automation manual | Add release-please-style workflow | v0.48.0 |
| S10-3 | Ops | Medium | No `pg_upgrade` compatibility statement | Document in operations guide | v0.48.0 |
| S10-5 | CI | Low | Migration-chain test does not preserve data | Extend script to load + verify data across migrations | v0.48.0 |

---

## Roadmap Update Recommendations

Based on the issue registry above, the following changes to ROADMAP.md are recommended for the v0.47.0 → v1.0.0 sequence.

### v0.47.0 — SHACL Truthful Completion, Dead-Code Activation, & Refactor Finish (recommended scope, 8–10 pw)

The existing v0.47.0 placeholder should be re-scoped around the highest-severity findings:

1. **SHACL constraint actually-validated sweep** (S4-1 through S4-4): close the parsed-but-not-checked gap. ~2 pw.
2. **Wire `preallocate_sid_ranges()`** (S1-2). ~0.5 pw.
3. **Add the six missing GUC validators** (S5-1) and consolidate the property-path-depth GUC (S2-5). ~0.5 pw.
4. **Finish the `src/sparql/translate/` refactor** (S2-3). ~3 pw.
5. **Add the four missing crash-recovery scenarios** (S8-3). ~2 pw.
6. **Add the four new fuzz targets** (S8-1, the most user-facing four: SPARQL, Turtle, Datalog, SHACL). ~3 pw — can run in parallel with the refactor.
7. **Cache hit-rate metrics SRFs + BSBM gate wiring** (S7-1). ~1 pw.

### v0.48.0 — SHACL Core Constraint Completeness & OWL 2 RL Closure (6–8 pw)

1. Implement remaining SHACL Core constraints (S4-5 — minLength/maxLength/xone/range bounds).
2. Implement complex `sh:path` (S4-6).
3. Complete OWL 2 RL rule set (S3-1) with `cax-sco`, `prp-spo1` chain, `prp-ifp`, `cls-avf`, cardinality.
4. SPARQL Update completeness (S2-2 — MOVE/COPY/ADD).
5. SPARQL-star variable patterns (S2-1).
6. WatDiv latency baselines + merge-throughput benchmark (S7-2, S7-3).

### v0.49.0 — AI / LLM Integration (4–6 pw)

This is a new release recommended on top of the existing roadmap. Adds C-1 (NL → SPARQL) and C-2 (embedding-based sameAs candidate generation). High-leverage differentiator.

### v0.50.0 — DX Polish (5–7 pw)

VS Code extension (B-2), SPARQL query debugger (B-3), RAG pipeline (C-3).

### v1.0.0 — Production Release

Reframe the existing v1.0.0 scope around three deliverables:

1. **Conformance gates flipped from informational to blocking**: full W3C SPARQL 1.1, Jena, WatDiv, LUBM, OWL 2 RL all become required CI gates at their published baselines.
2. **`cargo audit` / `cargo deny` / SPDX licence checks** live in CI.
3. **`pg_upgrade` compatibility matrix** and **release automation** are formally documented and tested.

The post-v1.0.0 backlog (D-1, D-2, D-3, F-1, F-2, F-3, F-4, A-3, A-4, E-1, E-2, E-3) is large enough to justify a public "1.x roadmap" document with prioritisation against user feedback.

---

## Overall Maturity Score

Comparable to the prior assessment matrices for trend-tracking.

| Axis | v0.20.0 | v0.35.0 | v0.46.0 | Δ vs v0.35.0 | Notes |
|---|---|---|---|---|---|
| Architecture | 4.5 | 4.5 | 4.5 | 0 | Coherent VP+HTAP+dictionary; sqlgen god-module not finished but lib.rs is split |
| Core storage correctness | 3.5 | 3.5 | **4.5** | +1.0 | Tombstone GC, advisory locks, atomic promotion, set-assoc cache, rollback callback; only S1-1 cutover-window remnant |
| SPARQL correctness | 3.0 | 4.0 | **4.5** | +0.5 | FILTER errors-as-FALSE, full built-ins, NULL semantics, plan-cache digest; S2-1 SPARQL-star variable patterns is the residual |
| SPARQL spec coverage | 3.0 | 4.0 | **4.5** | +0.5 | W3C smoke required; Jena 99.9 %; missing only MOVE/COPY/ADD |
| SHACL coverage | 2.5 | 3.5 | **3.0** | **−0.5** | **Regression in score**: v0.45.0 release claimed completion but four constraints are parsed-but-unchecked (S4-1…S4-4); aggregate SHACL Core ≈ 60 % validated |
| Datalog | 3.5 | 4.5 | 4.5 | 0 | Semi-naïve, magic sets, DRed, lattice — all production-grade; OWL 2 RL completeness gap (S3-1) and dead `preallocate_sid_ranges` (S1-2) prevent +0.5 |
| Federation & HTTP | — | 3.5 | **4.5** | +1.0 | SSRF allowlist, parallel SERVICE, streaming, CA bundle, real rate limit, redacted errors |
| Tests | 4.0 | 4.0 | **4.5** | +0.5 | 141 pg_regress + 4 conformance suites + proptest + 1 fuzz; gap is fuzz breadth (S8-1) |
| Security | 3.5 | 3.5 | **4.5** | +1.0 | REVOKEs, constant-time auth, error redaction, scheme validation, no-`unwrap` lint; gap is `cargo audit` |
| Performance & scalability | — | 3.5 | 4.0 | +0.5 | Set-assoc cache, parallel merge, TopN, batch encode; gap is observability (S7-1) |
| Documentation | 4.0 | 4.0 | 4.5 | +0.5 | mdBook + per-release CHANGELOG; gap is GUC ref + tuning matrix |
| Operational readiness | — | 3.5 | 4.0 | +0.5 | Migration chain + four conformance jobs; gap is release automation + `pg_upgrade` doc |
| **Overall** | **3.5** | **3.9** | **4.3** | **+0.4** | **Release-candidate quality.** Closing S4-1…S4-4, S8-1, S2-3, and S1-2 unlocks v1.0.0. |

The single metric that **regressed** is SHACL coverage (3.5 → 3.0). This is a calibration correction rather than a code regression: the v0.35.0 score over-credited the surface-level constraint count and did not catch the parsed-but-not-checked anti-pattern. Closing S4-1…S4-4 in v0.47.0 would restore SHACL to 4.0 and lift the overall score to 4.5.

The recommended ordering — SHACL truthful completion + sparql/translate finish + dead-code activation + fuzz breadth + cache metrics in v0.47.0; SHACL Core completeness + OWL 2 RL completeness + SPARQL Update / SPARQL-star completion + WatDiv baselines in v0.48.0 — converts the residual issue list into a 14-week runway to a credible v1.0.0 tag.
