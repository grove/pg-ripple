# pg_ripple Overall Assessment 9

Date: 2026-04-28

Scope: This assessment reviewed pg_ripple as declared at v0.66.0 in [Cargo.toml](../Cargo.toml#L4) and [pg_ripple.control](../pg_ripple.control#L5). It covered the prior overall assessments present in [plans/](.), the architecture and release documents, all 139 roadmap Markdown files, the Rust extension source, SQL migrations, pg_regress fixtures, scripts, workflows, fuzz targets, benchmarks, mdBook docs, and the `pg_ripple_http` companion service. The standard applied here is a world-class PostgreSQL extension suitable for third-party security review and production deployment.

## Executive Summary

pg_ripple remains an unusually broad and ambitious PostgreSQL extension. The core architecture is coherent: dictionary-encoded integer joins, VP/HTAP storage, pgrx 0.18 integration, rich pg_regress coverage, migration discipline, SHA-pinned GitHub Actions, and visible effort to make release truth machine-checkable. Several Assessment 8 failures are genuinely improved: GitHub Actions pinning is now clean, the Docker release workflow no longer has the earlier build/push masking pattern, WCOJ is now described more honestly as a planner hint, and SELECT/ASK SPARQL cursors now use PostgreSQL portals.

The most critical gap is still not a missing headline feature; it is execution-path truth. Several features are implemented for one public path but bypassed by another equally valid path. CONSTRUCT writeback hooks run for `pg_ripple.insert_triple()` but not for SPARQL Update or bulk load. Graph RLS policies are created on `_pg_ripple.vp_rare` but not on promoted VP tables. Arrow Flight now emits real Arrow IPC, but the HTTP endpoint accepts unsigned tickets, ignores tombstones, and buffers the full result set in memory. These are production correctness and security failures, not polish issues.

The release-truth layer also needs hardening. The repository has scripts and status APIs intended to prevent overclaiming, but the API drift script depends on GNU `grep -P` and exits successfully when it extracts no functions; the roadmap evidence script examines `[Unreleased]` instead of the latest release and exits successfully. As a result, v0.66.0 can claim implemented evidence while tests only check function existence or JSON key presence.

Production readiness verdict: not production-ready for multi-tenant or externally exposed deployments. The project is in a strong late-alpha/early-beta shape for controlled evaluation, but it still has Critical issues in incremental maintenance, tenant isolation, Arrow export security/correctness, and release gates. The single most important action is to create one storage-level mutation contract that every write path must pass through, then attach CWB, RLS, metrics, and Citus side effects there rather than in selected SQL wrappers.

## Assessment Method

I reviewed the prior assessment files present in the repository: [PLAN_OVERALL_ASSESSMENT.md](PLAN_OVERALL_ASSESSMENT.md), [PLAN_OVERALL_ASSESSMENT_2.md](PLAN_OVERALL_ASSESSMENT_2.md), [PLAN_OVERALL_ASSESSMENT_3.md](PLAN_OVERALL_ASSESSMENT_3.md), [PLAN_OVERALL_ASSESSMENT_4.md](PLAN_OVERALL_ASSESSMENT_4.md), [PLAN_OVERALL_ASSESSMENT_6.md](PLAN_OVERALL_ASSESSMENT_6.md), [PLAN_OVERALL_ASSESSMENT_7.md](PLAN_OVERALL_ASSESSMENT_7.md), and [PLAN_OVERALL_ASSESSMENT_8.md](PLAN_OVERALL_ASSESSMENT_8.md). No `PLAN_OVERALL_ASSESSMENT_5.md` file was present by filename.

I read the authoritative project documents: [AGENTS.md](../AGENTS.md), [implementation_plan.md](implementation_plan.md), [ROADMAP.md](../ROADMAP.md), [CHANGELOG.md](../CHANGELOG.md), and [README.md](../README.md). I generated a one-line index for all 139 files in [roadmap/](../roadmap), then focused detailed verification on v0.55.0 through v0.66.0 and the current Assessment 8 remediation track.

Programmatic checks performed locally:

- Version metadata: [Cargo.toml](../Cargo.toml#L4) and [pg_ripple.control](../pg_ripple.control#L5) agree on `0.66.0`.
- GitHub Actions pinning: `refs=110 mutable=0`; [scripts/check_github_actions_pinned.sh](../scripts/check_github_actions_pinned.sh) passed.
- SECURITY DEFINER lint: [scripts/check_no_security_definer.sh](../scripts/check_no_security_definer.sh) passed with the existing allowlist.
- PT code documentation: [scripts/check_pt_codes.sh](../scripts/check_pt_codes.sh) reported all 43 PT codes documented.
- Migration headers: [scripts/check_migration_headers.sh](../scripts/check_migration_headers.sh) passed across 66 upgrade scripts.
- Test inventory: 183 pg_regress SQL files and 183 expected files, 12 fuzz targets, and 4 proptest modules.
- Local supply-chain tools: `cargo audit` and `cargo deny` were not installed locally; CI installs and runs them, but I did not run a fresh local audit.

The following could not be fully verified without a running PostgreSQL 18/pgrx instance, Citus cluster, and HTTP service: actual isolation behavior under concurrent writes, SPARQL conformance pass rates, HTTP Arrow endpoint runtime behavior, Citus worker propagation, benchmark throughput, and end-to-end migration execution. Findings based on static code are labelled as such where runtime confirmation is still required.

## Resolution of Prior Assessment Findings

| Finding ID from Assessment 8 | Status | Evidence |
|---|---|---|
| C1: CONSTRUCT writeback is not incremental | Partially Resolved | v0.65 added a CWB kernel and public insert/delete hooks in [construct_rules.rs](../src/construct_rules.rs#L955-L963), [dict_api.rs](../src/dict_api.rs#L197-L230), and [views_api.rs](../src/views_api.rs#L435), but SPARQL Update still writes directly through storage in [sparql/mod.rs](../src/sparql/mod.rs#L1047-L1066) and [sparql/mod.rs](../src/sparql/mod.rs#L1278-L1289), while bulk load bypasses the hook in [bulk_load.rs](../src/bulk_load.rs#L100-L125). |
| C2: promoted HTAP predicate retraction can fail | Partially Resolved | CWB retraction now distinguishes promoted tables and `_pg_ripple.vp_rare` in [construct_rules.rs](../src/construct_rules.rs#L930-L947), but that correctness only applies when the CWB hook is invoked. SPARQL Update and bulk-load bypasses leave this unproven end to end. |
| C3: mutable GitHub Actions refs | Resolved | Local scan found 110 action references and 0 mutable refs. [scripts/check_github_actions_pinned.sh](../scripts/check_github_actions_pinned.sh) passed. |
| C4: Arrow Flight stub/unsigned tickets | Partially Resolved | Real Arrow IPC exists in [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs#L2010-L2198), and HMAC fields are generated in [flight.rs](../src/flight.rs#L39-L79). However unsigned tickets are still accepted in [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs#L1976-L1977), tombstones are ignored in [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs#L2092-L2115), and the full result is buffered in memory. |
| H1: WCOJ is not a true triejoin | Resolved for release truth; capability still limited | [feature_status.rs](../src/feature_status.rs#L272-L284) now marks WCOJ as `planner_hint` and explicitly says a true Leapfrog Triejoin executor is not implemented. |
| H2: Citus helpers are not wired end to end | Still Open | [feature_status.rs](../src/feature_status.rs#L179-L239) still marks service pruning, HLL distinct, nonblocking promotion, and multihop pruning as planned or only partially implemented. Some entries also cite missing tests/docs. |
| H3: SPARQL cursors materialize full result sets | Partially Resolved | SELECT/ASK cursors now use `open_cursor`, `detach_into_name`, and `find_cursor` in [cursor.rs](../src/sparql/cursor.rs#L65-L98) and [cursor.rs](../src/sparql/cursor.rs#L123-L224). CONSTRUCT Turtle/JSON-LD cursor helpers still call `sparql_construct()` and collect vectors in [cursor.rs](../src/sparql/cursor.rs#L293-L352). |
| H4: SHACL-SPARQL rule support is overstated | Still Open | Docs still say `pg_ripple supports both sh:SPARQLConstraint and sh:SPARQLRule` in [shacl-sparql-rules.md](../docs/src/features/shacl-sparql-rules.md#L5), while the code warns that `sh:SPARQLRule` is not compiled in [af_rules.rs](../src/shacl/af_rules.rs#L142-L175). |
| H5: Docker release continues after build/push failure | Resolved | The release workflow no longer contains the prior Docker build/push `continue-on-error` masking pattern; workflow pinning also passes. |
| P1: construct-rule provenance over-attributes target triples | Partially Resolved | v0.65 added exact provenance paths in [construct_rules.rs](../src/construct_rules.rs#L955-L963), but incomplete write-path coverage means provenance is not maintained for all valid mutation APIs. |
| P2: Citus HLL COUNT(DISTINCT) not wired | Still Open | HLL availability helpers exist in [citus.rs](../src/citus.rs#L1067-L1087), but aggregate translation still emits exact `COUNT(DISTINCT ...)` in [group.rs](../src/sparql/translate/group.rs#L205-L229). |
| P3: Citus SERVICE pruning not wired | Still Open | `citus_service_pruning` is explicitly `planned` in [feature_status.rs](../src/feature_status.rs#L179-L191). |
| P4: WCOJ overclaim | Resolved for documentation/status | [feature_status.rs](../src/feature_status.rs#L272-L284) now accurately labels WCOJ as a planner hint. |
| P5: Arrow Flight performance/export gap | Partially Resolved | IPC is real, but [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs#L2118-L2198) still fetches all rows, builds full column vectors, writes one batch, and returns one buffered body. |
| S1: GitHub Actions SHA pinning missing | Resolved | Local workflow scan: 110/110 action refs are SHA-pinned. |
| S2: Arrow tickets unsigned | Partially Resolved | Signed tickets exist when a secret is configured in [flight.rs](../src/flight.rs#L39-L79), but unsigned tickets are generated when the secret is empty and accepted by validators in [flight.rs](../src/flight.rs#L112-L118) and [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs#L1976-L1998). |
| S3: construct-rule catalog SQL escaping | Resolved for the catalog path | The CWB catalog write path was refactored in v0.65; no remaining Assessment 8 catalog-escaping issue was found in the reviewed path. Dynamic SQL remains broad elsewhere and still needs a dedicated SQL-injection review before v1.0. |
| S4: distributed RLS/security propagation | Still Open | Graph access policies are created only on `_pg_ripple.vp_rare` in [security_api.rs](../src/security_api.rs#L33-L41) and [maintenance_api.rs](../src/maintenance_api.rs#L214-L242); no Citus RLS propagation implementation was found. |
| SC1: SHACL-SPARQL rule docs overclaim | Still Open | Docs overclaim support in [shacl-sparql-rules.md](../docs/src/features/shacl-sparql-rules.md#L43-L68); implementation warns and skips SPARQL rules in [af_rules.rs](../src/shacl/af_rules.rs#L171-L175). |
| SC2: CWB test matrix missing | Partially Resolved | [construct_rules.sql](../tests/pg_regress/sql/construct_rules.sql) exists, but it does not exercise SPARQL Update or bulk-load write paths. |
| SC3: version/date/API narrative drift | Still Open | [README.md](../README.md#L22) still says v0.63.0, [CHANGELOG.md](../CHANGELOG.md#L16) dates v0.66.0 to 2026-04-29, and [roadmap/v0.64.0.md](../roadmap/v0.64.0.md#L5) still says `Status: Planned`. |
| SC4: WCOJ truth drift | Resolved | WCOJ is now explicitly described as `planner_hint` in [feature_status.rs](../src/feature_status.rs#L272-L284). |
| A1: release claims need evidence | Still Open | [scripts/check_roadmap_evidence.sh](../scripts/check_roadmap_evidence.sh#L39-L44) targets the first changelog section, which is `[Unreleased]` in [CHANGELOG.md](../CHANGELOG.md#L8-L16), and exits 0 when extraction fails. |
| A2: construct-rule manual escaping | Resolved for the target area | The prior construct-rule catalog escaping risk appears addressed; no re-opened evidence was found in the current catalog path. |
| A3: no dead-helper/status claims | Still Open | [feature_status.rs](../src/feature_status.rs#L205-L239) cites missing docs/tests and claims Citus BRIN/RLS evidence not present in the tree. |
| A4: release/dashboard dead-helper claims | Still Open | Status/docs still claim evidence paths that do not exist, including `docs/src/reference/arrow-flight.md`, `docs/src/reference/scalability.md`, and `tests/integration/citus_rls_propagation.sh`. |

## Current Maturity Assessment

| Area | Maturity Rating | Evidence | Key Remaining Gap |
|---|---|---|---|
| Correctness & Semantic Fidelity | C+ | Broad pg_regress surface and mature SPARQL/Datalog modules exist, but CWB and SHACL-SPARQL claims diverge from execution paths. | One write-path contract and verified semantic conformance gates. |
| Bugs & Runtime Safety | B- | Explicit production `panic!` count is low; one visible panic remains in [construct_rules.rs](../src/construct_rules.rs#L242). | Runtime stress tests for update/merge/CWB concurrency are still needed. |
| Code Quality & Maintainability | C+ | Architecture is clear, but several modules exceed 1,100-2,300 lines. | Split storage, SPARQL, HTTP, and CWB modules around narrower contracts. |
| Performance & Scalability | C | HTAP, batching, cursors, and Citus helpers exist. | WCOJ, Citus pruning/HLL, Arrow export, and benchmark gates are not end-to-end production mechanisms. |
| Security | C+ | Actions are pinned and SECURITY DEFINER lint passes. | Tenant RLS and Arrow ticket validation have Critical gaps. |
| Test Coverage | B- | 183 pg_regress fixtures, 12 fuzz targets, and 4 proptest modules. | Recent features are often smoke-tested rather than behavior/negative/stress-tested. |
| Documentation & Developer Experience | C | Rich docs and roadmaps exist. | README, changelog, roadmap status, feature status evidence, and mdBook references are stale or contradictory. |
| CI/CD & Release Process | C+ | CI breadth is high and release workflow is improved. | Release-truth scripts can pass while checking nothing; benchmark/fuzz claims are not enforced. |
| Architecture & Long-term Health | C+ | Core architecture is sound and extensible. | Side effects are attached to wrapper APIs instead of storage mutations, causing correctness drift. |

## Critical Findings (Severity: Critical)

### CF-1: CONSTRUCT writeback bypasses SPARQL Update and bulk-load paths

Severity: Critical

Category: Correctness & Semantic Fidelity

Description: v0.65 added a real incremental CONSTRUCT writeback kernel, but it is wired to selected public wrappers rather than the storage mutation boundary. Public `insert_triple()` calls `construct_rules::on_graph_write()`, but SPARQL Update and bulk RDF loading call storage functions directly. That means derived graphs and provenance can be correct for one insertion API and stale for another.

Evidence:

- The CWB hook explicitly says it is called by `insert_triple` and `sparql_update` in [construct_rules.rs](../src/construct_rules.rs#L955-L963).
- The SQL wrapper `insert_triple()` calls the hook after named-graph writes in [dict_api.rs](../src/dict_api.rs#L197-L230).
- SPARQL `INSERT DATA` calls `storage::insert_triple_by_ids()` directly in [sparql/mod.rs](../src/sparql/mod.rs#L1038-L1048).
- SPARQL `DELETE DATA` and `DELETE/INSERT WHERE` call `storage::delete_triple_by_ids()` and `storage::insert_triple_by_ids()` directly in [sparql/mod.rs](../src/sparql/mod.rs#L1051-L1067) and [sparql/mod.rs](../src/sparql/mod.rs#L1272-L1290).
- Bulk loaders flush batches directly through `storage::batch_insert_encoded()` in [bulk_load.rs](../src/bulk_load.rs#L100-L125).

Impact: Any application using SPARQL Update or bulk load can silently skip incremental maintenance. Queries over CONSTRUCT writeback targets may return stale or missing inferred triples, and delete provenance can drift from explicit graph state. This invalidates the strongest v0.65 correctness claim for common production write paths.

Remediation: Move graph-mutation side effects into a storage-level mutation journal. Every insert/delete/batch operation should record affected graph IDs in a transaction-local set; at statement or transaction end, run CWB, provenance cleanup, metrics, and Citus/RLS side effects from that set. Add pg_regress tests that register a rule, mutate via `insert_triple`, SPARQL `INSERT DATA`, SPARQL `DELETE/INSERT WHERE`, `DELETE DATA`, and each bulk loader, then assert identical derived-graph state.

### CF-2: Graph-level RLS protects only `_pg_ripple.vp_rare`, not promoted VP tables

Severity: Critical

Category: Security

Description: Graph access controls are implemented as row-level security policies on `_pg_ripple.vp_rare`, but promoted predicates are moved into `_pg_ripple.vp_{id}_delta`, `_main`, `_tombstones`, and read views. Promotion does not apply equivalent policies to those tables. Once a predicate is promoted, graph-level authorization can become table-layout dependent.

Evidence:

- `do_grant_graph_access()` enables RLS and creates policies only on `_pg_ripple.vp_rare` in [security_api.rs](../src/security_api.rs#L33-L41).
- `do_revoke_graph_access()` drops policies only from `_pg_ripple.vp_rare` in [security_api.rs](../src/security_api.rs#L50-L54).
- `enable_graph_rls()` documents and creates policies on `_pg_ripple.vp_rare` only in [maintenance_api.rs](../src/maintenance_api.rs#L214-L242).
- Rare-predicate promotion creates the HTAP split and moves rows to a dedicated delta table in [storage/mod.rs](../src/storage/mod.rs#L475-L506), then attaches timeline/Citus hooks in [storage/mod.rs](../src/storage/mod.rs#L529-L540), but no RLS policy hook appears there.

Impact: Multi-tenant or role-isolated deployments can leak named-graph data as soon as heavily used predicates are promoted. This is a tenant-isolation failure and blocks production use where PostgreSQL roles enforce graph access.

Remediation: Treat RLS as part of VP table creation. `ensure_vp_table()` or promotion should apply policies to delta/main/tombstones/read views, and `grant_graph_access`/`revoke_graph_access` should update all existing VP relations. Add a Citus-aware integration test that grants a role access to one graph, promotes a predicate across the threshold, and verifies denied rows remain denied before and after promotion and merge.

### CF-3: Arrow Flight accepts unsigned tickets and exports stale, fully buffered data

Severity: Critical

Category: Security, Correctness & Performance

Description: v0.66 replaced the JSON stub with real Arrow IPC, but the current endpoint is not production-safe. It accepts unsigned tickets by default, reads `_main` and `_delta` directly without excluding tombstones, and buffers all rows plus the full Arrow IPC payload in memory before sending the response.

Evidence:

- `build_signed_ticket()` emits `unsigned` when no secret is configured in [flight.rs](../src/flight.rs#L57-L64).
- The in-extension validator skips signature verification for `sig = unsigned` in [flight.rs](../src/flight.rs#L112-L118).
- The HTTP validator reads `sig` and only verifies HMAC when the value is not `unsigned` in [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs#L1976-L1998).
- The endpoint queries promoted tables as `_main UNION ALL _delta` in [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs#L2092-L2115), bypassing the HTAP read semantics that subtract tombstones.
- The endpoint materializes `rows`, builds full `Vec<i64>` columns, writes one Arrow batch into `buf`, and returns `Body::from(buf)` in [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs#L2118-L2198).

Impact: Anyone who can reach the authenticated HTTP service can present an unsigned ticket unless deployment configuration prevents it elsewhere. Deleted triples can be exported after tombstoning. Large graph exports can exhaust service memory because neither database fetch nor HTTP response is chunked.

Remediation: Require `ARROW_FLIGHT_SECRET` in production mode and reject unsigned tickets unless an explicit development-only flag is set. Build export SQL from the same storage read abstraction used by SPARQL, including tombstone exclusion. Replace the one-shot query with database cursor paging and multiple Arrow record batches streamed through `Body::from_stream`. Add HTTP integration tests for unsigned rejection, bad signature rejection, expired tickets, tombstoned rows, and memory-bounded multi-batch export.

### CF-4: Release-truth gates can pass while checking nothing

Severity: Critical

Category: CI/CD & Release Process

Description: v0.64 introduced release-truth scripts, but two central checks are advisory and ineffective in the current local environment and changelog layout. The API drift checker depends on GNU `grep -P`; on macOS it extracts zero functions, prints a warning, and exits 0. The roadmap evidence checker targets the first changelog section, which is `[Unreleased]`, not the latest release. `just assess-release` runs both scripts, so a release assessment can appear green while missing the checks it claims to enforce.

Evidence:

- [check_api_drift.sh](../scripts/check_api_drift.sh#L32-L40) uses `grep -oP` and exits 0 when no `pg_extern` names are extracted.
- Local run output was `grep: invalid option -- P` followed by `WARNING: could not extract any pg_extern function names from src/`.
- [check_roadmap_evidence.sh](../scripts/check_roadmap_evidence.sh#L39-L44) extracts the first `## [` block, but [CHANGELOG.md](../CHANGELOG.md#L8-L16) places `[Unreleased]` before v0.66.0.
- Local run output was `WARNING: could not extract a version section from CHANGELOG.md` with exit 0.
- [justfile](../justfile#L201-L217) includes both scripts in `assess-release`.

Impact: Documentation drift, stale feature status, and unsupported completion claims can pass the release gate. This is a process-critical failure because the project relies on release-truth automation to control a very broad feature surface.

Remediation: Reimplement both checks in Rust or portable Python, make them fail closed, and require an explicit version argument. The roadmap evidence checker should ignore `[Unreleased]` unless explicitly requested and inspect the requested version block. The API drift checker should parse Rust with `syn` or use generated pgrx metadata, compare full signatures, and fail CI on extraction failure.

## High-Priority Findings (Severity: High)

### HF-1: Feature-status evidence contains stale or nonexistent references

Severity: High

Category: Code Quality & Maintainability

Description: `feature_status()` is meant to be machine-readable release truth, but multiple entries cite evidence that is missing or contradicted by source. This weakens every downstream readiness dashboard that consumes it.

Evidence:

- `citus_brin_summarise` is marked implemented and claims post-merge shard summarization in [feature_status.rs](../src/feature_status.rs#L205-L220), but the merge worker only calls local `brin_summarize_new_values` in [merge.rs](../src/storage/merge.rs#L411-L444).
- `citus_rls_propagation` claims `tests/integration/citus_rls_propagation.sh` in [feature_status.rs](../src/feature_status.rs#L222-L239), but that file is absent.
- `arrow_flight`, `wcoj`, and Citus entries cite `docs/src/reference/arrow-flight.md`, `docs/src/reference/query-optimization.md`, and `docs/src/reference/scalability.md` in [feature_status.rs](../src/feature_status.rs#L240-L284); those referenced docs are absent.
- `streaming_observability` cites `streaming_metrics.sql` in [feature_status.rs](../src/feature_status.rs#L290-L298), but the actual coverage is only part of [v066_features.sql](../tests/pg_regress/sql/v066_features.sql#L44-L57).

Impact: Operators and release tooling cannot rely on `feature_status()` as evidence. This recreates the Assessment 8 overclaim pattern in a more official API.

Remediation: Add a CI job that validates every evidence path in `feature_status()` exists and, for tests, maps to an executed workflow step. Use statuses such as `experimental`, `planned`, and `implemented` only when their evidence is present and behaviorally tested.

### HF-2: Documentation and version narrative are still inconsistent at v0.66.0

Severity: High

Category: Documentation & Developer Experience

Description: Public docs mix v0.63, v0.64, v0.66, and future-dated release state. This is not cosmetic: users deciding whether a limitation is still real will get conflicting answers.

Evidence:

- [README.md](../README.md#L22) still says `What works today (v0.63.0)`.
- [README.md](../README.md#L363-L375) still lists `Known limitations in v0.63.0`, including Arrow Flight as a JSON stub and true cursors as planned for v0.66.0.
- [CHANGELOG.md](../CHANGELOG.md#L16) dates v0.66.0 to 2026-04-29, one day after this assessment date.
- [ROADMAP.md](../ROADMAP.md#L119-L121) marks v0.64.0 through v0.66.0 released, but [roadmap/v0.64.0.md](../roadmap/v0.64.0.md#L5) still says planned.
- [implementation_plan.md](implementation_plan.md#L5-L22) says pgrx 0.18 in prose but still lists pgrx 0.17 in the stack table.

Impact: Users cannot determine which behavior to rely on, and maintainers cannot use docs as a release control. Documentation drift also hides security-sensitive changes such as Arrow ticket requirements.

Remediation: Make version synchronization a blocking check across README, CHANGELOG, ROADMAP, per-version roadmap files, control file, Cargo metadata, and feature status. Add a generated release summary from `feature_status()` so there is one source of truth.

### HF-3: Performance regression gates are not enforcing the README claim

Severity: High

Category: Performance & Scalability

Description: The README claims performance benchmarks fail builds on regressions, but the current benchmark workflow is manual and contains a merge-throughput step that can skip itself after running a SQL pgbench script as a shell script.

Evidence:

- [README.md](../README.md#L394-L397) claims continuous fuzzing, 10 percent performance regression failure, and 72-hour soak testing.
- [benchmark.yml](../.github/workflows/benchmark.yml#L1-L4) is `workflow_dispatch` only.
- [benchmark.yml](../.github/workflows/benchmark.yml#L96-L118) runs `bash benchmarks/merge_throughput.sql | tee ... || true`; the file is a pgbench SQL script, not a shell script, as shown in [merge_throughput.sql](../benchmarks/merge_throughput.sql#L1-L18).
- The parser exits 0 when no merge throughput results are found in [benchmark.yml](../.github/workflows/benchmark.yml#L106-L118).
- [merge_throughput_history.csv](../benchmarks/merge_throughput_history.csv#L1-L2) contains only v0.59.0 data, while [merge_throughput_baselines.json](../benchmarks/merge_throughput_baselines.json#L2) describes v0.53.0 baselines.

Impact: A major performance regression can land without CI detection, despite public docs saying the build will fail. The merge worker, Arrow export, WCOJ, and Citus paths all need real gates before v1.0.

Remediation: Run pgbench with `pgbench -f benchmarks/merge_throughput.sql`, remove `|| true`, fail when no results parse, and schedule the benchmark on protected branches. Update baselines per release and store trend artifacts generated by CI, not hand-maintained files.

### HF-4: SHACL-SPARQLRule documentation overclaims unsupported execution

Severity: High

Category: Correctness & Semantic Fidelity

Description: The docs describe `sh:SPARQLRule` as executable inference, but the code only warns and skips those rules. The machine-readable status says planned, while the feature docs say supported.

Evidence:

- [shacl-sparql-rules.md](../docs/src/features/shacl-sparql-rules.md#L5-L68) says pg_ripple supports SPARQL rules and writes constructed triples with `source = 1`.
- [af_rules.rs](../src/shacl/af_rules.rs#L142-L175) says `sh:SPARQLRule` patterns are detected and warned about but not compiled.
- [feature_status.rs](../src/feature_status.rs#L136-L150) marks `shacl_sparql_rule` as `planned` and says full routing was deferred.

Impact: Users can model validation/inference workflows that silently do not run. This is especially risky because SHACL rules look declarative and successful loading may be mistaken for execution.

Remediation: Either implement SPARQLRule execution through the derivation kernel with tests, or rewrite the docs to state that only `sh:TripleRule` compiles today and `sh:SPARQLRule` is warning-only.

### HF-5: Streaming observability counters are present but not connected

Severity: High

Category: Observability

Description: `streaming_metrics()` exposes counters for cursor rows, Arrow batches, and Arrow ticket rejections, but those incrementers are dead code or not reachable from the HTTP service. The v0.66 smoke test checks keys, not counter behavior.

Evidence:

- [stats.rs](../src/stats.rs#L18-L55) defines counters and marks row/Arrow increment helpers with `#[allow(dead_code)]`.
- [cursor.rs](../src/sparql/cursor.rs#L98-L224) increments pages opened/fetched but not rows streamed.
- [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs#L2047-L2198) rejects tickets and sends Arrow bytes without updating extension counters.
- [v066_features.sql](../tests/pg_regress/sql/v066_features.sql#L44-L57) only verifies metric keys exist.

Impact: Operators may see zeros for important streaming signals while traffic is flowing. Incident response and SLO dashboards built on these metrics will be misleading.

Remediation: Increment row counts when pages are returned, expose HTTP-service Arrow metrics through the HTTP service metrics endpoint or a shared telemetry channel, and test that counters change after cursor fetches and rejected Arrow tickets.

### HF-6: SBOM and supply-chain artifacts are stale

Severity: High

Category: Security

Description: The repository metadata is v0.66.0, but the checked-in SBOM and SBOM diff are several releases behind.

Evidence:

- [Cargo.toml](../Cargo.toml#L4) and [pg_ripple.control](../pg_ripple.control#L5) declare v0.66.0.
- Local SBOM metadata reports component `pg_ripple 0.51.0` with timestamp `2026-04-23T19:01:13.224101000Z`.
- [sbom_diff.md](../sbom_diff.md#L1-L10) is for v0.60.0 to v0.61.0 and says generated in 2025.

Impact: Security reviewers and downstream packagers cannot use the SBOM as evidence for the current release. Dependency changes after v0.51.0 are not represented in the checked-in artifact.

Remediation: Regenerate CycloneDX SBOMs during release, diff them against the prior tag, and fail release CI when the SBOM component version does not match `Cargo.toml`.

### HF-7: Citus scalability features remain helpers rather than integrated behavior

Severity: High

Category: Performance & Scalability

Description: Citus helper APIs exist, but several v0.62-v0.66 distributed claims remain planned or partially wired. This is better than overclaiming in some places, but release text still says v0.66 made integrated Citus paths real.

Evidence:

- [ROADMAP.md](../ROADMAP.md#L121) says v0.66 includes integrated Citus pruning, HLL, BRIN, RLS, and promotion paths.
- [feature_status.rs](../src/feature_status.rs#L179-L204) marks service pruning, HLL distinct, and nonblocking promotion as planned.
- [group.rs](../src/sparql/translate/group.rs#L205-L229) emits exact `COUNT(DISTINCT ...)`, not HLL aggregate SQL.
- [storage/mod.rs](../src/storage/mod.rs#L475-L540) performs synchronous promotion with an advisory lock and no shadow-table promotion state.

Impact: Distributed workloads will route through coordinator or exact paths where docs suggest optimized distributed behavior. This affects performance expectations and cluster sizing.

Remediation: Keep helper APIs, but split feature claims into `helper exists`, `translator wired`, `write path wired`, and `CI verified on Citus`. Move entries to implemented only after end-to-end tests pass.

### HF-8: v0.66 regression tests are mostly smoke tests

Severity: High

Category: Test Coverage

Description: The v0.66 feature gate checks GUC defaults, JSON field presence, function existence, and an empty cursor. It does not verify Arrow HTTP security, tombstone exclusion, row streaming metrics, Citus per-shard behavior, or true WCOJ query behavior.

Evidence:

- [v066_features.sql](../tests/pg_regress/sql/v066_features.sql#L1-L8) lists the v0.66 coverage scope.
- The Arrow test only checks that `export_arrow_flight()` returns a ticket with fields in [v066_features.sql](../tests/pg_regress/sql/v066_features.sql#L26-L40).
- The metrics test checks key presence in [v066_features.sql](../tests/pg_regress/sql/v066_features.sql#L44-L57).
- The cursor test only runs an empty result in [v066_features.sql](../tests/pg_regress/sql/v066_features.sql#L90-L94).

Impact: High-risk features can appear released with only presence-level validation. This increases regression risk and makes release notes less trustworthy.

Remediation: Add behavior, negative, and edge-case tests for each v0.66 claim. HTTP Arrow needs integration tests; Citus needs a real multi-node job; cursors need non-empty paged result tests and row counter assertions.

## Medium-Priority Findings (Severity: Medium)

### MF-1: A production panic remains in construct-rule topological sorting

Severity: Medium

Category: Bugs & Runtime Safety

Description: The only explicit `panic!` found in non-test Rust source is in construct-rule ordering. Even if it represents an internal invariant, server extension code should convert invariant failures into PostgreSQL errors with context.

Evidence: [construct_rules.rs](../src/construct_rules.rs#L242) calls `panic!` when `in_degree` is missing for a rule.

Impact: A malformed or unexpected construct-rule graph can crash the backend instead of returning a controlled SQL error.

Remediation: Replace the panic with a fallible error path that names the rule, source graph, and catalog state. Add a regression test for a malformed dependency cycle or missing-degree condition.

### MF-2: CONSTRUCT cursor helpers still materialize full results

Severity: Medium

Category: Performance & Scalability

Description: SELECT/ASK cursor streaming is materially improved, but `sparql_cursor_turtle()` and `sparql_cursor_jsonld()` still call `sparql_construct()` and collect all triples before chunking serialized output.

Evidence: [cursor.rs](../src/sparql/cursor.rs#L293-L352) calls `sparql_construct()`, collects triples into a vector, then chunks.

Impact: Users can choose a cursor-named export function and still hit full-result memory behavior for CONSTRUCT outputs.

Remediation: Build a CONSTRUCT iterator that streams template bindings through the same portal page mechanism, serializing Turtle/JSON-LD chunks without materializing all triples.

### MF-3: WCOJ is honestly labelled but remains a planner hint, not an executor

Severity: Medium

Category: Performance & Scalability

Description: The truth problem is largely fixed, but cyclic and high-degree join workloads still lack a true intersecting-iterator Leapfrog Triejoin executor.

Evidence: [feature_status.rs](../src/feature_status.rs#L272-L284) explicitly says WCOJ reorders joins and a true executor is not implemented.

Impact: Users expecting worst-case optimal complexity will still get PostgreSQL join plans with hints/reordering, which may be insufficient for dense cyclic BGPs.

Remediation: Either keep WCOJ as planner metadata only through v1.0, or implement a real executor behind an experimental GUC and benchmark it on cyclic BGP suites.

### MF-4: Large modules are becoming ownership boundaries rather than cohesive units

Severity: Medium

Category: Code Quality & Maintainability

Description: Several core files are large enough to slow review and make side-effect wiring easy to miss.

Evidence: Local line counts: [lib.rs](../src/lib.rs) 2,341 lines, [storage/mod.rs](../src/storage/mod.rs) 2,207 lines, [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs) 2,206 lines, [sparql/mod.rs](../src/sparql/mod.rs) 1,887 lines, and [construct_rules.rs](../src/construct_rules.rs) 1,173 lines.

Impact: Cross-cutting changes such as CWB, RLS, and observability are likely to be wired into one path and missed in another.

Remediation: Split modules around explicit contracts: storage mutation journal, SPARQL Update executor, SELECT translator, CONSTRUCT executor, Arrow export service, HTTP routing, and metrics.

### MF-5: Conformance and stress coverage remain partly informational

Severity: Medium

Category: Test Coverage

Description: CI breadth is strong, but several important suites are informational or fail-tolerant, and the local assessment did not run full conformance due environment requirements.

Evidence: [ci.yml](../.github/workflows/ci.yml#L327-L333) marks the full W3C suite informational, [ci.yml](../.github/workflows/ci.yml#L818-L824) marks the entailment suite informational, and [ci.yml](../.github/workflows/ci.yml#L1010-L1015) marks BSBM informational.

Impact: A regression in conformance or benchmark behavior can be visible but non-blocking. For v1.0, non-blocking should be limited to exploratory suites with tracked trend budgets.

Remediation: Promote smoke subsets and recent-feature conformance cases to blocking. Publish pass-rate trends for informational suites and define dates for promotion to blocking status.

### MF-6: Current release install packaging is unverified from checked-in SQL

Severity: Medium

Category: CI/CD & Release Process

Description: The repository has 66 upgrade scripts through [pg_ripple--0.65.0--0.66.0.sql](../sql/pg_ripple--0.65.0--0.66.0.sql), but no checked-in `sql/pg_ripple--0.66.0.sql` base install file was found. pgrx may generate install SQL during packaging, but this assessment did not verify that clean `CREATE EXTENSION pg_ripple` works from only checked-in release artifacts.

Evidence: [pg_ripple.control](../pg_ripple.control#L5) sets `default_version = 0.66.0`; the latest checked-in SQL file is [pg_ripple--0.65.0--0.66.0.sql](../sql/pg_ripple--0.65.0--0.66.0.sql).

Impact: Source packaging or downstream distribution can fail if generated SQL is not included or reproducible.

Remediation: Add a release packaging check that builds from a clean checkout, installs the packaged extension into PostgreSQL 18, and runs `CREATE EXTENSION pg_ripple` plus the full migration-chain test from v0.1.0 to current.

## Low-Priority Findings (Severity: Low)

### LF-1: SECURITY DEFINER lint comment is stale

Severity: Low

Category: Documentation & Developer Experience

Description: The lint script says pg_ripple uses SECURITY DEFINER in exactly two places but lists one allowlisted function.

Evidence: [check_no_security_definer.sh](../scripts/check_no_security_definer.sh#L5-L25) describes two places while the allowlist contains only `ddl_guard_vp_tables`.

Impact: The script behavior is correct today, but the comment can confuse reviewers during security audit.

Remediation: Update the comment to match the allowlist, or add the missing second allowlist item if one exists.

### LF-2: Roadmap release status is inconsistent across aggregate and per-version files

Severity: Low

Category: Documentation & Developer Experience

Description: [ROADMAP.md](../ROADMAP.md#L112-L121) marks v0.62.0 and v0.64.0 released, but [roadmap/v0.62.0.md](../roadmap/v0.62.0.md#L5) and [roadmap/v0.64.0.md](../roadmap/v0.64.0.md#L5) still say planned.

Impact: Release tracking tools and readers can disagree about milestone state.

Remediation: Generate per-version status from a single machine-readable release manifest.

### LF-3: Implementation plan still has a pgrx version mismatch

Severity: Low

Category: Documentation & Developer Experience

Description: The implementation plan introduction says pgrx 0.18, but the technology table says pgrx 0.17.

Evidence: [implementation_plan.md](implementation_plan.md#L5-L22).

Impact: New contributors may install or debug against the wrong pgrx version.

Remediation: Update the table and include the implementation plan in the version-sync lint.

### LF-4: Fuzz targets exist, but continuous fuzzing is not wired in workflows

Severity: Low

Category: Test Coverage

Description: The repository has 12 fuzz targets, but no GitHub workflow references `cargo fuzz`. README claims continuous fuzzing.

Evidence: Fuzz targets are under [fuzz/fuzz_targets](../fuzz/fuzz_targets), while workflow search found no `cargo fuzz` usage; [README.md](../README.md#L394) claims continuous fuzzing.

Impact: Fuzz coverage depends on manual execution and may not catch regressions before release.

Remediation: Add a scheduled short fuzz job and a longer nightly/manual fuzz workflow with artifact upload for crashes and corpus changes.

### LF-5: CHANGELOG release dates need a consistency pass

Severity: Low

Category: Documentation & Developer Experience

Description: v0.66.0 is dated after the assessment date, and older entries around v0.61-v0.63 use dates that do not match the surrounding 2026 release chronology.

Evidence: [CHANGELOG.md](../CHANGELOG.md#L16-L18) dates v0.66.0 to 2026-04-29.

Impact: This undermines release provenance, even when code metadata is otherwise synchronized.

Remediation: Normalize changelog dates from tags or release metadata and make date drift a blocking release check.

## Gap Analysis Summary Tables

### Correctness & Semantic Fidelity

| Area | Finding count by severity | Top gap | Remediation priority |
|---|---|---|---|
| Correctness & Semantic Fidelity | Critical 1, High 1, Medium 0, Low 0 | CONSTRUCT writeback is not attached to all graph mutation paths. | Build a storage-level mutation journal and behavior-test every write API. |

### Bugs & Runtime Safety

| Area | Finding count by severity | Top gap | Remediation priority |
|---|---|---|---|
| Bugs & Runtime Safety | Critical 0, High 0, Medium 1, Low 0 | A production `panic!` remains in CWB dependency ordering. | Replace panic with PostgreSQL error and add malformed-rule regression coverage. |

### Code Quality & Maintainability

| Area | Finding count by severity | Top gap | Remediation priority |
|---|---|---|---|
| Code Quality & Maintainability | Critical 0, High 1, Medium 1, Low 0 | Feature-status evidence and large modules make truth drift easy. | Validate evidence paths and split side-effect-heavy modules. |

### Performance & Scalability

| Area | Finding count by severity | Top gap | Remediation priority |
|---|---|---|---|
| Performance & Scalability | Critical 1, High 2, Medium 2, Low 0 | Arrow/Citus/WCOJ paths are partial and benchmark gates are ineffective. | Make performance claims end-to-end and enforce benchmark gates. |

### Security

| Area | Finding count by severity | Top gap | Remediation priority |
|---|---|---|---|
| Security | Critical 2, High 1, Medium 0, Low 1 | Promoted-table RLS and unsigned Arrow tickets block production security. | Enforce RLS on all VP relations and require signed Arrow tickets. |

### Test Coverage

| Area | Finding count by severity | Top gap | Remediation priority |
|---|---|---|---|
| Test Coverage | Critical 0, High 1, Medium 1, Low 1 | Recent features have smoke tests but lack negative/stress/integration coverage. | Add behavior tests for v0.65-v0.66 and wire scheduled fuzzing. |

### Documentation & Developer Experience

| Area | Finding count by severity | Top gap | Remediation priority |
|---|---|---|---|
| Documentation & Developer Experience | Critical 0, High 2, Medium 0, Low 4 | README, roadmap, changelog, feature status, and docs disagree. | Generate release docs from one metadata source and fail on drift. |

### CI/CD & Release Process

| Area | Finding count by severity | Top gap | Remediation priority |
|---|---|---|---|
| CI/CD & Release Process | Critical 1, High 1, Medium 1, Low 0 | Release-truth and benchmark gates can pass without checking real behavior. | Make checks fail closed and run clean package/install tests. |

### Architecture & Long-term Health

| Area | Finding count by severity | Top gap | Remediation priority |
|---|---|---|---|
| Architecture & Long-term Health | Critical 1, High 2, Medium 1, Low 0 | Side effects are attached to API wrappers, not core architectural contracts. | Centralize mutation, authorization, and telemetry contracts before adding features. |

## Recommended Action Plan

1. XL, v0.67.0: Introduce a storage-level graph mutation journal used by `insert_triple`, `insert_triple_by_ids`, `delete_triple_by_ids`, `batch_insert_encoded`, SPARQL Update, and all loaders.
2. L, v0.67.0: Rewire CONSTRUCT writeback, provenance cleanup, and CWB metrics to consume the mutation journal.
3. M, v0.67.0: Add pg_regress coverage proving CWB equivalence across public insert, SPARQL Update, and bulk-load APIs.
4. L, v0.67.0: Apply graph RLS policies to every VP relation and view at creation, promotion, grant, revoke, and migration time.
5. L, v0.67.0: Add promoted-predicate RLS tests, including threshold promotion and HTAP merge.
6. M, v0.67.0: Require signed Arrow tickets by default; make unsigned tickets explicit development-only configuration.
7. L, v0.67.0: Rewrite Arrow export to use storage read semantics with tombstone exclusion and paged database cursors.
8. L, v0.67.0: Add HTTP Arrow integration tests for bad signature, unsigned ticket, expiry, tombstone exclusion, and multi-batch export.
9. M, v0.67.0: Reimplement release-truth scripts in portable Python or Rust and make extraction failures hard failures.
10. M, v0.67.0: Add CI validation that every `feature_status()` evidence path exists and is linked to an executed test or doc.
11. S, v0.67.0: Update README, CHANGELOG, ROADMAP, per-version roadmaps, and implementation plan to v0.66.0 truth.
12. M, v0.67.0: Regenerate SBOM and SBOM diff during release, then fail if SBOM component version differs from `Cargo.toml`.
13. M, v0.67.0: Fix benchmark workflow invocation, remove `|| true`, and fail when benchmark output cannot be parsed.
14. L, v0.67.0: Add a scheduled performance workflow with merge, vector, BSBM, and Citus workload trend artifacts.
15. M, v0.67.0: Replace `construct_rules.rs` panic with a recoverable PostgreSQL error and add regression coverage.
16. L, v0.68.0: Implement streaming CONSTRUCT Turtle/JSON-LD iterators or rename helpers to avoid cursor overclaim.
17. L, v0.68.0: Wire Citus HLL aggregate translation behind `pg_ripple.approx_distinct` with exact fallback and tests.
18. L, v0.68.0: Wire Citus SERVICE and multihop pruning into the SPARQL translator only after explain and correctness tests exist.
19. XL, v0.68.0: Design nonblocking VP promotion with shadow tables, progress state, RLS propagation, and crash recovery tests.
20. M, v0.68.0: Add scheduled short fuzzing and longer manual fuzz workflows for all 12 fuzz targets.
21. L, v0.69.0: Split large modules along mutation, translation, export, and HTTP routing boundaries.
22. XL, v1.0.0: Run and publish a clean-package install test, migration-chain test, Citus integration run, conformance pass-rate report, and security-audit checklist as release artifacts.

## Appendix: Feature Claim Verification Matrix

| Feature | First claimed in version | Roadmap file | Implementation file(s) | Status | Notes |
|---|---:|---|---|---|---|
| GitHub Actions SHA pinning | v0.64.0 | [roadmap/v0.64.0.md](../roadmap/v0.64.0.md) | [.github/workflows](../.github/workflows), [check_github_actions_pinned.sh](../scripts/check_github_actions_pinned.sh) | ✅ | Local scan found 110 refs and 0 mutable refs. |
| Docker release digest integrity | v0.64.0 | [roadmap/v0.64.0.md](../roadmap/v0.64.0.md) | [release.yml](../.github/workflows/release.yml) | ✅ | Prior continue-on-error release flaw appears resolved. |
| Feature-status SQL API | v0.64.0 | [roadmap/v0.64.0.md](../roadmap/v0.64.0.md) | [feature_status.rs](../src/feature_status.rs) | ⚠️ | API exists, but several statuses/evidence paths are stale or false. |
| Roadmap evidence check | v0.64.0 | [roadmap/v0.64.0.md](../roadmap/v0.64.0.md) | [check_roadmap_evidence.sh](../scripts/check_roadmap_evidence.sh) | ❌ | Checks `[Unreleased]` or exits 0 on extraction failure. |
| API drift check | v0.64.0 | [roadmap/v0.64.0.md](../roadmap/v0.64.0.md) | [check_api_drift.sh](../scripts/check_api_drift.sh) | ❌ | GNU `grep -P` dependency makes local macOS run check nothing and exit 0. |
| CONSTRUCT writeback kernel | v0.63.0/v0.65.0 | [roadmap/v0.65.0.md](../roadmap/v0.65.0.md) | [construct_rules.rs](../src/construct_rules.rs), [dict_api.rs](../src/dict_api.rs) | ⚠️ | Kernel exists, but SPARQL Update and bulk load bypass hooks. |
| HTAP-aware CWB retraction | v0.65.0 | [roadmap/v0.65.0.md](../roadmap/v0.65.0.md) | [construct_rules.rs](../src/construct_rules.rs) | ⚠️ | Retraction path exists where invoked; end-to-end path coverage incomplete. |
| SPARQL SELECT cursor streaming | v0.66.0 | [roadmap/v0.66.0.md](../roadmap/v0.66.0.md) | [cursor.rs](../src/sparql/cursor.rs) | ✅ | Portal-based SELECT/ASK paging is real. |
| CONSTRUCT Turtle/JSON-LD cursor streaming | v0.66.0 | [roadmap/v0.66.0.md](../roadmap/v0.66.0.md) | [cursor.rs](../src/sparql/cursor.rs) | ⚠️ | Helpers still materialize CONSTRUCT results before chunking. |
| Arrow Flight signed tickets | v0.66.0 | [roadmap/v0.66.0.md](../roadmap/v0.66.0.md) | [flight.rs](../src/flight.rs), [main.rs](../pg_ripple_http/src/main.rs) | ⚠️ | HMAC exists, but unsigned tickets are accepted by default paths. |
| Arrow IPC export | v0.62.0/v0.66.0 | [roadmap/v0.66.0.md](../roadmap/v0.66.0.md) | [main.rs](../pg_ripple_http/src/main.rs) | ⚠️ | Real IPC exists, but tombstones are ignored and export is fully buffered. |
| WCOJ explain metadata | v0.66.0 | [roadmap/v0.66.0.md](../roadmap/v0.66.0.md) | [feature_status.rs](../src/feature_status.rs), [explain.rs](../src/sparql/explain.rs) | ✅ | Honest planner-hint status. True WCOJ executor remains unimplemented by design/status. |
| Citus BRIN summarise API | v0.66.0 | [roadmap/v0.66.0.md](../roadmap/v0.66.0.md) | [citus.rs](../src/citus.rs), [merge.rs](../src/storage/merge.rs) | ⚠️ | Explicit API exists; merge worker evidence for per-shard call is missing. |
| Citus RLS propagation | v0.66.0 | [roadmap/v0.66.0.md](../roadmap/v0.66.0.md) | [security_api.rs](../src/security_api.rs), [feature_status.rs](../src/feature_status.rs) | ❌ | Feature status cites a missing integration test; RLS only targets `_pg_ripple.vp_rare`. |
| Citus HLL COUNT DISTINCT | v0.62.0/v0.66.0 | [roadmap/v0.66.0.md](../roadmap/v0.66.0.md) | [citus.rs](../src/citus.rs), [group.rs](../src/sparql/translate/group.rs) | ❌ | Helper exists; aggregate translation still emits exact `COUNT(DISTINCT ...)`. |
| Citus SERVICE pruning | v0.62.0/v0.66.0 | [roadmap/v0.66.0.md](../roadmap/v0.66.0.md) | [feature_status.rs](../src/feature_status.rs), [citus.rs](../src/citus.rs) | ❌ | Marked planned; not wired into SPARQL translator. |
| Nonblocking VP promotion | v0.66.0 | [roadmap/v0.66.0.md](../roadmap/v0.66.0.md) | [storage/mod.rs](../src/storage/mod.rs) | ❌ | Promotion remains synchronous with advisory lock and atomic rare-table move. |
| SHACL SPARQL rules | v0.8.0/v0.65.0 | [roadmap/v0.65.0.md](../roadmap/v0.65.0.md) | [af_rules.rs](../src/shacl/af_rules.rs), [shacl-sparql-rules.md](../docs/src/features/shacl-sparql-rules.md) | ❌ | Docs say supported; code warns and skips `sh:SPARQLRule`. |
| Streaming observability metrics | v0.66.0 | [roadmap/v0.66.0.md](../roadmap/v0.66.0.md) | [stats.rs](../src/stats.rs), [cursor.rs](../src/sparql/cursor.rs), [main.rs](../pg_ripple_http/src/main.rs) | ⚠️ | Keys exist; row, Arrow batch, and ticket rejection counters are not fully connected. |
| Continuous fuzzing | v0.51.0 | [README.md](../README.md#L394) | [fuzz/fuzz_targets](../fuzz/fuzz_targets), [.github/workflows](../.github/workflows) | ⚠️ | 12 targets exist; no workflow runs `cargo fuzz`. |
| Performance regression failure gate | v0.51.0/v0.55.0 | [README.md](../README.md#L395) | [benchmark.yml](../.github/workflows/benchmark.yml), [benchmarks](../benchmarks) | ⚠️ | Benchmark workflow is manual and merge gate can skip on parse failure. |
| SBOM freshness | v0.64.0 | [roadmap/v0.64.0.md](../roadmap/v0.64.0.md) | [sbom.json](../sbom.json), [sbom_diff.md](../sbom_diff.md) | ❌ | SBOM metadata is v0.51.0 while current package is v0.66.0. |
