# pg_ripple Overall Assessment 10

Date: 2026-04-29

Scope: This assessment reviewed pg_ripple as declared at v0.69.0 in [Cargo.toml](../Cargo.toml#L4) and [pg_ripple.control](../pg_ripple.control#L5), covering the three releases since Assessment 9: v0.67.0 (Assessment 9 critical remediation, mutation journal, RLS propagation, Arrow Flight hardening), v0.68.0 (portal-based CONSTRUCT cursors, Citus HLL/SERVICE pruning, nonblocking VP promotion, scheduled fuzz CI), and v0.69.0 (module restructuring of `src/sparql/`, `src/construct_rules/`, `src/storage/`, and `pg_ripple_http/src/`). The standard applied is a world-class PostgreSQL extension suitable for third-party security review and production deployment. Every Critical/High finding cites file paths and line ranges.

## Executive Summary

Assessment 9's central architectural thesis — that side effects (CWB hooks, RLS, metrics) were attached to wrapper APIs instead of the storage mutation boundary — has been **substantially resolved by v0.67.0**. The introduction of [`src/storage/mutation_journal.rs`](../src/storage/mutation_journal.rs) plus the relocation of `mutation_journal::record_write` / `record_delete` / `flush()` calls into `storage::insert_triple_by_ids` ([src/storage/mod.rs](../src/storage/mod.rs#L1689-L1693)) and `storage::delete_triple_by_ids` ([src/storage/mod.rs](../src/storage/mod.rs#L1789-L1793)) means SPARQL Update, which calls those functions directly ([src/sparql/execute.rs](../src/sparql/execute.rs#L553), [L572](../src/sparql/execute.rs#L572), [L778](../src/sparql/execute.rs#L778), [L789](../src/sparql/execute.rs#L789)), now correctly fires CONSTRUCT writeback hooks. RLS propagation to dedicated VP delta and main tables is similarly real ([src/security_api.rs](../src/security_api.rs#L40-L104), [src/security_api.rs](../src/security_api.rs#L143-L182), [src/storage/promote.rs](../src/storage/promote.rs#L52-L60)). Arrow Flight unsigned tickets are rejected by default ([pg_ripple_http/src/arrow_encode.rs](../pg_ripple_http/src/arrow_encode.rs#L44-L54)). The release-truth Python gates exist with mandatory `--version` arguments and fail-closed behavior ([scripts/check_roadmap_evidence.py](../scripts/check_roadmap_evidence.py), [scripts/check_api_drift.py](../scripts/check_api_drift.py)). v0.69.0's module restructuring is a real readability and ownership win without behavioral regressions in the static review.

That progress is genuine. However, four large gaps remain that prevent the world-class rating the project's documentation suggests, and several new gaps were introduced by the v0.67.0–v0.68.0 work.

The single most important remaining gap is that **bulk loading bypasses the mutation journal entirely**. [src/bulk_load.rs](../src/bulk_load.rs#L100-L127) flushes batches through `storage::batch_insert_encoded`, which (unlike `insert_triple_by_ids`) contains **no `mutation_journal` calls** ([src/storage/mod.rs](../src/storage/mod.rs#L618-L690)). Loading a million-triple Turtle file therefore silently leaves CONSTRUCT writeback targets stale, despite the v0.67.0 changelog explicitly claiming bulk load is covered. This is the same Assessment 9 C1 concern, partially fixed for one path and not for another. The accompanying regression test [cwb_write_path_equivalence.sql](../tests/pg_regress/sql/cwb_write_path_equivalence.sql) exists and is cited as evidence in `feature_status()`, but it cannot prove what is not implemented.

The second gap is **release-truth fragility around evidence paths**. [src/feature_status.rs](../src/feature_status.rs) cites `docs/src/reference/scalability.md` six times ([L192](../src/feature_status.rs#L192), [L208](../src/feature_status.rs#L208), [L223](../src/feature_status.rs#L223), [L237](../src/feature_status.rs#L237), [L251](../src/feature_status.rs#L251), [L265](../src/feature_status.rs#L265)), `docs/src/reference/arrow-flight.md` ([L280](../src/feature_status.rs#L280)), and `tests/integration/citus_rls_propagation.sh` ([L247](../src/feature_status.rs#L247)) — none of which exist on disk. The v0.67.0 GATE-02 CI job that is supposed to verify these evidence paths must therefore either be silently passing (a bug) or be running but not blocking, in which case the entire release-truth premise is theatre.

The third gap is **documentation drift across release boundaries**. [README.md](../README.md#L33) still says "What works today (v0.67.0)" and the limitations section is at v0.67.0 — two releases stale. [roadmap/v0.67.0.md](../roadmap/v0.67.0.md#L5) is still marked "Status: Planned" despite v0.67.0 being two releases in the past. [sbom.json](../sbom.json#L14) reports `pg_ripple@0.51.0` — eighteen releases stale, despite SBOM-01 in v0.67.0 explicitly claiming SBOM regeneration in CI. [docs/src/features/shacl-sparql-rules.md](../docs/src/features/shacl-sparql-rules.md) still claims `sh:SPARQLRule` is supported, while [src/shacl/af_rules.rs](../src/shacl/af_rules.rs#L171-L175) only warns and skips. This is the third assessment in a row to flag the SHACL-SPARQL overclaim.

The fourth gap is that the **mutation journal flushes per-triple, not per-statement**. Both [src/storage/mod.rs](../src/storage/mod.rs#L1693) and [src/storage/mod.rs](../src/storage/mod.rs#L1792) call `mutation_journal::flush()` immediately after a single triple's `record_write`/`record_delete`. A SPARQL `INSERT DATA` of N quads will run the full CONSTRUCT writeback pipeline N times rather than once with N affected graph IDs collapsed. For workloads with even modest CONSTRUCT rule counts, this turns linear writes into quadratic hot loops. The journal's documented design explicitly says "Bulk loaders call `flush()` once after all rows are inserted" — the implementation does not honor this contract.

Top five recommended actions, in priority order: (1) Wire bulk-load batch flush through the mutation journal at end-of-load, not per batch; (2) Make `flush()` defer to statement boundary (e.g., transaction-end callback) rather than firing after every single-triple insert/delete; (3) Make the v0.67.0 GATE-02 evidence-path validator actually fail CI when the cited file is missing, then create or remove the missing files; (4) Update README, sbom.json, and `roadmap/v0.67.0.md` to current state, and either implement `sh:SPARQLRule` or delete the docs page; (5) Promote the W3C SPARQL conformance smoke subset and BSBM regression gate to required status before tagging v1.0.0. Production-readiness verdict: **Late beta, suitable for controlled pilots; not yet production-ready for tenant-isolated, multi-writer, externally exposed deployments.**

## Assessment Method

The assessment read the documents listed in the prompt, plus the v0.67.0 mutation journal kernel, the v0.68.0 promotion-with-recovery code, and the v0.69.0 module facades. Two parallel exploration subagents enumerated module sizes, evidence-path existence, auth-call sites in the HTTP service, and the SPARQL-Update → storage call graph. Programmatic checks were performed where possible:

- **Migration scripts**: confirmed via [list_dir](../sql/) that `pg_ripple--0.66.0--0.67.0.sql`, `pg_ripple--0.67.0--0.68.0.sql`, and `pg_ripple--0.68.0--0.69.0.sql` are all present and use the `-- Migration X.Y.Z → A.B.C` header form required by [scripts/check_migration_headers.sh](../scripts/check_migration_headers.sh).
- **Version metadata**: [Cargo.toml](../Cargo.toml#L4), [pg_ripple.control](../pg_ripple.control#L5), and [ROADMAP.md](../ROADMAP.md#L105-L108) all agree on v0.69.0; [pg_ripple_http/Cargo.toml](../pg_ripple_http/Cargo.toml#L2) is independently versioned at 0.16.0.
- **GitHub Actions pinning**: spot-checked four workflows and observed only 40-character SHA pins.
- **Evidence paths**: programmatically searched for `docs/src/reference/scalability.md`, `docs/src/reference/arrow-flight.md`, `docs/src/reference/query-optimization.md`, and `tests/integration/citus_rls_propagation.sh`; none exist.
- **Mutation journal call sites**: `grep` confirms `storage::insert_triple_by_ids` and `storage::delete_triple_by_ids` are the only places that call `mutation_journal::record_*`/`flush()`, and that `bulk_load.rs` does not.
- **CREATE POLICY IF NOT EXISTS**: appears at [src/security_api.rs](../src/security_api.rs#L98) and [L175](../src/security_api.rs#L175); PostgreSQL 18 added this clause, so this is not a syntax bug for the declared target.

Items not verified (require a running PG18/Citus/HTTP environment): runtime SPARQL conformance pass rates, Arrow Flight end-to-end streaming behavior with multi-million-row exports, Citus per-shard HLL accuracy, mutation-journal flush behavior under concurrent writers, mdBook build success, the W3C/Jena/WatDiv/LUBM/OWL2RL conformance suites' actual current pass rates, and the runtime behavior of `recover_interrupted_promotions()` under simulated crash. These are labeled "Static analysis — runtime confirmation required" in the affected findings.

## Resolution of Prior Assessment 9 Findings

| Finding ID | Status in v0.67–v0.69 | Evidence | Remaining gap |
|---|---|---|---|
| **C1** CWB hook bypass | **Partially Resolved** | Mutation journal kernel in [mutation_journal.rs](../src/storage/mutation_journal.rs); `insert_triple_by_ids` calls `record_write` + `flush` at [storage/mod.rs](../src/storage/mod.rs#L1692-L1693); `delete_triple_by_ids` calls `record_delete` + `flush` at [storage/mod.rs](../src/storage/mod.rs#L1791-L1792); SPARQL Update routes through these at [sparql/execute.rs](../src/sparql/execute.rs#L553), [L572](../src/sparql/execute.rs#L572), [L778](../src/sparql/execute.rs#L778), [L789](../src/sparql/execute.rs#L789). | **Bulk load still bypasses.** [bulk_load.rs](../src/bulk_load.rs#L100-L127) `flush_batch` calls `storage::batch_insert_encoded` which contains zero `mutation_journal` calls. Re-raised as **CF-1**. |
| **C2** CWB retraction correctness end-to-end | **Partially Resolved** | Retraction lives in [construct_rules/retract.rs](../src/construct_rules/retract.rs); SPARQL Update path now invokes it via the journal. | Bulk-load deletes still bypass; tests do not assert end-to-end retract semantics for promoted predicates after merge under all paths. **CF-2**. |
| **C4** Arrow Flight unsigned tickets / tombstones / buffer | **Largely Resolved** | Unsigned ticket rejection at [arrow_encode.rs](../pg_ripple_http/src/arrow_encode.rs#L45-L54); HMAC verify at [L67-L80](../pg_ripple_http/src/arrow_encode.rs#L67-L80); tombstone-exclusion query stated in v0.67.0 changelog (FLIGHT-SEC-02). | **Static analysis — runtime confirmation required** for true streaming (Body::from_stream vs buffered batch). Memory ceiling under 10M-row export not measured. **HF-3**. |
| **H2** Citus end-to-end wiring | **Partially Resolved** | HLL translation wired at [sparql/translate/group.rs](../src/sparql/translate/group.rs#L240-L246); SERVICE annotation hook at [translate/graph.rs](../src/sparql/translate/graph.rs#L364-L366); promotion status tracking added in v0.68.0 PROMO-01. | Multi-node integration still unverified ([tests/integration/citus_rls_propagation.sh](../tests/integration/citus_rls_propagation.sh) cited in feature_status but **does not exist**). **HF-1**. |
| **H3** CONSTRUCT cursor materializes | **Resolved** | `ConstructCursorIter` lazy iterator in [src/sparql/cursor.rs](../src/sparql/cursor.rs); changelog STREAM-01 says portal-based per-page application. | Per-page memory bound to `pg_ripple.export_batch_size`; runtime confirmation still wanted. |
| **H4 / SC1** SHACL-SPARQL rule overclaim | **Still Open** | [docs/src/features/shacl-sparql-rules.md](../docs/src/features/shacl-sparql-rules.md#L1-L80) still claims support; [src/shacl/af_rules.rs](../src/shacl/af_rules.rs#L171-L175) still only warns. | **CF-3**. Third assessment in a row. |
| **P2** Citus HLL COUNT(DISTINCT) | **Resolved** (translator wired) | [sparql/translate/group.rs](../src/sparql/translate/group.rs#L240-L246) emits `hll_cardinality(hll_add_agg(hll_hash_bigint(x)))::bigint` when GUC is on and HLL extension present. | Accuracy bounds not documented or tested; **MF-7**. |
| **P3** Citus SERVICE pruning | **Resolved** (hook wired) | [translate/graph.rs](../src/sparql/translate/graph.rs#L364-L366) calls `citus_service_shard_annotation()` when GUC is on. | Annotation effectiveness not benchmarked end-to-end on a real Citus cluster. **MF-8**. |
| **P5** Arrow Flight buffered | **Partially Resolved** | Batch-streaming claim in v0.67.0 FLIGHT-SEC-02 with `arrow_batch_size` GUC. | Static analysis — `Body::from_stream` vs single `Body::from(buf)` path not confirmed. **HF-3**. |
| **S2** Arrow unsigned tickets | **Resolved** | Unsigned tickets rejected by default at [arrow_encode.rs](../pg_ripple_http/src/arrow_encode.rs#L45-L54). | None for the default config; dev-allow path is a documentation issue. |
| **S4** RLS not on promoted VPs | **Resolved** | `apply_rls_to_vp_table` invoked from `ensure_htap_tables` at [storage/mod.rs](../src/storage/mod.rs#L426-L427) and from `promote_predicate` at [storage/promote.rs](../src/storage/promote.rs#L59-L60); `apply_rls_policy_to_all_dedicated_tables` called from `do_grant_graph_access` at [security_api.rs](../src/security_api.rs#L139). | Unit tests exist via [tests/pg_regress/sql/rls_promotion.sql](../tests/pg_regress/sql/rls_promotion.sql); end-to-end Citus propagation still unproven. New SQL-injection-via-role-name concern. **HF-4**. |
| **SC2** CWB test matrix incomplete | **Partially Resolved** | [tests/pg_regress/sql/cwb_write_path_equivalence.sql](../tests/pg_regress/sql/cwb_write_path_equivalence.sql) exists. | Bulk-load equivalence cannot be exercised because bulk load doesn't fire the hook. **CF-1 / MF-1**. |
| **SC3** Doc/version drift | **Still Open** | [README.md](../README.md#L33) at v0.67.0; [roadmap/v0.67.0.md](../roadmap/v0.67.0.md#L5) "Planned"; [sbom.json](../sbom.json#L14) at v0.51.0; [docs/src/features/shacl-sparql-rules.md](../docs/src/features/shacl-sparql-rules.md) overclaims. | **HF-2 / HF-5**. |
| **A1** `check_roadmap_evidence.sh` checks `[Unreleased]` | **Resolved (replacement provided)** | [scripts/check_roadmap_evidence.py](../scripts/check_roadmap_evidence.py) requires `--version` and is fail-closed. | The legacy `.sh` still exists; if `justfile` or any CI step still calls `.sh`, the bug remains live. **MF-2**. |
| **A3** `feature_status.rs` cites missing evidence | **Still Open** | [feature_status.rs](../src/feature_status.rs#L192) `docs/src/reference/scalability.md` (×6 occurrences), [L247](../src/feature_status.rs#L247) `tests/integration/citus_rls_propagation.sh`, [L280](../src/feature_status.rs#L280) `docs/src/reference/arrow-flight.md` — **none exist on disk**. | Either the GATE-02 CI job is silently passing (it must be — otherwise CI would block), or it is informational only. **CF-4**. |
| **A4** Release dashboard claims missing files | **Still Open** | Same evidence-path validation gap as A3. | **CF-4**. |

## Maturity Assessment

| Area | Rating | Evidence | Key Remaining Gap |
|---|---|---|---|
| Correctness & Semantic Fidelity | **B−** | Mutation journal closes SPARQL Update path; CWB retraction is real where invoked. | Bulk load still bypasses; per-triple flush turns batched writes into quadratic CWB. |
| Bugs & Runtime Safety | **B** | Production `panic!` removed in v0.67.0 PANIC-01; RLS, Arrow Flight defaults harden the base. | Concurrent-writer mutation journal semantics not stress-tested; SQL injection via role names in RLS DDL needs review. |
| Code Quality & Maintainability | **B−** | v0.69.0 module split improves cognitive load; `pg_ripple_http/main.rs` reduced to ~250 lines; `construct_rules/` is now a directory. | `src/lib.rs` (~2,300), `src/storage/mod.rs` (~2,100), `src/sparql/mod.rs` still ~1,800, `routing.rs` ~1,200 — split incomplete. |
| Performance & Scalability | **C+** | HLL aggregate translation wired; portal-based CONSTRUCT cursor real; nonblocking promotion has crash recovery. | Per-triple journal flush, Arrow Flight memory ceiling unverified, BSBM/WatDiv gates non-blocking, no merge-throughput regression gate enforced. |
| Security | **C+** | RLS propagation real; unsigned Arrow tickets rejected by default; SHA pinning clean. | SQL injection via role names in RLS DDL; SBOM 18 releases stale; SSRF allowlist for SERVICE not re-audited; no documented threat model for HTTP companion. |
| Test Coverage | **B−** | 186 pg_regress tests; 12 fuzz targets; nightly fuzz workflow; `cwb_write_path_equivalence.sql` and `rls_promotion.sql` added. | No `v067_features.sql` / `v069_features.sql`; bulk-load CWB not testable until C1 closed; no Arrow Flight HTTP-level integration test. |
| Documentation & DX | **C** | CHANGELOG entries detailed; per-version roadmap files exist. | README two releases stale; SHACL-SPARQL overclaim persists; SBOM stale; v0.67.0 roadmap file marked "Planned"; missing reference docs. |
| CI/CD & Release Process | **B−** | Python gate scripts require `--version`; `validate-feature-status` job exists; SBOM verify in release flow (claimed). | SBOM regeneration claim contradicted by checked-in artifact; benchmark trend gate exists but has only one historical row. |
| Architecture & Long-term Health | **B−** | Mutation journal contract is the right abstraction; module restructuring respects single responsibility. | Side effects still attached at function-call boundary, not transaction-end callback; `pgrx::SubtransactionGuard`/`xact_callback` not used. |
| Ecosystem Completeness | **B−** | dbt adapter shipped; HTTP companion has Datalog REST + Arrow Flight + Prometheus metrics; vector + SPARQL hybrid search; GraphRAG export. | No SPARQL 1.2 tracking; no live query subscription API; no R2RML virtual graph layer (mapping exists but materialized); no graph visualization beyond explorer SPA. |

## Critical Findings (Severity: Critical)

### CF-1 — Bulk load bypasses the mutation journal; CONSTRUCT writeback silently stale after Turtle/N-Triples imports

**Location**: [src/bulk_load.rs](../src/bulk_load.rs#L100-L127) `flush_batch`; [src/storage/mod.rs](../src/storage/mod.rs#L618-L690) `batch_insert_encoded` (no `mutation_journal::*` calls anywhere in the function body); [src/storage/mutation_journal.rs](../src/storage/mutation_journal.rs#L20) doc comment says "Bulk loaders call `flush()` once after all rows are inserted" — implementation does not honor this.

**Description**: The v0.67.0 changelog claims the mutation journal "unifies CONSTRUCT writeback across all write paths: `insert_triple`, SPARQL INSERT DATA, `load_ntriples`, and `load_turtle`." The implementation is true for the first two but false for the bulk loaders. `load_ntriples`, `load_turtle`, `load_rdfxml`, `load_jsonld`, `load_nquads`, `load_trig`, and `load_ntriplesstar` (counted from `flush_batch` call sites at [bulk_load.rs](../src/bulk_load.rs#L192), [L201](../src/bulk_load.rs#L201), [L230](../src/bulk_load.rs#L230), [L236](../src/bulk_load.rs#L236), [L261](../src/bulk_load.rs#L261), [L267](../src/bulk_load.rs#L267), [L296](../src/bulk_load.rs#L296), [L302](../src/bulk_load.rs#L302), [L432](../src/bulk_load.rs#L432), [L438](../src/bulk_load.rs#L438), [L465](../src/bulk_load.rs#L465), [L472](../src/bulk_load.rs#L472), [L497](../src/bulk_load.rs#L497), [L503](../src/bulk_load.rs#L503), [L528](../src/bulk_load.rs#L528), [L534](../src/bulk_load.rs#L534)) all flow through `batch_insert_encoded`, which writes directly to `_pg_ripple.vp_*_delta` and `_pg_ripple.vp_rare` and never touches `mutation_journal`.

**Impact**: A user who registers a CONSTRUCT writeback rule and then bulk-loads source-graph triples will observe an empty target graph until they manually call `refresh_construct_rule()` or invoke `pg_ripple.insert_triple()` for one additional triple. This is precisely the v0.65.0 / Assessment 9 problem with one path now fixed. The tests that purport to prove the equivalence ([cwb_write_path_equivalence.sql](../tests/pg_regress/sql/cwb_write_path_equivalence.sql)) cannot exercise the bulk-load path because the hook is not invoked.

**Recommended fix**: Add `mutation_journal::record_write(g_id)` calls inside the batch-flush loop of `batch_insert_encoded` (cheap due to `has_no_rules()` fast-path), and add `mutation_journal::flush()` at the end of each `load_*` function in `bulk_load.rs`. Then add an end-to-end pg_regress test that creates a rule, calls `load_turtle()` with N source triples, and asserts the derived target graph is non-empty without calling `refresh_construct_rule`.

### CF-2 — Per-triple `mutation_journal::flush()` runs the entire CWB pipeline once per quad, not once per statement

**Location**: [src/storage/mod.rs](../src/storage/mod.rs#L1692-L1693) `insert_triple_by_ids`; [src/storage/mod.rs](../src/storage/mod.rs#L1791-L1792) `delete_triple_by_ids`; [src/dict_api.rs](../src/dict_api.rs#L229-L230) `insert_triple` (also flushes per-call).

**Description**: The journal kernel's documented design is correct: accumulate per-statement, dedupe graph IDs, then flush once. The wire-up violates that design. `insert_triple_by_ids` calls `record_write` immediately followed by `flush()` for every single quad. SPARQL Update for an `INSERT DATA { ... }` block of 100 triples will therefore call `construct_rules::on_graph_write(iri)` 100 times, each of which re-runs the full incremental CONSTRUCT delta query for every dependent rule. A 10-rule pipeline with 100-quad insert becomes 1,000 SPARQL evaluations.

**Impact**: For workloads with even a modest CONSTRUCT rule count, latency for `INSERT DATA` and SPARQL Update scales as O(quads × rules) rather than O(rules) per statement. This makes v0.65.0's incremental claim correct semantically but quadratically expensive. Combined with CF-1, the only currently-fast write path is the one that doesn't fire the hook.

**Recommended fix**: Replace per-call `flush()` with deferral to statement end. Two options: (a) register a `RegisterXactCallback` for `XACT_EVENT_PRE_COMMIT` that flushes the journal once; (b) have `sparql_update` and the public `insert_triple` SQL wrappers explicitly call `flush()` after the entire statement's quads have been inserted, with the per-call `flush()` removed from storage. Either way, dedupe affected graph IDs across the journal before invoking CWB.

### CF-3 — SHACL-SPARQL rule documentation overclaims for the third consecutive assessment

**Location**: [docs/src/features/shacl-sparql-rules.md](../docs/src/features/shacl-sparql-rules.md#L1-L80) ("`pg_ripple` supports both `sh:SPARQLConstraint` (validation) and `sh:SPARQLRule` (inference)"); [src/shacl/af_rules.rs](../src/shacl/af_rules.rs#L171-L175) (warns and skips when `sh:SPARQLRule` is detected); [src/feature_status.rs](../src/feature_status.rs#L136-L150) honestly says `planned`.

**Description**: This finding has appeared in Assessment 8 (SC1), Assessment 9 (H4 / SC1), and is unchanged in v0.69.0. Users modeling SHACL inference workflows will load a shapes graph that includes `sh:SPARQLRule`, see no error, and assume their derivations are running. Only a backend log warning indicates otherwise.

**Impact**: Silent semantic failure in a security/governance-adjacent feature. SHACL-AF rules are commonly used in regulated environments to enforce derivation invariants — silent skipping is a correctness violation that can land in audit reports.

**Recommended fix**: Either delete `docs/src/features/shacl-sparql-rules.md` and explicitly document that only `sh:TripleRule` is supported in the SHACL feature page, or implement `sh:SPARQLRule` end-to-end and add a regression test. A middle ground: rename the page to `shacl-rules.md`, restrict its scope to TripleRule, and add a "Not yet supported" callout for `sh:SPARQLRule` linking to a tracking issue.

### CF-4 — `feature_status()` cites evidence files that do not exist; v0.67.0 GATE-02 CI job either passes silently or is informational

**Location**: [src/feature_status.rs](../src/feature_status.rs#L192) `docs/src/reference/scalability.md` (×6 occurrences at L192, L208, L223, L237, L251, L265); [L247](../src/feature_status.rs#L247) `tests/integration/citus_rls_propagation.sh`; [L280](../src/feature_status.rs#L280) `docs/src/reference/arrow-flight.md`. Static `file_search` confirms none of these files exist.

**Description**: v0.67.0 GATE-02 added a `validate-feature-status` CI job that "verifies evidence paths exist on disk." If that check were enforced, the v0.67.0 build would fail because `docs/src/reference/scalability.md` does not exist. Since v0.67.0, v0.68.0, and v0.69.0 all tagged successfully, the check must be either (a) skipping these specific paths, (b) treating a `None`-or-missing path as a soft warning, or (c) running with `continue-on-error: true`. In all three cases the release-truth premise is broken.

**Impact**: Release dashboards and the `pg_ripple.feature_status()` SQL function — which exist precisely to give operators a machine-readable source of truth — are misleading by construction. This is the exact failure mode Assessment 8 (A1) and Assessment 9 (A3, A4) called out, and v0.67.0 announced it as fixed.

**Recommended fix**: Inspect `.github/workflows/ci.yml` `validate-feature-status` job; either make it fail closed on missing evidence and create the three files, or drop the citations from `feature_status.rs`. Add a smoke test in `tests/pg_regress/sql/feature_status.sql` that asserts every non-NULL `evidence_path` returned by `feature_status()` exists relative to the workspace root.

## High-Severity Findings

### HF-1 — Citus-end-to-end claim depends on a missing integration script

**Location**: [src/feature_status.rs](../src/feature_status.rs#L247) cites `tests/integration/citus_rls_propagation.sh`; the file does not exist. [src/citus.rs](../src/citus.rs) helpers exist; SERVICE annotation hook exists; HLL translation wired. However no multi-node test harness is checked into the repository to prove these compose correctly.

**Impact**: Distributed-deployment claims are not independently reproducible. A regression in any of the four wired pieces (HLL aggregate, SERVICE annotation, BRIN summarise, RLS propagation) would not be caught by CI.

**Fix**: Either implement the integration test using the existing `docker-compose.yml` (which already provides PG18 + Citus) or remove the evidence citation from `feature_status.rs` until the test exists.

### HF-2 — README is two releases stale

**Location**: [README.md](../README.md#L33) ("What works today (v0.67.0)"), [README.md](../README.md#L353) ("Known limitations in v0.67.0"). Current release is v0.69.0 ([Cargo.toml](../Cargo.toml#L4)).

**Impact**: New users decide whether v0.69.0 fixes the limitations they care about by reading v0.67.0 caveats. The "Known limitations" section may list v0.67.0-era issues that v0.68.0 or v0.69.0 actually addressed (e.g., portal-based CONSTRUCT cursors).

**Fix**: Update README at every release. Add a `scripts/check_readme_version.sh` that diffs the README header version against `Cargo.toml` and fails CI on drift. Wire into `just assess-release`.

### HF-3 — Arrow Flight streaming behavior under large exports is unverified

**Location**: [pg_ripple_http/src/arrow_encode.rs](../pg_ripple_http/src/arrow_encode.rs) (~400 lines per the subagent inventory); v0.67.0 FLIGHT-SEC-02 claims `arrow_batch_size`-driven batch streaming with `x-arrow-batches` header.

**Description**: Static review confirms HMAC verification, unsigned-ticket rejection, and tombstone-exclusion query construction are present. What is **not** verified is whether the response body is built via `Body::from_stream(...)` over a true `Stream<Item = Result<Bytes, _>>`, or whether the SPI cursor materializes all batches into a `Vec<u8>` and the response is `Body::from(buf)`. The Assessment 9 evidence at [pg_ripple_http/src/main.rs:2118-2198](../pg_ripple_http/src/main.rs) (pre-v0.69 split) was the second pattern.

**Impact**: A 50-million-row Arrow export that buffers in memory will OOM the HTTP companion long before the database is the bottleneck. With memory exhaustion and no streaming back-pressure, this is an availability gap, not just performance.

**Fix**: Confirm `Body::from_stream` over `tokio_stream::wrappers` or similar. Add an HTTP-level integration test that exports 10M triples through the endpoint and asserts process RSS stays under a configurable bound. Document the memory ceiling in user-facing reference.

### HF-4 — RLS DDL interpolates role names without quoting; potential SQL injection via `_pg_ripple.graph_access` writes

**Location**: [src/security_api.rs](../src/security_api.rs#L94-L100) (`format!("CREATE POLICY ... TO {role} USING (g = {graph_id})")`); [L172-L178](../src/security_api.rs#L172-L178) (`apply_rls_policy_to_all_dedicated_tables` interpolates `role` and `pg_privilege` directly); [L40-L60](../src/security_api.rs#L40-L60) (`apply_rls_to_vp_table` reads role from `_pg_ripple.graph_access` and interpolates raw).

**Description**: The `role` parameter is interpolated into DDL via `format!` without identifier quoting. While `do_grant_graph_access` is normally called via SQL pgrx wrappers (which themselves came through PostgreSQL parsing, so the role string can be bizarre but not arbitrary SQL), the secondary path `apply_rls_to_vp_table` reads role names back out of `_pg_ripple.graph_access` and interpolates them at promotion time. A user with INSERT on `_pg_ripple.graph_access` could inject SQL into a future promotion event. `_pg_ripple` is the internal schema and not usually granted to non-superusers, but the table is referenced by RLS and must remain readable; the threat boundary deserves explicit review.

**Impact**: In the worst case (user has DML on `_pg_ripple.graph_access` and controls promotion timing), arbitrary SQL execution at promotion time. In the realistic case, malformed role names cause confusing DDL failures.

**Fix**: Use `quote_ident` on role names before interpolation; validate role names with a regex (PostgreSQL identifiers) before storing in `_pg_ripple.graph_access`; add a SECURITY-DEFINER trigger that enforces validation. Add a `tests/pg_regress/sql/security_rls_role_injection.sql` that attempts to inject and asserts rejection.

### HF-5 — SBOM in repository is 18 releases stale despite v0.67.0 SBOM-01 claim

**Location**: [sbom.json](../sbom.json#L14) reports `pg_ripple@0.51.0`; [sbom_diff.md](../sbom_diff.md) is for v0.60.0→v0.61.0. v0.67.0 SBOM-01 claims release CI verifies the SBOM component version matches `Cargo.toml` and fails if not.

**Description**: Either the release CI step does not actually fail-close on mismatch, or the regenerated SBOM is uploaded only as a release asset and the in-tree `sbom.json` is never updated, or the check is silently skipped. In all cases, anyone using the in-tree SBOM for security review will see a v0.51.0 dependency closure.

**Impact**: Security reviewers cannot rely on the checked-in SBOM. Downstream packagers (Linux distros, container images) that consume `sbom.json` ship 18 releases of dependency drift.

**Fix**: Update `sbom.json` from CI on every release tag and commit the result back to `main` via a release-bot PR; add a `just check-sbom-version` step that compares `sbom.json` to `Cargo.toml` and is part of `just assess-release`.

### HF-6 — Plan cache key omits graph context and security context; cached plan reuse across roles/graphs

**Location**: Subagent quotes `cache_key()` constructing keys from `query_text`, `MAX_PATH_DEPTH`, and `BGP_REORDER` GUCs. Graph bindings, current role, and RLS-relevant context are not part of the key.

**Description**: Two queries with the same SPARQL text but different `current_role` (and therefore different effective RLS policies) share the same cached SQL plan. Result filtering is correct because RLS runs at SQL execution time. However, if the cached plan was specialized for one path (e.g., predicate-pushdown that became safe only because the prior role had access to all graphs), the next role might see a slower plan or — if the optimizer ever takes a privileged shortcut — a wrong-result plan.

**Impact**: Today: probably correct because PG enforces RLS at executor level regardless of plan shape. Future-risk: any optimizer change in pg_ripple that uses RLS-derived assumptions (e.g., "graph X is empty for this role, skip the join") would silently break.

**Fix**: Document the cache-key contract explicitly. Either add `current_role` and a hash of active GUCs that affect translation (not just two), or add a regression test that runs the same query under two roles with different RLS grants and asserts identical results.

### HF-7 — Mutation journal flush is not transaction-safe under SAVEPOINT/ROLLBACK

**Location**: [src/storage/mutation_journal.rs](../src/storage/mutation_journal.rs#L40-L120). `JOURNAL` is a `thread_local! RefCell<Vec<JournalEntry>>` cleared in `flush()` but **not** cleared on rollback.

**Description**: PostgreSQL allows `SAVEPOINT` / `ROLLBACK TO SAVEPOINT`. If a SPARQL Update inside a savepoint records writes and then the savepoint is rolled back, the storage rows are gone but `flush()` (already called per-quad — see CF-2) has already invoked CONSTRUCT writeback. Even with the per-call flush, derived triples have been written into target graphs based on now-rolled-back source rows. The journal does not register an `XactCallback`/`SubXactCallback` to clear-on-abort.

**Impact**: CONSTRUCT writeback can produce derived triples not supported by any source triple after a rollback. Subsequent retraction (DRed) may not reach them because the provenance row was either also rolled back or remains pointing to a missing source row.

**Fix**: Register `RegisterSubXactCallback` and `RegisterXactCallback` in `_PG_init` to clear the journal on abort and run flush once on `XACT_EVENT_PRE_COMMIT`. This combined with CF-2's fix gives a single coherent design.

### HF-8 — `pg_ripple_http` independently versioned at 0.16.0 with no documented compatibility matrix

**Location**: [pg_ripple_http/Cargo.toml](../pg_ripple_http/Cargo.toml#L2) `version = "0.16.0"`. CHANGELOG and release notes describe extension and HTTP changes together (e.g., FLIGHT-SEC-02 affects both), but the version number for the HTTP companion is decoupled and not pinned to extension version.

**Impact**: Operators upgrading `pg_ripple` to v0.69.0 do not know which `pg_ripple_http` tag is compatible. A `pg_ripple_http` 0.15 binary against a v0.69 extension may miss the v0.67.0 tombstone-exclusion query path, or vice versa.

**Fix**: Either lockstep the versions (`pg_ripple_http` 0.69.0 alongside `pg_ripple` 0.69.0) or publish a compatibility matrix in `docs/src/operations/`. If decoupled, add a startup check that the HTTP companion calls `pg_ripple.feature_status()` and refuses to start if the extension version is incompatible.

## Medium-Severity Findings

### MF-1 — `cwb_write_path_equivalence.sql` cannot prove what it claims

**Location**: [tests/pg_regress/sql/cwb_write_path_equivalence.sql](../tests/pg_regress/sql/cwb_write_path_equivalence.sql) cited as evidence for MJOURNAL-01/02/03. Because bulk load does not fire the hook (CF-1), any equivalence assertion across `insert_triple`, SPARQL Update, and bulk load must either be wrong or skip the bulk-load arm.

**Fix**: Inspect the test; either expand it to fail loudly when bulk load returns the wrong derived count (forcing CF-1 fix), or rename to `cwb_two_path_equivalence.sql` and document the gap.

### MF-2 — Legacy `check_roadmap_evidence.sh` and `check_api_drift.sh` still exist alongside Python replacements

**Location**: [scripts/check_roadmap_evidence.sh](../scripts/check_roadmap_evidence.sh) and [scripts/check_api_drift.sh](../scripts/check_api_drift.sh) coexist with `.py` versions. Risk: any caller (justfile, CI step, contributor) that still invokes `.sh` runs the old broken behavior.

**Fix**: Delete the `.sh` versions or replace their bodies with `exec python3 scripts/check_roadmap_evidence.py "$@"`.

### MF-3 — Plan-cache reset SQL function lacks documentation linkage

**Location**: `plan_cache_reset` and `plan_cache_stats` are exported (per subagent re-export list) but the user-facing docs do not document them.

**Fix**: Add a one-page reference under `docs/src/reference/plan-cache.md` and cite it from `feature_status.rs`.

### MF-4 — `recover_interrupted_promotions()` has no regression test simulating crash

**Location**: [src/storage/mod.rs](../src/storage/mod.rs) `recover_interrupted_promotions` (v0.68.0 PROMO-01); no test under `tests/pg_regress/sql/` matches `recover_*` or `promotion_recovery_*`. Listing of test directory shows no such file.

**Impact**: Crash recovery for the new nonblocking promotion path is unverified.

**Fix**: Add a regression test that artificially sets `promotion_status='promoting'`, runs `recover_interrupted_promotions()`, and asserts completion.

### MF-5 — `merge_throughput_history.csv` has only one historical row; trend gate cannot fire

**Location**: [benchmarks/merge_throughput_history.csv](../benchmarks/merge_throughput_history.csv) contains only `v0.59.0`. v0.67.0 BENCH-02 claims a weekly performance trend workflow that fails on >10% drop vs 4-week rolling average.

**Impact**: Either the workflow is not running, or it ran and did not append rows. Trend regression detection is non-functional in either case.

**Fix**: Schedule the workflow on protected branches; verify it commits results back to `main`; backfill historical rows from prior CI runs if available.

### MF-6 — `v067_features.sql` and `v069_features.sql` regression tests do not exist

**Location**: `tests/pg_regress/sql/` has `v066_features.sql` and `v068_features.sql`; no v067 or v069 file. The v0.67.0 changelog cites `cwb_write_path_equivalence.sql` and `rls_promotion.sql` as evidence; v0.69.0 changelog says "All 186 pg_regress tests pass" but adds none.

**Impact**: Future-version regressions in v0.67.0 or v0.69.0 features have no dedicated guard.

**Fix**: Add `v067_features.sql` covering ARROW_UNSIGNED rejection, mutation journal flush behavior, and Python gate scripts; add `v069_features.sql` covering the public re-export contract from `src/sparql/mod.rs` and `src/construct_rules/mod.rs`.

### MF-7 — Citus HLL accuracy bounds undocumented and untested

**Location**: [src/sparql/translate/group.rs](../src/sparql/translate/group.rs#L240-L246). HLL standard error is roughly 0.81/sqrt(2^precision); the chosen precision (default 14 in PG `hll`) gives ~0.02% error, but pg_ripple does not document or test it.

**Fix**: Add `docs/src/reference/approximate-aggregates.md` with error bounds; add a test that `COUNT(DISTINCT)` with HLL is within X% of exact for 1M-row dataset.

### MF-8 — Citus SERVICE shard annotation effectiveness not benchmarked

**Location**: [src/sparql/translate/graph.rs](../src/sparql/translate/graph.rs#L364-L366). Hook is wired; effect on actual shard pruning is not measured.

**Fix**: Add a Citus integration test that issues a SERVICE query with and without `pg_ripple.citus_service_pruning=on` and asserts shard-prune count differs in `EXPLAIN`.

### MF-9 — Decode of unknown dictionary IDs silently produces empty values

**Location**: [src/sparql/decode.rs](../src/sparql/decode.rs#L59-L71) (per subagent quote). Missing dictionary entries result in no row returned and downstream `None` lookups, which cascade to empty/NULL output rather than an error.

**Impact**: A storage-vs-dictionary corruption (e.g., dictionary row deleted but VP table still references the ID) produces silent wrong results in SPARQL output instead of a loud error.

**Fix**: Treat a missing dictionary entry as a hard error in non-development mode (via a GUC like `pg_ripple.strict_dictionary=on`); add a regression test for the corruption case.

### MF-10 — `src/lib.rs` (~2,300 lines) and `src/storage/mod.rs` (~2,100 lines) remain too large after v0.69.0 split

**Location**: Subagent line count: `src/lib.rs` ~2,262, `src/storage/mod.rs` ~2,104, `src/sparql/mod.rs` ~1,869 (despite v0.69.0 split), `src/datalog/compiler.rs` ~1,612, `src/sparql/federation.rs` ~1,519, `src/sparql/expr.rs` ~1,498, `src/export.rs` ~1,335, `src/views.rs` ~1,284.

**Impact**: Cross-cutting changes (RLS, observability, tracing) remain easy to wire into one path and miss in another — the same Assessment 8 root cause.

**Fix**: Continue the v0.69.0 program. Specific candidates: split GUC registrations out of `src/lib.rs` into `src/gucs/registration.rs`; split `src/storage/mod.rs` along `dictionary_io.rs`, `htap_io.rs`, `vp_rare_io.rs`. The `src/sparql/mod.rs` 1,869 lines is misleading because most are re-exports and the 3 SQL entry-point bodies — confirm and either accept or extract those bodies.

### MF-11 — Property-based tests not present for `ConstructTemplate` / `apply_construct_template`

**Location**: v0.68.0 added `prepare_construct()`/`apply_construct_template()` in `src/sparql/mod.rs` per CHANGELOG. No proptest module under `src/sparql/` matches `construct_template`.

**Fix**: Add a proptest that round-trips arbitrary CONSTRUCT templates and binding sets and asserts deterministic output.

### MF-12 — Fuzz corpus for `sparql_parser` may not exercise SPARQL Update grammar

**Location**: [fuzz/fuzz_targets/sparql_parser.rs](../fuzz/fuzz_targets/sparql_parser.rs) exists. Whether the corpus and harness drive `parse_update` (vs only `parse_query`) is not verified statically.

**Fix**: Add a separate `fuzz_targets/sparql_update.rs` that calls `spargebra::SparqlParser::new().parse_update`; seed the corpus with valid SPARQL Update fragments.

### MF-13 — Streaming observability counters increment but are not exposed via `pg_ripple_http /metrics`

**Location**: Assessment 9 HF-5 noted the same gap. Subagent confirms `pg_ripple_http/src/metrics.rs` is ~96 lines (Prometheus counters), and the v0.67.0 FLIGHT-SEC-02 changelog says `arrow_batches_sent` is added to extension `streaming_metrics()`. The HTTP /metrics endpoint and the in-extension metric are two different counters; cross-process aggregation is not documented.

**Fix**: Either expose extension `streaming_metrics()` JSON via a `/metrics/extension` route, or document that operators must scrape both.

### MF-14 — `pg_ripple.export_batch_size` interaction with `pg_ripple.arrow_batch_size` and `pg_ripple.vp_promotion_batch_size` not documented

**Location**: Three new batch-size GUCs added in v0.66/v0.67/v0.68 with overlapping semantics. No comparative reference exists in `docs/src/reference/`.

**Fix**: Add a one-page GUC reference cross-linking the three.

### MF-15 — `[Unreleased]` CHANGELOG section is empty since v0.69 release; new contributors have no guidance on where to add entries

**Location**: [CHANGELOG.md](../CHANGELOG.md#L8-L13).

**Fix**: Add a comment block explaining the convention; consider a CONTRIBUTING.md note.

### MF-16 — `roadmap/v0.67.0.md` still marked `Status: Planned` two releases post-tag

**Location**: Subagent quotes `roadmap/v0.67.0.md` line 5: `**Status: Planned** | **Scope: Very Large**`.

**Fix**: Add an automated check that compares each `roadmap/v*.md` `Status:` against the current release tag and fails if a released version is still "Planned".

### MF-17 — Datalog/CWB interaction undocumented and untested

**Description**: When a Datalog rule derives a new triple (via the lattice / fixed-point engine in [src/datalog/lattice.rs](../src/datalog/lattice.rs)), it writes into VP tables. Whether that write goes through `storage::insert_triple_by_ids` (and therefore fires CWB) is not stated in the architecture docs.

**Fix**: Confirm code path, document, add a regression test where a Datalog rule produces triples that should re-trigger a CWB rule downstream.

### MF-18 — `is_citus_worker_endpoint()` URL parsing untested for edge cases

**Location**: [src/citus.rs](../src/citus.rs) `is_citus_worker_endpoint`/`extract_url_host` (per subagent). No fuzz target for URL host extraction.

**Fix**: Add unit tests for IPv6, IDN, port-only, malformed URLs.

### MF-19 — Arrow Flight ticket has no replay protection beyond expiry

**Location**: v0.66 FLIGHT-01 added `nonce` field but the validator only checks signature + expiry + audience. A ticket replayed within its expiry window is accepted multiple times.

**Fix**: Maintain a server-side seen-nonce LRU cache (size bounded by max-tickets-per-expiry-window) and reject replays.

### MF-20 — `pg_ripple.feature_status()` taxonomy includes `experimental`, `planner_hint`, `manual_refresh`, `degraded`, `stub`, `planned`, `implemented` — no documented promotion criteria

**Fix**: Add `docs/src/reference/feature-status-taxonomy.md` with the criteria for each status.

## Low-Severity Findings and Enhancements

- The `mutation_journal::flush()` doc comment ([src/storage/mutation_journal.rs](../src/storage/mutation_journal.rs#L77-L84)) lists three call-site contracts; only two are honored. Update the comment to match reality or fix the implementation.
- [src/storage/mod.rs](../src/storage/mod.rs#L1683-L1685) doc comment "Direct callers must be the mutation journal flush function only" is misleading — the function calls the journal, not the other way around.
- `apply_rls_to_vp_table` ([src/security_api.rs](../src/security_api.rs#L40-L60)) silently drops policy creation errors via `let _ = ...`; convert to `pgrx::warning!` so operators see promotion-time RLS failures.
- [src/security_api.rs](../src/security_api.rs#L94) policy name is constructed from a 64-bit xxh3 hash; collision probability across 4B graphs is non-negligible; document or use a 128-bit suffix.
- [pg_ripple_http/Cargo.toml](../pg_ripple_http/Cargo.toml) does not pin `arrow` minor version; `arrow = "55"` allows 55.x and may break across IPC schema changes.
- The 12 fuzz targets run for 60s nightly per the v0.68.0 FUZZ-01 description; that is too short for meaningful coverage growth. Recommend 600s/target nightly and 14 400s/target weekly.
- [benchmarks/merge_throughput_baselines.json](../benchmarks/merge_throughput_baselines.json) baseline is stamped v0.53.0 but the file references current HTAP shape; refresh.
- [docs/src/features/shacl-sparql-rules.md](../docs/src/features/shacl-sparql-rules.md) — see CF-3.
- `tests/pg_regress/sql/` has 186 `.sql` and 186 `.out` files (subagent count); for a project this size (~25k lines of Rust) target is closer to 300+.
- The `Body::from_stream` runtime check would benefit from a runtime memory-bound assertion macro at endpoint level.
- README claims "continuous fuzzing" — v0.68.0 FUZZ-01 makes this technically true, but README does not link to `fuzz.yml`.
- [src/feature_status.rs](../src/feature_status.rs) lacks an entry for `mutation_journal` itself.
- No SBOM regeneration commit hook means the `sbom.json` will continue to drift; add a release-bot.
- Chart in [charts/pg_ripple/](../charts/pg_ripple) Helm chart should pin the `pg_ripple_http` image tag to a release SHA, not `latest`.
- `pg_ripple.control` lacks a `comment` describing v0.69.0 highlights.
- `roadmap/v0.69.0.md` correctly marked Released; `v0.67.0.md` is the only anomaly.
- [Cargo.toml](../Cargo.toml) does not pin a `rust-toolchain` version pin verified against [rust-toolchain.toml](../rust-toolchain.toml); ensure these agree.
- `CONTRIBUTING.md` is not present in the workspace listing; add one.
- The justfile `assess-release` recipe should print which evidence paths were validated to give operators a record.
- `pg_ripple_http` `/metrics` route auth model is not documented; verify whether it is accessible without `check_auth`.
- `src/llm/` and `src/kge.rs` modules exist but are not represented in `feature_status.rs`; either document or remove.
- `src/r2rml.rs` exists; the assessment scope says "no R2RML virtual graph layer" — clarify whether this is materialization-only or whether virtual semantics are planned.

## Positive Developments Since Assessment 9

- **Mutation journal kernel** ([src/storage/mutation_journal.rs](../src/storage/mutation_journal.rs)) is the right architectural primitive and the `has_no_rules()` fast-path is correctly placed. SPARQL Update path is genuinely fixed.
- **RLS propagation to promoted VP tables** ([src/storage/promote.rs](../src/storage/promote.rs#L52-L60), [src/security_api.rs](../src/security_api.rs#L139-L182)) fully closes Assessment 9 S4 / CF-2 for the SQL-API path.
- **Arrow Flight unsigned-ticket rejection** ([pg_ripple_http/src/arrow_encode.rs](../pg_ripple_http/src/arrow_encode.rs#L45-L54)) closes S2 with a clean default-deny posture.
- **Python gate scripts** ([scripts/check_roadmap_evidence.py](../scripts/check_roadmap_evidence.py), [scripts/check_api_drift.py](../scripts/check_api_drift.py)) require `--version` and are fail-closed — exactly the Assessment 9 prescription.
- **v0.69.0 module restructuring** is meaningful: `pg_ripple_http/src/main.rs` from 2,206 lines down to ~250; `src/construct_rules.rs` split into a directory; `src/sparql/` separated into parse/plan/decode/execute. This is the rare large refactor done with zero behavioral changes.
- **Nonblocking VP promotion with crash recovery** ([src/storage/promote.rs](../src/storage/promote.rs), v0.68.0 PROMO-01) introduces `promotion_status` tracking and `recover_interrupted_promotions()`.
- **Citus HLL aggregate translation** is wired ([src/sparql/translate/group.rs](../src/sparql/translate/group.rs#L240-L246)) and falls back cleanly when the `hll` extension is absent.
- **Portal-based CONSTRUCT cursor streaming** (v0.68.0 STREAM-01) closes Assessment 9 H3 for CONSTRUCT helpers.
- **Production `panic!` removed** in `construct_rules` topological sort (v0.67.0 PANIC-01).
- **`validate-feature-status` CI job exists** (v0.67.0 GATE-02) — even if currently incomplete, the framework is in place.
- **GitHub Actions SHA pinning** remains clean across 8 workflows.
- **CHANGELOG.md dates are now chronologically consistent** (2026-04-29 < 2026-05-06).

## Recommended Roadmap Items

1. **v0.70.0 (Critical, Small)**: Wire `mutation_journal::record_write` into `batch_insert_encoded` and `flush()` at end of every `load_*` function in `bulk_load.rs`. Add bulk-load arm to `cwb_write_path_equivalence.sql`. **Closes CF-1.**
2. **v0.70.0 (Critical, Medium)**: Move `mutation_journal::flush()` from per-call to end-of-statement via `RegisterXactCallback` and `RegisterSubXactCallback`. **Closes CF-2 and HF-7.**
3. **v0.70.0 (Critical, Small)**: Make GATE-02 fail-closed; create or remove the three missing evidence files. **Closes CF-4.**
4. **v0.70.0 (High, Small)**: Either implement `sh:SPARQLRule` or rewrite the docs page. **Closes CF-3.**
5. **v0.70.0 (High, Small)**: Refresh README to v0.69.0; add `scripts/check_readme_version.sh` and `roadmap/*.md` Status check to `just assess-release`. **Closes HF-2 and MF-16.**
6. **v0.70.0 (High, Small)**: Quote-ident role names in RLS DDL paths; add injection regression test. **Closes HF-4.**
7. **v0.70.0 (High, Small)**: Regenerate SBOM in release CI and commit back via release-bot. **Closes HF-5.**
8. **v0.71.0 (High, Medium)**: Confirm Arrow Flight uses `Body::from_stream`; add 10M-row HTTP integration test with RSS bound assertion. **Closes HF-3.**
9. **v0.71.0 (High, Small)**: Implement `tests/integration/citus_rls_propagation.sh` (multi-node) or remove citation. **Closes HF-1.**
10. **v0.71.0 (Medium, Small)**: Add `v067_features.sql` and `v069_features.sql`; add `recover_interrupted_promotions` test. **Closes MF-4 and MF-6.**
11. **v0.71.0 (Medium, Small)**: Document Citus HLL accuracy bounds; add accuracy regression test. **Closes MF-7.**
12. **v0.71.0 (Medium, Small)**: Add `pg_ripple_http`/`pg_ripple` compatibility matrix or version lockstep. **Closes HF-8.**
13. **v0.72.0 (Medium, Medium)**: Continue module split — `src/lib.rs`, `src/storage/mod.rs`, `pg_ripple_http/src/routing.rs`. **Closes MF-10.**
14. **v0.72.0 (Medium, Medium)**: Add proptest for `ConstructTemplate` and a separate fuzz target for SPARQL Update grammar. **Closes MF-11 / MF-12.**
15. **v0.72.0 (Medium, Small)**: Promote W3C SPARQL conformance smoke subset and BSBM regression gate to required CI status; backfill `merge_throughput_history.csv`. **Closes MF-5.**
16. **v0.72.0 (Medium, Small)**: Add nonce-replay protection to Arrow Flight ticket validation. **Closes MF-19.**
17. **v0.73.0 (Medium, Medium)**: SPARQL 1.2 / SPARQL-star tracking issue and design doc.
18. **v0.73.0 (Medium, Medium)**: WebSocket/SSE live SPARQL subscription API.
19. **v1.0.0 (Critical, Large)**: Full Apache Jena suite ≥95% pass rate (currently informational); third-party security audit; documented threat model for `pg_ripple_http`; published memory-bound contracts for streaming endpoints; clean-package install test from checked-in SQL only; multi-tenant deployment guide.

## Appendix A — Files Examined

Reviewed in full or in substantial part during this assessment:

- [AGENTS.md](../AGENTS.md), [README.md](../README.md), [ROADMAP.md](../ROADMAP.md), [CHANGELOG.md](../CHANGELOG.md), [Cargo.toml](../Cargo.toml), [pg_ripple.control](../pg_ripple.control)
- [plans/PLAN_OVERALL_ASSESSMENT_8.md](PLAN_OVERALL_ASSESSMENT_8.md), [plans/PLAN_OVERALL_ASSESSMENT_9.md](PLAN_OVERALL_ASSESSMENT_9.md), [plans/implementation_plan.md](implementation_plan.md)
- [src/storage/mod.rs](../src/storage/mod.rs) (lines 426-427, 618-720, 1670-1820), [src/storage/mutation_journal.rs](../src/storage/mutation_journal.rs) (full), [src/storage/promote.rs](../src/storage/promote.rs)
- [src/sparql/mod.rs](../src/sparql/mod.rs), [src/sparql/parse.rs](../src/sparql/parse.rs), [src/sparql/plan.rs](../src/sparql/plan.rs), [src/sparql/decode.rs](../src/sparql/decode.rs), [src/sparql/execute.rs](../src/sparql/execute.rs) (lines 521-792, 1197-1290), [src/sparql/translate/group.rs](../src/sparql/translate/group.rs), [src/sparql/translate/graph.rs](../src/sparql/translate/graph.rs)
- [src/construct_rules/mod.rs](../src/construct_rules/mod.rs), [src/construct_rules/catalog.rs](../src/construct_rules/catalog.rs), [src/construct_rules/scheduler.rs](../src/construct_rules/scheduler.rs), [src/construct_rules/delta.rs](../src/construct_rules/delta.rs), [src/construct_rules/retract.rs](../src/construct_rules/retract.rs)
- [src/security_api.rs](../src/security_api.rs) (lines 1-200), [src/maintenance_api.rs](../src/maintenance_api.rs)
- [src/feature_status.rs](../src/feature_status.rs) (lines 30-300)
- [src/shacl/af_rules.rs](../src/shacl/af_rules.rs)
- [src/citus.rs](../src/citus.rs)
- [src/bulk_load.rs](../src/bulk_load.rs) (lines 30-540)
- [src/dict_api.rs](../src/dict_api.rs) (lines 197-230)
- [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs), [pg_ripple_http/src/routing.rs](../pg_ripple_http/src/routing.rs), [pg_ripple_http/src/spi_bridge.rs](../pg_ripple_http/src/spi_bridge.rs), [pg_ripple_http/src/arrow_encode.rs](../pg_ripple_http/src/arrow_encode.rs), [pg_ripple_http/src/common.rs](../pg_ripple_http/src/common.rs)
- [scripts/check_roadmap_evidence.py](../scripts/check_roadmap_evidence.py), [scripts/check_api_drift.py](../scripts/check_api_drift.py), [scripts/check_github_actions_pinned.sh](../scripts/check_github_actions_pinned.sh), [scripts/check_no_security_definer.sh](../scripts/check_no_security_definer.sh), [scripts/check_pt_codes.sh](../scripts/check_pt_codes.sh), [scripts/check_migration_headers.sh](../scripts/check_migration_headers.sh)
- [.github/workflows/ci.yml](../.github/workflows/ci.yml), [.github/workflows/fuzz.yml](../.github/workflows/fuzz.yml), [.github/workflows/performance_trend.yml](../.github/workflows/performance_trend.yml), [.github/workflows/benchmark.yml](../.github/workflows/benchmark.yml), [.github/workflows/release.yml](../.github/workflows/release.yml)
- [sql/pg_ripple--0.66.0--0.67.0.sql](../sql/pg_ripple--0.66.0--0.67.0.sql), [sql/pg_ripple--0.67.0--0.68.0.sql](../sql/pg_ripple--0.67.0--0.68.0.sql), [sql/pg_ripple--0.68.0--0.69.0.sql](../sql/pg_ripple--0.68.0--0.69.0.sql)
- [tests/pg_regress/sql/cwb_write_path_equivalence.sql](../tests/pg_regress/sql/cwb_write_path_equivalence.sql), [tests/pg_regress/sql/rls_promotion.sql](../tests/pg_regress/sql/rls_promotion.sql), [tests/pg_regress/sql/v068_features.sql](../tests/pg_regress/sql/v068_features.sql)
- [docs/src/features/shacl-sparql-rules.md](../docs/src/features/shacl-sparql-rules.md)
- [sbom.json](../sbom.json), [sbom_diff.md](../sbom_diff.md)
- [benchmarks/merge_throughput_history.csv](../benchmarks/merge_throughput_history.csv), [benchmarks/merge_throughput_baselines.json](../benchmarks/merge_throughput_baselines.json)

## Appendix B — Programmatic Checks

| Check | Result |
|---|---|
| `Cargo.toml` version vs `pg_ripple.control` `default_version` | **Pass** — both 0.69.0 |
| `pg_ripple_http/Cargo.toml` version | **Note** — 0.16.0 (decoupled) |
| Migration scripts continuity (0.66→0.67→0.68→0.69) | **Pass** — all three present |
| Migration headers (`-- Migration X.Y.Z → A.B.C` form) | **Pass** for the three new scripts (per subagent quote) |
| GitHub Actions SHA pinning | **Pass** — spot-check shows 40-char SHAs across `ci.yml`; subagent counted 0 mutable refs |
| `feature_status()` evidence path existence | **Fail** — `docs/src/reference/scalability.md` (×6), `docs/src/reference/arrow-flight.md`, `docs/src/reference/query-optimization.md`, `tests/integration/citus_rls_propagation.sh` all missing |
| `tests/pg_regress/sql/` count | 186 `.sql` files (matches v0.68.0 changelog claim) |
| `tests/pg_regress/sql/cwb_write_path_equivalence.sql` exists | **Pass** |
| `tests/pg_regress/sql/rls_promotion.sql` exists | **Pass** |
| `tests/pg_regress/sql/v067_features.sql` exists | **Fail** — not present |
| `tests/pg_regress/sql/v069_features.sql` exists | **Fail** — not present |
| `recover_interrupted_promotions` regression test exists | **Fail** — no file matches |
| `[Unreleased]` CHANGELOG section empty | **Pass** |
| `roadmap/v0.67.0.md` Status: line | **Fail** — still "Planned" |
| `roadmap/v0.68.0.md` Status: line | **Pass** — "Released ✅" |
| `roadmap/v0.69.0.md` Status: line | **Pass** — "Released ✅" |
| README "What works today" version | **Fail** — at v0.67.0, two releases stale |
| `sbom.json` component version | **Fail** — 0.51.0, eighteen releases stale |
| Mutation journal call coverage of `insert_triple_by_ids` | **Pass** — [storage/mod.rs:1692-1693](../src/storage/mod.rs#L1692-L1693) |
| Mutation journal call coverage of `delete_triple_by_ids` | **Pass** — [storage/mod.rs:1791-1792](../src/storage/mod.rs#L1791-L1792) |
| Mutation journal call coverage of `batch_insert_encoded` | **Fail** — zero `mutation_journal::*` calls in [storage/mod.rs:618-690](../src/storage/mod.rs#L618-L690) |
| Mutation journal call coverage of `bulk_load.rs` | **Fail** — zero `mutation_journal::*` calls anywhere in [bulk_load.rs](../src/bulk_load.rs) |
| RLS policy applied to dedicated VP delta/main | **Pass** — [storage/mod.rs:426-427](../src/storage/mod.rs#L426-L427), [storage/promote.rs:59-60](../src/storage/promote.rs#L59-L60) |
| RLS DDL identifier quoting | **Fail** — role interpolated raw at [security_api.rs:94-100](../src/security_api.rs#L94-L100) |
| Arrow Flight unsigned-ticket rejection | **Pass** — [arrow_encode.rs:45-54](../pg_ripple_http/src/arrow_encode.rs#L45-L54) |
| HLL COUNT(DISTINCT) translation wired | **Pass** — [translate/group.rs:240-246](../src/sparql/translate/group.rs#L240-L246) |
| SERVICE shard annotation hook wired | **Pass** — [translate/graph.rs:364-366](../src/sparql/translate/graph.rs#L364-L366) |
| Per-triple flush vs per-statement flush | **Fail** — flush at every `insert_triple_by_ids`/`delete_triple_by_ids` call |
| Mutation journal cleared on `ROLLBACK TO SAVEPOINT` | **Fail** — no `RegisterSubXactCallback` |
| Python gate scripts require `--version` | **Pass** |
| Legacy `.sh` gate scripts removed | **Fail** — both `.sh` files still present |
| `validate-feature-status` CI job exists | **Pass** (presence) — but evidence-path check ineffective (see CF-4) |
| `fuzz.yml` workflow exists | **Pass** |
| `performance_trend.yml` workflow exists | **Pass** |
| 12 fuzz targets present | **Pass** |
| `merge_throughput_history.csv` historical rows | **Fail** — only v0.59.0; trend gate cannot fire |
| `cargo audit` / `cargo deny` in CI | **Pass** (per CHANGELOG; runtime not verified) |
| SECURITY DEFINER allowlist | **Pass** (per Assessment 9 carry-forward) |
| PT error code documentation | **Pass** (per Assessment 9; new PT codes since v0.66 not re-verified) |
| `CREATE POLICY IF NOT EXISTS` valid for PG18 target | **Pass** — supported in PostgreSQL 18 |
