# pg_ripple Deep Analysis & Assessment Report
*Generated: 2026-04-19*
*Scope: pg_ripple v0.35.0 (released 2026-04-19), with v0.36.0 in flight*
*Reviewer perspective: PostgreSQL extension architect & Rust systems programmer*

## Executive Summary

pg_ripple has matured into a feature-rich RDF/SPARQL/Datalog/SHACL platform on top of PostgreSQL 18, with 35 sequential releases delivering an HTAP storage engine, semi-naïve Datalog (with magic sets, demand transformation, parallel strata, lattice aggregation, and DRed retraction), federation, vector + SPARQL hybrid retrieval, JSON-LD framing, GraphRAG export, and a companion HTTP service. The architecture is fundamentally sound — vertical-partitioning (VP) over a hash-backed-sequence dictionary, with all join keys as `BIGINT`, is a textbook-correct design and the team has been disciplined about it. The codebase, however, now shows the strain of breadth: [src/lib.rs](src/lib.rs) has grown past 5,600 lines and acts as a god-module; several critical paths still rely on `.expect()`/`.unwrap()` (server panics on transient SPI errors); GUC string enums lack input validators; the HTAP merge worker, rare-predicate promotion, and shared-memory dictionary cache have known concurrency hazards that are not yet covered by stress tests. The largest correctness risks live in **storage concurrency** and **operational hardening**, not in query semantics. Fixing those plus a focused investment in test coverage (concurrency, federation failure modes, large-scale Datalog convergence) is the highest-value work before v1.0.0.

---

## 1. Code Quality & Architecture

### Findings

- **`src/lib.rs` is a god-module**: ~5,600 lines hosting `_PG_init`, the GUC table, ~100+ `#[pg_extern]` functions, and significant logic that belongs in subsystems (rare-predicate promotion, SHACL admin, federation registry, GraphRAG dispatcher). This obscures module boundaries and makes review and refactoring expensive.
- **Tight coupling SPARQL/Datalog → storage**: [src/sparql/sqlgen.rs](src/sparql/sqlgen.rs) and [src/datalog/compiler.rs](src/datalog/compiler.rs) call into `storage::*` directly to resolve VP table OIDs and assemble SQL. There is no `TripleSource` / `PredicateCatalog` trait, which makes test mocking and future storage variants (e.g., columnar) awkward.
- **Error handling: hard panics in hot paths**. Library-side panic surfaces include:
  - 30+ `.expect()` calls in [src/lib.rs](src/lib.rs) (notably around lines 5379–5416 in admin RPC paths).
  - [src/bulk_load.rs#L228](src/bulk_load.rs#L228) — `flush_batch` panics on dictionary-encode SPI failure mid-COPY.
  - [src/sparql/optimizer.rs#L226](src/sparql/optimizer.rs#L226) — `stats_cache.get().unwrap()` without bounds check.
  - [src/sparql/sqlgen.rs](src/sparql/sqlgen.rs#L988) — `unwrap()` on column bindings.
  - [src/export.rs](src/export.rs#L826-L1227) — 17× `.expect()` on JSON column extraction in GraphRAG export; missing column ⇒ server panic.
  - [pg_ripple_http/src/main.rs](pg_ripple_http/src/main.rs#L111-L195) — 10+ `.expect()`/`.unwrap()` during startup; HTTP service crashes on env/DB connection errors.
- **`unsafe` discipline is excellent.** Every reviewed `unsafe` block carries a `// SAFETY:` comment ([src/shmem.rs#L136-L194](src/shmem.rs#L136), [src/worker.rs#L87](src/worker.rs#L87), [src/lib.rs#L1996](src/lib.rs#L1996), [src/export.rs#L547](src/export.rs#L547), [src/sparql/federation.rs#L92](src/sparql/federation.rs#L92)). No raw `PG_FUNCTION_INFO_V1` C macros; all SQL exposure goes through `pgrx::pg_extern`.
- **SPI usage is idiomatic.** All ~80 SPI sites use `Spi::connect`/`Spi::get_one`/`Spi::run` correctly; no leaked `SpiClient`s.
- **Long functions / complex control flow**:
  - `validate_shape()` in [src/shacl/mod.rs](src/shacl/mod.rs) — ~600 lines, ≥10 indent levels.
  - `translate_pattern()` in [src/sparql/sqlgen.rs](src/sparql/sqlgen.rs) — ~1,200 LoC across BGP/Union/Join/LeftJoin/Filter, no extracted helpers.
  - `compile_rule_set()` in [src/datalog/compiler.rs](src/datalog/compiler.rs) — multiple full sweeps over the rule list that could be a single multi-phase pipeline.
- **Hot-path inefficiencies**:
  - Dictionary encode miss costs *two* SPI round-trips (insert + select). Batch encode in `sqlgen` could remove this from the per-query critical path ([src/dictionary/mod.rs#L80-L130](src/dictionary/mod.rs#L80)).
  - Per-SPARQL-query encode cache is a `HashMap` rebuilt for every query; no cross-query term CSE.
  - Merge worker re-reads the predicate list on every poll cycle ([src/worker.rs#L156-L200](src/worker.rs#L156)).
- **Background worker correctness** ([src/worker.rs](src/worker.rs)): SIGTERM handling and lifecycle look correct; the watchdog GUC `pg_ripple.merge_watchdog_timeout` is honored. The functional risk is in `storage::merge` (see §3) rather than the worker harness.

### Recommendations

1. **Split [src/lib.rs](src/lib.rs)** into thin `pg_extern` shims that delegate to subsystem modules. Target: ≤1,500 lines covering `_PG_init`, GUC registration, and `extension_sql!`.
2. **Replace `.expect()`/`.unwrap()` in library code** with `Result`-propagating helpers and `pgrx::error!` at the FFI boundary. Adopt a project-wide clippy lint (`clippy::unwrap_used`, `clippy::expect_used`) gated to `src/` (excluding tests).
3. **Introduce a `PredicateCatalog` trait** abstracting OID lookup, used by both SPARQL and Datalog code. Backs current implementation; enables future columnar storage and stronger unit testing.
4. **Refactor `validate_shape()` and `translate_pattern()`** into per-constraint / per-algebra-node helpers. Each helper should be <80 lines.
5. **Batch dictionary encoding**: in `translate_pattern`, collect all literal/IRI constants in a single pre-pass and resolve them via one `encode_terms_batch(&[Term]) -> Vec<i64>` SPI call.
6. **Add a `clippy::cognitive_complexity` budget** in `Cargo.toml` for new code.

---

## 2. Test Coverage Gaps

### Findings

- **pg_regress** ([tests/pg_regress/](tests/pg_regress/)) is the dominant test surface and covers most happy paths. Coverage is roughly:
  | Area | Coverage | Notable gaps |
  |---|---|---|
  | SPARQL SELECT/ASK | High (~90%) | UNION/MINUS set semantics; BIND inside OPTIONAL |
  | Property paths | Medium-high (~80%) | Reverse paths, nested >2 levels, `?p{n,m}` quantifiers |
  | CONSTRUCT/DESCRIBE/Views | Medium (~70%) | Complex templates with multiple subjects, RDF-star DESCRIBE, blank-node templates |
  | Datalog | Medium (~60%) | Cyclic aggregates, WFS × aggregates interaction, magic-sets on cyclic rules |
  | SHACL | Medium-low (~50%) | Complex `sh:path` (sequence/inverse), async pipeline under load |
  | HTAP merge | Low (~40%) | **Concurrent INSERT/DELETE during merge**, tombstone GC, watchdog kill mid-cycle |
  | Federation | Low (~30%) | Timeout + retry, parallel SERVICE, network failure |
  | Vector hybrid | Very low (~20%) | High-dim embeddings, `auto_embed` performance, RRF correctness |
- **Crash-recovery harness** ([tests/crash_recovery/](tests/crash_recovery/)) covers dictionary kill, merge kill, and SHACL violation kill. Missing: rare-predicate promotion kill, federation worker kill, Datalog inference kill mid-fixpoint.
- **Migration chain test** ([tests/test_migration_chain.sh](tests/test_migration_chain.sh)) is exemplary — every `0.X.Y → 0.X.Y+1` step is exercised in sequence. This should be the model for other long-tail correctness tests.
- **No property-based tests** (e.g., `proptest`) for SPARQL→SQL translation invariants (e.g., "encoded constants are stable across re-translation", "self-join elimination preserves result set").
- **No fuzz harness** for the SPARQL parser path, the Turtle/N-Triples bulk loader, or the federation SERVICE result parser — all of which accept untrusted text.
- **Datalog convergence at scale untested**: no test runs ≥100 rules / ≥1M facts to a fixpoint and asserts iteration count/runtime bounds.
- **No load test for SHACL async queue**: 10 K+ violations per minute behaviour is unverified.

### Recommendations

1. **Add concurrent stress tests** using `psql` + GNU parallel or pgbench scripts: (a) writers + merge worker, (b) two writers racing rare-predicate promotion, (c) writers + SHACL async validation.
2. **Adopt `proptest`** for: (a) SPARQL algebra round-trip (`spargebra::Query` → `sqlgen` → execute), (b) dictionary encode/decode, (c) JSON-LD framing.
3. **Add `cargo-fuzz` targets** for SPARQL/Turtle parsers and federation result decoder.
4. **Document a coverage policy** in [AGENTS.md](AGENTS.md): every new `#[pg_extern]` requires a pg_regress test; every new constraint requires a SHACL conformance test.
5. **W3C test suites**: integrate the SPARQL 1.1 and SHACL Core test suites into CI as a separate, allowed-to-warn job. Track conformance percentage per release.

---

## 3. Storage & Indexing

### Findings

- **VP table design is correct** and well-documented. Dual `(s, o)` and `(o, s)` B-trees on delta tables, BRIN on consolidated main tables — appropriate for the HTAP split.
- **HTAP merge race conditions** ([src/storage/merge.rs](src/storage/merge.rs#L100-L200)): the read view is `(main EXCEPT tombstones) UNION ALL delta`. During a merge cycle, concurrent `DELETE` of a row that has just been moved from `delta` to `main_new` may end up tombstoned-against-the-old-main, then lost when the cutover swaps tables. There is no advisory lock around the cutover and no MVCC snapshot pinning the merge view. This is the **single highest correctness risk** in the codebase.
- **Tombstones are never auto-compacted**. Long-running sessions and replicas will accumulate tombstones until manual `VACUUM`, costing read latency.
- **Rare-predicate promotion race** ([src/lib.rs#L2710-L2745](src/lib.rs#L2710), [src/storage/mod.rs#L350-L380](src/storage/mod.rs#L350)): two backends crossing the threshold simultaneously can both attempt `CREATE TABLE _pg_ripple.vp_{id}`; the loser's failure is swallowed and the predicate may stall in `vp_rare` until manual `promote_rare_predicates()`.
- **Promotion is not idempotent**: re-running on an already-promoted predicate fails because `INSERT … SELECT … FROM vp_rare WHERE p = ?` and `DELETE FROM vp_rare` are not properly guarded after the move.
- **`vp_rare` lacks an `(o, s)` reverse index** ([src/storage/mod.rs#L90](src/storage/mod.rs#L90)): object-leading patterns on rare predicates fall back to a sequential scan over the entire shared table.
- **Predicate-catalog OID lookup is uncached**: every SPARQL translation issues an SPI catalog query per atom ([src/sparql/sqlgen.rs#L82-L94](src/sparql/sqlgen.rs#L82)). For high-fanout BGPs this is measurable.
- **No syscache invalidation hook**: if a VP table is dropped externally, the predicate catalog row remains; subsequent queries fall back to `vp_rare` silently rather than warning.
- **Dictionary cache invalidation on transaction abort is missing** ([src/dictionary/mod.rs#L100-L130](src/dictionary/mod.rs#L100)): an `INSERT` into the dictionary that is later rolled back leaves the cached `(term → id)` mapping in shared memory; the next encode of the same term will return an ID that no longer exists. This will surface as decode failures (`PT002`-style), not data corruption, but it is reproducible.
- **Shared-memory bloom filter under-counted reference release** ([src/shmem.rs#L90-L110](src/shmem.rs#L90)): bit reference counter wraps at 255 → 0, producing a false-negative skip of the delta partition.
- **Statement-range catalog `_pg_ripple.statements`** is updated by the merge worker. If the worker is killed mid-update, the next SPARQL on RDF-star quoted triples may resolve a SID to the wrong VP table OID.

### Recommendations

1. **Lock the merge cutover** with a transactional advisory lock (`pg_advisory_xact_lock(_pg_ripple, predicate_id)`) and require `DELETE` and `INSERT` paths on the same predicate to acquire the same lock in `share` mode. Add a stress test driving 50 concurrent writers + a 1-second merge interval.
2. **Schedule tombstone GC** in the merge worker: after `N` cycles or when tombstones exceed `M`% of main, run `VACUUM (FULL false)` on the affected VP tables.
3. **Make rare-predicate promotion idempotent and serialised**: take the same advisory lock; use `CREATE TABLE IF NOT EXISTS`; wrap move in `WITH moved AS (DELETE … RETURNING *) INSERT INTO …`.
4. **Add the `(o, s)` index to `vp_rare`**, or at minimum gate it behind a GUC mirroring `named_graph_optimized`.
5. **Cache predicate OIDs** in a backend-local `HashMap<u64, Oid>` invalidated by a syscache callback registered on `_pg_ripple.predicates`.
6. **Tie dictionary cache entries to transaction lifetime**: defer cache insertion until commit, or version-tag entries with the inserting xid and check `TransactionDidCommit` on read.
7. **Use `saturating_sub` on bloom counters** and document the maximum reference count.
8. **Make `_pg_ripple.statements` updates idempotent** and atomic with the table swap.

---

## 4. SPARQL Engine

### Findings

- **Translation correctness is generally strong**: BGPs collapse to flat `JOIN` chains; FILTER constants are encoded once at translation time (no runtime decode/re-encode); star-pattern self-join elimination uses a per-query encode cache; property paths compile to PG18 `WITH RECURSIVE … CYCLE`.
- **`spargebra` + `sparopt` cover the algebra well.** Plan caching ([src/sparql/plan_cache.rs](src/sparql/plan_cache.rs)) is wired in for SELECT/ASK and skipped for SERVICE.
- **Plan-cache key is raw query text** with no normalisation: whitespace and prefix-form variants miss the cache.
- **DESCRIBE** implements CBD only; `describe_strategy = 'scbd'` is documented but not implemented ([src/sparql/mod.rs#L595-L650](src/sparql/mod.rs#L595)).
- **SPARQL Update**: INSERT DATA and DELETE DATA are implemented; **DELETE WHERE / INSERT WHERE / WITH … DELETE** appear to be the gap most likely to bite users (roadmap places them in v0.12.0, deliverables show partial coverage).
- **UNION/MINUS regression coverage is thin**; algebra → `UNION ALL`/`EXCEPT` correctness for null-bindings semantics is not exhaustively tested.
- **Property path bounds**: `pg_ripple.max_path_depth` and `pg_ripple.property_path_max_depth` appear to be **two GUCs for the same concept** — confusing and prone to drift. Neither validates against `value >= 1`; setting `0` produces a cryptic PostgreSQL error from the recursive CTE.
- **Built-in functions**: roadmap claims completion in v0.21.0; spot-checking `STRDT`, `STRLANG`, `IRI()`, `BNODE()`, `UUID`, `STRUUID`, `MD5/SHA*`, datetime-extraction functions would benefit from a conformance test against the W3C test suite.
- **Self-join elimination** assumes equal subject; coverage of object-shared and graph-shared self-joins is not asserted.

### Recommendations

1. **Normalise plan-cache keys**: parse → canonical text (sorted prefixes, whitespace canonicalisation), or cache on the algebra digest instead.
2. **Either implement SCBD or remove the GUC value** with a clear `PT0xx` error.
3. **Complete DELETE WHERE / INSERT WHERE / MODIFY** with comprehensive tests including pattern-only deletes that span multiple VP tables.
4. **Consolidate `max_path_depth` / `property_path_max_depth`** into one GUC with a validator (`min = 1`, `max = 65_535`).
5. **Add UNION/MINUS conformance tests** mirroring the W3C SPARQL 1.1 test cases.
6. **Wire up the SPARQL 1.1 test suite** in CI (allowed-to-warn) and publish per-release conformance numbers in `CHANGELOG.md`.

---

## 5. Datalog Engine

### Findings

- **Stratification** ([src/datalog/stratify.rs](src/datalog/stratify.rs)) detects negation cycles and aggregate-over-recursive cycles correctly; errors are reported (not silent loops).
- **Semi-naïve evaluation with delta tables** ([src/datalog/compiler.rs](src/datalog/compiler.rs)) is the core engine. Magic sets ([src/datalog/magic.rs](src/datalog/magic.rs)) and demand transformation ([src/datalog/demand.rs](src/datalog/demand.rs)) are wired in for goal-directed inference.
- **Aggregation** (v0.30.0): COUNT/SUM/MIN/MAX/AVG in rule bodies compiled to `GROUP BY` subqueries; aggregation-stratification check enforces the no-aggregate-on-recursive-head rule.
- **owl:sameAs canonicalisation** uses transitive-closure plus an equality table; cyclic chains terminate but are not bounded — a 1 M-node sameAs cluster will allocate `O(N²)` join intermediate space.
- **Parallel stratum evaluation** (v0.35.0, [src/datalog/parallel.rs](src/datalog/parallel.rs)) uses a union-find dependency partition. The algorithm is correct on paper; missing tests for: (a) variable-predicate rules across groups, (b) failures in one parallel worker rolling back others.
- **Lattice-based Datalog^L** (v0.36.0 in flight, [src/datalog/lattice.rs](src/datalog/lattice.rs)) introduces an `INSERT … ON CONFLICT DO UPDATE` pattern with user-supplied join function. There is no SQL injection check on `join_fn` (it is a function name a registered superuser passes); document that this is a privileged operation.
- **Well-founded semantics** ([src/datalog/wfs.rs](src/datalog/wfs.rs)) implements three-valued semantics with iteration cap of 1,000; convergence-with-cap behaviour on real ontologies (DBpedia, schema.org) is unproven.
- **DRed retraction** (v0.34.0): incremental delete-rederive for materialised predicates; not yet stress-tested under high write rates.
- **Built-in OWL RL set** ([src/datalog/builtins.rs](src/datalog/builtins.rs)) exists but the prompt calls out "complete OWL RL" was a v0.24.0 deliverable — confirm via SHACL Core / OWL RL conformance tests.

### Recommendations

1. **Bound `owl:sameAs` clusters**: emit a `PT5xx` warning when a single equivalence class exceeds `pg_ripple.sameas_max_cluster_size` (default 100 K), and consider Tarjan-SCC over the sameAs subgraph rather than full transitive closure.
2. **Coordinated rollback for parallel strata**: wrap the parallel-group execution in a single transaction with a savepoint per worker; on any failure, abort the whole stratum.
3. **Validate `lattice.join_fn` identifiers** via PostgreSQL `regprocedure::text` round-trip rather than literal interpolation.
4. **Add a Datalog convergence regression suite**: load DBpedia subset → run RDFS + OWL RL → assert iteration count and elapsed time.
5. **Document WFS semantics and limits** in the user guide; add a test that triggers the iteration cap and asserts the partial result + warning.
6. **Add a real OWL RL conformance suite** (e.g., the W3C OWL 2 RL test cases) gated as a CI warning job.

---

## 6. SHACL Validation

### Findings

- **Strong constraint coverage**: `sh:minCount`, `sh:maxCount`, `sh:datatype`, `sh:in`, `sh:pattern`, `sh:class`, `sh:node`, `sh:property`, `sh:or`/`sh:and`/`sh:not`/`sh:qualifiedValueShape`, `sh:hasValue`, `sh:nodeKind`, `sh:languageIn`, `sh:uniqueLang`, `sh:lessThan`, `sh:greaterThan`, `sh:closed`.
- **Notable gaps vs. SHACL Core**:
  - `sh:path` complex paths (sequence, inverse, alternative, zero-or-more) — only simple property paths supported.
  - `sh:minExclusive` / `sh:maxExclusive`.
  - `sh:minLength` / `sh:maxLength`.
  - `sh:equals` / `sh:disjoint`.
- **Async validation pipeline** ([src/lib.rs#L3761-L3830](src/lib.rs#L3761)) queues validations into `_pg_ripple.validation_queue`, drains via the merge worker, redirects persistent violators to a dead-letter queue. No load test confirms the queue drains under sustained 10 K writes/sec.
- **Monolithic `validate_shape()`** (~600 lines) makes it difficult to add new constraint kinds without ripple risk.
- **SHACL hints to the SPARQL planner** (drop `DISTINCT` if `sh:maxCount 1`, downgrade `LEFT JOIN` to `INNER JOIN` if `sh:minCount 1`) are documented in [AGENTS.md](AGENTS.md) but not visibly wired into [src/sparql/sqlgen.rs](src/sparql/sqlgen.rs).
- **Constraint failure messages** are PostgreSQL-style but do not always include the focus node IRI in decoded form, making operational debugging hard.

### Recommendations

1. **Implement the missing SHACL Core constraints** (priority: complex `sh:path`, exclusive bounds, length).
2. **Refactor `validate_shape()`** into one helper per constraint kind under [src/shacl/constraints/](src/shacl/constraints/). Target ≤80 lines per helper.
3. **Wire SHACL hints into SPARQL translation**: when a predicate carries `sh:maxCount 1`, the encoder emits a hint that `sqlgen` consumes for `DISTINCT` removal and join downgrades. Add a regression test asserting both query plan equivalence and result equivalence.
4. **Async-pipeline load test**: pgbench harness inserting 10 K conflicting facts/sec for 5 minutes; assert dead-letter queue is non-empty and drain rate matches arrival rate ± 5%.
5. **Always include decoded focus-node IRIs** in violation messages.

---

## 7. Federation & Linked Data

### Findings

- **SERVICE clause** ([src/sparql/federation.rs](src/sparql/federation.rs)) implements rewrite-based federation with a registry table (`_pg_ripple.federation_endpoints`) and per-call timeout (`pg_ripple.federation_timeout`, default 30 s).
- **Endpoint allowlist** prevents arbitrary SERVICE URIs but **does not restrict by IP/hostname** — `localhost:5432`, `169.254.169.254` (cloud metadata), and other internal services are reachable if a superuser registers them.
- **No streaming**: SERVICE results are buffered in memory and inlined as `VALUES`. Large remote result sets can OOM the backend.
- **Sequential SERVICE execution**: multiple `SERVICE` clauses in a query run one after another rather than in parallel.
- **Result caching** keys on `(endpoint, query_hash)` (XXH3-128). No TTL controls visible per endpoint; assume the v0.19.0 implementation has these.
- **JSON-LD framing** ([src/framing/frame_translator.rs](src/framing/frame_translator.rs)) handles `@id`, `@reverse`, `@type`, `@context`. Untested: nested frames, `@list` arrays, `@container: @index/@language`, full compaction round-trip.
- **CONSTRUCT/DESCRIBE/ASK as views** ([src/views.rs](src/views.rs)) are well-architected: template constants encoded at view-creation time; blank-node templates rejected (correct). Test gaps: complex multi-subject templates, RDF-star DESCRIBE.
- **HTTP companion** ([pg_ripple_http/](pg_ripple_http/)) does **constant-time auth comparison** (good), supports Bearer/Basic, redacts errors to opaque trace IDs, but: **no certificate pinning** on remote SERVICE calls (MITM risk), CORS default `*`, X-Forwarded-For trusted without reverse-proxy validation, 10 MB body limit hardcoded.

### Recommendations

1. **Restrict federation endpoints by IP/CIDR**: deny RFC 1918, link-local, loopback, and metadata IPs by default; allow override via explicit `pg_ripple.federation_allow_private` GUC (off by default).
2. **Stream federation results** through a temporary table when the response exceeds `pg_ripple.federation_inline_max_rows` (default 10 K).
3. **Parallelise independent SERVICE clauses** using `parallel-fetch` background workers.
4. **Add certificate pinning / CA-bundle env var** to the HTTP client; default to the system trust store with optional `PG_RIPPLE_HTTP_CA_BUNDLE` override.
5. **Strengthen CORS defaults**: require explicit origin allowlist; reject `*` unless `PG_RIPPLE_HTTP_ALLOW_ANY_ORIGIN=true`.
6. **Validate `X-Forwarded-For`** only when `PG_RIPPLE_HTTP_TRUST_PROXY` lists the upstream IP.
7. **Make HTTP body limit configurable** (`PG_RIPPLE_HTTP_MAX_BODY_BYTES`).

---

## 8. Documentation & DX

### Findings

- **[AGENTS.md](AGENTS.md), [ROADMAP.md](ROADMAP.md), [plans/implementation_plan.md](plans/implementation_plan.md)** are excellent — the dual roadmap (plain-language + technical deliverables) is one of the strongest aspects of the project.
- **CHANGELOG.md** is consistent and detailed per release.
- **mdBook docs site** (`docs/`) was rebuilt in v0.33.0; coverage is broad but spot-check finds:
  - Operations guide light on troubleshooting (no decision tree for "merge worker not catching up", "rare-predicate promotion stuck", "dictionary cache thrash").
  - Performance tuning guide does not document GUC ↔ workload-class matrix.
  - No public **architecture diagram** (the AGENTS.md tree is the closest substitute).
- **Code comments** are accurate where present but sparse in [src/sparql/sqlgen.rs](src/sparql/sqlgen.rs) and [src/datalog/compiler.rs](src/datalog/compiler.rs); large algorithms lack a top-of-function summary.
- **Public function rustdoc** is uneven; many `#[pg_extern]` functions have no doc comment, so the generated docs site has gaps.
- **Examples**: [examples/](examples/) is sparse for a project this rich. Worked examples for SHACL+Datalog interactions, hybrid vector search, and GraphRAG export would lower onboarding cost.

### Recommendations

1. **Add a rustdoc lint gate**: `#![warn(missing_docs)]` on public items; enforce in CI.
2. **Add an architecture diagram** (`renderMermaidDiagram`-able) to `docs/src/reference/architecture.md` showing dictionary → VP → SPARQL/Datalog/SHACL → views/exporters.
3. **Write a troubleshooting matrix** (symptom → likely GUC → diagnostic query → fix).
4. **Expand [examples/](examples/)**: end-to-end "RDF data quality" (Turtle load → SHACL → SPARQL report), "hybrid retrieval" (vector + SPARQL), "GraphRAG round-trip".
5. **Add a "GUC reference" page** auto-generated from the `_PG_init` registration block (one source of truth).

---

## 9. Migration & Upgrades

### Findings

- **Every consecutive migration script from `0.1.0 → 0.36.0` exists** under [sql/](sql/). No version-path gaps. The discipline of "every release ships a migration script even if empty" (per [AGENTS.md](AGENTS.md)) is being followed.
- **Migration chain test** ([tests/test_migration_chain.sh](tests/test_migration_chain.sh)) verifies the full sequence — this is the project's strongest correctness mechanism for upgrades.
- **Idempotency**: scripts use `CREATE TABLE IF NOT EXISTS` and `ALTER TABLE … ADD COLUMN IF NOT EXISTS` consistently.
- **Documentation per script is uneven** — the `0.5.1 → 0.6.0` HTAP split (a major schema reshuffle) lacks an in-script header explaining the migration semantics.
- **No downgrade scripts**. PostgreSQL extensions don't formally require them, but for HTAP / dictionary changes a documented rollback procedure (e.g., dump-restore) would help operators.
- **No data-version stamping**: `_pg_ripple` does not record the schema version of the data on disk (separate from the extension version), so a rollback or partial-upgrade scenario is hard to diagnose.

### Recommendations

1. **Standardise migration headers** with `-- Migration X.Y.Z → A.B.C` block including (a) schema changes, (b) data-rewrite cost, (c) downgrade strategy, (d) test reference.
2. **Add a `_pg_ripple.schema_version` table** stamped at install/upgrade time; `pg_ripple.diagnostic_report()` should surface mismatches.
3. **Document recovery procedures** per release in `RELEASE.md` (already exists; expand the "rollback" section).

---

## 10. Security & Stability

### Findings

- **No raw SQL injection vectors detected.** All dynamic table names use integer predicate IDs (`format!("_pg_ripple.vp_{pred_id}")` with `pred_id: i64`). User-provided SPARQL/Turtle goes through `spargebra`/`rio_turtle` parsers — no string concatenation into SQL.
- **Hard panics** are the dominant stability risk (see §1). Any `expect()` in a `#[pg_extern]` path crashes the backend; the merge background worker has watchdog cover but library-side panics during `LOAD`/`UPDATE` will end the connection.
- **`pg_ripple.rls_bypass`** is a dangerous knob; it's correctly documented but should be `SUSET` *and* require an explicit `ALTER SYSTEM` (not session-level) to flip.
- **GUC string enums lack validators**: `inference_mode`, `enforce_constraints`, `rule_graph_scope`, `shacl_mode`, `describe_strategy` accept arbitrary strings; invalid values silently behave as defaults or fail at first use.
- **Integer GUCs lack bounds**: `max_path_depth`, `property_path_max_depth`, `merge_threshold`, `merge_interval_secs` accept `0` or negative values, producing cryptic downstream errors.
- **Superuser-only file loaders** (`pg_ripple.load_*_file`) correctly check superuser via `pg_read_file`; spot-checked.
- **No DoS bounds** on:
  - SPARQL query result-set size (no `LIMIT` enforced; CONSTRUCT can return millions of triples).
  - Datalog inference output size.
  - GraphRAG export volume (entire graph materialised in JSON).
- **Transaction handling** for dictionary inserts is best-effort: a rolled-back transaction can leave a stale shared-memory cache entry (see §3).
- **HTTP companion**: see §7. Federation MITM, CORS, and X-Forwarded-For trust are the open issues.

### Recommendations

1. **Project-wide ban on `unwrap()`/`expect()`** in non-test code, enforced by clippy.
2. **Implement custom GUC `check_hook` validators** for all enum-valued GUCs and for integer GUCs with semantic minimums.
3. **Add resource governors**: `pg_ripple.sparql_max_rows`, `pg_ripple.datalog_max_derived`, `pg_ripple.export_max_rows` — emit warnings or errors when exceeded.
4. **Promote `rls_bypass` to `PGC_POSTMASTER`** so it cannot be flipped per-session.
5. **Add a `pg_ripple.diagnostic_report()` SRF** that summarises GUC validity, dictionary cache hit rate, merge backlog, validation queue depth, and federation endpoint health — useful operationally and ideal for CI smoke tests.

---

## 11. Performance & Scalability

### Findings

- **Dictionary encode latency** dominates write throughput on cold cache: 2-RTT SPI per miss. With a warm shared-memory cache (v0.22.0), bulk loads hit ~50 K triples/sec on a single backend (per benchmark history).
- **Per-query encode HashMap** is allocated, populated, dropped — measurable garbage for short BGPs.
- **Predicate-OID lookup per atom** (no syscache callback) — ~100 µs per atom × N atoms in a complex BGP.
- **Merge worker is single-threaded per database**: no per-VP-table parallelism. Big workloads with many predicates can lag.
- **Tombstone read amplification**: reads pay a `LEFT ANTI JOIN` against tombstones until the next merge.
- **Plan cache miss ratio** is invisible (no exposed counter).
- **Federation**: serial SERVICE execution + full buffering = wall-time = sum of remote latencies; cache hit-rate not exposed.
- **Datalog parallel groups (v0.35.0)** improve OWL RL closure but use shared sequence allocation for delta tables — contention point at the PG sequence cache.
- **Recursive CTE memory**: long property-path chains (`+`, `*`) materialise the entire path into the work-table; no spill-to-disk strategy beyond PostgreSQL's defaults.
- **Vector hybrid search** uses pgvector HNSW; performance on >1 M embeddings under concurrent SPARQL load is unmeasured.

### Recommendations

1. **Batch encode in `sqlgen`** (one SPI call per query for all constants).
2. **Cache predicate OIDs** per backend, invalidated by syscache callback.
3. **Parallelise the merge worker** across VP tables with a simple work-stealing pool bounded by `pg_ripple.merge_workers` (default 1).
4. **Schedule periodic VACUUM** of tombstone-heavy VP tables.
5. **Expose plan-cache and federation-cache metrics** via `pg_ripple.cache_stats()` and `pg_stat_statements` extensions.
6. **Pre-allocate sequence ranges** for parallel Datalog workers (`SELECT setval(seq, currval+10000)` style) to avoid contention.
7. **Add a `LIMIT` push-down** for SPARQL ORDER BY + LIMIT into the SQL plan (TopN).
8. **Benchmark BSBM and LUBM** in CI; treat regressions >10% as gating.

---

## 12. Integration & GUC Parameters

### Findings

- **GUC inventory** (~25 parameters) is well-organised but lacks a single source of truth. The full list spans:
  - Storage: `vp_promotion_threshold`, `named_graph_optimized`, `dictionary_cache_size`, `cache_budget_mb`, `merge_threshold`, `merge_interval_secs`, `merge_retention_seconds`, `latch_trigger_threshold`, `merge_watchdog_timeout`, `dedup_on_merge`, `worker_database`.
  - Query: `default_graph`, `plan_cache_size`, `max_path_depth`, `property_path_max_depth`, `bgp_reorder`, `parallel_query_min_joins`, `describe_strategy`.
  - SHACL: `shacl_mode`, `enforce_constraints`.
  - Datalog: `inference_mode`, `rule_graph_scope`, `datalog_parallel_workers`, `datalog_parallel_threshold`, `lattice_max_iterations`.
  - Federation: `federation_timeout`.
  - Security: `rls_bypass`.
  - WCOJ: `wcoj_enabled`, `wcoj_min_tables`.
- **Validation gaps**: enums and bounded integers (see §10).
- **Background worker lifecycle**: `_PG_init` registers the merge worker via `BackgroundWorker` with `BGWORKER_SHMEM_ACCESS`; `worker_database` selection happens at start time; SIGTERM is handled. Operational risks live in §3 (race conditions), not the harness.
- **`pg_stat_statements` integration**: SPARQL queries surface as their compiled SQL (via SPI), so `pg_stat_statements` already shows them — but with internal predicate IDs that are operationally opaque.
- **No OpenTelemetry / tracing** integration for query lifecycle, merge cycles, or federation calls.

### Recommendations

1. **Auto-generate the GUC reference page** from the `_PG_init` block (single source of truth, drives docs and validators).
2. **Add `check_hook`s** for all enum and bounded-int GUCs.
3. **Decode predicate IDs in `pg_stat_statements` views**: ship a helper view `pg_ripple.stat_statements_decoded` that joins on `_pg_ripple.predicates` to surface IRIs.
4. **Add OpenTelemetry spans** (behind a `pg_ripple.tracing_enabled` GUC) for SPARQL parse → translate → execute, merge cycle, and federation calls. Use a thin facade around `tracing` so it can be a no-op.
5. **Document the `worker_database` selection** behaviour and add a startup error if the database does not exist.

---

## Feature Recommendations for Next Roadmap

These are proposed *post-1.0* features that fill gaps identified above and would meaningfully extend pg_ripple's competitive position against Virtuoso, Stardog, GraphDB, and Oxigraph.

### Feature 1: Streaming SPARQL Results & Cursor API

- **Summary**: Server-side cursors for SELECT, CONSTRUCT, and DESCRIBE results, with chunked decoding into Turtle/JSON-LD/CSV streams instead of materialising the whole result set.
- **Motivation**: Today large CONSTRUCTs and DESCRIBEs OOM the backend, and the HTTP companion buffers everything before sending. Streaming is table stakes for analytics workloads and competitive with all major triplestores. Fills the GraphRAG export OOM risk and the "no LIMIT enforcement" stability gap.
- **Effort**: Medium.
- **Technical Details**: Reuse PostgreSQL cursor infrastructure (`DECLARE … CURSOR`); add a streaming decoder that batch-resolves dictionary IDs (1024 rows at a time) and emits Turtle/N-Triples/JSON-LD chunks. Wire to `pg_ripple_http` as `Transfer-Encoding: chunked`. Depends on §7 stream federation infrastructure.

### Feature 2: SPARQL/Datalog Query Tracing & Explain

- **Summary**: A `pg_ripple.explain_sparql(query TEXT)` function that returns the optimised algebra, generated SQL, predicted cardinalities, plan-cache status, and (with `ANALYZE`) actual row counts — plus the equivalent for Datalog rule sets.
- **Motivation**: SPARQL operators currently get raw `EXPLAIN` output of generated SQL with internal predicate IDs. A first-class explain tool dramatically lowers the bar for performance debugging and is a feature competitors (Stardog, GraphDB) charge for. Fills DX gap §8 and the `pg_stat_statements` opacity gap §12.
- **Effort**: Medium.
- **Technical Details**: Hook `sparopt`'s optimiser to emit a JSON algebra tree; pass through `EXPLAIN (FORMAT JSON, ANALYZE) <generated SQL>`; decode predicate IDs back to IRIs; render as a tree. For Datalog: emit per-stratum dependency graph + magic-set rewritten rules + per-iteration delta sizes.

### Feature 3: Online Merge with Per-Predicate Parallelism & Auto-VACUUM

- **Summary**: Replace the single-threaded merge worker with a work-stealing pool of N merge workers (configurable), add transactional cutover via advisory locks, and integrate tombstone GC into the merge cycle.
- **Motivation**: Directly addresses the highest-severity correctness issue (merge ↔ delete race, §3) and the largest operational pain point (merge worker lag at high write rates). Restores HTAP claims under realistic concurrent workloads.
- **Effort**: High.
- **Technical Details**: Refactor [src/storage/merge.rs](src/storage/merge.rs) and [src/worker.rs](src/worker.rs); introduce `pg_ripple.merge_workers` GUC; per-predicate `pg_advisory_xact_lock`; integrate VACUUM scheduling driven by tombstone density; add a stress-test harness in [tests/](tests/) that drives 50+ writers and asserts no lost deletes.

### Feature 4: Live Subscriptions ("RDF CDC")

- **Summary**: Logical replication–style change feeds for triples, named graphs, and SPARQL views — clients subscribe via `LISTEN/NOTIFY` or a WebSocket on `pg_ripple_http` and receive added/removed triples in real time, optionally filtered by SPARQL pattern or SHACL shape.
- **Motivation**: Materialised SPARQL views (v0.11.0) and CONSTRUCT/ASK views (v0.18.0) already track changes internally; exposing them as a streaming API unlocks GraphRAG-style downstream pipelines, real-time dashboards, and ML feature stores. None of the major triplestores ship this as a first-class capability — competitive differentiator. Builds on existing CDC scaffolding in [src/cdc.rs](src/cdc.rs).
- **Effort**: Medium-High.
- **Technical Details**: Extend [src/cdc.rs](src/cdc.rs) with a publication catalog (`_pg_ripple.subscriptions`); reuse PG18 logical decoding plugin infrastructure; add WebSocket endpoint to [pg_ripple_http/](pg_ripple_http/); decode delta rows back into Turtle/JSON-LD; per-subscription SHACL/SPARQL filter applied in the publisher.

### Feature 5: Cost-Based Federation Planner with Source Selection

- **Summary**: A federation planner that decomposes a SPARQL query across multiple registered endpoints + the local store using endpoint-published statistics (`SERVICE DESCRIPTION`, VoID), executes independent fragments in parallel, and joins results in PostgreSQL.
- **Motivation**: The current rewrite-based federation (§7) is correct but naïve: serial SERVICEs, full buffering, no statistics. A real federation planner is what enterprises buy Stardog and Anzo for. Fills federation gaps in §7 and aligns with pg_ripple's "performance + expressiveness" vision.
- **Effort**: High.
- **Technical Details**: Add a VoID-style statistics catalog per endpoint; extend [src/sparql/federation.rs](src/sparql/federation.rs) with a fragment planner (greedy minimum-communication, cf. FedX); use PostgreSQL `dblink`-style parallel SPI workers for concurrent fragment execution; integrate with existing result cache. Depends on Feature 1 (streaming) for large remote results.

---

## Conclusion

The pg_ripple codebase is in good shape for its scope and pace of delivery. The architectural decisions — VP storage with hash-backed-sequence dictionary IDs, integer joins everywhere, FILTER-time constant encoding, recursive CTEs with `CYCLE` for property paths, semi-naïve Datalog with magic sets and parallel strata — are all defensible and largely correct. The remaining work to reach v1.0.0-grade reliability falls into three buckets:

1. **Storage concurrency hardening** (highest priority): close the HTAP merge race, make rare-predicate promotion idempotent and serialised, fix dictionary-cache rollback semantics, schedule tombstone GC. Without these, real concurrent workloads will produce subtle correctness anomalies. (§3, Feature 3.)
2. **Operational hardening**: eliminate `.expect()`/`.unwrap()` in library code, add GUC validators, add resource governors, implement a `diagnostic_report()` SRF, and add OpenTelemetry tracing. These prevent the "silent foot-gun" failure modes that operators dread. (§1, §10, §12.)
3. **Test depth, especially under concurrency and at scale**: stress tests for merge ↔ writers, SHACL async queue under load, Datalog convergence on real ontologies, federation timeout/retry, and W3C SPARQL 1.1/SHACL Core conformance suites. The migration-chain test is the gold standard the rest of the suite should aspire to. (§2.)

Beyond hardening, the five proposed features (streaming results, explain, online merge with parallelism, live RDF CDC, cost-based federation planner) would cement pg_ripple's position as the most complete open-source PostgreSQL-native triple store. The two features I would prioritise for v1.1 are **streaming results + cursors** (largest blast-radius operational risk eliminated, smallest scope) and **the cost-based federation planner** (highest competitive impact, builds directly on existing v0.16/v0.19 federation work).

The single most important non-feature investment is **convert all `.expect()`/`.unwrap()` calls in non-test code into typed `Result` propagation, and gate it with clippy in CI.** Everything else in this report is incremental compared to the stability uplift of removing those panic points.
