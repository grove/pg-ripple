# pg_ripple Overall Assessment 8

Date: 2026-04-27

Scope: Repository-wide assessment of pg_ripple v0.63.0, including Rust implementation, SQL migrations, CI/release workflows, documentation, roadmap claims, prior assessment themes, and selected targeted validations.

This assessment treats the project as an ambitious PostgreSQL-native RDF, SPARQL, SHACL, Datalog, HTAP, federation, Citus, GraphRAG, and HTTP platform. The codebase is unusually broad for a PostgreSQL extension and has many real assets: a continuous migration chain, a large pg_regress suite, CI gates for clippy/audit/deny/security-definer linting, documented PT error codes, and meaningful implementations across storage, query translation, SHACL, Datalog, CDC, vector search, temporal RDF, and HTTP APIs.

The main conclusion is more nuanced than "mostly done" or "mostly missing": pg_ripple has a large working core, but several recent release claims have moved faster than the implementation. The highest risk is that v0.62.0 and v0.63.0 public claims describe production-grade incremental, distributed, and bulk-streaming behavior while the code often contains API stubs, planner hints, synchronous recompute paths, or not-yet-integrated helpers.

## Executive Summary

Current version metadata is internally consistent at v0.63.0 in [Cargo.toml](../Cargo.toml#L4-L7) and [pg_ripple.control](../pg_ripple.control#L4-L8), but public docs and release notes lag or overstate important capabilities. [README.md](../README.md#L22) still says "What works today (v0.59.0)", while [CHANGELOG.md](../CHANGELOG.md#L16-L39) claims v0.63.0 delivered incremental CONSTRUCT writeback, Citus CITUS-30 through CITUS-37, and DRed-based maintenance. The implementation does not yet support those claims end to end.

The strongest correctness concern is v0.63.0 CONSTRUCT writeback. The API and catalogs exist, and registration performs validation plus an initial full recompute, but I found no trigger or write-path hook that runs construct rules when source graphs change. The v0.63.0 roadmap says rules are run "incrementally on each write transaction" in [roadmap/v0.63.0.md](../roadmap/v0.63.0.md#L5-L20); the code path in [src/construct_rules.rs](../src/construct_rules.rs#L440-L525) registers the rule and immediately calls `run_full_recompute`, while [src/construct_rules.rs](../src/construct_rules.rs#L552-L592) exposes manual full refresh. A targeted trigger search found CDC, temporal, tenant, and dictionary triggers, but no construct-rule maintenance trigger or delta/DRed path.

The strongest security concern is supply-chain drift. The README says v0.60.0 closed GitHub Actions SHA pinning in [README.md](../README.md#L164), but a static workflow scan found 109 mutable action refs and zero 40-character SHA-pinned action refs. Representative examples include `actions/checkout@v6`, `dtolnay/rust-toolchain@stable`, and `actions/cache@v5` in [ci.yml](../.github/workflows/ci.yml#L16-L34), plus mutable Docker and Trivy actions in [release.yml](../.github/workflows/release.yml#L210-L238). This contradicts a hardening claim and leaves CI/release execution mutable.

The strongest performance/architecture concern is that several "world-class" query and distributed claims are not implemented at their advertised level. WCOJ is wired into the SPARQL path, but as cyclic-pattern detection plus a materialized CTE wrapper and session planner settings, not a true Leapfrog Triejoin executor. Arrow Flight export is a JSON ticket plus HTTP JSON stub, not Arrow IPC streaming. Multiple Citus roadmap items are API helpers or dead/unwired paths rather than translator/merge/write-path integrations.

Overall maturity: strong research-prototype-to-beta core, not yet production-hardening complete. The next milestone should be less feature expansion and more truth-in-release, executable acceptance gates, and closing the gap between advertised and actual semantics.

## Assessment Method

Evidence reviewed included:

- Required project docs: [AGENTS.md](../AGENTS.md), [README.md](../README.md), [ROADMAP.md](../ROADMAP.md), [CHANGELOG.md](../CHANGELOG.md), [plans/implementation_plan.md](implementation_plan.md), and prior `plans/PLAN_OVERALL_ASSESSMENT*.md` files.
- Current release metadata: [Cargo.toml](../Cargo.toml#L4-L21), [pg_ripple.control](../pg_ripple.control#L4-L8), and SQL migrations through v0.63.0.
- Key implementation areas: storage/HTAP merge, SPARQL SQL generation, WCOJ, CONSTRUCT writeback, SHACL-AF bridge, Citus helpers, Arrow Flight, HTTP service, CI/release workflows, and pg_regress fixtures.
- Targeted validations: migration-chain continuity, SECURITY DEFINER lint, PT-code documentation lint, workflow action pinning scan, construct-rule trigger search, WCOJ call-site search, and pg_regress SQL/expected fixture counts.

Validation results:

- Migration scripts are continuous from v0.1.0 through v0.63.0.
- `scripts/check_no_security_definer.sh` passed; one allowlisted `SECURITY DEFINER` use remains intentional per [scripts/check_no_security_definer.sh](../scripts/check_no_security_definer.sh#L14-L22).
- `scripts/check_pt_codes.sh` passed; all 43 PT codes found in source are documented by docs or changelog according to [scripts/check_pt_codes.sh](../scripts/check_pt_codes.sh#L15-L44).
- `tests/pg_regress/sql/*.sql` and `tests/pg_regress/expected/*.out` are balanced at 181 files each.
- Static workflow scan found 109 mutable GitHub Action refs and 0 SHA-pinned refs.
- I did not run the full `cargo pgrx test` or `cargo pgrx regress` suites during this assessment; the findings below are based on static inspection and targeted lightweight checks.

## Current Maturity Assessment

| Area | Current maturity | Assessment |
|---|---:|---|
| Core storage and CRUD | High beta | HTAP split, rare predicates, dictionary encoding, graph APIs, tombstones, and merge worker are real. Promotion and distributed hot paths still have scalability gaps. |
| SPARQL query/update | High beta | Broad translator and tests exist. Some advanced claims, especially WCOJ and distributed pushdown, are overstated. |
| SHACL | Beta | Many core and advanced constraints exist. SHACL-SPARQL constraints are wired; SHACL-SPARQL rules are documented as supported but code warns they are not compiled. |
| Datalog/inference | Beta | Significant implementation surface and tests exist. Needs stronger conformance, deletion, memory, and distributed-execution gates. |
| HTAP merge | Beta+ | Prior unsafe cutover pattern appears improved with rename-swap. Needs stress proof and Citus per-worker BRIN integration. |
| Citus scaling | Alpha/beta mixed | Several APIs and tests exist, but multiple v0.62/v0.63 distributed execution claims are not integrated into translators or workers. |
| HTTP service | Beta for SPARQL protocol | Auth, metrics, SPARQL endpoints, and explorer exist. Arrow Flight is a stub and ticket security is weak. |
| Security hardening | Mixed | Cargo audit/deny and SECURITY DEFINER lint are positive. Action pinning, release image failure handling, ticket signing, and threat-model coverage remain gaps. |
| Observability | Beta | Telemetry exists, but recent features lack deep readiness, lag, provenance, and degradation metrics. |
| Documentation | Mixed | Extensive docs exist, but version drift and overclaimed features undermine trust. |
| Test coverage | Broad but uneven | 181 pg_regress fixtures and conformance jobs are strong. Recent features have shallow existence/fallback tests rather than behavior/performance acceptance tests. |

## What Changed Since Earlier Assessments

Positive movement:

- Version metadata now lines up at v0.63.0 in [Cargo.toml](../Cargo.toml#L4-L7) and [pg_ripple.control](../pg_ripple.control#L4-L8).
- Migration discipline is strong: the v0.62.0 to v0.63.0 migration exists and creates the construct-rule catalogs in [sql/pg_ripple--0.62.0--0.63.0.sql](../sql/pg_ripple--0.62.0--0.63.0.sql).
- CI now includes `cargo clippy --features pg18 -- -D warnings`, `cargo deny check`, `cargo audit`, and SECURITY DEFINER lint in [ci.yml](../.github/workflows/ci.yml#L70-L104).
- WCOJ is no longer purely dormant. The BGP translator sets `ctx.wcoj_preamble`, SQL generation wraps cyclic SQL through `apply_wcoj_hints`, and execution runs `wcoj_session_preamble` as shown in [src/sparql/sqlgen.rs](../src/sparql/sqlgen.rs#L753-L770) and [src/sparql/mod.rs](../src/sparql/mod.rs#L370-L392).
- The HTAP merge cutover now uses rename-swap rather than dropping the old main table before the replacement exists. The critical sequence is in [src/storage/merge.rs](../src/storage/merge.rs#L351-L407).

Regressions or new gaps:

- v0.63.0 adds a large semantic claim, incremental CONSTRUCT writeback, but implementation is initial/manual full recompute only.
- v0.62.0/v0.63.0 Citus claims are much broader than the integrated code paths.
- README and docs now mix v0.59.0, v0.60.0, v0.62.0, v0.63.0, and v1.0.0 language in ways that make release status hard to trust.
- The changelog now has internal API signature mismatches for v0.63.0 Citus functions. For example, [CHANGELOG.md](../CHANGELOG.md#L30-L37) describes `service_result_shard_prune(endpoint TEXT, graph_iri TEXT) RETURNS BIGINT` and `brin_summarize_vp_shards()` as an SRF, while the actual pgrx functions take `Vec<String>` and `pred_id: i64` and return `Vec<i64>`/`i64` in [src/citus.rs](../src/citus.rs#L982-L1007).

## Critical Findings

### C1. CONSTRUCT Writeback Is Not Incremental Despite Release Claims

Severity: Critical

Category: correctness, architecture, documentation, test coverage

Evidence:

- [roadmap/v0.63.0.md](../roadmap/v0.63.0.md#L5-L20) says CONSTRUCT writeback rules are maintained incrementally whenever the source graph changes.
- [CHANGELOG.md](../CHANGELOG.md#L16-L23) claims incremental delta maintenance via DRed and CWB-01 through CWB-11.
- `create_construct_rule` validates, registers, and runs an initial full recompute in [src/construct_rules.rs](../src/construct_rules.rs#L440-L525).
- `refresh_construct_rule` manually reloads the rule and calls full recompute in [src/construct_rules.rs](../src/construct_rules.rs#L552-L592).
- I found no construct-rule trigger, no source-graph write hook, and no DRed delta maintenance path. Trigger searches found CDC, temporal, tenant, and dictionary triggers, not construct-rule triggers.

Impact:

- Users who rely on `mode := 'incremental'` will get stale derived graphs after inserts/deletes unless they manually refresh.
- Pipelines described as raw-to-canonical live transformations are not live.
- Any downstream Datalog, SPARQL, GraphRAG, or API behavior depending on canonical graph freshness can return incorrect results.

Recommended remediation:

- Either downgrade the public status to "manual full refresh" immediately or implement real write-path maintenance before calling this released.
- Introduce an explicit maintenance architecture: source graph dependency index, per-write affected-rule lookup, insert delta evaluation, delete rederive/retract path, and ordered downstream rule cascade.
- Add pg_regress tests that insert and delete source triples after rule creation and verify target graph changes without calling `refresh_construct_rule`.
- Make invalid `mode` values fail fast; the current API documents `'incremental'` and `'full'` in [src/construct_rules_api.rs](../src/construct_rules_api.rs#L15-L25), but the implementation stores arbitrary mode strings in [src/construct_rules.rs](../src/construct_rules.rs#L492-L516).

### C2. Construct-Rule Retraction Can Fail For Promoted HTAP Predicates

Severity: Critical

Category: correctness, data retention, tests

Evidence:

- HTAP predicate views are `UNION ALL` views over `main`, `tombstones`, and `delta` in [src/storage/merge.rs](../src/storage/merge.rs#L173-L193).
- `retract_exclusive_triples` deletes promoted triples with `DELETE FROM _pg_ripple.vp_{pred_id}` in [src/construct_rules.rs](../src/construct_rules.rs#L770-L781).
- Such views are not automatically updatable in PostgreSQL because they contain set operations and no `INSTEAD OF` trigger/rule was found in [src/storage/merge.rs](../src/storage/merge.rs).

Impact:

- `drop_construct_rule(retract := true)` and `refresh_construct_rule` can leave stale inferred triples behind for promoted predicates.
- The function only logs warnings on deletion failures, so callers may receive success while data remains incorrect.
- The current construct-rule regression file does not create a successful rule, promote the predicate, refresh/drop it, and assert derived triples are retracted. It mainly checks catalogs, API existence, and invalid-input rejection in [tests/pg_regress/sql/construct_rules.sql](../tests/pg_regress/sql/construct_rules.sql#L1-L130).

Recommended remediation:

- Route retraction through storage APIs that know delta/main/tombstone semantics, not direct `DELETE` against the read view.
- For promoted predicates, delete delta-resident triples from `_delta` and tombstone main-resident triples in `_tombstones`.
- Turn retraction failure into an error for rule refresh/drop, not a warning.
- Add tests for rare and promoted predicates, including shared-target provenance and post-merge retraction.

### C3. GitHub Actions SHA Pinning Is Claimed But Not Implemented

Severity: Critical

Category: security, supply chain, release integrity

Evidence:

- [README.md](../README.md#L164) says v0.60.0 closes GitHub Actions SHA pinning.
- Static scan found 109 mutable action refs and 0 SHA-pinned refs.
- [ci.yml](../.github/workflows/ci.yml#L16-L34) uses `actions/checkout@v6`, `dtolnay/rust-toolchain@stable`, and `actions/cache@v5`.
- [release.yml](../.github/workflows/release.yml#L210-L238) uses mutable Docker and Trivy actions.

Impact:

- CI and release execution can change without a repository diff.
- A compromised action tag or upstream change could alter build/test/release behavior.
- This directly contradicts the hardening story for v1.0.0.

Recommended remediation:

- Pin every third-party action to a full commit SHA and add a workflow lint that rejects non-SHA `uses:` refs.
- Record the update procedure for action SHAs, ideally via Dependabot grouped PRs.
- Treat this as a release blocker before the next tag.

### C4. Arrow Flight Export Is A Stub And Tickets Are Unsigned

Severity: Critical

Category: correctness, security, documentation, performance

Evidence:

- [README.md](../README.md#L36-L37) advertises Arrow/Flight bulk export.
- [CHANGELOG.md](../CHANGELOG.md#L48-L54) says the SQL function returns a ticket for Arrow IPC streaming and also admits the HTTP endpoint responds with a JSON stub.
- [src/flight.rs](../src/flight.rs#L1-L21) documents a signed timestamp and per-session HMAC.
- The implementation emits plain JSON with `graph_iri`, `graph_id`, `iat`, and `type`, with no signature, expiry, audience, nonce, or MAC in [src/flight.rs](../src/flight.rs#L34-L59).
- The HTTP endpoint parses that JSON, trusts `graph_iri`, constructs a debug SQL string, and returns JSON metadata instead of streaming Arrow IPC in [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs#L1879-L1991).

Impact:

- The feature is not an Arrow Flight bulk export implementation in the current default code path.
- Ticket tampering is possible if a caller has access to the endpoint; graph authorization is not cryptographically bound to the ticket.
- Benchmark claims such as 500k triples/second cannot be supported by the current JSON stub.

Recommended remediation:

- Rename current behavior to `export_arrow_ticket_preview` or mark the feature experimental until real Arrow IPC streaming exists.
- Add HMAC signing, expiry validation, graph authorization binding, and replay protection.
- Implement streaming over actual VP tables, including promoted tables, `vp_rare`, named graph filters, backpressure, and batch sizing.
- Add integration tests that decode the response as Arrow IPC, not JSON.

## High-Priority Findings

### H1. WCOJ Is Planner Guidance, Not A True Leapfrog Triejoin Executor

Severity: High

Category: performance, standards of documentation, architecture

Evidence:

- [docs/src/features/advanced-inference.md](../docs/src/features/advanced-inference.md#L25-L44) describes Leapfrog Triejoin behavior and says plans show a leapfrog node.
- [CHANGELOG.md](../CHANGELOG.md#L50-L50) says the BGP translator activates the Leapfrog-Triejoin algorithm.
- The implementation detects cyclic patterns and wraps SQL in materialized CTE hints in [src/sparql/sqlgen.rs](../src/sparql/sqlgen.rs#L753-L770).
- The WCOJ module itself describes sort-merge join hints in [CHANGELOG.md](../CHANGELOG.md#L1178-L1178), and the tests assert GUCs, cycle detection, and result equivalence in [tests/pg_regress/sql/sparql_wcoj.sql](../tests/pg_regress/sql/sparql_wcoj.sql#L1-L130), not worst-case optimal complexity or a real triejoin operator.

Impact:

- Users may expect output-sensitive WCOJ semantics and plan stability that PostgreSQL merge-join hints cannot guarantee.
- Documentation may lead users to misdiagnose query plans by looking for nonexistent leapfrog nodes.

Recommended remediation:

- Rename this capability to "cyclic BGP merge-join planner hints" unless a real triejoin executor is implemented.
- Add explain output that accurately says `wcoj_style: planner_hint`, `cyclic_bgp_detected: true`, and records planner settings used.
- Add benchmark gates only for measured speedups, not algorithmic claims.

### H2. v0.63 Citus Claims Are Mostly Helpers, Not Integrated Distributed Execution

Severity: High

Category: scalability, architecture, documentation, tests

Evidence:

- [roadmap/v0.63.0.md](../roadmap/v0.63.0.md#L73-L137) lists streaming fan-out, approximate COUNT(DISTINCT), batched dictionary encoding, per-worker timeline tables, non-blocking VP promotion, RLS propagation tests, and per-worker BRIN summarise.
- Source search found functions for `service_result_shard_prune`, `approx_distinct_available`, and `brin_summarize_vp_shards` in [src/citus.rs](../src/citus.rs#L982-L1135), but not translator integration for service pruning, HLL aggregate SQL generation, per-worker timeline local tables, or promotion state.
- `COUNT(DISTINCT)` still translates to SQL `COUNT(DISTINCT ...)` in [src/sparql/translate/group.rs](../src/sparql/translate/group.rs#L216-L236), with no HLL path found in `src/sparql`.
- Merge still calls local `brin_summarize_new_values` in [src/storage/merge.rs](../src/storage/merge.rs#L407-L445); it does not call `brin_summarize_vp_shards_impl`.
- Multi-hop pruning support exists as `ShardPruneSet`/`prune_hop`, but both are marked `#[allow(dead_code)]` in [src/citus.rs](../src/citus.rs#L740-L835), and no SPARQL translator call sites were found.
- Tests in [tests/pg_regress/sql/construct_rules.sql](../tests/pg_regress/sql/construct_rules.sql#L90-L120) only check function existence and no-Citus fallbacks for these v0.63 Citus helpers.

Impact:

- Citus users can see coordinator fan-out, exact distinct bottlenecks, blocking promotion, and stale worker BRIN summaries despite release claims.
- The codebase risks accumulating public APIs whose semantics are not connected to the query/write paths they are supposed to improve.

Recommended remediation:

- Split Citus items into "available API", "translator integrated", "worker integrated", and "CI-proven" status.
- Add Citus-conditional integration tests for real sharded clusters, not only non-Citus fallbacks.
- For each roadmap item, add an explain key and a behavioral test proving the optimized path is selected.

### H3. SPARQL Cursor API Materializes Results Before Returning Them

Severity: High

Category: performance, scalability, documentation

Evidence:

- The cursor module says it pages through large result sets, avoiding full materialization, in [src/sparql/cursor.rs](../src/sparql/cursor.rs#L1-L12).
- `sparql_cursor` immediately calls `sparql::sparql(query)` and stores the full `rows` vector before truncating or returning it in [src/sparql/cursor.rs](../src/sparql/cursor.rs#L20-L40).
- CONSTRUCT cursor variants similarly materialize all construct rows, convert to triples, and then chunk in [src/sparql/cursor.rs](../src/sparql/cursor.rs#L42-L120).

Impact:

- Large result sets still consume memory proportional to the full result, not page size.
- The v0.63 Citus streaming fan-out claim cannot be satisfied by this cursor implementation.

Recommended remediation:

- Rework the SPARQL execution path to expose an SPI cursor or set-returning iterator that pulls pages without collecting all rows.
- Ensure HTTP streaming uses the same cursor path with backpressure and cancellation.
- Add a memory-bound regression or pgbench test for multi-million-row result streams.

### H4. SHACL-SPARQL Rules Are Documented As Supported But Code Warns They Are Not

Severity: High

Category: standards compliance, documentation, correctness

Evidence:

- [README.md](../README.md#L48) says `sh:SPARQLRule` and `sh:SPARQLConstraint` are evaluated as native SPARQL queries.
- [docs/src/features/shacl-sparql-rules.md](../docs/src/features/shacl-sparql-rules.md#L5-L68) says pg_ripple supports both validation and inference rules.
- The SHACL-AF bridge explicitly warns that `sh:SPARQLRule` patterns are not compiled and are deferred to a future release in [src/shacl/af_rules.rs](../src/shacl/af_rules.rs#L138-L186).
- SHACL-SPARQL constraints are real: the validator dispatches `ShapeConstraint::SparqlConstraint` to `check_sparql_constraint` in [src/shacl/validator.rs](../src/shacl/validator.rs#L220-L270).

Impact:

- Users may define SHACL-SPARQL inference rules and assume derived triples are produced, while the system only warns and inserts a placeholder catalog entry.
- Standards-compliance claims are too broad.

Recommended remediation:

- Update docs to say `sh:SPARQLConstraint` is supported and `sh:SPARQLRule` is detected but not executed.
- If support is intended for v0.64/v1.0, route `sh:SPARQLRule` through the construct-rule engine once C1 is fixed.

### H5. Release Workflow Allows Docker Build/Push Failure To Continue

Severity: High

Category: release integrity, operations

Evidence:

- The Docker build/push step in [release.yml](../.github/workflows/release.yml#L218-L235) has `continue-on-error: true`.
- The Trivy scan follows in [release.yml](../.github/workflows/release.yml#L237-L244), but it scans the tag whether the just-built image was successfully pushed or not.

Impact:

- A release can appear successful after a failed image build/push, or scan a stale image with the intended tag.
- This undermines the SBOM/CVE hardening story.

Recommended remediation:

- Remove `continue-on-error: true` from build/push.
- Add a digest output check and scan the immutable digest produced by the build step.
- Fail the release if the pushed digest does not match the scanned digest.

## Performance And Scalability Gaps

### P1. Construct-Rule Full Recompute And Provenance Capture Are O(Target Graph), Not Delta-Based

`run_full_recompute` executes inserts and then records provenance by selecting every `source = 1` triple in the target graph for that predicate/graph in [src/construct_rules.rs](../src/construct_rules.rs#L663-L725). This is not restricted to rows created by the current rule execution.

Risks:

- Pre-existing inferred triples can be attributed to the new rule.
- Refresh/drop may remove triples the rule did not derive if provenance was over-attributed.
- Large target graphs make rule creation and refresh expensive.

Fix direction: capture inserted rows directly with `INSERT ... RETURNING` into provenance, or stage rule output with a run id and record exactly those rows.

### P2. Rare-Predicate Promotion Is Synchronous And Not The Claimed Shadow-Table Pattern

[roadmap/v0.63.0-full.md](../roadmap/v0.63.0-full.md#L395-L414) describes non-blocking VP promotion with a shadow table and `promotion_state`. The actual promotion path synchronously takes an advisory lock, creates the VP table, atomically deletes matching `vp_rare` rows, inserts them into the delta table, updates the predicate catalog, and then may distribute the table in [src/storage/mod.rs](../src/storage/mod.rs#L469-L536).

This is much safer than a two-statement move, but it is not non-blocking background promotion. Large predicates can still experience promotion latency and Citus DDL lock cost on the foreground write path.

Fix direction: either update the roadmap to match the current atomic CTE implementation or implement `promotion_state`, dual write/routing, background migration, and a tested final swap.

### P3. Approximate Distinct Is An Availability Probe, Not Query Pushdown

The v0.63.0 roadmap describes opt-in HLL SQL generation in [roadmap/v0.63.0-full.md](../roadmap/v0.63.0-full.md#L362-L382). The source has `approx_distinct_available_impl` in [src/citus.rs](../src/citus.rs#L1067-L1088), but aggregate translation still emits `COUNT(DISTINCT ...)` in [src/sparql/translate/group.rs](../src/sparql/translate/group.rs#L216-L236).

Fix direction: add a planner flag for approximate distinct, emit HLL SQL only when extension and GUC are present, expose the approximation in `sparql_explain`, and test with/without `pg_hll`.

### P4. WCOJ Session Preamble Can Affect More Than The Target Join

The WCOJ path runs a session preamble before query execution in [src/sparql/mod.rs](../src/sparql/mod.rs#L390-L392). If it changes planner settings broadly, it can influence unrelated subplans inside the same statement or transaction. The right model is local, scoped, and explain-visible planner guidance, not opaque session mutation.

Fix direction: use `SET LOCAL` inside a transaction-scoped SPI execution or avoid session GUC changes by rewriting only the target SQL fragment.

### P5. Arrow Flight And HTTP Export Do Not Enumerate Promoted VP Tables

The HTTP endpoint builds a simplified query against `vp_rare` and an always-empty lateral subquery for promoted tables in [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs#L1945-L1968). It then returns metadata instead of rows.

Fix direction: enumerate `_pg_ripple.predicates`, stream `vp_rare` plus every promoted VP view/table for the graph, and test parity with `sparql_construct`/export outputs.

## Security And Isolation Gaps

### S1. Mutable GitHub Actions Remain The Highest Supply-Chain Gap

Covered as C3. This should be treated as a v1.0 blocker.

### S2. Arrow Flight Tickets Lack Integrity, Expiry, And Authorization Binding

Covered as C4. The comments promise HMAC but [src/flight.rs](../src/flight.rs#L34-L59) emits unsigned JSON. The HTTP endpoint trusts request-body graph data in [pg_ripple_http/src/main.rs](../pg_ripple_http/src/main.rs#L1911-L1942). For bearer-token deployments this is still weaker than a bound, expiring, least-privilege ticket.

### S3. User-Facing SQL Assembly Still Leans On Manual Escaping

`create_construct_rule` validates names but builds the catalog insert via string interpolation and manual quote escaping for SPARQL, target graph, mode, generated SQL, and source graphs in [src/construct_rules.rs](../src/construct_rules.rs#L492-L518). This is not an immediate demonstrated injection because single quotes are escaped, but it is brittle and inconsistent with the project's own guidance to use structured APIs.

Fix direction: use `Spi::run_with_args`/parameterized SQL for all values and reserve formatting for internally generated identifiers that have been reduced to numeric predicate ids or validated identifiers.

### S4. Security Claims Need Runtime Authorization Tests

Positive: `SECURITY DEFINER` lint exists and passed, and graph RLS tests exist in the suite list. Gap: v0.63.0 claims per-worker RLS propagation tests in [roadmap/v0.63.0-full.md](../roadmap/v0.63.0-full.md#L416-L424), but no `citus_rls_propagation` test file was found. For Citus, a single-node RLS test is not enough.

Fix direction: add a Citus integration test that directly queries workers before/after grant/revoke as the roadmap describes.

## Correctness And Standards-Compliance Gaps

### SC1. Public SHACL-AF Rule Support Is Overstated

Covered as H4. The correct current claim is: SHACL-SPARQL constraints are supported; simple `sh:TripleRule` can be compiled to Datalog when inference is enabled; `sh:SPARQLRule` is detected and warned but not executed.

### SC2. CONSTRUCT Writeback Violates Its Own CWB Test Matrix

[roadmap/v0.63.0-full.md](../roadmap/v0.63.0-full.md#L260-L285) lists tests for target population, incremental insert/delete, refresh, stratification, shared target preservation, and drop retraction. The actual pg_regress fixture in [tests/pg_regress/sql/construct_rules.sql](../tests/pg_regress/sql/construct_rules.sql#L1-L130) does not cover these success cases.

Fix direction: implement the listed test matrix before treating v0.63.0 as complete.

### SC3. Changelog Dates And Version Narratives Are Incoherent

[CHANGELOG.md](../CHANGELOG.md#L16-L44) dates v0.63.0 and v0.62.0 as "2025", while later v0.60.0/v0.59.0 entries are dated in 2026 according to repository history reviewed during this assessment. [README.md](../README.md#L22) still anchors "What works today" at v0.59.0 despite v0.63.0 metadata.

Impact: operators cannot tell which promises belong to which installed version.

Fix direction: add a release-status table with `Implemented`, `Experimental`, `Stub`, `Planned`, and `CI-gated` columns, then update README/changelog from that source.

### SC4. WCOJ Documentation Describes Nonexistent Plan Nodes

[docs/src/features/advanced-inference.md](../docs/src/features/advanced-inference.md#L40-L44) tells users to check for a leapfrog node. The implementation emits SQL hints/CTEs, not a PostgreSQL custom scan node. This is a documentation-correctness issue even if the performance hint is useful.

## Test Coverage And Validation Gaps

The suite is broad, but recent releases need deeper, behavior-first acceptance tests.

Gaps to close first:

- Construct-rule happy path: create rule, verify target graph, insert source, delete source, refresh, drop with and without retraction, promoted predicate, post-merge, shared target graph.
- Construct-rule stale graph test: prove no manual refresh is needed if the feature remains advertised as incremental.
- Arrow Flight: real Arrow IPC response decode, malformed/tampered/expired ticket rejection, named graph authorization, promoted-table coverage, throughput benchmark.
- WCOJ: actual `sparql_explain` evidence and benchmark gate for cyclic patterns. If the claim remains Leapfrog Triejoin, add algorithm-specific validation or remove the claim.
- Citus: service pruning integrated into SPARQL, HLL aggregate SQL emitted, cursor streaming memory ceiling, non-blocking promotion, per-worker BRIN summarise after merge, worker-direct RLS propagation.
- Release workflows: lint for SHA-pinned actions; digest-based Docker scan.
- Documentation tests: examples in README/docs should map to real SQL function signatures.

Positive coverage to preserve:

- 181 pg_regress SQL files and 181 expected files.
- CI has a required W3C SPARQL smoke subset, blocking Jena/WatDiv/LUBM/OWL 2 RL jobs in [ci.yml](../.github/workflows/ci.yml#L230-L900), while W3C full and entailment are informational.
- `cargo deny`, `cargo audit`, and PT/security lints are good quality gates.

## Architecture And Maintainability Gaps

### A1. Release Claims Are Not Tied To Acceptance Gates

The project has rich roadmaps, but multiple items appear to have been marked released when only API shells, docs, or fallback behavior existed. This is a process architecture issue.

Fix direction: for each roadmap item require:

| Status | Required evidence |
|---|---|
| Implemented | Code path wired into normal execution, behavior test, docs updated. |
| Experimental | Feature flag/GUC, warning docs, limited test. |
| Stub | Publicly labeled as stub; no production claims. |
| CI-gated | Dedicated CI acceptance test or benchmark gate. |

### A2. Generated Dynamic SQL Needs A Safer Boundary

pg_ripple necessarily generates SQL for VP tables and SPARQL translation, but user-provided values should cross into SQL through parameters. Recent construct-rule code uses manual escaping, while several storage paths format numeric identifiers. The latter is mostly acceptable; the former should be cleaned up before the feature grows more complex.

### A3. Feature Modules Need Explicit Degradation Semantics

Citus, Arrow Flight, SHACL-AF rules, WCOJ, vector extensions, and HTTP endpoints often degrade when dependencies are absent. That is good, but each degradation needs a stable surface:

- Does the function return empty, false, warning, error, or stub JSON?
- Is degraded behavior visible in `sparql_explain`, metrics, logs, or SQL status functions?
- Is degraded behavior documented as safe or only a placeholder?

### A4. Dead Or Unwired Helpers Should Not Be Released As Capabilities

`ShardPruneSet` and `prune_hop` are a concrete example: useful helpers, but dead/unwired in [src/citus.rs](../src/citus.rs#L740-L835). Similar patterns exist for Citus service pruning and approximate distinct. Keeping these helpers is fine; presenting them as delivered distributed execution is not.

## Operational And Observability Gaps

Key missing operational signals:

- Construct-rule freshness: last successful incremental run, pending source deltas, failed rule count, stale target graph flag.
- Construct-rule provenance health: per-rule triple counts, over-attribution detector, retraction failures.
- Merge/Citus BRIN health: coordinator vs worker summarise status, last failure per predicate, shard lag.
- Cursor/export memory metrics: rows materialized, rows streamed, bytes sent, cancellation count.
- Arrow Flight metrics: ticket validation failures, expired/tampered tickets, batches sent, bytes sent, backpressure waits.
- Documentation/status telemetry: a SQL function such as `pg_ripple.feature_status()` that reports whether optional capabilities are implemented, configured, degraded, or unavailable.

Positive note: the project already has monitoring surfaces and Prometheus support, so this is an extension of an existing pattern rather than a new subsystem.

## Documentation And Developer-Experience Gaps

Highest-priority doc corrections:

- Update [README.md](../README.md#L22) from v0.59.0 to v0.63.0 or replace it with a generated current-version variable.
- Downgrade Arrow Flight docs in [README.md](../README.md#L36-L37) and [CHANGELOG.md](../CHANGELOG.md#L48-L54) to "ticket and JSON stub" until real Arrow IPC exists.
- Change WCOJ docs from "Leapfrog Triejoin algorithm" to "cyclic BGP planner hints" unless a true triejoin executor is built.
- Correct SHACL-SPARQL rule docs in [README.md](../README.md#L48) and [docs/src/features/shacl-sparql-rules.md](../docs/src/features/shacl-sparql-rules.md#L5-L68).
- Update [plans/implementation_plan.md](implementation_plan.md#L5-L20), which still references pgrx 0.17, while [Cargo.toml](../Cargo.toml#L19-L21) uses pgrx 0.18.0.
- Fix v0.63 Citus function signatures in [CHANGELOG.md](../CHANGELOG.md#L30-L37) to match [src/citus.rs](../src/citus.rs#L982-L1007).
- Add a "Known limitations in current release" section at the top of README, not buried in individual feature pages.

Developer-experience improvements:

- Add `just assess-release` that runs migration continuity, action pinning lint, PT/security lints, docs signature checks, and feature-status smoke tests.
- Generate SQL API docs from pgrx metadata or a small source parser to avoid signature drift.
- Add docs tests that execute README examples against a temporary database.

## Roadmap Gaps And Strategic Opportunities

The roadmap is already ambitious. The missing strategic element is not more scope; it is a quality model that makes the ambition safe.

Recommended roadmap additions:

1. Release Truth Gate
   - Every roadmap item must carry an implementation status and acceptance evidence link.
   - Changelog entries cannot say "implements" unless tests cover the normal execution path.

2. Feature Status SQL API
   - Add `pg_ripple.feature_status()` returning feature name, status, configured dependencies, degraded reason, CI gate, and docs URL.
   - Useful for operators and for keeping docs honest.

3. Incremental Maintenance Unification
   - Unify Datalog DRed, CONSTRUCT writeback, SHACL-AF rules, CDC, and live views under one delta/provenance engine.
   - Avoid separate partial provenance mechanisms that disagree on ownership and retraction.

4. Distributed Execution Contract
   - Define what Citus integration means at each layer: storage distribution, query pruning, recursive path pushdown, aggregate pushdown, worker-local inference, RLS propagation, and observability.
   - Make each layer independently testable.

5. Streaming Contract
   - Define a single streaming abstraction used by SPARQL cursors, HTTP responses, Arrow Flight, export functions, and Citus fan-out.
   - Include cancellation, memory ceilings, backpressure, and row/byte metrics.

6. Security Hardening Track
   - Action SHA pinning, release digest validation, ticket signing, threat model, fuzz targets for ticket/parser surfaces, dependency policy, and operator hardening guide.

## Recommended New Features

These would materially improve the project and roadmap after the current correctness gaps are closed.

### 1. `pg_ripple.feature_status()` And `/ready` Deep Readiness

A user-visible feature status table would reduce confusion immediately:

| feature | status | dependency | degraded_reason | ci_gate |
|---|---|---|---|---|
| construct_writeback | manual_refresh | none | incremental hook missing | construct_rules_behavior |
| arrow_flight | stub | arrow-flight feature | ipc streaming missing | arrow_flight_decode |
| wcoj | planner_hint | none | no triejoin executor | wcoj_benchmark |
| citus_hll_distinct | unavailable | hll | translator not wired | citus_hll |

Expose the same information through `pg_ripple_http /ready` so operators know whether optional features are actually active.

### 2. Delta Maintenance Kernel

Build one internal API for derived-triple ownership:

- `begin_derivation_run(rule_id, source_delta)`
- `insert_derived(rule_id, p, s, o, g, provenance)`
- `retract_derived(rule_id, source_delta)`
- `commit_derivation_run`

Use it for Datalog, CONSTRUCT writeback, SHACL rules, and future materialized SPARQL views.

### 3. Explainable Distributed SPARQL

Add `sparql_explain(..., distributed := true)` output showing:

- shard pruning reason
- pushed-down paths and aggregates
- worker SQL fragments
- coordinator materialization estimates
- approximate vs exact aggregate mode
- fallback reason when not pushed down

This would turn Citus features from invisible magic into debuggable operator tools.

### 4. Real Arrow Flight/ADBC Export Path

Implement a proper Arrow export stack:

- signed tickets
- Arrow IPC stream
- dictionary-decoded columns and optional encoded columns
- batch sizing and compression
- graph/predicate filters
- benchmark against COPY and JSON-LD export

This would make pg_ripple much more attractive for analytics, data lake, and ML workflows.

### 5. Release Evidence Dashboard

Generate an artifact per release containing:

- migration chain result
- conformance pass rates
- benchmark deltas vs baseline
- fuzz corpus status
- action pinning result
- docs/API signature drift result
- Docker digest and Trivy report

This would turn the current roadmap richness into a trustworthy release process.

### 6. Canonical Graph Pipeline UI/API

Once CONSTRUCT writeback is real, expose pipeline introspection:

- dependency graph
- last refresh/incremental run
- pending deltas
- rule order
- derived triple counts
- failed rules and retry controls

This fits pg_ripple's raw-to-canonical story and would be genuinely differentiated.

## Path To World-Class Status

The shortest path is to stop adding major surface area until recent claims are made executable.

### Phase 1: Truth And Safety Freeze

Target: 1-2 weeks

- Pin GitHub Actions to SHAs and add lint.
- Correct README/changelog/docs for Arrow Flight, WCOJ, SHACL-SPARQL rules, Citus status, and v0.63 construct writeback.
- Add `feature_status()` with honest statuses for the disputed features.
- Remove Docker release `continue-on-error` and scan immutable digests.

### Phase 2: v0.63 Correctness Closure

Target: 2-4 weeks

- Implement or explicitly de-scope incremental CONSTRUCT maintenance.
- Fix promoted-predicate retraction through HTAP storage primitives.
- Fix provenance over-attribution by capturing exact inserted rows.
- Add the full construct-rule test matrix from [roadmap/v0.63.0-full.md](../roadmap/v0.63.0-full.md#L260-L285).

### Phase 3: Streaming And Distributed Reality

Target: 4-8 weeks

- Make SPARQL cursor truly streaming.
- Wire Citus service pruning, HLL distinct, per-worker BRIN, and promotion state into normal paths or mark them planned.
- Add Citus integration tests that prove worker behavior.
- Implement Arrow IPC streaming or rename the feature.

### Phase 4: Production Hardening

Target: v1.0.0

- 30-day or at least 72-hour soak test with published workload and artifacts.
- Third-party security audit or internal threat model with tracked findings.
- Public benchmark baselines for BSBM, WatDiv, LUBM, bulk load, merge, vector search, and Arrow export.
- Upgrade/backup/restore acceptance tests from supported previous versions.

## Prioritized Next Actions

1. Pin every GitHub Action to a full commit SHA and add a CI lint that fails mutable refs.
2. Change README/changelog docs to mark CONSTRUCT writeback as manual full refresh until incremental maintenance is implemented.
3. Add construct-rule happy-path and retraction tests before touching more Citus or Arrow code.
4. Fix construct-rule retraction for promoted HTAP predicates by using delta/tombstone storage operations.
5. Change Arrow Flight docs/status to stub or implement signed-ticket Arrow IPC streaming.
6. Correct WCOJ wording to planner hints unless a real Leapfrog Triejoin executor is introduced.
7. Wire `approx_distinct_available` into aggregate translation or downgrade the Citus HLL claim.
8. Make `sparql_cursor` truly streaming or rename it to a chunked materialized export helper.
9. Add `pg_ripple.feature_status()` to make optional and partial features visible.
10. Add release evidence generation and prevent future changelog/API signature drift.

## Final Assessment

pg_ripple is impressive because it tries to bring serious semantic-web, graph, reasoning, and analytics functionality into PostgreSQL rather than wrapping a separate engine. The core direction is strong. The storage model is coherent, the test surface is large, and the project has accumulated many useful primitives.

The blocking issue is credibility at the release boundary. Recent docs and changelog entries present several capabilities as production-grade when the implementation is partial, stubbed, or not wired into the normal execution path. That is fixable, but it needs a deliberate pause: tighten the claims, close v0.63 correctness, make streaming and Citus behavior real, and turn roadmap bullets into executable acceptance gates.

If those changes land, pg_ripple can credibly move from "very capable beta with overstated edges" to a world-class PostgreSQL-native RDF platform.