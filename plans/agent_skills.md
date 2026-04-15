# pg_ripple — Agent Skills Proposal

> **Date**: 2026-04-15
> **Scope**: New and updated Copilot agent skills for `.github/skills/`
> **Status**: Proposal

---

## Current Skills Inventory

| Skill | Purpose | Maturity |
|---|---|---|
| `implement-version` | Deliver a roadmap milestone (code, tests, docs, checklist) | Solid — covers v0.1–v1.0 lifecycle |
| `fix-ci` | Diagnose and fix GitHub Actions failures | Solid — 6+ failure patterns catalogued |
| `create-pull-request` | Open a PR with Unicode-safe body, branch policy | Solid — guards against all known footguns |
| `release` | Tag a release (changelog, control file, CI gate) | Solid — end-to-end release procedure |

These four skills cover the core dev-ops loop well. The gaps lie in **day-to-day coding tasks**, **domain-specific knowledge**, and **cross-cutting concerns** that arise repeatedly during implementation of v0.7–v1.0.

---

## Proposed New Skills

### 1. `write-pg-regress-test`

**Trigger**: writing a new pg_regress SQL test, updating expected output, debugging test mismatches.

**Why it's needed**: pg_regress tests are the primary contract between the extension and its users. Every version adds 1–3 new test files. The process has project-specific conventions (schema setup, `allow_system_table_mods`, output diffing, `--resetdb` flags) that are easy to get wrong.

**Scope**:
- Template for a new `.sql` test file and its `.out` expected output
- Conventions: `SET client_min_messages`, `CREATE EXTENSION IF NOT EXISTS`, test isolation
- Loading test data (inline `insert_triple()` vs. `load_ntriples()`)
- Running: `cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"` with and without `--resetdb`
- Debugging output diff: comparing `results/*.out` vs `expected/*.out`
- Handling non-deterministic output: `ORDER BY`, `::text` casts, `LIMIT` guards
- Known gotchas: `pg_` schema prefix, test ordering within a file, SID sequence sensitivity

**Estimated effort**: 1–2 hours to write.

---

### 2. `write-sparql-sql-translation`

**Trigger**: implementing a new SPARQL feature (e.g., MINUS, aggregates, SERVICE, DELETE/INSERT WHERE), extending `src/sparql/sqlgen.rs`.

**Why it's needed**: SPARQL→SQL translation is the most complex and error-prone subsystem. It has hard invariants (integer joins, filter pushdown, self-join elimination, cycle detection) that are easy to violate. Eight upcoming versions (v0.7–v0.16) touch the SPARQL engine. A skill that encodes the translation patterns would prevent regressions.

**Scope**:
- Architecture recap: `spargebra::Query` → `sparopt` algebra → `sqlgen` SQL string → SPI execution → batch decode
- The integer-join invariant: encode all bound terms to `i64` before SQL generation; string comparisons in VP queries are a bug
- Pattern catalogue:
  - BGP → multi-way VP join with OID-based table lookup
  - OPTIONAL → `LEFT JOIN`
  - UNION → `UNION ALL` with column alignment
  - MINUS → `EXCEPT` or anti-join
  - FILTER → `WHERE` clause with inline-encoded constants
  - Aggregates → `GROUP BY` + aggregate SQL functions
  - Subqueries → derived tables
  - Property paths → `WITH RECURSIVE ... CYCLE`
  - VALUES → `VALUES` clause or CTE
  - BIND → computed column in `SELECT`
  - GRAPH → additional `g = ?` predicate in VP join
- Self-join elimination: star patterns on same subject → single scan with multiple joins
- SHACL hints: `sh:maxCount 1` → omit DISTINCT; `sh:minCount 1` → INNER JOIN
- Output decoding: collect all output i64 IDs → batch `WHERE id = ANY(...)` decode → emit rows
- Testing pattern: SPARQL string → expected SQL (via `sparql_explain()`) + expected JSONB output
- Common mistakes and how to catch them

**Estimated effort**: 3–4 hours to write.

---

### 3. `write-migration-script`

**Trigger**: preparing a version release that requires a SQL migration, or creating an empty migration for a Rust-only release.

**Why it's needed**: Every version needs a `sql/pg_ripple--X.Y.Z--X.(Y+1).Z.sql` migration script. Missing migrations break `ALTER EXTENSION ... UPDATE` for all downstream users. AGENTS.md documents this, but a dedicated skill would make the process foolproof — especially for versions with complex schema changes (e.g., v0.6.0 HTAP tables, v0.7.0 SHACL tables, v0.10.0 source column).

**Scope**:
- When to create a migration: every version, no exceptions
- Empty migration template (Rust-only changes): comment header explaining what's new
- Schema-change migration: `ALTER TABLE`, `CREATE TABLE`, `CREATE INDEX`, `CREATE SEQUENCE`
- Naming convention: `pg_ripple--{from}--{to}.sql`
- Testing: `tests/test_migration_chain.sh` — how it works, how to extend it
- Common schema changes by version:
  - Adding columns to VP tables (e.g., `source SMALLINT`)
  - Adding new catalog tables (e.g., `_pg_ripple.shacl_shapes`)
  - Creating new GIN/BRIN indexes
  - Creating new sequences
- Verification: apply migration chain from v0.1.0 to HEAD, confirm `\dx+ pg_ripple`
- Pitfalls: column ordering, `NOT NULL` vs nullable defaults, index naming collisions

**Estimated effort**: 1–2 hours to write.

---

### 4. `debug-pgrx`

**Trigger**: pgrx-specific compilation errors, SPI issues, shared-memory problems, background worker crashes, `#[pg_test]` failures that aren't CI-environment issues.

**Why it's needed**: `fix-ci` focuses on CI pipeline failures (missing deps, argument parsing, regress output diffs). This skill would cover the deeper pgrx 0.17 development issues that arise during implementation: SPI lifetime errors, `Datum` conversion panics, shared-memory allocation, background worker lifecycle, `_PG_init` ordering. These issues recur across versions and have non-obvious solutions.

**Scope**:
- pgrx 0.17 API patterns:
  - `Spi::connect()` lifetime and error handling
  - `Spi::get_one::<T>()` vs `Spi::run()` — when to use which
  - `SpiTupleTable` iteration patterns
  - `Datum` type mapping (`i64`, `String`, `bool`, `JsonB`, `Option<T>`)
  - `pg_extern` argument types and `default!()` macro
- Shared memory (`src/shmem.rs`):
  - `PgSharedMem` / `pg_shmem_init!` lifecycle
  - Sharded lock patterns for the dictionary cache
  - Slot overflow and eviction
  - GUC-driven sizing (`pg_ripple.dictionary_cache_size`)
- Background workers (`src/worker.rs`):
  - `BackgroundWorkerBuilder` configuration
  - `BGWORKER_SHMEM_ACCESS` flag
  - Signal handling (`SIGTERM`, `SIGHUP`)
  - Database connection from worker context
  - Watchdog timeout patterns
- Common pgrx errors:
  - `InvalidPosition` from empty `RETURNING` — use CTE upsert pattern
  - `PgRelation` lifetime issues — don't hold across SPI calls
  - `palloc`/`pfree` interaction with Rust ownership — when `unsafe` is required
  - `ereport` vs `panic!` — correct error reporting
- Debugging techniques:
  - `pgrx::log!()`, `pgrx::warning!()` for runtime tracing
  - `RUST_LOG` and `log_min_messages` interaction
  - `cargo pgrx run pg18` for interactive debugging
  - Attaching `lldb` to a backend process

**Estimated effort**: 3–4 hours to write.

---

### 5. `write-documentation`

**Trigger**: creating or updating `docs/src/` pages, building the mdBook site, writing SQL reference pages.

**Why it's needed**: Documentation is a deliverable in every version from v0.5.0 onward. The `implement-version` skill mentions documentation briefly, but the actual page structure, writing conventions, example patterns, and mdBook tooling deserve a dedicated skill. The documentation backlog (v0.1–v0.4 catch-up) is large.

**Scope**:
- Site structure recap: User Guide / Reference / Research (per `plans/documentation.md`)
- Page template: frontmatter convention, heading hierarchy, example format
- SQL reference page pattern:
  - Function signature table
  - Description with plain-language explanation
  - Example block (input → output)
  - GUC parameters that affect behaviour
  - Cross-references to related functions
- mdBook specifics:
  - `SUMMARY.md` wiring — every page must be linked
  - `mdbook build docs` — must pass without warnings
  - Code block language hints (`sql`, `sparql`, `turtle`, `rust`)
  - Admonitions / callout syntax
- Writing style:
  - Lead with what users can do, not implementation details
  - Short sentences, bullet points
  - No jargon unless defined
  - No emoji
- Example data conventions: use the foaf/schema.org vocabulary for all examples (consistent across docs)
- Cross-referencing: link to related SQL reference pages, GUC configuration, best practices

**Estimated effort**: 2–3 hours to write.

---

### 6. `benchmark`

**Trigger**: performance testing, running BSBM, profiling query plans, optimizing SPARQL→SQL translation, evaluating HTAP merge throughput.

**Why it's needed**: v0.13.0 is dedicated to performance, but performance questions arise much earlier — every SPARQL feature needs basic performance validation. HTAP merge throughput (v0.6.0), bulk load rates (v0.2.0), and query latency (v0.3.0+) are ongoing concerns. A skill that codifies how to measure, what to measure, and what target numbers to aim for would prevent performance regressions.

**Scope**:
- Benchmark environment setup:
  - `pgbench` via `pgrx-bench` — how to configure
  - Docker-based isolated benchmarks
  - Consistent hardware/resource configuration
- Datasets:
  - BSBM (Berlin SPARQL Benchmark) — standard RDF benchmark
  - SP²Bench — DBLP-based benchmark
  - Custom datasets: scaling from 1K to 10M triples
- What to measure:
  - Bulk load throughput: triples/second for N-Triples, Turtle, N-Quads
  - Single-triple insert latency: `insert_triple()` p50/p95/p99
  - SPARQL query latency: by pattern type (BGP, OPTIONAL, property path, aggregate)
  - HTAP merge duration and write amplification
  - Dictionary cache hit rate (shared memory stats)
  - Plan cache hit rate
- How to measure:
  - `sparql_explain()` for query plan inspection
  - `EXPLAIN (ANALYZE, BUFFERS)` on generated SQL
  - `pg_stat_statements` integration
  - `\timing` in psql
- Regression detection:
  - Baseline numbers per version
  - Before/after comparison template
  - CI integration (optional automated benchmarks)
- Known performance traps:
  - Missing ANALYZE after bulk load
  - Cartesian product from unbound variables
  - Property path without depth limit
  - Unbatched dictionary lookups

**Estimated effort**: 2–3 hours to write.

---

### 7. `implement-shacl`

**Trigger**: implementing SHACL validation (v0.7.0 core, v0.8.0 advanced), working in `src/shacl/`.

**Why it's needed**: SHACL is a self-contained subsystem with its own parsing, compilation, and execution model. It interacts with the storage engine (validation triggers), the dictionary (term encoding), and the SPARQL engine (constraint evaluation). The implementation plan has ~15 pages on SHACL design. A dedicated skill would distill this into an actionable procedure.

**Scope**:
- SHACL architecture: shapes graph → DDL constraints + async validation pipeline
- Core shapes (v0.7.0):
  - `sh:NodeShape`, `sh:PropertyShape`
  - `sh:datatype`, `sh:minCount`, `sh:maxCount`, `sh:pattern`, `sh:minLength`, `sh:maxLength`
  - `sh:class`, `sh:nodeKind`
  - Synchronous validation on `insert_triple()`
  - Validation reports as JSONB
- Advanced shapes (v0.8.0):
  - `sh:or`, `sh:and`, `sh:not`, `sh:xone`
  - `sh:qualifiedValueShape`
  - Async validation via background worker
  - Dead-letter queue for deferred violations
- SQL compilation:
  - Shape → `CHECK` constraint or trigger function
  - Cardinality constraints → query optimization hints
  - Validation query generation
- Storage:
  - `_pg_ripple.shacl_shapes` catalog table
  - Shape → predicate mapping for targeted validation
- Functions:
  - `load_shacl(turtle TEXT)` → parse and register shapes
  - `validate(graph_iri TEXT)` → full validation report
  - `list_shapes()` → registered shapes
  - `drop_shape(shape_iri TEXT)` → deregister
- Testing patterns:
  - W3C SHACL test suite (partial coverage)
  - Custom validation scenarios
  - Performance: validation overhead on insert path

**Estimated effort**: 3–4 hours to write.

---

### 8. `implement-datalog`

**Trigger**: implementing the Datalog reasoning engine (v0.10.0), working in `src/datalog/`.

**Why it's needed**: Datalog is the most algorithmically complex feature on the roadmap (10–12 person-weeks). It requires stratified negation, fixpoint computation, and RDFS/OWL RL materialization — all compiled to SQL. A dedicated skill would encode the stratification algorithm, the SQL compilation patterns, and the interaction with the `source` column for provenance tracking.

**Scope**:
- Architecture: rule parser → stratifier → SQL compiler → materialization loop
- Rule syntax: Datalog-style `head :- body` with built-in predicates
- Stratification:
  - Dependency graph construction
  - Stratified negation (no unstratifiable programs)
  - Bottom-up evaluation order
- SQL compilation:
  - Rule → `INSERT INTO ... SELECT ... WHERE ...` with `source = 1`
  - Recursive rules → `WITH RECURSIVE`
  - Negation → `NOT EXISTS`
  - Aggregation → GROUP BY in rule head
- Built-in rulesets:
  - RDFS entailment (rdfs:subClassOf, rdfs:subPropertyOf, rdfs:domain, rdfs:range)
  - OWL RL subset (owl:sameAs, owl:inverseOf, owl:TransitiveProperty)
- Provenance:
  - `source` column: 0 = explicit, 1 = inferred
  - Retraction: when base triples change, re-evaluate rules that depend on them
- Functions:
  - `load_rules(text TEXT)` → parse and register
  - `infer()` → run materialization to fixpoint
  - `list_rules()` → registered rules
- Testing:
  - RDFS entailment test suite
  - OWL RL test suite
  - Stratification edge cases
  - Performance: materialization time vs. dataset size

**Estimated effort**: 3–4 hours to write.

---

## Proposed Enhancements to Existing Skills

### `implement-version` — Enhancements

1. **Add a "Common Patterns by Version" section** that maps upcoming versions to the new domain-specific skills:
   - v0.7.0–v0.8.0 → delegate to `implement-shacl`
   - v0.10.0 → delegate to `implement-datalog`
   - v0.13.0 → delegate to `benchmark`
   - All versions → delegate to `write-pg-regress-test`, `write-migration-script`, `write-documentation`

2. **Add a "HTAP-Aware Testing" section** for v0.6.0+:
   - Tests must exercise both the delta path and the merged path
   - Merge worker must be triggered in test setup when testing post-merge behavior
   - Tombstone coverage: delete from main, verify query path excludes deleted triples

3. **Add a "Documentation Deliverables" reminder** with a link to `write-documentation` skill.

### `fix-ci` — Enhancements

1. **Add pattern G: Shared-memory allocation failure** — symptom: `FATAL: could not map anonymous shared memory` or `pg_shmem_init` panic. Cause: insufficient `shared_buffers` or `huge_pages` misconfiguration in CI. Fix: override `postgresql.conf` in the test step.

2. **Add pattern H: Background worker timeout** — symptom: HTAP merge tests hang or time out. Cause: merge worker not started or GUC `pg_ripple.worker_database` not set. Fix: ensure worker is registered and database GUC is configured before HTAP tests.

3. **Add pattern I: pg_regress non-deterministic output** — symptom: intermittent test failures from unordered output. Cause: missing `ORDER BY` in test queries. Fix: always sort output in test SQL.

### `release` — Enhancements

1. **Add a "Migration Script Verification" step** that runs `tests/test_migration_chain.sh` as part of the release checklist — currently not mentioned.

2. **Add a "Documentation Site Build" step** — verify `mdbook build docs` passes before tagging.

---

## Priority and Sequencing

Skills should be built in the order they're needed for upcoming versions:

| Priority | Skill | Needed By | Rationale |
|---|---|---|---|
| **P0** | `write-pg-regress-test` | Every version | Most frequently needed; immediately useful |
| **P0** | `write-migration-script` | Every version | Critical for upgrade path; small scope |
| **P1** | `debug-pgrx` | Every version | Recurring development friction; saves debugging time |
| **P1** | `write-sparql-sql-translation` | v0.7.0+ (SHACL queries), v0.12.0 | Core engine knowledge; prevents regressions |
| **P1** | `write-documentation` | v0.7.0+ | Documentation is a deliverable in every version |
| **P2** | `implement-shacl` | v0.7.0 | Domain-specific; needed before v0.7.0 starts |
| **P2** | `benchmark` | v0.7.0+ (perf validation), v0.13.0 | Useful early for perf validation; critical at v0.13.0 |
| **P3** | `implement-datalog` | v0.10.0 | Can wait until v0.10.0 is on the horizon |

---

## Skill File Convention

Each skill lives in `.github/skills/{skill-name}/SKILL.md` with YAML frontmatter:

```yaml
---
name: skill-name
description: 'One-sentence description. Use when: trigger phrases.'
argument-hint: 'Optional hint for the user'
---
```

The skill must be registered in AGENTS.md or in the VS Code `copilot-instructions.md` (whichever is the canonical skill registry for the project). The `description` field is what Copilot uses to match user intent to skills — it must include trigger phrases separated by semicolons.

---

## Estimated Total Effort

| Skill | Effort |
|---|---|
| `write-pg-regress-test` | 1–2 hours |
| `write-migration-script` | 1–2 hours |
| `debug-pgrx` | 3–4 hours |
| `write-sparql-sql-translation` | 3–4 hours |
| `write-documentation` | 2–3 hours |
| `implement-shacl` | 3–4 hours |
| `benchmark` | 2–3 hours |
| `implement-datalog` | 3–4 hours |
| Existing skill enhancements | 2–3 hours |
| **Total** | **~20–27 hours** |

---

## Summary

The four existing skills cover the release lifecycle well. The eight proposed skills fill the gaps in the **implementation loop**: writing tests, writing translations, writing docs, debugging the framework, and implementing the two largest upcoming subsystems (SHACL and Datalog). The enhancements to existing skills address patterns discovered during v0.5.1–v0.6.0 development that weren't present when the originals were written.

Building the P0 skills first (`write-pg-regress-test`, `write-migration-script`) gives immediate returns — they'll be used in every version. The P1 skills (`debug-pgrx`, `write-sparql-sql-translation`, `write-documentation`) reduce the most common friction points. The P2/P3 skills can be written just-in-time before their target versions begin.
