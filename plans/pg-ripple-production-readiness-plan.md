# pg-ripple Production Readiness Implementation Plan

**Status:** Proposed roadmap replacement  
**Baseline:** pg-ripple v0.128.0  
**Plan date:** 2026-08-26  
**Target:** pg-ripple v1.0.0 General Availability  
**Applies to:** PostgreSQL extension, `pg_ripple_http`, SQL migrations, Docker/Helm packaging, CI, documentation, and release engineering

---

## 1. Executive decision

pg-ripple should enter a **feature freeze immediately**. No new product families, query-language extensions, reasoning profiles, cloud integrations, or ecosystem adapters should be added before v1.0.0.

The current repository roadmap groups the remaining production work into v0.129.0–v0.132.0. That sequence is directionally correct, but it compresses critical correctness, upgrade safety, secure defaults, conformance integrity, crash recovery, performance qualification, API stability, external audit, and final soak testing into too few releases. This plan replaces that sequence with smaller, independently verifiable milestones.

The production-readiness program should follow five rules:

1. **Release by evidence, not by date.** A version ships only after every exit criterion is met.
2. **Fail closed.** Missing authentication, missing conformance data, failed migrations, unknown feature status, and unavailable security dependencies must stop startup or fail CI rather than degrade silently.
3. **Stable means end-to-end verified.** Object-existence tests, compilation-only tests, and successful skips do not qualify a feature as stable.
4. **The exact release artifact is the test subject.** Package installation, migration, container, HTTP, backup, and soak tests must run against built release artifacts rather than only a source checkout.
5. **Any release-candidate code change resets qualification.** A change after audit or soak testing requires rerunning the affected gates; a correctness, migration, security, storage, or worker change resets the full qualification run.

---

## 2. Current blockers and roadmap disposition

| Blocker | Current consequence | Remediation version |
|---|---|---|
| HTTP router contains obsolete Axum path captures | `pg_ripple_http` can panic during router construction and never listen | v0.128.1 |
| Production Docker image adds global PostgreSQL `trust` rules | Any reachable client can authenticate without a password | v0.128.1 |
| Async JSON writeback can report enabled without a complete event path | Silent RDF/relational divergence | v0.128.1 containment; v0.129.0 full repair |
| Direct JSON writeback has incorrect affected-row and type semantics | False success and failures on normal typed relational schemas | v0.129.0 |
| Migration-chain test stops before the current version | Upgrade defects can ship undetected | v0.130.0 |
| HTTP authentication is optional and public bind is the default | Accidental unauthenticated exposure | v0.131.0 |
| Required conformance jobs may skip or remain non-blocking | Green CI does not prove public conformance claims | v0.132.0 |
| Crash recovery, backup/restore, and failover evidence is incomplete | Operational behavior is not qualified | v0.133.0 |
| Scale evidence is stale, important scans are unbounded or quadratic, and HTTP result paths buffer or do not emit rows | Performance and streaming claims are not release-grade | v0.134.0 |
| Applications lack typed initial bindings, SPARQL cannot opt into the existing prefix registry, and public contracts remain fluid | Callers interpolate query text and there is no defensible compatibility promise | v0.135.0 |
| No external security audit report is on file | Native superuser extension and HTTP attack surface lack independent assurance | v0.136.0 |
| No exact-candidate 72-hour mixed-workload qualification artifact is on file | Long-running stability remains unproven | v0.137.0–v0.143.0 |

---

## 3. Definition of production ready

pg-ripple is production ready only when all of the following are true for the exact v1.0.0 candidate.

### 3.1 Correctness and data integrity

- No known open Critical or High correctness defects.
- Every feature labeled `stable` has at least one release-blocking end-to-end test.
- Randomized mutation tests show no lost, duplicated, or incorrectly retracted triples.
- Background processing is transactional, idempotent, restartable, and observable.
- Fresh install and supported upgrade paths result in equivalent catalog schemas and behavior.
- Backup, restore, point-in-time recovery, and replica promotion preserve graph data and extension metadata.

### 3.2 Secure defaults

- The production image never enables PostgreSQL `trust` authentication for remote clients.
- The HTTP service refuses a non-loopback bind without authentication unless an explicit development override is set.
- Administrative and mutating endpoints are protected separately from read-only endpoints.
- Database connections support certificate verification and a least-privilege service role.
- Outbound federation, rule-library, LLM, and cloud requests use a common SSRF and egress policy.
- No open Critical or High findings remain from dependency scanning, container scanning, internal review, or the external audit.

### 3.3 Conformance and product truth

- Required test corpora are immutable, checksum-verified, and always present in release CI.
- A required suite fails if zero tests run, a download fails, an expected test disappears, or an unexpected failure occurs.
- README badges and claims are generated from versioned release artifacts.
- `feature_status()` is validated against executable tests, documentation, dependencies, and release status.
- Experimental capabilities are not described as production-supported.

### 3.4 Operational readiness

- Background workers expose health, backlog, retry, dead-letter, duration, and failure metrics.
- Runbooks cover installation, secure deployment, upgrade, rollback, backup, restore, PITR, worker recovery, disk pressure, and incident diagnosis.
- Kubernetes liveness, readiness, shutdown, PodDisruptionBudget, and NetworkPolicy behavior is tested.
- A 72-hour mixed workload completes with no data-integrity failure, process crash, unrecovered worker failure, or unbounded resource growth.

### 3.5 Compatibility and support

- The stable SQL API, HTTP API, GUC set, catalog contract, extension/sidecar compatibility window, and deprecation policy are published.
- Breaking changes are machine-detected against a checked-in API manifest.
- The supported PostgreSQL versions, operating systems, architectures, and upgrade origins are explicit.
- Security reporting, patch policy, and support expectations are documented.

---

## 4. Scope control before v1.0.0

### 4.1 Stable-core candidate

The following capabilities may become stable in v1.0.0 after satisfying all gates:

- RDF dictionary and storage layer.
- Turtle, N-Triples, N-Quads, TriG, and RDF/XML import/export paths that pass round-trip tests.
- SPARQL 1.1 SELECT, ASK, CONSTRUCT, DESCRIBE, UPDATE, named graphs, and property paths within the pinned conformance scope.
- Core HTAP delta/main merge and maintenance operations.
- SHACL Core, if the pinned suite has zero unexpected failures.
- Datalog/RDFS/OWL RL functionality whose supported semantics are explicitly enumerated and tested.
- The standards-based `/sparql`, `/health`, `/ready`, and protected metrics endpoints.
- Install, upgrade, backup, restore, and operational tooling.

### 4.2 Conditional-stable candidates

These can be promoted only if their dedicated acceptance suites pass:

- JSON-to-relational writeback.
- CDC and live subscriptions.
- Temporal snapshots and diff.
- R2RML materialization.
- Arrow IPC export.
- Vector/SPARQL hybrid search.
- Citus distributed execution.

### 4.3 Beta or experimental by default at v1.0.0

Unless separately qualified, these should remain available but carry no stable compatibility or production-support promise:

- LLM and natural-language-to-SPARQL functionality.
- Rule-library federation and marketplace behavior.
- PPRL and differential-privacy helpers.
- Neuro-symbolic entity resolution.
- Hypothetical reasoning and automated rule drafting.
- GraphRAG-specific export and external AI integrations.
- Experimental Citus optimizations, planner hints, or dependency-degraded paths.

No beta or experimental feature may block the stable core from shipping, but it must fail safely and advertise its actual status.

---

## 5. Proposed version roadmap

| Version | Theme | Release type | Primary gate |
|---|---|---|---|
| **v0.128.1** | Emergency containment and safe patch | Out-of-band patch | No startup panic, no passwordless production image, no false-enabled async writeback |
| **v0.129.0** | JSON writeback and mutation integrity | Correctness release | Full insert/update/delete/retry/restart writeback matrix passes |
| **v0.130.0** | Installation and migration integrity | Upgrade release | Fresh install and every supported upgrade path are schema- and behavior-equivalent |
| **v0.131.0** | Secure-by-default runtime and packaging | Security release | Production deployments fail closed and least privilege is verified |
| **v0.132.0** | Conformance, feature truth, and release evidence | Assurance release | Required suites cannot skip; claims are artifact-backed |
| **v0.133.0** | Crash recovery, backup, failover, and operations | Resilience release | Fault-injection and recovery matrix passes |
| **v0.134.0** | Performance, scale, and true streaming qualification | Performance release | Large results use bounded memory and backpressure; disconnects and deadlines cancel PostgreSQL work; current evidence passes |
| **v0.135.0** | Safe application query API and compatibility freeze | RC0 | Typed bindings and registered-prefix mode pass security/cache gates; stable manifest and breaking-change gate are active |
| **v0.136.0** | External audit remediation and hardened candidate | RC1 | Audit covers streaming, bindings, prefix privileges, and invalidation with no unresolved Critical or High findings |
| **v0.137.0** | Qualification foundation and PITR | Qualification release | PITR reaches the named target and meets the clean promotion set |
| **v0.138.0** | Establish the supported Citus storage model | Architecture checkpoint | Distributed merge works safely, or explicit no-merge mode is supported and tested |
| **v0.139.0** | Qualify multi-node Citus | Qualification release | The required multi-node Citus gate meets the clean promotion set |
| **v0.140.0** | Qualify HTTP resilience | Resilience release | Bounded traffic, cancellation, readiness, pool, and shutdown behavior meet the gate |
| **v0.141.0** | Establish durable queue processing guarantees | Correctness release | JSON writeback and SHACL queues conserve counts and every qualification ID through restart |
| **v0.142.0** | Qualify injected faults and queue delivery | Fault qualification | Packet faults and embedding and bidi delivery guarantees meet the gate |
| **v0.143.0** | Complete sustained qualification | RC2 | Nightly, 6-hour, 24-hour, and 72-hour profiles meet repeated-qualification criteria and produce release evidence |
| **v1.0.0** | General Availability | GA | Exact qualified candidate promoted with complete evidence bundle |

The v0.137.0–v0.143.0 sequence is detailed in the [fail-closed CI coverage plan](../plan-failed-closed-ci-coverage.md), which retains the fifteen-pull-request delivery sequence.

---

# 6. Detailed implementation plan by version

## v0.128.1 — Emergency containment and safe patch

### Objective

Stop distribution of known release-breaking behavior without waiting for the broader refactors.

### Required changes

#### HTTP startup

- Replace every Axum 0.7-style `:capture` route with Axum 0.8 `{capture}` syntax in `pg_ripple_http/src/routing/mod.rs`.
- Refactor router construction so it can be invoked in a test without an active PostgreSQL connection.
- Add `pg_ripple_http/tests/router_construction.rs`:
  - Construct the complete router under `std::panic::catch_unwind`.
  - Enumerate all static and parameterized routes.
  - Send in-process requests using `tower::ServiceExt::oneshot`.
  - Assert each route resolves to an expected authentication, method, or handler response rather than `404` or a panic.
- Add `pg_ripple_http/tests/process_smoke.sh`:
  - Start PostgreSQL from the release image.
  - Start the built `pg_ripple_http` binary on an ephemeral port.
  - Poll `/health` and `/ready`.
  - Terminate with SIGTERM and verify clean exit.
- Replace the current compilation-only sidecar CI check with both compilation and startup gates.

#### Production database authentication

- Remove `docker/00-pg_hba.sh` from the production image.
- Publish a separate development image or opt-in initialization script, such as `pg-ripple-dev`, that clearly enables trust authentication.
- Make the production Compose file bind PostgreSQL to `127.0.0.1` by default unless the operator explicitly changes it.
- Add `tests/container/test_pg_hba.sh`:
  - Confirm passwordless TCP authentication fails.
  - Confirm SCRAM authentication with the configured password succeeds.
  - Confirm local Unix-socket behavior matches the documented policy.

#### Async writeback containment

Until v0.129.0 completes the event architecture:

- Change `enable_json_writeback()` to fail with a dedicated error when complete enqueue coverage cannot be installed.
- Never set `writeback_enabled = true` unless a validated event path exists.
- Mark `json_mapping_writeback` as `broken` or `experimental-disabled` in `feature_status()`.
- Add a visible warning to the v0.128.0 release notes and documentation.
- Consider yanking the affected `pg_ripple_http` crate version while preserving immutable release artifacts.

### Required CI jobs

- `http-router-construction`
- `http-process-smoke`
- `container-auth-negative-test`
- `writeback-enable-fails-closed`

### Exit criteria

- The release binary starts and responds to health checks.
- No production image accepts unauthenticated remote PostgreSQL connections.
- Async writeback cannot claim to be enabled when automation is unavailable.
- The v0.128.0 release page contains an explicit known-issues notice.

---

## v0.129.0 — JSON writeback and mutation integrity

### Objective

Replace the fragile per-table trigger design with a transactional, storage-level event path and make direct writeback correct for normal PostgreSQL schemas.

### Architecture decision

Adopt a **single mutation-journal integration** rather than installing independent triggers on `vp_rare`, every delta table, tombstone tables, and future promoted tables.

All RDF mutations already pass through central storage operations or the mutation journal. Writeback events should be derived there so that rare predicates, promoted predicates, tombstones, bulk operations, retries, and future storage changes cannot bypass automation.

Create `docs/adr/ADR-0001-json-writeback-event-path.md` documenting this decision and the rejected per-table-trigger alternative.

### Required changes

#### Module split

Replace `src/json_mapping.rs` with:

```text
src/json_mapping/
├── mod.rs
├── registry.rs
├── ingest.rs
├── export.rs
├── configuration.rs
├── writeback.rs
├── queue.rs
├── mutation_events.rs
└── tests.rs
```

No file should exceed the future 800-line soft threshold.

#### Public configuration API

Add:

```sql
pg_ripple.configure_json_writeback(
    mapping_name text,
    target_schema text,
    target_table text,
    key_columns text[],
    conflict_policy text default 'replace',
    enabled boolean default false
) returns jsonb
```

The function must validate:

- Mapping existence.
- Target relation existence and relation kind.
- All key-column names.
- Duplicate or empty key names.
- Generated, identity, dropped, and non-insertable columns.
- Conflict policy.
- Required privileges of the effective execution role.
- Target column PostgreSQL types through `pg_attribute` and `pg_type`.

Add:

```sql
pg_ripple.inspect_json_writeback(mapping_name text) returns jsonb
```

The result should include target OID, resolved columns and types, enabled state, pending count, retry count, dead-letter count, last success, and last error.

Direct writes to `_pg_ripple.json_mappings` should no longer be documented as supported configuration.

#### Transactional event generation

- Extend the mutation journal with a compact event record containing subject, predicate, graph, operation, and statement/transaction identifiers.
- At statement end, resolve changed predicates against configured mappings.
- Deduplicate events by `(mapping_id, subject_id, operation)` within the transaction.
- Insert queue rows in the same transaction as the RDF mutation.
- Ensure rollback removes both RDF changes and corresponding queue events.
- Define update coalescing rules:
  - Insert followed by update becomes one upsert.
  - Insert followed by delete in the same transaction becomes no event.
  - Repeated changes to the same subject become one final-state upsert.

#### Queue schema and worker

Extend `_pg_ripple.json_writeback_queue` with:

```text
attempt_count       integer not null default 0
next_attempt_at     timestamptz not null default now()
locked_at           timestamptz
locked_by           text
last_error          text
processed_at        timestamptz
dead_lettered_at    timestamptz
created_xid         xid8 or bigint transaction identifier
```

Add a partial unique index preventing duplicate pending events for the same mapping, subject, and operation.

The worker must:

- Claim work with `FOR UPDATE SKIP LOCKED`.
- Process a bounded batch.
- Update row state in the same transaction as each target write or a defined batch.
- Use exponential backoff with jitter.
- Move permanently failing rows to dead-letter state after a configurable limit.
- Expose retry and dead-letter metrics.
- Recover stale locks after worker or PostgreSQL restart.
- Be safe with multiple workers if parallelism is enabled later.

#### Correct target SQL

- Quote identifiers using PostgreSQL identifier APIs, never manual string escaping.
- Resolve target types from catalog OIDs.
- Generate explicit parameter casts to the target type.
- Preserve JSON null versus absent property semantics.
- Reject lossy conversions unless a mapping explicitly declares one.
- Use `RETURNING` or SPI processed-row metadata to return actual affected rows.
- Make `skip` return zero on conflict.
- Make a zero-row delete return zero.
- Validate all composite-key values before generating placeholders.
- Exclude generated columns from INSERT/UPDATE lists.

#### Observability

Add metrics:

- `pg_ripple_json_writeback_enqueued_total`
- `pg_ripple_json_writeback_processed_total`
- `pg_ripple_json_writeback_failed_total`
- `pg_ripple_json_writeback_dead_letter_total`
- `pg_ripple_json_writeback_pending`
- `pg_ripple_json_writeback_oldest_pending_seconds`
- `pg_ripple_json_writeback_duration_seconds`

Add structured logs with mapping name, queue ID, attempt count, target relation OID, and trace ID; never log row secrets.

### Required test matrix

Test all of the following through an actual background worker and release artifact:

- Rare predicate insert, update, and delete.
- Predicate promotion before and after enabling writeback.
- Delta and tombstone paths.
- Main-table delete.
- Named graph isolation.
- Bulk load and SPARQL UPDATE.
- Transaction rollback.
- PostgreSQL restart with pending work.
- Worker crash after target write but before queue status update.
- Idempotent replay.
- Text, integer, bigint, numeric, boolean, UUID, date, timestamp, JSONB, enum, and nullable columns.
- Single and composite keys.
- `replace`, `skip`, and `error` conflict policies.
- Missing key, invalid type, generated column, permission failure, and dead-letter behavior.

### Exit criteria

- No RDF mutation path bypasses event generation for a configured mapping.
- Queue and target state remain correct across rollback, restart, and replay.
- Affected-row counts are accurate.
- All supported PostgreSQL target types in the acceptance matrix round-trip correctly.
- No writeback configuration requires direct internal-catalog mutation.

---

## v0.130.0 — Installation and migration integrity

### Objective

Make fresh installation and upgrades independently verifiable and impossible to bypass with a vacuous migration test.

### Required changes

#### Migration graph tool

Create `scripts/migration_graph.py` that:

- Parses every `sql/pg_ripple--FROM--TO.sql` filename.
- Reads `default_version` from `pg_ripple.control` independently.
- Validates semantic version syntax.
- Detects gaps, cycles, duplicate edges, ambiguous paths, and unreachable versions.
- Produces the exact ordered path for each supported starting version.
- Fails if the current `default_version` is not reachable.
- Emits a machine-readable `target/migration-graph.json` artifact.

#### Dynamic chain test

Rewrite `tests/test_migration_chain.sh` or replace it with a Rust/Python harness that:

1. Installs an explicit baseline release.
2. Loads a representative data fixture.
3. Applies every migration returned by the independent graph tool.
4. Verifies the final installed extension version equals the control file version.
5. Executes post-latest feature assertions.
6. Verifies data and permissions after every checkpoint class.

The script must not derive the “applied checkpoint” from the same list used to calculate the expected endpoint.

#### Fresh-install versus upgrade equivalence

Create `scripts/schema_fingerprint.sql` and `scripts/compare_schema_fingerprints.py`.

The normalized fingerprint should include:

- Schemas and object ownership.
- Tables, columns, types, defaults, nullability, identity/generated attributes.
- Indexes, constraints, triggers, policies, RLS flags, sequences, and event triggers.
- Functions, signatures, return types, volatility, parallel-safety, security-definer flag, `search_path`, ACLs, and comments.
- Extension membership.
- Stable GUC names and defaults from the diagnostic API.

Compare:

- Fresh install at the candidate.
- Sequential upgrade from the oldest supported baseline.
- Direct upgrades from each supported recent release artifact.

Only an explicit allowlist may explain intentional differences.

#### Package-level installation matrix

Test the actual archives and container images for:

- Linux amd64.
- Linux arm64.
- macOS arm64 where feasible.
- Windows amd64 where release support is claimed.

At minimum, each artifact must install, create the extension, run the core smoke suite, and unload/drop cleanly.

#### Upgrade recovery

Add tests for:

- Migration failure inside a transaction.
- Restart after failed migration.
- Re-running `ALTER EXTENSION UPDATE` after correction.
- Backup restore followed by upgrade.
- Extension/HTTP sidecar version mismatch in strict mode.

### Support policy decision

Before this version ships, publish the pre-GA upgrade policy. Recommended policy:

- CI runs the complete historical sequential chain.
- Release qualification performs package upgrades from at least the last six published pre-GA versions.
- After v1.0, each minor release supports direct upgrade from the two previous minor releases and sequential upgrade from older supported releases.

### Exit criteria

- `default_version` is independently proven reachable.
- Fresh and upgraded schemas have identical fingerprints except approved differences.
- Representative data survives every supported path.
- Release artifacts, not only source checkouts, pass installation and upgrade tests.

---

## v0.131.0 — Secure-by-default runtime and packaging

### Objective

Make accidental insecure production deployment difficult and explicit development exceptions visible.

### Required changes

#### Typed HTTP configuration

Introduce a `Config` structure loaded once at startup. Validate all values before building pools or routers.

Recommended settings:

```text
PG_RIPPLE_HTTP_BIND
PG_RIPPLE_HTTP_MODE=development|production
PG_RIPPLE_HTTP_AUTH_TOKEN[_FILE]
PG_RIPPLE_HTTP_WRITE_TOKEN[_FILE]
PG_RIPPLE_HTTP_ADMIN_TOKEN[_FILE]
PG_RIPPLE_HTTP_METRICS_TOKEN[_FILE]
PG_RIPPLE_HTTP_ALLOW_UNAUTHENTICATED=0|1
PG_RIPPLE_HTTP_PG_SSLMODE=disable|require|verify-ca|verify-full
PG_RIPPLE_HTTP_PG_CA_FILE
PG_RIPPLE_HTTP_PG_CLIENT_CERT_FILE
PG_RIPPLE_HTTP_PG_CLIENT_KEY_FILE
```

Rules:

- Default bind is loopback outside the container packaging layer.
- A non-loopback bind without a read token is fatal unless the explicit development override is set.
- Production mode rejects the development override.
- Secret-file variants are preferred for Docker/Kubernetes secrets.
- Token values must not be printed in diagnostics.

#### Endpoint authorization registry

Replace scattered handler-level assumptions with a central endpoint classification:

```rust
enum AccessClass {
    PublicHealth,
    Read,
    Write,
    Admin,
    Metrics,
}
```

Generate a route-access manifest and require every route to declare one class. CI should fail on unclassified routes.

Add negative tests proving:

- Missing or invalid tokens are rejected.
- A read token cannot call write/admin endpoints.
- A write token cannot call admin-only endpoints.
- Metrics protection behaves as configured.
- Error bodies do not reveal database details.

#### PostgreSQL TLS and least privilege

- Replace unconditional `NoTls` with configurable TLS.
- Support hostname and CA verification.
- Document a dedicated `pg_ripple_http` database role.
- Provide `sql/roles/pg_ripple_http.sql` granting only the stable HTTP surface.
- Add a test proving the sidecar runs without superuser, database owner, or direct writes to internal tables.

#### Docker and Helm profiles

Publish:

- A minimal production extension image.
- A production all-in-one image with SCRAM, no trust rules, and pinned components.
- A separately named development image that may enable trust authentication.

Helm must require or generate secrets and include:

- NetworkPolicy.
- SecurityContext.
- Read-only root filesystem where possible.
- Dropped Linux capabilities.
- Non-root execution.
- Explicit egress rules for federation/LLM use.
- Secret references rather than literal values.

#### Outbound request policy

Create one shared policy module for federation, rule libraries, LLM calls, and cloud integrations:

- Resolve DNS before connecting.
- Block private, loopback, link-local, multicast, CGNAT, metadata, and configured ranges.
- Revalidate every redirect target.
- Limit redirect count.
- Enforce scheme and port allowlists.
- Support operator endpoint allowlists.
- Set connect, request, response-body, and total deadlines.
- Cap response size.
- Record destination and policy decision without logging credentials.

#### Secret handling

- Reject raw-looking secrets in GUCs intended to hold environment-variable names.
- Require the federation credential encryption key when credential storage is enabled.
- Add credential rotation and key-rotation tests.
- Redact DSNs, tokens, query parameters, and Authorization headers from logs and support bundles.

#### Supply-chain reproducibility

- Replace `go install ...@latest` with an immutable version and checksum.
- Pin base images by digest for releases.
- Pin Git dependencies by commit or verified tag object.
- Verify downloaded source archives with committed checksums.
- Publish provenance for binaries and images.

### Exit criteria

- Production mode cannot start unauthenticated on a public bind.
- Production images reject passwordless database access.
- The HTTP service runs with a least-privilege database role and verified TLS.
- Every route has a tested authorization class.
- No mutable `@latest` input remains in the release build.

---

## v0.132.0 — Conformance, feature truth, and release evidence

### Objective

Make every public capability claim traceable to immutable, executable evidence.

### Required changes

#### Pinned corpus lockfile

Create `tests/conformance/sources.lock` containing, for every suite:

- Suite name.
- Upstream repository.
- Immutable commit SHA.
- Archive SHA-256.
- Expected manifest count.
- Expected test count or accepted range.
- Licensing information.

The fetch scripts must reject mismatched checksums or unexpected corpus shape.

#### Blocking test semantics

Required suites must:

- Fail when the corpus is missing.
- Fail when download or extraction fails.
- Fail when zero tests run.
- Fail when the executed test count differs unexpectedly.
- Fail on every unexpected failure, timeout, or XPASS.
- Treat known failures as temporary exceptions with an issue, owner, rationale, and expiry.

Informational suites must be named `informational`, never `required` or `blocking`.

#### Report format

Every report should include:

```json
{
  "pg_ripple_version": "...",
  "git_sha": "...",
  "artifact_digest": "...",
  "postgres_version": "...",
  "suite": "...",
  "suite_commit": "...",
  "started_at": "...",
  "duration_seconds": 0,
  "expected_total": 0,
  "executed_total": 0,
  "passed": 0,
  "failed": 0,
  "skipped": 0,
  "xfail": 0,
  "xpass": 0,
  "unexpected_failures": []
}
```

Publish reports under `results/conformance/<version>/` and attach them to the GitHub release.

#### Feature evidence manifest

Generate `results/features/<version>/feature-evidence.json` from `feature_status()` plus repository metadata.

Each row must contain:

- Feature name and status.
- Stable API entry points.
- Required dependencies.
- Positive, negative, restart, migration, and security tests.
- Documentation path.
- Last verified version.
- Known limitations.
- Evidence artifact digest.

CI must reject:

- A `stable` feature without an end-to-end gate.
- A nonexistent test or docs path.
- A feature whose dependency-degraded mode is not tested.
- A public README claim that exceeds the evidence status.

#### Route and OpenAPI truth

- Generate the HTTP route inventory from the central router registry.
- Compare it with OpenAPI and documentation.
- Fail on undocumented routes, documented nonexistent routes, or missing access classifications.

#### Release evidence bundle

Create `scripts/build_release_evidence.py` and attach:

```text
release-evidence/<version>/
├── manifest.json
├── build-provenance.json
├── schema-fingerprints/
├── migrations/
├── conformance/
├── feature-evidence.json
├── security/
├── benchmark-summary.json
├── test-counts.json
├── sbom.json
└── checksums.txt
```

Required evidence generation must never use `|| true`.

### Exit criteria

- Required conformance suites cannot pass by skipping.
- Every public stable claim has versioned evidence.
- Route, OpenAPI, docs, and authorization inventories agree.
- The release evidence bundle is complete and signed.

---

## v0.133.0 — Crash recovery, backup, failover, and operations

### Objective

Prove that pg-ripple behaves correctly under the failures expected of a PostgreSQL production system.

### Required changes

#### Test-only fault injection

Behind a non-production test feature, add deterministic fault points around:

- Dictionary insert and cache publication.
- VP delta write.
- Mutation-journal flush.
- Merge phase transitions.
- Predicate promotion.
- Tombstone application.
- JSON writeback target execution.
- Queue status update.
- Datalog materialization and DRed retraction.
- Background-worker checkpoint and exit.

Fault points should support process termination, transaction error, delay, and simulated SPI failure.

#### Crash matrix

Automate PostgreSQL `SIGKILL` and restart tests for:

- Mid-load.
- Mid-merge.
- Mid-promotion.
- Mid-writeback.
- Mid-inference.
- Mid-extension upgrade.

After recovery, verify:

- PostgreSQL starts without manual catalog repair.
- Triple checksums and counts match the committed transaction history.
- No queue row is permanently locked.
- Workers resume or report a durable degraded state.
- Replaying work does not duplicate target changes.

#### Backup, restore, and PITR

Test:

- `pg_dump`/`pg_restore` for logical backup.
- Physical base backup and restore.
- Point-in-time recovery through graph mutations and schema changes.
- Restored extension ownership, policies, sequences, workers, and GUC guidance.
- Restore to a new host and reconnect the HTTP service.

#### Replication and failover

Create a primary/standby test that:

- Replays graph writes and extension catalog changes.
- Promotes the standby.
- Restarts background workers exactly once.
- Preserves queue and merge state.
- Verifies read-replica routing never sends writes to a standby.

#### Resource-pressure behavior

Test:

- Disk full during load and merge.
- Low shared memory.
- Statement timeout and query cancellation.
- Connection pool exhaustion.
- Worker crash loops.
- Queue backlog.
- Oversized parser and federation responses.

The system must fail with bounded errors and actionable metrics rather than panic or spin.

#### Operational APIs and runbooks

Add or verify:

- `pg_ripple.health()` for extension-level health.
- Worker status, last heartbeat, restart count, backlog, and last error.
- Schema version and migration status.
- Cache, merge, dead-letter, and replication diagnostics.

Publish runbooks for every failure scenario in `docs/src/operations/`.

### Exit criteria

- Every crash matrix scenario recovers automatically or reaches a documented, observable safe state.
- Backup, restore, PITR, and promotion preserve data and metadata.
- No fault test produces silent corruption.
- Production runbooks are executable and validated in CI or qualification environments.

---

## v0.134.0 — Performance, scale, and true streaming qualification

### Objective

Replace aspirational or stale performance claims with current, reproducible evidence, bound known pathological paths, and complete the existing HTTP streaming contract with backpressure, deadlines, cancellation, and clean pool reuse.

### Required changes

#### Fix known scale pathologies

- Replace OFFSET pagination in full-table batch scans with keyset pagination.
- Add supporting indexes for the chosen keyset order.
- Add `pg_ripple.max_predicate_union_branches`.
- Reject or replan variable-predicate queries exceeding the bound.
- Implement predicate-candidate pruning from constants, graph restrictions, statistics, or catalog metadata before building `UNION ALL` SQL.
- Measure planning time independently from execution time.
- Bound response, export, inference, and federation memory.

#### Complete true HTTP streaming

- Replace streaming-path `tokio_postgres::Client::query()` calls with direct `query_raw()`/`RowStream` polling from the HTTP body.
- Share one execution path between streaming-safe `/sparql` responses and `/sparql/stream`.
- Preserve typed RDF terms and projected-variable metadata, including empty result sets.
- Emit independently parseable SPARQL Results JSON, CSV, TSV, and N-Triples with bounded encoder buffers.
- Enforce slow-client backpressure, statement and idle deadlines, and PostgreSQL cancellation on disconnect, timeout, shutdown, or encoder failure.
- Commit or roll back before pool reuse and discard connections whose session state cannot be proven clean.
- Record first-byte latency, rows, bytes, active streams, errors, cancellation reasons/failures, and connection discards.

#### Benchmark environment

Use a dedicated, documented environment or stable self-hosted runner. Record:

- CPU model and governor.
- RAM.
- Storage model and filesystem.
- Kernel and container runtime.
- PostgreSQL configuration.
- Compiler and linker versions.
- Dataset checksums.
- Warm/cold cache state.

#### Workload tiers

At minimum publish:

| Tier | Dataset | Purpose |
|---|---:|---|
| Small | 1 million triples | Developer and CI regression |
| Medium | 10 million triples | Typical production qualification |
| Large | 100 million triples | Scale and planning behavior |
| Predicate stress | 1k, 10k, 50k predicates | Variable-predicate planning |

Workloads should include:

- Bulk and single-triple write throughput.
- Read-only SPARQL mix.
- Mixed HTAP writes plus queries.
- Merge backlog and recovery.
- Export throughput and peak memory.
- Datalog and SHACL workloads in the stable scope.
- HTTP streaming at 1, 10, 100, and 500 concurrent clients, including slow readers and deliberate disconnects.
- JSON writeback queue throughput and lag.

#### Regression gates

- Fail on an unapproved median or p95 regression greater than 10% against the accepted baseline.
- Fail on peak-memory regression greater than 15% for bounded workloads.
- Fail if planning time grows superlinearly beyond the documented predicate-catalog threshold.
- Require a signed baseline-update justification for intentional changes.

Streaming changes additionally require these blocking jobs:

```text
http-stream-format
http-stream-first-byte
http-stream-disconnect-cancel
http-stream-timeout
http-stream-slow-client
http-stream-memory
http-stream-pool-reuse
regress-v0134-streaming
migration-0133-to-0134
benchmark-streaming
```

The memory job streams at least one million rows within a fixed RSS envelope. The disconnect job proves the backend query leaves `pg_stat_activity`; timeout covers local and replica-routed work; pool reuse proves no transaction or setting leaks. Format tests use independent parsers. Every job reports assertion counts and uploads raw evidence; no required job may skip successfully or use `continue-on-error`.

#### Comparative evidence

Run a small, fair comparison against at least two established RDF engines using the same hardware, dataset, query files, warm-up policy, and reporting. Present the result as evidence, not marketing; include cases where pg-ripple is slower.

### Exit criteria

- Current raw benchmark artifacts exist for the candidate.
- Known OFFSET and unbounded UNION behavior is fixed or explicitly bounded.
- Streaming-safe formats do not buffer full results, and slow readers do not create unbounded buffering.
- Disconnects and deadlines cancel PostgreSQL work; subsequent pool users receive clean sessions.
- JSON, CSV, TSV, and N-Triples outputs pass independent parsers and preserve RDF term types.
- Performance claims identify exact hardware and dataset.
- No unapproved regression exceeds the release thresholds.

---

## v0.135.0 — Safe application query API and compatibility freeze

### Objective

Add typed initial SPARQL bindings and opt-in governed prefix resolution, define the contract that v1.x will support, and remove pre-GA architectural decisions that would otherwise become permanent liabilities.

### Required decisions

#### Typed initial bindings

- Add binding overloads for `sparql()`, `sparql_construct()`, `sparql_describe()`, and `sparql_cursor()` using the W3C SPARQL Results JSON term shape.
- Parse and validate URI/literal values, variables, limits, and query scope before execution; reject blank nodes, RDF-star terms, multi-row bindings, and parameterized Update in v1.
- Apply bindings after parsing as a programmatically constructed one-row algebra relation. Never substitute into SPARQL text or concatenate values into generated SQL.
- Dictionary-encode values and pass typed SQL parameters. Cache by query structure and binding names/types, not values.
- Add authenticated `POST /sparql/bindings` and reuse the v0.134 streaming, timeout, cancellation, authorization, rate-limit, replica, and format paths.

#### Registered prefix mode

- Keep `pg_ripple.sparql_prefix_mode = 'strict'` as the default and add explicit `registered` mode.
- Extend the existing `_pg_ripple.prefixes` registry with validated ownership, timestamps, restricted mutations, and transactional generation state; do not add a parallel registry.
- Query-local declarations always win. Add only missing validated prologue declarations in deterministic order; never search and replace QName-looking text.
- Include registry generation in registered-mode plan keys. Committed changes invalidate affected plans; rolled-back changes alter neither generation nor cache behavior.

#### Public schema name

Make an explicit decision on the `pg_ripple` SQL schema and the server-wide `allow_system_table_mods=on` requirement.

**Recommended decision:** move the canonical public API to a non-reserved schema such as `ripple`, keep the extension name `pg_ripple`, and retain `_pg_ripple` only as an internal implementation schema.

Because this is pre-GA, make the breaking namespace change now rather than requiring the server-wide setting forever.

Implementation requirements:

- Add a migration that renames the public schema and repairs qualified references.
- Provide an automated SQL rewrite guide for clients.
- For existing upgraded installations, optionally retain time-limited compatibility wrappers where technically safe.
- Fresh production installs must no longer require `allow_system_table_mods=on` solely for the public schema.

If the project rejects the rename, record an ADR accepting the operational risk and test the setting in every supported deployment mode.

#### Stable API manifest

Generate and check in `api/stable-v1.json` containing:

- SQL functions and signatures.
- Return row schemas.
- Volatility and security properties.
- Stable GUCs, types, contexts, and defaults.
- HTTP methods, paths, request/response schemas, error codes, and access classes.
- Public catalog views.
- Extension/sidecar compatibility rules.

CI compares every candidate against this manifest. A breaking difference requires:

- A `BREAKING:` changelog entry.
- An approved compatibility exception.
- A migration or deprecation path.
- A major version after v1.0.

#### Error and deprecation contracts

- Freeze public PT error codes.
- Define structured HTTP error schemas.
- Add `deprecated_since`, `replacement`, and `removal_version` metadata to deprecated APIs.
- Promise at least one minor-release deprecation window after GA.

#### Stable feature profile

Add a machine-readable production profile:

```sql
select * from pg_ripple.supported_surface('v1');
```

It should list the exact stable features, optional dependencies, unsupported combinations, and evidence artifact.

#### Query-interface test gates

```text
regress-v0135-bindings
regress-v0135-prefix-mode
http-bindings
bindings-plan-cache
prefix-cache-invalidation
bindings-security-negative
bindings-fuzz-smoke
prefix-fuzz-smoke
api-stability-manifest
migration-0134-to-0135
fresh-vs-upgrade-0135
```

Regression coverage includes every supported query form and FILTER, OPTIONAL, UNION, MINUS, VALUES, BIND, subqueries, aggregation/HAVING, paths, named graphs, and federation policy. Security-negative tests prove binding and prefix payloads cannot alter query, SQL, prologue, or catalog semantics. HTTP tests rerun v0.134 backpressure, timeout, disconnect cancellation, replica, and pool-cleanup behavior. Fuzz jobs retain corpora and artifacts; migration checks preserve populated prefix registries and compare fresh versus upgraded schemas and privileges. Required jobs report assertion counts and cannot skip successfully.

### Exit criteria

- Stable SQL, HTTP, GUC, error, and catalog contracts are checked in and CI-enforced.
- Typed bindings pass semantic, injection-negative, plan-reuse, HTTP, and fuzz gates without putting values into SPARQL or SQL text.
- Strict prefix mode remains the default; registered mode is deterministic, permission-controlled, transactional, and cache-safe.
- Fresh and upgraded v0.135 installations are schema-, privilege-, and behavior-equivalent.
- The public schema/`allow_system_table_mods` decision is closed.
- No new public API is accepted after this version except to remediate a release blocker.
- v0.135.0 is declared RC0 and begins the code-freeze period.

---

## v0.136.0 — External audit remediation and hardened candidate

### Objective

Obtain independent assurance on the frozen production surface and remediate all serious findings.

### Audit scope

The external assessment should include:

- Native PostgreSQL extension boundary and pgrx usage.
- Unsafe Rust, C hooks, shared memory, latches, and background workers.
- Dynamic SQL and identifier handling.
- `SECURITY DEFINER`, ownership, grants, RLS, and `search_path`.
- Dictionary consistency, mutation journal, merge, promotion, and tombstones.
- JSON writeback and other asynchronous pipelines.
- HTTP authentication and authorization.
- SSRF, DNS rebinding, redirects, credential handling, and egress control.
- Parser and query resource exhaustion.
- HTTP streaming backpressure, deadlines, cancellation, transaction/session cleanup, output encoding, and connection-pool reuse.
- Typed binding parsing, generated SQL parameters, plan reuse, and injection-negative boundaries.
- Prefix-registry privileges, prologue processing, transactional generation, and cache invalidation.
- Docker, Compose, Helm, and supply-chain defaults.
- Upgrade and migration privilege preservation.

### Required activities

- Publish `audit/engagement-brief.md` before the audit begins.
- Provide the auditor the exact RC0 commit and build artifacts.
- Track each finding with severity, owner, remediation PR, test, and disposition.
- Remediate every Critical and High finding.
- Resolve or formally accept Medium findings with public rationale before GA.
- Rerun affected conformance, migration, crash, and performance gates.
- Run extended fuzzing and sanitizers against parser and pure-Rust components.
- Produce an audit summary that may be published without exposing exploitable details before fixes are available.

### Exit criteria

- Auditor confirms no unresolved Critical or High findings in the assessed scope.
- Every remediation has a regression test.
- The final report or attestation is stored with the release evidence.
- One new independent readiness assessment reports zero Critical and High blockers.

---

## v0.143.0 — Final GA qualification

### Objective

Qualify one exact candidate under sustained production-like load and freeze all evidence for v1.0.0.

### Candidate rules

- Build all artifacts from one signed commit.
- Record binary and image digests before testing.
- Do not rebuild or retag artifacts during qualification.
- Any storage, migration, worker, security, parser, API, or dependency change invalidates the qualification run.

### 72-hour workload

Use a mixed workload that includes:

- Continuous RDF ingestion.
- SPARQL read and update mix.
- Named graphs.
- Merge and predicate promotion.
- Stable-scope SHACL and reasoning.
- HTTP queries and streaming.
- Large streams, slow clients, deliberate disconnects, request deadlines, and replica-routed cancellation.
- Parameterized queries with varying values and concurrent execution.
- Committed and rolled-back prefix changes with concurrent registry reads.
- JSON writeback if proposed as stable.
- Planned PostgreSQL restart and sidecar restart.
- Backup during load and restore verification in a parallel environment.
- Replica replay and promotion exercise.

### Required acceptance thresholds

- Zero data checksum differences from the committed workload ledger.
- Zero PostgreSQL or HTTP process crashes outside deliberate fault events.
- Zero unrecovered worker failures.
- Zero dead-letter rows for valid workload input.
- Query error rate below 0.01%, excluding deliberate invalid requests.
- No monotonic memory leak: final steady-state RSS no more than 15% above the first steady-state hour, with no positive unbounded slope.
- Queue lag and merge backlog remain within documented capacity thresholds.
- p95 and p99 latency remain within the v0.134.0 accepted performance envelope.
- Stream memory remains within the v0.134.0 envelope and cancellation latency remains within its accepted bound.
- No stale binding or registered-prefix plan is observed across committed or rolled-back generation changes.
- No High or Critical vulnerability appears in a final scan.

### Final assessment and documentation

- Run a second independent production-readiness assessment.
- Require two consecutive assessments with zero open Critical or High blockers.
- Freeze installation, upgrade, security, operations, API, and known-limitations docs.
- Complete support and security policies.
- Produce the final release-evidence bundle.

### Exit criteria

- The exact candidate passes the full 72-hour workload.
- Two consecutive readiness assessments contain no open Critical or High blockers.
- Audit, conformance, migration, resilience, performance, and security evidence is complete.
- The release manager signs the GA checklist.

---

## v1.0.0 — General Availability

### Release requirements

v1.0.0 should contain no behavior change from the qualified candidate other than unavoidable version metadata. If a code change is required, issue a new RC and rerun affected qualification gates.

Publish:

- Platform archives and container images by immutable digest.
- Checksums, SBOMs, signatures, and provenance.
- Migration and rollback guide.
- Stable API manifest.
- Feature support matrix.
- Conformance reports.
- Benchmark reports.
- Audit attestation.
- 72-hour soak report.
- Known limitations.
- Security and support policy.

### Post-GA release policy

- v1.0.x contains only backward-compatible bug and security fixes.
- New features wait for v1.1.0.
- Critical security fixes receive an advisory and patch release.
- Every patch runs artifact install, supported upgrade, core conformance, HTTP startup, container-auth, and regression gates.
- Stable APIs cannot be removed or changed before v2.0.0 without a documented exceptional policy.

---

# 7. CI and release-gate redesign

## 7.1 Required pull-request checks

The default branch should require these checks:

```text
format
clippy
unit-pgrx
pg-regress
http-router-construction
http-process-smoke
migration-graph
fresh-install
schema-fingerprint
container-auth-negative-test
security-definer-lint
unsafe-documentation-lint
dynamic-sql-lint
docs-and-api-drift
feature-evidence-validation
```

For relevant path changes, additionally require:

```text
writeback-e2e
upgrade-matrix-fast
conformance-smoke
crash-recovery-fast
benchmark-smoke
helm-security
http-stream-format
http-stream-disconnect-cancel
http-bindings
bindings-security-negative
api-stability-manifest
fresh-vs-upgrade-0135
```

No required check may use `continue-on-error`, `|| true`, or successful skip behavior.

## 7.2 Nightly checks

- Full pinned conformance matrix.
- All fuzz targets with retained corpus and crash artifacts.
- Full historical migration chain.
- Crash-recovery matrix.
- Dependency and image scan.
- Medium-tier benchmarks.
- Streaming memory, slow-client, timeout, cancellation, pool-reuse, and concurrency qualification.
- Retained-corpus binding JSON and prefix-prologue fuzzing.
- Documentation and feature-evidence regeneration check.

## 7.3 Release qualification

Before creating a release:

1. Build artifacts.
2. Install and test those artifacts.
3. Run supported upgrade matrix.
4. Run required conformance suites.
5. Run security and container tests.
6. Run benchmark gates.
7. Generate and validate evidence.
8. Only then create and sign the release.

This reverses the risky pattern of creating a release before all release-specific evidence is known.

---

# 8. Engineering process and governance

## 8.1 Milestones and labels

Create GitHub milestones for every proposed version. Use labels:

```text
severity/critical
severity/high
severity/medium
severity/low
gate/correctness
gate/migration
gate/security
gate/conformance
gate/resilience
gate/performance
gate/api-freeze
area/extension
area/http
area/docker
area/helm
area/docs
release-blocker
```

Every release-blocker issue must have one directly responsible individual, a target milestone, and an explicit acceptance test.

## 8.2 Review rules

Require two approvals for changes touching:

- `src/shmem*`
- PostgreSQL hooks and background workers.
- `src/storage/`
- SQL migrations and extension install SQL.
- `SECURITY DEFINER`, RLS, grants, or ownership.
- HTTP authentication and SSRF policy.
- Docker/Helm security defaults.
- Release workflows.

At least one approval must come from a maintainer who did not author the change. Audit-remediation PRs should also be reviewed by the auditor where possible.

## 8.3 Definition of done for each production item

A production-readiness item is complete only when it has:

- Implementation.
- Positive and negative tests.
- Restart or rollback test where stateful.
- Migration and fresh-install coverage where schema-visible.
- Security analysis.
- Metrics and logs.
- Documentation and runbook update.
- Feature-status update.
- Release-evidence entry.

## 8.4 Architecture decision records

Create ADRs for at least:

- JSON writeback event architecture.
- Public schema and `allow_system_table_mods` decision.
- Stable-core feature scope.
- HTTP authentication model.
- PostgreSQL TLS and service-role model.
- Upgrade support window.
- Required conformance corpora and known-failure policy.
- Release evidence and artifact-promotion model.

---

# 9. GA acceptance checklist

The release manager must be able to mark every item below complete.

## Correctness

- [ ] Zero open Critical correctness issues.
- [ ] Zero open High correctness issues.
- [ ] Stable feature end-to-end matrix passes.
- [ ] Randomized mutation and restart oracle passes.
- [ ] JSON writeback is either fully qualified or explicitly non-stable.

## Installation and upgrades

- [ ] Fresh install passes from every platform artifact.
- [ ] Full historical sequential migration chain passes.
- [ ] Supported recent package-upgrade matrix passes.
- [ ] Fresh and upgraded schema fingerprints match.
- [ ] Failed-upgrade recovery test passes.

## Security

- [ ] Production image rejects passwordless remote database access.
- [ ] Public HTTP bind requires authentication.
- [ ] Route authorization manifest is complete.
- [ ] Least-privilege sidecar role test passes.
- [ ] PostgreSQL TLS verification test passes.
- [ ] SSRF/egress test suite passes.
- [ ] No unresolved Critical or High dependency/image finding.
- [ ] External audit has no unresolved Critical or High finding.

## Conformance and truth

- [ ] Required corpora are pinned and checksum-verified.
- [ ] No required suite skipped.
- [ ] No unexpected conformance failures.
- [ ] README and badges match release artifacts.
- [ ] Stable feature evidence manifest validates.

## Resilience and operations

- [ ] Crash-recovery matrix passes.
- [ ] Logical backup/restore passes.
- [ ] Physical backup/PITR passes.
- [ ] Replica promotion passes.
- [ ] Disk-pressure and cancellation tests pass.
- [ ] Production runbooks are complete.

## Performance

- [ ] Current small, medium, and large benchmark artifacts published.
- [ ] No unapproved >10% latency or throughput regression.
- [ ] No unapproved >15% peak-memory regression.
- [ ] Variable-predicate and export paths are bounded.
- [ ] 72-hour candidate soak passes.

## Compatibility and release

- [ ] Stable API manifest frozen.
- [ ] Public schema decision closed.
- [ ] Extension/HTTP compatibility policy tested.
- [ ] Deprecation and support policy published.
- [ ] SBOM, provenance, checksums, and signatures verified.
- [ ] Two consecutive readiness assessments report zero Critical and High blockers.

---

# 10. First implementation backlog

Open these issues immediately, in this order:

1. **PRD-001: Fix all Axum 0.8 route captures and add router-construction test.**
2. **PRD-002: Add release-image HTTP process smoke test.**
3. **PRD-003: Remove remote PostgreSQL trust authentication from the production image.**
4. **PRD-004: Make async JSON writeback fail closed and annotate v0.128.0.**
5. **PRD-005: Approve ADR for mutation-journal-based writeback events.**
6. **PRD-006: Implement transactional writeback queue schema and retry/dead-letter state.**
7. **PRD-007: Implement typed target SQL and accurate affected-row semantics.**
8. **PRD-008: Build independent migration graph validator.**
9. **PRD-009: Build fresh-versus-upgrade schema fingerprint comparison.**
10. **PRD-010: Pin conformance corpora and fail on missing/zero tests.**
11. **PRD-011: Add typed, fail-closed HTTP configuration and route access registry.**
12. **PRD-012: Create signed release-evidence manifest and make it mandatory.**

No unrelated feature PR should merge ahead of these items.

---

# 11. Success outcome

Following this plan changes pg-ripple’s release model from:

> “Many implemented features plus a large green CI matrix”

into:

> “A deliberately scoped stable core whose correctness, upgrade safety, security, conformance, recovery behavior, performance, and exact artifacts are independently verifiable.”

That is the threshold pg-ripple should meet before calling any release production ready.
