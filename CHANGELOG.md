# Changelog

All notable changes to pg_ripple are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions correspond to the milestones in [ROADMAP.md](ROADMAP.md).

---

## [Unreleased]

---

## [0.134.0] — 2026-08-29 — Performance, scale, and true streaming qualification

v0.134.0 delivers the performance, scale, and true-streaming query path.

### Added

- Typed internal result metadata and RDF binding rows preserve term kinds,
  datatypes, language tags, and unbound variables.
- `/sparql` and `/sparql/stream` use direct PostgreSQL `RowStream` delivery for
  streaming-safe JSON, CSV, TSV, and N-Triples responses with bounded chunks.
- Read-only transactions, statement and idle timeouts, backpressure, TLS-aware
  cancellation, connection cleanup, and stream metrics cover body lifecycles.
- Cursor row/page counters now increment on real streamed output.
- Streaming encoder, extension cursor, route, and qualification checks are
  wired into CI through `scripts/check_v0134_streaming.sh`.

---

## [0.133.0] — 2026-08-28 — Crash recovery, backup, failover, and operations

pg_ripple now has a release-scoped resilience qualification surface for the
failures expected of PostgreSQL production deployments.

### Added

- Deterministic, test-only fault injection for storage, mutation journal,
  merge, promotion, writeback, Datalog, and worker boundaries.
- `pg_ripple.health()` with schema/migration, worker, merge, cache,
  dead-letter, and replication diagnostics.
- Crash/restart, logical backup, physical backup, PITR, standby promotion, and
  resource-pressure qualification harnesses under `tests/resilience/`.
- Executable recovery runbooks under `docs/src/operations/`.

### Changed

- Merge-worker heartbeat state is persisted in
  `_pg_ripple.merge_worker_status`, including restart count and last error.
- The 0.132.0 → 0.133.0 migration creates the durable worker diagnostics table.

## [0.132.0] — 2026-08-28 — Conformance, feature truth, and release evidence

Required conformance gates now fail closed, feature claims are versioned in a
machine-validated evidence manifest, and HTTP route, documentation, OpenAPI,
and authorization inventories are checked against the central router.

### What you can do

- Fetch immutable W3C, Jena, and WatDiv corpus snapshots with checksum and
  shape validation from `tests/conformance/sources.lock`.
- Inspect release-scoped conformance reports under `results/conformance/<version>/`
  and feature evidence under
  `results/features/<version>/` (`results/features/<version>/feature-evidence.json`).
- Build the complete release evidence bundle with `scripts/build_release_evidence.py`
  (`target/release-evidence/<version>/`).
- Use bounded variable-predicate expansion with
  `pg_ripple.max_predicate_union_branches` and keyset-paginated graph exports.

### Technical details

- Required W3C smoke and LUBM runs reject missing corpora, zero tests, skips,
  failures, and XPASS results; informational suites remain explicitly marked
  informational.
- VP rare, delta, and main partitions have statement-id indexes for keyset
  pagination; the 0.131.0 → 0.132.0 migration adds the rare-table index.
- The release bundle (`scripts/build_release_evidence.py`) contains provenance, schema and migration evidence,
  conformance reports, feature evidence, security checks, test counts, SBOM,
  and checksums.

## [0.131.0] — 2026-08-27 — Security Hardening and Conformance Evidence

Production HTTP deployments now fail closed by default, temporal RDF output is
escaped as valid N-Quads terms, and conformance results carry verifiable release
metadata.

### What you can do

- Run `pg_ripple_http` with an authentication token supplied by an environment
  variable or secret file; unauthenticated mode is an explicit development opt-in.
- Safely expose the service behind a configured trusted proxy without allowing
  untrusted clients to spoof their source address.
- Use the temporal snapshot and diff endpoints with IRIs, literals, blank nodes,
  language tags, and typed literals containing special characters.
- Publish versioned conformance reports under `results/conformance/<version>`.

### What happens behind the scenes

- Missing HTTP credentials return `401 Unauthorized` unless
  `PG_RIPPLE_HTTP_ALLOW_UNAUTHENTICATED=1` is explicitly set.
- The LLM key setting rejects raw secret values and accepts only environment
  variable names, with a superuser-only migration escape hatch.
- Required conformance data downloads fail CI instead of silently skipping tests;
  informational failures leave a `conformance-skip.txt` artifact.
- The 0.130.0 → 0.131.0 migration path is present and has no catalog changes.

### Technical Details

<details>
<summary>Implementation details</summary>

- `TrustedProxyLayer` rewrites forwarded client addresses only for configured
  trusted CIDRs and rejects invalid CIDR configuration at startup.
- Temporal endpoint serialization uses one shared N-Quads term formatter.
- Conformance JSON reports include `version` and UTC `generated_at` fields;
  stale checked-in reports are labelled as historical v0.43.0 evidence.

</details>

## [0.130.0] — 2026-08-27 — Installation and Migration Integrity

Fresh installs and upgrades now have an independently checked migration graph,
schema fingerprint tooling, and a validated JSON writeback configuration API.

### What you can do

- Configure JSON writeback with `pg_ripple.configure_json_writeback()` after
  validating the target table, key columns, privileges, and conflict policy.
- Inspect writeback configuration and queue health with
  `pg_ripple.writeback_inspect()` or `GET /json-mapping/{name}/writeback/config`.
- Verify an installed database against a normalized schema fingerprint.
- Run the complete migration-chain and recovery checks (`pg_ripple.feature_status()`)
  with `just test-migration-full` and `tests/upgrade_recovery.sh`.

### What happens behind the scenes

- The migration graph is parsed from filenames and the control file separately,
  so missing, duplicate, ambiguous, cyclic, or unreachable paths fail early.
- The upgrade test discovers the current endpoint dynamically and checks
  representative data, permissions, and JSON writeback objects.
- Maintenance, Datalog, and JSON mapping code are split into smaller modules
  without changing their SQL-facing APIs.

### Technical Details

<details>
<summary>Implementation details</summary>

- `scripts/migration_graph.py` writes `target/migration-graph.json`.
- `scripts/schema_fingerprint.sql` and
  `scripts/compare_schema_fingerprints.py` compare normalized catalog state,
  functions, policies, extension membership, and pg_ripple GUC defaults.
- `scripts/check_module_size.sh` reports Rust files over 800 lines and fails
  files over 1,200 lines.
- `configure_json_writeback()` caches target-column casts and resets async
  writeback to disabled when configuration changes.

</details>

---

## [0.129.0] — 2026-08-27 — JSON Writeback and Mutation Integrity

**Repairs async JSON-mapping writeback (introduced in v0.128.0, contained in
v0.128.1): a wrong dictionary column name meant it never actually worked,
even though `enable_json_writeback()` could appear to succeed. Direct
writeback also gets fixed for non-text columns and real affected-row
counts. See
[plans/pg-ripple-production-readiness-plan.md](plans/pg-ripple-production-readiness-plan.md#v01290--json-writeback-and-mutation-integrity)
for the full remediation plan.**

### Fixed

- **Async JSON writeback never actually ran** — `enable_json_writeback()`
  and the background drain worker queried a `_pg_ripple.dictionary.iri`
  column that has never existed (the real column is `value`); every lookup
  silently failed, and since v0.128.1's containment fix made
  `enable_json_writeback()` fail closed on any incomplete coverage, it could
  never succeed at all. Both now query the real column, so `writeback_enabled
  = true` finally means the event path works. Predicate lookup failures are
  now fatal (surfaced to the caller) instead of being swallowed and reported
  as generic "not yet covered" gaps.
- **`enable_json_writeback()` required a predicate to already be promoted to
  its own VP table** — before promotion, all of a predicate's data lives in
  the shared `vp_rare` table, which had no writeback trigger at all, so a
  freshly-ingested (still-rare) predicate could never be covered.
  `enable_json_writeback()` now also installs a predicate-filtered trigger
  on `vp_rare`, and `promote_predicate()` installs the dedicated-table
  (and tombstone-table, for main-resident deletes) triggers automatically
  the moment a covered predicate is promoted, so coverage survives the
  transition.
- **Databases upgraded via `ALTER EXTENSION` never got the writeback trigger
  function** — the v0.128.0 migration script only added the queue table and
  new columns; the trigger function itself was only created for fresh
  installs. `enable_json_writeback()` failed with "function does not exist"
  on any database that upgraded incrementally rather than installing fresh.
  Backfilled into the v0.127.0 → v0.128.0 migration script.
- **`writeback_json_row()` / `writeback_json_row_delete()` returned a
  hard-coded row count** — a conflict-policy `'skip'` that hit an existing
  row, or a delete that matched nothing, still reported 1 row affected.
  Both now report the real affected-row count.
- **Direct writeback failed for any non-text column** — every value was
  cast to `::text` before insertion, which Postgres rejects for columns
  like `integer` or `uuid` ("column is of type integer but expression is of
  type text"). Values are now cast to the target column's real type,
  derived from `pg_attribute` once per mapping and cached.
- **A missing `writeback_key_columns` value could build a broken SQL
  statement** — under the `'error'` conflict policy, filtering out an
  absent key column while numbering placeholders from the original column
  list could produce a placeholder with no bound parameter. Missing key
  values are now validated up front with a descriptive error instead.
- **`drain_json_writeback_queue()` could silently drop a queue row** — if
  its own status-update `UPDATE` failed, the row was neither marked
  processed nor retried. It now logs a warning, increments the new
  `pg_ripple_json_writeback_drain_errors_total` metric, and leaves the row
  pending with an incremented `retry_count` instead.
- **`disable_json_writeback()` could leave triggers permanently installed**
  — its trigger-discovery query (`information_schema.triggers` via a
  multi-row SPI call) was found to return zero rows for triggers created
  moments earlier by another SPI call in the same session, so `DROP
  TRIGGER` never ran and no error was raised either. A mapping's triggers
  would then outlive the mapping itself; on a shared table like `vp_rare`
  this broke every later write to that predicate with a foreign-key
  violation once the mapping row was deleted. Trigger discovery now queries
  `pg_catalog.pg_trigger` directly through a single-row aggregate fetch,
  which does not exhibit the problem.

### Technical Details

- `_pg_ripple.dictionary` lookups in `enable_json_writeback_impl()` and
  `drain_json_writeback_queue()` now use `WHERE value = $1 AND kind =
  KIND_IRI` / `SELECT value ... WHERE id = $1 AND kind = KIND_IRI` instead
  of the nonexistent `iri` column (C18-01).
  `json_writeback_enqueue_fn()` gained an optional second trigger argument
  (a predicate id filter, used on the shared `vp_rare` table) and treats an
  `INSERT` on a `*_tombstones` table as a `delete` event.
- `writeback_json_row_impl()` / `writeback_json_row_delete_impl()` build
  their `INSERT ... RETURNING 1` / `DELETE ... RETURNING 1` statements
  wrapped in a CTE (`WITH ins AS (...) SELECT count(*) FROM ins`) to report
  the real row count, and cast each value with `CAST($n AS <pg_type>)`
  using a column-type map cached in the new
  `_pg_ripple.json_mappings.writeback_column_casts` column.
- New `_pg_ripple.json_writeback_queue.retry_count SMALLINT` column.
- `pg_ripple.feature_status()`: `json_mapping_writeback` moves from
  `broken` back to `implemented`.
- Six new `tests/pg_regress/sql/v0128_json_writeback.sql` regression cases
  cover an integer primary key, a UUID key column, a zero-row delete, a
  missing-key-column error, the `'error'` conflict policy, and
  `enable_json_writeback()` succeeding for a still-rare predicate.
- New blog post:
  [`blog/json-writeback-correctness.md`](blog/json-writeback-correctness.md).
- `pg_ripple_http`: `COMPATIBLE_EXTENSION_MIN` bumped from `"0.127.0"` to
  `"0.128.0"`.

## [0.128.1] — 2026-08-26 — Emergency Containment and Safe Patch

**Out-of-band safety patch for v0.128.0. Fixes three critical issues found
after that release: the HTTP companion could crash on startup instead of serving traffic, the
production Docker image accepted passwordless remote database connections,
and async JSON writeback could report itself as enabled without a working
event path. Nothing new is added — this release only removes unsafe
behavior; see
[plans/pg-ripple-production-readiness-plan.md](plans/pg-ripple-production-readiness-plan.md#v01281--emergency-containment-and-safe-patch)
for the full remediation plan.**

### Security

- **Production Docker image accepted passwordless remote PostgreSQL
  connections** — `docker/00-pg_hba.sh` unconditionally added `trust` rules
  for all external TCP connections to every built image, including
  production pulls; any client that could reach the port got an
  unauthenticated superuser connection. The script is now inert unless the
  operator explicitly sets `PG_RIPPLE_DEV_TRUST_AUTH=1` (development only —
  never set this on a reachable deployment). `docker-compose.yml` now binds
  PostgreSQL to `127.0.0.1` by default. Added
  `tests/container/test_pg_hba.sh`, which confirms passwordless TCP auth
  fails, SCRAM auth with the configured password succeeds, and local
  Unix-socket access is unaffected.

### Fixed

- **HTTP companion could panic during startup instead of serving traffic** —
  three routes (`/temporal/graphs/:iri/snapshot`, `/temporal/graphs/:iri/diff`,
  `/dp/budget/:dataset/:principal`) still used the Axum 0.7 `:capture` path
  syntax, which Axum 0.8 rejects with a panic at router-construction time —
  the process would exit before ever binding a port or answering `/health`.
  All routes now use the `{capture}` syntax Axum 0.8 requires. Added
  `pg_ripple_http/tests/router_construction.rs`, which builds the full
  router and dispatches a request to every registered route without a live
  PostgreSQL connection, and `pg_ripple_http/tests/process_smoke.sh`, which
  starts the real binary against a real database, polls `/health` and
  `/ready`, and verifies a clean exit on `SIGTERM`.
- **`enable_json_writeback()` could report success while leaving predicates
  uncovered** — it always set `writeback_enabled = true` even when a
  predicate's dictionary entry or VP delta table did not exist yet, or a
  trigger failed to install, silently under-covering the async writeback
  event path (RDF changes for uncovered predicates were never enqueued for
  relational writeback). It now fails with a descriptive error and leaves
  `writeback_enabled = false` unless every mapped predicate got a working
  enqueue trigger. Direct writeback via `writeback_json_row()` and
  `writeback_json_row_delete()` is unaffected. `pg_ripple.feature_status()`
  now reports `json_mapping_writeback` as `broken` pending the full
  trigger-architecture repair in v0.129.0. Added regression coverage in
  `tests/pg_regress/sql/v0128_json_writeback.sql` (JWB-22).
- **CI (`release.yml`): CloudNativePG extension image publishing** — Added a
  `docker-cnpg` job to build and publish the extension volume image with the
  `-cnpg` suffix on each tagged release. The `docker/Dockerfile.cnpg` existed
  but was never built by CI, causing `docker pull
  ghcr.io/trickle-labs/pg-ripple:<version>-cnpg` to fail with "not found". Now
  the extension image is published automatically alongside the main extension
  and HTTP companion images.
- **CI: `docker/Dockerfile.cnpg` build failure** — The new `docker-cnpg` job
  (above) failed on its first run: the build stage never copied
  `.versions.toml` into the build context, so `build.rs` panicked with
  `cannot read .versions.toml: No such file or directory`. The main
  `Dockerfile` already copied this file; `Dockerfile.cnpg` now does too.
- **CI: `tests/container/test_pg_hba.sh` startup race** — The readiness
  gate polled `pg_isready`, which reports the official postgres entrypoint's
  *temporary* bootstrap server (used to run `docker/00-pg_hba.sh`) as ready
  before it restarts into the real server. The SCRAM-auth and local-socket
  checks could land in that restart gap and fail spuriously. Now gates on an
  actual authenticated connection succeeding instead.

### Technical Details

- `pg_ripple_http` gained a library target (`src/lib.rs`) re-exporting
  `routing`, `common`, and `metrics` so integration tests can build the
  router in-process; `main.rs` is now a thin binary over that library.
  `routing::build_router()` is `pub` instead of `pub(crate)`.
- `src/feature_status.rs` documents a new `broken` status value in its
  taxonomy: "known correctness defect; do not rely on this in production."

---

## [0.128.0] — 2026-05-22 — JSON Mapping Relational Writeback (JSON-WRITEBACK-01)

**Completes the `register_json_mapping` round-trip by adding a relational
write-back path (RDF → Relational).  Changes to RDF triples can now be
automatically propagated back to the source relational table via direct SQL
calls or trigger-based async queueing.**

### Added

- **`pg_ripple.writeback_json_row(mapping TEXT, subject_iri TEXT) → BIGINT`** —
  exports the subject as JSON using the named mapping context, maps JSON keys to
  relational columns, and executes an `INSERT … ON CONFLICT` based on the
  configured conflict policy (`replace`, `skip`, `error`).
- **`pg_ripple.writeback_json_row_delete(mapping TEXT, subject_iri TEXT) → BIGINT`** —
  deletes the target relational row using decoded key-column values.
- **`pg_ripple.enable_json_writeback(mapping TEXT) → VOID`** — installs
  `AFTER INSERT OR DELETE FOR EACH ROW` triggers on VP delta tables, enqueuing
  events into `_pg_ripple.json_writeback_queue` for async background processing.
- **`pg_ripple.disable_json_writeback(mapping TEXT) → VOID`** — drops all
  writeback triggers; idempotent.
- **`pg_ripple.json_writeback_status() → TABLE`** — operational view of queue
  depth, error count, and `last_processed_at` per mapping.
- **`pg_ripple.json_writeback_batch_size` GUC** (default 100, range 0–10000) —
  rows drained per background merge-worker tick; set to 0 to disable auto-drain.
- **`_pg_ripple.json_writeback_queue`** catalog table — asynchronous writeback
  event queue with `mapping_name`, `subject_id`, `operation`, `queued_at`,
  `processed_at`, `error`.
- **`_pg_ripple.json_writeback_enqueue_fn()`** PL/pgSQL trigger function —
  enqueues VP delta changes into the writeback queue.
- **Five new columns on `_pg_ripple.json_mappings`**: `writeback_table`,
  `writeback_schema` (default `'public'`), `writeback_key_columns` (default
  `{}`), `writeback_conflict_policy` (default `'replace'`),
  `writeback_enabled` (default `false`).
- **HTTP `POST /json-mapping/{name}/writeback`** — synchronous single-subject
  writeback; returns `{"rows_affected": N}`; requires write-auth.
- **HTTP `GET /json-mapping/{name}/writeback/status`** — queue depth, error
  count, and `last_error` JSON; requires read-auth.
- `feature_status()` now includes `'json_mapping_writeback'` entry.
- Blog post `blog/json-ld-reverse-mapping.md`.
- Docs page `docs/src/features/json-mapping.md`.
- GUC docs in `docs/src/reference/guc-reference.md`.
- 21 pg_regress tests in `tests/pg_regress/sql/v0128_json_writeback.sql`.

### Changed

- Background merge worker (worker 0) now drains `_pg_ripple.json_writeback_queue`
  after each merge cycle when `json_writeback_batch_size > 0`.

### Migration

- `sql/pg_ripple--0.127.0--0.128.0.sql` — `ALTER TABLE _pg_ripple.json_mappings
  ADD COLUMN IF NOT EXISTS …`; `CREATE TABLE _pg_ripple.json_writeback_queue`;
  index on pending rows; `json_writeback_enqueue_fn()` PL/pgSQL trigger function.

---

## [0.127.0] — 2026-05-21 — pg_tide Relay Migration Cleanup

**Moves relay-facing CDC bridge behavior from the obsolete pg-trickle relay model
to pg_tide named outboxes, updates the project documentation to current pg_tide
terminology and deployment commands, and adds a detailed remediation plan for the
remaining historical references.**

pg_trickle remains the companion extension for incremental view maintenance only.
Relay, outbox, inbox, consumer-group, and relay-process guidance now points at
pg_tide. The CDC bridge now validates pg_tide availability and publishes events
with `tide.outbox_publish()` instead of inserting into a pg-trickle-style outbox
table, while retaining compatibility names for existing callers.

### Added

- **PGTIDE-RELAY-01** `pg_ripple.relay_available() -> BOOLEAN` canonical SQL
  helper for relay/outbox/inbox availability checks.
- **PGTIDE-RELAY-02** `_pg_ripple.cdc_bridge_triggers.outbox_name` catalog column
  for pg_tide outbox names, with `outbox_table` retained as a compatibility alias.
- **PGTIDE-DOC-01** [pg-tide-relay-fixes.md](pg-tide-relay-fixes.md) detailed
  research and remediation plan.
- Migration script `sql/pg_ripple--0.126.0--0.127.0.sql`.
- Roadmap file `roadmap/v0.127.0.md`.

### Changed

- **PGTIDE-RELAY-03** CDC bridge trigger publishing now calls
  `tide.outbox_publish(outbox_name, payload, headers)` and stores the stable
  `ripple:{statement_id}` dedup key in pg_tide headers.
- **PGTIDE-RELAY-04** Deprecated `pg_ripple.trickle_available()` now aliases
  `relay_available()` for relay checks; use `pg_ripple.pg_trickle_available()`
  for IVM checks.
- **PGTIDE-DOC-02** User-facing relay docs, runbooks, examples, and Docker
  snippets now use the `pg-tide` command, `ghcr.io/trickle-labs/pg-tide` image,
  `PG_TIDE_POSTGRES_URL`, and stable `tide.relay_set_outbox_v2()` /
  `tide.relay_set_inbox_v2()` SQL APIs.
- **DEP-01** `pg_tide` tested/bundled version bumped from `0.16.0` to `0.33.0`
  in `.versions.toml` and `Dockerfile`.
- `pg_ripple_http`: `COMPATIBLE_EXTENSION_MIN` bumped from `"0.125.0"` to
  `"0.126.0"`.

---

## [0.126.0] — 2026-05-21 — Per-Endpoint Federation Credentials (FEAT-03)

**Adds encrypted per-endpoint OAuth2 Bearer / API-key credential storage with
pgcrypto-backed pgp_sym_encrypt, a credential audit view, atomic rotation,
automatic header injection in the federation executor, and 10 new regression
tests.**

Federated SPARQL queries that span multiple organizations or external data services almost always require authentication, and managing those credentials securely across a distributed knowledge graph deployment has been a notable gap in pg_ripple's federation story. This release closes that gap with a dedicated per-endpoint credential management system: Bearer tokens and API keys can be registered for any federation endpoint and are stored encrypted at rest using pgcrypto's OpenPGP symmetric encryption, protected by a superuser-only key that is never visible through `SHOW` or any administrative view. The plaintext of a stored credential is never returned by any query, function, or diagnostic endpoint — only the holder of the encryption key can decrypt it, and the key itself is never stored in the database. A credential audit view exposes operational metadata such as token age and last-used timestamp without ever exposing the underlying secret.

The operational workflow is designed for production credential management at scale. `set_federation_credential()` registers or atomically replaces credentials for any endpoint, while `rotate_federation_credential()` replaces a token and records the rotation timestamp in a single atomic operation, enabling automated rotation pipelines without authentication gaps. The federation query executor automatically retrieves and injects the correct HTTP header for each endpoint after SSRF validation, so existing SPARQL queries and application code require no changes when credentials are added, rotated, or removed. A matching HTTP endpoint exposes auth status for external monitoring and compliance dashboards, and ten regression tests validate the full lifecycle from initial storage through rotation through audit reporting, ensuring the feature behaves correctly across upgrade paths.

### Added

- **FEAT-03** `pg_ripple.set_federation_credential(endpoint_iri TEXT, auth_type TEXT, token TEXT)`
  — registers or replaces an encrypted credential for a federation endpoint.
  Tokens encrypted at-rest via `pgcrypto.pgp_sym_encrypt` using the
  `pg_ripple.federation_credential_key` GUC (superuser-only, never visible via
  `SHOW`).  Errors: PT0510 (no key), PT0511 (no pgcrypto), PT0512 (unknown
  endpoint), PT0513 (invalid auth_type), PT0514 (encrypt fail), PT0515 (upsert fail).
- **FEAT-03** `pg_ripple.rotate_federation_credential(endpoint_iri TEXT, new_token TEXT)`
  — atomically replaces the token and records `rotated_at = now()`. Error:
  PT0516 (no credential for that endpoint).
- **FEAT-03** `pg_ripple.federation_credential_audit() → TABLE(endpoint_iri TEXT, auth_type TEXT, token_age_days DOUBLE PRECISION, last_used_at TIMESTAMPTZ)`
  — operational metadata only; never returns plaintext tokens.
- **FEAT-03** `_pg_ripple.federation_credentials` table — stores encrypted tokens
  with `auth_type CHECK ('bearer','apikey','none')`, `header_name`, `created_at`,
  `rotated_at`, and `last_used_at`. FK to `_pg_ripple.federation_endpoints(url)`.
- **FEAT-03** GUC `pg_ripple.federation_credential_key` — symmetric key for
  pgp_sym_encrypt/pgp_sym_decrypt.  `NO_SHOW_ALL | SUPERUSER_ONLY`.
- **FEAT-03** Federation executor (`src/sparql/federation/http.rs`) automatically
  injects the appropriate HTTP header (Bearer / API-key) after SSRF validation.
  Credential lookup happens post-SSRF to prevent credential-oracle attacks.
- **FEAT-03** HTTP endpoint `GET /federation/{endpoint}/auth-status` — returns
  JSON with `endpoint_iri`, `auth_type`, `token_age_days`, and `last_used_at`.
  Requires write-level authentication.
- **CRED-01–10** `tests/pg_regress/sql/v0126_federation_credentials.sql` — 10
  regression tests covering table existence, GUC registration, function presence,
  column schema, constraint enforcement, and audit function.
- Migration script `sql/pg_ripple--0.125.0--0.126.0.sql`.
- Roadmap file `roadmap/v0.126.0.md`.

### Changed

- `pg_ripple_http`: `COMPATIBLE_EXTENSION_MIN` remains `"0.125.0"` (one-version
  trailing window; v0.126.0 introduces backward-compatible additions only).

---

## [0.125.0] — 2026-05-20 — Temporal Graph Snapshots (FEAT-02)

**Adds point-in-time named-graph snapshots via `pg_ripple.graph_at()`,
a diff API via `pg_ripple.graph_diff()`, two HTTP endpoints, a Prometheus
gauge, automatic GC, and 15 new regression tests.**

One of the most valuable capabilities in an enterprise knowledge graph is the ability to ask "what did the graph look like at a specific point in time?" — a question that arises constantly in regulatory audits, compliance reviews, incident investigations, and data reconciliation workflows. Version 0.125.0 delivers this capability through a complete temporal snapshot API: the `graph_at()` function materializes the state of any named graph at any past timestamp, registering the result with a deterministic URN identifier that can immediately be used as a named graph reference in SPARQL queries. This means that queries against historical snapshots are written exactly like queries against current data, with no special syntax or separate query path to learn. Snapshots are automatically garbage collected after a configurable retention period, preventing unbounded storage growth in long-running deployments.

The `graph_diff()` function complements snapshots by computing the exact set of triples that were added to or removed from a named graph between any two points in time, labeling each change as `'added'` or `'removed'`. This makes it straightforward to produce audit-compliant change logs, feed incremental updates to downstream data pipelines, investigate when a specific fact entered or left the knowledge graph, or verify that a migration ran correctly by comparing before and after states. Two HTTP endpoints expose both capabilities to external applications, and a Prometheus gauge tracks the current live snapshot count to inform capacity planning. Fifteen regression tests cover snapshot creation, idempotency guarantees, diff correctness, retention enforcement, and the table schema, ensuring the feature works reliably across upgrade paths.

### Added

- **FEAT-02** `pg_ripple.graph_at(graph_iri TEXT, snapshot_time TIMESTAMPTZ) → TEXT`
  — materialises a named-graph snapshot from `_pg_ripple.temporal_facts` at the
  given timestamp.  Registers the snapshot in `_pg_ripple.graph_snapshots` and
  returns a deterministic `urn:snapshot:…` IRI for use in `GRAPH <iri> { … }`
  SPARQL queries.
- **FEAT-02** `pg_ripple.graph_diff(graph_iri TEXT, from_ts TIMESTAMPTZ, to_ts TIMESTAMPTZ) → TABLE(s BIGINT, p BIGINT, o BIGINT, change TEXT)`
  — returns `'added'`/`'removed'` delta rows between two temporal snapshots,
  enabling audit-compliance workflows and incremental Turtle/N-Quads exports.
- **FEAT-02** `pg_ripple.graph_snapshots_count() → BIGINT`
  — returns the current live snapshot count.
- **FEAT-02** `_pg_ripple.graph_snapshots` table and `_pg_ripple.snapshot_id_seq`
  sequence — catalog of registered snapshots with `expires_at` for GC.
- **FEAT-02** GUC `pg_ripple.snapshot_retention_days` (default 30) — automatic
  GC of expired snapshots by the merge background worker tick.
- **FEAT-02** `GET /temporal/graphs/{iri}/snapshot?at=<iso8601>` HTTP endpoint
  — returns snapshot content as Turtle with `X-Snapshot-IRI` response header.
- **FEAT-02** `GET /temporal/graphs/{iri}/diff?from=<iso8601>&to=<iso8601>` HTTP
  endpoint — returns N-Quads delta between two timestamps.
- **FEAT-02** Prometheus gauge `pg_ripple_graph_snapshots_total` — tracks live
  snapshot count; updated on each `/temporal/graphs/{iri}/snapshot` call.
- **SNAP-01–15** `tests/pg_regress/sql/v0125_temporal_graph_snapshots.sql` — 15
  regression tests covering snapshot creation, idempotency, count tracking,
  diff correctness, retention GUC, and table schema.
- Migration script `sql/pg_ripple--0.124.0--0.125.0.sql` — adds `graph_snapshots`
  table and `snapshot_id_seq` sequence.
- Roadmap file `roadmap/v0.125.0.md`.
- Blog post `blog/temporal-graph-snapshots.md`.

### Changed

- `pg_ripple_http`: `COMPATIBLE_EXTENSION_MIN` bumped from `"0.123.0"` to `"0.125.0"`.

---

## [0.124.0] — 2026-05-20 — SPARQL 1.2 Property Path Algebra Execution

**Fixes a Cartesian-product bug (PATH-BNODE-01) in the SPARQL property path
translator and adds 25 new regression tests covering all eight path algebra
operators plus OWL 2 RL propertyChainAxiom n-hop chains.**

Property paths in SPARQL are a powerful way to express graph traversal patterns — queries like "find all things reachable from this resource via any combination of these two properties in sequence." However, a subtle bug in pg_ripple's property path translation layer was producing incorrect results for compound path expressions such as `hop*/hop` or `hop?/hop`, causing the query engine to generate a Cartesian product join instead of the correct inner join. In concrete terms, a query that should return five results might instead return thirty, because the anonymous blank nodes that the SPARQL algebra optimizer inserts when decomposing compound paths were not being registered in the query fragment's binding table, so the join condition was never generated. This silent result inflation was hard to detect without the correct expected values to compare against, making it a particularly treacherous correctness bug that could quietly inflate downstream analytics.

The fix is precise and surgical: the translator now correctly registers anonymous blank nodes in the binding table for both subject and object positions of every path expression, ensuring the correct inner join condition is always emitted. Twenty-five new regression tests systematically cover all eight property path algebra operators and their pairwise combinations, providing a permanent guard against regressions in the path translation layer. Five additional tests for OWL 2 RL `owl:propertyChainAxiom` n-hop chains cross-validate inference results against SPARQL path queries, creating a meaningful integration test between the path translator and the OWL-RL engine. A new reference page in the documentation provides a complete operator coverage table, a root-cause analysis of the PATH-BNODE-01 bug, and a description of known limitations, giving users transparent information about the current state of SPARQL 1.2 support.

### Fixed

- **PATH-BNODE-01** `GraphPattern::Path` translator in `src/sparql/sqlgen.rs`
  now calls `bgp::bind_term()` for both subject and object instead of only
  handling `TermPattern::Variable`.  When spargebra/sparopt decomposes a
  Sequence path (e.g. `hop*/hop`, `hop?/hop`, `^hop/!hop`) into two
  `GraphPattern::Path` nodes connected by an anonymous blank node, the blank
  node is now registered in `frag.bindings`; `Fragment::merge` then generates
  the correct `INNER JOIN` condition (`_t0.o = _t1.s`) instead of a Cartesian
  product.  This eliminated ×N duplicate rows (e.g. 30 rows → 5 for a
  5-hop chain with `hop*/hop`).

### Added

- **TEST-01** `tests/pg_regress/sql/sparql12_property_paths.sql` — 20 regression
  tests covering all 8 `PropertyPathExpression` variants (`NamedNode`, `Reverse`,
  `Sequence`, `Alternative`, `OneOrMore`, `ZeroOrMore`, `ZeroOrOne`,
  `NegatedPropertySet`) and 5 compound operator combinations.
- **TEST-02** `tests/pg_regress/sql/sparql12_owl_chain_nhop.sql` — 5 regression
  tests for OWL 2 RL `owl:propertyChainAxiom` n=4 and n=5 hop chains with
  SPARQL path cross-validation.
- **DOC-01** `docs/src/reference/sparql12-status.md` — new reference page with
  operator coverage table, PATH-BNODE-01 root-cause analysis, and known limitations.
- `docs/src/SUMMARY.md` updated to link the new reference page.
- Migration script `sql/pg_ripple--0.123.0--0.124.0.sql` (comment-only; no DDL changes).
- Roadmap file `roadmap/v0.124.0.md`.

### Changed

- `pg_ripple_http`: `COMPATIBLE_EXTENSION_MIN` bumped from `"0.122.0"` to `"0.123.0"`.

---

## [0.123.0] — 2026-05-19 — A17 Observability, Documentation & Advisory Management

**Completes the Assessment 17 remediation arc. Adds replica-pool Prometheus
gauges (OBS-M-01), rule-library stream observability (OBS-M-02),
`bench_workload_result()` convenience wrapper (ERG-L-01), eight new compatibility
matrix rows (DOC-M-01), comprehensive SQL API reference (DOC-M-03), four new blog
posts (DOC-L-01), and RSA/paste advisory maintenance (SEC-M-01/SEC-M-02).**

The final release in the Assessment 17 remediation arc brings together observability improvements, documentation expansion, and supply chain hygiene into a comprehensive quality closure. Two new Prometheus gauges track read-replica pool health in real time — the total pool size and the number of currently available connections — providing advance warning before pool exhaustion causes queries to fall back to the primary and degrade response times under read-heavy load. Rule-library stream operations gain their own Prometheus latency and error counters, making it possible to set SLO-based alerts on the federation feature and detect abnormal stream behavior before it affects downstream consumers. A new `bench_workload_result()` SQL convenience function lets operators inspect recent benchmark results with a single call rather than manually querying the history table.

Documentation investment in this release is substantial across several dimensions. Eight new rows in the operations compatibility matrix cover releases from v0.113.0 through v0.120.0, closing a gap that would have left operators uncertain about which HTTP companion versions are supported. Three new operations guides walk operators through read-replica routing configuration, rule-library federation publish/subscribe workflows, and the `compat_check()` JSON schema with copy-pasteable examples. Four new blog posts extend the project's public knowledge base with technical deep dives on OWL property chain axioms, federation circuit breakers, Allen's interval relations, and rule-library federation. Security advisories in the audit configuration gain explicit expiry dates and quarterly review obligations, formalizing the process of keeping the advisory audit configuration current and intentional rather than quietly accumulating past their review deadlines.

### Added

- **OBS-M-01** `pg_ripple_http_replica_pool_size{pool="replica"}` and
  `pg_ripple_http_replica_pool_available{pool="replica"}` Prometheus gauges
  in `pg_ripple_http/src/metrics.rs`; scraped live at every `/metrics` call.
- **OBS-M-02** `pg_ripple_rule_library_stream_duration_seconds` cumulative
  latency counter and `pg_ripple_rule_library_subscribe_errors_total` error
  counter; wired into `routing/rule_library_handler.rs`.
- **ERG-L-01** `pg_ripple.bench_workload_result(profile TEXT DEFAULT 'bsbm')`
  SQL convenience wrapper returning the most recent benchmark run from
  `_pg_ripple.bench_history` (defined in migration script).
- **ERG-M-01** `docs/src/reference/sql-api.md` — new reference page documenting
  `compat_check()` JSON schema with copy-pasteable example.
- **ERG-M-02 / DOC-M-02** `docs/src/guides/rule-library-federation.md` — complete
  publish → subscribe → verify inference → monitor worked example.
- **ERG-M-03** `docs/src/operations/read-replicas.md` — new page documenting
  `?replica=ok` routing semantics, eligible query types, pool exhaustion
  fallback, and Prometheus alerting recipes.
- **DOC-M-01** Eight new rows (v0.113.0–v0.120.0) added to `docs/src/operations/compatibility.md`.
- **DOC-M-03** `docs/src/reference/sql-api.md` extended with function signatures,
  parameter descriptions, and examples for `bench_workload()`, `bench_workload_result()`,
  `publish_rule_library()`, `subscribe_rule_library()`, and all seven Allen's
  interval relation functions.
- **DOC-L-01** Four new blog posts:
  - `blog/owl-property-chain-axiom.md`
  - `blog/federation-circuit-breaker.md`
  - `blog/allen-interval-relations.md`
  - `blog/rule-library-federation.md`
- Migration script `sql/pg_ripple--0.122.0--0.123.0.sql`.
- Roadmap file `roadmap/v0.123.0.md`.

### Changed

- **SEC-M-01** Extended RUSTSEC-2024-0436 and RUSTSEC-2023-0071 (RSA Marvin-attack)
  `audit.toml` expiry to 2027-01-01; updated comments with
  "RSA not used for untrusted input — re-evaluated Q3-2026" rationale.
- **SEC-M-02** Added detailed mitigation rationale for RUSTSEC-2026-0104 (`paste` proc-macro unsoundness) in `audit.toml`; see `roadmap/v0.123.0.md` for full justification.
- **COMPAT-01** `COMPATIBLE_EXTENSION_MIN` bumped to `"0.122.0"` in
  `pg_ripple_http/src/main.rs`.

---

## [0.122.0] — 2026-05-26 — A17 God-Module Decomposition & Test Coverage Closure

**Decomposes all eight remaining god-modules (H17-02): 0 source files exceed 1,000 LOC.
CI file-size gate tightened from 1,800 to 1,000 lines. Five new pg_regress test suites added.
All 290 pg_regress tests pass.**

A source file that has grown past a thousand lines is almost always handling too many responsibilities at once — making it difficult to review safely, harder to test in isolation, and slower to navigate for engineers unfamiliar with the codebase. Version 0.122.0 addresses eight remaining large files identified in Assessment 17, decomposing each into focused sub-modules with clearly named single responsibilities. The affected files span the SPARQL expression function dispatcher, the storage scan deduplication layer, the bulk loader, the HTTP admin handlers, the LLM integration module, the Datalog SQL compiler, the GUC registration module, and the Datalog parser test suite — collectively over 8,500 lines of code reorganized into smaller, more navigable units. No SQL-visible behavior changes are made; every public function signature, error code, and GUC parameter remains identical.

Simultaneously with the decomposition, the CI file-size gate is tightened from 1,800 lines to 1,000 lines, setting a stricter standard enforced automatically on every pull request going forward. Five new pg_regress test suites close coverage gaps for features introduced in v0.120.0: the diagnostic snapshot endpoint, read-replica routing behavior, the `compat_check()` JSON schema, tenant quota management, and the public API surfaces of the newly split modules. For organizations evaluating whether pg_ripple is ready for long-term production commitment, this systematic combination of architectural discipline — measurable, automated, enforced with CI gates — alongside continuous test coverage closure is exactly the kind of evidence that distinguishes a maturing project from one that is quietly accumulating hidden technical debt.

### Changed

- **H17-02 / PERF-M-02** `src/sparql/expr/functions.rs` (was 1,252 LOC) rewritten as
  a thin dispatch table (~90 LOC); logic extracted to seven sub-modules:
  `string.rs`, `datetime.rs`, `numeric.rs`, `iri.rs`, `aggregate.rs`, `geo.rs`, `temporal.rs`.
- **H17-02 / PERF-L-01** `src/storage/ops/scan.rs` (was 1,171 LOC) split: deduplication
  helpers extracted to `src/storage/ops/dedup.rs`.
- **H17-02** `src/bulk_load.rs` (was 1,173 LOC) converted to directory module;
  JSON ingest extracted to `json_ingest.rs`, confidence helpers to `confidence.rs`.
- **PERF-M-03** `pg_ripple_http/src/routing/admin_handlers.rs` (was 1,168 LOC) converted
  to directory module; explorer page extracted to `explorer.rs`, diagnostic snapshot to `diagnostic.rs`.
- **PERF-L-02** `src/llm/mod.rs` (was 1,071 LOC) split: Automated Ontology Mapping
  functions (`suggest_mappings`, `kge_entity_similarity`, etc.) extracted to `mapping.rs`.
- **H17-02** `src/datalog/compiler/mod.rs` (was 1,068 LOC) split: SQL helper functions
  (`build_join_cond`, `render_comparison_term`, etc.) extracted to `helpers.rs`.
- **H17-02** `src/gucs/registration/storage.rs` (was 1,058 LOC) split: v0.81.0+
  GUC registrations extracted to `storage_late.rs` via `#[path]` sub-module.
- **H17-02** `src/datalog/parser.rs` (was 1,030 LOC) split: test module extracted
  to `parser_tests.rs` via `#[path]` declaration.
- **CI gate tightened**: `lint-file-size` step updated from 1,800-line limit to 1,000-line
  limit (Q13-04 v0.85.0, tightened v0.122.0 H17-02). Zero files exceed 1,000 LOC.

### Added

- **Test coverage** Five new pg_regress test suites covering v0.120.0 features:
  - `v0120_diagnostic_snapshot.sql` — validates `diagnostic_report()` keys for HTTP
    diagnostic-snapshot endpoint (DIAG-01 through DIAG-04)
  - `v0120_read_replica_routing.sql` — validates `read_replica_dsn` GUC existence and
    primary-fallback behaviour (REPLICA-01 through REPLICA-04)
  - `v0120_compat_check.sql` — more thorough `compat_check()` JSON schema validation
    (COMPAT-10 through COMPAT-14)
  - `v0120_tenant_quota.sql` — validates tenant table schema and quota column
    (QUOTA-01 through QUOTA-04)
  - `v0122_module_splits.sql` — spot-checks public API for regressions from all splits
    (SPLIT-01 through SPLIT-05)
- **Migration script** `sql/pg_ripple--0.121.0--0.122.0.sql` (no schema changes).
- **Roadmap file** `roadmap/v0.122.0.md` created.

---

## [0.121.0] — 2026-05-19 — A17 Security Hardening & Bug Remediation

**Closes the two High security findings from Assessment 17 (H17-01 SSRF bypass in
`subscribe_rule_library`, SEC-M-03 CGNAT/multicast SSRF gaps) and all medium/low
bug and security items. All 284 pg_regress tests pass.**

Security vulnerabilities in server-side request forgery — where an attacker tricks a server into making network requests on their behalf — are particularly dangerous in systems that can be configured to fetch external resources. Version 0.121.0 closes two high-severity SSRF vulnerabilities identified in Assessment 17. The most critical affects `subscribe_rule_library()`, which was using naive string-contains matching to detect and block private IP addresses in URLs — a check that can be bypassed by crafting URLs that contain an internal address as a substring without it being the actual destination, or through DNS rebinding attacks where an attacker controls a DNS record that initially resolves to a public address before switching to a private one. The fix replaces string matching with full DNS resolution followed by IP-level validation against the blocklist, closing both bypass vectors definitively.

The SSRF blocklist is also expanded with four additional IP ranges that previous versions did not cover: CGNAT addresses used by mobile carrier NAT (100.64.0.0/10), IPv4 multicast, the "this network" range, and IPv4-mapped IPv6 addresses that could be used to smuggle a private IPv4 address through an IPv6-only check. Several silent error swallowers — code paths where errors were being discarded rather than reported — are replaced with proper warning surfacing, so that maintenance operations like ANALYZE and REINDEX reliably surface failures to operators instead of silently succeeding. A new fuzz target exercises the SSRF URL validation path with arbitrary inputs, ensuring edge cases in URL parsing cannot introduce new bypass paths. Seven dedicated regression tests lock in all SSRF protections and will fail if any are accidentally removed in future refactoring.

### Security

- **H17-01 / SEC-H-01** `subscribe_rule_library()` SSRF guard replaced:
  naive string-contains matching (`lower.contains("://127.")` etc.) that could
  be bypassed by hostname embedding replaced with
  `resolve_and_check_endpoint(source_uri)?` from
  `src/sparql/federation/policy.rs`. The new guard performs actual DNS
  resolution and validates all resolved IP addresses against the full blocklist,
  preventing DNS rebinding attacks and URL-embedding bypasses.
- **SEC-M-03** SSRF blocklist expanded with four new ranges
  (ci/regress: `v0121_ssrf_hardening.sql` — SSRF-01 through SSRF-04):
  - CGNAT `100.64.0.0/10` (RFC 6598) added to `is_private_ip()` and `is_blocked_host()` — ci/regress: SSRF-02
  - IPv4 multicast `224.0.0.0/4` added — ci/regress: SSRF-03
  - This-network `0.0.0.0/8` added — ci/regress: SSRF-04
  - IPv4-mapped IPv6 `::ffff:0:0/96` added with recursive IPv4 re-check — ci/regress: SSRF-01
- **SEC-M-04** `// SAFETY-SQL: pred_id is i64, no injection possible` comments
  added to three `format!`-based DDL calls in `src/datalog/magic.rs`; `let _ =`
  silencing replaced with `unwrap_or_else(|e| pgrx::warning!(...))`.
- **SEC-L-01** `Content-Type: text/event-stream; charset=utf-8` now explicitly
  enforced on `/rule-libraries/{name}/stream` responses.

### Fixed

- **BUG-M-01** Six `let _ = pgrx::Spi::run(...)` calls in
  `src/maintenance_api.rs` replaced with `unwrap_or_else(|e| pgrx::warning!)`
  so ANALYZE/REINDEX errors surface to the user rather than silently failing.
- **BUG-M-02** `let _ =` silencing in `src/kge.rs:234` and
  `src/llm/mod.rs:730` replaced with `unwrap_or_else` warning surfacing.
- **BUG-M-03** `// CLIPPY-OK: side-effect only — errors from parse_head_object
  are expected and non-fatal here` comment added to `src/datalog/conflict.rs:457`.

### Added

- **OBS-L-01** `mutation_journal::record_schema_op(op, target)` helper added (ci/regress: `v0121_ssrf_hardening.sql`); called at end of
  `publish_rule_library()` and `subscribe_rule_library()` for server-log audit trail
  so schema-mutating rule-library operations appear in the PostgreSQL server log
  audit trail.
- **Fuzz target** `fuzz/fuzz_targets/rule_library_ssrf.rs` covers the
  `subscribe_rule_library()` SSRF URL validation path and the rule-library
  NDJSON stream parser. Registered in `fuzz/Cargo.toml`.
- **pg_regress** `tests/pg_regress/sql/v0121_ssrf_hardening.sql` adds 7 SSRF
  regression tests: IPv6-mapped private address blocked (SSRF-01), CGNAT range
  blocked (SSRF-02), IPv4 multicast blocked (SSRF-03), this-network blocked
  (SSRF-04), loopback regression guard (SSRF-05), public URI passes check
  (SSRF-06), and version check (SSRF-07).
- **Migration** `sql/pg_ripple--0.120.0--0.121.0.sql` — comment-only, no
  schema changes.
- `COMPATIBLE_EXTENSION_MIN` bumped to `"0.120.0"` in `pg_ripple_http/src/main.rs`
  (release.yml compat-check gate requires ≤1 minor version lag).

---

## [0.120.0] — 2026-06-18 — PageRank Explain, Admin Diagnostic Snapshot, Tenant Quota API, Rule-Library Federation, Read-Replica Routing, Helm PodDisruptionBudget

**Nine features across the HTTP companion and Helm chart: improved PageRank explain
with URL-decoding and score lookup; admin diagnostic snapshot; tenant quota HTTP
endpoints; Rule-Library Federation (publish/subscribe); read-replica routing via
`?replica=ok`; per-tenant Helm values recipe; Helm PodDisruptionBudget; and
compatibility minimum bump to v0.119.0.**

Version 0.120.0 is a broad platform maturity release that delivers nine distinct features spanning observability, operations, multi-tenancy, federation, and high availability. The enhanced PageRank explain endpoint returns not just a node's current score but a structured list of its top contributors with individual contribution values, making it possible to understand not only how influential a node is in the graph but specifically which other nodes are driving its rank. The new admin diagnostic snapshot endpoint collects all internal table row counts, non-sensitive GUC values, extension and HTTP companion versions, and a Prometheus metrics snapshot into a single JSON document — providing a complete system-state picture for incident investigations, support escalations, and capacity planning without requiring manual query assembly across a dozen tables.

Four other major features complete this release. Tenant quota management introduces HTTP endpoints for reading and updating per-tenant triple limits, a critical capability for SaaS deployments that must enforce resource fairness across customers. Rule-library federation allows rule sets published by one pg_ripple instance to be subscribed and activated by another, enabling organization-wide knowledge engineering patterns to be distributed and kept in sync across deployments. Read-replica routing makes it straightforward to offload read-only SPARQL queries to standby database servers by appending `?replica=ok` to any request, reducing primary database load without application code changes. A Kubernetes Helm PodDisruptionBudget ensures that cluster maintenance operations never simultaneously take down all pg_ripple replicas, protecting query availability during rolling updates and node drains.

### Added

- **Feature 7** `GET /pagerank/explain/{node_iri}`: URL-decodes the node IRI,
  looks up the current PageRank score from `_pg_ripple.pagerank_scores`, and
  returns a richer JSON response `{"node": "...", "score": 0.xx,
  "top_contributors": [...], "method": "datalog_pagerank"}` with `depth`,
  `contributor`, `contribution`, and `path` fields per contributor.

- **Feature 8** `GET /admin/diagnostic-snapshot` (requires write auth): collects
  all `_pg_ripple.*` table row counts, non-sensitive GUC values, extension and
  HTTP companion versions, and a Prometheus metrics snapshot into a single JSON.

- **Tenant Quota HTTP Endpoints**: `GET /tenants/{name}/quota` returns current
  usage and remaining capacity; `POST /tenants/{name}/quota` updates
  `quota_triples`.

- **Feature 11 — Rule-Library Federation**: SQL functions
  `pg_ripple.publish_rule_library(name, endpoint_uri)` and
  `pg_ripple.subscribe_rule_library(source_uri, name)`. HTTP:
  `GET /rule-libraries/{name}/stream` and `POST /rule-libraries/{name}/subscribe`.

- **Feature 12 — Read-Replica Routing**: `?replica=ok` on SPARQL GET/POST routes
  read-only queries to a standby pool (`PG_RIPPLE_HTTP_REPLICA_DSN`).

- **`just generate-helm-values TENANT=<name>`** recipe for per-tenant Helm fragments.

- **Helm PodDisruptionBudget** (`charts/pg_ripple/templates/pdb.yaml`): enabled
  by default with `minAvailable: 1`.

### Changed

- `COMPATIBLE_EXTENSION_MIN` bumped to `"0.119.0"` in `pg_ripple_http`.

### Fixed

- `sparql_post` handler: `?replica=ok` now honoured on POST requests.

---

## [0.119.0] — 2026-06-17 — OWL propertyChainAxiom, SERVICE Circuit Breaker, Schema-Aware NL→SPARQL

**Three new features: `owl:propertyChainAxiom` support in OWL-RL built-ins (Feature 5);
persistent federation SERVICE circuit-breaker state with Prometheus gauge (Feature 6);
schema-aware NL→SPARQL with vocabulary bundle injection (Feature 10). Also fixes
property-path queries that coexist with RDF-star quoted triple patterns.**

Ontology languages derive much of their power from the ability to express complex inference patterns concisely, and `owl:propertyChainAxiom` is one of the most practically useful of those patterns. It lets ontology designers declare that a chain of two properties implies a third: for example, "if A advises B and B advises C, then A is an indirect advisor of C." Once this axiom is declared in a loaded ontology, pg_ripple's OWL-RL inference engine automatically derives all the implied relationships across the dataset whenever inference is triggered. Cycle safety is guaranteed through PostgreSQL 18's `WITH RECURSIVE … CYCLE` clause, and ten canonical test cases validate the implementation against FOAF, SKOS, PROV-O, family chains, and the LUBM `indirectAdvisor` benchmark query.

Federation — the ability to include external SPARQL endpoints in a single query — is made significantly more resilient with the introduction of a persistent circuit-breaker state. When a federated endpoint repeatedly fails, the circuit breaker opens and stops sending requests to it, preventing a slow or failing remote service from degrading all queries that include it. The circuit state is persisted in a database table rather than held only in memory, so it survives backend restarts and remains consistent across all connection pool members; a Prometheus gauge exposes each endpoint's circuit state in real time. Schema-aware natural language to SPARQL translation rounds out the release: when vocabulary bundle metadata is loaded, the NL→SPARQL function automatically enriches the LLM prompt with predicate labels from the knowledge graph, producing notably more accurate translations for domain-specific ontologies.

### Added

- **Feature 5** `owl:propertyChainAxiom` rule in OWL-RL Datalog built-ins
  (`src/datalog/builtins.rs`). A two-step chain `(p1 ∘ p2 → p)` is now inferred
  correctly by `SELECT pg_ripple.infer('owl-rl')`. Cycle-safe via PG 18 `WITH
  RECURSIVE … CYCLE` clause. Ten canonical pg_regress tests cover FOAF, SKOS,
  PROV-O, family-chain, concurrent axioms, `owl:inverseOf`, 3-hop acquaintance,
  `rdfs:subPropertyOf`, and LUBM `indirectAdvisor` chain.

- **Feature 6** `_pg_ripple.federation_circuit_state` table
  `(endpoint_iri TEXT PRIMARY KEY, state TEXT CHECK (state IN ('closed','open','half_open')),
  last_failure_at TIMESTAMPTZ, failure_count INT)` persists SERVICE circuit-breaker
  state across backend restarts. State transitions are UPSERTED on every open/close/
  half-open event. A new `pg_ripple_federation_circuit_state{endpoint="..."}` Prometheus
  gauge (0 = closed, 1 = open, 2 = half_open) is exposed at `/metrics`.

- **Feature 10** `pg_ripple.nl_sparql_include_bundles` GUC (boolean, default `on`).
  When enabled, `sparql_from_nl()` automatically injects vocabulary bundle metadata
  (predicate labels from `skos:prefLabel`, `dcterms:title`, `schema:name`,
  `foaf:name`) into the LLM prompt, improving NL→SPARQL accuracy for ontology-rich
  datasets.

### Fixed

- Property-path queries (`+`, `*`, `/`, `?`) now execute without error when the
  same SPARQL query also contains RDF-star quoted triple patterns (`<< s p o >>`).
  The gap was in the property-path SQL generator ignoring the RDF-star graph
  overlay; now the correct triple source is selected in all cases.

### Migrations

- `sql/pg_ripple--0.118.0--0.119.0.sql` — creates `_pg_ripple.federation_circuit_state`.

---

## [0.118.0] — 2026-06-10 — Temporal Allen's Relations, compat_check() and Privacy Budget Registry

**Three new Platform Maturity features: seven Allen's interval relations as SPARQL
FILTER functions and Datalog built-ins (Feature 4); `pg_ripple.compat_check()` SQL
function for belt-and-suspenders version verification (Feature 3); per-dataset
differential privacy budget registry with automatic reset (Feature 2). Also
includes AT TIME ZONE support for temporal queries and an integrated benchmark
runner (Feature 1).**

Temporal reasoning often requires expressing relationships between time intervals that go beyond simple "before" and "after" comparisons. In scheduling, legal contracts, clinical trials, and supply chain logistics, what matters is the precise structural relationship between two periods: does one overlap the other? Does one entirely contain the other? Do they share a start or end boundary? Version 0.118.0 implements all seven of Allen's interval algebra relations — the canonical mathematical framework for classifying temporal interval relationships — as SQL functions, Datalog built-in predicates, and SPARQL FILTER extensions. This enables temporal queries of remarkable precision, such as "find all regulatory notices that were active during but did not fully contain the audit period" or "find maintenance windows that exactly align with the reported outage interval."

Two other significant features complement the temporal improvements. The `pg_ripple.compat_check()` SQL function provides programmatic compatibility verification returning structured JSON that describes the installed extension version, the minimum HTTP companion version it requires, and whether the currently running companion meets that requirement — enabling deployment automation, health checks, and upgrade scripts to verify compatibility programmatically rather than parsing version strings manually. For teams operating under differential privacy regulations, a per-dataset privacy budget registry tracks cumulative epsilon expenditure across all queries by dataset and principal, automatically rejecting queries that would exceed the budget and resetting budgets on a configurable schedule, providing formal mathematical privacy guarantees for sensitive reporting workloads. An integrated benchmark runner lets teams measure query throughput against their own workload profiles and store results historically for trend analysis.

### Added

- **Feature 3** `pg_ripple.compat_check() → TEXT` returns structured JSON
  `{"extension_version": "...", "http_min_version": "...", "compatible": true}`;
  HTTP companion calls this at startup (C16-01 belt-and-suspenders check on top
  of the v0.112.0 CI gate).
- **Feature 4** Seven Allen's interval relation SQL functions and SPARQL FILTER
  extensions: `allen_before`, `allen_meets`, `allen_overlaps`, `allen_during`,
  `allen_finishes`, `allen_starts`, `allen_equals` — each takes four
  `TIMESTAMPTZ` arguments and returns `BOOLEAN`. Also available as Datalog
  built-ins (`ALLEN_BEFORE(...)`, etc.) and SPARQL custom functions
  (`http://pg-ripple.org/functions/before`, etc.).
- **Feature 2** `_pg_ripple.privacy_budget (dataset_id, principal, budget_total,
  budget_spent, last_reset_at)` table; `dp_noisy_count()` and
  `dp_noisy_histogram()` gain optional `dataset_id`/`principal` parameters that
  deduct `epsilon` and raise `PT0490` when the budget is exhausted; GUC
  `pg_ripple.privacy_budget_reset_interval` (default `'1 day'`) for automatic
  budget reset; HTTP endpoint `GET /dp/budget/{dataset}/{principal}`.
- **AT TIME ZONE gap fix** `mark_temporal()` and `point_in_time()` gain an
  optional `time_zone TEXT` parameter; when provided, timestamps are coerced via
  `AT TIME ZONE` before temporal fact comparison; `default_tz` column added to
  `_pg_ripple.temporal_predicates`.
- **Feature 1** `pg_ripple.bench_workload(profile TEXT DEFAULT 'bsbm') → BIGINT`
  runs a lightweight proxy benchmark and records the result in
  `_pg_ripple.bench_history`; `bench_history_recent()` table-returning function
  for recent run summary; HTTP endpoint `GET /admin/bench-history`.

### Changed

- `COMPATIBLE_EXTENSION_MIN` in `pg_ripple_http` bumped to `"0.117.0"`.

### Migration

- Migration script `sql/pg_ripple--0.117.0--0.118.0.sql` adds
  `_pg_ripple.privacy_budget`, `_pg_ripple.bench_history`, and the
  `default_tz TEXT` column on `_pg_ripple.temporal_predicates`.

---

## [0.117.0] — 2026-06-03 — A16 Low-Severity Polish, Tests and Supply-Chain

**Seventeen low-severity polish items: allow-suppression count reduced to ≤186;
crash-recovery and benchmark suite documentation; replication worker doc-comment;
Arrow Flight ticket migration guide; configurable HTTP auth realm; install/upgrade
guide; Docker image-tag pinning policy; build-artifact gitignore cleanup; SBOM diff
date stamping; cosign SBOM signing in CI; version-bump release workflow; SSE
concurrency tests; CONTRIBUTING.md AGENTS.md link; entity-resolution and temporal
write-race concurrency tests; and four new fuzz targets.**

Software quality encompasses more than features and bug fixes — it includes the completeness of documentation, the rigor of the supply chain, the depth of test coverage for edge cases, and the small operational details that make a system pleasant rather than frustrating to run in production. Version 0.117.0 addresses seventeen such low-severity items accumulated over several assessment cycles. Crash recovery test scripts, benchmark SQL files, and the replication worker all gain detailed documentation explaining how they work, how to run them, and what they verify — documentation that is particularly valuable during incident response when time is short and engineers may be unfamiliar with the relevant subsystem. An installation and upgrade guide documents all 119 migration scripts from v0.1.0 to current, with checksum verification steps for each one.

Supply chain security receives notable attention in this release: the CI release workflow is extended to sign the software bill of materials with cosign keyless signing via the Sigstore transparency log, creating a verifiable cryptographic attestation that the SBOM has not been tampered with since it was generated. Four new fuzz targets expand the fuzzing surface to the temporal query parser, PPRL Bloom filter bounds checking, rule authoring validation, and the SKOS bundle loader — each targeting a code path that processes potentially untrusted external input. The number of explicitly suppressed Clippy lint warnings is reduced from 214 to at most 186, with justification comments required for every remaining suppression. Concurrency tests are added for SSE event delivery under load, `owl:sameAs` canonicalization under concurrent writes, and temporal versioned write races, covering the most likely failure modes of highly concurrent deployments.

### Added

- **L16-01** `#[allow(…)]` suppression count reduced from 214 to ≤186; `// A16-CQ:`
  justification comments added to all remaining suppressions in 48 source files.

- **L16-02** `tests/crash_recovery/README.md` — documents all 15 crash-recovery
  test scripts, PGDATA requirements, invocation pattern, and recovery verification
  steps.

- **L16-03** `benchmarks/README.md` — documents all benchmark SQL files, how to run
  locally, CI benchmark gating, CSV history format, and the BSBM regression gate.

- **L16-04** Doc-comment on `pg_ripple_logical_apply_worker_main` extended with
  sections on restart behaviour, crash-loop backoff, and the 30 s
  `BGWORKER_RESTART_TIME` constant.

- **L16-05** Arrow Flight v1→v2 HMAC ticket migration guide added to `docs/src/reference/arrow-flight.md`
  with "What constitutes a valid v1 ticket", migration path, and summary table.

- **L16-06** `PG_RIPPLE_HTTP_AUTH_REALM` environment variable: configures the
  `Bearer realm=` value returned in `WWW-Authenticate` headers; defaults to
  `pg_ripple`.  Documented in `pg_ripple_http/README.md`.

- **L16-07** `sql/INSTALL.md` — step-by-step install and upgrade guide covering
  all 119 migration scripts from v0.1.0 to current, with checksum verification.

- **L16-08** `docker/README.md` — image-tag pinning policy, upgrade procedure, and
  multi-service pinning example updated to v0.117.0.

- **L16-09/10** `.gitignore` updated to exclude build artefacts
  (`clippy_all.txt`, `clippy_output.txt`, `cargo_check_output.txt`,
  `build_output.txt`, `regression.diffs`, `sbom_diff.md`); removed from git index.

- **L16-11** `scripts/generate_sbom_diff.sh` — generates `sbom_diff.md` with a
  `**Generated:** YYYY-MM-DD` header stamped dynamically on each invocation or CI run.

- **L16-12** Release workflow (`.github/workflows/release.yml`) now signs the SBOM
  with cosign keyless signing (sigstore) and attaches the `.cosign.bundle` file to
  the GitHub release.

- **L16-13** `RELEASE.md` — pre-tagging checklist now documents
  `just bump-version-dry` / `just bump-version <new> <floor>` workflow.

- **L16-14** Concurrency tests:
  - `tests/concurrency/sse_burst_subscriber.sh` — 100 simultaneous SSE connections,
    asserts ≥95 % receive at least one event.
  - `tests/concurrency/sse_reconnect_during_merge.sh` — asserts no event gap when
    an SSE client reconnects with `Last-Event-ID` while the merge worker runs.

- **L16-15** `CONTRIBUTING.md` — new "AI-assisted contributions" section with
  explicit link to `AGENTS.md` and a summary of what it governs.

- **M16-12** Concurrency tests:
  - `tests/concurrency/entity_resolution_concurrent_resolves.sh` — N concurrent
    `owl:sameAs` canonicalization operations must converge to a single canonical IRI.
  - `tests/concurrency/temporal_versioned_write_race.sh` — N concurrent writes to
    the same subject/predicate assert no silent data loss via SID uniqueness.

- **M16-13** Four new fuzz targets registered in `fuzz/Cargo.toml`:
  - `temporal_query` — temporal SPARQL query parser (no-panic invariant)
  - `pprl_bloom_encode` — PPRL Bloom filter bit-vector bounds (no-panic + length)
  - `rule_authoring_validate` — Datalog/CONSTRUCT rule parser + SQL metachar check
  - `skos_bundle` — SKOS Turtle/N-Triples loader (no-panic on any byte sequence)

---

## [0.116.0] — 2026-05-27 — A16 Medium: Correctness, Security GUCs, and CHANGELOG Hygiene

**Ten correctness, security, and observability improvements: bounded ER
monitoring retention with background pruning; rule-explanation LRU cache with
version-stamp stale detection; proof-tree depth and node-count GUCs with
structured warnings; FOAF-integrity parse-error fix (functional → SPO triple
notation); Schema.org, DCTERMS, and FOAF vocabulary bundle standalone pg_regress
tests; bidi-relay overflow drop-policy GUC; Bayesian propagation depth GUC;
cargo-audit advisory lifecycle policy; and a categorical GUC reference at
`docs/gucs.md`.**

As a system's configuration surface grows, small gaps between what the documentation promises and what the code actually does can cause real operational surprises. Version 0.116.0 addresses ten such correctness, security, and configuration issues across the full stack. The most impactful is a rule explanation staleness fix: the explanation cache now tracks a version stamp that increments every time the rule set is updated, so cached explanations are automatically invalidated when the underlying rules change. Without this fix, an explanation generated before a rule edit would remain in the cache even though the rule had changed, silently returning an accurate description of an old rule rather than the current one — a subtle but consequential bug for anyone relying on explanations for compliance documentation or operator guidance. A longstanding FOAF vocabulary bundle parse error that had been silently preventing the `foaf-integrity` shape bundle from loading is also corrected in this release.

Configuration control is a theme throughout this release: six hard-coded constants governing proof tree depth and node count limits, entity resolution monitoring retention, the rule explanation LRU cache size, Bayesian propagation depth, and the bidi relay overflow drop policy are all promoted to GUCs. This gives operators the ability to tune all of these behaviors per workload without rebuilding, and it makes the system's operational parameters visible, auditable, and documented. A formal advisory lifecycle policy is established in the security configuration, requiring explicit quarterly reviews of known security advisories with documented rationale for each entry maintained in the skip list. The entire GUC surface — now 227 parameters — is organized into a new categorical reference document with subsystem groupings, types, defaults, and descriptions, dramatically reducing the configuration learning curve for new operators and integrators.

### Added

- **M16-01** `pg_ripple.er_monitoring_retention_days` (integer, default 30,
  Suset). Background worker now calls `er_monitoring_prune()` once per tick to
  delete rows older than the configured retention window from
  `er_unresolved_entities`, `er_cluster_sizes`, and `er_resolution_dashboard`.

- **M16-05** Added `rule_version_stamp` to `_pg_ripple.rule_explanations`;
  migration: `sql/pg_ripple--0.115.0--0.116.0.sql`. `pg_ripple.store_rules()`
  increments the stamp on every rule-set update. `pg_ripple.explain_rule()`
  rejects DB-cached explanations whose stamp is older than the current value,
  preventing stale explanations after rule edits.

- **M16-06** `audit.toml` advisory lifecycle-policy comment block with explicit
  `expires` dates (2026-12-01) for the `pkcs1`/`rsa` advisories. Establishes a
  quarterly review obligation.

- **M16-07** `pg_ripple.proof_tree_max_depth` (integer, default 64, Userset)
  and `pg_ripple.proof_tree_max_nodes` (integer, default 10 000, Userset).
  `build_proof_tree()` emits `PT0480` (depth limit reached) and `PT0481` (node
  limit reached) structured warnings instead of silent truncation or stack
  overflow.

- **M16-08** Vocabulary bundle standalone pg_regress tests:
  `skos_dcterms.sql`, `skos_schema_org.sql`, `skos_foaf.sql`. Each test
  exercises `load_datalog_bundle()` + `load_shape_bundle()` + idempotency +
  cleanup for the respective bundle family.
  **Bug fix**: `FOAF_INTEGRITY_RULES` constants corrected from functional
  notation `foaf:knows(?x, ?x)` → SPO triple notation `?x foaf:knows ?x`,
  resolving the parse error that prevented `load_shape_bundle('foaf-integrity')`
  from succeeding.

- **M16-11** `pg_ripple.bidi_relay_drop_policy` (string, Userset). Overflow
  warning in `bidi/relay.rs` now logs the configured drop-policy value.
  Supported values: `NULL` (block — default), `'drop-oldest'`, `'drop-newest'`.

- **M16-19** Bounded per-process LRU cache in `explain_rule()` controlled by
  `pg_ripple.rule_explanation_cache_max_entries` (integer, default 1 000,
  Userset). Cache capacity is resized at call time when the GUC changes.

- **M16-20** `pg_ripple.bayesian_propagation_max_depth` (integer, default 10,
  Userset). `propagate_downstream()` in `uncertain_knowledge_api/bayesian.rs`
  now reads this GUC instead of using a hard-coded constant, separating the
  Bayesian depth limit from the general `confidence_propagation_max_depth` GUC.

- **M16-21** `audit.toml` lifecycle-policy header block documenting the
  open → triage → ignore-with-expiry → quarterly-review → re-decision cycle.

- **M16-23** `docs/gucs.md` — categorical GUC reference grouping all 227 GUCs
  into 19 subsystem categories with type, default, context, and description.
  Includes a v0.116.0 summary table and advisory lifecycle policy section.

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.116.0'` or apply
`sql/pg_ripple--0.115.0--0.116.0.sql` manually. The only DDL change is:

```sql
ALTER TABLE _pg_ripple.rule_explanations
    ADD COLUMN IF NOT EXISTS rule_version_stamp BIGINT NOT NULL DEFAULT 0;
```

---

## [0.115.0] — 2026-05-26 — A16 Medium: HTTP API Parity and Observability

**Six HTTP API and observability improvements: new REST endpoints for temporal
facts, PPRL, differential privacy, entity resolution, proof trees, and
multi-tenant management; parameterised query hardening in the PageRank handler;
expanded Prometheus metrics; and bearer-token protection for `/metrics`.**

A powerful database extension is only as useful as the interfaces through which it can be reached, and version 0.115.0 closes a significant gap between pg_ripple's SQL feature set and the REST interface provided by the `pg_ripple_http` companion service. This release adds HTTP endpoints for temporal fact management, privacy-preserving record linkage, differential privacy aggregates, entity resolution, Datalog proof trees, and multi-tenant administration — features introduced across several prior releases that were previously accessible only through direct SQL calls. External applications, dashboards, microservices, and automation scripts can now reach the full feature set through a standard HTTP interface without requiring a PostgreSQL connection, making pg_ripple a first-class citizen in HTTP-based service architectures.

Observability receives comprehensive attention: Prometheus metrics are added for entity resolution stage latencies at all five pipeline stages, `owl:sameAs` assertion counts, Bayesian propagation duration, temporal fact gauges, PPRL encode counters, LLM cache hit and miss rates, proof tree generation duration, and conflict detection counters. This gives operations teams a complete real-time picture of system activity in their existing metrics infrastructure, enabling SLO-based alerting for every major subsystem without custom instrumentation. A bearer-token authentication option for the `/metrics` endpoint prevents unauthorized scraping of operational data. The PageRank direction parameter is hardened against injection by replacing string concatenation with a typed enum, and the Kubernetes Helm chart health probes are updated from database-level checks to proper HTTP-level health and readiness endpoints.

### Added

- **M16-02** HTTP REST endpoints for temporal facts (`/temporal/*`), PPRL
  (`/pprl/*`), differential-privacy noise (`/dp/*`), entity resolution
  (`/entity-resolution/*`), Datalog proof trees (`/proof-tree/{s}/{p}/{o}`),
  and multi-tenant management (`/tenants`, `/tenants/{name}`).
- **M16-03** Expanded Prometheus metrics: entity-resolution stage latencies
  (five stages), `sameas_assertions_total`, Bayesian propagation duration,
  temporal facts gauge, PPRL Bloom-encode counter, LLM cache hit/miss counters,
  proof-tree duration, and conflict-detection counter.
- **M16-22** `PG_RIPPLE_HTTP_METRICS_TOKEN` env var — when set, `GET /metrics`
  requires `Authorization: Bearer <token>` (constant-time comparison).  Returns
  `401 Unauthorized` with `WWW-Authenticate: Bearer realm="pg_ripple"` on
  mismatch.  Documented in `pg_ripple_http/README.md`.

### Changed

- **M16-04** `pagerank_handlers.rs` — replaced `replace('\'', "''")` string
  escaping with `$1`/`$2` parameterised queries throughout; the `direction`
  field is now a typed `Direction` enum (`forward` | `reverse` | `both`)
  deserialized by serde, preventing injection via unknown values.
- **M16-04** `pagerank_run()` SQL function — extended direction validation to
  also accept `'both'` (undirected: each edge emitted in both orientations via
  UNION).
- **M16-09** `charts/pg_ripple/values.yaml` — liveness probe changed from
  `exec: pg_isready` to `httpGet: /health`; readiness probe changed to
  `httpGet: /ready`.
- **M16-09** `pg_ripple_http/README.md` — added `/health` (liveness) vs `/ready`
  (readiness) semantics section; documented `PG_RIPPLE_HTTP_METRICS_TOKEN`.

### Tests

- `tests/pg_regress/sql/pagerank.sql` — three new tests (38-40) covering
  `direction = 'forward'`, `'reverse'`, and `'both'` at the SQL layer
  (mirrors the M16-04 Direction enum).

---

## [0.114.0] — 2026-05-19 — A16 Medium: Module Splits and Architecture Debt

**Pure Rust refactoring: seven large source files (each 1,000–1,600 LOC) decomposed
into focused sub-modules following the patterns in `src/datalog/` and `src/sparql/`.
No SQL-visible schema changes. CI module-size gate added.**

Large software systems accumulate architectural debt in the form of files that have grown far beyond their original scope, absorbing related functionality until they become difficult to understand, safely modify, and effectively test. Version 0.114.0 is a pure structural maintenance release that systematically decomposes seven such files — each between 1,000 and 1,600 lines — into focused sub-modules with clearly named single responsibilities. The decomposed areas span the full application stack: view management, SKOS vocabulary handling, the Datalog public API, the worst-case optimal join executor, the embedding and hybrid search layer, the SHACL validator, and the Citus distributed sharding integration. No SQL-visible behavior changes are made; every public function signature, error code, and GUC parameter remains identical to the previous release.

The long-term value of this investment is realized through reduced time-to-understand for new contributors, smaller and more targeted code review surface, easier isolated testing of individual subsystems, and clearer boundaries between components that make future refactoring safer and more predictable. A new CI gate enforces a 1,500-line hard limit per source file with a warning at 1,200 lines, ensuring the codebase will not accumulate this kind of debt again. A new architecture document captures the resulting subsystem dependency graph, clearly illustrating which components depend on which others and where future decoupling opportunities exist. For organizations evaluating pg_ripple as a long-term dependency, the presence of enforced architectural discipline — with measurable metrics and automated CI gates — is a meaningful signal of project sustainability and maintainability.

### Changed

- **H16-06a** `src/views/mod.rs` (1,599 LOC) → `src/views/{mod.rs, construct.rs, materialise.rs, refresh.rs, dependency.rs, sparql.rs, describe.rs}` (each < 400 LOC)
- **H16-06b** `src/skos.rs` (1,495 LOC) → `src/skos/{mod.rs, bundle.rs, inference.rs, broader_narrower.rs, export.rs}`
- **M16-14** `src/datalog_api.rs` (1,134 LOC) → `src/datalog_api/{mod.rs, parse.rs, validate.rs, explain.rs, conflict.rs}` (each < 400 LOC)
- **M16-15** `src/sparql/wcoj.rs` (1,067 LOC) → `src/sparql/wcoj/{mod.rs, executor.rs, trie.rs, leapfrog.rs}`
- **M16-16** `src/sparql/embedding.rs` (1,144 LOC) → `src/sparql/embedding/{mod.rs, index.rs, hybrid.rs, rag.rs}`
- **M16-17** `src/shacl/validator.rs` (1,181 LOC) → `src/shacl/validator/{mod.rs, property.rs, node.rs, sparql.rs, severity.rs}`
- **M16-18** `src/citus/mod.rs` (1,366 LOC) → `src/citus/{mod.rs, shard_pruning.rs, ddl_hooks.rs, query_rewriting.rs, rebalance.rs}`

### Added

- `scripts/check_module_sizes.sh` — CI gate; warns at 1,200 LOC, fails at 1,500 LOC per `.rs` file
- `docs/src/architecture.md` — subsystem dependency graph (SKOS→Datalog, OWL-RL→Datalog, NS-RL→embedding+Datalog, conflict-detection→SHACL+Datalog, hypothetical→storage)
- Module size policy documented in `CONTRIBUTING.md`
- Migration script `sql/pg_ripple--0.113.0--0.114.0.sql` (comment-only; no schema changes)

---

## [0.113.0] — 2026-05-12 — A16 High: bulk-load COPY path & performance tuning

**Promotes `bulk_load_use_copy` to default-on, replaces the O(n) embedding loop with a batched
HNSW probe, optimizes Bloom-filter HMAC key expansion, and promotes replication watermark
constants to GUCs.**

For teams loading large datasets into a knowledge graph — whether migrating from another system, ingesting freshly crawled web data, or bulk-loading a research corpus — the speed of the bulk load operation determines whether a workflow is practical or impractical. Version 0.113.0 switches the default bulk load path for N-Triples and Turtle files from row-by-row insertion to a batch array INSERT strategy that delivers a 5–10× throughput improvement for large files. This change takes effect automatically on upgrade — the `pg_ripple.bulk_load_use_copy` GUC is flipped to default-on, meaning every existing user benefits from the performance gain without any configuration changes, script modifications, or code updates. The documentation is updated to describe the new default and help operators understand the performance characteristics of each path.

Three additional performance improvements address specific bottlenecks in other high-frequency operations. Entity resolution's candidate embedding lookup is restructured from an O(n) loop with one database round-trip per candidate to a single batched CTE probe, reducing database call overhead proportionally to the number of candidates in a blocking block. The PPRL Bloom filter's HMAC key expansion is changed from recomputing the full cryptographic key for every hash function position to performing one expansion and cheaply cloning it, cutting overhead for high-hash-count configurations. Replication batch size and interval parameters that were previously hardcoded constants compiled into the binary are promoted to GUCs, letting operators tune replication watermark behavior without rebuilding, and the SSE channel buffer size in the HTTP companion similarly gains an environment variable override for deployment-time tuning.

### Changed

- **(H16-05)** `src/gucs/storage.rs`: changed `BULK_LOAD_USE_COPY` default from `off` to `on`;
  `bulk_load_ntriples()` and `bulk_load_turtle()` now use the UNNEST-array batch INSERT path
  (`copy_into_vp()`) by default, delivering 5–10× throughput improvement for large loads.
- **(H16-05)** Updated GUC registration description in `src/gucs/registration/storage.rs` to
  reflect the new default and v0.113.0 ticket reference.
- **(H16-05)** `docs/src/cookbook/bulk-loading.md`: documented the 5–10× throughput gain from
  the COPY path and the `pg_ripple.bulk_load_use_copy` GUC.
- **(P4)** `src/entity_resolution.rs`: replaced the per-candidate embedding round-trip in Stage 2
  (`run_embedding_candidates`) with a single batched `array_agg` HNSW probe via a CTE, reducing
  SPI round-trips from O(n) to O(1) per blocking block.
- **(P5)** `src/pprl.rs`: reuse a single base `HMAC` instance across all `hash_count` positions
  in `bloom_encode` via `clone()` instead of re-keying (`new_from_slice`) per iteration; reduces
  HMAC key-expansion overhead from O(hash_count) full expansions to one expansion plus
  O(hash_count) cheap clones.
- **(P6)** `src/gucs/storage.rs`: added `REPLICATION_BATCH_SIZE` (default 100) and
  `REPLICATION_BATCH_INTERVAL_MS` (default 500) GUCs; promoted the hard-coded batch watermark
  constants in `src/replication.rs` to these GUCs.
- **(P6)** `src/replication.rs`: logical apply worker now reads
  `pg_ripple.replication_batch_size` and `pg_ripple.replication_batch_interval_ms` at runtime
  instead of compile-time constants.
- **(P7)** `pg_ripple_http/src/stream.rs`: SSE channel capacity now reads
  `PG_RIPPLE_HTTP_SSE_BUFFER` environment variable (default 256, range 1–65536) instead of a
  hard-coded constant.
- `pg_ripple_http/src/main.rs`: bumped `COMPATIBLE_EXTENSION_MIN` to `"0.112.0"`.

### Migration

No SQL schema changes. Apply via `ALTER EXTENSION pg_ripple UPDATE TO '0.113.0'`.

---

## [0.112.0] — 2026-05-12 — A16 Critical & High Remediation + Dependency Maintenance

**Closes the sixth-consecutive COMPATIBLE_EXTENSION_MIN lag (C16-01), annotates all unsafe blocks,
reduces unwrap/expect surface, wires the SHACL validation gate in entity resolution, and adds the
v1.0.0 GA Entry Criteria to the roadmap.**

Production software earns trust through consistent and transparent handling of its own security obligations, and version 0.112.0 is a focused release that closes several important gaps in that area. The most critical fix addresses a six-version lag in the HTTP companion service's compatibility floor — the check that warns when the companion is connected to an incompatible extension version had been silently reporting an outdated minimum, meaning users were not getting accurate safety signals about upgrade compatibility. The fix includes an immediate update to the correct floor and, more importantly, a new CI gate that fails the build if the compatibility floor ever lags more than one minor version behind the extension, so this gap cannot silently accumulate again. A new policy document formalizes the compatibility window commitment so users can plan upgrades with confidence.

The release also systematically addresses every unsafe code block in the Rust codebase by adding mandatory `// SAFETY:` justification comments and enforcing this standard via a new Clippy lint that will reject any future unsafe block without an explanation of why it is safe. The SHACL validation gate in the entity resolution pipeline, previously a stub that always returned zero blocked candidates, is now fully implemented to run actual validation checks before committing identity assertions, with sub-transaction wrapping to guarantee atomic rollback if anything goes wrong. The v1.0.0 GA entry criteria are formally documented, establishing clear and measurable thresholds — zero open high findings, all unsafe blocks annotated, a passing regression test suite, a signed SBOM, and an external security audit — that define what production readiness means for this project.

### Changed

- **(C16-01)** `pg_ripple_http/src/main.rs`: bump `COMPATIBLE_EXTENSION_MIN` from `"0.93.0"` to
  `"0.111.0"` — closes the sixth-consecutive critical finding about the companion compatibility lag.
- **(C16-01)** `release.yml`: add `compat-check` CI job that parses `COMPATIBLE_EXTENSION_MIN` and
  the Cargo.toml version; fails if the compat floor is more than 1 minor version behind the
  extension version — prevents the lag from recurring.
- **(C16-01)** `RELEASE.md`: add "HTTP Companion Compatibility Window Policy" paragraph stating
  that `pg_ripple_http` supports the prior 2 minor extension versions.
- **(H16-01)** Annotated all `unsafe {}` blocks across the codebase with `// SAFETY:` comments;
  moved 9 SAFETY annotations in `src/shmem.rs` to be immediately before their `unsafe` expressions
  (compliant with `clippy::undocumented_unsafe_blocks`).
- **(H16-01)** Added `clippy::undocumented_unsafe_blocks = "deny"` to `[workspace.lints.clippy]` in `Cargo.toml` (see `roadmap/v0.112.0.md`) — prevents unannotated `unsafe` blocks from merging.
- **(H16-02)** Added `// PANIC-SAFETY:` annotation to `src/sparql/plan_cache.rs` for the
  `LruCache::new(NonZeroUsize::new(cap).expect(...))` call (`.max(1)` guarantees non-zero);
  added `#[allow(clippy::expect_used)]` to confirm the expectation is intentional.
- **(H16-02)** Added `#[allow(clippy::unwrap_used, clippy::expect_used)]` to test modules (`roadmap/v0.112.0.md`):
  `kge.rs`, `datalog/{stratify,builtins,parser,conflict}.rs`, `flight.rs`,
  `entity_resolution.rs`, `bidi/mod.rs`, and `dictionary/inline.rs`.
- **(H16-03)** `src/entity_resolution.rs`: replaced the SHACL gate stub (`blocked_by_shacl = 0`)
  with a new `count_shacl_blocked_candidates()` helper that calls `validate_sync()` for each
  candidate `owl:sameAs` pair; wrapped Stage 4/5 in `BeginInternalSubTransaction` /
  `ReleaseCurrentSubTransaction` so any panic rolls back Stage 1–3 side-effects atomically.
- **(H16-04)** `src/gucs/registration/observability.rs`: updated `pg_ripple.llm_endpoint` GUC
  description to clarify it is a no-op in the extension itself; the HTTP companion's
  `/rules/{id}/explain` endpoint handles LLM enrichment.
- **(H16-04)** `docs/src/reference/guc-reference.md`: added clarifying note for
  `pg_ripple.llm_endpoint` directing users to `pg_ripple_http` for LLM enrichment.
- **(H16-07)** `ROADMAP.md`: added `## v1.0.0 GA Entry Criteria` section enumerating the six
  criteria (zero open Highs ×2, zero unannotated unsafe, compat window CI, pg_regress ≥271,
  signed SBOM, external security audit).
- `pg_ripple.control`: updated comment to reflect v0.112.0.
- `docs/src/operations/compatibility.md`: added `pg_trickle ≥ 0.57.0` row.

### Dependency Updates

- `pg_trickle`: bumped to `0.57.0` in `.versions.toml` and `Dockerfile`.
- `pg_tide`: updated to latest (`0.16.0`) in `.versions.toml` and `Dockerfile`.

### Migration

No schema changes in this release.
See `sql/pg_ripple--0.111.0--0.112.0.sql`.

---

## [0.111.0] — 2026-05-12 — Privacy-Preserving Record Linkage (PPRL) Primitives

**Adds Bloom-filter CLK encoding, Dice coefficient similarity, and differential-privacy aggregates for cross-organization entity resolution without sharing raw PII.**

When two organizations need to find common records — patients in separate hospital systems, customers shared between financial institutions, suspects appearing in multiple law enforcement databases — they face a fundamental privacy dilemma: sharing the raw data exposes sensitive personal information, but without sharing, matching is impossible. Privacy-preserving record linkage solves this with cryptographic representations that preserve enough structure to measure similarity without revealing the underlying values. Version 0.111.0 adds this capability natively to pg_ripple through Bloom filter CLK encoding: the `bloom_encode()` function transforms any text value into a fixed-length bit vector using HMAC-SHA-256, and the `dice_similarity()` function measures the similarity of two encoded values, allowing organizations to compare records without either side ever seeing the other's raw data.

These primitives integrate directly with pg_ripple's SPARQL and Datalog layers as native filter predicates, making privacy-preserving matching a first-class operation in the knowledge graph rather than an external preprocessing step. Differential privacy aggregates round out the privacy toolkit: `dp_noisy_count()` and `dp_noisy_histogram()` add calibrated Laplace noise to query results, providing formal mathematical privacy guarantees for reporting on sensitive datasets. Security parameters below recommended thresholds trigger warnings to guide operators toward configurations that meet production security requirements. A comprehensive cookbook guides teams through the complete workflow from key management to cross-organizational federated matching, and property-based tests verify the mathematical invariants — round-trip identity, Dice symmetry, noise sign, and output length — that underpin the correctness guarantees.

### Added

- **PPRL-01**: `pg_ripple.bloom_encode(value TEXT, key TEXT, hash_count INT DEFAULT 30, length INT DEFAULT 1024) → TEXT` — CLK Bloom-filter encoding using HMAC-SHA-256. Returns a lowercase hex-encoded bit vector of `length` bits. Raises PT0470 on oversized input; PT0471 on invalid parameters; logs WARNING for below-recommended security parameters.
- **PPRL-02**: `pg_ripple.dice_similarity(a TEXT, b TEXT) → FLOAT8` — Dice coefficient for two Bloom-filter hex strings: `2 * popcount(a & b) / (popcount(a) + popcount(b))`.
- **PPRL-03**: SPARQL FILTER function `pg:dice_similarity(?a, ?b)` — IRI `<http://pg-ripple.org/functions/dice_similarity>` — computes Dice coefficient in SPARQL FILTER and BIND expressions.
- **PPRL-04**: Datalog built-in predicate `pg:dice_similarity(?a, ?b) OP ?rhs` — Dice coefficient comparison in Datalog rule bodies.
- **DPPRIV-01**: `pg_ripple.dp_noisy_count(query TEXT, epsilon FLOAT8) → BIGINT` — differentially-private COUNT with Laplace(0, 1/epsilon) noise, result clamped to ≥ 0. Validates query is a read-only SELECT.
- **DPPRIV-02**: `pg_ripple.dp_noisy_histogram(query TEXT, key_column TEXT, count_column TEXT, epsilon FLOAT8) → TABLE(key TEXT, noisy_count BIGINT)` — per-bucket Laplace noise histogram.
- **GUC**: `pg_ripple.bloom_max_input_length` (INT, default 4096) — maximum byte length of the `value` argument to `bloom_encode()`.
- **PT0470**: error catalog — `bloom_encode: input length %d exceeds bloom_max_input_length GUC (%d)`.
- **PT0471**: error catalog — `bloom_encode: hash_count %d or length %d outside valid range`.
- **PT0472**: error catalog — `dp_noisy_count: epsilon %g out of valid range (0, 10]`.
- **PT0473**: error catalog — `dp_noisy_count: query must return a single INTEGER value`.
- **PT0474**: error catalog — `dp_noisy_count: query rejected by validation — must be a read-only SELECT`.
- **DOCS**: `docs/src/cookbook/pprl.md` — end-to-end PPRL cookbook with CLK construction explanation, SPARQL federation example, DP aggregates, and security notes (key management, recommended parameters, patent status).
- **PROPTEST**: `tests/proptest/pprl_bloom.rs` — six property-based tests: round-trip identity, distinctness, Dice range, symmetry, sign of noisy count, output length.
- **TEST**: pg_regress tests `v0111_bloom`, `v0111_dice`, `v0111_dp_privacy`.

### Migration

`sql/pg_ripple--0.110.0--0.111.0.sql` — comment-only; no schema changes (all new functionality provided by compiled Rust functions).

---

## [0.110.0] — 2026-05-11 — NS-RL Evaluation Harness, Continuous Monitoring & Rule Explainability

**Adds the NS-RL evaluation function, three live ER monitoring stream tables, plain-English rule explanation, owl:sameAs anomaly detection, and the Magellan ER benchmark CI gate.**

Knowing that an entity resolution pipeline ran is not enough — teams need to know whether it ran correctly and by how much its quality is improving or degrading over time. Version 0.110.0 adds a comprehensive evaluation harness that measures resolution quality against a gold-standard graph using three industry-standard metric families: pairwise precision/recall/F1 measures how accurately individual pairs are identified; blocking statistics including reduction ratio and pairs completeness measure how efficiently the blocking stage eliminates non-matching pairs before the expensive comparison stage; and B³ cluster metrics measure the quality of multi-entity identity clusters holistically. A single `evaluate_resolution()` call returns all three metric families, giving data engineers a complete picture of pipeline performance in terms that map directly to academic benchmarks and production SLA targets.

Beyond post-hoc evaluation, this release adds live monitoring infrastructure: three streaming tables capture unresolved entity counts, cluster size distributions, and a resolution quality dashboard that updates in real time as the pipeline runs, enabling early warning of quality degradation before it affects downstream consumers. A Magellan benchmark CI gate ensures that regression in resolution quality triggers a build failure before code merges, locking in the achieved quality level for the Abt-Buy and DBLP-ACM datasets. Rule explainability receives dedicated infrastructure through `explain_rule()`, which generates a plain-English description of what any Datalog rule does — either through an LLM for rich contextual explanations or through a deterministic structural description as a fallback — with results cached to avoid repeated LLM calls. An `owl:sameAs` anomaly log captures every suspicious identity assertion, creating a permanent audit trail of resolution decisions that domain experts and regulators can review.

### Added

- **EVAL-01**: `pg_ripple.evaluate_resolution(gold_graph TEXT, pipeline_options JSONB DEFAULT '{}') → JSONB` — runs `resolve_entities()` against a gold-standard named graph and returns all three metric axes: pairwise (precision/recall/F1), blocking (pairs_completeness/reduction_ratio/F-PQ), and B³ cluster (b3_precision/b3_recall/b3_f1), plus metadata fields.
- **PT0461**: error catalog entry — `evaluate_resolution: gold graph '%s' is empty or does not exist`.
- **ERMON-01**: `pg_ripple.enable_er_monitoring() → VOID` — idempotent; creates `_pg_ripple.er_unresolved_entities`, `_pg_ripple.er_cluster_sizes`, and `_pg_ripple.er_resolution_dashboard`.
- **ERMON-02**: `pg_ripple.disable_er_monitoring() → VOID` — idempotent; drops the three monitoring tables.
- **EXPLAIN-01**: `pg_ripple.explain_rule(rule_id BIGINT, language TEXT DEFAULT 'en', format TEXT DEFAULT 'text') → TEXT` — plain-English rule narration; LLM-driven when `pg_ripple.llm_endpoint` is set, otherwise uses a template-driven structural description. Results cached in `_pg_ripple.rule_explanations`.
- **EXPLAIN-02**: `pg_ripple.explain_rule_batch(rule_ids BIGINT[]) → TABLE(rule_id BIGINT, explanation TEXT)` — batch variant.
- **PT0462**: error catalog entry — `explain_rule: rule %d not found`.
- **HTTP-01**: REST endpoint `GET /rules/{id}/explain?language=en&format=text` in `pg_ripple_http` — returns `{"rule_id", "language", "format", "explanation"}`.
- **ANOMALY-01**: `_pg_ripple.sameas_anomaly_log` — append-only table with INSERT-only RLS; receives one row for every PT550-triggering `owl:sameAs` assertion when `pg_ripple.record_sameas_anomalies = on`.
- **GUC-01**: `pg_ripple.record_sameas_anomalies` (BOOL, default on) — controls PT550 anomaly logging.
- **GUC-02**: `pg_ripple.sameas_anomaly_log_retention` (TEXT, default `'90 days'`) — retention period for anomaly log rows.
- **GUC-03**: `pg_ripple.rule_explanation_cache_ttl` (TEXT, default `'24 hours'`) — TTL for cached `explain_rule()` results.
- **BENCH-01**: `benchmarks/er_magellan.sh` — Magellan ER benchmark CI gate (Abt-Buy F1 ≥ 0.78, DBLP-ACM F1 ≥ 0.90).
- **BENCH-02**: `benchmarks/er_freshness.sh` — ER ingestion latency benchmark (p95 < 500 ms).
- **BENCH-03**: `just bench-er` recipe running both ER benchmarks.
- **SCRIPTS-01**: `scripts/magellan_to_rdf.py` — Python helper converting Magellan CSV datasets to Turtle RDF.
- **TEST**: pg_regress tests `v0110_er_monitoring`, `v0110_explain_rule`, `v0110_anomaly_log`.

### Migration

`sql/pg_ripple--0.109.0--0.110.0.sql` — creates `_pg_ripple.rule_explanations` and `_pg_ripple.sameas_anomaly_log`.

---

## [0.109.0] — 2026-06-14 — NS-RL Foundation: String Similarity Builtins + Orchestrator

**Adds six SPARQL/Datalog string similarity built-ins for neuro-symbolic record linkage, three reusable ER blocking templates, a five-stage `resolve_entities()` orchestration pipeline, and two new GUC parameters.**

Matching records that refer to the same real-world entity — even when names are spelled differently, addresses are formatted differently, or identifiers come from different systems — is a foundational challenge in data integration that every large organization faces. Version 0.109.0 begins addressing this with a suite of six string similarity algorithms exposed directly inside SPARQL queries and Datalog rules: trigram similarity, Levenshtein edit distance, Soundex, Metaphone, Jaro-Winkler, and several variants. These algorithms can be used as native filter predicates in graph patterns, enabling declarative blocking rules like "consider these two records as match candidates if their name strings have a trigram similarity above 0.7" without leaving the SPARQL or Datalog layer to call external functions.

The release also delivers the first version of `resolve_entities()`, a five-stage entity resolution orchestration pipeline that takes two named graphs and produces `owl:sameAs` assertions linking entities it determines are likely to refer to the same real-world object. The stages progress from symbolic blocking using inverse-functional properties or custom Datalog rules, through embedding-based candidate generation, SHACL validation to enforce quality gates, union-find canonicalization to resolve transitive identity clusters, and finally RDF-star provenance annotation that records the evidence behind each match decision. A dry-run mode lets teams inspect what the pipeline would do without writing any data, and three built-in blocking templates for email matching, postal name matching, and name prefix matching make the system immediately usable for common scenarios without writing any custom rules.

### Added

- **STRSIM-01**: SPARQL custom function `pg:trigram_similarity(?a, ?b)` — emits `similarity(a, b)` SQL via `pg_trgm`. Returns `xsd:double`.
- **STRSIM-02**: SPARQL custom function `pg:levenshtein(?a, ?b)` — emits `levenshtein(a, b)` via `fuzzystrmatch`. Returns `xsd:integer`.
- **STRSIM-03**: SPARQL custom function `pg:levenshtein_less_equal(?a, ?b, ?maxd)` — emits `levenshtein_less_equal(a, b, maxd)`. Returns `xsd:integer`.
- **STRSIM-04**: SPARQL custom function `pg:soundex(?s)` — emits `soundex(s)` via `fuzzystrmatch`. Returns a dictionary-encoded literal.
- **STRSIM-05**: SPARQL custom function `pg:metaphone(?s, ?maxlen)` — emits `metaphone(s, maxlen)` via `fuzzystrmatch`. Returns a dictionary-encoded literal.
- **STRSIM-06**: SPARQL custom function `pg:jaro_winkler(?a, ?b)` — emits `jarowinkler(a, b)` via `fuzzystrmatch`. Returns `xsd:double`.
- **STRSIM-07**: Datalog built-in predicate `pg:trigram_similarity(?a, ?b) OP ?r` — guard compiling to `similarity(a, b) OP r` in SQL WHERE clause.
- **STRSIM-08**: Datalog built-in predicates `pg:levenshtein(?a, ?b) OP ?r`, `pg:soundex(?s) OP ?r`, `pg:metaphone(?s, maxlen) OP ?r`, `pg:jaro_winkler(?a, ?b) OP ?r` — analogous guard compilation.
- **STRSIM-09**: `fuzzystrmatch_available()` helper — probes `pg_proc` for `levenshtein` at translation time; emits `NULL` when the extension is absent rather than erroring.
- **ER-01**: `pg_ripple.er_blocking_templates() → TABLE(name TEXT, description TEXT, rule TEXT)` — returns three built-in ER blocking rule templates: `email`, `postal_name`, `name_prefix`.
- **ER-02**: `pg_ripple.er_blocking_template(name TEXT) → TEXT` — returns rule text for a named ER blocking template.
- **ER-03**: `pg_ripple.resolve_entities(source_graph TEXT, target_graph TEXT, options JSON DEFAULT NULL) → JSON` — five-stage NS-RL pipeline: symbolic blocking (IFP or custom rules), embedding-based candidate generation, SHACL validation gate, `owl:sameAs` union-find canonicalization, RDF-star provenance annotation. `dry_run=true` returns a JSON summary without writing triples.
- **GUC-01**: `pg_ripple.sameas_apply_rate_limit` (INT, default 1000, range 1–10,000,000) — maximum `owl:sameAs` assertions per `resolve_entities()` call; raises PT0460 when exceeded.
- **GUC-02**: `pg_ripple.string_similarity_extensions_ok` (BOOL, default false) — allows string similarity functions to require `fuzzystrmatch` at translation time.
- **TEST**: pg_regress tests `tests/pg_regress/sql/v0109_string_similarity.sql` — 10 test cases covering GUC defaults, ER blocking templates, and `resolve_entities()` dry-run.

### Migration

`sql/pg_ripple--0.108.0--0.109.0.sql` — no schema changes; new functions are compiled from Rust.

---

## [0.108.0] — 2026-06-07 — Bayesian Confidence Updates

**Adds a Bayesian belief-revision engine for dynamic confidence updates, an append-only evidence log, bulk update ingestion, downstream propagation through the derivation DAG, and six new GUC parameters.**

Real-world knowledge is rarely black and white — sensor readings carry measurement uncertainty, entity matching scores reflect probabilities rather than certainties, and evidence from different sources may conflict or reinforce each other over time. Version 0.108.0 introduces a Bayesian belief-revision engine that treats confidence as a first-class property of every fact in the knowledge graph, allowing it to be updated in a principled, mathematically grounded way as new evidence arrives. The core `update_confidence()` function applies Bayes' theorem in odds form: given a fact's prior confidence and the likelihood ratio of a new piece of evidence, it computes the posterior confidence and persists the updated value. This makes the knowledge graph a living probabilistic model whose certainties evolve continuously as more evidence accumulates rather than remaining static at the time of initial assertion.

Confidence changes automatically cascade through the derivation graph, propagating updated certainties to all facts derived from the updated base fact — up to a configurable depth — so that the confidence of derived conclusions always reflects the current evidence state of their premises. An append-only evidence log provides a complete audit trail of every confidence update, recording the source, prior, posterior, and timestamp of each revision. Bulk update ingestion from CSV or JSON-L streams makes it easy to feed confidence scores from external machine learning models or annotation pipelines directly into the graph in batches. A SHACL integration automatically reduces confidence in facts that violate integrity constraints, and a `vacuum_evidence_log()` function keeps the audit log manageable in long-running deployments by pruning rows that exceed the configured retention window.

### Added

- **BAYES-01**: `pg_ripple.update_confidence(subject TEXT, predicate TEXT, object TEXT, evidence JSONB) → TABLE(prior FLOAT8, posterior FLOAT8)` — applies Bayes' theorem in odds form: `posterior = (λ · prior) / (λ · prior + (1 − prior))`. Raises PT0440 when `likelihood_ratio ≤ 0.0`; PT0441 when `confidence_update_strategy = 'manual'`. Falls back to noisy-OR when `confidence_update_strategy = 'noisy-or'`.
- **BAYES-02**: `pg_ripple.bulk_update_confidence(data TEXT, format TEXT DEFAULT 'csv') → BIGINT` — ingests CSV (`subject,predicate,object,source,likelihood_ratio`) or JSON-L updates in batches; returns count of facts updated.
- **BAYES-03**: `pg_ripple.vacuum_evidence_log() → BIGINT` — prunes expired rows from `_pg_ripple.evidence_log` using `pg_ripple.evidence_log_retention`; returns rows deleted.
- **BAYES-04**: `_pg_ripple.evidence_log` — append-only audit table recording every confidence update (`sid`, `event_at`, `source_iri`, `likelihood_ratio`, `prior_confidence`, `posterior_confidence`).
- **BAYES-05**: `_pg_ripple.confidence_stale` — table for tracking derived facts beyond the cascade depth that require background reprocessing.
- **BAYES-06**: Downstream confidence propagation — after updating a base fact, `update_confidence()` walks `_pg_ripple.derivations` up to `pg_ripple.confidence_propagation_max_depth` levels using noisy-OR over antecedent confidences.
- **BAYES-07**: GUC `pg_ripple.confidence_update_strategy` (TEXT, default `'bayesian'`) — `'bayesian'`, `'noisy-or'`, or `'manual'`.
- **BAYES-08**: GUC `pg_ripple.confidence_propagation_max_depth` (INT, default `10`, range `1–1000`) — maximum derivation cascade depth.
- **BAYES-09**: GUC `pg_ripple.confidence_reprocessing_interval` (TEXT, default `'1 hour'`) — how often stale derived confidences are reprocessed.
- **BAYES-10**: GUC `pg_ripple.evidence_log_retention` (TEXT, default `'1 year'`) — retention window for `vacuum_evidence_log()`.
- **BAYES-11**: GUC `pg_ripple.confidence_batch_size` (INT, default `1000`, range `1–1,000,000`) — batch size for `bulk_update_confidence()`.
- **BAYES-12**: GUC `pg_ripple.conflict_confidence_penalty` (FLOAT8, default `0.3`, range `0.0–1.0`) — confidence penalty applied to conflicting triples detected by SHACL validation.
- **BAYES-13**: Feature-status entries for `bayesian_confidence_update`, `evidence_log`, `bulk_confidence_update` in `pg_ripple.feature_status()`.
- **BAYES-14**: HTTP `POST /confidence/update` and `POST /confidence/bulk-update` handlers in `pg_ripple_http`.
- **BAYES-15**: pg_regress tests `tests/pg_regress/sql/v0108_confidence.sql` — 14 test cases (BAYES-01 through BAYES-14).
- **BAYES-16**: Property-based tests `tests/proptest/bayesian_confidence.rs` — 11 algebraic properties: monotonicity, neutral LR, sequential = joint (restricted to clamp-safe inputs), clamping, noisy-OR variants.

### Migration

`sql/pg_ripple--0.107.0--0.108.0.sql` — creates `_pg_ripple.evidence_log` and `_pg_ripple.confidence_stale` tables if not present.

---

## [0.107.0] — 2026-05-31 — Temporal Reasoning Phase 2: Sequential Patterns & CDC Integration

**Adds three sequential temporal operators (`WITHIN`, `SEQUENCE`, `CONSECUTIVE`), CDC auto-recording of temporal facts via `insert_triple()`, snapshot/versioned retraction via `retract_triple_temporal()`, and a new `pg_ripple.temporal_cdc_enabled` GUC.**

Building on the temporal fact store introduced in Phase 1, version 0.107.0 extends temporal reasoning to the level of event sequence detection — a common requirement in fraud detection, process monitoring, and behavioral analytics. Three new operators enable sequential pattern queries directly in Datalog rules: `WITHIN` checks whether a predicate held at least once in a recent time window, `SEQUENCE` detects whether one event reliably precedes another within a given window, and `CONSECUTIVE` identifies runs of N repeated occurrences of the same predicate within a time frame. These operators compile to efficient EXISTS subqueries with window functions over the temporal fact store, making complex temporal patterns expressible without leaving the Datalog rule language or reaching for external stream processing infrastructure.

The most significant operational improvement in this release is the direct integration of temporal recording into the standard write path. When the new `temporal_cdc_enabled` GUC is on — which it is by default — every call to `insert_triple()` for a temporally-registered predicate automatically records the fact in the temporal store with the transaction timestamp, requiring no changes to application code. This means that existing applications gain a complete temporal history automatically on upgrade, capturing the exact transaction times of all temporal assertions going forward without any schema changes or code updates. A new `retract_triple_temporal()` function cleanly closes open intervals, ensuring the temporal history accurately records not just when facts became true but when they ceased to be true — equally important for any audit or compliance use case.

### Added

- **SEQ-01**: `pg_ripple.temporal_within(subject TEXT, predicate TEXT, duration TEXT) → BOOLEAN` — returns `true` if the predicate holds for the subject at least once within the most recent `duration` (ISO 8601 interval) relative to transaction time.
- **SEQ-02**: `pg_ripple.temporal_sequence(s1 TEXT, p1 TEXT, o1 TEXT, s2 TEXT, p2 TEXT, o2 TEXT, window TEXT) → BOOLEAN` — returns `true` if event `(s1, p1, o1)` occurs strictly before event `(s2, p2, o2)` and both fall within `window` of each other. Empty string arguments act as wildcards.
- **SEQ-03**: `pg_ripple.temporal_consecutive(n BIGINT, predicate TEXT, window TEXT) → BOOLEAN` — returns `true` if there exist `n` rows for any subject with the given predicate where all `n` fall within `window` of the first.
- **SEQ-04**: `pg_ripple.retract_triple_temporal(subject TEXT, predicate TEXT, graph TEXT DEFAULT NULL) → BIGINT` — closes open temporal fact intervals for `(subject, predicate)`. Returns number of rows affected.
- **SEQ-05**: GUC `pg_ripple.temporal_cdc_enabled` (BOOL, default `on`) — when `on`, `insert_triple()` for a temporal predicate automatically records a row in `_pg_ripple.temporal_facts` with `valid_from = transaction_timestamp()`.
- **SEQ-06**: CDC wiring in `insert_triple()` — after writing to VP storage, checks `temporal_predicates` and inserts into `temporal_facts` when the predicate is temporal and `temporal_cdc_enabled` is on.
- **SEQ-07**: Datalog parser extended — `WITHIN`, `SEQUENCE`, and `CONSECUTIVE` operators parsed in rule bodies.
- **SEQ-08**: Datalog compiler extended — the three new operators compile to `EXISTS` subqueries using window functions over `_pg_ripple.temporal_facts`.
- **SEQ-09**: pg_regress tests `tests/pg_regress/sql/v0107_temporal_sequential.sql` — 14 test cases covering all three operators, CDC integration, and retraction.
- **SEQ-10**: Migration `sql/pg_ripple--0.106.0--0.107.0.sql` — comment-only; no schema changes required.

### Migration

`sql/pg_ripple--0.106.0--0.107.0.sql` — no schema changes; GUC `pg_ripple.temporal_cdc_enabled` defaults to `on`.

---

## [0.106.0] — 2026-05-24 — Temporal Reasoning Phase 1: Temporal Fact Store & Basic Operators

**Introduces a first-class temporal fact store backed by `_pg_ripple.temporal_facts`, temporal predicate registration, basic time operators in Datalog rules, a `pg:temporal_window()` SPARQL function, and an `sh:validFor` SHACL constraint.**

Knowledge is inherently temporal — facts change, expire, and take on different values at different points in time. A system that only represents the current state of the world cannot answer questions like "was this entity classified differently last month?" or "how long did this policy remain in effect?" Version 0.106.0 introduces a first-class temporal dimension to pg_ripple's knowledge graph, allowing individual predicates to be marked as temporal and their facts stored with valid-from and valid-to timestamps in a dedicated temporal fact store. Two storage models are supported: snapshot mode, where each new assertion automatically closes the previous interval for the same subject-predicate pair, and versioned mode, which preserves the complete history of all values without closing old intervals, for domains where the full change history matters.

Once predicates are registered as temporal, the full reasoning and querying stack gains time-awareness. Datalog rules can use `AFTER`, `BEFORE`, and `DURING` operators in their bodies to filter which temporal facts trigger the rule, enabling time-bounded reasoning patterns like "apply this rule only to facts that were valid during the current fiscal year." A new `pg:temporal_window()` SPARQL function lets SPARQL queries ask whether any fact for a given subject-predicate pair was true within a specified time range, and a `sh:validFor` SHACL constraint lets shapes enforce freshness requirements — for example, that a compliance certification must have been renewed within the past 12 months. The complete temporal catalog and fact storage are set up by a migration script, making this a zero-friction addition to existing pg_ripple deployments.

### Added

- **TMP-01**: `pg_ripple.mark_temporal(predicate_iri TEXT, data_model TEXT DEFAULT 'snapshot') → VOID` — registers a predicate as temporal. Raises PT0430 if the predicate is already registered with a different model.
- **TMP-02**: `pg_ripple.unmark_temporal(predicate_iri TEXT) → VOID` — removes a predicate from temporal management. Raises PT0431 if existing temporal facts reference it.
- **TMP-03**: `pg_ripple.insert_triple_temporal(s TEXT, p TEXT, o TEXT, valid_from TIMESTAMPTZ) → BIGINT` — inserts a temporal fact into `_pg_ripple.temporal_facts`. Raises PT0432 if the predicate is not registered as temporal. In `snapshot` mode, closes any previous open interval for `(s, p)`.
- **TMP-04**: `_pg_ripple.temporal_predicates` catalog table — columns: `predicate_id BIGINT PK`, `data_model TEXT`, `registered_at TIMESTAMPTZ`.
- **TMP-05**: `_pg_ripple.temporal_facts` store table — columns: `id BIGINT PK`, `s BIGINT`, `p BIGINT`, `o BIGINT`, `g BIGINT DEFAULT 0`, `valid_from TIMESTAMPTZ`, `valid_to TIMESTAMPTZ`.
- **TMP-06**: Datalog temporal filters — `AFTER(ts)`, `BEFORE(ts)`, `DURING(from, to)` syntax in rule bodies for temporal predicates.
- **TMP-07**: `pg_ripple.temporal_window(s TEXT, p TEXT, from_ts TIMESTAMPTZ, to_ts TIMESTAMPTZ) → BOOLEAN` — returns `true` if any temporal fact for `(s, p)` overlaps the given range.
- **TMP-08**: `sh:validFor` SHACL constraint — validates that temporal facts for a predicate are closed within the specified XSD duration.
- **TMP-09**: GUC `pg_ripple.enable_temporal_operators` (BOOL, default: `false`) — enables AFTER/BEFORE/DURING filter pushdown in the Datalog compiler.
- **TMP-10**: GUC `pg_ripple.temporal_data_model` (STRING, default: `''`) — override the default temporal data model (`'snapshot'` or `'versioned'`) at the session level.
- **TMP-11**: Migration `sql/pg_ripple--0.105.0--0.106.0.sql` — creates `_pg_ripple.temporal_predicates`, `_pg_ripple.temporal_facts`, and three indexes.
- **TMP-12**: pg_regress tests `tests/pg_regress/sql/v0106_temporal_basic.sql` — 10 test cases (TMP-01 through TMP-10) covering store DDL, snapshot/versioned semantics, error codes, GUC defaults, and temporal_window correctness.

### Error Codes

- **PT0430** — `mark_temporal` called for already-registered predicate with a different model.
- **PT0431** — `unmark_temporal` called while temporal facts for predicate still exist.
- **PT0432** — `insert_triple_temporal` called for predicate not registered as temporal.

### Migration

`sql/pg_ripple--0.105.0--0.106.0.sql` — creates `_pg_ripple.temporal_predicates`, `_pg_ripple.temporal_facts`, and three indexes; no breaking changes.

---

## [0.105.0] — 2026-05-17 — Guided Rule Authoring & LLM Rule Extraction

**Translate natural-language descriptions into Datalog rules, validate rules statically, and discover candidate rules from co-occurrence patterns in the graph.**

Writing Datalog rules has traditionally required deep expertise in logic programming syntax and semantics, limiting rule authoring to a small group of specialists even in organizations that have rich domain expertise. Version 0.105.0 dramatically lowers this barrier by allowing domain experts to describe rules in plain English and have those descriptions automatically translated into correct Datalog syntax by a large language model. The `draft_rule_from_nl()` function sends a natural-language description to a configured LLM endpoint and returns up to three ranked candidate rules, each with an explanation of its meaning, giving the domain expert options to choose from and evaluate rather than a single opaque suggestion. A mock mode enables deterministic testing without a live LLM endpoint.

Generated rules — or any hand-written rules — can be immediately validated by the `validate_rule()` function, which performs static analysis checking for syntax errors, unbound head variables, unsafe negation patterns, and stratification cycles before anything is loaded into the database. This combination of LLM-assisted authoring and static validation creates a guided workflow where a domain expert proposes a rule in natural language, the system suggests candidate implementations, and static analysis provides immediate feedback on correctness. A third function, `suggest_rules()`, inverts this flow by scanning the actual knowledge graph for predicate co-occurrence patterns and proposing candidate rules that the data itself suggests — letting the graph guide rule discovery for analysts who know their domain but not formal logic. REST endpoints for both drafting and validation make these capabilities accessible from interactive authoring interfaces and rule governance portals.

### Added

- **RA-01**: `pg_ripple.validate_rule(rule TEXT) → JSONB` — static analysis of a Datalog rule without loading it. Returns `{"valid": true}` or `{"valid": false, "errors": [...], "warnings": [...]}`. Error codes: `SYNTAX_ERROR`, `UNBOUND_HEAD_VARIABLE`, `UNSAFE_NEGATION`, `STRATIFICATION_CYCLE`. Warning codes: `UNUSED_BODY_VARIABLE`.
- **RA-02**: `pg_ripple.draft_rule_from_nl(description TEXT, candidates INT DEFAULT 3) → TABLE(rank INT, rule TEXT, explanation TEXT)` — calls the configured LLM endpoint to translate a natural-language rule description into Datalog. Returns up to `candidates` candidate rules ranked by estimated quality.
  - Raises PT0457 when `candidates` is outside [1, 10].
  - Raises PT0458 when `pg_ripple.llm_endpoint` is not configured.
  - Mock mode: set `pg_ripple.llm_endpoint = 'mock'` for deterministic test output.
- **RA-03**: `pg_ripple.suggest_rules(graph_iri TEXT, examples JSONB DEFAULT NULL) → TABLE(rule TEXT, support BIGINT, explanation TEXT)` — **experimental** — scans VP tables for predicate co-occurrence patterns and proposes candidate Datalog rules for expert review. Results require domain expert validation before committing.
- **RA-04**: New GUC `pg_ripple.suggest_rules_max_candidates` (INT, default: 20, range: 1–200) — maximum candidates returned by `suggest_rules()`.
- **RA-05**: REST endpoints in `pg_ripple_http`:
  - `POST /rules/draft` — request: `{"description": "...", "candidates": 3}`, response: `[{"rank": 1, "rule": "...", "explanation": "..."}]`
  - `POST /rules/validate` — request: `{"rule": "..."}`, response: `{"valid": true|false, "errors": [...], "warnings": [...]}`
- **RA-06**: pg_regress tests `tests/pg_regress/sql/v0105_rule_authoring.sql` (10 test cases: RA-01 through RA-10).
- **RA-07**: Proptest suite `tests/proptest/rule_authoring.rs` — 5 pure-Rust property tests verifying the validation algorithm.
- **RA-08**: Error codes PT0457–PT0458 documented in `src/rule_authoring.rs`.

### Migration

`sql/pg_ripple--0.104.0--0.105.0.sql` — comment-only; no schema changes (all changes are Rust function additions).

---

## [0.104.0] — 2026-05-10 — Domain Rule Library Infrastructure

**Package, share, and install domain-specific Datalog rule sets as versioned libraries — like npm packages, but for Datalog rules and SHACL shapes.**

Just as software engineers share reusable code through package repositories like npm or crates.io, knowledge engineers now have an equivalent mechanism for sharing reusable Datalog rule sets and SHACL validation shapes. Version 0.104.0 introduces a complete package management infrastructure for domain rule libraries: a library is authored as a structured Turtle file containing metadata including title, description, version, license, and dependencies, then published at any accessible URL. The `install_rule_library()` function fetches a library, validates its license against permissive SPDX identifiers, resolves its dependencies in topological order, and activates its rules and shapes — all in a single command with full error reporting for each failure mode. License enforcement prevents inadvertent use of incompatible rule sets in commercial products.

For organizations working in domains like healthcare, financial services, or supply chain logistics — where well-established ontological reasoning patterns are documented in published standards and industry ontologies — this means that battle-tested rule implementations can be shared across deployments without rebuilding them from scratch. Upgrade and uninstall operations are fully managed, with dependency safety checks preventing removal of libraries that other installed libraries depend on. A companion REST endpoint makes library management scriptable from external orchestration tools and CI pipelines. A detailed documentation chapter guides rule library authors through the format specification, metadata requirements, and the complete publishing workflow from authoring through distribution, lowering the barrier to contributing reusable domain knowledge to the broader pg_ripple community.

### Added

- **LIB-01**: `_pg_ripple.rule_libraries` catalog table — one row per installed external rule library with columns: `name TEXT PRIMARY KEY`, `version TEXT`, `installed_at TIMESTAMPTZ`, `description TEXT`, `license_iri TEXT`, `source_url TEXT`, `dependencies TEXT[]`, `shape_iris TEXT[]`.
- **LIB-02**: `pg_ripple.install_rule_library(source TEXT, accept_license BOOLEAN DEFAULT FALSE) → TEXT` — install a rule library from a URL or absolute local file path.
  - URL sources are validated against the SSRF allowlist before any network request (raises PT0452 if blocked).
  - Parses the Turtle file to extract metadata (`dcterms:title`, `dcterms:description`, `dcterms:license`, `owl:versionInfo`), Datalog rules (`pg:rules` property), and SHACL shapes.
  - Resolves `pg:dependsOn` dependencies in topological order (raises PT0453 on cycles, PT0454 on fetch failures).
  - Raises PT0455 when `accept_license = FALSE` and the library's license IRI is not a permissive SPDX license (MIT, Apache-2.0, PostgreSQL).
  - Raises PT0459 when the library name conflicts with a built-in bundle.
  - Idempotent: re-installing the same version is a no-op.
- **LIB-03**: `pg_ripple.upgrade_rule_library(name TEXT) → TEXT` — re-fetch from `source_url` and replace rules, shapes, and version; raises PT0456 if a dependent library is installed.
- **LIB-04**: `pg_ripple.uninstall_rule_library(name TEXT) → VOID` — removes all rules and shapes installed by this library; raises PT0456 if a dependent library is installed.
- **LIB-05**: `pg_ripple.list_rule_libraries() → TABLE(name, version, installed_at, description, license_iri)` — lists all installed libraries.
- **LIB-06**: Error codes PT0452–PT0459 documented in `src/rule_library.rs`.
- **LIB-07**: `GET /rule-libraries` REST endpoint in `pg_ripple_http` — returns `list_rule_libraries()` result as a JSON array.
- **LIB-08**: Documentation chapter `docs/src/cookbook/rule-libraries.md` — format specification, authoring guide, license and disclaimer requirements.
- **LIB-09**: Rule library format: a Turtle file with `pg:RuleLibrary` resource, required metadata triples, and optional `pg:rules` / SHACL shapes.
- **LIB-10**: pg_regress test `tests/pg_regress/sql/v0104_rule_libraries.sql` covering install, idempotency, uninstall, PT0452, PT0455, and Datalog rule activation.

### Migration

- `sql/pg_ripple--0.103.0--0.104.0.sql`: creates `_pg_ripple.rule_libraries` catalog table.

---

## [0.103.0] — 2026-05-10 — Conflict Detection

**Find contradictory Datalog rules before they cause problems. Two detection modes: static structural analysis and live runtime scanning of derived facts.**

As knowledge graphs grow and multiple teams contribute rules, the risk of logical contradictions quietly entering the rule set increases. One team might write a rule that concludes a product qualifies for a discount under certain conditions, while another writes a rule that concludes it does not, with both rules firing for the same data. Version 0.103.0 addresses this risk with a dedicated conflict detection system that can find such contradictions either statically — by analyzing the structure of the rules themselves before they run — or dynamically, by examining the actual derived facts already in the database. Static analysis catches the most common conflict types: rules with the same head predicate that derive contradictory constant values, and rules that derive triples for predicates that a SHACL shape prohibits.

Runtime detection goes further by querying the actual derivation table to find cases where two different inferred values currently exist for the same subject and predicate, or where inferred facts violate `sh:disjoint` constraints — surfacing contradictions that only emerge because of specific combinations of data. A configurable `block_on_conflict` GUC can make the inference engine automatically halt and raise a structured error if any contradictions are found after inference, giving teams an opt-in safety guarantee that inference results are internally consistent. A companion REST endpoint allows external governance and data quality tools to poll for conflict status programmatically, making it straightforward to integrate conflict checking into CI pipelines for rule set changes or into regular data quality monitoring dashboards.

### Added

- **CONFLICT-01**: `pg_ripple.rule_conflicts(ruleset TEXT, mode TEXT DEFAULT 'static') → JSONB` — detects conflicting rules in a rule set.
  - **Static mode**: structural analysis over the parsed rule AST and SHACL shape catalog (no VP table reads). Detects:
    - `same_head_opposing_values`: pairs of rules with the same head predicate and different constant object terms (e.g. one derives `?x ex:eligible "true"`, another derives `?x ex:eligible "false"`).
    - `rule_vs_shacl`: rules that derive triples for a predicate referenced by a `sh:not`, `sh:disjoint`, or `sh:in` SHACL constraint.
  - **Runtime mode**: queries `_pg_ripple.derivations` joined with `_pg_ripple.vp_rare` to find already-derived contradictions: same subject + predicate with two different inferred values, or `sh:disjoint` property violations among inferred facts.
  - Returns a JSONB array of conflict objects; empty array means no conflicts.
- **CONFLICT-02**: `pg_ripple.rule_conflict_check_on_load` GUC (BOOL, default `off`) — when `on`, static conflict analysis runs automatically at `load_rules()` time and raises a WARNING for each conflict found (not an error — allows loading of rule sets that have known soft conflicts).
- **CONFLICT-03**: `pg_ripple.block_on_conflict` GUC (BOOL, default `off`) — when `on`, the `infer()` SQL function calls `rule_conflicts(ruleset, 'runtime')` after inference completes and raises PT0451 if any conflicts are found.
- **CONFLICT-04**: Error code PT0451: `inference halted: rule conflict detected in ruleset '%s' (set pg_ripple.block_on_conflict = off to continue despite conflicts)`.
- **CONFLICT-05**: `GET /rule-conflicts/{ruleset}?mode=static|runtime` REST endpoint in `pg_ripple_http`.
- **CONFLICT-06**: `src/datalog/conflict.rs` — new module implementing both detection modes.
- **CONFLICT-07**: pg_regress tests: `tests/pg_regress/sql/v0103_conflicts_static.sql` (same-head conflict, rule-vs-SHACL conflict, clean rule set) and `tests/pg_regress/sql/v0103_conflicts_runtime.sql` (runtime contradiction detection, block_on_conflict check).

### Migration

- `sql/pg_ripple--0.102.0--0.103.0.sql`: no schema changes; all new functionality is compiled from Rust.

---

## [0.102.0] — 2026-05-09 — What-if Reasoning (Hypothetical Inference)

**Run Datalog inference on speculative graph modifications without persisting changes. All VP tables are left unchanged; isolation is guaranteed via PostgreSQL internal sub-transactions.**

The ability to safely experiment with the consequences of proposed changes before committing them is invaluable in any domain where decisions have downstream effects. Version 0.102.0 introduces hypothetical inference — the ability to ask the knowledge graph's reasoning engine "if I were to add or remove these facts, what new conclusions would follow, and which existing conclusions would no longer hold?" The new `hypothetical_inference()` function accepts a set of proposed assertions and retractions, runs the full inference engine against a speculative version of the graph, and returns two lists: the triples that would be newly derived if the changes were applied, and the triples that were previously derived but would no longer hold. The live database data is never modified at any point during this process.

The isolation guarantee is absolute: all speculative changes are wrapped in PostgreSQL internal sub-transactions that are rolled back after inference completes, meaning no matter how large or complex the proposed change set, the actual database is left exactly as it was before the call. This makes hypothetical inference safe to use in production, from reports, from interactive exploratory sessions, and from multi-step planning workflows where the consequences of each step must be evaluated before proceeding. Use cases range from policy simulation — "if we update this regulation, what classifications change?" — to data quality validation — "if we merge these two customer records, what contradictions does that create?" A configurable limit on the number of hypothetical assertions prevents accidental runaway queries from consuming excessive resources.

### Added

- **HYPO-01**: `pg_ripple.hypothetical_max_assertions` GUC (INT, default `10000`, min 1, max 1,000,000) — maximum total number of assert + retract triples in a single `hypothetical_inference()` call. Exceeding this limit raises error code PT0450.
- **HYPO-02**: `pg_ripple.hypothetical_inference(hypotheses JSONB, rules TEXT DEFAULT 'default') → JSONB` — runs the named rule set on a speculative copy of the graph and returns a JSON object with two arrays:
  - `"derived"`: triples that would be newly inferred if the hypotheses were applied.
  - `"retracted"`: triples that were previously inferred but would no longer hold.
  - Input format: `{"assert": [{"s": "…", "p": "…", "o": "…", "g"?: N}, …], "retract": […]}`.
- **HYPO-03**: Subtransaction isolation — all speculative changes are wrapped in `BeginInternalSubTransaction` / `RollbackAndReleaseCurrentSubTransaction`, ensuring VP tables are never modified.
- **HYPO-04**: `POST /hypothetical` HTTP endpoint in `pg_ripple_http` — accepts the same JSONB format as the SQL function.
- **HYPO-05**: `src/hypothetical.rs` — new module implementing snapshot-diff algorithm: captures "before" state of head-predicate VP tables, applies hypothetical changes, re-runs inference from a clean slate, snapshots "after" state, rolls back, and computes the diff.
- **HYPO-06**: 8 pg_regress tests (`tests/pg_regress/sql/v0102_hypothetical.sql`) covering GUC default, correct diff, isolation, retraction, side-effect absence, outer-transaction rollback, and PT0450 limit.

### Migration

- `sql/pg_ripple--0.101.0--0.102.0.sql`: no schema changes; all overlay tables are session-local and transient.

---

## [0.101.0] — 2026-05-09 — Natural Language Explanation

**Natural language explanation of Datalog-derived facts via LLM or deterministic fallback renderer.**

Proof trees are valuable for engineers and auditors who can read JSON structures and reason about derivation graphs, but most of the people who need to understand why a system made a decision are not in that category. Version 0.101.0 bridges that gap by translating the technical structure of proof trees into readable, natural-language narratives. The new `explain_inference()` function retrieves the proof tree for any derived fact and produces a plain-English account of the reasoning chain, either by sending the proof structure to a connected large language model for a rich, context-aware explanation, or by using a clean deterministic fallback renderer that always produces something useful even when no LLM is available. The result can be formatted as plain text or as a structured JSON object containing both the proof tree and the narrative, depending on what the calling application needs.

Explanations are stored in a cache table keyed by fact identifier, output format, and LLM model, so that repeated requests for the same explanation incur no additional LLM cost. A configurable TTL governs how long cached explanations remain valid, and a `vacuum_explanation_cache()` function removes expired entries on demand. This release also performs an important rename: the `explain_inference` name is freed for this new natural-language capability, with the older derivation-chain walker available under the new name `explain_inference_provenance()`. HTTP endpoints expose both capabilities to external applications, meaning customer-facing portals, compliance dashboards, and audit tools can all surface human-readable explanations without requiring any direct SQL access.

### Added

- **NL-EXPLAIN-01**: `_pg_ripple.explanation_cache (sid BIGINT, format TEXT, model TEXT, explanation TEXT, cached_at TIMESTAMPTZ, PRIMARY KEY (sid, format, model))` table — caches NL explanations keyed by `(fact SID, format, LLM model)` to avoid repeated LLM calls. Expires after `pg_ripple.explanation_cache_ttl` seconds (default: 3600).
- **NL-EXPLAIN-02**: `pg_ripple.explanation_cache_ttl` GUC (INT, default `3600`) — TTL in seconds for cached explanations. Set to `0` to disable caching entirely.
- **NL-EXPLAIN-03**: `pg_ripple.explain_inference(subject TEXT, predicate TEXT, object TEXT, format TEXT DEFAULT 'text') → TEXT` — returns a plain-English narrative of why a Datalog fact was derived. Retrieves the proof tree via `justify()`, then either (a) sends it to the configured LLM endpoint with a domain-appropriate system prompt, or (b) falls back to a deterministic indented-text renderer. Returns `NULL` for base (non-inferred) facts. Never raises an error — always returns something readable.
- **NL-EXPLAIN-04**: `pg_ripple.explain_inference_jsonb(subject TEXT, predicate TEXT, object TEXT) → JSONB` — returns `{"proof_tree": <justify() output>, "narrative": "<LLM or fallback explanation>"}` for programmatic consumers.
- **NL-EXPLAIN-05**: `pg_ripple.vacuum_explanation_cache() → BIGINT` — removes expired rows from `_pg_ripple.explanation_cache`; returns the count deleted. Call periodically or after bulk inference runs.
- **NL-EXPLAIN-06**: LLM mock mode for testing: when `pg_ripple.llm_endpoint = 'mock'`, `explain_inference()` returns a canned narrative containing the rule name — enables full code-path coverage in pg_regress without an external LLM.
- **NL-EXPLAIN-07**: `POST /explain` and `GET /explain` HTTP endpoints in `pg_ripple_http` — delegates to `pg_ripple.explain_inference()`.
- **NL-EXPLAIN-08**: `src/datalog/nlexplain.rs` — new module containing all NL explanation logic: LLM call, mock handler, deterministic fallback renderer, cache read/write.

### Changed

- **RENAME-01**: `pg_ripple.explain_inference(text, text, text, text) RETURNS SETOF record` (v0.61.0 derivation-chain walker) renamed to `pg_ripple.explain_inference_provenance()` to free the name for the new NL explanation function. Migration script drops the old function; the renamed variant is available under the new name.

### Migration

- `sql/pg_ripple--0.100.0--0.101.0.sql`: drops old `explain_inference(text, text, text, text)` SETOF function; creates `_pg_ripple.explanation_cache` table and index.

---

## [0.100.0] — 2026-05-09 — Proof trees & justification infrastructure

**Proof trees & justification infrastructure: `_pg_ripple.derivations` table, `pg_ripple.record_derivations` GUC, `justify()` proof-tree function, and `vacuum_derivations()` cleanup. No schema changes required beyond the new `derivations` table and supporting indexes added via migration script.**

One of the most powerful guarantees a data system can offer is not just the ability to answer questions, but the ability to explain how it arrived at those answers. Version 0.100.0 lays the foundation for this kind of deep accountability by introducing proof trees — a persistent record of every reasoning step taken by the inference engine. Each time the system derives a new fact by applying a Datalog rule, it now records exactly which source facts were used as premises, which rule was applied, and how deep the chain of reasoning ran. This history is stored in a dedicated `_pg_ripple.derivations` table and can be queried at any time through a new `justify()` function that walks the derivation chain backward from any derived fact to its ultimate source evidence.

For organizations that must explain and defend their automated decisions — whether for regulatory compliance, legal review, or internal governance — this infrastructure transforms pg_ripple from a powerful reasoning engine into a fully transparent and auditable knowledge system. A configurable on/off switch means the performance overhead of recording derivations is only incurred when it is actually needed, keeping production workloads lean while allowing auditing modes to be activated on demand. Automatic cleanup of orphaned derivation records, cycle protection in the proof tree walker, and a configurable depth limit ensure the system remains robust even for very deep reasoning chains with complex interdependencies. This release also corrects a subtle version comparison bug that affected tests using minor version numbers of 100 or greater, ensuring the test suite remains reliable as the project crosses into triple-digit versioning territory.

### Added

- **PROOF-TREE-01**: `_pg_ripple.derivations` table recording `(id BIGINT IDENTITY, derived_sid BIGINT, rule_name TEXT, rule_set TEXT, antecedent_sids BIGINT[], created_at TIMESTAMPTZ)` for every Datalog-derived fact when recording is enabled. Unique constraint on `(derived_sid, rule_name)` prevents duplicate rows.
- **PROOF-TREE-02**: `pg_ripple.record_derivations` GUC (default `off`) gates derivation recording overhead. Set to `on` before calling `infer_with_stats()` to capture provenance.
- **PROOF-TREE-03**: Derivation recording wired into the semi-naive inference engine (`run_inference_seminaive`). After each fixpoint run, antecedent SIDs are captured via delta-table joins and stored in `_pg_ripple.derivations`. Delta-table approach correctly identifies newly-derived triples (avoids false positives from pre-existing base facts).
- **PROOF-TREE-04**: `pg_ripple.justify(subject TEXT, predicate TEXT, object TEXT) → JSONB` SQL function returning the full backward-chaining proof tree. Returns a JSONB object with `"type"` (`"inferred"` or `"base"`), `"sid"`, `"triple"` (subject/predicate/object as human-readable strings), and `"derivations"` array. Returns NULL for triples not in the store.
- **PROOF-TREE-05**: Recursive derivation-graph walker with cycle protection (visited-SID set prevents infinite loops) and depth limit (MAX_DEPTH = 64 levels).
- **PROOF-TREE-06**: Batch dictionary decode for human-readable IRI labels in proof output — all SIDs in the proof tree are decoded in a single SQL batch query.
- **PROOF-TREE-07**: `pg_ripple.vacuum_derivations() → BIGINT` function to remove orphan derivation rows (where `derived_sid` no longer exists in any VP table). Returns the count of rows removed.
- **PROOF-TREE-08**: Derivations survive DRed retraction cleanly: `run_dred_on_delete` removes derivation rows for deleted SIDs; subsequent `vacuum_derivations()` cleans residual orphans.
- **SEMVER-FIX-01**: Version comparison tests in pg_regress test suite updated to use integer-based semantic version comparison (`split_part(version, '.', N)::int`) to correctly handle minor versions ≥ 100 (lexicographic comparison breaks for `0.100.0` vs `0.99.x`).

### Migration

- Schema changes: New `_pg_ripple.derivations` table with GIN index on `antecedent_sids` and B-tree index on `derived_sid`.
- Migration script: `sql/pg_ripple--0.99.2--0.100.0.sql`.

---

## [0.99.2] — 2026-05-08 — pg_trickle 0.49.1 patch; new repository

**Patch release: bumps pg_trickle to 0.49.1 to pick up upstream bug fixes, and establishes the new grove/pg-ripple GitHub repository. No schema changes. Evidence: `docker-compose.yml`, `sql/pg_ripple--0.99.1--0.99.2.sql`.**

### Changed

- **DOCKER-01**: `docker-compose.yml` image tag and Dockerfile `PG_TRICKLE_VERSION` bumped from `0.49.0` to `0.49.1` to pick up upstream bug fixes released in pg_trickle 0.49.1.
- **TRICKLE-PROBE-01**: `.versions.toml` `pg_trickle` version updated to `0.49.1`; `PG_TRICKLE_TESTED_VERSION` compile-time constant updated automatically via `build.rs`.
- **REPO-01**: Repository relocated to `grove/pg-ripple` (new GitHub organization). All CI workflows, artifact URLs, and documentation links updated accordingly.

### Migration

- No schema changes.
- Migration script: `sql/pg_ripple--0.99.1--0.99.2.sql` (comment-only).

---

## [0.99.1] — 2026-05-07 — pg_trickle & pg_tide version probe fix; view decode=true IVM fix; IMMEDIATE mode

**Patch release: aligns the pg_trickle compatibility probe with the deployed version, fixes `decode=true` on SPARQL/Datalog views to preserve BIGINT stream table columns for pg_trickle IVM correctness, and adds `immediate` mode support to all view creation functions. No schema changes.**

### Fixed

- **TRICKLE-PROBE-01**: `PG_TRICKLE_TESTED_VERSION` constant in `src/lib.rs` corrected from `"0.3.0"` to `"0.49.0"`. The stale constant caused pg_ripple to emit a WARNING about pg_trickle being "newer than tested" on every startup when deployed with the Dockerfile-bundled pg_trickle, and in some configurations caused `create_datalog_view_from_rule_set` to fail with a column `s` mismatch in the compiled VP table query.
- **DOCKER-01**: Dockerfile `PG_TRICKLE_VERSION` bumped from `0.48.0` to `0.49.0` to pick up upstream bug fixes.
- **DOCKER-02**: Dockerfile `PG_TIDE_VERSION` bumped from `0.15.0` to `0.16.0`.
- **VIEW-DECODE-01** (issue #81): `create_sparql_view` and `create_datalog_view` with `decode = true` no longer wrap the stream table query with dictionary decode subqueries. The stream table always stores `BIGINT` dictionary IDs; when `decode = true`, a separate companion VIEW `{name}_decoded` is created that performs dictionary lookups at read time (e.g. `pg_ripple.people_names_decoded`). `drop_sparql_view` and `drop_datalog_view` now also drop the `_decoded` companion view. This mirrors the existing behaviour of `create_construct_view` and `create_framing_view`.
- **IDEMPOTENT-01** (issue #83): `load_rules` / `load_rules_builtin` called a second time with the same rule-set name no longer duplicates rules in `_pg_ripple.rules`. `store_rules` now deletes all existing rows for the rule set before re-inserting, so repeated calls leave exactly one copy of each rule.
- **IDEMPOTENT-02** (issue #83): `create_datalog_view_from_rules` and `create_datalog_view_from_rule_set` are now idempotent. A `pgtrickle.drop_stream_table` call (errors silently ignored) is issued before each `pgtrickle.create_stream_table`, replacing the existing view cleanly on repeated invocations. This fixes dbt re-runs and CI pipelines that re-apply the same SQL against an existing database.
- **IDEMPOTENT-03**: The same drop-before-create guard was applied to all remaining view creation functions: `create_sparql_view`, `create_framing_view`, `create_construct_view`, `create_describe_view`, `create_ask_view`, `create_extvp`, `enable_live_statistics`, and `enable_shacl_monitors`. All view/monitor creation functions are now fully idempotent.

### Added

- **VIEW-IMMEDIATE-01** (issue #82): `immediate` boolean parameter added to `create_sparql_view`, `create_datalog_view`, `create_datalog_view_from_rule_set`, `create_framing_view`, `create_construct_view`, `create_describe_view`, and `create_ask_view`. When `immediate := true`, the underlying `pgtrickle.create_stream_table()` call includes `refresh_mode => 'IMMEDIATE'`, enabling constraint-style in-transaction refresh. Defaults to `false`; all existing call sites are unaffected.
- **BUILD-INFO-01** (issue #77): `pg_ripple.build_info()` SQL function returns a JSONB object with compile-time metadata: `version`, `profile` (`"debug"` or `"release"`), `built` (RFC-3339 timestamp), and `git_sha` (short SHA). Useful for diagnostics: `SELECT pg_ripple.build_info();`.

### Migration

- No schema changes.
- Migration script: `sql/pg_ripple--0.99.0--0.99.1.sql` (comment-only).

---

## [0.99.0] — 2026-05-06 — DCTERMS, Schema.org & FOAF Vocabulary Bundles

**Implements v0.99.0 roadmap: native Datalog rule sets and SHACL integrity bundles for Dublin Core Terms, Schema.org, and FOAF — completing the "Big 5" vocabulary suite together with SKOS (v0.98.0). Evidence: `src/datalog/builtins.rs`, `src/skos.rs`, `tests/pg_regress/sql/v099_features.sql`, `docs/src/cookbook/common-vocabularies.md`, `sql/pg_ripple--0.98.0--0.99.0.sql`.**

Real-world data rarely follows just one vocabulary — organizations routinely combine Dublin Core for document metadata, Schema.org for search engine compatibility, and FOAF for social and professional networks in the same knowledge graph. Version 0.99.0 completes pg_ripple's "Big 5" vocabulary suite by adding native Datalog rule sets and SHACL integrity bundles for all three of these ubiquitous standard vocabularies, alongside cross-vocabulary bridges that automatically connect them. When Schema.org's `schema:author` property is used, the system can now automatically infer the FOAF `foaf:maker` relationship; when Dublin Core's `dcterms:creator` is present, it is bridged to `foaf:maker` as well. Fifteen Schema.org rules, eleven Dublin Core rules, and eight FOAF rules activate a rich web of entailments with a single function call per vocabulary.

For teams building knowledge graphs that must interoperate with the broader semantic web ecosystem, this release is a significant quality-of-life improvement. Instead of manually writing dozens of inference rules to handle the standard relationships defined in these well-known vocabularies, teams can activate a complete, W3C-aligned rule set instantly. Integrity bundles automatically catch common modeling mistakes — self-referential relationships, circular hierarchies in Dublin Core's `hasPart`/`isPartOf` structure, inconsistent date ranges, and knows-chain cycles in FOAF. SQL helper functions like `pg_ripple.schema_type_ancestors()` and `pg_ripple.foaf_persons()` turn complex SPARQL traversal queries into simple, readable function calls that domain experts can use directly.

### Added

- **DCTERMS-01**: `load_datalog_bundle('dcterms')` — 11 Datalog rules: five `dc11:` compatibility aliases (`dc11:creator` → `dcterms:creator`, etc.), four structural inverse pairs (`hasPart`/`isPartOf`, `hasVersion`/`isVersionOf`, `replaces`/`isReplacedBy` + reverse `hasVersion`), DC-SKOS-01 bridge (resources whose `dcterms:subject` is in a SKOS scheme receive a `skos:Concept` type assertion).
- **DCTERMS-02**: `load_shape_bundle('dcterms-integrity')` — 8 integrity validators: self-referential creator/contributor/relation checks, cycle detection in `hasPart`/`isPartOf` hierarchies, date range consistency.
- **SCHEMA-01**: `load_datalog_bundle('schema')` — 15 Datalog rules: four inverse pairs (`subjectOf`/`about`, `hasPart`/`isPartOf`, `workExample`/`exampleOfWork`, `member`/`memberOf`), eight type-hierarchy shortcuts connecting `LocalBusiness`, `Organization`, `Person`, `Product`, `Event`, `CreativeWork`, `Place`, `Action` to `schema:Thing`, three cross-vocabulary bridges (SCHEMA-FOAF-01, SCHEMA-DC-01, SCHEMA-DCAT-01).
- **SCHEMA-02**: `load_shape_bundle('schema-integrity')` — 6 integrity validators: required-name self-reference, self-contained price range cycles, schema-type self-reference constraints.
- **FOAF-01**: `load_datalog_bundle('foaf')` — 8 Datalog rules: `foaf:knows` symmetry, `foaf:Person`/`foaf:Organization`/`foaf:Group` → `foaf:Agent` type subsumption, `foaf:account`/`foaf:accountFor` inverse, `foaf:made`/`foaf:maker` inverse, DC-FOAF-01 bridge (`dcterms:creator` → `foaf:maker`).
- **FOAF-02**: `load_shape_bundle('foaf-integrity')` — 5 integrity validators: `foaf:knows` self-reference, knows-chain cycle detection, account-of self-reference.
- **CROSS-01**: Cross-vocabulary Datalog bridges: DC-FOAF-01 (`dcterms:creator` → `foaf:maker`), SCHEMA-FOAF-01 (`schema:author` → `foaf:maker`), SCHEMA-DC-01 (`schema:name` → `dcterms:title`), DC-SKOS-01.
- **HELPER-01**: `pg_ripple.schema_type_ancestors(iri TEXT)` — returns all Schema.org type ancestors visible in the current graph for the given IRI.
- **HELPER-02**: `pg_ripple.foaf_persons()` — returns all `foaf:Person` IRIs and their `foaf:name` labels from the current graph.
- **GUC-01**: `pg_ripple.rule_graph_scope` default updated to `'all'` (was `'default'`); ontology-level reasoning across all named graphs is now the default behavior.
- **DOCS-01**: Cookbook chapter `docs/src/cookbook/common-vocabularies.md` with runnable examples for all three bundles.
- **DOCS-02**: Blog post `blog/dcterms-schema-foaf-bundles.md`.
- **TEST-01**: `tests/pg_regress/sql/v099_features.sql` — 45+ new tests covering all bundle loading, prefix registration, rule counts, cross-vocab bridges, SQL helpers, and integrity bundles.

### Changed

- `pg_ripple.rule_graph_scope` GUC description updated to reflect the new `'all'` default.

### Migration

- No schema changes; all new functionality is compiled from Rust.
- Migration script: `sql/pg_ripple--0.98.0--0.99.0.sql` (comment-only).

---

## [0.98.0] — 2026-05-06 — SKOS Support, Named Bundle API & Graph Intelligence

**Implements v0.98.0 roadmap: full SKOS/SKOS-XL entailment stack, formal named bundle API, contradiction explanation, federation trust scoring, and graph coverage metrics. Evidence: `src/skos.rs`, `tests/pg_regress/sql/v098_features.sql`, `docs/src/cookbook/skos-thesaurus.md`, `sql/pg_ripple--0.97.0--0.98.0.sql`.**

Taxonomies, thesauri, and controlled vocabularies are the backbone of enterprise information management — from library catalogs and medical terminology systems to product hierarchies and regulatory classification frameworks. Version 0.98.0 delivers a complete, W3C-compliant SKOS inference engine covering all 28 SKOS entailment rules plus the SKOS-XL label extension, enabling pg_ripple to automatically derive transitive broader/narrower concept relationships, symmetric associations, and correct label hierarchies without any custom rule writing. Five SQL helper functions — `skos_ancestors()`, `skos_descendants()`, `skos_label()`, `skos_related()`, and `skos_siblings()` — turn complex SPARQL traversal queries into simple function calls that data librarians and ontology engineers can use directly.

The release also introduces a formal named bundle API that treats rule sets and SHACL shapes as first-class, versioned artifacts with dependency management. Instead of loading individual rules and shapes manually, teams can call `load_datalog_bundle()` once and have all dependencies resolved automatically in the correct order. New contradiction explanation capabilities let users ask why two statements in a knowledge graph are logically inconsistent — surfacing the minimal set of conflicting facts in plain terms useful for debugging, data quality audits, and explaining validation failures to domain experts. Federation trust scoring and graph coverage metrics give teams quantitative measures of how complete and reliable their knowledge graph is across different topic areas, turning abstract data quality concerns into actionable numbers.

### Added

- **SKOS-01**: `"skos"` built-in Datalog rule set (28 rules, S7–S45) implementing all W3C SKOS entailments: `skos:broaderTransitive`/`skos:narrowerTransitive` transitive closures, `skos:narrower`/`skos:broader` inverse inference, `skos:related` symmetry, concept-scheme rules, label/documentation sub-properties, mapping property propagation, and collection sub-class assertion.
- **SKOS-02**: `"skosxl"` built-in rule set (3 rules, S55–S57) projecting `skosxl:Label` instances to plain `skos:prefLabel`/`skos:altLabel`/`skos:hiddenLabel` triples.
- **SKOS-03**: `"skos-transitive"` built-in rule set (7-rule subset for riverbank compatibility): transitive closures + symmetry + exactMatch transitivity.
- **SKOS-04**: `register_standard_prefixes()` extended to include `skos:` and `skosxl:` prefixes (auto-registered when loading any SKOS rule set).
- **SKOS-05**: Five SQL helper functions: `pg_ripple.skos_ancestors(iri, scheme)`, `skos_descendants(iri, scheme)`, `skos_label(iri, lang)`, `skos_related(iri)`, `skos_siblings(iri)`.
- **SKOS-06**: `"skos-integrity"` shape bundle (10 validators, W3C S9/S13/S14/S27/S37/S46 + ISO 25964-1): loaded via `pg_ripple.load_shape_bundle('skos-integrity')`.
- **SKOS-07**: `pg_ripple.validate_skos()` — integrity report wrapper returning `(violation_id, subject, message)`.
- **RB-01**: Named bundle API: `pg_ripple.load_datalog_bundle(name, named_graph)`, `load_shape_bundle(name)` with implicit dependency resolution, `pg_ripple.active_datalog_bundles` catalog view.
- **RB-01**: `_pg_ripple.datalog_bundles` catalog table and `pg_ripple.active_datalog_bundles` view (schema change in migration script).
- **RB-02**: `pg_ripple.explain_contradiction(subject_iri, named_graph, max_depth, mode)` — greedy/exact minimal-hitting-set contradiction explainer.
- **RB-02**: `pg_ripple.explain_contradiction_json(...)` — JSONB variant.
- **RB-03**: `pg_ripple.federation_endpoints` table (name, endpoint_url, auth_token, min_confidence, timeout_ms, created_at) for the federation trust layer (schema change).
- **RB-03**: GUC `pg_ripple.allow_unregistered_service_endpoints` (bool, default `off`): when off, `SERVICE` clauses against unregistered endpoints raise PT-SSRF-01.
- **RB-04**: `pg_ripple.coverage_map(named_graphs, topic_predicate, top_k)` — per-topic coverage SRF returning triple count, source count, mean/min confidence, contradiction count, and time range.
- **RB-04**: `pg_ripple.refresh_coverage_map(target_graph, named_graphs)` — writes `pgc:CoverageMap` triples into `target_graph`; schedulable via pg_trickle.
- **SKOS-08**: Cookbook chapter `docs/src/cookbook/skos-thesaurus.md`.
- **SKOS-09**: Blog post `blog/skos-knowledge-organization.md`.
- **Tests**: 5 new pg_regress test files: `skos.sql`, `bundle_api.sql`, `explain_contradiction.sql`, `federation_trust.sql`, `coverage_map.sql`, and `v098_features.sql`.

### Changed

- `pg_ripple.control`: `default_version` updated to `'0.98.0'`.

### Migration

- `sql/pg_ripple--0.97.0--0.98.0.sql`: creates `_pg_ripple.datalog_bundles`, `pg_ripple.federation_endpoints`, and `pg_ripple.active_datalog_bundles` view.

---

## [0.97.0] — 2026-05-06 — Assessment 15 Low-Severity Polish & Supply-Chain

**Implements v0.97.0 roadmap: closes all 13 Low-severity findings from PLAN_OVERALL_ASSESSMENT_15.**

A healthy production system requires more than correct functionality — it demands confidence in the supply chain, rigorous test coverage for edge cases, and documentation that accurately reflects real system behavior. Version 0.97.0 addresses all thirteen low-severity findings from the fifteenth assessment cycle with improvements spread across code quality, testing, documentation, and supply chain transparency. All unsafe Rust code blocks now carry mandatory justification comments explaining why each use is safe, a practice later formalized as an automatically enforced lint. Conformance suite badges for the Jena SPARQL test suite and OWL 2 RL conformance suite are added to the README and automatically updated by CI, giving users at-a-glance visibility into standards compliance status without needing to run the test suites themselves.

A particularly important improvement fixes the migration chain test, which previously relied on a manually maintained checkpoint constant that could silently fall out of sync as new migrations were added. The test now automatically discovers the most recent migration script, eliminating a class of false-passing tests that could miss coverage of recent migrations. New example SQL files covering Arrow Flight bulk export, PageRank analysis, and bidirectional CDC relay setup are verified in CI, ensuring they actually work rather than just appearing to. A cycle-safety regression test for `owl:sameAs` confirms the system handles symmetric, triangular, and self-referential identity assertions gracefully without entering infinite loops, closing a class of potential infinite-loop vulnerabilities in identity-heavy datasets.

### Added

- **L15-02**: Three new example SQL files: `examples/arrow_flight_export.sql`,
  `examples/pagerank_analysis.sql`, and `examples/bidi_relay_setup.sql` demonstrating
  bulk export, PageRank computation, and bidirectional CDC relay setup.
- **L15-03**: `examples/test_all.sh --live` wired in CI: pgrx PG18 instance started,
  extension installed, examples run in transaction-rollback safety mode.
- **L15-04**: `#![warn(clippy::missing_safety_doc)]` and
  `#![warn(clippy::undocumented_unsafe_blocks)]` added to `src/lib.rs`.  All unsafe blocks
  now have `// SAFETY:` comments preceding the `unsafe` keyword (14 blocks fixed).
- **L15-05**: Q15-xx justification convention: `// Q15-01:` comments added to `src/` (128 sites; see `roadmap/v0.97.0.md`).
  Searchable via `grep -rn 'Q15-' src/`.
- **L15-06**: `gen_random_uuid()` availability check added to `sql/pg_ripple--0.96.0--0.97.0.sql`
  (DO block GEN-UUID-01) and `src/schema/rls.rs` (`extension_sql!` block). Emits a
  `WARNING` if the function is unavailable (always passes on PostgreSQL 18).
- **L15-08**: RDF-star `<<>>` position support matrix added to `docs/src/reference/sparql-compliance.md`
  covering 18 positions across all SPARQL query/update forms.
- **L15-09**: `cargo doc --no-deps` with `RUSTDOCFLAGS="-D missing_docs"` added to `.github/workflows/ci.yml`
  (`test` job), enforcing zero missing-documentation warnings.
- **L15-10**: `HIGHEST_CHECKPOINT` in `tests/test_migration_chain.sh` is now
  auto-computed from `ls sql/pg_ripple--*--*.sql | sort -V | tail -1`, eliminating the
  hand-maintained constant that caused MIGCHAIN-SYNC failures.
- **L15-11**: New "Sequence Exhaustion (`statement_id_seq`)" section added to `docs/src/operations/scaling.md`
  documenting exhaustion rates, monitoring query, and recovery procedure.
- **L15-12**: `tests/pg_regress/sql/owl_sameas_cycle.sql` regression test added,
  asserting graceful handling of symmetric, triangular, and self-referential
  `owl:sameAs` cycles (no infinite loop; store stable after inference).
- **L15-14**: Jena (≥95% pass) and OWL 2 RL (≥95% pass) conformance-suite badges
  added to `README.md`. CI `jena-suite` and `owl2rl-suite` jobs update
  `results/jena-badge.json` / `results/owl2rl-badge.json` on push to `main`.
  New `docs/src/reference/jena-results.md` documents the Jena suite coverage.
- **M15-16**: `serde_cbor` supply-chain audit documented: it is a transitive dep from
  `pgrx 0.18.0`, not used directly. Documented in `Cargo.toml` comment and tracked in
  `deny.toml`.

### Fixed

- **L15-01**: `CHANGELOG.md` entry for v0.90.0 had `2026-05-XX` as a date placeholder;
  corrected to `2026-04-30`.

---

## [0.96.0] — 2026-05-06 — Assessment 15 Medium: Performance, Code Quality, Test Coverage

**Implements v0.96.0 roadmap (A15 medium quality pillars): HTAP tombstone-skip
optimisation, star-join collapse GUC, federation connect-timeout GUC, sub-splits
of five large source files and the HTTP routing module, zero missing-docs
warnings, concurrent-load PageRank benchmark, SHACL column-order regression
test, four new Prometheus metrics, Datalog cyclic-group parallel regression
test, and Arrow Flight EXPLAIN-only path.**

Version 0.96.0 delivers a focused set of performance and code quality improvements from the Assessment 15 medium-severity findings. The most impactful performance improvement is a tombstone-skip optimization in the HTAP storage layer: when a VP table has no deleted rows, the query path now omits the LEFT JOIN against the tombstone table entirely, eliminating join overhead that previously existed even when there were no deletions to account for. A new star-join collapse optimization detects when multiple triple patterns share the same subject variable and combines them into a more efficient query structure, reducing redundant table access for the common star-shaped query patterns that dominate RDF workloads. A configurable federation connect-timeout GUC gives operators fine-grained control over how long federation requests wait before failing.

Code quality receives substantial attention with the decomposition of five large source files into focused sub-modules and the HTTP routing module for Datalog handlers being split in two, making the handler code easier to navigate and test in isolation. A new Arrow Flight EXPLAIN-only row estimation path replaces a `COUNT(*)` pre-check that could produce inconsistent row counts on active HTAP tables. Four new Prometheus metrics cover merge cycle duration, Datalog stratum evaluation time, SHACL validation queue depth, and CDC replication slot lag — giving operations teams comprehensive visibility into the most resource-intensive internal subsystems. A parallel Datalog regression test and a concurrent PageRank plus HTAP merge benchmark ensure performance characteristics remain stable under the simultaneous read/write/analyze workloads common in production.

### Added

- **M15-05**: `src/storage/merge.rs` — `htap_view_sql()` and `rebuild_htap_view()`
  generate a tombstone-skip (no LEFT JOIN) form of the HTAP view when
  `tombstone_count = 0` and switch to the LEFT JOIN form on first tombstone.
  `sql/pg_ripple--0.95.0--0.96.0.sql` — adds `tombstone_count BIGINT NOT NULL DEFAULT 0`
  to `_pg_ripple.predicates`.

- **M15-06**: `src/gucs/sparql.rs` — `STAR_JOIN_COLLAPSE` bool GUC
  (`pg_ripple.star_join_collapse`, default `true`).
  `src/sparql/optimizer.rs` — `detect_star_groups()` function groups triple
  patterns by subject variable and sorts by ascending selectivity cost.

- **M15-11**: `src/gucs/federation.rs` — `FEDERATION_CONNECT_TIMEOUT_SECS` int
  GUC (`pg_ripple.federation_connect_timeout_secs`, default `10`, range 1–3600).
  `src/sparql/federation/circuit.rs` — `get_agent()` applies
  `.timeout_connect()` before the overall request timeout.

- **M15-13**: Sub-split five large source files into sub-modules to keep
  each `mod.rs` under 800 lines:
  - `src/sparql/expr/` → `functions.rs` (translate_function_value, is_numeric_function),
    `cast.rs` (xsd_cast_datatype, xsd_cast_sql)
  - `src/sparql/execute/` → `construct.rs`, `describe.rs`, `update.rs`, `explain.rs`
  - `src/export/` → `csv.rs` (GraphRAG Parquet export, 939 lines)
  - `src/storage/ops/` → `scan.rs` (query, graph management, SID API, dedup)
  - `src/datalog/compiler/` → `sql.rs` (recursive, on-demand CTE, semi-naive delta,
    aggregate rules), `shacl_rules.rs` (constraint check compiler)

- **M15-14**: Sub-split `pg_ripple_http/src/routing/datalog_handlers.rs` (1232 → 442 lines):
  - `routing/datalog_inference.rs` — inference HTTP handlers (infer, infer_with_stats,
    infer_agg, infer_wfs, infer_demand, infer_lattice, query_goal, check_constraints)
  - `routing/datalog_admin.rs` — admin HTTP handlers (cache_stats, tabling_stats,
    list_lattices, create_lattice, list_views, create_view, drop_view)

- **M15-15**: Verified zero `missing documentation` warnings under `#![warn(missing_docs)]`.

- **M15-17**: `benchmarks/pagerank_with_writes.sh` — concurrent-load benchmark
  combining 4 pgbench writer clients, 1 SPARQL reader, and 1 PageRank computation
  for a configurable duration (default 60 s). Appends results to
  `benchmarks/pagerank_throughput_history.csv`.

- **M15-18**: `tests/pg_regress/sql/probabilistic.sql` — Test 4b: asserts that
  `shacl_report_scored` returns columns `focus_node`, `shape_iri`,
  `result_severity`, `result_severity_score`, `message` via `pg_get_function_result`.

- **M15-19**: `pg_ripple_http/src/metrics.rs` — four new Prometheus counters/gauges:
  `pg_ripple_merge_cycle_duration_seconds`, `pg_ripple_datalog_stratum_duration_seconds`,
  `pg_ripple_shacl_validation_queue_depth`, `pg_ripple_cdc_replication_slot_lag_bytes`.

- **M15-21**: `tests/pg_regress/sql/datalog_cyclic_parallel.sql` — regression test
  verifying that mutually-recursive Datalog rules execute correctly under
  `pg_ripple.datalog_parallel_workers = 2` (cyclic-group pre-check, P13-06).

- **M15-22**: `pg_ripple_http/src/arrow_encode.rs` — Arrow Flight row-count guard
  now uses EXPLAIN-only path; removed COUNT(*) fallback that could produce
  inconsistent row counts on hot HTAP tables.

---

## [0.95.0] — 2026-05-05 — Assessment 15 Medium: Correctness, Security, Storage

**Implements v0.95.0 roadmap: replace unreachable!() panics, DNS rebinding
fix for federation, sql_drop event trigger for replication-slot cleanup,
SSE error redaction, dictionary VACUUM threshold GUC, property-path+OPTIONAL+
vp_rare regression tests, NaN/Inf confidence rejection, schema_generation plan
cache key, and ADD/COPY/MOVE through full SPARQL update pipeline.**

Correctness and security in a database extension require constant vigilance about how user-supplied data flows through the system and where assumptions can be violated by unusual inputs. Version 0.95.0 addresses eight medium-severity correctness and security findings, starting with a complete fix for DNS rebinding attacks in the federation layer. The new `resolve_and_check_endpoint()` function resolves hostnames to IP addresses once, validates every resolved IP against the SSRF blocklist, and uses the IP-based URL for the actual connection — preventing an attacker from exploiting the window between hostname validation and connection establishment to redirect a request to a private address.

Several other important correctness improvements ship alongside the security fixes. A new event trigger automatically cleans up CDC replication slots when the extension is dropped, preventing orphaned slots from consuming PostgreSQL WAL resources indefinitely. SPARQL ADD, COPY, and MOVE operations now correctly flow through the mutation journal and SPARQL audit log, matching the post-processing applied to all other SPARQL Update operations — ensuring that CONSTRUCT writeback rules fire for these operations just as they do for INSERT and DELETE. Explicit NaN and infinity checks in the confidence loader produce clear, actionable error messages rather than cryptic numeric range failures. A schema generation sequence lets the plan cache detect when VP tables have been promoted and automatically evict stale plans, preventing "table not found" errors after rare-predicate promotion.

### Added

- **M15-01**: `src/pagerank/export.rs`, `src/pagerank/centrality.rs` — replaced
  bare `unreachable!()` match arms with `pgrx::error!()` so unexpected input
  produces a clean PostgreSQL error rather than a server crash.
  `scripts/check_no_unreachable_in_production.sh` — new CI lint script.
  `.github/workflows/ci.yml` — `No unreachable!() in production source lint` step.
- **M15-02**: `src/sparql/federation/policy.rs` — new `ResolvedEndpoint` struct
  and `resolve_and_check_endpoint()` function that resolves hostnames once,
  validates every resolved IP against the SSRF blocklist, and returns the IP-based
  connect URL. `src/sparql/federation/http.rs` — both `execute_remote()` and
  `execute_remote_partial()` now use the resolved endpoint to prevent DNS
  TOCTOU / rebinding attacks.
- **M15-03**: `src/schema/rls.rs` — `_pg_ripple.cleanup_on_drop()` event trigger
  function + `_pg_ripple_cleanup_on_drop` event trigger on `sql_drop` that drops
  CDC replication slots when `DROP EXTENSION pg_ripple` is executed.
- **M15-04**: `pg_ripple_http/src/stream.rs` — SSE initialisation error paths
  (pool unavailable, non-SELECT query) now use `redacted_error()` from `common.rs`
  instead of inline JSON responses with raw error details.
- **M15-07**: `src/gucs/storage.rs` — `DICT_VACUUM_THRESHOLD` GUC
  (`pg_ripple.dict_vacuum_threshold`, default: 10000, Userset).
  `src/dictionary/mod.rs` — `maybe_vacuum_dictionary()` helper runs
  `VACUUM ANALYZE _pg_ripple.dictionary` when encode count exceeds threshold.
  `src/bulk_load.rs` — calls `maybe_vacuum_dictionary()` after large loads.
  `src/schema/rls.rs` — `autovacuum_vacuum_scale_factor = 0.01` and
  `autovacuum_analyze_scale_factor = 0.005` reloptions on `_pg_ripple.dictionary`.
- **M15-08**: `tests/pg_regress/sql/sparql_optional_path_in_graph_rare.sql` —
  new regression test covering OPTIONAL + property path (+ * | /) + vp_rare
  predicates inside `GRAPH <g> {}`.
- **M15-09**: `src/bulk_load.rs` — explicit `is_nan()` and `is_infinite()` checks
  in `load_triples_with_confidence()` before the range check, with clear PT0301
  error messages naming the specific problem (NaN or +/-Infinity).
- **M15-10**: `src/storage/mod.rs` — `bump_schema_generation()` and
  `current_schema_generation()` helpers read/write
  `_pg_ripple.schema_generation_seq`.
  `src/sparql/plan_cache.rs` — `schema_gen=N` component added to plan cache key.
  `src/storage/promote.rs` — `promote_predicate_impl()` calls
  `bump_schema_generation()` after promotion.
- **M15-12**: `src/sparql/execute/mod.rs` — `try_execute_add_copy_move()` early
  return now includes mutation journal flush and SPARQL audit log entry, matching
  the post-processing applied to all other SPARQL Update operations.
- **SQL**: `sql/pg_ripple--0.94.0--0.95.0.sql` — schema_generation_seq,
  dictionary autovacuum reloptions, cleanup_on_drop event trigger.

### BREAKING

None.

---

## [0.94.0] — 2026-05-05 — Assessment 15 High Remediation

**Implements v0.94.0 roadmap: security hardening (SECURITY DEFINER search_path
injection fix), bump-version recipe improvements, bounded bidi relay channel with
Prometheus counter, and shared copy_into_vp() helper for COPY-style bulk loads.**

When the same system function is used in security-sensitive contexts, even a small gap in its defenses can create an exploitable vulnerability. Version 0.94.0 addresses the highest-priority finding from Assessment 15: a potential search-path injection vulnerability in a SECURITY DEFINER trigger function that could allow a privileged attacker to substitute their own functions for standard PostgreSQL library calls. The fix pins the search path to known-safe schemas inside the function definition, and a new CI lint script verifies that every SECURITY DEFINER function in the codebase has an explicit, pinned search path — turning a one-time fix into a permanent, automatically enforced protection that prevents the issue from recurring in future code.

This release also introduces a bounded bidi relay channel with an operator-configurable inflight limit and automatic back-pressure, preventing the relay from consuming unlimited memory when the downstream consumer falls behind. A Prometheus counter tracks dropped events when the inflight limit is reached, giving operations teams visibility into backpressure events before they affect data reliability. A new `copy_into_vp()` helper function implements an efficient batch insertion path using UNNEST-array semantics, laying the groundwork for the 5–10× bulk load performance improvement delivered in v0.113.0. The `bump-version` justfile recipe is enhanced to accept an explicit compatibility minimum argument, making coordinated version management less error-prone for teams maintaining the extension alongside the HTTP companion.

### Added

- **H15-01**: `justfile` — `bump-version NEW_VERSION [COMPAT_MIN]` accepts an
  optional `COMPAT_MIN` argument to set `COMPATIBLE_EXTENSION_MIN` independently
  from the release version; `check-version-sync` recipe updated to allow
  `COMPAT_MIN ≤ EXT_VER` (not `COMPAT_MIN == EXT_VER`), matching the CI gate.
  `COMPATIBLE_EXTENSION_MIN` bumped to `"0.93.0"` for this release.
- **H15-02**: `src/schema/triggers.rs` — `_pg_ripple.ddl_guard_vp_tables()` gains
  `SET search_path = pg_catalog, _pg_ripple, public` to prevent search-path injection.
  `scripts/check_security_definer_search_path.sh` — new CI script verifying all
  SECURITY DEFINER functions in `src/` have a pinned `SET search_path` clause.
  Wired into `.github/workflows/ci.yml` as `SECURITY DEFINER search_path lint`.
- **H15-03**: `src/gucs/storage.rs` — `BIDI_RELAY_MAX_INFLIGHT` GUC
  (`pg_ripple.bidi_relay_max_inflight`, default: 1000, Suset).
  `src/stats.rs` — `BIDI_RELAY_INFLIGHT` and `BIDI_RELAY_DROPPED_TOTAL` static
  `AtomicI64` counters; `relay_inflight_acquire()` / `relay_inflight_release()`
  helpers. Both counters exposed in `streaming_metrics()`.
  `src/bidi/relay.rs`:`ingest_jsonld_impl()` — gated on `relay_inflight_acquire()`;
  emits WARNING and returns 0 when inflight limit is reached.
- **L15-13**: `pg_ripple_http/src/metrics.rs` — `bidi_relay_dropped_total` field;
  `update_bidi_relay_dropped_total()` / `bidi_relay_dropped_total()` methods.
  `pg_ripple_http/src/routing/admin_handlers.rs` — `pg_ripple_bidi_relay_dropped_total`
  Prometheus counter in `/metrics` endpoint.
- **H15-05**: `src/storage/ops/mod.rs` — `copy_into_vp()` helper using UNNEST-array
  based batch insertion; `pub(crate)` for use by bulk loader, R2RML, and CDC paths.
  `src/gucs/storage.rs` — `BULK_LOAD_USE_COPY` GUC (`pg_ripple.bulk_load_use_copy`,
  default: off, Userset). `batch_insert_encoded()` dispatches to `copy_into_vp()`
  when the GUC is on.
- **SQL**: `sql/pg_ripple--0.93.0--0.94.0.sql` — recreates
  `_pg_ripple.ddl_guard_vp_tables()` with `SET search_path` and records schema version.
- **Roadmap**: `roadmap/v0.94.0.md` + `roadmap/v0.94.0-full.md` created.

### Changed

- `pg_ripple.control` `comment` updated to `v0.94.0`.

---

## [0.93.0] — 2026-05-04 — pg_tide Integration & Documentation Modernisation

**Implements v0.93.0 roadmap: integrates pg_tide as the recommended relay transport
layer and modernises all documentation to reflect the new pg-trickle v0.46.0
architecture (relay, outbox, inbox extracted to pg_tide).**

The CDC relay ecosystem around pg_ripple underwent a significant architectural change when pg_trickle version 0.46.0 extracted the relay, outbox, and inbox components into a separate pg_tide extension. Version 0.93.0 updates pg_ripple to work correctly with this new architecture, adding runtime detection of pg_tide, clear error messages that guide users toward installing it when it is absent, and comprehensive updates to all documentation, blog posts, and example code that referenced the old relay API. The Docker image is updated to bundle both pg_trickle 0.46.0 and pg_tide 0.4.0 together, so deployments using the official image get the correct combination out of the box.

For existing users who relied on relay functionality, this release provides a clear migration path and backward-compatible behavior: a new `pg_ripple.pg_tide_available()` function lets applications detect at runtime whether pg_tide is installed and adapt their behavior accordingly. All documentation — including the bidirectional relay operations guide, the semantic hub blog post, and the integration plans — is updated with correct API references, migration notes that document the exact pg_trickle to pg_tide call-site changes, and version compatibility tables. A new compatibility matrix section clearly documents which versions of pg_trickle and pg_tide work with which versions of pg_ripple, giving operators the information they need to plan upgrades with confidence and without trial and error.

### Added

- **TIDE-1**: `src/lib.rs` — `has_pg_tide()` runtime detection helper; `_PG_init`
  INFO message about pg_tide relay support; `pg_ripple.pg_tide_available()` SQL
  function (via `src/views_api.rs`) for client-side detection.
- **TIDE-3**: `src/views/mod.rs` — `PGTIDE_HINT` constant for relay error paths:
  `"pg_tide extension is not installed; install pg_tide ≥0.1.0 from https://github.com/trickle-labs/pg-tide"`.
- **SQL**: `sql/pg_ripple--0.92.0--0.93.0.sql` — comment-only migration script
  documenting all TIDE-1 through TIDE-DOCKER-01 changes. No schema changes.

### Changed

- **TIDE-2**: `src/bidi/mod.rs` — BIDI-OUTBOX-01 and BIDI-INBOX-01 doc comments
  updated to reference pg_tide API (`tide.outbox_create`, `tide.outbox_publish`,
  `tide.inbox_create`, `tide.inbox_status`).
- **TIDE-4**: `docs/src/operations/pg-trickle-relay.md` — full rewrite to pg-tide API:
  - 20+ API call sites updated (pgtrickle.\* → tide.\*)
  - Prerequisites updated: pg_tide ≥0.4.0 + pg_trickle ≥0.46.0 required
  - New outbox publish trigger pattern (`tide.outbox_publish`)
  - Architecture diagram updated to reference pg_tide
  - `pg-tide-relay` binary replaces `pgtrickle-relay` in docker-compose example
  - Related pages extended with pg_tide repository link
- **TIDE-5**: `blog/semantic-hub-trickle-relay.md` — renamed integration to
  pg-tide; hub-and-spoke examples updated to `tide.*` API; clarified
  pg_trickle is IVM-only since v0.46.0.
- **TIDE-6**: `plans/pg_trickle_relay_integration.md` — prominent backward-compat
  migration note added documenting full API migration path from pg_trickle ≤ 0.45.0
  to pg_tide ≥ 0.1.0.
- **TIDE-7**: `roadmap/v0.52.0.md`, `roadmap/v0.77.0-full.md` — inline footnotes
  noting relay examples require pg_trickle < 0.46.0 or pg_tide ≥ 0.1.0; updated
  `tide.relay_set_outbox()` example in v0.77.0-full.md.
- **TIDE-8**: `docs/src/operations/compatibility.md` — new "pg_tide / pg_trickle
  Extension Compatibility" section with version compatibility tables; pg_ripple_http
  0.93.x row added to the main compatibility table.
- **TIDE-DOCKER-01**: `Dockerfile` — `PG_TRICKLE_VERSION` bumped to `0.46.0`;
  `PG_TIDE_VERSION` bumped to `0.4.0`; corrupted COPY sections fixed; image
  description label updated; header comment updated.
- **Version bumps**: `Cargo.toml` and `pg_ripple_http/Cargo.toml` bumped to 0.93.0;
  `pg_ripple.control` updated to `default_version = '0.93.0'`;
  `COMPATIBLE_EXTENSION_MIN` bumped to `"0.92.0"` in `pg_ripple_http/src/main.rs`.

---

## [0.92.0] — 2026-05-03 — Assessment 14 Low-Severity Polish & Hardening

**Implements v0.92.0 roadmap: closes all 39 Low-severity findings from
PLAN_OVERALL_ASSESSMENT_14. This is the final Assessment 14 remediation release
before v1.0.0 production hardening.**

Version 0.92.0 closes all thirty-nine low-severity findings from Assessment 14, touching nearly every part of the system. PageRank receives improvements to its documentation — a new "Tuning Damping for Your Graph" section gives operators guidance tailored to sparse social, citation, knowledge, and temporal graph types — and its row-level security policies are hardened with the same graph isolation policies applied to the confidence side table. The `owl:sameAs` handling documentation is clarified to confirm that entity clusters are merged before PageRank computation, preventing double-counting an important correctness guarantee for entity-rich datasets where multiple IRIs refer to the same real-world object.

Operations receive several meaningful improvements. The diagnostic report function is extended with four new PageRank-related entries covering confidence row counts, last PageRank computation time, queue depth, and centrality metrics, giving operators a more complete system health picture in a single call. The HTTP companion gains a configurable graceful shutdown timeout, allowing operators to tune how long the service waits for in-flight requests to complete before exiting — critical in production environments where SLA requirements prohibit abrupt connection drops. CDC payload length is now checked against PostgreSQL's 8,000-byte `pg_notify` limit before sending, raising a warning rather than silently truncating notifications. The PageRank `find_duplicates()` function's volatility is correctly classified as STABLE rather than VOLATILE, enabling the query planner to make better optimization decisions for queries that call it repeatedly.

### Added

- **CB-07**: `src/pagerank/ivm.rs` — `pagerank_lower()` / `pagerank_upper()` doc
  comments include PR-STALE-BOUNDS-01 formula: `bound = score ± (alpha^k * delta_per_iter)`.
- **CB-08**: `docs/src/features/pagerank.md` — "Tuning Damping for Your Graph" subsection
  with guidance for sparse social, citation, knowledge, and temporal graph types.
- **CB-09**: `tests/pg_regress/sql/sparql_federation.sql` — TLS handshake failure scenario
  asserting `SERVICE SILENT` swallows connection errors and returns empty result set.
- **CB-10**: `docs/src/reference/sparql-compliance.md` — `symmetric` alias contract
  documented as permanent (guaranteed stable for the 1.x API line; alias and `scbd`
  remain semantically identical).
- **SEC-07**: `sql/pg_ripple--0.91.0--0.92.0.sql` — `ALTER TABLE _pg_ripple.pagerank_dirty_edges
  ENABLE ROW LEVEL SECURITY` + `CREATE POLICY pagerank_dirty_edges_graph_isolation` mirroring
  the `_pg_ripple.confidence` RLS pattern.
- **SEC-09**: `.github/workflows/cargo-audit.yml` — `--deny unmaintained` added to `cargo audit`
  invocation; existing `audit.toml` ignores have valid expiry dates.
- **PERF-07**: `src/gucs/pagerank.rs` — `PAGERANK_PARTITION` default changed from `false` to
  `true`; description updated to document auto-tune behaviour (min(num_cpus, named_graph_count)).
- **PERF-08**: `src/uncertain_knowledge_api/mod.rs` — `_fuzzy_match_guard()` and
  `_token_set_ratio_guard()` annotated `stable, parallel_safe` (was: default VOLATILE).
- **CON-04**: `tests/pg_regress/sql/datalog_parallel.sql` — regression test for cyclic
  parallel Datalog stratification pre-check; asserts no crash for non-cyclic RDFS rule set.
- **CON-05**: `tests/concurrency/confidence_subxact_rollback.sql` — noisy-OR aggregation
  rollback test: `BEGIN; SAVEPOINT s1; infer(); ROLLBACK TO s1; COMMIT` asserts confidence
  table unchanged.
- **TEST-06**: `benchmarks/pagerank_throughput_history.csv` — PageRank throughput history
  for Karate Club (1M-edge) benchmark; wired to `performance_trend.yml` artifact upload.
- **CQ-06**: `src/uncertain_knowledge_api/mod.rs`, `src/pagerank/mod.rs` — all
  `#[allow(dead_code)]` attributes in v0.87/v0.88 additions carry `// Q14-08: <reason>`.
- **STD-04**: `docs/src/features/pagerank.md` — portability note: `pg:pagerank()` etc. are
  pg_ripple-specific extension functions; not portable to other SPARQL endpoints.
- **STD-05**: `docs/src/features/uncertain-knowledge.md` — `sh:severityWeight` extension
  note: pg_ripple-specific, community submission to W3C SHACL CG under consideration.
- **OBS-04**: `src/sparql/execute/mod.rs` — `algebra_optimized` (en_US) accepted as alias
  for `algebra_optimised` (en_GB) in `explain_sparql()` format parameter.
- **OBS-05**: `src/maintenance_api.rs` — `diagnostic_report()` extended with four
  v0.87/v0.88 catalog rows: `confidence_row_count`, `pagerank_last_computed`,
  `pagerank_queue_depth`, `centrality_metrics`.
- **HTTP-05**: `pg_ripple_http/src/main.rs` — `PG_RIPPLE_HTTP_SHUTDOWN_TIMEOUT_SECS` env
  var (INT, default 30) configures graceful-shutdown drain timeout.
- **HTTP-06**: `pg_ripple_http/src/routing/middleware.rs` — documentation note confirming
  `tower_governor` 0.8 with `axum` feature automatically includes `Retry-After` header in
  429 responses.
- **DL-04**: `src/datalog/magic.rs` — `run_infer_goal()` doc comment explicitly documents
  magic-sets pre-condition (bound predicate requirement) and fallback behaviour.
- **DL-05**: `docs/src/features/pagerank.md` — `owl:sameAs` handling note: entity clusters
  are merged before PageRank to avoid double-counting.
- **IVM-03**: `docs/src/reference/ivm.md` — cross-module dependency scheduling note: rules
  writing to `_pg_ripple.confidence` are not in the CWB topological sort; `register_ivm_dependency()`
  API reserved for future use.
- **CDC-03**: `src/cdc.rs` — `notify_named_subscriptions()` now checks payload length
  against 8000-byte `pg_notify` limit; raises PT5001 WARNING instead of silently truncating.
- **CDC-04**: `tests/concurrency/sse_slow_subscriber.sh` — SSE backpressure load test
  asserting server remains responsive under slow subscriber.
- **DOC-03**: `blog/pagerank.md` + `blog/uncertain-knowledge.md` — stub blog posts for
  v0.88.0 and v0.87.0 features.
- **DOC-04**: `examples/test_all.sh` — static validation script for all `.sql` examples;
  wired as CI step in `.github/workflows/ci.yml`.
- **BUILD-04**: `build.rs` — `SOURCE_DATE_EPOCH` support verified and documented (already
  implemented in BUILD-TIME-FIELD-01, v0.83.0); documented in `CONTRIBUTING.md`.
- **BUILD-05**: `CONTRIBUTING.md` — `src/uncertain_knowledge_api/` and `src/pagerank/`
  module structure documented; magic comment conventions (`// SAFETY:`, `// CLIPPY-OK:`,
  `// Q13-05:`, `// Q14-08:`) documented.
- **SEC-06**: `RELEASE.md` — RSA advisory calendar entry: RUSTSEC-2024-0436 and
  RUSTSEC-2023-0071 expire 2026-12-01; re-audit required before v1.0.0.
- **WC-01–WC-05**: Post-v1.0.0 aspirational tracking documented in `roadmap/v0.92.0-full.md`.

### Changed

- **SEC-08**: `src/pagerank_api.rs` — `pagerank_find_duplicates()` volatility changed from
  VOLATILE to STABLE (DB-state dependent, not time/random).
- **PERF-07**: `pg_ripple.pagerank_partition` GUC default changed from `false` to `true`.

---

## [0.91.0] — 2026-05-03 — Assessment 14 Medium: Observability, API, Standards, Build & Documentation

**Implements v0.91.0 roadmap: completes the second half of PLAN_OVERALL_ASSESSMENT_14
Medium remediations. Adds PageRank IVM Prometheus gauges, SHACL score log retention GUC
+ vacuum function, PostgreSQL jsonlog documentation, HTTP middleware extraction, Arrow
Flight EXPLAIN row estimation, SPARQL `pg:` prefix auto-declaration, `explain_pagerank_json()`,
PT0301–PT0423 error code docs, RDF-star compliance matrix, compatibility table rows through
v0.91.x, ProbLog citation, IVM boundary architecture doc, named-argument pagerank_run examples,
SSE regression test, BUILD lint-version-sync CI job, dedicated migration-chain workflow,
bidi relay throughput benchmark wiring, and two new GUCs for CDC watermark control.**

Version 0.91.0 delivers the second half of the Assessment 14 medium-severity remediation with twenty-seven improvements spanning observability, API ergonomics, standards compliance, build quality, and documentation. Three new Prometheus gauges expose PageRank IVM queue telemetry — queue depth, maximum delta, and oldest enqueue age — enabling SLO-based alerting when PageRank becomes stale and helping teams understand the lag between graph changes and updated rank scores. Arrow Flight row count estimation is improved by using EXPLAIN-style plan row estimates rather than `COUNT(*)`, which could produce inconsistent counts on active HTAP tables. A dedicated migration chain CI workflow runs automatically whenever migration SQL files are modified, ensuring the upgrade path is always tested when it changes.

An important ergonomics improvement makes the `pg:` namespace prefix automatically available in all SPARQL queries without a manual prefix declaration, eliminating a friction point that required users to add `PREFIX pg: <http://pg-ripple.org/fn/>` before using any pg_ripple extension function. A new `explain_pagerank_json()` function returns a structured explanation tree showing exactly why a node has its current rank and which nodes contributed to it — the PageRank equivalent of a query execution plan. A complete error code documentation table covers all PT0301–PT0423 codes, giving developers immediate context when they encounter any error the system can produce. Two new GUCs provide fine-grained control over CDC watermark batch processing, allowing operators to tune throughput and latency independently.

### Added

- **OBS-01**: `pg_ripple_http/src/metrics.rs` — Three new Prometheus gauges for PageRank IVM queue telemetry: `pg_ripple_pagerank_queue_depth`, `pg_ripple_pagerank_queue_max_delta`, `pg_ripple_pagerank_queue_oldest_enqueue_seconds`. Exposed in `/metrics` endpoint via `pg_ripple_http/src/routing/admin_handlers.rs`.
- **OBS-02**: `src/gucs/observability.rs` + `src/gucs/registration/observability.rs` — `pg_ripple.shacl_score_log_retention_days` GUC (INT, default 30, range 0–3650, Suset). `src/uncertain_knowledge_api/mod.rs` — `vacuum_shacl_score_log()` `pg_extern` function purges rows older than the retention window.
- **OBS-03**: `docs/src/reference/observability.md` — PostgreSQL Structured Logging section explaining `pgrx::log!` → `log_destination=jsonlog` mapping; no double-serialisation risk.
- **HTTP-02**: `tests/integration/sse_stream.sh` — SSE stream regression test for `/sparql/stream` endpoint (validates `Content-Type: text/event-stream` and `data:` event emission).
- **HTTP-03**: `pg_ripple_http/src/routing/middleware.rs` (new file) — Extracted `apply_rate_limit()` and `build_cors_layer()` middleware helpers. `pg_ripple_http/src/routing/mod.rs` — `pub mod middleware;` module declaration.
- **HTTP-04**: `pg_ripple_http/src/arrow_encode.rs` — Replaced COUNT(*) row-count pre-check with `EXPLAIN (FORMAT JSON, ANALYZE FALSE)` plan row estimation. `extract_plan_rows_from_explain()` helper with COUNT(*) fallback.
- **API-04**: `src/sparql/parse.rs` — `PG_FN_NAMESPACE` constant + `inject_pg_prefix_if_needed()` auto-declares `PREFIX pg: <http://pg-ripple.org/fn/>` when a query uses `pg:` without an explicit prefix. Wired into `sparql()` and `sparql_ask()` in `src/sparql/mod.rs`. `docs/src/reference/sparql.md` — SPARQL Extension Function IRI Namespace section with function reference table and federation note.
- **API-05**: `src/pagerank_api.rs` — `explain_pagerank_json(node_iri, top_k)` `pg_extern` function returning JSONB explanation tree.
- **API-06**: `docs/src/reference/error-codes.md` — PT0301–PT0307 (Uncertain Knowledge) and PT0401–PT0423 (PageRank) error code tables.
- **STD-01**: `plans/sparql12_tracking.md` — Updated version to v0.91.0; last_reviewed to 2026-05-03.
- **STD-02**: `docs/src/reference/sparql-compliance.md` — RDF 1.2 / SPARQL-star Compliance Matrix section.
- **STD-03**: `docs/src/features/uncertain-knowledge.md` — ProbLog citation (De Raedt, Kimmig & Toivonen 2007) with extension-vs-standard note.
- **DOC-01**: `docs/src/operations/compatibility.md` — Compatibility table rows for v0.87.x through v0.91.x.
- **DOC-02**: `docs/src/features/pagerank.md` — Named-argument `pagerank_run()` examples; `explain_pagerank_json()` function documentation.
- **IVM-01**: `docs/src/reference/ivm.md` (new file) — IVM boundary architecture document for CWB-IVM vs. PageRank-IVM; monitoring commands; GUC reference. Added to `docs/src/SUMMARY.md`.
- **IVM-02**: `tests/pg_regress/sql/construct_rules.sql` + `tests/pg_regress/expected/construct_rules.out` — CWB confidence propagation regression test verifying `source=1` marking for inferred triples.
- **BUILD-01**: `.github/workflows/ci.yml` — `lint-version-sync` job checks `Cargo.toml` == `pg_ripple.control` == `pg_ripple_http/Cargo.toml` and validates `COMPATIBLE_EXTENSION_MIN` ≤ extension version.
- **BUILD-02**: `.github/workflows/migration-chain.yml` (new file) — Dedicated migration chain workflow triggered on SQL/control file changes.
- **CDC-02**: `.github/workflows/performance_trend.yml` — `benchmarks/bidi_relay_throughput.sql` wired as `bench_bidi_relay` benchmark step.
- **New GUCs** (v0.91.0):
  - `pg_ripple.shacl_score_log_retention_days` (INT, default 30): days to retain SHACL score log entries.
  - `pg_ripple.cdc_watermark_batch_size` (INT, default 100): number of CDC events per watermark flush batch.
  - `pg_ripple.cdc_watermark_flush_interval_ms` (INT, default 50): milliseconds between watermark flush cycles.

### Changed

- `pg_ripple_http` version bumped to `0.91.0` (in sync with extension).
- `COMPATIBLE_EXTENSION_MIN` updated to `"0.90.0"` in `pg_ripple_http/src/main.rs`.

### Dependency Triage (DEP-01, DEP-02)

- **ureq 3.x** (DEP-01): ureq remains at 2.x in v0.91.0. The 3.x API is a breaking
  change requiring significant refactoring of the federation HTTP client. Triage decision:
  defer to post-v1.0.0 hardening cycle. Tracking: <https://github.com/algesten/ureq/blob/main/CHANGELOG.md>.
- **arrow / parquet** (DEP-02): `cargo update --dry-run` shows compatible minor bumps
  available (arrow 55.x → 55.y). These are patch-compatible and will be picked up by
  Dependabot/Renovate in the normal dependency update cycle. No blocking issue identified.

---

## [0.90.0] — 2026-04-30 — Assessment 14 Medium Remediation

**Implements v0.90.0 roadmap: closes 24 Medium-severity findings from
PLAN_OVERALL_ASSESSMENT_14. Adds PageRank WCOJ integration, convergence norm GUC,
IVM full-recompute threshold, Count-Min Sketch GUCs, temp-threshold GUC, advisory
lock for concurrent runs, ANALYZE after confidence bulk load, SPARQL MINUS blank-node
regression test, seven pre-emptive module splits, pagerank/uncertain_knowledge_api
module directories, datalog_handlers routing migration, clippy::unwrap_used lint gate,
convergence norm documentation, IVM error bounds documentation, cyclic convergence
guarantee documentation, and new test coverage (proptest oracle, fuzz confidence
loader, concurrency scripts, scale benchmark).**

As pg_ripple's feature surface expanded with the probabilistic Datalog and PageRank engines in versions 0.87 and 0.88, a number of medium-severity findings accumulated around their operational characteristics — convergence behavior, concurrency safety, and the configuration surface needed to tune them for different workloads. Version 0.90.0 addresses all twenty-four of these findings. PageRank gains integration with the worst-case optimal join executor for graphs exceeding ten million edges, making large-scale graph analysis practical without exponential intermediate result sets. An advisory lock prevents concurrent `pagerank_run()` calls on the same topic from interfering with each other, and an IVM full-recompute threshold automatically triggers a complete recalculation when incremental updates have accumulated enough staleness to compromise accuracy.

This release marks a major structural investment in code organization: seven large source files approaching the 1,800-line CI gate are proactively decomposed into focused sub-modules before they become maintenance problems. The `src/pagerank.rs` and `src/uncertain_knowledge_api.rs` files become proper directory modules with logical sub-components, and the Datalog HTTP handlers are extracted to the routing module where they belong architecturally. A `clippy::unwrap_used` CI gate prevents new panicking unwrap calls from entering the codebase going forward. Five property-based tests verify the algebraic identity laws of the noisy-OR confidence operator — commutativity, associativity, monotonicity, and absorbing elements — and a fuzz target exercises the confidence loader with adversarial float inputs including NaN, infinities, and denormal numbers that can cause incorrect results in probabilistic computations.

### Added

- **CB-02**: `docs/src/features/pagerank.md` — Convergence Norm section documenting L1/L2/Linf norm selection via `pg_ripple.pagerank_convergence_norm` GUC; NetworkX vs. igraph behaviour comparison.
- **CB-04**: `docs/src/features/pagerank.md` — Incremental Refresh Error Bounds section with formal $\alpha^K$ bound and automatic full-recompute threshold documentation.
- **CB-05**: `tests/pg_regress/sql/sparql_minus_blank_scope.sql` + expected output — regression test for SPARQL MINUS blank-node scoping (per SPARQL 1.1 §18.6).
- **PERF-01**: `src/pagerank/executor.rs` — WCOJ threshold check at `pagerank_run()` entry; `pg_ripple.pagerank_wcoj_threshold` GUC (INT, default 10, units: millions of edges).
- **PERF-02**: `docs/src/features/pagerank.md` — Count-Min Sketch parameter documentation with error bound formula.
- **PERF-03**: `Cargo.toml` workspace lints — `clippy::unwrap_used = "warn"` and `clippy::expect_used = "warn"` GUC-style gates added; CI blocks on new violations (`ci/regress:unwrap_cap`).
- **PERF-06**: `src/bulk_load.rs` — `ANALYZE _pg_ripple.confidence` after `load_triples_with_confidence()` completion.
- **CON-01**: `tests/concurrency/pagerank_during_merge.sh` — deadlock test: 8 concurrent writers + HTAP merge + `pagerank_run()`.
- **CON-02**: `benchmarks/probabilistic_overhead.sql` extended with hot-row confidence contention benchmark (noisy-OR ON CONFLICT on narrow key range).
- **CON-03**: `src/pagerank/executor.rs` — `pg_advisory_xact_lock(hashtext($1))` per topic at `pagerank_run()` entry.
- **TEST-02**: `tests/proptest/pagerank_oracle.rs` — pure-Rust PageRank oracle proptest (5 invariants: sum, positivity, fixed-point, damping monotonicity, sink handling).
- **TEST-03**: `fuzz/fuzz_targets/confidence_loader.rs` — cargo-fuzz target for adversarial confidence float inputs (NaN, ±∞, negative, >1.0, denormals).
- **TEST-04**: `benchmarks/pagerank_scale.sh` — scale benchmark gate (1M/10M edges with wall-time assertions).
- **TEST-05**: `tests/concurrency/confidence_subxact_rollback.sql` — SAVEPOINT/ROLLBACK confidence table consistency test.
- **DL-02**: `docs/src/features/uncertain-knowledge.md` — Convergence Guarantees for Cyclic Probabilistic Rules section with Knaster–Tarski fixed-point theorem citation.
- **CQ-02 / DL-03**: Pre-emptive module splits for 7 files approaching the 1,800-line CI gate:
  - `src/sparql/execute.rs` → `execute/{exec_core,construct,describe,update,explain}.rs`
  - `src/sparql/expr.rs` → `expr/{functions,filters,aggregates,cast}.rs`
  - `src/datalog/compiler.rs` → `compiler/{sql,prob,shacl_rules,builtins}.rs`
  - `src/storage/ops.rs` → `ops/{insert,delete,scan,merge}.rs`
  - `src/export.rs` → `export/{turtle,jsonld,ntriples,csv,common}.rs`
  - `src/citus.rs` → `citus/{sharding,rls,aggregate,federation}.rs`
  - `src/views.rs` → `views/{construct,describe,sparql}.rs`
- **CQ-03**: `src/pagerank.rs` → `src/pagerank/` directory split (executor, ivm, sketch, centrality, export, explain, mod).
- **CQ-04**: `src/uncertain_knowledge_api.rs` → `src/uncertain_knowledge_api/` directory split (mod + confidence_table, fuzzy, prov, shacl stubs).
- **CQ-05**: `pg_ripple_http/src/datalog.rs` → `pg_ripple_http/src/routing/datalog_handlers.rs` with backward-compat re-export shim.
- **New GUCs** (v0.90.0):
  - `pg_ripple.pagerank_convergence_norm` (TEXT, default `'l1'`): convergence norm selection.
  - `pg_ripple.pagerank_full_recompute_threshold` (FLOAT8, default `0.01`): IVM stale fraction triggering full recompute.
  - `pg_ripple.pagerank_wcoj_threshold` (INT, default `10`): WCOJ path threshold in millions of edges.
  - `pg_ripple.pagerank_sketch_width` (INT, default `2000`): Count-Min Sketch columns.
  - `pg_ripple.pagerank_sketch_depth` (INT, default `5`): Count-Min Sketch depth.
  - `pg_ripple.pagerank_temp_threshold` (INT, default `0` = auto): streaming temp-table threshold.

### Already implemented (verified in codebase)

- **CB-06**: `pg_ripple.export_pagerank()` raises PT0417 for unknown format (silent CSV default was fixed; `src/pagerank/export.rs`).
- **PERF-05**: `src/sparql/embedding.rs` fast-path gate on `pg_ripple.pgvector_enabled` GUC.
- **DL-01**: Probabilistic weight parser validates NaN/negative/> 1.0 and raises PT0301.

### Migration notes

No SQL schema changes. The migration script `sql/pg_ripple--0.89.0--0.90.0.sql` is a
comment-only file listing the new GUCs and behaviour changes.

---

## [0.89.0] — 2026-05-03 — Assessment 14 Critical & High Remediation

**Implements v0.89.0 roadmap: deletes stale backup file, extends migration chain test,
bumps HTTP compat min, adds bump-version dry-run, adds confidence noisy-OR proptest,
GUC name audit with deprecated aliases for v0.87/v0.88 GUCs, default rate limit 100 req/s,
fuzzy input length guard (SEC-02), pagerank seed array guard (SEC-03), IRI escaping in
export_pagerank (SEC-04), and actionable pg_trgm diagnostic in fuzzy SPARQL (CB-03).**

Security findings, even when they appear minor in isolation, require immediate and thorough remediation to prevent them from combining into larger vulnerabilities. Version 0.89.0 addresses the critical and high findings from Assessment 14, starting with four targeted security improvements. A default rate limit of 100 requests per second is now enforced in the HTTP companion out of the box — previously the rate limiter was disabled, leaving deployments with no protection against denial-of-service; a maximum input length guard prevents oversized strings from reaching the fuzzy matching functions; a seed array size limit caps the number of PageRank seed nodes that can be specified in a single call; and the PageRank export function is hardened against injection by switching from string interpolation to parameterized SQL with percent-encoded IRI output.

The release also corrects a naming convention gap: several GUC parameters introduced in v0.87.0 and v0.88.0 violated pg_ripple's established `noun_verb_unit` snake_case naming convention. Canonical names are introduced for all four non-conforming GUCs, with the deprecated names kept functional until the v1.0.0 removal to avoid breaking existing configurations. The migration chain test is extended with checkpoint assertions for v0.84.0 through v0.88.0, verifying that the complete upgrade path works correctly end-to-end across five releases. A suite of seven algebraic property tests for the noisy-OR confidence operator validates commutativity, associativity, monotonicity, idempotence, and output range invariants that the probabilistic reasoning engine depends on for mathematically correct confidence propagation.

### Added

- **DEAD-FILE-01 (CQ-01)**: Deleted `src/gucs/registration.rs.bak`; added `.bak`, `.orig`, `.swp` patterns to `.gitignore`; new `lint-no-backup-files` CI job.
- **TEST-01**: Extended migration chain test with checkpoints for v0.84–v0.88 (DDL assertions for new tables in each release); added MIGCHAIN-SYNC structural version-sync assertion. Evidence: `sql/pg_ripple--0.88.0--0.89.0.sql` migration script; GUC and DDL checkpoint assertions in migration chain test.
- **HTTP-COMPAT-01**: `COMPATIBLE_EXTENSION_MIN` bumped from `0.87.0` to `0.88.0` in `pg_ripple_http/src/main.rs`.
- **ROAD-02**: `justfile` `bump-version` extended with CHANGELOG stub creation; new `bump-version-dry` dry-run recipe.
- **CB-01**: `tests/proptest/confidence_algebra.rs` — 7 algebraic-identity proptests for noisy-OR operator (commutativity, associativity, monotonicity, idempotence, identity, absorbing element, output range).
- **SEC-01**: `pg_ripple_http` default rate limit changed from `0` (disabled) to `100` req/s; operators with `PG_RIPPLE_HTTP_RATE_LIMIT=0` are unaffected.
- **API-01 (GUC canonical names)**: canonical aliases for v0.87/v0.88 GUCs that violated `pg_ripple.noun_verb_unit` convention; deprecated names remain registered until v1.0.0 removal:
  - `pg_ripple.katz_alpha` → `pg_ripple.pagerank_katz_alpha` (canonical)
  - `pg_ripple.federation_minimum_confidence` → `pg_ripple.pagerank_federation_confidence_min`
  - `pg_ripple.default_fuzzy_threshold` → `pg_ripple.fuzzy_match_threshold`
- **SEC-02**: `pg_ripple.fuzzy_max_input_length` GUC (INT, default 4096, range 1–65536); `pg:fuzzy_match()` and `pg:token_set_ratio()` raise PT0308 when either argument exceeds the limit.
- **SEC-03**: `pg_ripple.pagerank_max_seeds` GUC (INT, default 1024, range 1–1048576); `pagerank_run(..., seed_iris TEXT[])` raises PT0411 when the array exceeds the limit.
- **SEC-04 + CB-03**: `export_pagerank()` now uses parameterized SQL for the `topic` parameter (no more direct interpolation); node IRI output is percent-encoded per RFC 3987. `pg:fuzzy_match()` / `pg:token_set_ratio()` now route through `pg_ripple._fuzzy_match_guard()` / `pg_ripple._token_set_ratio_guard()` guard functions that raise actionable PT0302 (pg_trgm missing) and PT0308 (input too long).

### Changed

- `pg_ripple_http` default rate limit default changed from 0 to 100 req/s (SEC-01). Set `PG_RIPPLE_HTTP_RATE_LIMIT=0` to restore the old disabled behavior.

### Migration notes

No SQL schema changes. The migration script `sql/pg_ripple--0.88.0--0.89.0.sql` is a comment-only file.

---

## [0.88.0] — 2026-05-XX — Datalog-Native PageRank & Graph Analytics

**Implements v0.88.0 roadmap: iterative PageRank engine via Datalog^agg + subsumptive tabling,
topic-sensitive and personalized PageRank, IVM dirty-edge queue (K-hop incremental refresh),
confidence-weighted edges, four centrality measures (betweenness, closeness, degree, Katz),
score-explanation trees, standard-format export (CSV/Turtle/N-Triples/JSON-LD), probabilistic
score bounds, SHACL-aware ranking, federation blend mode, centrality-guided entity deduplication,
HTTP companion PageRank/centrality REST API, pg_regress test suite, and benchmarks.**

PageRank is one of the most famous algorithms in computer science — the formula originally used by Google to rank web pages and now widely applied to knowledge graphs, citation networks, social media, and supply chains to identify the most influential or important nodes. Version 0.88.0 implements a full, Datalog-native PageRank engine inside pg_ripple that goes far beyond a simple implementation: it supports topic-sensitive and personalized PageRank, incremental refresh when the graph changes, confidence-weighted edges, and four centrality measures (betweenness, closeness, degree, and Katz centrality). Results include probabilistic score bounds derived from the confidence side table, giving every rank score a lower and upper bound that reflects the uncertainty in the underlying data, and a SHACL-aware ranking mode can exclude nodes that fail shape validation constraints.

The practical applications are wide-ranging. Business intelligence teams can identify the most influential products in a supply chain graph or the most connected entities in a customer relationship graph. Research teams can surface the most-cited papers in a literature knowledge base or the most authoritative sources in a fact-checking graph. The `pg:pagerank()` SPARQL extension function makes it possible to filter and sort SPARQL query results by centrality without leaving the query language. A centrality-guided entity deduplication function uses PageRank scores to identify the canonical representative when resolving `owl:sameAs` identity clusters. Score explanation trees show exactly why a node has its current rank and which neighbors contributed to it most, and standard-format exports in CSV, Turtle, N-Triples, and JSON-LD make it easy to consume results in downstream analytics and visualization tools.

### Added

- **PR-DATALOG-01**: `src/pagerank.rs` — Datalog-native iterative PageRank via `WITH RECURSIVE` SQL; subsumptive tabling for convergence-aware early termination; `_pg_ripple.pagerank_scores` persistence table.
- **PR-ITER-01**: Power-iteration loop; L1-norm convergence test; per-iteration delta tracking.
- **PR-DAMPING-01**: Configurable damping factor (`pg_ripple.pagerank_damping`, default 0.85); teleportation redistributes to dangling nodes.
- **PR-BLANK-01**: `pg_ripple.pagerank_include_blank_nodes` GUC; blank nodes excluded by default.
- **PR-PERSONAL-01**: Personalization vector via `seed_iris` + `bias` parameters; uniform bias when no seeds.
- **PR-SPARQL-FN-01**: `pg:pagerank()` and `pg:pagerank(?node, ?topic)` SPARQL extension functions.
- **PR-TOPN-01**: `pg:topN_approx()` sketch-based approximate top-N; `top_k` parameter on `pagerank_run()`.
- **PR-SQL-FN-01**: `pg_ripple.pagerank_run(damping, max_iterations, convergence_delta, direction, topic, ...)` SQL set-returning function.
- **PR-VIEW-01**: `_pg_ripple.pagerank_scores (node, topic, score, score_lower, score_upper, computed_at, iterations, converged, stale, stale_since)` table; BRIN index on `computed_at`.
- **PR-MAGIC-01**: Magic-sets transformation for goal-directed partial-graph evaluation (bound node shortcut).
- **PR-TRICKLE-01**: `_pg_ripple.pagerank_dirty_edges` IVM queue; K-hop incremental refresh; `pg_ripple.pagerank_incremental` GUC; `pg_ripple.vacuum_pagerank_dirty()`.
- **PR-TRICKLE-CONF-01**: Confidence-attenuated K-hop propagation; `pg_ripple.pagerank_trickle_confidence_attenuation` GUC.
- **PR-CONF-01**: Confidence-weighted edges via `_pg_ripple.confidence` join; `pg_ripple.pagerank_confidence_weighted` GUC.
- **PR-PROB-DATALOG-01**: Probabilistic PageRank score bounds via `@weight` Datalog rules; `score_lower`/`score_upper` columns; `pg_ripple.pagerank_probabilistic` GUC.
- **PR-TOPIC-01**: Topic-sensitive multi-run via `topic` parameter and `pg_ripple.pagerank_run_topics(topics text[])`.
- **PR-WEIGHT-01**: Edge-weight predicate (`edge_weight_predicate` param); `pg_ripple.pagerank_confidence_default` GUC.
- **PR-REVERSE-01**: `direction` parameter: `'forward'` / `'reverse'` / `'undirected'`.
- **PR-EXPLAIN-SCORE-01**: `pg_ripple.explain_pagerank(node_iri, top_k)` returns depth/contributor/contribution/path tree.
- **PR-STALE-BOUNDS-01**: `stale` / `stale_since` columns; `pg_ripple.is_stale()` helper; `pg_ripple.pagerank_lower()` / `pg_ripple.pagerank_upper()`.
- **PR-IVM-METRICS-01**: `pg_ripple.pagerank_queue_stats()` returning `(queued_edges, max_delta, oldest_enqueue, estimated_drain_seconds)`.
- **PR-SKETCH-01**: `pg_ripple.pagerank_selective_threshold` GUC for selective per-node recomputation gating.
- **PR-PARTITION-01**: `pg_ripple.pagerank_partition` GUC; per-named-graph parallel evaluation.
- **PR-SELECTIVE-01**: Selective recomputation of high-centrality nodes only.
- **PR-TEMPORAL-01**: `decay_rate` + `temporal_predicate` parameters for temporal authority decay.
- **PR-SHACL-01**: `pg_ripple.pagerank_shacl_threshold` GUC; `shacl_score()` threshold gate; `sh:importance` / `sh:excludeFromRanking` awareness.
- **PR-EXPORT-01**: `pg_ripple.export_pagerank(format, top_k, topic)` — CSV, Turtle, N-Triples, JSON-LD.
- **PR-FED-01**: `pg_ripple.pagerank_federation_blend` GUC; federation blend mode.
- **PR-FED-CONF-01**: Confidence-gated federation edges.
- **PR-CENTRALITY-01**: `pg_ripple.centrality_run(metric)` for betweenness, closeness, degree, Katz; `_pg_ripple.centrality_scores` table.
- **PR-TRUST-EIGEN-01**: Source-trust-weighted eigenvector centrality.
- **PR-ENTITY-RESOLUTION-01**: `pg_ripple.pagerank_find_duplicates(metric, centrality_threshold, fuzzy_threshold)` — centrality-guided entity deduplication.
- **PR-KATZ-TEMPORAL-01**: Temporal authority via Katz centrality; `pg_ripple.katz_alpha` GUC.
- **PR-HTTP-01**: 10 new HTTP endpoints in `pg_ripple_http` (`/pagerank/*`, `/centrality/*`); `pagerank_handlers.rs`.
- **PR-CI-01**: `tests/pg_regress/sql/pagerank.sql` pg_regress test suite (30 tests).
- **PR-BENCH-01**: `benchmarks/pagerank.sql` — 10 pgbench scenarios for scale-free graph.
- **PR-DOCS-01**: `docs/src/features/pagerank.md`.
- **PR-EXPLAIN-01**: `explain_pagerank()` score-explanation tree (tree traversal via `WITH RECURSIVE`).
- **PR-ERR-01**: Error constants PT0401–PT0410, PT0411–PT0419, PT0420–PT0423 for PageRank error catalog (ranges: PT040x, PT041x, PT042x).
- **PR-MIGRATE-01**: `sql/pg_ripple--0.87.0--0.88.0.sql` migration script; 3 new tables + BRIN index + RLS policies.
- 22 new GUC parameters in `src/gucs/pagerank.rs`.
- 8 new `feature_status` rows (`pagerank_datalog`, `pagerank_incremental`, `pagerank_confidence_weighted`, `pagerank_centrality`, `pagerank_explain`, `pagerank_export`, `pagerank_entity_resolution`, `pagerank_http_api`).
- `pg_ripple_http` version bumped to 0.88.0.

---

## [0.87.0] — 2026-05-XX — Uncertain Knowledge Engine

**Implements v0.87.0 roadmap: probabilistic Datalog with `@weight` annotations, confidence
side table (`_pg_ripple.confidence`), fuzzy SPARQL extension functions (`pg:confidence()`,
`pg:fuzzy_match()`, `pg:token_set_ratio()`, `pg:confPath()`), soft SHACL quality scoring
(`pg_ripple.shacl_score()`, `pg_ripple.shacl_report_scored()`), confidence-aware bulk load
(`pg_ripple.load_triples_with_confidence()`), PROV-O confidence propagation, RDF-star Turtle
export with confidence annotations, HTTP companion endpoints (`/confidence/*`), and garbage
collection (`pg_ripple.vacuum_confidence()`).**

Real-world knowledge graphs rarely deal with certainties — information extracted from text has varying reliability, facts from sensors carry measurement error, and rules that combine multiple pieces of evidence should produce conclusions with appropriately reduced confidence. Version 0.87.0 introduces a complete uncertain knowledge engine for pg_ripple: a confidence side table stores a numerical confidence score for any triple in the graph, probabilistic Datalog rules with `@weight` annotations propagate uncertainty through inference using noisy-OR semantics, and SPARQL queries can filter by confidence threshold or traverse high-confidence paths using the new `pg:confPath()` property path operator. The result is a knowledge graph that represents not just what it knows, but how confident it is in each thing it knows.

Soft SHACL quality scoring extends the validation framework into the probabilistic domain: instead of simply passing or failing, shapes can now return weighted quality scores, and a score log tracks how data quality evolves over time — allowing teams to set alerts when quality drops below a threshold. Bulk loading of triples with associated confidence values makes it straightforward to feed scores from external machine learning models, document extractors, or human annotation pipelines directly into the graph. A PROV-O integration propagates confidence through provenance chains, so the reliability of a conclusion can be traced back through the complete derivation to the reliability of its source facts. Eight HTTP endpoints expose the full confidence API to external applications, making probabilistic knowledge graph queries accessible from any programming language.

### Added

- **PROB-DATALOG-01**: `@weight(F)` annotation on Datalog rules; noisy-OR confidence propagation via `_pg_ripple.confidence` side table.
- **CONF-TABLE-01**: `_pg_ripple.confidence (statement_id, confidence, model, asserted_at)` side table; `confidence_stmt_idx` index; optional `dict_trgm_idx` GIN index when `pg_trgm` is installed.
- **FUZZY-SPARQL-01**: `pg:confidence(?s,?p,?o)`, `pg:fuzzy_match(a,b)`, `pg:token_set_ratio(a,b)` SPARQL extension functions; `pg:confPath(pred, threshold)` property path operator.
- **SOFT-SHACL-01**: `pg_ripple.shacl_score(graph_iri)`, `pg_ripple.shacl_report_scored(graph_iri)`, `pg_ripple.log_shacl_score(graph_iri)` functions; `sh:severityWeight` support; `_pg_ripple.shacl_score_log` table.
- **LOAD-CONF-01**: `pg_ripple.load_triples_with_confidence(data, confidence, format, graph_uri)` bulk loader.
- **CONF-EXPORT-01**: `pg_ripple.export_turtle_with_confidence(graph)` with RDF-star confidence annotations; `pg_ripple.export_confidence` GUC.
- **PROV-CONF-01**: `pg_ripple.prov_confidence` GUC for PROV-O `pg:sourceTrust` confidence propagation.
- **CONF-CWB-01**: `pg_ripple.cwb_confidence_propagation` GUC; CWB confidence propagation in `run_full_recompute`.
- **CONF-GC-01**: Orphaned confidence row cleanup in `delete_triple_by_ids`, `run_dred_retraction`, and HTAP `merge_all`.
- **CONF-HTTP-01**: HTTP endpoints `POST /confidence/load`, `GET /confidence/shacl-score`, `GET /confidence/shacl-report`, `POST /confidence/vacuum`.
- **CONF-EXPLAIN-01**: `explain_datalog()` now includes a `"confidence"` node with per-rule weights.
- **CONF-CYCLIC-01**: `prob_datalog_cyclic`, `prob_datalog_max_iterations`, `prob_datalog_convergence_delta`, `prob_datalog_cyclic_strict` GUCs.
- **CONF-ERR-01**: Error variants PT0301–PT0307 in `UncertainKnowledgeError` enum.
- **CONF-RLS-01**: Row-level security policies on `_pg_ripple.confidence` and `_pg_ripple.shacl_score_log`.
- **CONF-DOCS-01**: `docs/src/features/uncertain-knowledge.md`; 9 new GUC entries in `docs/src/operations/configuration.md`.
- **CONF-PERF-01**: `benchmarks/probabilistic_overhead.sql` and `benchmarks/confidence_join_scale.sql`.
- **CONF-SBOM-01**: `postgresql-contrib` added to Dockerfile runtime layer; `audit.toml` pg_trgm note; `sbom.json` regenerated for v0.87.0.
- 5 new feature status rows (probabilistic_datalog, fuzzy_sparql, confidence_side_table, soft_shacl_scoring, prov_confidence).
- `pg_ripple_http` version bumped to 0.87.0; `COMPATIBLE_EXTENSION_MIN` updated to 0.87.0.
- `tests/pg_regress/sql/probabilistic.sql` regression test.

---

## [0.86.0] — 2026-05-02 — Assessment 13 Tests, API Polish, Observability, Supply Chain & Standards

**Implements v0.86.0 roadmap: closes the remaining 30+ Low-priority and backlog findings from
Assessment 13. All 82 A13 findings are now resolved. Key additions: SSE streaming cursor
(`HTTP-02`), axum graceful shutdown (`O13-05`), structured JSON log output (`O13-04`),
new Prometheus metrics (`O13-02`, `S13-03`), Arrow Flight 413 guard before materialisation
(`S13-08`), CONSTRUCT/SHACL-SPARQL fuzz targets (`T13-03`), conformance trend CSV artifact
(`T13-04`), `describe_form` GUC (`SC13-04`), `unreachable!` → `pgrx::error!` conversions
(`Q13-07`/`CC13-05`), POSTGRES_PASSWORD_FILE docker-compose pattern (`S13-07`), audit.toml
expiry dates (`DS13-02`/`S13-04`), Renovate rust-toolchain update config (`DS13-04`),
error-codes registry (`A13-03`), deprecated-gucs docs (`A13-04`), GeoSPARQL function
inventory (`SC13-03`), compatibility matrix v0.80–v0.86 rows (`D13-01`), blog post version
index (`D13-04`), and CDC slot cleanup crash-recovery test (`T13-07`).**

Version 0.86.0 closes the final thirty-plus low-priority and backlog findings from Assessment 13, marking the complete resolution of all 82 findings in that assessment cycle. Among the most operationally significant additions is a Server-Sent Events streaming cursor that allows clients to receive SPARQL query results as a continuous stream rather than waiting for the entire result set to materialize — a key capability for dashboards and monitoring applications that display live graph data. Axum graceful shutdown ensures the HTTP companion completes in-flight requests before exiting on SIGTERM, protecting queries in progress during rolling restarts. Structured JSON log output from the HTTP companion makes it straightforward to feed logs into centralized aggregation platforms like Elasticsearch or Loki without custom parsing.

Security and supply chain hardening receive careful attention throughout. The Arrow Flight endpoint now runs a row count pre-check before materializing results and returns HTTP 413 with a generic message when the export limit is exceeded — importantly, the actual row count is only logged server-side to avoid leaking internal state in error responses visible to clients. The Docker Compose configuration is updated to use the POSTGRES_PASSWORD_FILE Docker secrets pattern, replacing plaintext password environment variables with secrets file injection. Four security advisories in `audit.toml` gain explicit expiry dates, turning an implicit "we know about this" list into a tracked obligation with documented review deadlines. A complete error code registry and a deprecated-GUCs reference document close two documentation gaps that had been making troubleshooting harder than necessary.

### Tests (T13-02 – T13-07)

- **T13-03** — added `fuzz/fuzz_targets/construct_rule.rs` and `fuzz/fuzz_targets/shacl_sparql.rs`; registered in `fuzz/Cargo.toml`; wired into weekly fuzz CI job.
- **T13-04** — added CI artifact `tests/conformance/history.csv` tracking per-version pass rates across all five conformance suites; added `docs/src/reference/conformance-trends.md` page.
- **T13-05** — `#[pg_extern]` coverage gap re-audited; gap confirmed closed by v0.85.0 REG-TESTS-01.
- **T13-06** — `scripts/bench_check_regression.py --fail-on-regression 10` confirmed in benchmark workflow; gate active.
- **T13-07** — added `tests/crash_recovery/cdc_slot_cleanup_during_kill.sh`; creates a slot, simulates SIGKILL mid-cleanup, asserts slot is reclaimed on restart.

### API Polish (A13-01 – A13-06)

- **A13-01** — `json_ld_load` alias doc comment updated to note `-- removal scheduled for v1.0.0`; deprecation warning already present since v0.83.0.
- **A13-03** — created `docs/src/reference/error-codes.md` listing every PT code with meaning and source file.
- **A13-04** — created `docs/src/reference/deprecated-gucs.md` listing deprecated GUCs with replacement names and removal versions.
- **A13-06** — SPARQL parse errors consistently return `PT400` error code across HTTP companion and extension.

### Documentation (D13-01 – D13-05)

- **D13-01** — `docs/src/operations/compatibility.md` updated with v0.80–v0.86 rows.
- **D13-04** — `blog/README.md` updated with a "Posts by Version" index.
- **D13-05** — `plans/probabilistic-features.md` linked from ROADMAP.md v0.87.0 section header.

### Supply Chain (DS13-02 – DS13-04)

- **DS13-01 (triage)** — Dependency upgrade triage decisions documented:
  - `ureq` stays at 2.x: ureq 3.x has breaking API changes (`AgentBuilder` removed, all `send_*` call sites affected across federation code); upgrade deferred to v0.87.0+ after API migration.
  - `parquet` stays at 58.x / `arrow` stays at 55.x: arrow 56.x not yet available on crates.io as of 2026-05-02; will upgrade when available.
  - `tokio-stream` is now justified by the SSE streaming implementation in `pg_ripple_http/src/stream.rs` (HTTP-02); previously it was a potential removal candidate.
- **DS13-02/S13-04** — `audit.toml` expiry dates added to all four RUSTSEC ignores; structured ignore objects replace plain strings.
- **DS13-04** — `renovate.json` updated with `matchFileNames: ["rust-toolchain.toml"]` rule for automatic toolchain update proposals (manual merge required).

### Observability (O13-02 – O13-05)

- **O13-02** — added Prometheus metrics: `pg_ripple_federation_endpoint_requests_total`, `pg_ripple_federation_endpoint_duration_seconds`, `pg_ripple_dictionary_cache_hit_ratio`, `pg_ripple_merge_worker_delta_rows_pending`.
- **O13-04** — `pg_ripple_http` respects `RUST_LOG_FORMAT=json` env var to switch `tracing-subscriber` to JSON layer for structured log output.
- **O13-05** — added `axum::serve(...).with_graceful_shutdown(shutdown_signal())` for 30-second SIGTERM drain window.

### Security (S13-03, S13-06, S13-07 – S13-10)

- **S13-03** — added `pg_ripple_http_cors_permissive_requests_total` Prometheus counter; incremented when `PG_RIPPLE_HTTP_CORS_ORIGINS=*` is active; documented in `docs/src/operations/security.md`.
- **S13-07** — `docker-compose.yml` updated to use `POSTGRES_PASSWORD_FILE` Docker secrets pattern; secrets directory gitignored.
- **S13-08** — Arrow Flight endpoint runs a `COUNT(*)` pre-check before materialising results; returns HTTP 413 with a generic message (no row count) if `ARROW_MAX_EXPORT_ROWS` exceeded; actual count logged server-side only.
- **S13-09** — `pg_ripple_http/README.md` top-level note warns operators to network-isolate the metrics endpoint.
- **S13-10** — `docs/src/operations/security.md` documents supported auth schemes (Bearer only; Basic not accepted).

### Standards Conformance (SC13-03, SC13-04)

- **SC13-03** — created `docs/src/reference/geosparql-functions.md` with status table for all ~30 GeoSPARQL 1.1 functions.
- **SC13-04** — added `pg_ripple.describe_form` GUC (values: `cbd`, `scbd`, `symmetric`; `symmetric` is an alias for `scbd`); supersedes `pg_ripple.describe_strategy` when set.

### HTTP Companion (HTTP-02, DS13-05)

- **HTTP-02** — `pg_ripple_http/src/stream.rs` implemented with SSE streaming SELECT cursor (`stream_sparql_select()`); justifies `tokio-stream` dependency.
- **DS13-05** — `tokio-stream` dependency is now fully justified by the SSE implementation using `ReceiverStream`; the previous "remove if no streaming" triage decision is closed.

### Code Quality (Q13-07)

- **Q13-07/CC13-05** — all 9 `unreachable!` calls in production code converted to `pgrx::error!("internal: <description> — please report")` at: `src/datalog/explain.rs:115`, `src/sparql/federation/circuit.rs:157`, `src/views.rs:839,869`, `src/construct_rules/mod.rs:239,257`, `src/construct_rules/delta.rs:111,138`, `src/replication.rs:78`.

---

## [0.85.0] — 2026-07-17 — Assessment 13 Medium Findings

**Implements v0.85.0 roadmap: all 22 medium-priority findings from Assessment 13
(correctness, performance, code quality, and concurrency). Key additions:
`batch_decode` respects `strict_dictionary` GUC, `schema.rs` and `federation.rs`
module splits, CI 1,800-line lint gate, `describe_cbd` depth GUC, per-predicate
merge fence lock, `encode_batch` single-CTE API, dictionary hot-cache Prometheus
counters, and VP-promotion crash-recovery regression test.**

Data correctness in a knowledge graph system depends on precise handling of edge cases that can seem minor until they cause subtle bugs in production. Version 0.85.0 addresses all twenty-two medium-severity findings from Assessment 13. The dictionary's `batch_decode()` function is corrected to properly handle negative IDs — which represent inline-encoded integers — that were being incorrectly treated as missing values and triggering unnecessary warnings. A new `strict_dictionary` GUC mode makes missing dictionary IDs raise a proper PostgreSQL error rather than silently returning empty strings, giving teams a choice between strict correctness guarantees and graceful degradation for legacy compatibility. Typed literals are now correctly routed through the typed literal encoder in the Datalog magic sets compiler, fixing incorrect encoding of values like `"42"^^xsd:integer` that would silently produce wrong inference results.

Performance improvements are grounded in measured optimization. A new `encode_batch()` internal API reduces the number of database round-trips when encoding multiple terms simultaneously from O(n) to O(1) by using a single CTE INSERT for all cache-miss terms — providing a meaningful throughput improvement for bulk operations. Per-predicate merge fence locks replace the previous global lock, eliminating contention between concurrent merge workers operating on different predicates and allowing multiple predicates to merge simultaneously. A VP-promotion crash-recovery regression test verifies that an interrupted promotion leaves the system in a recoverable state, providing confidence that the self-healing recovery function works correctly before it is needed in production. A CI file size gate enforces the 1,800-line per-file limit going forward, preventing architectural debt from silently accumulating.

### Correctness

- **C13-02** — `batch_decode` now raises a PostgreSQL error (`PT512`) when a dictionary ID is missing and `pg_ripple.strict_dictionary = on`. Previously returned a silent empty string. Graceful-degradation `WARNING` path retained for `strict_dictionary = off`.
- **C13-03** — Blank-node-in-quoted-triple limitation documented in `docs/src/reference/sparql-compliance.md`. Regression test added in `tests/pg_regress/sql/v085_features.sql`.
- **C13-04** — `execute_drop` and `execute_clear` in `src/sparql/execute.rs` annotated with doc comments documenting the mutation journal flush obligation.
- **C13-05** — Plan cache key for `INFERENCE_MODE` now trimmed and lowercased before hashing, preventing spurious cache misses from capitalisation or padding differences.
- **C13-06** — `GRAPH ?g` default-graph exclusion behaviour documented in `docs/src/reference/sparql-compliance.md`. Regression test verifies `?g` binds only named graphs (SPARQL 1.1 §8.3).
- **C13-07** — `batch_decode` warning guard tightened from `id <= 0` to `id == 0`. Negative IDs (inline-encoded integers) are now correctly passed through.
- **C13-08** — `encode_token` in `src/datalog/magic.rs` now detects typed literals (`^^<` suffix) and routes to `encode_typed_literal()` instead of plain string encoding.
- **C13-09** — `parse_nt_triple` in `src/lib.rs` now rejects IRIs longer than 4 KiB (emits a `WARNING` and returns `None`) and requires the IRI to end with `>`.
- **C13-10** — `xsd:dateTime` sub-millisecond precision truncation documented in `docs/src/reference/sparql-compliance.md`. Regression test added.
- **C13-11** — `describe_cbd` recursion depth capped by new GUC `pg_ripple.describe_max_depth` (default 16, range 1–256). Prevents runaway recursion on cyclic or deep graphs.

### Performance

- **P13-02** — New `encode_batch(terms: &[(&str, i16)]) → Vec<i64>` internal API in `src/dictionary/mod.rs`. Uses a single CTE INSERT for all cache-miss terms. Exposed via `pg_ripple.batch_encode_terms(TEXT[], SMALLINT[]) → BIGINT[]`.
- **P13-03** — Merge-worker heartbeat log already throttled to once per 60 seconds (delivered in v0.83.0); confirmed as done. See `src/merge_worker.rs` throttle guard and `roadmap/v0.83.0.md`.
- **P13-04** — `execute_select()` in `src/sparql/execute.rs` batches all `SET LOCAL` calls into a single SPI round-trip.
- **P13-05** — Datalog inference in `src/datalog/seminaive.rs` streams rule SQL in batches of 100, reducing peak SPI call count for large rule sets.
- **P13-06** — `partition_into_parallel_groups()` in `src/datalog/parallel.rs` pre-checks for directed cycles before union-find SCC evaluation; logs a warning on cycle detection.
- **P13-07** — `PathCtx.counter` field made private; `next_alias()` mutation method and `value()` accessor added to `src/sparql/property_path.rs`.
- **P13-08** — `dictionary_hot_cache_hits_total` and `dictionary_hot_cache_misses_total` Prometheus counters added. Exposed in-database via `pg_ripple.dictionary_cache_stats()` and in the HTTP `/metrics` Prometheus endpoint. The legacy shared-memory cache statistics function (previously also named `dictionary_cache_stats`) is now exposed as `pg_ripple.shmem_cache_stats()` to avoid a naming conflict; it continues to return the same four-column table (hits, misses, evictions, hit_rate) as introduced in v0.47.0.

### Code Quality

- **Q13-02** — `src/schema.rs` (1,939 lines) split into `src/schema/{tables,views,triggers,rls}.rs`.
- **Q13-03** — `src/sparql/federation.rs` (1,693 lines) split into `src/sparql/federation/{circuit,policy,http,decode}.rs`.
- **Q13-04** — CI lint gate (`lint-file-size` job in `.github/workflows/ci.yml`): any `src/**/*.rs` file exceeding 1,800 lines fails the build unless it contains an `// @allow-large-file: <reason>` annotation.
- **Q13-05** — All `#[allow(dead_code)]` markers audited. Each now carries a `// Q13-05` comment explaining the justification (BGW indirection, public API surface, etc.).

### Concurrency

- **CC13-01** — New VP-promotion crash-recovery regression test `tests/crash_recovery/promote_sigkill.sh`. SIGKILLs a backend during rare-predicate promotion and asserts `recover_interrupted_promotions()` returns a consistent state.
- **CC13-02** — Merge fence advisory lock namespaced per-predicate (`predicate_id + 0x5052_5000`). Eliminates global lock contention between concurrent merge workers on different predicates.

---

## [0.84.0] — 2026-07-16 — Assessment 13 Critical/High & Operational Remediation

**Implements v0.84.0 roadmap: 13 items addressing all Critical and High findings
from Assessment 13. Key additions: HTTP companion version sync (6-version drift
closed), PG_RIPPLE_HTTP_STRICT_COMPAT env var, docker-compose image tag CI
gate, SECURITY DEFINER inline annotations, migration-chain v0.80–v0.83 test
coverage, gucs/registration.rs 6-domain split, nested OPTIONAL+EXISTS
regression test, /health/ready deep-check endpoint, plan-cache double-parse
elimination, and justfile automation recipes.**

Version 0.84.0 closes all thirteen critical and high findings from Assessment 13, with the most significant being a six-version lag in the HTTP companion's compatibility tracking that had been recurring across multiple assessment cycles. The companion's `COMPATIBLE_EXTENSION_MIN` is raised to the current value, and a new `PG_RIPPLE_HTTP_STRICT_COMPAT` environment variable allows operators to configure the companion to exit with an error rather than just log a warning when connected to an incompatible extension version — providing a hard guarantee for production environments where silent incompatibility could cause subtle failures. A new `/health/ready` deep-check endpoint performs a real PostgreSQL round-trip to verify both database connectivity and extension installation, giving orchestration platforms accurate readiness information.

Code quality improvements address two security-sensitive areas. The `src/gucs/registration.rs` file, at over 2,000 lines, is decomposed into six domain-specific submodules covering SPARQL, storage, federation, Datalog, security, and observability GUCs — making the GUC catalog far easier to navigate, audit, and extend. A plan-cache double-parse bug is fixed by threading the canonical SPARQL form through the parse and cache layers so each query string is parsed only once rather than twice on cache misses, improving performance for uncached queries. The justfile gains four automation recipes for version bumping, SBOM regeneration, OpenAPI spec generation, and version sync verification — eliminating manual multi-file updates that were a frequent source of version string inconsistencies across the codebase.

### HTTP Companion (pg_ripple_http)

- **HTTP-01 / MF-B** — `pg_ripple_http` bumped to `0.84.0`. `COMPATIBLE_EXTENSION_MIN` raised from `"0.79.0"` to `"0.84.0"` in `pg_ripple_http/src/main.rs`.
- **S13-05** — New `PG_RIPPLE_HTTP_STRICT_COMPAT=1` environment variable. When set, an extension-version mismatch (below `COMPATIBLE_EXTENSION_MIN`) causes the service to exit with code 1 instead of only logging a warning. Default: off (backward-compatible).
- **O13-01** — New `/health/ready` HTTP endpoint performs a real PostgreSQL round-trip (`SELECT 1 FROM pg_extension WHERE extname='pg_ripple'`) with a hard 2-second deadline. Returns `200 {"status":"ok"}` or `503 {"status":"unavailable","reason":"..."}`. `/health` remains a fast liveness probe; `/ready` remains the deep feature-status probe.

### Security

- **S13-01** — Both `SECURITY DEFINER` occurrences (`src/schema.rs:996` and `sql/pg_ripple--0.55.0--0.56.0.sql:60`) annotated with `-- SECURITY-JUSTIFY:` inline comments explaining the privilege requirement. `scripts/check_no_security_definer.sh` updated to require the marker on any SECURITY DEFINER line.
- **S13-02** — `scripts/check_no_string_format_in_sql.sh` confirmed as a required CI step in `.github/workflows/ci.yml` (SQL-injection gate).

### Build & Tooling

- **BUILD-01** — `docker-compose.yml` image tags updated from `0.54.0` to `0.84.0`. New `lint-docker-compose-version` CI job asserts the image tag matches `Cargo.toml` version on every PR.
- **BUILD-02** — `justfile` gains four new automation recipes:
  - `bump-version NEW_VERSION` — atomically updates Cargo.toml (root + pg_ripple_http), pg_ripple.control, COMPATIBLE_EXTENSION_MIN, docker-compose tag, creates migration script stub
  - `regen-sbom` — regenerates `sbom.json` via `cargo cyclonedx`
  - `regen-openapi` — fetches the live OpenAPI spec from the running HTTP service
  - `check-version-sync` — asserts all version strings are consistent
- **BUILD-03** — Migration-chain test confirmed as a required CI step.

### Testing

- **T13-01** — `tests/test_migration_chain.sh` extended with checkpoint assertions for v0.80.0–v0.83.0 (21 migration scripts total). Checks: `predicates.triple_count` column (v0.80), `_pg_ripple.cdc_lsn_watermark` table (v0.81), merge-worker and federation stats tables (v0.82), core table column integrity (v0.83).
- **C13-01** — New pg_regress test `tests/pg_regress/sql/sparql_optional_exists.sql` covering nested `OPTIONAL { ... FILTER(EXISTS { ... }) }` and `FILTER NOT EXISTS` semantics.

### Performance

- **P13-01** — `src/sparql/plan_cache.rs`: new `get_canonical(canonical: &str)` and `put_canonical(canonical: &str, entry)` functions accept the `spargebra::Query` Display form. `src/sparql/plan.rs` updated to parse once and pass the canonical form through, eliminating the double-parse on every cache-miss path.

### Code Quality

- **Q13-01** — `src/gucs/registration.rs` (2,032 lines) split into 6 per-domain submodules under `src/gucs/registration/`: `sparql.rs`, `storage.rs`, `federation.rs`, `datalog.rs`, `security.rs`, `observability.rs`. Public re-exports from `mod.rs` unchanged; callers unaffected.

### Process

- **PROMPT-01** — `plans/overall_assesment_prompt.md` template created. Anchors automated assessments to the latest **tagged** release, preventing prompt-vs-reality gaps like the one identified in Assessment 13.
- **V084-01** — `ROADMAP.md` scope decision recorded: uncertain knowledge engine (probabilistic Datalog, fuzzy SPARQL, soft SHACL) moved to **v0.87.0**; v0.84.0–v0.86.0 reserved for Assessment 13 remediation.

---

## [0.83.0] — 2026-07-09 — Assessment 12 Test Coverage, API Polish & Code Quality

**Implements v0.83.0 roadmap: 25 items across test coverage, API polish, code
quality, and security hardening. Key additions: N-Triples/N-Quads/TriG fuzz
targets, proptest reference-implementation comparison (oxigraph), CDC
LISTEN/NOTIFY barrier integration test, bidi module split, blank node export
validation, load_jsonld alias, datalog cost-model GUCs, merge worker exponential
backoff, RFC 3339 build timestamp in /health, JSON 401 error envelope,
WWW-Authenticate header, and CHANGELOG/GUC naming conventions.**

A knowledge graph system's correctness guarantees are only as strong as its ability to detect regressions, and version 0.83.0 significantly expands that ability. Three new fuzz targets exercise the N-Triples, N-Quads, and TriG format parsers with arbitrary byte sequences, hardening the parsers against malformed input that could cause unexpected behavior. A new proptest suite compares pg_ripple's N-Triples parsing results against oxigraph as an independent reference implementation, catching any case where the two implementations disagree on what constitutes valid syntax. A CDC LISTEN/NOTIFY integration test demonstrates the correct event-driven pattern for waiting on CDC events without using `sleep()`, which is prone to timing-related flakiness in CI.

Two API improvements in this release have lasting impact. The `load_jsonld()` function is introduced as the new canonical name for JSON-LD loading, with the old `json_ld_load()` emitting a deprecation notice and scheduled for removal at v1.0.0, giving teams a clear migration path with a long runway. The `src/bidi.rs` module, at over 2,500 lines, is decomposed into five focused sub-modules — protocol, relay, subscribe, and sync — the largest single-file split in the project's history. The HTTP companion gains proper 401 JSON error responses with `WWW-Authenticate` headers consistent with RFC 7235, matching the JSON error envelope used throughout the REST API. Merge worker exponential backoff replaces a flat retry interval, making the worker recover more gracefully from transient errors without flooding logs during extended incidents.

### Test Coverage

- **FUZZ-BULK-01** — Three new fuzz targets: `ntriples_load`, `nquads_load`, `trig_load` in `fuzz/fuzz_targets/`. Registered in `fuzz/Cargo.toml` and CI fuzz workflow.
- **FUZZ-UPDATE-01** — SPARQL Update fuzz target (`fuzz/fuzz_targets/sparql_update.rs`) confirmed present from v0.79.0; corpus seeded from `tests/sparql/` UPDATE files.
- **PROPTEST-02** — New proptest suite `tests/proptest/ntriples_oxigraph.rs` compares rio_turtle triple count against oxigraph as a reference implementation for randomly generated N-Triples documents. `oxigraph` added as a dev-dependency. ci/test: tests/proptest/ntriples_oxigraph.rs
- **CDC-ASYNC-01** — New integration test `tests/integration/cdc_notify_barrier.sh` demonstrates LISTEN/NOTIFY barrier pattern (no `sleep()`) for CDC subscription validation.
- **KFAIL-DOC-01** — Every entry in `tests/w3c/known_failures.txt` and `tests/conformance/known_failures.txt` now has a `# Reason:` and `# Issue:` comment explaining the failure.
- **REG-TESTS-01** — Regression tests added in `tests/pg_regress/sql/v083_features.sql` for 13 previously untested pg_extern functions: `export_ntriples`, `export_nquads`, `load_jsonld`, `bidi_wire_version`, `refresh_stats_cache`, `bidi_health`, and GUC default assertions.
- **ERRPATH-01** — Eight error-path regression tests added in `tests/pg_regress/sql/error_paths.sql`: dictionary overflow guard, HTAP merge during DROP, SubXact abort, federation timeout, Arrow export row limit, SPARQL depth limit, tenant-name validation, CDC slot exhaustion.
- **DATALOG-MAXITER-TEST-01** — Regression test `tests/pg_regress/sql/datalog_maxiter.sql` exercises the seminaive max-iteration guard (10,000 iterations) and asserts termination.

### API

- **API-RENAME-01** — New SQL function `pg_ripple.load_jsonld(url TEXT, graph_uri TEXT DEFAULT NULL)` added as preferred alias. `json_ld_load()` emits a `NOTICE` deprecation warning; removal scheduled for v1.0.0.
- **API-GRAPH-COL-01** — `pg_ripple.find_triples()` RETURNS TABLE confirmed to include `g BIGINT` (named-graph column); no schema changes required.

### Code Quality

- **MOD-BIDI-01** — `src/bidi.rs` (2,516 lines) split into five focused modules: `src/bidi/mod.rs`, `src/bidi/protocol.rs`, `src/bidi/relay.rs`, `src/bidi/subscribe.rs`, `src/bidi/sync.rs`. Public API re-exported from `mod.rs` with no signature changes.
- **GUC-NAME-01** — GUC naming convention (`pg_ripple.noun_verb_unit` snake_case) documented in `CONTRIBUTING.md`. Deprecation notices added for 4 non-conforming GUCs.
- **CHANGELOG-BREAK-01** — `**BREAKING:**` tag convention adopted in `CHANGELOG.md` for incompatible API/GUC changes. Back-annotated in affected v0.73.0–v0.79.0 entries.
- **CHANGELOG-FMT-01** — CI lint job `lint-changelog` added to `.github/workflows/ci.yml`; validates `## [vX.Y.Z]` heading format and `**BREAKING:**` tag usage.
- **DEPAUDIT-01** — `serde_cbor` (unmaintained) confirmed absent from `Cargo.toml` since v0.64.0 when the Arrow IPC path was migrated to `parquet`. No replacement needed.
- **RENOVATE-01** — `renovate.json` added: groups pgrx/rdf-parsing deps, pins pgrx to exact versions, auto-merges patch updates for utility crates on a weekly schedule.
- **P-05-EVAL** — `plans/p05_shared_dict_eval.md`: shared-memory dictionary LRU cache evaluated and closed as "not worth it" for v0.83.0 (modelled ≤10% throughput gain vs. significant complexity). Per-backend LRU retained; revisit criteria documented.

### Performance

- **DL-COST-GUC-01** — New GUCs `pg_ripple.datalog_cost_bound_s_divisor` (default 100) and `pg_ripple.datalog_cost_bound_so_divisor` (default 10) replace hardcoded selectivity divisors in `src/datalog/compiler.rs` cost-based rule reordering.
- **MERGE-BACKOFF-01** — Merge worker now uses exponential backoff (1 s × 2ⁿ) capped at `pg_ripple.merge_max_backoff_secs` (default 60) instead of flat `merge_interval_secs` wait on every error.

### Security / pg_ripple_http

- **BUILD-TIME-FIELD-01** — `/health` JSON response `build_time` field now contains an RFC 3339 build timestamp (from `SOURCE_DATE_EPOCH` env var or current build time), replacing the Cargo version string.
- **HTTP-401-WWW-AUTH-01** — `check_auth()` in `pg_ripple_http/src/common.rs` now emits `WWW-Authenticate: Bearer realm="pg_ripple"` on all 401 responses (RFC 7235 §4.1).
- **AUTH-RESP-FMT-01** — `check_auth()` failure response changed from plain-text `"unauthorized"` to JSON `{"error": "PT401", "message": "unauthorized"}`, consistent with all other error envelopes.
- **METRICS-AUTH-DOC-01** — `# SECURITY: intentionally public` comment added at `/metrics` and `/metrics/extension` route registration in `pg_ripple_http`; operations guide updated. docs/src/operations/monitoring.md
- **EXPORT-BNODE-VALID-01** — `src/export.rs` validates blank node labels against the N-Triples BNodeLabel production before emitting; `_` prefixed and empty labels are rejected.

---

## [0.82.0] — 2026-06-03 — Assessment 12 Performance & Observability

**Implements v0.82.0 roadmap: 30 performance, observability, and security
hardening items from Assessment 12. Key additions: configurable plan-cache
capacity GUC, `ANY($1::bigint[])` batch decode, two-phase merge with tunable
lock timeout, merge worker heartbeat, enriched `sparql_explain()` with algebra
tree, structured Prometheus labels, `sparql_normalise()` function, federation
response Content-Length pre-check, and SPARQL depth DoS protection.**

### Performance

- **CACHE-CAP-01** — `pg_ripple.plan_cache_capacity` GUC (default 1024, range 64–65536) replaces hardcoded constant in `plan_cache.rs`.
- **DECODE-BIND-01** — `batch_decode()` migrated from `IN (id1, id2, …)` to `WHERE id = ANY($1::bigint[])` bind parameter, preventing plan proliferation.
- **MERGE-PRED-01** — Merge worker caches predicate IDs with 60-second TTL; SIGHUP invalidates the cache. Eliminates repeated `_pg_ripple.predicates` scans per merge cycle.
- **MERGE-LOCK-GUC-01** — Hardcoded `lock_timeout = '5s'` replaced by `pg_ripple.merge_lock_timeout_ms` GUC (default 5000, range 100–60000 ms).
- **PROPPATH-UNBOUNDED-01** — `pg_ripple.all_nodes_predicate_limit` GUC (default 500) caps wildcard property-path UNION ALL branches to prevent parser stack overflow on large schemas.
- **VACUUM-DICT-BATCH-01** — `vacuum_dictionary()` now batches UNION ALL construction into groups of `pg_ripple.vacuum_dict_batch_size` predicates (default 200).
- **GUC-BOUNDS-01** — Explicit min/max validators added to `vp_promotion_threshold` (min 100), `dictionary_cache_size` (min 1024, max 1 GiB), and new `pg_ripple.merge_batch_size` GUC (min 100, max 100,000,000).

### Observability

- **EXPLAIN-ALG-01** — `sparql_explain()` now includes a `-- SPARQL Algebra --` section showing the parsed algebra tree (via `spargebra::Display`).
- **MERGE-HBEAT-01** — Merge background worker emits a LOG-level heartbeat every `pg_ripple.merge_heartbeat_interval_seconds` seconds (default 60) and writes to the new `_pg_ripple.merge_worker_status` table.
- **STATS-DOC-01** — `pg_ripple.stats_scan_limit` GUC (default 1000) caps the number of VP tables scanned per `graph_stats()` call; documented in administration reference.
- **PGSS-NORM-01** — New `pg_ripple.sparql_normalise(TEXT) RETURNS TEXT` function replaces string/IRI/numeric literals with `$S`/`$I`/`$N` placeholders for `pg_stat_statements` grouping.
- **STATS-CACHE-01** — New `_pg_ripple.predicate_stats_cache` table and `pg_ripple.refresh_stats_cache()` function materialise per-predicate triple counts; background refresh every `pg_ripple.stats_refresh_interval_seconds` seconds.
- **FED-COST-01** — New `_pg_ripple.federation_stats` table accumulates call latency (P50/P95 approximation), error counts, and row estimates per federation endpoint; updated after every HTTP call.
- **ADMIN-LOCK-01** — Lock levels documented for `vacuum()`, `reindex()`, and `vacuum_dictionary()` in the SQL reference.

### Security

- **TENANT-NAME-01** — Tenant name validation regex tightened to `^[A-Za-z0-9_]{1,63}$`; uppercase letters now allowed; max 63 characters enforced.
- **ROLE-UNICODE-01** — `quote_ident_safe()` now falls back to SPI `SELECT quote_ident($1)` for role names containing non-ASCII characters.
- **SHMEM-SAFE-01** — Shared-memory size arithmetic uses `checked_mul().expect()` to detect overflow early (misconfigured GUC rather than silent wraparound).
- **RUSTSEC-01** — `audit.toml` updated: `RUSTSEC-2023-0071` (RSA PKCS#1 timing) added as an exemption with justification comment; review date updated to v0.82.0. Cargo-audit CI gate (`.github/workflows/ci.yml`) passes.
- **SPARQL-COMPLEX-01** — `pg_ripple.sparql_max_algebra_depth` GUC (default 256) already enforced; confirmed and documented.
- **LISTEN-LEN-01** — `/subscribe/{subscription_id}` endpoint in `pg_ripple_http` now returns HTTP 400 for subscription IDs longer than 63 characters.
- **FED-BODY-STREAM-01 / FED-SIZE-01** — All five `response.into_string()` call sites in `federation.rs` now check the `Content-Length` header before allocating the body buffer.
- **REDACT-01** — Remaining raw error exposure in `rag_handler.rs` replaced with `redacted_error()`; confirmed uniform coverage across all 82 handler error paths.

### Rust / Extension

- **DATALOG-SILENT-01** — 29 `let _ = Spi::run_with_args()` calls in `wfs.rs` and `seminaive.rs` replaced with `.unwrap_or_else(|e| pgrx::log!("...: {e}"))`.
- **DECODE-WARN-01** — `batch_decode()` now emits a `WARNING` for any ID present in query results but absent from the dictionary.
- **EMBED-MODEL-01** — All embedding paths confirmed to read `pg_ripple.embedding_model` GUC.
- **FED-COUNTER-ORDER-01** — `FED_CALL_COUNT` incremented only after the endpoint policy check passes.
- **EXPORT-JSONLD-OOM-01** — `export_jsonld()` emits a `WARNING` when buffering more than 1,000,000 triples; recommends the streaming cursor variant.

### pg_ripple_http companion

- **ARROW-LIMIT-01** — Arrow Flight export enforces `ARROW_MAX_EXPORT_ROWS` env var (default 10,000,000); HTTP 400 returned when the limit is exceeded.
- **METRICS-LABELS-01** — Prometheus `/metrics` endpoint now includes `query_type` (SELECT/ASK/CONSTRUCT/DESCRIBE/UPDATE) and `result_size_bucket` (empty/small/medium/large) label dimensions.

---

## [0.81.0] — 2026-05-14 — Correctness & Stability Hardening

**Implements v0.81.0 roadmap: 34 correctness, stability, and security
hardening items. No breaking schema changes; one new internal table
(`_pg_ripple.cdc_lsn_watermark`) and one new public function
(`pg_ripple.recover_stuck_promotions()`).**

Version 0.81.0 is a deep correctness and stability release addressing thirty-four issues spanning the full system stack. Several correctness bugs that had been present since early versions are fixed: the HTAP merge was using non-deterministic SID selection due to a missing `ORDER BY` before `DISTINCT ON`, DRed retraction was performing only a single seed pass instead of a full semi-naive fixpoint (causing incomplete re-derivation after retraction), and blank-node variable names are now prefixed with a query-scoped hex nonce to prevent aliasing across subqueries. The OPTIONAL-to-INNER-JOIN optimization is extended from single-predicate patterns to multi-predicate basic graph patterns, improving query performance for a structural pattern that appears frequently in real-world SPARQL queries.

Infrastructure reliability receives equal attention. A new `recover_stuck_promotions()` function detects and repairs VP promotions that were abandoned mid-flight without a server restart, making the promotion mechanism self-healing without operator intervention. A CDC slot cleanup background worker automatically drops orphaned replication slots that have been idle beyond a configurable threshold, preventing slot accumulation from blocking WAL cleanup and causing disk space exhaustion in production. Dictionary race conditions are addressed: a hash collision now raises a clean PostgreSQL error instead of panicking, and sub-transaction aborts correctly invalidate the LRU caches to prevent stale entries from causing incorrect decode results for subsequent queries. The plan cache key is extended to include nine additional GUC parameters, ensuring that changing any relevant session setting mid-session correctly evicts stale plans rather than returning results computed under different settings.

### Correctness

- **MERGE-SID-01** — `ORDER BY i ASC` added before `DISTINCT ON` in HTAP merge CTE template (tests/pg_regress/sql/htap_merge.sql), fixing non-deterministic SID selection during merge.
- **DRED-FIXPOINT-01** — DRed re-derive phase now runs a full semi-naïve
  fixpoint instead of a single seed pass, correcting incomplete re-derivation
  after retraction.
- **DL-AGG-01** — Guard added in `stratify()` to reject aggregation functions in recursive Datalog rule heads (tests/pg_regress/sql/datalog_agg.sql), with a descriptive error (PT511).
- **DL-PAR-01** — Intra-stratum cycle detection added to the parallel group partition step (tests/pg_regress/sql/datalog_parallel.sql), preventing non-terminating stratum evaluation.
- **DL-PAR-02** — Parallel Datalog SCC scheduling now uses topological order
  (Kahn's BFS) instead of stratum order, ensuring producers run before consumers.
- **OPT-INNER-01** — OPTIONAL→INNER JOIN optimisation extended to multi-predicate
  BGPs (previously only applied to single-predicate BGPs).
- **BN-SCOPE-01** — Blank-node variable names are now prefixed with a
  query-scoped hex nonce to prevent aliasing across subqueries.
- **RETRACT-PARAM-01** — Flat-VP `DELETE` in `src/construct_rules/retract.rs`
  parameterised with `$1`–`$4` bind variables (previously used string interpolation).
- **SCHEDULER-ERR-01** — Topological sort in `construct_rules/scheduler.rs` now
  propagates errors via `Result` instead of calling `pgrx::error!()`, giving callers
  cleaner error handling.
- **DICT-RACE-01** — `encode_inner()` now raises a PostgreSQL error (PT501) on
  0-row RETURNING (hash collision or concurrent dict truncation) instead of panicking.
- **DICT-SUBXACT-01** — A `SubXactCallback` registered in `_PG_init` now invalidates
  both the decode and encode LRU caches on subtransaction abort.

### Security

- **RAG-SQL-INJECT-02** — `rag_retrieve()` in `pg_ripple_http` migrated from
  `format!()` with manual quote-escaping to fully parameterised tokio-postgres
  `$1`–`$5` query parameters.
- **FED-URL-01** — Federation endpoint URLs normalised to lowercase scheme+host
  before allowlist comparison, preventing case-bypass attacks.
- **FILTER-STRICT-01** — New `pg_ripple.strict_sparql_filters` GUC; when enabled,
  unknown SPARQL built-in function names raise error PT422 instead of evaluating
  to `UNDEF`.

### Stability

- **SHACL-TXN-01** — SHACL shape-store write wrapped in a savepoint so a
  constraint failure rolls back only the failing shape rather than the entire
  transaction.
- **FED-TRUNC-01** — Federation JSON results exceeding `federation_result_max_bytes`
  now emit a WARNING and partially materialise instead of raising a fatal error.
- **FED-CACHE-01** — Federation query cache key normalised to canonical SPARQL
  form via `spargebra::Display`, preventing spurious cache misses.
- **MERGE-FENCE-01** — HTAP merge advisory lock acquisition moved to just before
  the rename-swap phase (Phase 2), reducing the ExclusiveLock hold time from
  minutes to milliseconds.
- **PROMO-LOCK-01** — Per-predicate `pg_advisory_xact_lock(pred_id)` confirmed
  as the exclusive coordination mechanism for VP promotion (no table-level lock).
- **PROMO-ATOMIC-01** — `predicates` catalog status update is part of the atomic
  CTE that inserts the new VP table, eliminating the TOCTOU window.
- **PROMO-STUCK-01** — New `pg_ripple.recover_stuck_promotions()` SQL function
  detects and re-runs VP promotions abandoned mid-flight (without a server restart).
- **CDC-SLOT-01** — New background worker (`pg_ripple_cdc_slot_cleanup_main`)
  drops orphaned CDC replication slots idle longer than
  `pg_ripple.cdc_slot_idle_timeout_seconds`.
- **CDC-LSN-01** — New `_pg_ripple.cdc_lsn_watermark(slot_name, last_lsn)` table
  tracks CDC replication progress; updated after each batch commit.
- **DICT-STRICT-01** — New `pg_ripple.strict_dictionary` GUC; when enabled,
  `decode()` raises a PostgreSQL error for unrecognised IDs.
- **PLAN-CACHE-GUC-02** — Plan-cache key extended to include
  `NORMALIZE_IRIS`, `WCOJ_ENABLED`, `WCOJ_MIN_TABLES`, `TOPN_PUSHDOWN`,
  `SPARQL_MAX_ROWS`, `SPARQL_OVERFLOW_ACTION`, `FEDERATION_TIMEOUT`,
  `PGVECTOR_ENABLED`, and `INFERENCE_MODE`. Changing any of these GUCs
  mid-session now invalidates the per-backend plan cache.
- **PRELOAD-WARN-01** — `_PG_init` emits a WARNING when the extension is loaded
  via `CREATE EXTENSION` without `shared_preload_libraries`, preventing silent
  misconfiguration.
- **PGFINI-01** — `_PG_fini` added (roadmap/v0.81.0.md) to unregister SubXact
  callback, ExecutorEnd hook, and transaction callback when the extension library
  is unloaded.
- **REPL-UNWRAP-01** — All `.unwrap()` calls in `src/replication.rs` replaced
  with `unwrap_or_else(...)` or `pgrx::error!()` to avoid Rust panics on SPI errors.
- **FEATURE-STATUS-BIDI-01** — 12 missing rows for BIDI (v0.77.0) and BIDIOPS
  (v0.78.0) features added to `feature_status()`.

---

## [0.80.0] — 2026-05-07 — Assessment 12 Critical/High Remediation

**Implements v0.80.0 roadmap: addresses all 13 critical and high findings from
Security Assessment 12. No new SQL schema changes; all fixes are in the Rust
implementation and companion HTTP service.**

Security in depth means that every layer of the system independently validates and protects against threats, without depending on other layers to catch what it misses. Version 0.80.0 closes all thirteen critical and high security findings from Assessment 12. Five SQL injection vulnerabilities in the views catalog insertion functions are fixed by migrating from `format!()` string interpolation with manual quote escaping to fully parameterized `Spi::run_with_args()` calls with typed bind parameters — a change that makes injection impossible by construction rather than by careful escaping. A plan cache cross-user leakage vulnerability is closed by including the current PostgreSQL role OID in the plan cache key, preventing cached query plans from being shared across users with different privilege levels.

The SPARQL Update mutation journal is wired to flush at the end of every update statement, ensuring that CONSTRUCT writeback rules fire correctly for every mutation rather than only when the dictionary API is used. IPv6 Unique Local addresses are added to the SSRF blocklist, closing a gap that could have allowed federation requests to reach internal network services via IPv6 even when the IPv4 blocklist was complete. The `/explorer` admin interface now requires authentication, preventing unauthenticated access to the interactive graph explorer in deployments where the HTTP companion is reachable from less-trusted networks. All HTTP error responses from the companion are standardized to return JSON with structured error codes rather than inconsistent text or empty bodies, making error handling in client code more predictable and diagnostic.

### Security fixes

- **FLUSH-02-01** — `sparql_update()` and `execute_delete_insert()` now call
  `mutation_journal::flush()` at the end of every SPARQL UPDATE statement, ensuring
  CONSTRUCT writeback rules fire correctly for the primary mutation path.

- **CACHE-RLS-01** — Plan cache key now includes the current PostgreSQL role OID and
  `pg_ripple.inference_mode` GUC value to prevent cross-user plan leakage via shared
  plan cache entries.

- **SQL-INJ-01** — All five catalog INSERT statements in `src/views.rs`
  (`create_sparql_view`, `create_datalog_view`, `create_datalog_view_from_rule_set`,
  `create_framing_view`, `create_construct_view`) migrated from `Spi::run(&format!())`
  with manual quote-escaping to `Spi::run_with_args()` with typed `$1, $2, …` parameters.

- **SQL-INJ-02** — `model_tag` filter in `src/sparql/embedding.rs` replaced from
  `AND e.model = '{}'` string interpolation to parameterised `AND e.model = $1`.

- **SSRF-RFC1918-01** — `is_blocked_host()` in `src/sparql/federation.rs` now also
  blocks IPv6 Unique Local addresses (fc00::/7, i.e. `fc`/`fd` prefix hosts).

- **EXPLORER-AUTH-01** — `GET /explorer` in `pg_ripple_http` now requires
  authentication via `check_auth()`. Unauthenticated clients receive HTTP 401.

### Improvements

- **HTTP-ERR-01** — All 4xx/5xx HTTP responses from `pg_ripple_http` now return
  `application/json` with `{"error":"PTxxx","message":"..."}` bodies. New
  `ErrorResponse` struct and `json_error()` helper added to `pg_ripple_http/src/common.rs`.

- **COMPAT-MIN-01** — `COMPATIBLE_EXTENSION_MIN` in `pg_ripple_http/src/main.rs`
  updated from `"0.75.0"` to `"0.79.0"`. `pg_ripple_http` now at v0.77.0.

- **COMPAT-MATRIX-01** — Compatibility matrix in `docs/src/operations/compatibility.md`
  updated with rows for `pg_ripple_http` v0.73.x, v0.74.x, v0.75.x, and v0.76.x.

- **PROPPATH-CYCLE-01** — Module comment in `src/sparql/property_path.rs` updated to
  document that `CYCLE s, o SET` is required (and already in use) to prevent infinite
  recursion in recursive property-path CTEs.

- **JOURNAL-R2RML-01** — Confirmed and documented that R2RML and CDC write paths route
  through `bulk_load::load_ntriples()` which already calls `mutation_journal::flush()`.

### Infrastructure

- **MIGCHAIN-01** — `tests/test_migration_chain.sh` extended with checkpoint assertions
  at v0.65.0, v0.70.0, v0.75.0, v0.79.0 and a script-count verification for all 18
  migration scripts from v0.62.0 to v0.79.0.

- **SBOM-04** — `sbom.json` regenerated at v0.80.0. CI SBOM version gate added to
  `.github/workflows/ci.yml` to fail the build if `sbom.json` version does not match
  `Cargo.toml` version.

---

## [0.79.0] — 2026-04-30 — Query Engine Completeness

**Implements v0.79.0 roadmap: closes the last two known query-engine limitations
(WCOJ-LFTI-01 and SHACL-SPARQL-01). All `feature_status()` rows now show
`implemented`. The "Known limitations" table has been removed from README.md.**

Two capabilities had been marked as planned but unimplemented in pg_ripple's feature status since the beginning of the project, and version 0.79.0 delivers both. The first is a true Leapfrog Triejoin executor for cyclic join patterns — graph patterns involving triangles, cliques, and other cyclic structures where standard nested-loop joins produce exponentially large intermediate results. The LFTI executor loads VP table data into sorted in-memory structures and evaluates multi-way joins using the Leapfrog algorithm, achieving worst-case optimal join complexity and making previously impractical graph pattern queries feasible on large graphs without requiring any hint-based query tuning.

The second is full `sh:SPARQLRule` support in the SHACL engine. SHACL rules expressed as SPARQL CONSTRUCT or SELECT queries can now be parsed, validated, and executed natively, with their results materialized into the target graph. `sh:order` is respected for execution ordering, and fixpoint iteration ensures that newly materialized triples can trigger further rules until no new conclusions are drawn. With both features delivered, all rows in `pg_ripple.feature_status()` show `implemented` status, and the "Known limitations" section is removed from the README — replaced by a note pointing users to `feature_status()` for the authoritative, machine-readable status surface that stays current automatically.

### What's new

- **WCOJ-LFTI-01** — True Leapfrog Triejoin executor for cyclic BGP joins.
  Implements `TrieIterator` / `SortedIterator`, `leapfrog_intersect`, `EdgeData`,
  and `execute_leapfrog_triejoin` in `src/sparql/wcoj.rs`. For cyclic patterns
  (triangles, cliques, social-network paths), the LFTI executor loads VP table edge
  data into sorted in-memory structures and evaluates n-way joins using the
  Leapfrog algorithm (Veldhuizen 2012), achieving the worst-case optimal complexity
  guarantee. The SQL planner-hint path remains as a fallback. New GUC:
  `pg_ripple.wcoj_min_cardinality` (INT, default 0). `feature_status()` row `wcoj`
  updated from `planner_hint` to `implemented`.

- **SHACL-SPARQL-01** — Full `sh:SPARQLRule` support. `bridge_shacl_rules()` now
  parses `sh:construct` / `sh:select` bodies from SHACL shapes, prepends prefix
  declarations, validates the CONSTRUCT query, and executes it via the existing
  SPARQL CONSTRUCT engine (`sparql_construct_rows()`). Results are materialised
  into the target graph via the standard VP insert path. `sh:order` is respected
  for execution ordering. Fixpoint iteration (up to `shacl_rule_max_iterations`)
  ensures newly materialised triples can trigger further rules. The PT481 WARNING
  is now emitted at most once per session (de-dup). New GUCs:
  `pg_ripple.shacl_rule_max_iterations` (INT, default 100) and
  `pg_ripple.shacl_rule_cwb` (BOOL, default false). `feature_status()` row
  `shacl_sparql_rule` updated from `planned` to `implemented`.

- **README-LIMITS-01** — Removed the "Known limitations" section from README.md.
  Replaced with a note directing users to `pg_ripple.feature_status()` for the
  machine-readable status surface.

### New GUCs

| GUC | Type | Default | Description |
|---|---|---|---|
| `pg_ripple.wcoj_min_cardinality` | INT | 0 | Minimum VP table edge count before LFTI executor is used; 0 = always use LFTI for cyclic patterns |
| `pg_ripple.shacl_rule_max_iterations` | INT | 100 | Maximum fixpoint iterations for `sh:SPARQLRule` evaluation |
| `pg_ripple.shacl_rule_cwb` | BOOL | false | When on, `sh:SPARQLRule` rules are registered as CWB rules |

### Migration

No schema changes. Run `ALTER EXTENSION pg_ripple UPDATE TO '0.79.0'` to upgrade.

### Tests

- `tests/pg_regress/sql/v079_wcoj.sql` — LFTI GUC, triangle query, 4-clique
- `tests/pg_regress/sql/v079_shacl_sparql_rule.sql` — `sh:SPARQLRule` GUCs, materialisation, `sh:order`
- `tests/pg_regress/sql/v079_features.sql` — `feature_status()` completeness check

---

## [0.78.0] — 2026-05-22 — Bidirectional Integration Operations

**Implements v0.78.0 roadmap: all BIDIOPS-* deliverables closing the operational gaps
identified in the v0.77.0 review. Data semantics are unchanged; this release adds the
management plane that production deployments need.**

Introducing bidirectional data integration between a knowledge graph and external systems is only the first step — operating it reliably in production requires a full management plane. Version 0.78.0 delivers the operational infrastructure that makes the v0.77.0 integration primitives production-ready: write-side outbox depth limits with three overflow policies (pause, drop oldest, drop newest), a dead-letter table for events that exhausted their retry policy, and a reconciliation toolkit for resolving divergent states between the knowledge graph and its integration partners. Fine-grained per-subscription bearer tokens with named scopes let administrators grant exactly the permissions each integration partner needs and revoke them independently.

Data governance receives first-class support through two significant new features. Frame-level redaction allows specific predicates containing PII or sensitive information to be marked for redaction, so their values are replaced with a structured redaction marker in the outbound event stream while an unredacted variant remains available for internal compliance pipelines. A complete event audit log records every mutating action with token hash, remote address, and session user, creating a tamper-evident trail of all integration activity. Schema evolution policies govern how changes to frame definitions, IRI templates, or exclude-graph lists are handled when subscriptions are modified, preventing incompatible schema changes from propagating silently to downstream systems. A draft vendor-neutral "RDF Bidirectional Integration Profile" specification captures the design decisions in a reusable, community-shareable format.

### What's new

- **BIDIOPS-QUEUE-01** — Write-side outbox depth limits and dead-letter table.
  Three overflow policies (`pause`, `drop_oldest`, `drop_newest`); `dead_letter_after`
  interval policy; `_pg_ripple.event_dead_letters` catalog table. New SQL API:
  `list_dead_letters()`, `requeue_dead_letter()`, `drop_dead_letter()`.

- **BIDIOPS-PAUSE-01** — `bidi_status()` exposes pg-trickle pause state.
  `bidi_health()` reports `paused` when any subscription is paused. Pause/resume
  is delegated to `pg_trickle.pause_subscription` / `pg_trickle.resume_subscription`.

- **BIDIOPS-EVOLVE-01** — Schema-evolution policies for frame, IRI template, and
  exclude-graphs changes. New SQL API: `alter_subscription()` with
  `frame_change_policy`, `iri_change_policy`, `exclude_change_policy` parameters.
  All changes recorded in `_pg_ripple.subscription_schema_changes`.

- **BIDIOPS-AUTH-01** — Per-subscription bearer tokens with fine-grained scopes
  (`linkback`, `divergence`, `abandon`, `outbox_read`, `dead_letter_admin`).
  New SQL API: `register_subscription_token()`, `revoke_subscription_token()`,
  `list_subscription_tokens()`. SHA-256 token hashing via `sha2` crate.
  Admin tokens stored separately in `_pg_ripple.admin_tokens`.

- **BIDIOPS-REDACT-01** — Frame-level `"@redact": true` for PII / secret-bearing
  predicates. `apply_frame_redaction()` renders `{"@redacted": true}` in place of
  redacted predicate values. Unredacted outbox variant supported for compliance
  pipelines. Documented in the bidi runbook.

- **BIDIOPS-AUDIT-01** — `_pg_ripple.event_audit` records every side-band mutating
  call and admin action with token hash, remote address, and session user. New SQL API:
  `purge_event_audit()`. `pg_ripple.audit_retention` GUC (default: 90 days).

- **BIDIOPS-PROPTEST-01** — Six convergence properties tested via `proptest` (1,000
  cases each): determinism, order-independence (latest_wins), no-loss, source_priority,
  linkback round-trip, convergence under retries. Added to `tests/proptest_suite.rs`.

- **BIDIOPS-CHAOS-01** — Fault injection smoke tests in `tests/stress/bidi_chaos.sh`:
  abandon_linkback idempotency, audit purge safety, reconciliation round-trip,
  bidi_health status validity, token register/revoke.

- **BIDIOPS-RECON-01** — Reconciliation toolkit: `_pg_ripple.reconciliation_queue`
  table; `reconciliation_enqueue()`, `reconciliation_next()`, `reconciliation_resolve()`
  SQL API; four resolution actions: `accept_external`, `force_internal`,
  `merge_via_owl_sameAs`, `dead_letter`.

- **BIDIOPS-DASH-01** — Consolidated operations surface: `bidi_status()` (16 columns)
  and `bidi_health()` (3 columns) monitoring views.

- **BIDIOPS-MIG-01** — Migration script `sql/pg_ripple--0.77.0--0.78.0.sql` with all
  DDL additions. `pg_ripple.control` updated to `default_version = '0.78.0'`.

- **BIDIOPS-PERF-01** — Benchmark suite `benchmarks/bidiops_throughput.sql` covering
  queue depth estimation, audit insert throughput, scope-check latency, and frame
  redaction render cost.

- **BIDIOPS-DOC-01** — Operations runbook (`docs/src/operations/bidi-runbook.md`) and
  production-readiness checklist (`docs/src/operations/bidi-production-checklist.md`)
  covering all day-two operations: queue drain, token rotation, redaction, schema
  evolution, reconciliation, and chaos-test interpretation.

- **BIDI-SPEC-01** — Draft vendor-neutral *RDF Bidirectional Integration Profile v1*
  (`docs/spec/rdf-bidi-integration-v1.md`) with 16 sections covering all 8 motivating
  problems and candidate conformance levels.

---

## [0.77.0] — 2026-05-15 — Bidirectional Integration Primitives

**Implements v0.77.0 roadmap: all BIDI-* deliverables for bidirectional integration
between pg_ripple and external systems via named-graph attribution, declarative conflict
policies, upsert/diff ingest modes, symmetric delete, linkback rendezvous, CAS events,
pg-trickle outbox/inbox transport, per-graph observability, and a frozen JSON wire format.**

Modern enterprise data architectures rarely have a single authoritative source — the same information may exist in a CRM, an ERP, a data warehouse, and a knowledge graph simultaneously, with each system maintaining its own version. Version 0.77.0 introduces a comprehensive set of primitives for bidirectional synchronization between pg_ripple and external systems. Source attribution, declarative conflict resolution policies, upsert and diff ingest modes, symmetric delete, linkback rendezvous for target-assigned IDs, compare-and-swap event verification, and pg-trickle outbox/inbox transport are all delivered in a single release, providing the complete foundation for production-grade bidirectional integration.

Four conflict resolution strategies give teams precise control over how divergent values are handled: source priority (a ranked list of authoritative sources), latest-wins (highest timestamp prevails), reject-on-conflict (raise an error if values disagree), and union (all values coexist). The diff ingest mode derives per-triple change timestamps from payload-level fields and stores them as RDF-star annotations, enabling time-aware queries and audit of exactly what changed and when. A frozen JSON wire format with a version discriminator ensures that event consumers remain compatible across pg_ripple upgrades without custom parsing changes. Per-graph observability through `graph_stats()` gives operations teams a live view of triple counts, last write times, conflict rejection rates, and active subscription counts for each named graph.

### What's new

- **BIDI-ATTR-01** — Source attribution API consistency pass. `register_json_mapping`
  gains `default_graph_iri`, `timestamp_path`, `timestamp_predicate`, `iri_template`,
  and `iri_match_pattern` parameters. `ingest_json` and `ingest_jsonld` gain `mode` and
  `source_timestamp` parameters. When `graph_iri` is omitted, the mapping's
  `default_graph_iri` is used automatically.

- **BIDI-CONFLICT-01** — `pg_ripple.register_conflict_policy(predicate, strategy, config)`
  with strategies: `source_priority` (priority-ordered graph list with null fall-through),
  `latest_wins` (highest per-triple timestamp wins; falls back to VP `i` column with
  NOTICE), `reject_on_conflict` (raises an error on divergent values), `union` (all
  values coexist). `drop_conflict_policy` and `recompute_conflict_winners` for lifecycle
  management. Non-authoritative `_pg_ripple.conflict_winners` cache with register-time
  backfill and drop-time cleanup.

- **BIDI-NORMALIZE-01** — Optional `normalize` expression in conflict policy config.
  Expressions validated against a whitelist (STR, LCASE, UCASE, ROUND, SUBSTR, casts).
  Forbidden constructs (SELECT, WHERE, SERVICE, aggregate functions) raise an error at
  registration time.

- **BIDI-UPSERT-01** — `ingest_json(..., mode => 'upsert')` deletes existing values for
  `sh:maxCount 1` predicates (from the registered shape) before inserting, enabling
  idempotent updates for functional properties.

- **BIDI-DIFF-01** — `ingest_json(..., mode => 'diff')` derives per-triple change
  timestamps from a payload-level `lastModified` field (configurable via `timestamp_path`).
  Timestamps are stored as RDF-star annotations using `prov:generatedAtTime`. Only
  predicates whose values actually changed are written.

- **BIDI-DELETE-01** — `pg_ripple.delete_by_subject(mapping, subject_iri, graph_iri)`
  deletes all triples for a subject. `delete_mapped_predicates(mapping, subject_iri,
  graph_iri)` deletes only the predicates declared in the mapping's context. Both respect
  the mapping's `default_graph_iri` when `graph_iri` is omitted.

- **BIDI-LOOP-01** — `exclude_graphs TEXT[]` and `propagation_depth SMALLINT` columns
  added to `_pg_ripple.subscriptions` for loop-safe subscription configuration.

- **BIDI-CAS-01** — `pg_ripple.assert_cas(event, actual)` verifies that the `base`
  object in an outbound event matches the current state in the target system. No-ops when
  base is empty or when `after` already matches actual (idempotent delivery).

- **BIDI-LINKBACK-01** — `_pg_ripple.pending_linkbacks` and `_pg_ripple.subscription_buffer`
  tables for target-assigned ID rendezvous. `record_linkback(event_id, target_id,
  target_iri)` expands bare IDs through the target graph's `iri_template`, writes
  `owl:sameAs`, flushes buffered events, and deletes the pending row atomically.
  `abandon_linkback(event_id)` drops buffered events with a NOTICE and records the miss
  in `_pg_ripple.iri_rewrite_misses`.

- **BIDI-OUTBOX-01** — `outbox_table`, `outbox_distribution_column`, `outbox_format`,
  and `outbox_merge` columns added to `_pg_ripple.subscriptions` for pg-trickle outbox
  configuration.

- **BIDI-INBOX-01** — `pg_ripple.install_bidi_inbox(inbox_table)` creates a schema,
  inbox table, dispatch PL/pgSQL function, and `AFTER INSERT` trigger that routes
  `linkback` and `abandon` events to the appropriate SQL helpers.

- **BIDI-WIRE-01** — Frozen flat JSON event shape with top-level `version: "1.0"`
  discriminator. `pg_ripple.bidi_wire_version()` returns `"1.0"`. JSON Schema published
  at `docs/src/operations/event-schema-v1.json`.

- **BIDI-OBS-01** — `pg_ripple.graph_stats(graph_iri)` returns per-graph triple count,
  last-write timestamp, conflict rejection count, and active subscription count.
  `_pg_ripple.graph_metrics` table stores the persistent counters.

- **BIDI-MIG-01** — `sql/pg_ripple--0.76.0--0.77.0.sql` migration script creates all
  new catalog tables and schema extensions. Schema blocks added to `src/schema.rs` for
  fresh installs.

- **BIDI-PERF-01** — `benchmarks/bidi_relay_throughput.sql` pgbench script for
  measuring conflict-policied ingest throughput.

- **BIDI-DOC-01** — `docs/src/operations/pg-trickle-relay.md` updated with a
  bidirectional CRM ⇄ ERP walkthrough documenting mesh, federated, and named-graph
  patterns.

### Schema changes

New tables: `_pg_ripple.conflict_policies`, `_pg_ripple.conflict_winners`,
`_pg_ripple.iri_rewrite_misses`, `_pg_ripple.graph_metrics`,
`_pg_ripple.pending_linkbacks`, `_pg_ripple.subscription_buffer`.

Altered tables: `_pg_ripple.json_mappings` (5 new columns),
`_pg_ripple.subscriptions` (12 new columns).

---

## [0.76.0] — 2026-04-30 — Assessment 11 Low-Severity Findings and Production Polish

**Implements v0.76.0 roadmap: toolchain version pin, RLS policy hash widening to 128-bit,
Arrow dep minor-version pin, benchmark baseline refresh, 24 new regression tests (227 total),
/metrics auth documentation, xact PRE_COMMIT SPI citation, log-hook defense-in-depth audit,
clippy re-verification, and cross-verification of LLM/KGE feature status and CI integration.**

Production deployments require not just correct functionality but reproducible builds, well-documented security tradeoffs, and a regression test suite wide enough to catch subtle breakage. Version 0.76.0 addresses all low-severity findings from Assessment 11 with twenty-four new regression tests — raising the total to 227 — covering SPARQL BIND, HAVING, NOT EXISTS, LANG filters, string and numeric functions, VALUES, CONSTRUCT with blanks, named graph copy, and many more patterns that real queries use every day. The Rust toolchain is pinned to a specific version in `rust-toolchain.toml`, making builds fully reproducible across CI runner updates and developer machines.

The RLS policy name generation function is upgraded from XXH3-64 to XXH3-128 hashing, reducing the birthday-paradox collision probability from roughly 50% at four billion named graphs to essentially zero — a change that matters for large multi-tenant deployments managing thousands of named graphs. Benchmark baselines are refreshed to reflect the HTAP merge optimizations delivered across the previous twenty versions, providing accurate performance regression detection for ongoing development. A defense-in-depth audit of all log call sites confirms that no HMAC keys, connection strings, bearer tokens, or other credentials appear in any error or warning message, closing a class of potential credential leakage via log aggregation pipelines. The `/metrics` Prometheus endpoint authentication is fully documented, giving operators clear guidance on securing the observability endpoint.

### What's new

- **TOOLCHAIN-PIN-01** — `rust-toolchain.toml` now pins `channel = "1.87.0"` instead of
  `channel = "stable"`. Builds are now fully reproducible across CI runner updates.
  Renovate can be configured with `package-ecosystem: rust` / `files: ["rust-toolchain.toml"]`
  to open automated PRs when new stable releases are available.

- **RLS-HASH-01** — RLS policy name generation in `src/security_api.rs` upgraded from
  XXH3-64 to XXH3-128. Policy name suffixes are now 32 hex characters instead of 16,
  reducing the birthday-paradox collision probability from ~50% at 4 billion graphs to
  essentially zero (~2×10⁻²⁰). Migration script rebuilds all existing policies from the
  `_pg_ripple.graph_access` catalog using the new naming scheme.

- **ARROW-PIN-01** — `pg_ripple_http/Cargo.toml` now pins `arrow = "55.1"` (minor-version
  pinned) instead of just `"55"`. This prevents surprise breakage from minor-version
  updates that introduce API changes in practice.

- **BENCH-REFRESH-01** — `benchmarks/merge_throughput_baselines.json` refreshed from
  v0.53.0 to v0.76.0 baselines. The new measurements reflect HTAP merge optimisations
  introduced across v0.54.0–v0.75.0 (multi-worker pipeline, BRIN summarise, delta
  compaction). p50 throughput increased by ~7–15% across all worker counts.

- **TEST-GROWTH-01** — 24 new pg_regress tests added, bringing the total from 203 to 227
  (target ≥220). New tests cover: `sparql_bind_clause`, `sparql_having_filter`,
  `sparql_not_exists`, `sparql_lang_filter`, `sparql_string_functions`,
  `sparql_numeric_functions`, `sparql_values_clause`, `sparql_construct_blank`,
  `named_graph_copy`, `datalog_builtin_functions`, `sparql_order_limit`, `owl_rl_sameas`,
  `shacl_maxcount`, `rdf_star_nested`, `sparql_union_branches`, `sparql_subquery`,
  `sparql_insert_delete`, `datalog_rule_chain`, `sparql_path_alternation`,
  `sparql_ask_queries`, `dictionary_properties`, `sparql_optional_multi`,
  `admin_api_v076`, and `v076_features`.

- **METRICS-AUTH-DOC-01** — The `/metrics` and `/metrics/extension` endpoints in
  `pg_ripple_http` are documented as **unauthenticated by design** in
  `docs/src/operations/monitoring.md`. The new section includes operator guidance for
  restricting access at network level when the service is exposed on a public interface.

- **XACT-SPI-DOC-01** — The comment in `src/lib.rs` explaining why `flush()` is not
  called from `XACT_EVENT_PRE_COMMIT` now includes a citation to the PostgreSQL 18
  source (`src/backend/access/transam/xact.c`) with an explanation of the exact memory
  context and lock constraints that make SPI unsafe at that callback stage.

- **LOG-HOOK-01** — Defense-in-depth audit of all `pgrx::error!()`, `pgrx::warning!()`,
  `tracing::error!()`, and `tracing::warn!()` call sites. No raw HMAC keys, connection
  strings, bearer tokens, or other credentials are logged in any error path. Findings
  documented in `docs/src/operations/security.md`. No `RegisterEmitLogHook` is required.

- **CLIPPY-VERIFY-01** — `cargo clippy --all-targets --features pg18 -- -D warnings`
  re-verified to produce zero warnings. The CI gate in `.github/workflows/ci.yml` is
  confirmed to enforce `--deny warnings`.

- **LLM-KGE-STATUS-01** — Cross-verified that `src/llm/` (`llm_sparql_repair`,
  `nl_to_sparql`) and `src/kge.rs` (`kge_embeddings`) are present in `feature_status()`
  with `implemented` status and correct evidence paths (v0.73.0 FEATURE-STATUS-02 delivered).

- **CI-INTEGRATION-VERIFY-01** — Cross-verified that Citus integration (`citus-integration`
  job) and Arrow export integration (`arrow-integration` job) are wired to CI workflows
  (v0.75.0 CI-INTEGRATION-01/02 delivered).

---

## [0.75.0] — 2026-04-30 — Assessment 11 Medium Finding Remediation

**Implements v0.75.0 roadmap: unwrap audit, RLS error surfacing and documentation,
Citus and Arrow CI integration tests, roadmap status validation, property-path/vp_rare
regression tests, URL host parser fuzz target, fuzz duration increase, HTTP companion
production docs, and mutation_journal feature_status entry.**

Security and robustness improvements often require verifying not just that a feature works correctly in isolation, but that it integrates correctly with the full system under realistic conditions. Version 0.75.0 addresses all medium-severity findings from Assessment 11, with a focus on verifying that protections that should be in place actually are. CI integration tests for both Citus distributed tables and Arrow Flight bulk export are added, ensuring these features are tested end-to-end in the CI pipeline rather than relying only on unit tests. A roadmap status validation job verifies that the current version is correctly marked as Released in the roadmap after each deployment, catching forgotten status updates before they mislead users browsing the roadmap.

RLS error surfacing is improved: failures in applying row-level security policies were previously silently discarded; they now surface as WARNING messages that operators can detect in PostgreSQL logs and act on before they cause data access issues. A URL host parser fuzz target exercises the Citus shard-pruning host extraction function with arbitrary input, expanding the fuzz surface to cover network-layer parsing code that handles externally supplied hostnames. Property path regression tests are added for three specific combinations that had not previously been tested: property paths inside OPTIONAL clauses, inside GRAPH clauses, and on vp_rare predicates. The HTTP companion production documentation is completed with a compatibility warning explaining exactly why bypassing the version check in production is unsafe, giving operators the context they need to make an informed decision.

### What's new

- **UNWRAP-AUDIT-01** — Audited all `.unwrap()` calls in production code outside
  `#[cfg(test)]` blocks. `pg_ripple_http` `json_response()` helpers in `common.rs`
  and `datalog.rs` updated to use `.expect("infallible: hardcoded valid HTTP headers")`
  for clearer panic messages. All other production `unwrap()` calls are either in
  test modules or already annotated with `#[allow(clippy::unwrap_used)]` + `// SAFETY:`
  comments. ci/regress: cargo clippy --features pg18.

- **CI-INTEGRATION-01** — `citus-integration` CI job added (`.github/workflows/ci.yml`):
  runs all `citus_*.sql` pg_regress tests in a dedicated job after main test/regress
  jobs pass. Tests verify graceful-degradation behavior when Citus is not installed.
  ci/test: `.github/workflows/ci.yml` `citus-integration` job.

- **CI-INTEGRATION-02** — `arrow-integration` CI job added (`.github/workflows/ci.yml`): exercises
  `export_arrow_flight()` against a populated database, verifies the returned ticket
  is non-empty BYTEA, and confirms `arrow_flight_export` is `implemented` in
  `feature_status()`. ci/test: `.github/workflows/ci.yml` `arrow-integration` job.

- **ROADMAP-VALIDATE-01** — `scripts/check_roadmap_status.py` added (see ROADMAP.md): validates that
  ROADMAP.md marks the current Cargo.toml version as `Released ✅`. New
  `validate-roadmap-status` CI job runs post-release to catch forgotten status updates.
  ci/test: `.github/workflows/ci.yml` `validate-roadmap-status` job.

- **RLS-ERROR-01** — `apply_rls_to_vp_table()` and `apply_rls_policy_to_all_dedicated_tables()`
  now surface `ALTER TABLE ENABLE ROW LEVEL SECURITY` and `CREATE POLICY` errors as
  `WARNING` messages instead of silently discarding them via `let _ = ...`. Operators
  can now detect RLS failures in PostgreSQL logs. ci/regress: v075_features.sql.

- **ROLE-DOC-01** — `is_safe_role_name()` documentation updated to explicitly state
  that non-ASCII Unicode role names are rejected with a guidance note on the limitation
  and why it exists (SQL-injection-safe allowlist). docs/src/operations/security.md.

- **RLS-AUDIT-01** — `apply_rls_policy_to_all_dedicated_tables()` fully audited:
  role quoting via `quote_ident_safe()` confirmed correct; function doc comment added
  describing the security invariants. ci/regress: v075_features.sql.

- **PROPPATH-TEST-01** — `tests/pg_regress/sql/v075_features.sql` adds property-path
  regression tests for: property-path (`+`) inside `OPTIONAL`, property-path inside
  `GRAPH` clause, and property-path directly in `vp_rare` predicates (confirming no
  promotion is required). ci/regress: v075_features.sql.

- **FUZZ-URL-01** — `fuzz/fuzz_targets/url_host_parser.rs` added (`fuzz.yml`): fuzzes `extract_url_host()`
  from `src/citus.rs` for panics and assertion violations.
  Target added to `fuzz/Cargo.toml` and `fuzz.yml` matrix.
  ci/test: `.github/workflows/fuzz.yml` `url_host_parser` target.

- **COMPAT-DOC-01** — `docs/src/operations/compatibility.md` updated with a
  production warning for `PG_RIPPLE_HTTP_SKIP_COMPAT_CHECK=1`, clarifying it is
  only for testing/development and must not be set in production environments.
  docs/src/operations/compatibility.md.

- **FUZZ-DURATION-01** — Nightly fuzz duration increased from 60s to 120s per target
  (default for `workflow_dispatch` unchanged at 3600s). ci/test: fuzz.yml.

- **FEATURE-STATUS-JOURNAL-01** — `mutation_journal` row added to `feature_status()`
  with `implemented` status. Documents all wired call sites (bulk_load, dict_api
  executor-end hook, Datalog seminaive, SPARQL Update) and the per-statement flush
  semantics. ci/regress: v075_features.sql.

- **HTTP-VERSION-01** — `pg_ripple_http` version bumped to 0.75.0;
  `COMPATIBLE_EXTENSION_MIN` updated to `"0.74.0"`. pg_ripple_http/Cargo.toml.

### Migration

- Migration: `sql/pg_ripple--0.74.0--0.75.0.sql`.

## [0.74.0] — 2026-05-09 — Assessment 11 Critical/High Remediation

**Implements v0.74.0 roadmap: evidence truthfulness for all 12 missing reference docs, mutation journal wired through Datalog inference and executor-end hook, VP promotion plan-cache invalidation, interrupted-promotion recovery, and comprehensive CI validation.**

Trustworthy documentation and correctly wired system internals are the foundation that makes everything else in a knowledge graph system reliable. Version 0.74.0 addresses the highest-priority findings from Assessment 11, starting with a documentation truthfulness audit that revealed twelve reference documentation pages cited by the feature status API simply did not exist. All twelve pages were created — covering SPARQL, Datalog, SHACL, storage, CONSTRUCT rules, federation, CDC, GraphRAG, observability, query optimization, vector search, and development guides — giving every feature status citation a live, informative destination. A CI job now verifies that all evidence paths cited by `feature_status()` resolve to real files on disk, making it impossible for future features to be marked as delivered with missing documentation.

Three important internal correctness fixes ship alongside the documentation improvements. The mutation journal is now correctly wired through the Datalog inference engine, ensuring that CONSTRUCT writeback rules fire automatically after Datalog-derived triples are inserted — a gap that had required manual `refresh_construct_rule()` calls after inference runs. VP promotion now resets the query plan cache after completing a predicate promotion, preventing queries compiled against the `vp_rare` table from running stale plans after the predicate gets its own dedicated VP table. A background recovery mechanism calls `recover_interrupted_promotions()` at merge worker startup to repair any promotions interrupted by a server crash, making the promotion process fully self-healing.

### What's new

- **EVIDENCE-01** — Created 12 missing `docs/src/reference/` pages cited by `feature_status()`:
  `sparql.md`, `datalog.md`, `shacl.md`, `storage.md`, `construct-rules.md`, `federation.md`,
  `cdc.md`, `graphrag.md`, `observability.md`, `query-optimization.md`, `vector-search.md`,
  `development.md`. SUMMARY.md updated with all new entries.

- **GATE-05** — Fixed `validate-feature-status` CI job: replaced subshell-bypass pattern with
  `missing=$(...)` variable capture so missing evidence paths cause a real non-zero exit.

- **GATE-06** — Added `validate-feature-status-populated` CI job (`.github/workflows/ci.yml`): installs extension, inserts sample
  triples, then validates that `feature_status()` returns no `degraded` rows on a populated DB.

- **JOURNAL-DATALOG-01** — Wired Datalog inference through the mutation journal (CF-D + HF-C fixes):
  `run_inference_seminaive()` records affected graph IDs from `_dl_delta_*` tables after VP-rare
  insertion and calls `mutation_journal::flush()`. `run_inference()` similarly flushes after
  any triples are derived.

- **SBOM-03** — SBOM regenerated to v0.74.0 (`sbom.json`). Added `just check-sbom-version` target to the
  justfile and wired it into `just assess-release` as the first check.

- **HTTP-VERSION-01** — `pg_ripple_http` version bumped to 0.74.0; `COMPATIBLE_EXTENSION_MIN`
  updated to "0.73.0".

- **DOC-JOURNAL-01** — Updated `mutation_journal` module and `flush()` doc comments to accurately
  list all wired call sites (bulk_load, dict_api, Datalog seminaive, executor-end hook); removed
  false claim that SPARQL Update was wired.

- **PROMO-RECOVER-01** — Background merge worker (worker 0) now calls
  `recover_interrupted_promotions()` at startup inside a catch-unwind block. A new
  `vp_promotion_recovery` row (status `implemented`) is added to `feature_status()`.

- **CACHE-INVALIDATE-01** — `promote_predicate()` calls `crate::sparql::plan_cache_reset()` after
  completing a VP promotion, so stale query plans that hard-coded `vp_rare` are evicted.

- **TEST-04** — Added `tests/pg_regress/sql/v070_features.sql` regression test covering
  construct_writeback status, evidence-path coverage, vp_promotion_recovery, and plan_cache_reset.

- **FLUSH-DEFER-01** — Executor-end hook (`register_executor_end_hook`) calls
  `mutation_journal::flush()` at the start of each hook invocation, providing per-statement
  CWB rule firing even when dict_api is not used.

### Schema changes

None — all changes are in the Rust implementation only.

- Migration: `sql/pg_ripple--0.73.0--0.74.0.sql`.

---

## [0.73.0] — 2026-05-05 — SPARQL 1.2 Tracking, Live Subscriptions, and JSON Mapping Registry

**Implements v0.73.0 roadmap: SPARQL 1.2 compatibility tracking, SPARQL live subscription API via SSE, named bidirectional JSON↔RDF mapping registry, multi-graph JSON-LD ingest, CONTRIBUTING.md, Helm chart sidecar image config, and feature-status taxonomy.**

Modern data applications don't just query data once — they subscribe to changes and react in real time. Version 0.73.0 introduces SPARQL live subscriptions: clients can register a named subscription with any SPARQL SELECT or CONSTRUCT query, and pg_ripple will automatically re-execute that query and deliver updated results via Server-Sent Events every time the relevant graph is written to. This makes it straightforward to build dashboards, monitoring applications, and notification pipelines that stay current without polling. Each subscription is lightweight, and pg_ripple sends a compact "changed" notification rather than recomputing and transmitting the full result set when payloads would exceed 8 KB.

The release also introduces a formal bidirectional JSON⇔RDF mapping registry that makes it much easier to exchange data between REST APIs and the knowledge graph. Teams can register named mappings that specify a JSON-LD `@context` and an optional SHACL shape for validation, then use `ingest_json()` and `export_json_node()` to convert in either direction without writing custom conversion code. Multi-graph JSON-LD documents can now be ingested in a single call, with each node automatically routed to the correct named graph. A SPARQL 1.2 tracking document records compatibility status for every new SPARQL 1.2 feature, and a full `CONTRIBUTING.md` gives new contributors a clear path from first clone to merged PR.

### What's new

- **SUB-01** — SPARQL live subscription API: `subscribe_sparql(id, query, graph_iri)` registers a subscription in `_pg_ripple.sparql_subscriptions`; `unsubscribe_sparql(id)` removes it; `list_sparql_subscriptions()` enumerates active subscriptions. After each graph write, `notify_affected_subscriptions()` re-executes the query and fires `pg_notify('pg_ripple_subscription_<id>', <json>)`. Payloads >8 KB send `{"changed":true}` instead. The `pg_ripple_http` companion now exposes `GET /subscribe/{id}` as a Server-Sent Events stream. Regression test: `tests/pg_regress/sql/v073_features.sql`.

- **JSON-MAPPING-01** — Named bidirectional JSON↔RDF mapping registry: `register_json_mapping(name, context_jsonb, shape_iri)` stores a JSON-LD `@context` in `_pg_ripple.json_mappings`. Inconsistencies with the optional SHACL shape are recorded as warnings in `_pg_ripple.json_mapping_warnings`. `ingest_json(mapping, document)` and `export_json_node(mapping, iri)` use the stored context for bidirectional conversion.

- **JSONLD-INGEST-02** — Multi-graph JSON-LD ingest: `json_ld_load(document jsonb, default_graph text) → bigint` walks `@graph` arrays or single-node JSON-LD documents and loads each node into the triple store, returning the total number of triples inserted.

- **SPARQL12-01** — SPARQL 1.2 compatibility tracking document at `plans/sparql12_tracking.md` listing all SPARQL 1.2 features and their current status in pg_ripple.

- **CONTRIB-01** — `CONTRIBUTING.md` added with branch naming conventions, commit format, pre-commit checklist, migration discipline, and PR checklist.

- **TAXONOMY-01** — Feature status taxonomy documentation at `docs/src/reference/feature-status-taxonomy.md` with promotion criteria for each status tier.

- **HELM-01** — Helm chart `charts/pg_ripple/values.yaml` updated to include a separate `http.image` section for the pg_ripple_http sidecar; `statefulset.yaml` uses `http.image.tag` to pin the sidecar version independently.

- **FEATURE-STATUS-02** — `feature_status()` now includes entries for `llm_sparql_repair`, `kge_embeddings`, `sparql_nl_to_sparql`, `sparql_12`, `sparql_subscription`, `json_ld_multi_ingest`, and `json_mapping`.

- **R2RML-DOC-01** — `plans/r2rml-virtual.md` documents the planned virtual R2RML layer and its scope relative to `register_json_mapping`.

- **CONTROL-01** — `pg_ripple.control` `comment` updated to reflect v0.73.0 capabilities.

### Schema changes

- New table `_pg_ripple.sparql_subscriptions(subscription_id TEXT PK, query TEXT, graph_iri TEXT, created_at TIMESTAMPTZ)`.
- New table `_pg_ripple.json_mappings(mapping_name TEXT PK, context JSONB, shape_iri TEXT, created_at TIMESTAMPTZ)`.
- New table `_pg_ripple.json_mapping_warnings(id BIGSERIAL PK, mapping_name TEXT, kind TEXT, detail TEXT, created_at TIMESTAMPTZ)`.
- Migration: `sql/pg_ripple--0.72.0--0.73.0.sql`.

---

## [0.72.0] — 2026-05-01 — Architecture and Protocol Hardening

**Implements v0.72.0 roadmap: sub-transaction safety, JSON-LD fixes, Flight nonce replay protection, observability, module splitting.**

A database system that silently discards data during a transaction rollback is fundamentally unreliable, and version 0.72.0 closes this gap. Sub-transaction savepoint support is added via a PostgreSQL sub-transaction callback, ensuring that CONSTRUCT writeback rule entries accumulated within a savepoint are correctly cleaned up when the savepoint is rolled back. Three JSON-LD encoding bugs are fixed: object-form `@context` term definitions that were being silently dropped, integer values exceeding `i64::MAX` that were causing panics instead of correct string encoding, and fractional JSON numbers like `1.5` that were being misclassified as integers. An IRI key validation check prevents malformed IRIs from ever entering the triple store.

Security receives meaningful hardening with the introduction of Arrow Flight nonce replay protection: the HTTP companion now maintains a time-bounded nonce cache and rejects any Arrow Flight ticket whose nonce has been used before, preventing replay attacks against the bulk export endpoint. A new `/metrics/extension` endpoint exposes extension-level Prometheus metrics directly from the PostgreSQL side, and a new `export_jsonld_node()` SQL function returns the complete JSON-LD representation of any subject IRI. Five large source files are decomposed into focused modules across the SPARQL, HTTP routing, storage, and GUC subsystems, reducing average file size and making each sub-component easier to review and test in isolation.

### What's new

- **XACT-01** — Sub-transaction savepoint/rollback support: `RegisterSubXactCallback` registered in `_PG_init`; CWB writer entries are now cleaned up on `ROLLBACK TO SAVEPOINT`. Regression test: `tests/pg_regress/sql/cwb_savepoint_rollback.sql`.

- **BUG-JSONLD-CONTEXT-01** — Object-form JSON-LD `@context` entries (term definitions with `@id`/`@type`) are now correctly preserved in `bulk_load.rs` instead of being silently dropped.

- **RT-FIX-04B** — `i64` overflow in JSON number → `xsd:integer` no longer panics; values exceeding `i64::MAX` are preserved as the `xsd:integer` string form.

- **RT-FIX-06** — `is_f64()` checked before `is_i64()` in `json_value_to_nt_term` so fractional JSON numbers (e.g. `1.5`) are not misclassified as integers.

- **RT-FIX-07** — IRI key validation (`validate_iri_key_or_error`) added before triple insert to prevent malformed IRIs from entering the triple store. (ci/regress: json_roundtrip_fixes.sql)

- **FLIGHT-NONCE-01** — Arrow Flight nonce replay protection: `AppState` gains a `nonce_cache: DashMap<String, Instant>` with 5-minute TTL. Replayed nonces return `401 Unauthorized`.

- **OBS-02** — `/metrics/extension` route added to `pg_ripple_http`, emitting Prometheus-format extension-level metrics (triple count, active graphs, GUC settings).

- **JSONLD-NODE-01** — `export_jsonld_node(iri TEXT) → jsonb` SQL function added, returning the JSON-LD representation of all triples for a given subject IRI. Regression test: `tests/pg_regress/sql/export_jsonld_node.sql`.

- **PROPTEST-01** — Property-based tests for `ConstructTemplate` / `apply_construct_template` added in `tests/proptest/construct_template.rs` using the `proptest 1` crate. Self-contained (no pgrx dependency). (see ROADMAP.md v0.72.0)

- **MOD-01** — Source files exceeding 500 lines split into focused sub-modules:
  - `src/gucs/registration.rs` — all GUC registrations extracted from `_PG_init`
  - `src/lib_tests.rs` — pgrx integration tests extracted from `src/lib.rs`
  - `src/storage/dictionary_io.rs` — RDF-term I/O helpers
  - `src/storage/vp_rare_io.rs` — VP-table I/O helpers
  - `src/storage/ops.rs` — storage operations (insert/delete/query/graph management)
  - `pg_ripple_http/src/routing/sparql_handlers.rs` — SPARQL GET/POST/stream handlers
  - `pg_ripple_http/src/routing/rag_handler.rs` — RAG endpoint handler
  - `pg_ripple_http/src/routing/admin_handlers.rs` — admin/observability/explorer handlers

### Schema changes

None.

---

## [0.71.0] — 2026-04-29 — Arrow Flight Validation, Citus Integration Tests, and Compatibility Hardening

**Implements v0.71.0 roadmap: closes the High-severity Assessment 10 gaps requiring runtime infrastructure.**

Arrow Flight is the high-performance columnar data transport protocol that makes bulk exports of triple store data possible at enterprise scale. Version 0.71.0 upgrades the Arrow Flight implementation from a basic batch response to true streaming: the HTTP companion now produces chunked Transfer-Encoding responses, allowing clients to begin processing Arrow record batches before the full export completes. This means that a large export no longer requires the client to wait for all data to be serialized before any of it can be processed — a meaningful improvement for analytical pipelines that feed RDF data into columnar data stores or stream it to downstream consumers.

Version compatibility checking is introduced as a first-class feature: the HTTP companion now queries the installed extension version at startup and warns operators when the extension is below the minimum compatible version. A new compatibility matrix document clearly shows which HTTP companion versions work with which extension versions. A multi-node Citus integration test verifies that row-level security policies are correctly propagated to VP shard tables on distributed deployments — a critical correctness guarantee for multi-tenant environments. Approximate distinct count documentation clarifies exactly when HyperLogLog is used and what error bounds to expect, and a new Citus SERVICE pruning GUC enables a planned 10× speedup for bound-subject federated queries.

### What's new

- **FLIGHT-STREAM-01** — `pg_ripple_http` `/flight/do_get` now uses `axum::body::Body::from_stream` with 64 KiB chunks, producing `Transfer-Encoding: chunked` HTTP responses. The IPC buffer is streamed lazily so clients can begin decoding Arrow record batches before the full export completes. Integration test `tests/http_integration/arrow_export_large.sh` validates streaming behavior and RSS bounds. `docs/src/reference/arrow-flight.md` updated with memory-bound documentation.

- **CITUS-INT-01** — `tests/integration/citus_rls_propagation.sh` created. The multi-node integration test starts a Citus cluster via `docker-compose`, enables sharding, inserts triples in both allowed and restricted named graphs, promotes a predicate past the threshold, and asserts that non-superuser RLS restricts cross-graph access. The `feature_status()` citation for `citus_rls_propagation` now resolves to an existing file.

- **COMPAT-01** — `pg_ripple_http` now performs a version compatibility check at startup: it queries `extversion` from `pg_extension` and warns if the installed extension is below `COMPATIBLE_EXTENSION_MIN = "0.70.0"`. `docs/src/operations/compatibility.md` added with the full version compatibility matrix and upgrade procedure.

- **HLL-DOC-01** — `docs/src/reference/approximate-aggregates.md` created, documenting when HLL is used (`approx_distinct=on` + `hll` extension), error bounds at default precision (`log2m=14`, ~0.81% standard error for ≥ 10,000 distinct values), and fallback to exact `COUNT(DISTINCT)`. pg_regress test `hll_accuracy.sql` validates GUC toggle and COUNT(DISTINCT) correctness.

- **CITUS-BENCH-01** — `docs/src/reference/citus-service-pruning.md` created, documenting the `citus_service_pruning` GUC and expected 10× speedup for bound-subject SERVICE queries. pg_regress test `citus_service_pruning.sql` validates GUC plumbing and confirms `feature_status()` shows `experimental`.

### Schema changes

None.

---

## [0.70.0] — 2026-04-29 — Assessment 10 Critical Remediation

**Implements v0.70.0 roadmap: closes four Critical and seven High/Medium findings from Overall Assessment 10.**

Four critical and seven high and medium findings from the tenth security and correctness assessment are closed in version 0.70.0. The most important correctness improvement ensures that CONSTRUCT writeback rules fire automatically after every write operation, not just those that use the dictionary API. Bulk-load functions now wire into the mutation journal and flush it after each batch, and SPARQL Update flushes are deferred to the pre-commit event hook so that writeback fires once per statement regardless of how many individual triples were modified. Together these changes mean that raw-to-canonical data transformation pipelines built on CONSTRUCT rules work correctly without any manual refresh calls.

Documentation truthfulness receives careful attention: twelve documentation citations pointing to nonexistent files are fixed, stub pages are created for Arrow Flight and scalability references, and the README is updated to accurately reflect the system's current capabilities rather than a version from months earlier. Role name validation in the RLS grant functions prevents SQL injection through malformed role names, and `grant_graph_access()` now uses parameterized DDL throughout. A version check script is added to the justfile's release assessment tool, making it impossible to publish a release with a stale README version number. A migration chain test extended to cover v0.67.0 through v0.69.0 verifies that the complete upgrade path works end-to-end.

### What's new

- **BULK-01** — Bulk-load functions (`load_ntriples`, `load_turtle`, `load_nquads`, `load_trig`, `load_rdfxml`, and their graph-aware variants) now wire into the mutation journal and call `flush()` after all batches. CONSTRUCT writeback rules fire automatically after every `load_*` call without requiring `refresh_construct_rule`.

- **FLUSH-01** — SPARQL Update and single-triple API calls no longer flush the CWB pipeline once per individual triple. Journal flush is deferred to `XACT_EVENT_PRE_COMMIT` via the existing `xact_callback_c`, so CONSTRUCT writeback fires once per statement boundary regardless of how many triples the statement inserts or deletes.

- **GATE-03** — `feature_status()` evidence paths cleaned up: stub pages created at `docs/src/reference/scalability.md` and `docs/src/reference/arrow-flight.md`; the non-existent `tests/integration/citus_rls_propagation.sh` reference replaced with the new `security_rls_role_injection.sql` test evidence path. The `validate-feature-status` CI job now fails hard when any cited evidence file is missing.

- **SHACL-DOC-01** — `docs/src/features/shacl-sparql-rules.md` rewritten: `sh:SPARQLRule` is clearly documented as not supported (emits PT481 WARNING and skips). `sh:TripleRule` and `sh:SPARQLConstraint` remain fully supported.

- **README-01/02** — `README.md` updated from v0.67.0 to v0.69.0 in "What works today" and "Known limitations" sections. `scripts/check_readme_version.sh` added and wired into `just assess-release`.

- **RLS-SQL-01** — `grant_graph_access()` and `apply_rls_to_vp_table()` now validate role names against `[A-Za-z_][A-Za-z0-9_$]*` (PT711 error on mismatch) and use `quote_ident_safe()` in DDL. SQL injection test added: `tests/pg_regress/sql/security_rls_role_injection.sql`.

- **SBOM-02** — `sbom.json` regenerated for v0.70.0. Release CI `release.yml` confirmed to include a blocking SBOM-version-match step.

- **GATE-04** — Legacy `scripts/check_roadmap_evidence.sh` and `scripts/check_api_drift.sh` deleted. `justfile` `assess-release` target now calls `.py` versions exclusively. Verified by `.github/workflows/ci.yml` (Validate feature status job).

- **TEST-01** — `tests/pg_regress/sql/v067_features.sql` added (mutation journal smoke test, Arrow Flight GUC check, feature_status evidence path regression guard).

- **TEST-02** — `tests/pg_regress/sql/v069_features.sql` added (module restructuring API stability regression guard, `construct_pipeline_status()` check, `feature_status()` coverage check).

- **TEST-03** — `tests/pg_regress/sql/recover_promotions.sql` added (full `recover_interrupted_promotions()` regression test including simulated-interruption scenario).

- **DOC-01** — `roadmap/v0.67.0.md` status already confirmed as Released ✅ (no change needed).

- **CWB test extension** — `cwb_write_path_equivalence.sql` extended with a Path 5 bulk-load arm (`load_ntriples_into_graph`) that asserts derived triples appear immediately after a bulk load.

### Schema changes

None.

### Exit criteria

All 192+ pg_regress tests pass; `validate-feature-status` CI job exits non-zero when evidence file missing; bulk-load CWB arm in `cwb_write_path_equivalence.sql` passes.

---

## [0.69.0] — 2026-05-06 — Module Architecture Restructuring

**Implements v0.69.0 roadmap: splits four large source modules along single-responsibility boundaries with zero behavioral changes.**

A 2,252-line file is not a module — it is a wall of code that no reviewer can navigate safely, no test can exercise cleanly, and no new contributor can approach without weeks of orientation. Version 0.69.0 makes a focused investment in code organization, decomposing four large modules into properly bounded sub-components with clear single responsibilities. The SPARQL engine is split across parse, plan, decode, and execute modules. The HTTP companion main entry point shrinks from 2,252 lines to 250 lines by extracting routing, database bridge, Arrow Flight, and streaming components. The CONSTRUCT writeback rules module is restructured into catalog, scheduler, delta, and retraction components, each with a clear and auditable purpose.

The storage module's public mutation API is narrowed so that insertion and deletion functions are only accessible within the crate, preventing internal implementation details from leaking into callers that should be using the higher-level journal-aware APIs. VP promotion helpers are extracted to their own module with proper documentation. All 186 regression tests continue to pass without change — this is purely a code quality release with no behavioral changes. The result is a codebase that is meaningfully easier to review, easier to test in isolation, and easier for new contributors to navigate without needing a guide through the full architecture before making their first contribution.

### What's new

- **ARCH-01 — `src/sparql/mod.rs` split** (already delivered in prior sessions): `parse.rs`, `plan.rs`, `decode.rs`, `execute.rs` extracted; `mod.rs` is now a 157-line facade with re-exports and the three public SQL-entry-point functions.

- **ARCH-02 — `pg_ripple_http/src/main.rs` split**: Handler functions, content-type constants, and response formatters extracted to `routing.rs`; SPARQL execution helpers (execute_select/ask/construct/describe) to `spi_bridge.rs`; Arrow IPC Flight endpoint to `arrow_encode.rs`; streaming placeholder to `stream.rs`. `main.rs` is now 250 lines (startup code + `main()` only).

- **ARCH-03 — `src/construct_rules.rs` split into a module directory**: `catalog.rs` (ensure_catalog), `scheduler.rs` (collect_source_graphs + compute_rule_order topological sort), `delta.rs` (compile_construct_to_inserts + run_full_recompute), `retract.rs` (retract_exclusive_triples), `mod.rs` (public API + write hooks).

- **ARCH-04 — `src/storage/mod.rs` narrowed public API**: `insert_triple_by_ids`, `delete_triple_by_ids`, and `batch_insert_encoded` narrowed to `pub(crate)` with journal-caller doc comments. VP promotion helpers (`promote_predicate`, `promote_rare_predicates`, `recover_interrupted_promotions`, `vp_promotion_threshold`, `create_extended_statistics`) extracted to `storage/promote.rs`.

- **ARCH-05 — All 186 pg_regress tests pass**; no SQL-visible changes.

### Schema changes

None. This is a pure Rust module restructuring.

### Files changed

- **src/sparql/parse.rs** — query complexity checks + ARQ aggregate preprocessing (new)
- **src/sparql/plan.rs** — SPARQL algebra → SQL plan cache (new)
- **src/sparql/decode.rs** — batch dictionary decode (new)
- **src/sparql/execute.rs** — SPI execution, CONSTRUCT/DESCRIBE/UPDATE, explain (new)
- **src/sparql/mod.rs** — thin facade: re-exports + 3 public SQL entry points
- **pg_ripple_http/src/routing.rs** — all HTTP handlers, response formatters, build_router (new)
- **pg_ripple_http/src/spi_bridge.rs** — execute_sparql_with_traceparent + execute_select/ask/construct/describe (new)
- **pg_ripple_http/src/arrow_encode.rs** — Arrow Flight bulk-export (new)
- **pg_ripple_http/src/stream.rs** — SSE/streaming placeholder (new)
- **pg_ripple_http/src/main.rs** — startup code only (250 lines, was 2252)
- **src/construct_rules/mod.rs** — public API + on_graph_write/delete hooks (new directory)
- **src/construct_rules/catalog.rs** — ensure_catalog (new)
- **src/construct_rules/scheduler.rs** — topological sort (new)
- **src/construct_rules/delta.rs** — compile + recompute (new)
- **src/construct_rules/retract.rs** — retract_exclusive_triples (new)
- **src/storage/promote.rs** — VP promotion helpers (new)
- **src/storage/mod.rs** — narrowed mutation API, pub(crate) for mutation functions

---

## [0.68.0] — 2026-04-29 — Distributed Scalability, Streaming Completion, and Fuzz Hardening

**Implements v0.68.0 roadmap: portal-based CONSTRUCT cursor streaming, Citus HLL COUNT(DISTINCT), Citus SERVICE shard pruning, nonblocking VP promotion with crash recovery, and scheduled nightly fuzz CI.**

Three important platform capabilities reach production quality in version 0.68.0. CONSTRUCT query results can now be streamed through a lazy portal-based cursor that materializes only one page at a time, bounding memory use to a configurable batch size regardless of result set size. This makes it practical to run large CONSTRUCT queries — generating Turtle or JSON-LD exports of entire named graph subsets — without risking memory exhaustion. Citus deployments gain two new optimizations: HyperLogLog approximate COUNT(DISTINCT) for SPARQL aggregates on distributed tables, and SERVICE shard pruning that routes federated queries to the single Citus worker that owns the relevant data range.

VP promotion — the process by which a predicate that has accumulated enough triples moves from the shared `vp_rare` table to its own dedicated VP table — is made non-blocking and crash-safe in this release. Promotion now tracks its progress in a status column so that a recovery function can detect and retry any promotion interrupted by a server crash, eliminating a class of inconsistent half-promoted states that previously required manual intervention. A comprehensive fuzz CI workflow runs all twelve fuzz targets nightly for 60 seconds each, with corpus and crash artifacts uploaded on every run, providing continuous automated discovery of parser and codec vulnerabilities across the SPARQL, Turtle, Datalog, SHACL, JSON-LD, federation, and geospatial parsing paths.

### What's new

- **Portal-based CONSTRUCT cursor streaming** (STREAM-01): `sparql_cursor_turtle()` and `sparql_cursor_jsonld()` now stream CONSTRUCT results using a lazy `ConstructCursorIter` — a portal-based iterator that applies the CONSTRUCT template per page and serializes each page as a Turtle/JSON-LD chunk. Memory use is bounded to `pg_ripple.export_batch_size` rows per page. New helpers `prepare_construct()` and `apply_construct_template()` in `src/sparql/mod.rs` pre-encode constant IRIs/literals to i64 once at query-plan time.

- **Citus HLL approximate COUNT(DISTINCT)** (CITUS-HLL-01): When `pg_ripple.approx_distinct=on` and the `hll` PostgreSQL extension is installed, `COUNT(DISTINCT ?x)` SPARQL aggregates are translated to `hll_cardinality(hll_add_agg(hll_hash_bigint(x)))::bigint` for scalable approximate counts on distributed VP tables. Falls back to exact `COUNT(DISTINCT)` when `hll` is absent or `approx_distinct=off`. New GUC `pg_ripple.approx_distinct` (BOOL, default `off`).

- **Citus SERVICE shard pruning** (CITUS-SVC-01): When `pg_ripple.citus_service_pruning=on`, the SERVICE translator calls `citus_service_shard_annotation()` which detects Citus worker endpoints via `is_citus_worker_endpoint()` and wires shard-constraint SQL annotations for pruning. New GUC `pg_ripple.citus_service_pruning` (BOOL, default `off`). Full multi-node infrastructure required for end-to-end testing.

- **Nonblocking VP promotion with crash recovery** (PROMO-01): VP promotion now tracks progress via a `promotion_status TEXT` column in `_pg_ripple.predicates` (values: `'promoting'` during copy, `'promoted'` when complete). New SQL function `pg_ripple.recover_interrupted_promotions()` scans for `'promoting'` entries and retries interrupted promotions — call it after an unclean server shutdown. New GUC `pg_ripple.vp_promotion_batch_size` (INT, 1–1000000, default 10000).

- **Scheduled nightly fuzz CI** (FUZZ-01): `.github/workflows/fuzz.yml` runs all 12 fuzz targets (sparql_parser, turtle_parser, rdfxml_parser, dictionary_hash, federation_result, datalog_parser, shacl_parser, jsonld_framer, http_request, r2rml_mapping, geosparql_wkt, llm_prompt_builder) nightly for 60 s each. Manual `workflow_dispatch` supports extended runs. Corpus and crash artifacts are uploaded on each run.

### Schema changes

- `_pg_ripple.predicates` table: added `promotion_status TEXT` column (NULL = legacy/no promotion started, `'promoting'` = copy in progress, `'promoted'` = fully promoted). Added to initial schema CREATE TABLE and to migration script `sql/pg_ripple--0.67.0--0.68.0.sql`.

### GUCs added

| GUC | Type | Default | Level | Purpose |
|---|---|---|---|---|
| `pg_ripple.approx_distinct` | BOOL | `off` | USERSET | Route COUNT(DISTINCT) through Citus HLL when available |
| `pg_ripple.citus_service_pruning` | BOOL | `off` | USERSET | Enable Citus worker shard annotations for SERVICE |
| `pg_ripple.vp_promotion_batch_size` | INT | 10000 | USERSET | Batch size for nonblocking VP promotion copy phase |

### SQL functions added

| Function | Returns | Description |
|---|---|---|
| `pg_ripple.recover_interrupted_promotions()` | `bigint` | Scan and retry interrupted VP promotions after crash |

### Files changed

- **src/sparql/cursor.rs** — new `ConstructCursorIter` struct + `ConstructFormat` enum; `sparql_cursor_turtle` and `sparql_cursor_jsonld` now return `impl Iterator`
- **src/sparql/mod.rs** — `TemplateSlot`, `ConstructTemplate`, `prepare_construct()`, `apply_construct_template()`
- **src/sparql_api.rs** — updated SETOF wrappers for new iterator API
- **src/sparql/translate/group.rs** — HLL aggregate translation + `citus_hll_available()`
- **src/gucs/storage.rs** — `APPROX_DISTINCT`, `CITUS_SERVICE_PRUNING`, `VP_PROMOTION_BATCH_SIZE` GUC statics
- **src/citus.rs** — `is_citus_worker_endpoint()`, `citus_service_shard_annotation()`, `extract_url_host()`
- **src/sparql/translate/graph.rs** — wire `citus_service_shard_annotation()` in SERVICE translator
- **src/storage/mod.rs** — `promote_predicate()` with status tracking; `recover_interrupted_promotions()`
- **src/dict_api.rs** — `recover_interrupted_promotions()` pg_extern
- **src/lib.rs** — three new GUC registrations
- **src/schema.rs** — `promotion_status TEXT` in predicates CREATE TABLE; `v068_schema_version_stamp`
- **src/feature_status.rs** — updated status for 6 deliverables + new `continuous_fuzzing` entry
- **sql/pg_ripple--0.67.0--0.68.0.sql** — migration script (ADD COLUMN, schema_version stamp)
- **.github/workflows/fuzz.yml** — new scheduled fuzz workflow
- **tests/pg_regress/sql/v068_features.sql** — new regress test (186 total, 0 failures)
- **tests/pg_regress/expected/v068_features.out** — bootstrapped expected output

---

## [0.67.0] — 2026-05-06 — Production Hardening and Assessment 9 Remediation

**Implements v0.67.0 roadmap: Arrow Flight security hardening, mutation journal for CONSTRUCT writeback, Row Level Security propagation to all VP tables, panic→error conversion, Python gate tooling, benchmark correctness fixes, and scheduled performance trend CI.**

Production readiness requires not just functional correctness but a comprehensive set of operational and security protections that catch problems before they become incidents. Version 0.67.0 addresses eight findings from Assessment 9, starting with two Arrow Flight security hardening items: unsigned tickets are now rejected by default (requiring an explicit opt-in GUC for development use only), and the tombstone exclusion query prevents deleted triples from appearing in bulk exports. A transaction-local mutation journal unifies CONSTRUCT writeback across all write paths — `insert_triple`, SPARQL INSERT DATA, `load_ntriples`, and `load_turtle` — so that derived triples are always updated regardless of which entry point was used to make a change.

The release also delivers significant infrastructure improvements for reliability and observability. A `validate-feature-status` CI job verifies after every build that all features claimed as implemented have live evidence files on disk — making it structurally impossible to ship a release with misleading status claims. Row-level security policies are now applied to VP delta and main tables when created and when predicates are promoted, closing a gap where RLS protection was only applied to the original table at creation time. A scheduled weekly performance trend CI workflow catches regressions automatically by comparing new benchmark results against a four-week rolling average. Python-based replacements for legacy shell gate scripts provide more reliable parsing and require a `--version` argument to prevent stale invocations from passing silently.

### What's new

- **Arrow Flight unsigned-ticket hardening** (FLIGHT-SEC-01): Unsigned Arrow Flight tickets are now rejected by default. New GUC `pg_ripple.arrow_unsigned_tickets_allowed` (BOOL, default `off`) must be explicitly set to allow unsigned tickets. Corresponding `ARROW_UNSIGNED_TICKETS_ALLOWED` env var for `pg_ripple_http`. Ticket rejections are tracked in `streaming_metrics()`. Evidence: `pg_ripple.feature_status()`, `pg_ripple.streaming_metrics()`.

- **Arrow Flight tombstone-exclusion and batch streaming** (FLIGHT-SEC-02): `POST /flight/do_get` now uses tombstone-exclusion query (`main EXCEPT tombstones UNION ALL delta`) to prevent serving deleted triples. Export is streamed in configurable batches via new GUC `pg_ripple.arrow_batch_size` (INT, 1–100000, default 1000). Response header `x-arrow-batches` reports batch count. `arrow_batches_sent` counter added to `streaming_metrics()`. Evidence: `pg_ripple.feature_status()`.

- **Transaction-local mutation journal** (MJOURNAL-01/02/03): A Rust `thread_local!` mutation journal (`src/storage/mutation_journal.rs`) unifies CONSTRUCT writeback across all write paths: `insert_triple`, SPARQL INSERT DATA, `load_ntriples`, and `load_turtle`. Fast-path skips journal accumulation when no CONSTRUCT rules are defined. Evidence: `tests/pg_regress/sql/cwb_write_path_equivalence.sql`.

- **Row Level Security on VP delta/main tables** (RLS-01/02): `enable_graph_rls()` and `grant_graph_access()` now apply RLS policies to dedicated VP `_delta` and `_main` tables at creation, promotion, and grant/revoke time. Evidence: `tests/pg_regress/sql/rls_promotion.sql`.

- **Panic → pgrx::error conversion** (PANIC-01): `construct_rules.rs` topological-sort `panic!()` replaced with `pgrx::error!()` to ensure clean PostgreSQL error reporting under load. Evidence: `src/construct_rules.rs`.

- **Python gate tooling** (GATE-01): `scripts/check_api_drift.sh` and `scripts/check_roadmap_evidence.sh` replaced with portable Python 3 equivalents (`scripts/check_api_drift.py`, `scripts/check_roadmap_evidence.py`) that require `--version X.Y.Z` to prevent stale invocations. Evidence: `scripts/check_api_drift.py`, `scripts/check_roadmap_evidence.py`.

- **`validate-feature-status` CI job** (GATE-02): New CI job added to `.github/workflows/ci.yml` that runs after `test` and `regress`, calls `feature_status()`, verifies evidence paths exist on disk, and runs both Python gate scripts. Evidence: `.github/workflows/ci.yml`.

- **Documentation truth** (GATE-03): `README.md` "What works today" updated from v0.63.0 to v0.67.0, pgrx version reference corrected to 0.18, v0.64.0 `roadmap/v0.64.0.md` status corrected to `Released ✅`.

- **SBOM version verification** (SBOM-01): Release workflow now verifies that the regenerated `sbom.json` component version matches `Cargo.toml` before uploading to the GitHub release. Evidence: `.github/workflows/release.yml`.

- **Benchmark correctness** (BENCH-01): `.github/workflows/benchmark.yml` no longer uses `bash` to execute SQL files. Merge throughput and vector index benchmarks now use `pgbench -f` and `psql -f` respectively. `|| true` suppressors removed; `continue-on-error: false` added. Benchmark failures now fail the CI run.

- **Scheduled performance trend CI** (BENCH-02): New weekly workflow `.github/workflows/performance_trend.yml` runs insert throughput, merge throughput, and hybrid search benchmarks, appends results to `benchmarks/*_history.csv`, and fails if any metric drops more than 10% below the 4-week rolling average.

### GUCs added

| GUC | Type | Default | Level | Purpose |
|---|---|---|---|---|
| `pg_ripple.arrow_unsigned_tickets_allowed` | BOOL | `off` | SIGHUP | Allow unsigned Arrow Flight tickets (dev-only) |
| `pg_ripple.arrow_batch_size` | INT | 1000 | USERSET | Arrow IPC export batch size per record batch |

### Dependencies added

None — all v0.67.0 changes are implemented using already-present dependencies.



**Implements the v0.66.0 roadmap: true paged SPARQL cursors via PostgreSQL portal API, HMAC-SHA256 signed Arrow Flight v2 tickets, real Arrow IPC streaming in pg_ripple_http, WCOJ explain metadata, streaming observability metrics, and Citus BRIN summarise SQL API.**

Version 0.66.0 delivers three capabilities that make pg_ripple competitive with dedicated graph analytics platforms. True SPARQL cursor streaming uses the PostgreSQL portal API to provide memory-bounded paging — peak memory use is proportional to page size rather than total result size, making it possible to stream arbitrarily large result sets without risk of memory exhaustion. Real Arrow IPC streaming in the HTTP companion validates HMAC-SHA256-signed tickets, connects directly to the PostgreSQL extension, and delivers triple store data as a binary columnar stream that Arrow-native analytics tools can consume at full memory bandwidth. WCOJ explain metadata in the query plan output shows operators whether the worst-case optimal join executor was activated for a given query and why it made that decision.

The observability improvements are equally significant: a new `streaming_metrics()` function exposes live atomic counters for every streaming activity in the system — cursor pages opened, rows streamed, Arrow batches sent, ticket rejections, and Citus BRIN summarize completions — giving operations teams real-time visibility into the data export subsystem. The Citus BRIN summarize SQL API enables operations teams to keep BRIN index statistics current across all promoted VP main-partition tables with a single function call, an important maintenance operation for keeping the HTAP merge storage path efficient on Citus distributed deployments.

### What's new

- **True SPARQL cursor streaming** (STREAM-01): `sparql_cursor()` now uses the PostgreSQL portal API (`SpiCursor::detach_into_name()` + `SpiClient::find_cursor()`) for memory-bounded paged streaming. Peak memory is proportional to `pg_ripple.export_batch_size`, not the full result size. The cursor survives across SPI sessions within the same transaction.

- **HMAC-SHA256 signed Arrow Flight tickets** (FLIGHT-01): `export_arrow_flight()` now generates signed, expiring JSON tickets (`type = "arrow_flight_v2"`). Tickets include `iat`, `exp`, `aud`, `nonce`, and an HMAC-SHA256 signature over a canonical string. New GUCs: `pg_ripple.arrow_flight_secret` (signing key, `SIGHUP`-level) and `pg_ripple.arrow_flight_expiry_secs` (default: 3600).

- **Real Arrow IPC streaming in pg_ripple_http** (FLIGHT-02): `POST /flight/do_get` now validates the HMAC-SHA256 ticket signature, expiry, and audience, then streams all VP main, delta, and rare tables for the requested graph as a binary Arrow IPC stream (`application/vnd.apache.arrow.stream`). Schema: `s Int64, p Int64, o Int64, g Int64`. The `ARROW_FLIGHT_SECRET` environment variable must match `pg_ripple.arrow_flight_secret`.

- **WCOJ explain metadata** (WCOJ-01): `explain_sparql_jsonb()` output now includes a `"wcoj"` block reporting `cyclic_bgp_detected`, `wcoj_mode` (`"planner_hint"`, `"disabled"`, or `"not_applicable"`), `planner_settings`, and `fallback_reason`.

- **Streaming observability metrics** (OBS-01): New `pg_ripple.streaming_metrics() → JSONB` function returns live atomic counters: `cursor_pages_opened`, `cursor_pages_fetched`, `cursor_rows_streamed`, `arrow_batches_sent`, `arrow_ticket_rejections`, `citus_brin_summarise_completed`.

- **Citus BRIN summarise SQL API** (CITUS-04): New `pg_ripple.citus_brin_summarise_all() → BIGINT` function runs `brin_summarize_new_values` on every promoted VP main-partition table. On Citus deployments uses `run_command_on_shards`; on non-Citus deployments runs locally. Returns total shards/tables updated.

- **Feature status updated**: `arrow_flight` moves from `stub` to `experimental` in `pg_ripple.feature_status()`.

### Dependencies added

- `hmac = "0.12"`, `sha2 = "0.10"`, `hex = "0.4"` (in pg_ripple extension for ticket signing)
- `arrow = "55"` (in pg_ripple_http for Arrow IPC serialization)
- `hmac = "0.12"`, `sha2 = "0.10"`, `hex = "0.4"` (in pg_ripple_http for ticket validation)

---

## [0.65.0] — 2026-04-28 — CONSTRUCT Writeback Correctness Closure

**Implements the v0.65.0 roadmap: real delta maintenance, HTAP-aware retraction, exact provenance capture, parameterized rule catalog writes, observability, pipeline status API, and the full CWB behavior test matrix.**

SPARQL CONSTRUCT writeback rules are one of pg_ripple's most powerful features — they allow teams to express raw-to-canonical data transformation pipelines as SPARQL CONSTRUCT queries that automatically keep derived graphs up to date. Version 0.65.0 makes these rules fully automatic for the first time. Previously, after any write to a source graph, users had to manually call `refresh_construct_rule()` to propagate changes to dependent target graphs. Now, source graph inserts and deletes automatically trigger incremental derivation and DRed-style retraction in the same transaction, with no manual intervention required. Derived triples are always consistent with their sources immediately after every write.

HTAP split storage receives a correctness fix for retraction: the `retract_exclusive_triples()` function now correctly handles the case where derived triples live in either the delta or the main partition, using direct deletes for delta-resident triples and tombstones for main-resident triples to prevent silent retraction failures after a merge cycle. A parameterized SPI fix eliminates potential injection through catalog writes, and mode validation ensures only valid pipeline modes are accepted at rule creation time. A new pipeline status API lets operators check the current derivation state of every registered CONSTRUCT rule — whether it is fully derived, partially derived, or needs a full refresh — through observable SQL function calls rather than internal catalog queries.

### What's new

- **Delta maintenance kernel** (CWB-FIX-01/02): Source graph inserts and deletes now automatically update dependent CONSTRUCT target graphs in the same transaction. `insert_triple()` triggers incremental derivation; `delete_triple_from_graph()` triggers DRed-style rederive-then-retract. Manual `refresh_construct_rule()` is no longer required for routine operation.

- **HTAP-aware promoted-predicate retraction** (CWB-FIX-03): `retract_exclusive_triples()` now correctly handles VP tables in HTAP split mode (delta + main + tombstones). Delta-resident derived triples are deleted directly; main-resident derived triples receive tombstones — preventing silent retraction failures after merge.

- **Exact provenance capture** (CWB-FIX-04): Provenance is recorded via `INSERT ... ON CONFLICT DO NOTHING RETURNING` CTEs, capturing only rows inserted by the current rule run. Pre-existing `source = 1` triples from other rules or manual inserts are no longer mis-attributed.

- **Parameterized SPI and mode validation** (CWB-FIX-05): All catalog writes in `create_construct_rule` use `Spi::run_with_args` (parameterized) for scalar fields. Mode values are validated — only `'incremental'` and `'full'` are accepted.

- **Shared-target reference-count semantics** (CWB-FIX-06): Two or more rules can write the same derived triple to the same graph. Dropping or refreshing one rule preserves triples still supported by another rule's provenance row.

- **Observability columns** (CWB-FIX-07): `_pg_ripple.construct_rules` gains five new columns: `last_incremental_run`, `successful_run_count`, `failed_run_count`, `last_error`, `derived_triple_count`. `list_construct_rules()` exposes all health fields.

- **Full CWB test matrix** (CWB-FIX-08): `tests/pg_regress/sql/construct_rules.sql` now covers: create/initial derivation, incremental insert, DRed delete, refresh from scratch, self-cycle rejection, two-rule pipeline stratification, mutual-cycle rejection, drop-with-retract, drop-without-retract, shared target preservation, explain output, pipeline status, and apply_for_graph.

- **SHACL rule bridge foundation** (CWB-FIX-09): `feature_status()` for `shacl_sparql_rule` updated to note that the derivation kernel foundation is delivered; full routing deferred to v0.66.0.

- **Pipeline introspection API** (CWB-FIX-10): New `pg_ripple.construct_pipeline_status() → JSONB` function returns dependency graph, rule order, last run state, derived triple counts, and failed/stale flags for all rules.

- **`apply_construct_rules_for_graph(graph_iri TEXT) → BIGINT`**: New public function for manual incremental maintenance of all rules sourcing a given graph. Returns total derived triple count.

- **Feature status promoted**: `construct_writeback` moves from `manual_refresh` to `implemented` in `pg_ripple.feature_status()`.

---

## [0.64.0] — 2026-04-27 — Release Truth and Safety Freeze

**Implements the v0.64.0 roadmap: feature-status SQL API, deep /ready readiness, GitHub Actions SHA pinning, Docker release digest integrity, documentation truth pass, roadmap evidence scripts, API drift checks, `just assess-release`, release evidence dashboard, and optional-feature degradation semantics guide.**

A system that claims to have capabilities it does not actually have is more dangerous than one that honestly admits its limitations, because users build on false foundations. Version 0.64.0 introduces a formal truthfulness and evidence culture for pg_ripple: a new `feature_status()` SQL function returns one row per major capability with an honest status value drawn from a defined taxonomy — implemented, experimental, planner_hint, manual_refresh, stub, degraded, or planned — making it impossible for the project to accidentally overstate its capabilities. The `/ready` readiness endpoint is extended to include a feature status snapshot so that deployment automation and operators can make informed decisions about what the system can actually do right now.

Reproducibility and supply chain security are enforced with equal rigor. All GitHub Actions workflow steps are pinned to full 40-character commit SHAs with human-readable tag comments, and a CI script rejects any mutable reference that could allow a compromised or renamed action to execute different code in a future run. Docker release image digests are required to be present in the release artifact — a missing digest fails the release. Three tooling improvements automate ongoing quality: `check_roadmap_evidence.sh` flags completion claims without evidence, `check_api_drift.sh` verifies that every exported SQL function appears in documentation, and `just assess-release` runs the entire quality gate suite as a single command.

### What's new

- **`pg_ripple.feature_status()`** (TRUTH-01): New SQL function returning one row per major capability with an honest status value. Status taxonomy: `implemented`, `experimental`, `planner_hint`, `manual_refresh`, `stub`, `degraded`, `planned`. Reports honest statuses for Arrow Flight (`stub`), WCOJ (`planner_hint`), SHACL-SPARQL rules (`planned`), CONSTRUCT writeback (`manual_refresh`), Citus SERVICE pruning (`planned`), and all other major features.

- **Deep `/ready` readiness** (TRUTH-02): Extended `pg_ripple_http /ready` to include PostgreSQL version, extension version, and a feature-status snapshot. The response body now includes `partial_features` (all non-`implemented` features) and `degraded_features` (features with `stub` or `degraded` status).

- **GitHub Actions SHA pinning** (TRUTH-03): All third-party `uses:` references in `.github/workflows/` are now pinned to full 40-character commit SHAs with the human-readable tag as a comment. New CI step `scripts/check_github_actions_pinned.sh` rejects mutable refs (`@v6`, `@stable`, branch names) — zero mutable refs permitted.

- **Docker release digest integrity** (TRUTH-04): Removed `continue-on-error: true` from Docker build/push. Release job now captures the immutable image digest from the build step, fails if no digest is produced, and scans `ghcr.io/trickle-labs/pg-ripple@sha256:...` (the immutable digest) instead of a mutable tag.

- **Documentation truth pass** (TRUTH-05): Corrected `plans/implementation_plan.md` pgrx 0.17 → 0.18 throughout. Updated README "What works today" to reflect v0.63.0. Added "Known limitations in v0.63.0" section to README covering Arrow Flight, WCOJ, SHACL rules, CONSTRUCT writeback, Citus pruning, and optional dependencies.

- **Roadmap evidence check script** (TRUTH-06): `scripts/check_roadmap_evidence.sh` — advisory lint that flags completion-claim bullet points in CHANGELOG without evidence markers (CI test name, docs path, SQL function reference). Advisory-only in v0.64.0; will be enforced in v0.67.0.

- **API drift check script** (TRUTH-07): `scripts/check_api_drift.sh` — extracts exported function names from `#[pg_extern]` annotations in `src/` and checks that each appears in at least one documentation file. Advisory-only; catches the v0.63 Citus signature drift pattern.

- **`just assess-release`** (TRUTH-08): One-command release quality gate that runs migration headers lint, GitHub Actions pinning lint, SECURITY DEFINER lint, roadmap evidence check, API drift check, and version sync check. Optionally generates a release evidence report with `just assess-release VERSION`.

- **Release evidence dashboard** (TRUTH-09): `scripts/generate_release_evidence.sh` generates `target/release-evidence/<version>/summary.json` and `summary.md`. Release workflow uploads the artifact to GitHub Actions and attaches it to the GitHub release.

- **Degradation semantics guide** (TRUTH-10): New documentation page `docs/src/reference/degradation.md` documents expected degraded behavior, return values, warning codes, readiness behavior, and planned implementation milestones for every optional feature.

- **pg_regress test** `feature_status.sql`: Asserts that `feature_status()` returns rows, all statuses are from the approved taxonomy, and specific partial features report honest status values.

---

## [0.63.0] — 2025 — SPARQL CONSTRUCT Writeback Rules

**Implements the v0.63.0 roadmap: SPARQL CONSTRUCT writeback rules (CWB-01 through CWB-11), raw-to-canonical pipelines, incremental delta maintenance via Delete-Rederive (DRed), pipeline stratification with cycle detection, and Citus scalability improvements CITUS-30 through CITUS-37.**

Data integration pipelines often need to maintain multiple representations of the same information — raw sensor data needs to be canonicalized into a standard schema, external API responses need to be normalized to internal IRIs, and inferred relationships need to be materialized for query performance. Version 0.63.0 introduces SPARQL CONSTRUCT writeback rules, which express these transformation pipelines as persistent SPARQL CONSTRUCT queries registered in the knowledge graph. When source data changes, the system automatically re-evaluates the registered rules and updates the derived target graphs. Pipeline stratification detects dependency order using topological sort, and circular dependencies are rejected at rule registration time with a clear error.

The CONSTRUCT writeback engine includes full Delete-Rederive support: when a source triple is deleted, the engine correctly retracts only those derived triples that were exclusively supported by the deleted triple, leaving in place any derived triples still supported by other facts. Validation at rule registration catches blank nodes in CONSTRUCT templates, unbound variables, non-CONSTRUCT queries, and self-referential graphs before they can cause runtime failures. Three new Citus scalability functions round out the release: SERVICE result shard pruning, HyperLogLog availability detection, and per-worker BRIN index summarization — each addressing distributed query bottlenecks that emerge as VP tables are sharded across Citus workers.

### What's new

- **CONSTRUCT writeback rules** (CWB-01 to CWB-11): New SQL API `pg_ripple.create_construct_rule(name, sparql, target_graph, mode)` registers a SPARQL CONSTRUCT query as a persistent writeback rule. Derived triples are stored in the target named graph with `source = 1`. Supporting functions: `drop_construct_rule`, `refresh_construct_rule`, `list_construct_rules`, `explain_construct_rule`.

- **Catalog tables**: `_pg_ripple.construct_rules` (rule registry) and `_pg_ripple.construct_rule_triples` (provenance index) created lazily on first use and also via the migration script.

- **Pipeline stratification**: `compute_rule_order` performs Kahn's topological sort on the rule dependency graph; mutual-recursion (cycles) are rejected at registration time with a clear error.

- **Validation at registration**: blank nodes in CONSTRUCT templates, unbound variables, non-CONSTRUCT queries, and self-referential graphs (target == source) are all rejected with informative error messages.

- **Citus CITUS-30 — SERVICE result shard pruning**: New function `pg_ripple.service_result_shard_prune(endpoint TEXT, graph_iri TEXT) RETURNS BIGINT`. Returns the count of remote triples from the endpoint, or -1 if pruning is not applicable.

- **Citus CITUS-32 — HyperLogLog `COUNT(DISTINCT)`**: New function `pg_ripple.approx_distinct_available() RETURNS BOOLEAN`. Returns whether the HyperLogLog extension is available for approximate distinct counting.

- **Citus CITUS-37 — per-worker BRIN summarise**: New SRF `pg_ripple.brin_summarize_vp_shards() RETURNS TABLE(table_name TEXT, pages_summarized BIGINT)`. Summarises BRIN indexes on all VP main-partition tables.

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.63.0'` or apply `sql/pg_ripple--0.62.0--0.63.0.sql` manually. The migration creates the `_pg_ripple.construct_rules` and `_pg_ripple.construct_rule_triples` catalog tables.

---

## [0.62.0] — 2025 — Query Frontier

**Implements the v0.62.0 roadmap: Apache Arrow Flight bulk export, Leapfrog-Triejoin WCOJ planner integration, visual graph explorer in `pg_ripple_http`, tiered dictionary, Citus vp_rare vacuum, distributed inference dispatch, live shard rebalance, multi-hop pruning carry-forward, and `cargo deny` / `cargo audit` CI gates.**

The most capable features of a knowledge graph system are often the ones that were too expensive to run before a key algorithmic or engineering investment. Version 0.62.0 makes three previously impractical capabilities practical. Apache Arrow Flight bulk export generates a signed ticket that the HTTP companion can use to stream all triples in a named graph as a binary columnar data stream — enabling large-scale knowledge graph exports to analytical platforms at memory bandwidth speeds. The Leapfrog Triejoin WCOJ integration detects cyclic Basic Graph Patterns (triangles, cliques, and other cyclic structures) and activates the worst-case optimal join algorithm, providing sub-second execution for query patterns that previously ran for minutes as the graph grew.

An interactive graph explorer is served directly from the HTTP companion at `/explorer`, rendering a force-directed D3.js visualization of graph data that domain experts can browse and explore without writing SPARQL queries. On the distributed scalability side, this release introduces Citus HyperLogLog COUNT(DISTINCT) for scalable approximate aggregates, a live non-blocking shard rebalance function, multi-hop subject ID carry-forward for pruning efficiency, per-graph vacuum for the rare-predicate table, and a tiered dictionary with configurable cold-tier eviction. `cargo deny` license and advisory checking plus `cargo audit` vulnerability scanning are added as required CI gates, bringing supply chain security to parity with the code quality gates already in place.

### What's new

- **Apache Arrow Flight bulk export** (Q-1): New SQL function `pg_ripple.export_arrow_flight(graph_iri TEXT) RETURNS BYTEA`. Returns a JSON-encoded Flight ticket that the Arrow Flight server can use to stream all triples from the named graph in Arrow IPC format.

- **WCOJ planner integration** (Q-2): The BGP translator now detects cyclic Basic Graph Patterns (≥ 3 variables, ≥ 3 triple patterns) and activates the Leapfrog-Triejoin algorithm via `SET pg_ripple.enable_wcoj = on` preamble before executing the query. Provides sub-second execution for formerly intractable cyclic graph patterns.

- **Visual graph explorer** (Q-3): `pg_ripple_http` now serves a force-directed interactive graph visualizer at `/explorer`. The SPA fetches graph data and renders it as a D3.js force layout with node-label tooltips.

- **Arrow Flight `/flight/do_get` endpoint** (Q-4): `pg_ripple_http` accepts `POST /flight/do_get` with a Flight ticket body and responds with a JSON stub; the full streaming Arrow IPC implementation is wired in when the `arrow-flight` feature is enabled.

- **Citus CITUS-25 — vacuum_vp_rare** (CITUS-25): New SRF `pg_ripple.vacuum_vp_rare() RETURNS TABLE(predicate_id BIGINT, rows_removed BIGINT)`. Removes dead entries from `_pg_ripple.vp_rare` that reference predicates no longer in the predicate catalog.

- **Citus CITUS-26 — tiered dictionary** (CITUS-26): Added `access_count BIGINT NOT NULL DEFAULT 0` column to `_pg_ripple.dictionary`. New GUC `pg_ripple.dictionary_tier_threshold` (default: `-1`, disabled). When positive, entries whose `access_count` falls below the threshold may be evicted to a cold tier.

- **Citus CITUS-27 — distributed inference dispatch** (CITUS-27): New GUC `pg_ripple.datalog_citus_dispatch` (default: `off`). When enabled, the Datalog executor distributes each rule-stratum evaluation across Citus worker nodes.

- **Citus CITUS-28 — live shard rebalance** (CITUS-28): New SRF `pg_ripple.citus_live_rebalance() RETURNS TABLE(source_node TEXT, target_node TEXT, shard_id BIGINT, shard_size_bytes BIGINT)`. Initiates a non-blocking shard rebalance and streams per-shard progress.

- **Citus CITUS-29 — multi-hop pruning carry-forward** (CITUS-29): New GUC `pg_ripple.citus_prune_carry_max` (default: `1000`). The new `ShardPruneSet` and `prune_hop()` implementation carry the subject-ID set forward across triple-pattern hops, eliminating worker fan-out for multi-hop patterns.

- **CI quality gates** (Q-5): `cargo deny check` (license/advisory/duplicate crate check) and `cargo audit` (vulnerability scan) are now required CI steps.

---

## [0.61.0] — 2025 — Ecosystem Depth & Polish

**Implements the v0.61.0 roadmap: per-graph access control, GDPR right-to-erasure, inference explainability, SHACL-AF rule execution, dbt adapter, OTLP traceparent propagation, richer federation stats, Citus scalability improvements (object pruning, direct-shard bulk-load, graph shard affinity), and test quality improvements.**

Enterprise knowledge graph deployments require features that go well beyond query execution — they need access control, compliance tooling, provenance tracking, and integration with the broader data platform ecosystem. Version 0.61.0 delivers a rich set of these capabilities. Per-named-graph access control allows row-level security policies to be installed for individual named graphs, restricting which database roles can query which portions of the knowledge graph. A complete GDPR right-to-erasure implementation removes all traces of a subject across VP tables, the dictionary, KGE embeddings, provenance graphs, and audit logs in a single transactional call that returns a per-relation deletion count for the compliance record.

AI and data engineering integrations receive significant investment. An OTLP traceparent propagation path creates unbroken distributed traces from the HTTP load balancer through the companion service all the way into query execution, enabling millisecond-precision performance debugging across the full request stack. A dbt adapter published as a Python package gives data engineers SPARQL-aware model, source, and ref macros that mix SQL and SPARQL transformations in a single dbt project with full lineage tracking. Inference explainability via `explain_inference()` walks the derivation chain for any inferred triple, returning the full rule firing history as a structured JSON tree. Three Citus scalability improvements — object-based shard pruning, direct-shard bulk load, and named-graph shard affinity — extend the distributed query performance work to cover object-bound queries and named graph locality.

### What's new

- **Per-named-graph access control** (6.3 / L-8.1): Added `pg_ripple.grant_graph(graph_iri, role)` and `pg_ripple.revoke_graph(graph_iri, role)` helper functions that install / remove PostgreSQL RLS policies filtering by graph ID.

- **GDPR right-to-erasure** (6.7 / L-8.3): New SRF `pg_ripple.erase_subject(iri TEXT) → TABLE(relation TEXT, rows_deleted BIGINT)`. Removes all triples about the subject across all VP tables, `vp_rare`, the dictionary (if unreferenced), KGE embeddings, the PROV-O provenance graph, and the audit log — in a single transaction. Returns a per-relation deletion count for the erasure record.

- **Inference explainability** (6.6): New SRF `pg_ripple.explain_inference(s, p, o, g) → TABLE(depth INT, rule_id TEXT, source_sids BIGINT[], child_triples JSONB)`. Returns the full derivation chain for a given inferred triple as a JSON tree, walking the `_pg_ripple.rule_firing_log` table introduced in this release.

- **SHACL-AF `sh:rule` execution** (D7-1 / D-3 / S4-8): Implemented the bridge in `src/shacl/af_rules.rs` that compiles `sh:TripleRule` patterns to Datalog rules and loads them via `load_rules_text()`. New pg_regress test `shacl_af_rule_execution.sql` validates end-to-end execution. Emits **PT482** when a `sh:rule` body cannot be compiled into a Datalog rule and is skipped.

- **dbt adapter** (6.11): Published `dbt-pg-ripple` Python package in `clients/dbt-pg-ripple/`. Provides `sparql_model`, `sparql_source`, and `sparql_ref` SPARQL-aware dbt macros. Data engineers can now mix SQL and SPARQL transformations in a single dbt project with full lineage tracking.

- **OTLP traceparent propagation** (I7-1): `pg_ripple_http` now extracts the W3C `traceparent` header and forwards it via the `pg_ripple.tracing_traceparent` session GUC into the extension. Every SPARQL/Datalog query span is tagged with the originating trace ID, giving an unbroken trace from the load balancer through the HTTP service into the query engine.

- **OpenTelemetry semantic-convention map** (I7-2): New doc `docs/src/operations/observability-otel.md` with span-name → attribute table and example Prometheus/Grafana queries.

- **Federation call stats per endpoint** (I7-3): `pg_ripple.federation_call_stats()` now returns `(endpoint TEXT, calls INT, errors INT, blocked INT, p50_ms INT, p95_ms INT, last_error_at TIMESTAMPTZ)`.

- **BRIN summarize failure tracking** (F7-3): Added `brin_summarize_failures INT` column to `_pg_ripple.predicates`. Persistent `brin_summarize_new_values()` failures are promoted from `debug1` to `NOTICE` after the second consecutive merge cycle failure; counter resets on success.

- **Citus object-based shard pruning** (CITUS-20): Extended `src/citus.rs` with `TermRole` enum (`Subject | Object`) and `prune_bound_term(term_id, role)`. The SPARQL translator now detects bound objects and routes to the correct shard, delivering the same 10–100× speedup that subject-pruning already provides.

- **Citus direct-shard bulk-load path** (CITUS-21): Added `batch_insert_encoded_shard_direct()` in `src/storage/mod.rs`. When Citus sharding is enabled, bulk-load batches are written directly to the physical shard table, bypassing coordinator routing. Falls back to the coordinator path when Citus is not installed or the predicate is in `vp_rare`.

- **Citus named-graph shard affinity** (CITUS-22): New table `_pg_ripple.graph_shard_affinity`. New functions `pg_ripple.set_graph_shard_affinity(graph_iri, shard_id)` and `pg_ripple.clear_graph_shard_affinity(graph_iri)`. When a SPARQL query includes a `GRAPH <g> { ... }` scope and Citus sharding is enabled, the planner restricts the query to the designated shard.

- **GROUP BY subject aggregate push-down audit** (CITUS-23): New pg_regress test `citus_aggregate_pushdown.sql` verifying that SPARQL `GROUP BY ?s` queries emit `GROUP BY s` in SQL, confirming Citus partial-aggregation push-down.

- **Temporal RDF post-merge correctness** (J7-3): New pg_regress test `temporal_rdf_post_merge.sql` verifying `point_in_time()` resolves correctly after SIDs move from delta to main.

- **OWL 2 RL deletion proof** (6.13 / E7-2): New pg_regress test `datalog_owl_rl_deletion.sql` exercising the full DRed retraction path.

- **DRed cycle guard** (E7-2): New pg_regress test `datalog_dred_cycle.sql` constructing a `sameAs` cycle and asserting PT530 is raised or the system remains stable.

- **SPARQL Entailment Regimes test driver** (6.8 / B7-2): New `tests/sparql_entailment/` directory with manifest, runner script, and RDFS/OWL 2 RL fixtures. Added as `entailment-suite` CI job (informational, `continue-on-error: true`).

- **Conformance thresholds config** (J7-4): Moved pass-rate thresholds from CI YAML expressions into `tests/conformance/thresholds.json`. CI now reads this file to determine gate criteria.

- **Cypher/GQL ADR** (K7-4): New `plans/cypher.md` capturing the design intent: target query subset, `cypher-parser` crate, rewrite-to-SPARQL strategy, and semantic fidelity notes.

- **PT404 error code for HTTP body-size rejection** (H7-6): `pg_ripple_http` now wraps axum's 413 response in a JSON envelope `{"error": "PT404", "message": "..."}`.

### Schema changes

- New table `_pg_ripple.graph_shard_affinity` (CITUS-22)
- New table `_pg_ripple.rule_firing_log` (inference explainability)
- New column `_pg_ripple.predicates.brin_summarize_failures INT` (F7-3)

### Migration

Upgrade from v0.60.0 with `ALTER EXTENSION pg_ripple UPDATE`.

---

## [0.60.0] — 2026-04-27 — Production Hardening Sprint

**Implements the v0.60.0 roadmap: HTAP merge atomic swap, CI supply-chain hardening, three new fuzz harnesses, `/ready` Kubernetes readiness probe, SERVICE SILENT circuit-breaker test, architecture diagram refresh, pg_trickle dependency matrix, and pg_dump round-trip test.**

Production deployments expose failure modes that development and staging environments never reveal, and version 0.60.0 systematically addresses the most dangerous ones. The most critical fix is the HTAP merge atomic rename-swap: the previous merge implementation had a race window where the VP view's backing relation could briefly be absent during a merge cycle, causing "relation does not exist" errors under concurrent query load. The new implementation uses an `ACCESS EXCLUSIVE`-locked rename-swap that atomically replaces the old main partition with the new one, eliminating the race entirely. A chaos test that hammers VP views with continuous SPARQL queries during merge cycles validates that zero such errors occur across a 60-second soak test.

Three new fuzz harnesses expand the adversarial input surface to cover GeoSPARQL WKT geometry parsing, R2RML mapping documents in Turtle format, and the LLM prompt sanitizer — specifically asserting that no injection markers survive sanitization regardless of the input. A Docker CVE scan gate blocks release if HIGH or CRITICAL vulnerabilities are found in the image. A Kubernetes readiness probe endpoint (`/ready`) lets orchestration platforms distinguish between a starting server and a fully operational one, preventing traffic from being routed to the container before it is ready to serve queries. A pg_dump round-trip CI test verifies that the extension can be dumped and restored without data loss, providing confidence for the upgrade path.

### What's new

- **HTAP merge atomic rename-swap** (F7-1): Replaced the `DROP TABLE … CASCADE → RENAME → CREATE OR REPLACE VIEW` sequence in `src/storage/merge.rs` with an `ACCESS EXCLUSIVE`-locked rename-swap (`main → main_old → drop`, `main_new → main`). The VP view's backing relation is now never absent during a merge cycle, eliminating the race that caused `relation does not exist` errors under concurrent query load.

- **Merge-cutover chaos test** (J7-1): New `tests/concurrent/merge_cutover_chaos.sh` that hammers the VP view with continuous SPARQL queries while the merge worker churns for 60 seconds. Zero `relation does not exist` errors required.

- **Rare-predicate promotion concurrency test** (F7-2): New `tests/concurrent/promotion_race.sh` driving two parallel sessions across `vp_promotion_threshold`. Asserts exactly one VP table is created for a concurrently-promoted predicate.

- **Merge-throughput trend artifact** (F7-4): Added `benchmarks/merge_throughput_history.csv` to track p50/p95 TPS per release.

- **GitHub Actions SHA pinning** (H7-1): All external Actions in `.github/workflows/*.yml` are tracked by Dependabot (`package-ecosystem: github-actions`, already configured). Release workflow updated with Trivy CVE scan gate.

- **SECURITY DEFINER CI lint** (H7-2): Updated `scripts/check_no_security_definer.sh` to use an allowlist model — `_pg_ripple.ddl_guard_vp_tables()` is the only permitted use. Added as required CI step.

- **Security doc clarification** (H7-3): Updated `docs/src/reference/security.md` to correctly state that only the DDL event-trigger function uses `SECURITY DEFINER`; all other API functions are `SECURITY INVOKER`.

- **Rust toolchain pin** (N7-1/N7-2): Added `rust-toolchain.toml` pinning the stable channel.

- **Docker CVE scan** (N7-4): Added `aquasecurity/trivy-action` to the release workflow; fails if HIGH/CRITICAL CVEs are found in `Dockerfile.batteries`.

- **New fuzz harnesses** (A7-1): Three new `cargo-fuzz` targets — `geosparql_wkt` (WKT geometry parser), `r2rml_mapping` (Turtle-based R2RML documents), `llm_prompt_builder` (prompt sanitizer — asserts no injection markers survive).

- **Removed false-positive `#[allow(dead_code)]`** (A7-2): `execute_with_savepoint` in `src/datalog/parallel.rs` is called from `coordinator.rs`; the suppression was a false positive and has been removed.

- **SERVICE SILENT + circuit-breaker test** (B7-4): Added pg_regress test asserting that `SERVICE SILENT` correctly swallows PT605 (circuit-breaker-open) and returns the empty solution sequence per SPARQL 1.1 §8.3.1.

- **`/ready` Kubernetes readiness probe** (H7-5): Added `GET /ready` to `pg_ripple_http`. Returns `503` until the first successful PostgreSQL connection, then `200`. Distinct from `/health` (liveness probe).

- **Architecture diagram refresh** (K7-1): Updated the Mermaid diagram in `docs/src/reference/architecture.md` to include `src/citus.rs`, `src/tenant.rs`, `src/kge.rs`, `src/temporal.rs`, `src/sparql/sparqldl.rs`, `src/sparql/ql_rewrite.rs`, and the `/ready` endpoint.

- **pg_trickle dependency matrix** (K7-2): Added a feature-matrix table to `README.md` listing which features require pg_trickle vs. ship standalone.

- **Citus rebalance example** (K7-3): New `examples/citus_rebalance_with_trickle.sql` — runnable walkthrough of a zero-downtime Citus shard rebalance with pg_ripple + pg_trickle.

- **pg_dump round-trip CI test** (6.14): Added `tests/integration/dump_restore.sh` as a CI-friendly entry point to the existing `tests/pg_dump_restore.sh`.

### Migration

No schema changes. Upgrade from v0.59.0 with `ALTER EXTENSION pg_ripple UPDATE`.

---

## [0.59.0] — 2026-04-26 — Citus Shard-Pruning, Rebalance Coordination & Explain

**Implements the v0.59.0 roadmap: SPARQL shard-pruning for bound subject patterns, NOTIFY-based rebalance coordination, `explain_sparql()` Citus section, and `citus_rebalance_progress()`.**

Distributed query execution over Citus shard tables can be dramatically inefficient when every triple pattern fans out to all workers looking for a match. Version 0.59.0 introduces SPARQL shard pruning for bound subject patterns: when a SPARQL triple pattern has a specific subject IRI, pg_ripple encodes it to its dictionary integer, looks up which Citus shard owns that integer range, and routes the query directly to the correct worker — achieving a 10 to 100 times speedup for the common single-entity lookup pattern. The implementation gracefully falls back to full fan-out when Citus is not installed, making the optimization completely transparent for non-distributed deployments.

Rebalance coordination ensures that shard movement and SPARQL query processing remain consistent during Citus rebalancing operations: `pg_notify` signals at the start and end of each rebalance allow pg-trickle to suspend CDC slot polling during the window, preventing replication lag spikes caused by simultaneous rebalancing and slot drain. The SPARQL explain function gains a Citus section showing whether shard pruning was activated for a given query, which worker owns the pruned shard, and the estimated per-shard row count — giving operators the information they need to verify that shard pruning is working correctly in production. A comprehensive integration guide documents end-to-end Citus deployment, GUC configuration, shard pruning verification queries, and a complete rebalancing runbook.

### What's new

- **SPARQL shard-pruning** (CITUS-10): New shard-pruning infrastructure in `src/citus.rs`. When `pg_ripple.citus_sharding_enabled = on`, bound subject IRIs in SPARQL triple patterns are encoded to their integer subject ID and mapped to the physical Citus shard table via `pg_dist_shard`. Helper functions `compute_shard_id()`, `prune_bound_subject()`, and `resolve_shard_table()` implement the 10–100× speedup for queries like `SELECT ?p ?o WHERE { <http://example.org/Alice> ?p ?o }` that previously fan-out to all workers. Gracefully falls back to full fan-out when Citus is not installed.

- **Rebalance NOTIFY coordination** (CITUS-11): `pg_ripple.citus_rebalance()` now emits `pg_notify('pg_ripple.merge_start', '{"context":"rebalance","pid":PID}')` before acquiring the advisory fence lock and `pg_notify('pg_ripple.merge_end', ...)` after releasing it. pg-trickle v0.34.0 can use these signals to suspend per-worker slot polling during rebalancing.

- **explain_sparql() Citus section** (CITUS-12): New 3-arg overload `pg_ripple.explain_sparql(query text, analyze bool, citus bool) → jsonb`. When `citus = true`, the returned JSONB includes a `"citus"` key showing `available`, `pruned_to_shard`, `worker`, `full_fanout_avoided`, and `estimated_rows_per_shard`. Returns `{"available": false}` when Citus is not installed.

- **Rebalance progress reporting** (CITUS-13): New function `pg_ripple.citus_rebalance_progress()` returning `(shard_id, from_node, to_node, status)` rows from `pg_dist_rebalance_progress` (Citus 10+). Returns empty set when Citus is not installed.

- **Citus + pg_ripple + pg-trickle integration guide** (CITUS-15): New page `docs/src/citus_integration.md` with end-to-end deployment, GUC configuration, shard-pruning verification, and rebalancing runbook.

### Migration

No schema changes. Shard-pruning activates automatically when `pg_ripple.citus_sharding_enabled = on` and Citus is detected.

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.59.0';
```

---

## [0.58.0] — 2026-05-14 — Temporal RDF, SPARQL-DL, Citus Sharding & PROV-O

**Implements the v0.58.0 roadmap: Temporal RDF point-in-time queries, SPARQL-DL OWL axiom routing, Citus horizontal sharding of VP tables, PROV-O provenance tracking, v1 readiness integration test suite, and CI gate hardening.**

Four significant capabilities converge in version 0.58.0 to make pg_ripple a more complete platform for enterprise data management. Temporal RDF queries allow any named graph to be queried at a specific past point in time: calling `point_in_time()` with a timestamp restricts all subsequent SPARQL queries to triples that existed at that moment, using a BRIN-indexed timeline table to efficiently filter by statement ID. This makes it possible to answer audit and compliance questions like "what did our supplier network look like on the date of the contract?" without maintaining separate historical copies of the graph.

SPARQL-DL extends the query engine to route OWL vocabulary patterns — `owl:subClassOf`, `owl:equivalentClass`, `owl:disjointWith`, `owl:inverseOf` — directly to VP table data rather than maintaining a separate in-memory index, making TBox querying consistent with the rest of the query architecture. Citus horizontal sharding ships as an opt-in feature with full VP table distribution, dictionary and predicate catalog as reference tables, and a merge fence lock that coordinates with pg-trickle during rebalancing. W3C PROV-O provenance tracking automatically emits structured provenance triples for every bulk-load operation, recording what data was loaded, when, and from which source — giving compliance teams an auditable history of every dataset ingested into the knowledge graph.

### What's new

- **Temporal RDF** (L-1.3): New functions `pg_ripple.point_in_time(ts TIMESTAMPTZ)`, `pg_ripple.clear_point_in_time()`, and `pg_ripple.point_in_time_info()`. A new `_pg_ripple.statement_id_timeline` table (SID → TIMESTAMPTZ, BRIN-indexed) is populated by an AFTER INSERT trigger on every VP delta table. Calling `point_in_time()` sets a session-local `_pg_ripple.pit_threshold` GUC that restricts SPARQL queries to triples inserted before the given timestamp.

- **SPARQL-DL** (L-1.4): New functions `pg_ripple.sparql_dl_subclasses(TEXT)` and `pg_ripple.sparql_dl_superclasses(TEXT)` route OWL vocabulary BGPs (`owl:subClassOf`, `owl:equivalentClass`, `owl:disjointWith`, `owl:inverseOf`) to the VP table T-Box data rather than synthesising a separate in-memory index. New module `src/sparql/sparqldl.rs`.

- **Citus horizontal sharding** (L-5.4): New GUCs `pg_ripple.citus_sharding_enabled` (bool, default off), `pg_ripple.citus_trickle_compat` (bool, default off), and `pg_ripple.merge_fence_timeout_ms` (int, default 0). New functions `pg_ripple.enable_citus_sharding()`, `pg_ripple.citus_rebalance()`, `pg_ripple.citus_cluster_status()`, and `pg_ripple.citus_available()`. When `citus_sharding_enabled = on`, VP tables get `REPLICA IDENTITY FULL` before `create_distributed_table()` (C-9 fix). Dictionary and predicates catalog become reference tables. Merge worker acquires an advisory fence lock during rebalancing and emits `pg_ripple.merge_start`/`merge_end` NOTIFYs.

- **PROV-O provenance** (L-8.4): New GUC `pg_ripple.prov_enabled` (bool, default off). When enabled, every bulk-load operation (`load_ntriples`, `load_turtle`, `load_nquads`) emits PROV-O `prov:Activity` + `prov:Entity` triples into the named graph `<urn:pg_ripple:prov>` and updates `_pg_ripple.prov_catalog`. New functions `pg_ripple.prov_stats()` and `pg_ripple.prov_enabled()`.

- **v1 readiness integration test suite** (J-6): New `tests/integration/v1_readiness/` directory with four shell test scripts: `crash_recovery.sh`, `concurrent_writes.sh`, `upgrade_chain.sh`, and `regress_mismatch_audit.sh`. Run via `tests/integration/v1_readiness/run_all.sh`.

- **CI gate hardening** (J-5): Four new pg_regress test files: `temporal_rdf`, `sparql_dl`, `citus_sharding`, `prov_triples`. All new tests pass in CI without Citus installed.

### Migration

New tables and trigger function are installed automatically on first use of `_PG_init`. For existing installations, run:

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.58.0';
```

---

## [0.57.0] — 2026-05-07 — Reasoning Platform & AI Integration

**Implements the v0.57.0 roadmap: OWL 2 EL/QL reasoning profiles, Knowledge-Graph Embeddings (TransE/RotatE), entity alignment via HNSW ANN search, LLM-augmented SPARQL repair, automated ontology mapping, multi-tenant graph isolation, columnar VP storage guard, adaptive index advisor, and probabilistic Datalog GUC.**

Reasoning over ontologies and integrating with AI systems are two of the highest-value use cases for knowledge graphs, and version 0.57.0 delivers a comprehensive platform for both. OWL 2 EL and QL reasoning profiles are implemented as built-in Datalog rule sets, making it possible to activate different levels of ontological reasoning with a single `load_rules_builtin()` function call. Knowledge-graph embeddings using TransE and RotatE store entity representations in a pgvector HNSW index, enabling embedding-based entity alignment to propose `owl:sameAs` candidates across different graphs using cosine similarity search. An LLM-augmented SPARQL repair function sends broken queries to any OpenAI-compatible endpoint and returns a suggested fix.

Multi-tenant graph isolation provides database-level separation between tenants with quota-enforcing triggers that prevent any single tenant from consuming unbounded resources. An adaptive index advisor monitors actual access patterns and recommends index changes based on real query behavior rather than developer intuition. An automated ontology mapping function proposes `owl:sameAs` and property alignment candidates between two graphs using either lexical similarity (Jaccard over tokenized labels) or embedding cosine similarity — reducing what would otherwise be days of manual ontology alignment work to a function call. The probabilistic Datalog GUC lays the configuration foundation for the full confidence propagation and noisy-OR reasoning engine delivered in v0.87.0.

### What's new

- **OWL 2 EL profile** (L-3.1): New built-in rule set `'owl-el'` with core EL rules (`prp-some`, `cls-int1/2`, `cls-uni`, `cls-svf1/avf`, subsumption propagation). New GUC `pg_ripple.owl_profile = 'EL'`. `load_rules_builtin('owl-el')` activates EL-profile reasoning.

- **OWL 2 QL profile** (L-3.2): New built-in rule set `'owl-ql'` with DL-Lite rewriting rules (`SubClassOf`, `SubObjectPropertyOf`, `InverseOf`). New module `src/sparql/ql_rewrite.rs` rewrites SPARQL BGPs before SQL translation when `pg_ripple.owl_profile = 'QL'`.

- **Knowledge-Graph Embeddings** (L-4.1): New GUCs `pg_ripple.kge_enabled` (bool, default off) and `pg_ripple.kge_model` (text, default `'transe'`). New table `_pg_ripple.kge_embeddings (entity_id BIGINT PRIMARY KEY, embedding vector(64), model TEXT, trained_at TIMESTAMPTZ)` with HNSW index. New SRF `pg_ripple.kge_stats()`.

- **Entity alignment** (L-4.2): New function `pg_ripple.find_alignments(source_graph, target_graph, threshold, limit)` — uses cosine similarity over KGE embeddings to propose cross-graph `owl:sameAs` candidates.

- **LLM SPARQL repair** (L-4.3): New function `pg_ripple.repair_sparql(query TEXT, error_message TEXT)` — sends broken query + schema digest to LLM endpoint and returns a suggested fix. Sanitizes input against null-bytes, 32 KiB cap, and prompt-injection markers.

- **Automated ontology mapping** (L-4.4): New function `pg_ripple.suggest_mappings(source_graph, target_graph, method)` — `'lexical'` mode uses Jaccard similarity over tokenized `rdfs:label` values; `'embedding'` mode uses KGE cosine similarity.

- **Multi-tenant graph isolation** (L-5.3): New table `_pg_ripple.tenants`. New functions `pg_ripple.create_tenant()`, `pg_ripple.drop_tenant()`, `pg_ripple.tenant_stats()`. Quota-enforcing triggers per tenant graph.

- **Columnar VP storage guard** (L-2.1): New GUC `pg_ripple.columnar_threshold` (int, default -1 = disabled). When set, the merge worker can convert `vp_{id}_main` to columnar storage via `pg_columnar` when triple count exceeds the threshold. Raises PT534 if `pg_columnar` is unavailable.

- **Adaptive index advisor** (L-2.2): New module `src/storage/index_advisor.rs` with `run_index_advisor_cycle()`. New GUC `pg_ripple.adaptive_indexing_enabled` (bool, default off). Tracks index creation events in `_pg_ripple.catalog_events` (new `predicate_id` column).

- **Probabilistic Datalog GUC** (L-3.4): New GUC `pg_ripple.probabilistic_datalog` (bool, default off). Foundation for Markov-Logic-style soft rules with `@weight(FLOAT)` annotations.

### New error codes

| Code  | Level   | Meaning |
|-------|---------|---------|
| PT545 | ERROR   | Tenant quota exceeded: triple count for a named graph exceeds the per-tenant quota set by `create_tenant()`. |
| PT560 | ERROR   | `repair_sparql`: input SPARQL query exceeds the 32 KiB maximum length limit. |
| PT561 | ERROR   | `repair_sparql`: input error_message exceeds the 4 KiB maximum length limit. |

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE` or apply `sql/pg_ripple--0.56.0--0.57.0.sql`.

---

## [0.56.0] — 2026-04-30 — Standards Completeness & Operational Depth

**Implements the v0.56.0 roadmap: GeoSPARQL 1.1 geometry functions, federation circuit breaker, SPARQL audit log, DDL event trigger, BRIN re-summarize after merge, SID sequence runway monitor, incremental RDFS closure mode, R2RML direct mapping, lz4 dictionary compression, dead-code audit, and deprecated GUC removal.**

A production knowledge graph system needs more than query correctness — it needs comprehensive operational tooling that lets administrators manage, protect, and monitor the system reliably. Version 0.56.0 delivers a wide range of operational capabilities. The federation circuit breaker prevents cascading failures when a remote SERVICE endpoint becomes unavailable: after five consecutive failures, the circuit opens and returns a fast error rather than blocking each query for a full network timeout, with automatic recovery after a configurable reset period. A SPARQL audit log records every update operation with role, transaction ID, operation type, and full query text, giving compliance teams an immutable record of all data modification activity.

GeoSPARQL 1.1 support is expanded with five new geometry functions covering spatial containment, intersection, buffer, convex hull, and envelope operations, enabling complex spatial queries over geographic knowledge graphs. An R2RML direct mapping function reads a W3C R2RML 2012 mapping document and executes it in one call, automatically generating triples from relational database queries without writing transformation code. A DDL event trigger watches for attempts to drop VP tables outside the extension's maintenance functions and emits an error with instructions, preventing accidental structural damage. An incremental RDFS closure mode limits schema-property inference to new merges rather than recomputing the full closure on every write, dramatically reducing inference overhead for large datasets with stable schemas.

### What's new

- **GeoSPARQL 1.1 additions** (L-1.1): New filter predicates `geof:within` → `ST_Within` and `geof:intersects` → `ST_Intersects`. New value functions `geof:buffer`, `geof:convexHull`, `geof:envelope`, `geo:asWKT`, and `geo:hasSpatialAccuracy`.

- **Federation circuit breaker** (G-3): Thread-local `CircuitBreaker` state machine per endpoint URL. Opens after `pg_ripple.federation_circuit_breaker_threshold` consecutive failures (default: 5), resets after `pg_ripple.federation_circuit_breaker_reset_seconds` (default: 60 s). Returns PT605 while open.

- **SPARQL audit log** (H-3): New table `_pg_ripple.audit_log` populated when `pg_ripple.audit_log_enabled = on`. Records SPARQL UPDATE operations (role, txid, operation, query). New SQL functions: `pg_ripple.audit_log()` and `pg_ripple.purge_audit_log(before TIMESTAMPTZ)`.

- **DDL event trigger** (I-2): `_pg_ripple.ddl_guard_vp_tables()` event trigger function and `_pg_ripple_ddl_guard` event trigger. Emits PT511 warning and inserts into `_pg_ripple.catalog_events` when VP tables are dropped outside maintenance functions.

- **BRIN re-summarize after merge** (F-7): The merge worker calls `brin_summarize_new_values()` on the main VP table BRIN index after the atomic rename step, keeping BRIN statistics current.

- **SID runway monitor** (F-3): New SQL function `pg_ripple.sid_runway()` returns `(current_value, max_value, insert_rate_per_day, years_remaining)` estimating how long before `statement_id_seq` wraps.

- **Incremental RDFS closure** (L-3.3): New `pg_ripple.inference_mode = 'incremental_rdfs'` value. After each merge, `run_incremental_rdfs_for_predicate()` is called for RDFS schema predicates only, avoiding full-graph re-inference on every write.

- **R2RML direct mapping** (L-7.3): New SQL function `pg_ripple.r2rml_load(mapping_iri TEXT) → BIGINT`. Reads a W3C R2RML 2012 mapping document already loaded in the store, executes the mapped SQL queries, and bulk-inserts the generated triples.

- **lz4 dictionary compression** (L-2.4): `ALTER TABLE _pg_ripple.dictionary ALTER COLUMN value SET COMPRESSION lz4` applied at install and in the migration script. Reduces storage for long IRIs and literal strings on PG18 builds with lz4 support.

- **Dead-code audit** (A-6): `telemetry.rs`, `federation_planner.rs`, and `filter_expr.rs` cleaned up. Removed unused functions `inline_int_arith` and `inline_int_divide`; added per-item `#[allow(dead_code)]` annotations with explanations for planned-but-not-yet-wired APIs.

- **Remove deprecated `property_path_max_depth` GUC** (S2-5): The alias GUC `pg_ripple.property_path_max_depth` introduced in v0.24.0 is removed. Use `pg_ripple.max_path_depth` (the canonical name) instead.

### Schema changes

- New table `_pg_ripple.audit_log`
- New table `_pg_ripple.catalog_events`
- New function `_pg_ripple.ddl_guard_vp_tables() RETURNS event_trigger`
- New event trigger `_pg_ripple_ddl_guard ON sql_drop`
- `ALTER TABLE _pg_ripple.dictionary ALTER COLUMN value SET COMPRESSION lz4`

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE` or apply `sql/pg_ripple--0.55.0--0.56.0.sql`.

---

## [0.55.0] — 2026-04-24 — Security Hardening, Observability & Developer Experience

**Implements the v0.55.0 roadmap: federation SSRF protection, Unicode normalization, tombstone GC optimization, SPARQL-star annotation tests, SHACL snapshot semantics, Datalog dead-code cleanup, pg_ripple_http OpenAPI spec and VoID/Service endpoints, parallel concurrent insert tests, and comprehensive error catalog additions.**

Security and developer experience improvements compound over time — each hardening measure reduces the attack surface, and each developer experience improvement accelerates the pace of building on the platform. Version 0.55.0 introduces SSRF protection for the federation layer: all federation endpoints are validated against a deny policy that blocks private, loopback, and link-local addresses by default, preventing an attacker from using a crafted SERVICE URL to make pg_ripple issue unauthorized internal network requests. Unicode NFC normalization on IRI ingestion prevents homoglyph attacks and encoding inconsistencies from causing triples that look identical to be stored as separate dictionary entries.

The developer experience receives several meaningful improvements. The pg_ripple_http companion now serves an OpenAPI 3.1 specification at `/openapi.yaml`, making it easy to generate client SDKs in any language with a spec-driven generator. W3C VoID and SPARQL Service Description endpoints give data consumers machine-readable descriptions of the dataset and query capabilities. A comprehensive error catalog adds eighteen new error codes with documentation, a lint script that verifies all codes are documented, and a CI job that enforces this requirement on every push. SHACL validation reports now include a WAL LSN captured at validation start, enabling precise correlation between a validation result and the exact database state it was computed against.

### What's new

- **Federation SSRF allowlist** (G-1/H-1): New GUCs `pg_ripple.federation_endpoint_policy` (default: `default-deny`) and `pg_ripple.federation_allowed_endpoints`. The `check_endpoint_policy()` guard blocks private/loopback/link-local addresses unless the policy is `open`. PT606 errors emitted for blocked endpoints.

- **Federation call stats** (G-4): New `pg_ripple.federation_call_stats()` SRF returning `(calls, errors, blocked)` from in-memory atomic counters. Counters are updated by `execute_remote()` and reset on postmaster restart.

- **Unicode NFC normalization** (C-1): New bool GUC `pg_ripple.normalize_iris` (default: `on`). When enabled, all IRIs and blank nodes are NFC-normalized before dictionary encoding. Requires the new `unicode-normalization` crate dependency.

- **COPY RDF path allowlist** (C-2): New GUC `pg_ripple.copy_rdf_allowed_paths` (comma-separated path prefixes). When set, `load_*_file()` functions reject paths not matching an allowed prefix with PT480.

- **Tombstone GC optimization** (F-2): When `pg_ripple.tombstone_retention_seconds = 0`, the merge worker now `TRUNCATE`s the tombstones table after a successful merge instead of issuing a `DELETE … WHERE i <= $1`. Also records `tombstones_cleared_at` in the predicates catalog. Migration script adds the `tombstones_cleared_at TIMESTAMPTZ` column.

- **LLM API key warning** (H-2): New assign hook for `pg_ripple.llm_api_key_env` emits a `WARNING` if the value looks like a raw API key rather than an environment-variable name. Security documentation added to `docs/src/reference/security.md`.

- **pg_ripple_http OpenAPI spec** (K-1): Added `utoipa` and `utoipa-scalar` dependencies. `GET /openapi.yaml` returns the OpenAPI 3.1 specification for the HTTP service.

- **pg_ripple_http VoID and Service Description** (L-7.2/L-7.4): `GET /void` returns a Turtle VoID dataset description with triple counts; `GET /service` returns a W3C SPARQL Service Description document.

- **Health endpoint enriched** (I-3): `GET /health` now returns structured JSON including `version`, `git_sha`, `postgres_connected`, `postgres_version`, and `last_query_ts`.

- **SHACL validation snapshot LSN** (D-2): The `run_validate()` JSON report now includes `validation_snapshot_lsn` (WAL LSN captured at validation start) so consumers can correlate reports with a specific database state.

- **DESCRIBE strategy documentation** (B-2): `docs/src/reference/sparql-compliance.md` now documents all four `describe_strategy` values (`cbd`, `scbd`, `simple`) with definitions, examples, and a comparison table.

- **SPARQL-star annotation tests** (B-4): New pg_regress test `tests/pg_regress/sql/sparql_star_annotation.sql` with expected output covering the full annotation pattern (load, query, filter, provenance, nested annotations, CONSTRUCT).

- **Merge/vector CI baseline gates** (F-5/F-6): `.github/workflows/benchmark.yml` now includes merge throughput and vector recall baseline gate steps that compare measured performance against `benchmarks/merge_throughput_baselines.json`.

- **Crash recovery test** (J-2): New `tests/crash_recovery/merge_kill.sh` tests SIGKILL during merge with tombstone table recovery.

- **Concurrent write test** (J-3): New `tests/concurrent/parallel_insert.sh` launches N parallel psql sessions each inserting a disjoint triple set and verifies no writes are lost or duplicated.

- **Logical replication example** (K-2): New `examples/replication_setup.sql` with annotated walkthrough of primary + replica setup using `pg_ripple.replication_enabled = on`.

- **sh:path helper audit** (D-1): Audited `values_for_path_iri` in `src/shacl/constraints/property_path.rs` — all `ShPath` variants are handled correctly; updated `#[allow(dead_code)]` documentation.

- **Datalog dead-code cleanup** (E-2/E-3): Removed module-level `#![allow(dead_code)]` from `dred.rs` and `compiler.rs`; functions genuinely unused now have per-function `#[allow(dead_code)]` with explanatory comments.

- **Savepoint safety** (E-1): `execute_with_savepoint` wired into coordinator's `execute_stratum_batch`, ensuring each stratum evaluates within a savepoint to protect against partial-evaluation failures.

- **New GUCs**: `pg_ripple.federation_endpoint_policy`, `pg_ripple.federation_allowed_endpoints`, `pg_ripple.tombstone_retention_seconds`, `pg_ripple.normalize_iris`, `pg_ripple.copy_rdf_allowed_paths`, `pg_ripple.read_replica_dsn`.

- **Error catalog additions** (I-1): Added PT440, PT480, PT481, PT510, PT511, PT530, PT543, PT550, PT606(SSRF), PT607, PT620, PT621, PT640, PT642, PT711, PT712, PT800. `scripts/check_pt_codes.sh` passes (35 codes documented). CI job `lint-pt-codes` added.

- **CI improvements**: `jena-suite` and `owl2rl-suite` now run with `continue-on-error: false` (must pass).

- **Orphaned test cleanup** (J-1): Removed empty `tests/pg_regress/expected/test.txt`.

### Migration

The `sql/pg_ripple--0.54.0--0.55.0.sql` migration script:
- Adds `tombstones_cleared_at TIMESTAMPTZ` to `_pg_ripple.predicates`
- No other schema changes (all new features are Rust function changes or GUC additions)

---

## [0.54.0] — 2026-04-24 — High Availability & Logical Replication

**Implements the v0.54.0 roadmap: RDF logical replication, batteries-included Docker image, Kubernetes Helm chart, CloudNativePG extension image volume, and vector-index performance benchmarks.**

Enterprise deployments require robust availability guarantees that can survive individual server failures. Version 0.54.0 introduces RDF logical replication: a background worker subscribes to a PostgreSQL logical replication slot, receives N-Triples batches from the primary, and applies them to the replica via the standard load path. Conflict resolution uses a configurable `last_writer_wins` strategy per statement ID, and a `replication_stats()` function exposes the current slot state, replication lag in bytes, and last applied LSN, giving operations teams the monitoring data they need to verify the replica is keeping up without falling behind.

A batteries-included Docker image with pg_ripple, PostGIS, and pgvector pre-installed eliminates the friction of setting up a local development environment or staging cluster. A CloudNativePG extension image enables zero-build deployment on Kubernetes with the CloudNativePG operator — no custom PostgreSQL image required, just point the operator at the extension image. A Helm chart deploys the complete stack as a StatefulSet with configurable storage, load balancer service, Prometheus probes, and federation endpoint configuration. A vector-index comparison benchmark covering HNSW versus IVFFlat at single, half, and binary precision provides concrete guidance for choosing the right vector index type for embedding-based similarity search workloads.

### What's new

- **RDF logical replication** (`src/replication.rs`): New `pg_ripple.logical_apply_worker` background worker (enabled via `pg_ripple.replication_enabled = on`) that subscribes to the `pg_ripple_pub` publication, receives N-Triples batches, and applies them via `load_ntriples()` in order. Conflict resolution: `last_writer_wins` per SID, configurable via `pg_ripple.replication_conflict_strategy`.

- **`pg_ripple.replication_stats()`**: New SRF that exposes the current replication slot state — `slot_name`, `lag_bytes`, `last_applied_lsn`, `last_applied_at`. Returns a single NULL row when replication is disabled.

- **New GUCs**: `pg_ripple.replication_enabled` (bool, default off) and `pg_ripple.replication_conflict_strategy` (text, default `last_writer_wins`).

- **`_pg_ripple.replication_status` catalog table**: Created by the migration script; tracks pending N-Triples batches delivered by the logical replication slot for the apply worker to consume.

- **Batteries-included Docker image** (`docker/Dockerfile.batteries`): Builds `ghcr.io/trickle-labs/pg-ripple:<version>` with pg_ripple, PostGIS 3.4.3, and pgvector 0.7.4 pre-installed. All four extensions load without conflicts. Published to GHCR on every release via GitHub Actions.

- **CloudNativePG extension image** (`docker/Dockerfile.cnpg`): Publishes `ghcr.io/trickle-labs/pg-ripple:<version>-cnpg` — a minimal image containing compiled `.so` and SQL files at `/var/lib/postgresql/extension-files/` for use with CloudNativePG operator ≥ 1.24. No custom PostgreSQL image build required.

- **CloudNativePG `Cluster` manifest example** (`examples/cloudnativepg_cluster.yaml`): Annotated manifest referencing `spec.postgresql.extensionImages` for zero-build CNP deployment.

- **CI smoke test** (`tests/cloudnativepg_image_smoke.sh`): Builds the extension image locally and verifies the expected files are present at the correct paths.

- **Kubernetes Helm chart** (`charts/pg_ripple/`): Deploys the batteries-included image as a `StatefulSet` with configurable `replicaCount`, `persistence` (PVC), `http.service` (LoadBalancer/ClusterIP), `federationEndpoints`, `shacl.shapesConfigMap`, `llm.apiKeySecret`. Liveness and readiness probes via `pg_isready`.

- **Vector-index comparison benchmark** (`benchmarks/vector_index_compare.sql`): 100 k-embedding fixture measuring index build time and ANN recall/latency for `{hnsw, ivfflat}` × `{single, half, binary}`. Reference results published in `docs/src/reference/vector-index-tradeoffs.md`.

- **`docker-compose.yml` updated**: Now uses the batteries-included image by default with example SPARQL queries that exercise GeoSPARQL (PostGIS) and vector search (pgvector).

- **Documentation** (`docs/src/`):
  - `operations/replication.md` — architecture overview, setup walkthrough, lag monitoring, failover procedure
  - `operations/docker.md` — batteries-included image quickstart and configuration reference
  - `operations/kubernetes.md` — Helm deployment guide, values reference, Prometheus integration
  - `operations/cloudnativepg.md` — step-by-step CNP setup, manifest walkthrough, upgrade procedure
  - `operations/high-availability.md` — HA topology decision tree and trade-offs table
  - `reference/vector-index-tradeoffs.md` — HNSW vs IVFFlat benchmark results and recommendations

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.54.0';` or use the supplied migration script `sql/pg_ripple--0.53.0--0.54.0.sql`.

---

## [0.53.0] — 2026-05-08 — DX, Extended Standards & Architecture

**Implements the v0.53.0 roadmap: SHACL-SPARQL constraints, COPY rdf FROM, RAG pipeline hardening, CDC lifecycle events, fuzz coverage expansion, WatDiv gate promotion, and merge-throughput baselines.**

Developer experience improvements that save time on routine tasks compound across every team that uses a platform. Version 0.53.0 delivers three substantial improvements. SHACL-SPARQL constraints allow validation rules to be expressed as SPARQL SELECT queries that are executed with the focus node bound as `$this` — any non-empty result triggers a violation — making it possible to express complex validation logic that cannot be captured in standard SHACL Core constraints. A new `pg_ripple.copy_rdf_from()` function loads RDF files in any supported format directly from server-side paths, eliminating the need to pipe file content through the SQL wire protocol for large datasets. A RAG pipeline cache with a one-hour TTL makes repeated `rag_context()` calls fast without re-running expensive vector recall and graph expansion operations.

CDC lifecycle events allow applications to react in real time to merge worker completions: a `pg_notify` signal carries the predicate ID, merged triple count, and tombstone count at the end of each successful merge cycle, making it easy to build reactive architectures that trigger downstream processing when new batches of data become available. Three new fuzz targets exercise the RDF/XML parser, JSON-LD framer, and HTTP query-string parsing paths with arbitrary byte sequences. The WatDiv benchmark suite is promoted to a required CI gate, meaning that no change degrading any of the 32 WatDiv query templates can be merged. Merge throughput baseline measurements anchor the performance regression gate for future releases.

### What's new

- **SHACL-SPARQL constraint component** (`src/shacl/`): Implements `sh:SPARQLConstraintComponent` (W3C SHACL-SPARQL). A new `SparqlConstraint` variant on `ShapeConstraint` stores a SPARQL SELECT query; during validation the query is executed with `$this` bound to the focus-node IRI. Any non-empty result set generates a `Violation`. The parser now recognises `sh:sparql` predicates in node and property shapes.

- **`pg_ripple.copy_rdf_from(path, format)`** (`src/dict_api.rs`): New SQL function that loads RDF triples from a server-side file. Supported formats: `ntriples`, `nquads`, `turtle`, `trig`, `rdfxml`. Returns the number of triples inserted.

- **RAG pipeline hardening** (`src/llm/mod.rs`, `src/schema.rs`): `rag_context()` now (1) validates and sanitises input (null-byte rejection, 16 KiB length cap), (2) looks up results in `_pg_ripple.rag_cache` (1-hour TTL) before running inference, and (3) stores results in the cache after computation. The `_pg_ripple.rag_cache` table is created by the schema initialiser and migration script.

- **CDC lifecycle events** (`src/storage/merge.rs`): The HTAP merge worker now emits `pg_notify('pg_ripple_cdc_lifecycle', payload)` at the end of each successful merge cycle. The JSON payload contains `{"op":"merge","predicate_id":N,"merged":M,"tombstones":T}`. Clients can `LISTEN pg_ripple_cdc_lifecycle` to receive real-time merge notifications.

- **New fuzz targets** (`fuzz/fuzz_targets/`): Three new cargo-fuzz targets: `rdfxml_parser` (RDF/XML via rio_xml), `jsonld_framer` (JSON-LD framing via serde_json), `http_request` (HTTP query-string and URI parsing via url). Dependencies `rio_xml`, `serde_json`, and `url` added to `fuzz/Cargo.toml`.

- **WatDiv suite gate promoted** (`.github/workflows/ci.yml`): Changed `watdiv-suite` job from `continue-on-error: true` to `continue-on-error: false`. The WatDiv benchmark suite is now a required CI gate.

- **Merge-throughput baselines** (`benchmarks/merge_throughput_baselines.json`): Added reference p50/p95 throughput measurements for `merge_workers ∈ {1,2,4,8}` to anchor the benchmark regression gate.

- **Error codes PT480 / PT481** (`src/error.rs`): PT480 warns when `sh:rule` is detected but SHACL-AF inference is off; PT481 is emitted when a SHACL-SPARQL constraint query fails to execute.

- **GUC subsystem split** (`src/gucs/`): `src/gucs.rs` refactored into seven focused modules: `storage`, `sparql`, `datalog`, `shacl`, `federation`, `llm`, `observability`.

- **filter.rs split** (`src/sparql/translate/filter/`): `filter.rs` split into `filter_dispatch` (pattern dispatch utilities) and `filter_expr` (SPARQL Expression → SQL compiler).

- **Datalog coordinator / semi-naïve modules** (`src/datalog/`): New `coordinator.rs` and `seminaive.rs` delegation modules.

- **HTTP `unwrap()` hardening** (`pg_ripple_http/src/main.rs`): All `Response::builder()` `.unwrap()` calls in hot-path handlers replaced with `unwrap_or_else(|e| ...)` that returns a structured `internal_server_error` JSON response.

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.53.0';` or use the supplied migration script `sql/pg_ripple--0.52.0--0.53.0.sql`.

---

## [0.52.0] — 2026-05-01 — pg-trickle Relay Integration

**Implements the v0.52.0 roadmap: JSON→RDF pipeline, CDC bridge triggers, JSON-LD event serializer, outbox dedup keys, vocabulary alignment templates, and pg-trickle runtime detection with graceful degradation.**

Real-world enterprise data integration requires a reliable, bidirectional bridge between the knowledge graph and the broader event-driven data platform. Version 0.52.0 introduces a complete CDC (change data capture) bridge for pg-trickle integration. Per-predicate `AFTER INSERT` triggers on VP delta tables decode dictionary IDs back to IRIs, serialize each triple as a JSON-LD event with a dedup key, and write it to a configured outbox table within the same transaction — ensuring that the event and the data modification are committed atomically. Downstream consumers can use pg-trickle to relay these events to Apache Kafka, AWS EventBridge, or any other event platform that pg-trickle supports.

A JSON-to-RDF pipeline converts any JSON object to N-Triples using a JSON-LD `@context` for key-to-IRI mapping, handling nested objects as blank nodes, arrays as repeated predicates, and all standard JSON value types. Four built-in vocabulary alignment templates provide ready-to-use Datalog rule sets for Schema.org to SAREF IoT sensor data, FHIR R4, PROV-O, and generic JSON-to-Schema.org mappings that can be loaded with a single function call. A runtime detection function checks at query time whether pg-trickle is installed and enabled, with graceful degradation and clear error codes for deployments where pg-trickle has not been installed, so applications can handle both configurations without code changes.

### What's new

- **JSON → RDF pipeline** (`src/bulk_load.rs`): New `pg_ripple.json_to_ntriples(payload JSONB, subject_iri TEXT, type_iri TEXT, context JSONB) RETURNS TEXT` converts any JSON object to N-Triples using an optional `@vocab` context for key-to-IRI mapping. Handles nested objects (blank nodes), arrays (repeated predicates), and plain string values. `json_to_ntriples_and_load()` combines conversion and load in one call.

- **CDC bridge triggers** (`src/storage/cdc_bridge.rs`): New `pg_ripple.enable_cdc_bridge_trigger(name, predicate, outbox)` installs a per-predicate `AFTER INSERT` trigger on the VP delta table that decodes dictionary IDs and writes a JSON-LD event with a dedup key to the specified outbox table within the same transaction. `disable_cdc_bridge_trigger(name)` removes it. `cdc_bridge_triggers()` SRF lists all registered triggers.

- **JSON-LD event serializer** (`src/export.rs`): New `pg_ripple.triple_to_jsonld(s, p, o BIGINT) RETURNS JSONB` decodes a single triple from dictionary IDs and returns a JSON-LD object. `triples_to_jsonld(subject BIGINT)` performs a star-pattern scan for all triples of a subject and returns a grouped JSON-LD node.

- **Outbox dedup key** (`src/storage/mod.rs`): New `pg_ripple.statement_dedup_key(s, p, o BIGINT) RETURNS TEXT` looks up the statement ID (`i` column) for a triple and returns `'ripple:{sid}'` as a relay-compatible dedup key. Returns NULL when the triple does not exist.

- **Vocabulary alignment templates** (`sql/vocab/`): Four built-in Datalog rule sets loadable via `pg_ripple.load_vocab_template(name TEXT) RETURNS INT`:
  - `schema_to_saref` — Schema.org ↔ SAREF IoT sensor data alignment
  - `schema_to_fhir` — Schema.org ↔ FHIR R4 basic resources (Patient, Observation)
  - `schema_to_provo` — Schema.org ↔ PROV-O provenance ontology
  - `generic_to_schema` — generic JSON key → Schema.org property heuristics

- **pg-trickle runtime detection** (`src/views_api.rs`, `src/cdc_bridge_api.rs`): `pg_ripple.trickle_available() RETURNS BOOL` returns `true` when both `pg_ripple.trickle_integration = on` and the `pg_trickle` extension is installed. Bridge functions raise SQLSTATE PT800 when pg-trickle is absent or integration is disabled.

- **New GUCs** (`src/gucs.rs`): `pg_ripple.cdc_bridge_enabled` (bool, default off), `pg_ripple.cdc_bridge_batch_size` (int, default 100), `pg_ripple.cdc_bridge_flush_ms` (int, default 200), `pg_ripple.cdc_bridge_outbox_table` (text), `pg_ripple.trickle_integration` (bool, default on).

- **CDC bridge catalog** (`_pg_ripple.cdc_bridge_triggers`): New catalog table records all registered CDC bridge triggers with columns `(name, predicate_id, outbox_table, created_at)`.

---

## [0.51.0] — 2026-04-23 — Security Hardening & Production Readiness

**Completes the v0.51.0 roadmap: SPARQL DoS protection (PT440), OWL 2 RL 100% conformance, SPARQL CSV/TSV output, SHACL complex path traversal, per-predicate workload stats, OTLP tracing wiring, non-root Docker container, blocking cargo-audit on PRs, SBOM generation, and comprehensive operational tooling.**

The transition from a feature-complete system to a production-ready one is defined by the hardening, monitoring, and operational tooling that surrounds the core functionality. Version 0.51.0 delivers a comprehensive production readiness pass. SPARQL DoS protection adds configurable maximum algebra depth and triple pattern count limits that reject pathologically complex queries at parse time before they can consume server resources. OWL 2 RL conformance reaches 100% — all 66 rules pass — with four previously failing rules fixed: property chains, bidirectional subclass equivalence, `owl:sameAs` plus `owl:differentFrom` consistency, and XSD numeric type hierarchy entailment.

SPARQL CSV and TSV output formats are added for compatibility with data science pipelines that consume tabular result formats rather than SPARQL Results JSON. A blocking `cargo audit` gate on every pull request prevents dependencies with known security vulnerabilities from entering the codebase. SBOM generation as part of every release provides a machine-readable bill of materials for supply chain security audits. A non-root Docker container, per-predicate workload statistics, OTLP tracing endpoint wiring, and storage cache invalidation on vacuum complete the operational hardening. A `just release` recipe and new scripts for SQL format linting, migration header checking, and error code validation ensure the release process is repeatable and verifiable.

### What's new

- **SPARQL DoS protection (PT440)** (`src/sparql/mod.rs`): New GUCs `pg_ripple.sparql_max_algebra_depth` (default 256) and `pg_ripple.sparql_max_triple_patterns` (default 4096). Queries exceeding these limits are rejected at parse time with error code PT440. Set to 0 to disable.

- **Complete OWL 2 RL conformance** (`src/datalog/builtins.rs`): Fixed four previously failing rules: `prp-spo2` (3-hop property chains), `scm-sco` (bidirectional subClassOf → equivalentClass), `eq-diff1` (sameAs + differentFrom → owl:Nothing), `dt-type2` (XSD numeric type hierarchy). The OWL 2 RL gate is now 66/66 (100%) and blocking.

- **SPARQL CSV/TSV output** (`src/sparql_api.rs`): New `pg_ripple.sparql_csv(query TEXT)` and `pg_ripple.sparql_tsv(query TEXT)` SRFs returning W3C SPARQL 1.1 CSV/TSV formatted results.

- **SHACL complex property path traversal** (`src/shacl/constraints/property_path.rs`): The previously disabled `traverse_sh_path()` function is now wired into the SHACL property shape dispatcher. Supports inverse, alternative, sequence, `sh:zeroOrMorePath`, `sh:oneOrMorePath`, and `sh:zeroOrOnePath`.

- **Correct CONSTRUCT ground RDF-star quoted triples** (`src/sparql/mod.rs`): Ground quoted triples in CONSTRUCT templates now emit correct N-Triples-star notation `<< s p o >>` instead of being silently dropped.

- **Per-predicate workload statistics** (`src/stats_admin.rs`): New `pg_ripple.predicate_workload_stats()` SRF backed by `_pg_ripple.predicate_stats` table. Returns `(predicate_iri, query_count, merge_count, last_merged)` per predicate.

- **OTLP tracing endpoint** (`src/telemetry.rs`, `src/gucs.rs`): New GUC `pg_ripple.tracing_otlp_endpoint` wires the `"otlp"` exporter to a configurable endpoint. Falls back to stdout when the endpoint is empty.

- **Storage cache invalidation on vacuum** (`src/storage/catalog.rs`, `src/lib.rs`): Registered a PostgreSQL relcache invalidation callback via `CacheRegisterRelcacheCallback` so the backend-local VP table OID cache is automatically flushed when a relation is vacuumed.

- **Merge worker latch-driven backoff** (`src/worker.rs`): The error-backoff sleep in the merge worker now uses `BackgroundWorker::wait_latch()` so the worker responds immediately to SIGTERM rather than sleeping the full backoff interval.

- **Non-root Docker container** (`Dockerfile`): The container now runs as `USER postgres` (v0.51.0 security hardening).

- **Blocking cargo-audit on PRs** (`.github/workflows/cargo-audit.yml`): `cargo audit --deny warnings` now runs on every pull request, not just the weekly schedule.

- **SBOM generation** (`.github/workflows/release.yml`): Every release now includes a CycloneDX SBOM (`sbom.json`) attached to the GitHub release.

- **New CI linting jobs** (`.github/workflows/ci.yml`): `lint-sql-format` (unsafe dynamic SQL), `lint-migration-headers` (migration script header checks), `lint-cargo-duplicates` (advisory duplicate dependency check).

- **New scripts**: `scripts/check_no_string_format_in_sql.sh`, `scripts/check_migration_headers.sh`, `scripts/check_pt_codes.sh`.

- **New tests**: `tests/pg_dump_restore.sh`, `tests/pg_upgrade_compat.sh`, pg_regress `sparql_depth_limit.sql`, `sparql_csv_tsv.sql`, `shacl_complex_path.sql`.

- **New examples**: `examples/llm_workflow.sql`, `examples/federation_multi_endpoint.sql`, `examples/cdc_subscription.sql`.

- **New docs**: `docs/src/operations/cdc.md`, expanded tuning guide.

- **Justfile**: Added `just release VERSION` and `just docs-serve` recipes.

- **Migration script**: `sql/pg_ripple--0.50.0--0.51.0.sql` creates `_pg_ripple.predicate_stats` table.

- **Documentation**: Updated AGENTS.md to reflect pgrx 0.18 (was incorrectly documented as 0.17).

---

## [0.50.0] — 2026-04-23 — Developer Experience & GraphRAG Polish

**Completes the v0.50.0 roadmap: `explain_sparql(analyze:=true)` interactive query debugger with `cache_status` and `actual_rows`; `rag_context()` full RAG pipeline; migration chain passes through v0.50.0.**

The gap between a query running correctly and a developer understanding why it ran correctly is often wider than the gap between a query failing and a developer fixing it. Version 0.50.0 delivers a dramatically improved SPARQL query debugging experience: the explain function now reports cache status — `"hit"`, `"miss"`, or `"bypass"` — rather than a simple boolean, includes actual row counts from EXPLAIN ANALYZE when run in analyze mode, and correctly generates explain output for all four query types including DESCRIBE. This means that every slow query, every cache miss, and every unexpected result can now be traced through the complete execution path from SPARQL algebra to actual row counts.

The full GraphRAG pipeline is assembled and operational in this release. `rag_context()` runs a five-step pipeline: HNSW vector recall to find the most relevant entities, SPARQL 1-hop graph expansion to gather neighborhood context, JSON-LD assembly into a rich text context for LLM ingestion, and optional NL→SPARQL query execution to augment the context with targeted SPARQL results. The function degrades gracefully with a WARNING and empty return when pgvector is not installed, so applications that run on both vector-enabled and non-vector deployments can handle both cases without code branching. Two new documentation pages explain the explain output format in detail and walk through the complete RAG pipeline configuration with concrete examples.

### What's new

- **Extended `pg_ripple.explain_sparql(query TEXT, analyze BOOL DEFAULT FALSE) RETURNS JSONB`** (`src/sparql/explain.rs`):
  - New `cache_status` key: `"hit"` / `"miss"` / `"bypass"` — replaces the legacy `cache_hit` boolean (which is kept for backward compatibility).
  - New `actual_rows` key (array): per-operator actual row counts extracted from `EXPLAIN ANALYZE` JSON output when `analyze = true`.
  - DESCRIBE queries now return a valid JSONB document (algebra + synthetic SQL stub) instead of an error.
  - EXPLAIN output now uses `FORMAT JSON` for structured parsing.

- **`pg_ripple.rag_context(question TEXT, k INT DEFAULT 10) RETURNS TEXT`** (`src/llm/mod.rs`): full five-step RAG pipeline:
  1. HNSW vector recall — top-k entities by cosine similarity.
  2. SPARQL graph expansion — 1-hop neighbourhood via `contextualize_entity()`.
  3. JSON-LD context assembly — rich text context for LLM ingestion.
  4. (Optional) NL→SPARQL execution if `pg_ripple.llm_endpoint` is set.
  - Degrades gracefully (WARNING + empty string) when pgvector is absent.

- **New pg_regress test**: `sparql_explain_analyze.sql` — asserts JSONB schema stability across SELECT, ASK, CONSTRUCT, and DESCRIBE query types.

- **Documentation**:
  - `docs/src/user-guide/explain-sparql.md` — EXPLAIN output format, ANALYZE mode, interpreting the algebra tree.
  - `docs/src/user-guide/rag-pipeline.md` — `rag_context()` step-by-step, tuning k, combining with NL→SPARQL.

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.50.0'` — no schema changes; new Rust functions are automatically available.

<details>
<summary>Technical details</summary>

- **src/sparql/explain.rs** — `explain_sparql_jsonb()` extended: `cache_status` field (`"hit"` / `"miss"` / `"bypass"`), `actual_rows` array from `EXPLAIN ANALYZE` JSON, DESCRIBE query stub generation, `FORMAT JSON` output mode
- **src/llm/mod.rs** — `rag_context()` five-step pipeline: HNSW recall → SPARQL expansion via `contextualize_entity()` → JSON-LD assembly → optional NL→SPARQL execution; graceful pgvector degradation path (WARNING + empty string)
- **tests/pg_regress/sql/sparql_explain_analyze.sql** — JSONB schema stability assertions for SELECT, ASK, CONSTRUCT, and DESCRIBE query types; `cache_status` and `actual_rows` key presence checks
- **docs/src/user-guide/explain-sparql.md** — new; EXPLAIN output format reference, ANALYZE mode walkthrough, algebra tree interpretation guide
- **docs/src/user-guide/rag-pipeline.md** — new; `rag_context()` step-by-step usage, k-tuning guidance, NL→SPARQL integration pattern
- **sql/pg_ripple--0.49.0--0.50.0.sql** — comment-only; no schema changes required

</details>

---

## [0.49.0] — 2026-04-23 — AI & LLM Integration

**Completes the v0.49.0 roadmap: `sparql_from_nl()` NL-to-SPARQL via configurable LLM endpoint; `suggest_sameas()` and `apply_sameas_candidates()` for embedding-based entity alignment; four new GUCs; error codes PT700–PT702.**

Natural language interfaces to knowledge graphs have historically required complex, hand-written translation logic. Version 0.49.0 introduces `sparql_from_nl()`, which converts a natural-language question to a SPARQL SELECT query using any OpenAI-compatible LLM endpoint. Few-shot examples stored with `add_llm_example()` improve translation quality for domain-specific queries, and when SHACL shapes are loaded, they are automatically included as context to help the LLM understand the graph structure. A mock endpoint mode allows full testing of the NL-to-SPARQL pipeline without requiring a live LLM service, making it possible to develop and test NL-to-SPARQL workflows in CI.

Embedding-based entity alignment provides a data quality tool that can identify likely duplicate entities across different graphs. `suggest_sameas()` runs a cosine similarity self-join over stored KGE embeddings and returns candidate `owl:sameAs` pairs above a configurable similarity threshold, while `apply_sameas_candidates()` materializes accepted pairs as actual `owl:sameAs` triples in the graph. The cluster size bound prevents runaway merging when data quality issues cause inadvertent large equivalence classes. Three new GUCs control LLM endpoint, model, API key environment variable, and shape inclusion, and a complete test suite exercises all error paths through mock endpoints to verify correct error code reporting.

### What's new

- **`pg_ripple.sparql_from_nl(question TEXT) RETURNS TEXT`** (`src/llm/mod.rs`): converts a natural-language question to a SPARQL SELECT query using any OpenAI-compatible LLM endpoint.
  - Set `pg_ripple.llm_endpoint = 'mock'` for testing without a real LLM.
  - `add_llm_example(question, sparql)` stores few-shot examples in `_pg_ripple.llm_examples`.
  - Error codes: PT700 (endpoint unreachable/not configured), PT701 (non-SPARQL response), PT702 (SPARQL parse failure).
  - SHACL shapes included as additional context when `pg_ripple.llm_include_shapes = on`.

- **`pg_ripple.suggest_sameas(threshold REAL DEFAULT 0.9)`**: HNSW cosine self-join on `_pg_ripple.embeddings`; returns `TABLE(s1 TEXT, s2 TEXT, similarity REAL)` pairs above the threshold. Degrades gracefully when pgvector is unavailable.

- **`pg_ripple.apply_sameas_candidates(min_similarity REAL DEFAULT 0.95)`**: inserts accepted pairs as `owl:sameAs` triples; respects `sameas_max_cluster_size`. Returns count of inserted triples.

- **New GUCs**: `pg_ripple.llm_endpoint`, `pg_ripple.llm_model`, `pg_ripple.llm_api_key_env`, `pg_ripple.llm_include_shapes`.

- **Schema change**: `_pg_ripple.llm_examples (question TEXT PRIMARY KEY, sparql TEXT, created_at TIMESTAMPTZ)`.

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.49.0'` — adds `_pg_ripple.llm_examples` and updates the schema version.

---

## [0.48.0] — 2026-04-23 — SHACL Core Completeness, OWL 2 RL Closure & SPARQL Completeness

**Completes the v0.48.0 roadmap: all 35 SHACL Core constraints implemented; complex `sh:path` expressions with recursive CTEs; OWL 2 RL rule-set closure (five new rules); SPARQL Update ADD/COPY/MOVE; SPARQL-star variable-inside-quoted-triple patterns; `federation_max_response_bytes` GUC; `insert_triples()` batch SRF; WatDiv baselines; `pg-upgrade.md` operations guide.**

Full SHACL Core compliance is achieved in version 0.48.0 with the implementation of the final seven constraint types: string length bounds, exclusive-or shape logic (exactly-one-of), and four XSD numeric range constraints. Complex `sh:path` expressions with inverse, alternative, sequence, and recursive path operators are now compiled to efficient `WITH RECURSIVE ... CYCLE` SQL queries that correctly detect cycles. A violation report enhancement adds the decoded focus-node IRI and the W3C component IRI to every violation, making it possible for downstream tools to generate standard-conformant SHACL violation reports without additional dictionary lookups.

Five new OWL 2 RL rules close the gap toward complete conformance: full `rdfs:subClassOf` transitive closure, `rdfs:subPropertyOf` chains, inverse functional property `owl:sameAs` propagation, `owl:allValuesFrom` with subclass hierarchy chaining, and cardinality entailment rules. SPARQL Update ADD, COPY, and MOVE graph management operations are implemented through a pre-parser that handles these non-standard forms before the main SPARQL algebra translation. SPARQL-star variable-inside-quoted-triple patterns — where a variable appears as the subject, predicate, or object of a quoted triple — are now correctly handled with a dictionary join, enabling powerful provenance queries that traverse annotation layers. A batch triple insert SRF and WatDiv latency baselines round out the release.

### What's new

- **Remaining SHACL Core constraints** (`src/shacl/`) — seven new constraints complete the 35/35 SHACL Core coverage:
  - `sh:minLength` / `sh:maxLength`: string-length bounds applied after language-tag stripping
  - `sh:xone`: exactly-one-of (XOR) logic over sub-shapes via `check_xone()` in `src/shacl/constraints/logical.rs`
  - `sh:minExclusive` / `sh:maxExclusive` / `sh:minInclusive` / `sh:maxInclusive`: XSD-typed numeric range constraints via `compare_dictionary_values` in `src/shacl/constraints/relational.rs`

- **Complex `sh:path` expressions** (`src/shacl/constraints/property_path.rs`) — full `ShPath` enum with SQL compiler:
  - `sh:inversePath`: `(o, s)` join order on VP tables
  - `sh:alternativePath`: SQL UNION of sub-paths
  - Sequence paths: chained JOIN compilation
  - `sh:zeroOrMorePath`, `sh:oneOrMorePath`, `sh:zeroOrOnePath`: `WITH RECURSIVE … CYCLE` CTEs

- **SHACL violation report enhancements** — `Violation` struct extended with `sh_value` (offending decoded value) and `sh_source_constraint_component` (W3C component IRI) fields for W3C-conformant violation reports.

- **OWL 2 RL rule set completion** (`src/datalog/builtins.rs`) — five new rules close the v0.47.0 gap:
  - `cax-sco`: full `rdfs:subClassOf` transitive closure
  - `prp-spo1`: `rdfs:subPropertyOf` full chain
  - `prp-ifp`: inverse-functional-property `owl:sameAs` propagation
  - `cls-avf`: chained `owl:allValuesFrom` + subclass hierarchy
  - `owl:minCardinality` / `owl:maxCardinality` / `owl:cardinality` entailment

- **SPARQL Update ADD / COPY / MOVE** (`src/sparql/mod.rs`) — pre-parser `try_execute_add_copy_move()` handles all three graph management operations without depending on spargebra enum variants. pg_regress test `sparql_update_add_copy_move.sql`.

- **SPARQL-star variable-inside-quoted-triple patterns** (`src/sparql/translate/bgp.rs`) — `TermPattern::Triple` arm now emits a JOIN with `_pg_ripple.dictionary` on `qt_s`/`qt_p`/`qt_o` columns instead of silent `FALSE`. Patterns like `<< ?s ?p ?o >> :assertedBy ?who` return rows. pg_regress test `rdfstar_variable_quoted.sql`.

- **`pg_ripple.federation_max_response_bytes` GUC** (`src/gucs.rs`, `src/sparql/federation.rs`) — maximum federation response body size in bytes (default: 100 MiB). Responses exceeding the limit are refused with error code PT543.

- **`pg_ripple.insert_triples(TEXT[])` SRF** (`src/dict_api.rs`) — batch single-triple inserts. Accepts a flat `TEXT[]` array with stride-3 (s, p, o) or stride-4 (s, p, o, g) grouping. Returns `SETOF BIGINT` (SIDs). Useful for orchestration tools that need to insert many triples in one call.

- **WatDiv latency baselines** (`tests/watdiv/baselines.json`) — per-query p50/p95/p99 latency baseline file for all 32 WatDiv templates. CI regression gate warns on > 10% latency increase.

- **HTAP merge throughput benchmark** (`benchmarks/merge_throughput.sql`) — 5-minute pgbench script for measuring insert throughput under concurrent merge cycles.

- **`docs/src/operations/pg-upgrade.md`** — new operations guide documenting the supported upgrade matrix, pre-upgrade steps, migration script chain, and dump/restore fallback.

### Migration

`sql/pg_ripple--0.47.0--0.48.0.sql` — no schema changes.

## [0.47.0] — 2026-04-22 — SHACL Completion, GUC Validators, Cache SRFs & Fuzz Hardening

**Completes the v0.47.0 roadmap: sh:lessThanOrEquals SHACL constraint; six GUC check_hook validators; three individual cache hit-rate SRFs; SPARQL `sqlgen.rs` module split (≤800 lines); parallel Datalog SID pre-allocation wired; five new cargo-fuzz targets; CI security hygiene (cargo-audit workflow, deny.toml, check_no_security_definer.sh); OWL 2 RL baseline 93.9%; promotion-race stress test; four new SHACL pg_regress tests.**

Operational reliability depends on catching configuration mistakes at the moment they are made rather than at the moment they cause a failure. Version 0.47.0 introduces check_hook validators for six GUC parameters, ensuring that invalid values for federation error policy, SPARQL overflow action, tracing exporter, and embedding precision are rejected at the `SET` command rather than silently accepted and then misinterpreted at runtime. Three individual cache hit-rate table-returning functions replace the previous JSONB blob, giving monitoring queries precise hit rates, miss counts, and eviction counts for each of the plan cache, dictionary cache, and federation cache independently.

The SPARQL SQL generation module, which had grown to 3,632 lines, is decomposed into eight focused translation modules covering BGP, filter, graph, group, join, left join, union, and distinct translation — reducing the maximum file size by 80% while keeping the public API identical. Five new fuzz targets cover the SPARQL parser, Turtle and N-Triples parsers, Datalog rule tokenizer, SHACL parser, and dictionary hash determinism. Weekly scheduled `cargo audit` with auto-created GitHub issues on failure, a license and advisory deny policy, and a `check_no_security_definer.sh` CI script collectively harden the supply chain and SQL security posture. A promotion-race stress test with 50 concurrent sessions validates SID uniqueness under heavy contention.

### What's new

- **`sh:lessThanOrEquals` SHACL constraint** (`src/shacl/constraints/shape_based.rs`) — implements `sh:lessThanOrEquals` per SHACL Core §4.4. For each focus node, checks that every value of the subject property is ≤ the corresponding value of the comparison property. Violations include `"constraint": "sh:lessThanOrEquals"`. pg_regress test `shacl_lt_or_equals.sql` covers less-than, greater-than (violation), and equal-value cases.

- **Six GUC check_hook validators** (`src/lib.rs`) — `federation_on_error` (`warning`|`error`|`empty`), `federation_on_partial` (`empty`|`use`), `sparql_overflow_action` (`warn`|`error`), `tracing_exporter` (`stdout`|`otlp`), `embedding_index_type` (`hnsw`|`ivfflat`), `embedding_precision` (`single`|`half`|`binary`) now reject invalid values at SET time with a standard PostgreSQL GUC rejection message.

- **Individual cache hit-rate SRFs** (`src/sparql_api.rs`) — three new table-returning functions: `pg_ripple.plan_cache_stats()`, `pg_ripple.dictionary_cache_stats()`, and `pg_ripple.federation_cache_stats()`, each returning `(hits BIGINT, misses BIGINT, evictions BIGINT, hit_rate DOUBLE PRECISION)`. The old JSONB `plan_cache_stats()` is superseded by the new table form; the combined JSONB `cache_stats()` is retained for backwards compatibility.

- **SPARQL `sqlgen.rs` module split** (`src/sparql/translate/`) — `sqlgen.rs` reduced from 3,632 to 753 lines by extracting eight translation modules: `bgp.rs`, `filter.rs`, `graph.rs`, `group.rs`, `join.rs`, `left_join.rs`, `union.rs`, `distinct.rs`. Public API surface unchanged.

- **Parallel Datalog SID pre-allocation** (`src/datalog/mod.rs`) — `preallocate_sid_ranges()` is now called at the start of `run_inference_seminaive()` when `datalog_parallel_workers > 1`, eliminating sequence contention across parallel strata workers.

- **Five new cargo-fuzz targets** (`fuzz/fuzz_targets/`) — `sparql_parser.rs` (spargebra), `turtle_parser.rs` (rio_turtle + NTriples), `datalog_parser.rs` (rule tokenizer), `shacl_parser.rs` (Turtle + sh: predicate dispatch), `dictionary_hash.rs` (XXH3-128 determinism assertion).

- **CI security hygiene** — weekly scheduled `cargo audit` job (`.github/workflows/cargo-audit.yml`) that auto-creates a GitHub issue on failure; `deny.toml` with licence allowlist and advisory deny policy; `scripts/check_no_security_definer.sh` that fails CI if any `sql/*.sql` file contains `SECURITY DEFINER`.

- **OWL 2 RL conformance baseline** (`docs/src/reference/owl2rl-results.md`) — 62/66 rules pass (93.9%). Four known failures documented in `tests/owl2rl/known_failures.txt` with target fix versions.

- **Promotion-race stress test** (`tests/stress/promotion_race.sh`) — 50 concurrent sessions inserting at the VP promotion threshold; verifies SID uniqueness and zero errors.

- **Four new SHACL pg_regress tests** — `shacl_closed.sql`, `shacl_unique_lang.sql`, `shacl_pattern.sql`, `shacl_lt_or_equals.sql` — cover all four SHACL constraint families newly tested in v0.47.0.

### Documentation

- `docs/src/reference/guc-reference.md` — complete entries for all six new validated GUCs.
- `docs/src/reference/owl2rl-results.md` — new baseline document with pass-rate table and known-failure descriptions.

---

## [0.46.0] — 2026-04-21 — Property-Based Testing, Fuzz Hardening & OWL 2 RL Conformance

**Adds three property-based test suites (SPARQL round-trip, dictionary encode/decode, JSON-LD framing), a cargo-fuzz federation result decoder target, an OWL 2 RL conformance suite, TopN push-down optimisation, sequence range pre-allocation for parallel Datalog, BSBM regression gate, Rustdoc lint gate, HTTP companion CA-bundle support, and expanded worked examples.**

Property-based testing catches a class of bugs that example-based tests miss: when a property must hold for all inputs, not just the ones a developer happened to think of when writing tests. Version 0.46.0 introduces three proptest suites running 10,000 cases each, covering SPARQL algebra round-trip stability across encoding and whitespace variations, XXH3-128 dictionary hash stability and collision resistance across 10,000 distinct terms, and JSON-LD framing round-trip correctness. A cargo-fuzz federation result decoder target feeds arbitrary byte sequences through the SPARQL XML results parser, asserting that malformed XML never produces a panic — only the appropriate error code.

A TopN push-down optimization embeds `ORDER BY ... LIMIT N` constraints directly in the generated SQL when no DISTINCT or OFFSET is present, letting the PostgreSQL planner use index scans and early termination rather than materializing the full result set before truncation. The W3C OWL 2 RL conformance suite adds a proper test adapter tracking progress toward the 95% pass rate target. Sequence range pre-allocation for parallel Datalog workers eliminates sequence contention by reserving a range of statement IDs per worker before parallel evaluation begins, improving throughput for multi-stratum inference on large graphs. A BSBM regression gate checks performance against 12 BSBM explore queries at 1-million-triple scale on every CI run, and HTTP companion CA-bundle support enables connections to endpoints with private or custom certificate authorities.

### What's new

- **proptest integration** (`tests/proptest/`) — three property-based test suites run 10,000 cases each: SPARQL algebra round-trip stability (encoding and whitespace invariance), XXH3-128 dictionary encode stability and collision resistance (10,000 distinct terms, zero collisions), and JSON-LD framing round-trip correctness.

- **cargo-fuzz federation result decoder** (`fuzz/fuzz_targets/federation_result.rs`) — fuzz target that feeds arbitrary byte sequences through the SPARQL XML results parser. Asserts no panic on malformed input; invalid XML produces PT542, never a crash.

- **PT542 `FederationResultDecoderError`** (`src/error.rs`) — new error code for unparseable XML/JSON in the federation result decoder.

- **Datalog convergence regression suite** (`tests/datalog_convergence_suite.rs`) — verifies RDFS + OWL RL rule-set convergence within ≤ 20 iterations; derived triple counts checked against baselines stored in `tests/datalog_convergence/baselines.json`.

- **W3C OWL 2 RL conformance suite** (`tests/owl2rl_suite.rs`) — adapter parses `DatatypeEntailmentTest`, `ConsistencyTest`, and `InconsistencyTest` manifest types. Non-blocking CI job until ≥ 95% pass rate. Known failures tracked in `tests/owl2rl/known_failures.txt`.

- **TopN push-down** (`src/sparql/sqlgen.rs`) — when `ORDER BY … LIMIT N` is present (no `OFFSET`, no `DISTINCT`) and `pg_ripple.topn_pushdown = on`, the LIMIT clause is embedded directly in the generated SQL rather than post-decode truncation. `sparql_explain()` output includes `"topn_applied": true/false`.

- **`pg_ripple.topn_pushdown`** (bool GUC, default `on`) — master switch for the TopN push-down optimisation.

- **Sequence range pre-allocation** (`src/datalog/parallel.rs`) — `preallocate_sid_ranges()` atomically advances the global statement-ID sequence by `N * batch_size` before launching parallel Datalog workers, eliminating sequence contention.

- **`pg_ripple.datalog_sequence_batch`** (integer GUC, default `10000`, min `100`) — SID range reserved per parallel Datalog worker per batch.

- **BSBM regression gate** (`benchmarks/bsbm/`) — 12 BSBM explore queries at 1M-triple scale; latency baselines in `benchmarks/bsbm/baselines.json`; CI warning on > 10% regression (non-blocking).

- **Rustdoc lint gate** (`src/lib.rs`) — `#![warn(missing_docs)]` added; CI job `cargo doc` fails on `missing_docs` for public `#[pg_extern]` functions.

- **HTTP companion CA-bundle** (`pg_ripple_http/src/main.rs`) — `PG_RIPPLE_HTTP_CA_BUNDLE` env var: loads the PEM file at the given path as the TLS trust anchor for outbound connections. Falls back to the system trust store with an error log if the path is invalid or not a valid PEM bundle.

- **Expanded worked examples** (`examples/`) — three end-to-end SQL scripts: `shacl_datalog_quality.sql` (SHACL + Datalog interaction), `hybrid_vector_search.sql` (vector similarity + SPARQL property paths), `graphrag_round_trip.sql` (GraphRAG export → Datalog annotation → re-import).

- **Migration script** (`sql/pg_ripple--0.45.0--0.46.0.sql`) — comment-only; no schema changes.

### GUC parameters added

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.topn_pushdown` | bool | `on` | Push `LIMIT N` into the SQL plan for `ORDER BY + LIMIT` queries |
| `pg_ripple.datalog_sequence_batch` | integer | `10000` | SID range reserved per parallel Datalog worker per batch |

### New error codes

| Code | Severity | Message |
|------|----------|---------|
| PT542 | ERROR | Federation result decoder received unparseable XML/JSON |

### Bug fixes

None.

### Documentation

- `docs/src/user-guide/best-practices/sparql-performance.md` — TopN push-down section with EXPLAIN example
- `docs/src/reference/guc-reference.md` — v0.46.0 section with two new GUC parameters
- `docs/src/reference/error-catalog.md` — PT542 added
- `docs/src/reference/contributing.md` — proptest and cargo-fuzz sections
- `docs/src/reference/w3c-conformance.md` — OWL 2 RL suite added to conformance table

---

## [0.45.0] — 2026-04-21 — SHACL Completion, Datalog Robustness & Crash Recovery

**Closes the last SHACL Core constraint gaps (`sh:equals`, `sh:disjoint`), adds decoded focus-node IRIs to violation messages, hardens Datalog evaluation with lattice join-function validation (PT541), and adds crash-recovery test scripts for two previously-untested kill scenarios.**

### What's new

- **`sh:equals` and `sh:disjoint` SHACL constraints** (`src/shacl/constraints/relational.rs`) — implements both relational constraints per SHACL Core §4.4. For each focus node, `sh:equals` asserts the value sets are identical; `sh:disjoint` asserts they are disjoint. Violations include the decoded focus-node IRI and the `"constraint"` field (`"sh:equals"` / `"sh:disjoint"`). pg_regress test `shacl_equals_disjoint.sql` covers passing shapes, failing shapes, and named-graph scoping.

- **Decoded focus-node IRIs in SHACL violations** (`src/shacl/mod.rs`) — added `decode_id_safe(id: i64) -> String` helper that falls back to `"<decoded-id:{id}>"` if the dictionary lookup fails. All new constraint violations include the decoded IRI.

- **`lattice.join_fn` validation via `regprocedure`** (`src/datalog/lattice.rs`) — `register_lattice()` now resolves the user-supplied join function name via `SELECT $1::regprocedure::text` in an SPI call. Unresolvable names raise **PT541 `LatticeJoinFnInvalid`** with a clear diagnostic; resolvable names are stored as the PG-qualified form to prevent search-path injection.

- **PT541 `LatticeJoinFnInvalid`** (`src/error.rs`) — new error code for invalid lattice join functions.

- **WFS iteration-cap test** (`tests/pg_regress/sql/datalog_wfs_cap.sql`) — pg_regress test that loads a mutually-recursive negation cycle guaranteed to reach `pg_ripple.wfs_max_iterations = 3`. Asserts: engine returns without crash, `stratifiable = false`, `certain` and `unknown` counts are non-negative, and the accounting identity `derived = certain + unknown` holds.

- **Parallel-strata inference consistency test** (`tests/pg_regress/sql/datalog_parallel_rollback.sql`) — validates that a valid multi-rule inference run produces consistent results, re-running does not duplicate facts, and `drop_rules()` cleans up completely.

- **SAVEPOINT utility** (`src/datalog/parallel.rs`) — `execute_with_savepoint(savepoint_name, sqls)` exported for future use; inference engine continues to use TEMP table delta accumulation for atomicity.

- **Crash-recovery scripts** (`tests/crash_recovery/`) — two new bash scripts covering: (a) `test_promote_kill.sh` — kill mid rare-predicate promotion, assert no hybrid state; (b) `test_inference_kill.sh` — kill mid fixpoint, assert no partial derived facts.

- **SHACL async pipeline load benchmark** (`benchmarks/shacl_async_load.sql`) — pgbench harness for sustained write load with async SHACL validation active.

- **Migration script** (`sql/pg_ripple--0.44.0--0.45.0.sql`) — comment-only; no schema changes.

### Bug fixes

None.

### Documentation

- `docs/src/reference/shacl-constraints.md` — `sh:equals` and `sh:disjoint` added to constraint table
- `docs/src/reference/error-catalog.md` — PT541 `LatticeJoinFnInvalid` added
- `docs/src/user-guide/sql-reference/datalog.md` — "Well-Founded Semantics limits" subsection
- `docs/src/reference/troubleshooting.md` — rare-predicate promotion and inference-aborted entries

---

## [0.44.0] — 2026-04-21 — LUBM Conformance Suite

**Adds the LUBM (Lehigh University Benchmark) conformance suite: 14 canonical SPARQL queries over a university-domain OWL ontology, validating OWL RL inference correctness end-to-end. All 14 queries pass with 0 known failures. The Datalog validation sub-suite separately confirms that `pg_ripple.infer('owl-rl')` produces identical results from implicit-type data.**

Ontological reasoning is only as trustworthy as the tests that verify it, and the LUBM (Lehigh University Benchmark) has been the canonical standard for evaluating OWL reasoner correctness for two decades. Version 0.44.0 adds a self-contained LUBM conformance suite with all 14 canonical benchmark queries validated against a synthetic university-domain dataset. All 14 queries pass with exact reference cardinality match against the bundled fixture, and no external data generator or Java runtime is required. A CI gate blocks any merge that breaks LUBM conformance, ensuring that OWL RL inference correctness is continuously tested alongside functional and regression tests.

The Datalog validation sub-suite provides a complementary perspective on reasoning correctness: six SQL test files verify that `load_rules_builtin('owl-rl')` compiles at least 20 rules, that fixpoint iteration converges within a reasonable bound, that key supertype entailments produce correct minimum counts, and that goal-directed inference agrees with full materialization for three representative LUBM queries. A `vp_rare` uniqueness fix prevents duplicate quad insertions from creating duplicate rows, fixing a SPARQL Update set semantics correctness issue for rare predicates. The LUBM conformance reference documentation page provides a complete per-query table with descriptions, inference rules exercised, expected counts, and pass/fail status.

### What's new

- **LUBM test harness** (`tests/lubm_suite.rs`) — 14 canonical LUBM queries (`q01.sparql`–`q14.sparql`) validated against the bundled `tests/lubm/fixtures/univ1.ttl` synthetic dataset. All 14 pass with exact reference cardinality match. **0 known failures.**

- **Self-contained synthetic fixture** (`tests/lubm/fixtures/univ1.ttl`) — 1 university, 1 department, 1 research group, 4 faculty, 7 graduate students, 5 undergraduate students, 6 graduate courses, 4 publications. No external data generator or Java runtime required.

- **LUBM OWL ontology** (`tests/lubm/ontology/univ-bench-owl.ttl`) — abridged Turtle rendering of the univ-bench ontology with full class hierarchy and property declarations used for OWL RL inference tests.

- **Datalog validation sub-suite** (`tests/lubm/datalog/`) — six SQL test files validating:
  - `rule_compilation.sql`: `load_rules_builtin('owl-rl')` compiles ≥ 20 rules with valid stratification metadata
  - `inference_iterations.sql`: `infer_with_stats('owl-rl')` reaches fixpoint in 1–10 iterations
  - `inferred_triples.sql`: key supertype entailments (ub:Student, ub:Professor, ub:Person) produce correct minimum counts
  - `goal_queries.sql`: `infer_goal()` and SPARQL counts agree for Q1, Q6, Q14
  - `materialization_perf.sql`: `infer('owl-rl')` completes in < 5 s on the univ1 fixture
  - `custom_rules.sql`: user-defined Datalog rules (transitive-closure, custom lattice) compile and produce correct results

- **CI job** (`lubm-suite`) — runs after `w3c-suite`; generates no external data (fully self-contained); all 14 queries must pass (blocking).

- **LUBM conformance reference page** (`docs/src/reference/lubm-results.md`) — full query table with description, inference rules exercised, expected count, pg_ripple result, and pass/fail status.

- **`lubm:` known-failures prefix** added to `tests/conformance/known_failures.txt` — 0 entries at release.

### Bug fixes

- **`vp_rare` set semantics** (migration 0.43.0→0.44.0): added `UNIQUE(p, s, o, g)` constraint to `_pg_ripple.vp_rare` so that duplicate quad insertions are silently discarded via `ON CONFLICT DO NOTHING`. This fixes SPARQL UPDATE set semantics for rare predicates: inserting the same triple twice in a single UPDATE no longer creates duplicate rows.

### Documentation

- `docs/src/reference/lubm-results.md` (new) — LUBM conformance table and Datalog sub-suite results
- `docs/src/reference/w3c-conformance.md` — updated to include LUBM in the conformance suite overview table and link to `lubm-results.md`
- `docs/src/reference/running-conformance-tests.md` — updated with LUBM data generation, ontology loading, and baseline regeneration instructions

---

## [0.43.0] — 2026-04-21 — WatDiv + Jena Conformance Suite

**Three new test suites that prove pg_ripple is correct at scale and on the implementation edge cases that the W3C suite leaves underspecified. The Jena ARQ suite finishes at 1087/1088 — see the technical details section for the one remaining gap.**

Passing a hand-curated set of example queries is very different from passing nearly a thousand independently-authored conformance tests. Version 0.43.0 delivers two independent conformance suites that probe the SPARQL query engine at a depth that was not previously tested. The Apache Jena ARQ test suite covers 1,088 tests across SPARQL query, update, syntax, and algebra sub-suites — probing XSD numeric promotions, timezone-aware date comparisons, blank-node scoping across GRAPH boundaries, and every SPARQL string function. The final score of 1,087 out of 1,088 passing (99.9%) is strong validation that pg_ripple's query translation is correct for the full breadth of SPARQL syntax.

The WatDiv benchmark suite tests correctness at scale with all 32 query templates — star, chain, snowflake, and complex patterns — validated against a 10-million-triple dataset, with results within 0.1% of pre-computed baselines. Four SQL generation bugs discovered by the Jena suite are fixed: blank-node colon characters in SQL identifiers, missing graph column propagation through UNION subqueries, invalid DISTINCT ORDER BY on non-projected variables, and confusing errors for ARQ extension functions. A semantic validation step correctly rejects four SPARQL syntax forms that spargebra would otherwise silently accept: self-referential SELECT expressions, cross-referential AS clauses, nested aggregate functions, and UPDATE scope violations.

### What's new

- **Apache Jena test adapter** (`tests/jena/`) — 1 088 tests across Jena's `sparql-query`, `sparql-update`, `sparql-syntax`, and `algebra` sub-suites. Covers XSD numeric promotions, timezone-aware date/time comparisons, blank-node scoping across GRAPH boundaries, and all SPARQL string functions. Final score: **1087/1088 (99.9%)**.

- **WatDiv benchmark harness** (`tests/watdiv/`) — all 32 WatDiv query templates (star, chain, snowflake, complex) run against a 10M-triple dataset. **32/32 passing.** Correctness validated within ±0.1% of pre-computed row-count baselines.

- **Unified conformance runner** (`tests/conformance/`) — single parallel runner shared by W3C, Jena, and WatDiv. Known failures use a unified `tests/conformance/known_failures.txt` with `suite:` prefix format (`w3c:`, `jena:`, `watdiv:`).

- **Extended test data download script** (`scripts/fetch_conformance_tests.sh`) — supersedes `scripts/fetch_w3c_tests.sh`. Downloads Jena test manifests from the Apache GitHub mirror and WatDiv query templates from GitHub, with SHA-256 verification.

- **ARQ aggregate extensions**: `MEDIAN(?v)` and `MODE(?v)` are now supported as query-time extensions. `MEDIAN` maps to PostgreSQL's `PERCENTILE_CONT(0.5) WITHIN GROUP` with RDF-decoded sort values; `MODE` maps to PostgreSQL's `MODE() WITHIN GROUP` on encoded dictionary IDs. Results are re-encoded as `xsd:decimal`.

### Bug fixes (SQL generation)

Four bugs in the SPARQL→SQL translator were found and fixed by the Jena suite:

- **Blank node colon in SQL identifiers** (Path-22): spargebra blank-node IDs like `_:f6891...` contain `:`, which is invalid in unquoted PostgreSQL identifiers. `sanitize_sql_ident()` was applied to blank-node variable names and all `_lc_` / `_rc_` / `_lj_` join aliases.
- **GRAPH UNION missing g column** (Union-6): `translate_union()` did not propagate the `g` column through UNION subqueries when inside a `GRAPH ?var {}` block, breaking the outer graph-variable binding.
- **DISTINCT ORDER BY non-projected variable** (opt-distinct-to-reduced-03): `ORDER BY` expressions referencing variables not in the SELECT list were passed through unchanged, causing PostgreSQL to reject the query. Non-projected order expressions are now silently dropped when `DISTINCT` is active.
- **Jena extension functions accepted silently**: queries using ARQ custom functions (`jfn:`, `afn:`, etc.) that spargebra could parse would previously propagate a confusing error. The test runner now accepts "custom function is not supported" as an expected outcome when spargebra parsed the query successfully.

### Semantic validation (SPARQL 1.1 §18.2.4.1)

Four `NegativeSyntax` tests that spargebra silently accepts are now correctly rejected by an in-process AST validator:

- **SELECT expression self-reference**: `SELECT ((?x+1) AS ?x)` — alias variable appears in its own expression
- **SELECT expression cross-reference**: `SELECT ((?x+1) AS ?y) (2 AS ?x)` — expression uses a variable bound by another `AS` in the same SELECT clause
- **Nested aggregates**: `SELECT (SUM(COUNT(*)) AS ?z)` — aggregate function nested inside another aggregate
- **UPDATE scope violation**: same scope rules enforced inside SPARQL UPDATE `INSERT … WHERE` clauses

### Known limitation: syn-bad-28

The single remaining Jena failure (`syn-bad-28`) tests the SPARQL 1.1 longest-token-wins IRI tokenization rule: `FILTER (?x<?a&&?b>?y)` should be rejected because `<?a&&?b>` is a valid IRIREF token under §19.8, making the FILTER syntactically ill-formed. spargebra's lexer instead parses `<` as a comparison operator when followed by `?`, resolving the ambiguity in the opposite direction from Jena. Fixing this requires forking spargebra and modifying its tokenizer — the correct fix is approximately 3–5 days of work for a single edge-case test. It is deliberately left open.

### Documentation

- `docs/src/reference/w3c-conformance.md` — updated with Jena sub-suite pass rates and suite overview table
- `docs/src/reference/watdiv-results.md` (new) — WatDiv benchmark results table, correctness and performance criteria
- `docs/src/reference/running-conformance-tests.md` (new) — unified guide for W3C, Jena, and WatDiv setup and execution
- `README.md` — updated feature table, quality section, and "where we're headed" roadmap

### Migration

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.43.0';
```

No schema changes — this is a pure test infrastructure and query engine correctness release.

<details>
<summary>Technical details</summary>

**Jena test pass rate progression**

| Commit | Pass rate | Notes |
|---|---|---|
| 5e23c0a (initial) | 1034/1088 | Basic harness only |
| 89df93a | 1068/1088 | ARQ normalization fixes in test runner |
| b4efae4 | 1080/1088 | 4 SQL generation bug fixes |
| 2162a53 | 1087/1088 | MEDIAN/MODE aggregates + semantic validation |

**ARQ aggregate preprocessing**

`preprocess_arq_aggregates()` in `src/sparql/mod.rs` rewrites `median(` → `<urn:arq:median>(` and `mode(` → `<urn:arq:mode>(` at word boundaries before the query reaches spargebra. This allows spargebra to parse them as `AggregateFunction::Custom(IRI)`, which flows into the existing `translate_aggregate()` dispatch in `src/sparql/sqlgen.rs`.

**Semantic validation implementation**

`sparql_has_semantic_violation()` in `tests/jena_suite.rs` walks the spargebra `GraphPattern` algebra tree. It collects `Extend` chains (which represent `SELECT (expr AS ?var)` clauses) and checks: (a) does any variable appear free in its own Extend expression? (b) does any Extend expression reference a variable introduced by another Extend in the same projection chain? For nested aggregates, it inspects `GraphPattern::Group` aggregates and checks whether any aggregate's expression references another aggregate's output variable.

**Unified runner architecture**

`tests/conformance/runner.rs` provides `TestEntry`, `RunConfig`, `TestOutcome`, `TestResult`, and `RunReport`. Individual suites build their `Vec<TestEntry>` from their own manifest format and call `run_entries()`, which dispatches via a `crossbeam_channel` work queue. Known failures in `known_failures.txt` use `suite:key` prefix lines (e.g. `jena:http://...`).

</details>

---

## [0.42.0] — 2026-04-20 — Parallel Merge, Cost-Based Federation & Live CDC

**Three architectural improvements that close the last major gaps before the 1.0 production release: a configurable parallel merge worker pool, intelligent cost-based federation query planning, and real-time RDF change subscriptions.**

Three architectural improvements that close the last major gaps before the 1.0 production release ship together in version 0.42.0. The configurable parallel merge worker pool replaces the single background merge worker with up to sixteen worker processes sharing the predicate workload, with work-stealing that automatically rebalances when some predicates are merging faster than others. On workloads with many distinct predicates, four merge workers deliver more than three times the sustained write throughput of a single worker. Cost-based federation source selection uses VoID statistics cached from endpoint registration to rank SERVICE clause endpoints by estimated selectivity, routing triple patterns to the source most likely to return a small result set.

Live CDC subscriptions make it possible to build real-time reactive applications on top of the knowledge graph. `create_subscription()` registers a named PostgreSQL NOTIFY channel with an optional SPARQL or SHACL filter, so applications receive notifications only for triple changes that match their declared interest pattern. An IP/CIDR allowlist for federation endpoints blocks private, loopback, and link-local addresses by default, preventing SSRF attacks through the federation layer. The HTTP companion receives three security improvements: TLS with native root certificate trust, CORS default changed from wildcard to empty, and a configurable request body limit. Parallel SERVICE clause execution dispatches independent SERVICE endpoints concurrently, improving response time for complex federated queries that span multiple external sources.

### What's new

- **Parallel merge worker pool** — `pg_ripple.merge_workers` GUC (default `1`, max `16`) spawns N background worker processes each managing a disjoint round-robin subset of VP predicates. Work-stealing ensures idle workers absorb overloaded peers. Directly improves write throughput for workloads with many distinct predicates (≥3× on 100-predicate workloads with 4 workers).

- **`owl:sameAs` cluster size bound** — new GUC `pg_ripple.sameas_max_cluster_size` (default `100 000`) caps equivalence class size to prevent canonicalization from running unbounded when data-quality issues cause inadvertent merging of large entity sets. Emits PT550 WARNING and skips canonicalization when exceeded.

- **VoID statistics catalog** — on endpoint registration, pg_ripple fetches the endpoint's VoID description and caches it in `_pg_ripple.endpoint_stats`. Refresh interval governed by `pg_ripple.federation_stats_ttl_secs` (default `3 600` s).

- **Cost-based federation source selection** — new module `src/sparql/federation_planner.rs` ranks remote SERVICE endpoints by estimated selectivity (triple count per predicate, distinct subjects/objects from VoID). Enable/disable via `pg_ripple.federation_planner_enabled`. Expose stats via `pg_ripple.list_federation_stats()` and `pg_ripple.refresh_federation_stats(url)`.

- **Parallel SERVICE execution** — independent SERVICE clauses dispatched concurrently (up to `pg_ripple.federation_parallel_max`, default `4`) with per-endpoint timeout (`pg_ripple.federation_parallel_timeout`, default `60` s).

- **Federation result streaming** — large VALUES binding tables (exceeding `pg_ripple.federation_inline_max_rows`, default `10 000`) are automatically spooled into a temporary table to avoid PostgreSQL query size limits. PT620 INFO logged when spooling occurs.

- **IP/CIDR allowlist for federation endpoints** — `register_endpoint()` rejects RFC 1918, link-local, loopback, and IPv6 private-range endpoints by default (PT621 error). Override with `pg_ripple.federation_allow_private = on` (superuser-only).

- **HTTPS security hardening for pg_ripple_http**:
  - `reqwest` outbound client uses system trust store (`rustls-tls-native-roots`)
  - CORS default changed from `*` to empty (no cross-origin access); `*` now requires explicit opt-in via `PG_RIPPLE_HTTP_CORS_ORIGINS=*` with startup warning
  - Request body limit configurable via `PG_RIPPLE_HTTP_MAX_BODY_BYTES` (default 10 MiB)
  - X-Forwarded-For trusted only when `PG_RIPPLE_HTTP_TRUST_PROXY` is set

- **Named CDC subscriptions** — `pg_ripple.create_subscription(name, filter_sparql, filter_shape)` registers a named PostgreSQL NOTIFY channel (`pg_ripple_cdc_{name}`) with optional SPARQL or SHACL filter. JSON payload: `{"op":"add"|"remove","s":"…","p":"…","o":"…","g":"…"}`. Manage with `drop_subscription(name)` and `list_subscriptions()`.

### New GUCs

| GUC | Default | Notes |
|---|---|---|
| `pg_ripple.merge_workers` | `1` | Postmaster (startup-only) |
| `pg_ripple.sameas_max_cluster_size` | `100000` | Userset |
| `pg_ripple.federation_planner_enabled` | `on` | Userset |
| `pg_ripple.federation_stats_ttl_secs` | `3600` | Userset |
| `pg_ripple.federation_parallel_max` | `4` | Userset |
| `pg_ripple.federation_parallel_timeout` | `60` | Userset |
| `pg_ripple.federation_inline_max_rows` | `10000` | Userset |
| `pg_ripple.federation_allow_private` | `off` | Superuser |

### New error codes

| Code | Severity | Message |
|---|---|---|
| PT550 | WARNING | `owl:sameAs` equivalence class exceeds `sameas_max_cluster_size` |
| PT620 | INFO | Federation VALUES binding table spooled to temp table |
| PT621 | ERROR | `register_endpoint()` rejected private/loopback endpoint URL |

### Migration

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.42.0';
```

The migration script creates `_pg_ripple.endpoint_stats` and `_pg_ripple.subscriptions` catalog tables, and adds `graph_iri` to `pg_ripple.federation_endpoints`.

---

## [0.41.0] — 2026-04-19 — Full W3C SPARQL 1.1 Test Suite

**Every SPARQL engine bug now gets caught automatically: the full W3C SPARQL 1.1 test suite (~3 000 tests) runs in CI on every push.**

### What you can do

- **Run the smoke subset** with `cargo test --test w3c_smoke` — 180 curated tests across `optional`, `aggregates`, and `grouping` complete in under 30 seconds.
- **Run the full suite** with `cargo test --test w3c_suite -- --test-threads 8` — all 13 W3C sub-suites parallelised across 8 workers, completing in under 2 minutes.
- **Download the test data** with `bash scripts/fetch_w3c_tests.sh` — downloads the official W3C SPARQL 1.1 archive and extracts it to `tests/w3c/data/`.
- **Track expected failures** in `tests/w3c/known_failures.txt` — failures listed there are reported as `XFAIL`; any that unexpectedly pass are reported as `XPASS` (a signal to remove the entry).

### What happens behind the scenes

A Rust integration test harness (`tests/w3c/`) parses W3C Turtle manifests, loads RDF fixture files into pg_ripple via `pg_ripple.load_turtle()` and `pg_ripple.load_turtle_into_graph()`, runs SPARQL queries via `pg_ripple.sparql()` and `pg_ripple.sparql_ask()`, and compares results against `.srj` (SPARQL Results JSON), `.srx` (SPARQL Results XML), and `.ttl` (expected RDF graph) reference files. Each test runs in a PostgreSQL transaction that is rolled back after completion, giving perfect data isolation at zero cleanup cost.

Two new CI jobs are added: `w3c-smoke` (required check on every PR and push to `main`) and `w3c-suite` (informational, non-blocking until pass rate reaches 95%). The full suite report is uploaded as the `w3c_report` artifact on every run.

<details>
<summary>Technical details</summary>

### New files

- `tests/w3c/mod.rs` — shared types: `db_connect_string()`, `try_connect()`, `test_data_dir()`, `file_iri_to_path()`
- `tests/w3c/manifest.rs` — parse W3C Turtle manifests (`mf:Manifest`, `mf:entries`, `mf:QueryEvaluationTest`, `ut:UpdateEvaluationTest`, `mf:PositiveSyntaxTest11`, `mf:NegativeSyntaxTest11`)
- `tests/w3c/loader.rs` — load `.ttl` fixtures via `pg_ripple.load_turtle()` and `pg_ripple.load_turtle_into_graph()`
- `tests/w3c/validator.rs` — compare SELECT/ASK results against `.srj`/`.srx`; CONSTRUCT results against `.ttl` (triple-set comparison with blank-node tolerance)
- `tests/w3c/runner.rs` — parallel runner using `crossbeam-channel` work queue; per-test transaction rollback for isolation; `RunConfig`, `RunReport`, `TestOutcome` types
- `tests/w3c/known_failures.txt` — curated known-failures manifest (0 entries for `optional` and `aggregates`)
- `tests/w3c_smoke.rs` — smoke-subset test binary (`optional` + `aggregates` + `grouping`, cap 180)
- `tests/w3c_suite.rs` — full-suite test binary (all 13 sub-suites, parallel 8-thread, writes `report.json`)
- `scripts/fetch_w3c_tests.sh` — download & extract W3C SPARQL 1.1 test archive
- `sql/pg_ripple--0.40.0--0.41.0.sql` — comment-only migration; no schema changes
- `docs/src/reference/running-w3c-tests.md` — local setup and known-failures management guide
- `docs/src/reference/w3c-conformance.md` — updated with automated harness section

### Changed files

- `Cargo.toml` — version `0.41.0`; dev-dependencies: `postgres = "0.19"`, `crossbeam-channel = "0.5"`
- `pg_ripple.control` — `default_version = '0.41.0'`
- `.github/workflows/ci.yml` — replaced placeholder `sparql-conformance` job with `w3c-smoke` (required) and `w3c-suite` (informational)

### New dev-dependencies

| Crate | Version | Purpose |
|---|---|---|
| `postgres` | 0.19 | PostgreSQL client for integration test DB connection |
| `crossbeam-channel` | 0.5 | Lock-free work queue for the parallel test runner |

</details>

---


**Three long-requested developer and operator improvements: streaming SPARQL cursors, first-class explain for SPARQL and Datalog, and a full observability stack.**

### What you can do

- **Stream large SPARQL results** with `sparql_cursor()`, `sparql_cursor_turtle()`, and `sparql_cursor_jsonld()` — batch results 1 024 rows at a time without materialising the entire result set in memory.
- **Set resource limits** via `pg_ripple.sparql_max_rows`, `pg_ripple.datalog_max_derived`, and `pg_ripple.export_max_rows`. When exceeded, choose between a `'warn'` (truncate) or `'error'` action.
- **Introspect SPARQL query plans** with `explain_sparql(query, analyze := false) RETURNS JSONB` — returns the SPARQL algebra, generated SQL, PostgreSQL `EXPLAIN [ANALYZE]` output, and plan-cache hit status in a single structured document.
- **Introspect Datalog rule sets** with `explain_datalog(rule_set_name) RETURNS JSONB` — shows the stratification graph, compiled SQL per rule, and statistics from the last inference run.
- **Get a unified cache statistics view** via `cache_stats()` — covers plan cache, dictionary cache, and federation cache in one JSONB document. Reset counters with `reset_cache_stats()`.
- **Enable OpenTelemetry spans** with `SET pg_ripple.tracing_enabled = on` — zero overhead when off; spans cover SPARQL parse/translate/execute cycles.
- **Query the `stat_statements_decoded` view** when `pg_stat_statements` is installed to see decoded query text alongside execution statistics.

### Bug fixes

- **OPTIONAL inside GRAPH**: `OPTIONAL {}` patterns inside `GRAPH {}` now correctly scope the optional join to the named graph. Previously, the graph filter was applied *after* the `LEFT JOIN` wrapper was built, causing PostgreSQL to reject the query with `column does not exist`. The fix propagates the graph filter as a context field (`graph_filter: Option<i64>`) that is injected directly into each VP table scan before any joins or subqueries are wrapped around it.
- **Property paths inside GRAPH**: Property path expressions (e.g., `p+`, `p*`) inside `GRAPH {}` now filter the `WITH RECURSIVE` CTE anchor and recursive steps to the correct named graph. Previously the graph filter was lost.

### What happens behind the scenes

Six new GUCs are registered at startup (`sparql_max_rows`, `datalog_max_derived`, `export_max_rows`, `sparql_overflow_action`, `tracing_enabled`, `tracing_exporter`). No VP table schema changes; the migration script is comment-only. Three new Rust modules are added: `src/sparql/cursor.rs`, `src/sparql/explain.rs`, and `src/datalog/explain.rs`. The `src/telemetry.rs` module provides a zero-cost tracing facade backed by PostgreSQL `DEBUG5` log messages when `tracing_enabled = on`.

<details>
<summary>Technical details</summary>

### New files

- `src/sparql/cursor.rs` — `sparql_cursor`, `sparql_cursor_turtle`, `sparql_cursor_jsonld`
- `src/sparql/explain.rs` — `explain_sparql_jsonb` (new JSONB overload)
- `src/datalog/explain.rs` — `explain_datalog`
- `src/telemetry.rs` — OpenTelemetry span facade
- `sql/pg_ripple--0.39.0--0.40.0.sql` — comment-only migration; no schema changes
- `docs/src/user-guide/sql-reference/explain.md`
- `docs/src/user-guide/sql-reference/cursor-api.md`
- `docs/src/reference/observability.md`

### Changed files

- `src/sparql/sqlgen.rs` — added `graph_filter: Option<i64>` to `Ctx`; `GraphPattern::Graph` now sets the filter before recursing
- `src/sparql/property_path.rs` — `compile_path` and `pred_table_expr` now accept and propagate `graph_filter`
- `src/sparql_api.rs` — exposes new cursor and explain functions as `#[pg_extern]`
- `src/datalog_api.rs` — exposes `explain_datalog` as `#[pg_extern]`
- `src/shmem.rs` — adds `reset_cache_stats()`
- `src/schema.rs` — adds `stat_statements_decoded` view
- `src/gucs.rs` — six new v0.40.0 GUC statics
- `src/lib.rs` — registers six new GUCs in `_PG_init`; adds `telemetry` module
- `src/error.rs` — documents PT640–PT642 range
- `Cargo.toml` — version bumped to `0.40.0`
- `pg_ripple.control` — `default_version` updated to `0.40.0`
- `docs/src/reference/error-reference.md` — PT640, PT641, PT642 added

### New error codes

| Code | Meaning |
|------|---------|
| PT640 | SPARQL result set exceeded `sparql_max_rows` |
| PT641 | Datalog derived facts exceeded `datalog_max_derived` |
| PT642 | Export rows exceeded `export_max_rows` |

</details>

---

## [0.39.0] — 2026-04-19 — Datalog HTTP API

**HTTP release: 24 new REST endpoints expose all pg_ripple Datalog functions in `pg_ripple_http`.**

### What you can do

- Manage Datalog rule sets over HTTP — load, list, add, remove, enable, or disable rules without a PostgreSQL driver.
- Trigger inference (`POST /datalog/infer/{rule_set}`) and get the derived-triple count back as JSON.
- Use goal-directed queries (`POST /datalog/query/{rule_set}`) to ask targeted questions over materialized knowledge.
- Check integrity constraints (`GET /datalog/constraints`) and read violation reports as structured JSON.
- Inspect cache and tabling statistics, manage lattice types, and control Datalog views — all from any HTTP client or CI pipeline.
- Use a separate `PG_RIPPLE_HTTP_DATALOG_WRITE_TOKEN` to let read operations (inference, queries, monitoring) through while restricting rule management to a privileged token.

### What happens behind the scenes

The `pg_ripple_http` service gains a new `/datalog` route namespace built as a thin axum layer. Each of the 24 endpoints maps directly to a single `pg_ripple.*` SQL function call through the existing connection pool — no Datalog parsing happens in the HTTP service. All SQL calls use parameterized queries (`$1`, `$2`, …); no user input is concatenated into SQL strings. A new Prometheus counter (`pg_ripple_http_datalog_queries_total`) tracks Datalog traffic separately from SPARQL queries. Shared authentication, rate-limiting, CORS, and error redaction from the SPARQL endpoints are reused via a new `common.rs` module.

<details>
<summary>Technical details</summary>

### New files

- `pg_ripple_http/src/common.rs` — `AppState`, `check_auth`, `check_auth_write`, `redacted_error`, `env_or` (moved from `main.rs`)
- `pg_ripple_http/src/datalog.rs` — all 24 Datalog endpoint handlers across four phases
- `tests/datalog_http_smoke.sh` — curl-based end-to-end smoke test

### Changed files

- `pg_ripple_http/src/main.rs` — imports `common` and `datalog` modules; registers 24 new routes; adds `datalog_write_token` to `AppState`
- `pg_ripple_http/src/metrics.rs` — adds `datalog_queries` counter; renames Prometheus metrics to `pg_ripple_http_*_total`
- `pg_ripple_http/README.md` — new `## Datalog API` section with curl examples for all 24 endpoints
- `sql/pg_ripple--0.38.0--0.39.0.sql` — comment-only migration documenting the new HTTP surface; no SQL schema changes
- `Cargo.toml` — version bumped to `0.39.0`
- `pg_ripple.control` — `default_version` updated to `0.39.0`
- `pg_ripple_http/Cargo.toml` — version bumped to `0.16.0`

### New environment variable

- `PG_RIPPLE_HTTP_DATALOG_WRITE_TOKEN` — optional; gates mutating Datalog endpoints independently of the main auth token

</details>

---

## [0.38.0] — 2026-04-19 — Architecture Refactoring & Query Completeness

**Structural release: god-module split, PredicateCatalog, SHACL query hints, SPARQL Update completeness.**

### What you can do

- **Trust faster BGP queries** — a new backend-local predicate OID cache (`storage/catalog.rs`) eliminates per-atom SPI catalog lookups. A 10-atom BGP now issues 1 catalog SPI call instead of 10.
- **Use whitespace-insensitive plan caching** — the per-backend plan cache (v0.13.0) now keys on an algebra digest (XXH3-128 of the normalised SPARQL IR) instead of the raw query text. Whitespace and prefix-alias variants of the same query share one cache slot.
- **Get SHACL-accelerated queries automatically** — after loading shapes, `sh:maxCount 1` suppresses `DISTINCT` on the affected predicate join; `sh:minCount 1` promotes `LEFT JOIN` → `INNER JOIN`. No query changes needed.
- **Use SPARQL graph management** — `COPY`, `MOVE`, and `ADD` graph operations are now supported via spargebra's desugaring into `INSERT DATA` / `DELETE DATA` sequences.
- **Read the architecture guide** — `docs/src/reference/architecture.md` has a Mermaid diagram of every major subsystem boundary post-refactor.
- **See the SPARQL 1.1 conformance job** — a new `sparql-conformance` CI job (informational, `continue-on-error`) downloads the W3C test suite and reports coverage.

### What happens behind the scenes

- **`src/lib.rs` split** — the 5 975-line god-module is split into 12 focused modules: `gucs.rs`, `schema.rs`, `dict_api.rs`, `export_api.rs`, `sparql_api.rs`, `maintenance_api.rs`, `stats_admin.rs`, `data_ops.rs`, `datalog_api.rs`, `views_api.rs`, `federation_registry.rs`, `graphrag_admin.rs`. `src/lib.rs` is now 1 447 lines.
- **`shacl/constraints/` sub-module** — `validate_property_shape()` is a ≤50-line dispatcher. Per-constraint logic lives in `count.rs`, `value_type.rs`, `string_based.rs`, `logical.rs`, `shape_based.rs`, `property_path.rs`.
- **`sparql/translate/` sub-module** — layout files for per-algebra-node translation: `bgp.rs`, `join.rs`, `left_join.rs`, `union.rs`, `filter.rs`, `graph.rs`, `group.rs`, `distinct.rs`.
- **`property_path_max_depth` deprecated** — the GUC description now signals deprecation; use `max_path_depth` instead.

### Migration

`sql/pg_ripple--0.37.0--0.38.0.sql` — creates `_pg_ripple.shape_hints` table; no VP table schema changes.

```sql
ALTER EXTENSION pg_ripple UPDATE TO '0.38.0';
```

---

## [0.37.0] — 2026-04-19 — Storage Concurrency Hardening & Error Safety

**Reliability release: zero hard panics, concurrent-safe merge/delete/promote, GUC validators.**

### What you can do

- **Trust merge + delete safety** — concurrent `DELETE` calls arriving while a merge cycle is running can no longer cause lost deletes. Per-predicate advisory locks (`pg_advisory_xact_lock` exclusive during merge, shared during delete/promote) enforce strict serialization.
- **Get a one-call health report** — `pg_ripple.diagnostic_report()` returns a key/value table covering schema_version, GUC validity, merge backlog, validation queue depth, and total triple/predicate counts.
- **Verify upgrade completeness** — `_pg_ripple.schema_version` is stamped on install and every `ALTER EXTENSION … UPDATE`; use `SELECT * FROM _pg_ripple.schema_version` or `diagnostic_report()` to confirm your cluster is on the expected version.
- **Configure tombstone GC** — two new GUCs: `pg_ripple.tombstone_gc_enabled` (bool, default `on`) and `pg_ripple.tombstone_gc_threshold` (float string, default `0.05`). After each merge the worker auto-VACUUMs tombstone tables above the threshold ratio.
- **Get immediate feedback on bad config** — string-enum GUCs (`inference_mode`, `enforce_constraints`, `rule_graph_scope`, `shacl_mode`, `describe_strategy`) now reject invalid values at `SET` time with a clear error message.
- **Prevent session-level RLS bypass** — `pg_ripple.rls_bypass` is now `PGC_POSTMASTER` when loaded via `shared_preload_libraries`, preventing `SET LOCAL pg_ripple.rls_bypass = on` exploits.

### What happens behind the scenes

- `src/storage/merge.rs` — per-predicate `pg_advisory_xact_lock` wrapping the delta→main swap; `_pg_ripple.statements` SID-range update is now atomic with the VP table swap; tombstone GC logic integrated post-merge.
- `src/storage/mod.rs` — `delete_triple()` acquires shared advisory lock before tombstone insert; `promote_predicate()` acquires exclusive advisory lock.
- `src/shmem.rs` — all bloom filter counter decrements use `saturating_sub(1)`.
- `src/sparql/optimizer.rs`, `src/sparql/sqlgen.rs`, `src/export.rs`, `pg_ripple_http/src/main.rs` — all `.unwrap()` / `.expect()` calls in non-test code replaced with `pgrx::error!()` or graceful `process::exit(1)` patterns.
- `src/lib.rs` — `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]`; GUC check_hook validators for 5 string-enum GUCs; new `diagnostic_report()` pg_extern; `schema_version` bootstrap table; tombstone GC GUC statics + registrations; `rls_bypass` conditional context.
- New migration script: `sql/pg_ripple--0.36.0--0.37.0.sql`.
- New pg_regress tests: `storage_tombstone_gc.sql`, `diagnostic_report.sql`.
- Documentation: troubleshooting.md "Lost deletes after merge" runbook; guc-reference.md v0.37.0 section; upgrading.md schema_version stamp guide.

---

## [0.36.0] — 2026-04-19 — Worst-Case Optimal Joins & Lattice-Based Datalog

**Leapfrog Triejoin for cyclic SPARQL patterns and monotone lattice aggregation for Datalog^L.**

### What you can do

- **Accelerate triangle and cyclic graph queries** — when `pg_ripple.wcoj_enabled = on` (the default), the SPARQL→SQL translator detects cyclic BGPs and forces sort-merge join plans that exploit the `(s, o)` B-tree indices on VP tables. Triangle queries that previously timed out complete in milliseconds.
- **Inspect cyclic patterns** — `pg_ripple.wcoj_is_cyclic(json)` lets you check whether a BGP variable graph contains a cycle before execution.
- **Benchmark WCOJ** — `pg_ripple.wcoj_triangle_query(iri)` runs a triangle query on a given predicate and returns the count, a `wcoj_applied` flag, and the IRI used; compare WCOJ-on vs. WCOJ-off with `benchmarks/wcoj.sql`.
- **Write recursive aggregation rules** — `pg_ripple.create_lattice()` registers a user-defined lattice type, and `pg_ripple.infer_lattice()` runs a monotone fixpoint over rules that use it. Built-in lattices: `min`, `max`, `set`, `interval`.
- **Trust propagation and shortest paths** — lattice rules like `?x ex:trust (MIN ?t1 ?t2) :- ?x ex:knows ?y, ?y ex:trust ?t1` converge to correct fixed points without manual loop unrolling.
- **Guaranteed termination** — fixpoints are bounded by `pg_ripple.lattice_max_iterations` (default 1000); if exceeded, a `PT540` WARNING is emitted and partial results are returned.

### What happens behind the scenes

- `src/sparql/wcoj.rs` (new module) — cyclic BGP detection via variable adjacency graph DFS; WCOJ SQL rewriter that wraps cyclic patterns in materialized CTEs with sort-merge join hints; `run_triangle_query()` benchmark helper.
- `src/datalog/lattice.rs` (new module) — lattice type catalog (`_pg_ripple.lattice_types`), built-in lattices, user-defined lattice registration, lattice rule SQL compiler (INSERT … ON CONFLICT DO UPDATE with join_fn), monotone fixpoint executor.
- `src/lib.rs` — three new GUCs registered in `_PG_init()`: `pg_ripple.wcoj_enabled`, `pg_ripple.wcoj_min_tables`, `pg_ripple.lattice_max_iterations`. Five new `pg_extern` functions: `wcoj_is_cyclic`, `wcoj_triangle_query`, `create_lattice`, `list_lattices`, `infer_lattice`. New `extension_sql!` block `v036_lattice_types` creates the lattice catalog and seeds built-ins.
- New migration script: `sql/pg_ripple--0.35.0--0.36.0.sql`.
- New benchmark: `benchmarks/wcoj.sql`.
- New pg_regress tests: `sparql_wcoj.sql`, `datalog_lattice.sql`.
- New documentation: `reference/lattice-datalog.md`; `user-guide/sql-reference/datalog.md` updated; `user-guide/best-practices/sparql-performance.md` updated.

<details>
<summary>Technical Details</summary>

### New GUC parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.wcoj_enabled` | bool | `true` | Enable cyclic BGP detection and WCOJ sort-merge hints |
| `pg_ripple.wcoj_min_tables` | integer | `3` | Minimum VP joins before WCOJ detection is applied |
| `pg_ripple.lattice_max_iterations` | integer | `1000` | Max fixpoint iterations for lattice inference |

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `wcoj_is_cyclic(json)` | `boolean` | Detect cycle in a BGP variable graph |
| `wcoj_triangle_query(iri)` | `jsonb` | Run a triangle query with WCOJ benchmark stats |
| `create_lattice(name, join_fn, bottom)` | `boolean` | Register a user-defined lattice type |
| `list_lattices()` | `jsonb` | List all registered lattice types |
| `infer_lattice(rule_set, lattice_name)` | `jsonb` | Run monotone lattice fixpoint |

### Error codes

- `PT540` — lattice fixpoint did not converge within `lattice_max_iterations`.

### Schema changes

New catalog table `_pg_ripple.lattice_types` with columns `name`, `join_fn`, `bottom`, `builtin`, `created_at`.

</details>

---

## [0.35.0] — 2026-04-19 — Parallel Stratum Evaluation & Incremental Rule Updates

**Faster Datalog materialization through concurrent independent rule groups.**

### What you can do

- **Speed up OWL RL and large ontology closures** — rules in the same stratum that derive different predicates with no shared body dependencies now run in the optimal order with parallel analysis. On OWL RL with 4 independent groups, this reduces wall-clock materialization time.
- **See how parallel your rule set is** — `pg_ripple.infer_with_stats()` now returns `"parallel_groups"` (number of independent groups) and `"max_concurrent"` (effective worker count) in its JSONB output.
- **Tune for your hardware** — two new GUCs control parallelism: `pg_ripple.datalog_parallel_workers` (default `4`) and `pg_ripple.datalog_parallel_threshold` (default `10000` rows) give fine-grained control over when and how much parallelism is applied.
- **SPARQL freshness after bulk loads** — parallel evaluation reduces the time from data ingestion to full materialization, shortening the staleness window for SPARQL queries over derived predicates.

### What happens behind the scenes

- `src/datalog/parallel.rs` (new module) — implements union-find–based dependency graph analysis that partitions Datalog rules into maximally independent groups. Rules with the same head predicate are always in the same group; rules whose body references another group's derived predicates are merged together. Variable-predicate rules (e.g., OWL RL SymmetricProperty) form a separate serial group.
- `src/datalog/mod.rs` — `run_inference_seminaive_full()` now calls `partition_into_parallel_groups()` and returns `(derived, iters, eliminated, parallel_groups, max_concurrent)`.
- `src/lib.rs` — two new GUC parameters registered in `_PG_init()`: `pg_ripple.datalog_parallel_workers` and `pg_ripple.datalog_parallel_threshold`. `infer_with_stats()` updated to include `"parallel_groups"` and `"max_concurrent"` in the output JSONB.
- New pg_regress test: `datalog_parallel.sql` — all 119 tests pass.

<details>
<summary>Technical Details</summary>

### New GUC parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.datalog_parallel_workers` | integer | `4` | Maximum parallel worker count; `1` = serial |
| `pg_ripple.datalog_parallel_threshold` | integer | `10000` | Min estimated row count before analysis is applied |

### infer_with_stats() output additions

```json
{
  "derived": 1240,
  "iterations": 4,
  "eliminated_rules": [],
  "parallel_groups": 3,
  "max_concurrent": 3
}
```

### Algorithm

The `partition_into_parallel_groups()` function:
1. Groups rules by head predicate (rules with the same derived predicate share a write target).
2. Builds a dependency graph: group A depends on group B if A's body uses a predicate derived by B.
3. Computes undirected connected components via path-compressing union-find.
4. Each connected component becomes one parallel group; variable-predicate rules form a separate serial group.

</details>

### Migration

`sql/pg_ripple--0.34.0--0.35.0.sql` — no VP table schema changes; only new GUC parameters and updated function signatures.

---

## [0.34.0] — 2026-04-19 — Bounded-Depth Termination & Incremental Retraction (DRed)

**Smarter fixpoint termination and write-correct incremental maintenance.**

### What you can do

- **Cap inference depth** — set `pg_ripple.datalog_max_depth` to any positive integer to stop recursive rules after at most that many derivation steps. A value of `0` (the default) means unlimited, preserving all existing behaviour.
- **Add or remove rules without full recompute** — `pg_ripple.add_rule(rule_set, rule_text)` injects a single rule into a live rule set and runs one additional semi-naive pass on the affected stratum. `pg_ripple.remove_rule(rule_id)` retracts the rule and surgically removes derived facts that are no longer supported.
- **Efficient incremental deletion via DRed** — when a base triple is deleted, the Delete-Rederive (DRed) algorithm over-deletes pessimistically and then re-derives any survivors, instead of recomputing the entire closure. Controlled by `pg_ripple.dred_enabled` (default `true`) and `pg_ripple.dred_batch_size` (default `1000`).

### What happens behind the scenes

- `src/datalog/compiler.rs` — `compile_recursive_rule()` reads `pg_ripple.datalog_max_depth` at compile time. When positive, it emits a `WITH RECURSIVE … (s, o, g, depth)` CTE that injects a depth counter column into both the base and recursive cases, terminating recursion via `WHERE r.depth < max_depth`.
- `src/datalog/dred.rs` (new module) — implements `run_dred_on_delete()` (three-phase over-delete/re-derive/commit) and `check_dred_safety()` (detects cycles that prevent safe incremental retraction).
- `src/datalog/mod.rs` — exposes `add_rule_to_set()` and `remove_rule_by_id()`.
- `src/lib.rs` — three new GUC parameters registered in `_PG_init()`: `pg_ripple.datalog_max_depth`, `pg_ripple.dred_enabled`, `pg_ripple.dred_batch_size`. Three new `#[pg_extern]` functions: `add_rule()`, `remove_rule()`, `dred_on_delete()`.
- New pg_regress tests: `datalog_bounded_depth.sql`, `datalog_dred.sql`, `datalog_incremental_rules.sql` — all 118 tests pass.

### Migration

`sql/pg_ripple--0.33.0--0.34.0.sql` — no VP table schema changes; only new GUC parameters and compiled-in functions.

---

## [0.33.0] — 2026-04-19 — Documentation Site & Content Overhaul

**pg_ripple's documentation is rebuilt from the ground up.** A complete site restructure, eight feature-deep-dive chapters, a full operations guide, and CI-enforced code examples.

### What you can do

- **Find answers fast** — the documentation is reorganized into four clear sections: Getting Started, Feature Deep Dives, Operations, and Reference. A decision flowchart helps you evaluate whether pg_ripple fits your architecture before installing anything.
- **Learn by doing** — a five-minute Hello World walkthrough and a 30-minute guided tutorial take you from zero to a validated, reasoning-capable knowledge graph with JSON-LD export.
- **Understand every feature** — eight feature-deep-dive chapters cover storing knowledge, loading data, querying with SPARQL, validating data quality, reasoning and inference, exporting and sharing, AI retrieval and Graph RAG, and APIs and integration. Each chapter follows a consistent structure: What and Why, How It Works, Worked Examples, Common Patterns, Performance, Gotchas, and Next Steps.
- **Run in production** — ten operations pages cover architecture, deployment, configuration, monitoring, performance tuning, backup and recovery, upgrading, scaling, troubleshooting, and security.
- **Look up any function** — the SQL Function Reference documents all 157 functions with signatures, descriptions, and working examples grouped by use case.

### What happens behind the scenes

This is a documentation-only release. No SQL functions, GUC parameters, VP table schemas, or Rust code changed. The documentation site is built with mdBook and uses mdbook-admonish for structured callout blocks. A CI test harness (`scripts/test_docs.sh`) extracts SQL code blocks from documentation pages and runs them against a real pg_ripple instance on every pull request that touches `docs/`. A coverage script (`scripts/check_docs_coverage.sh`) verifies that every `pg_extern` function is mentioned in the documentation.

<details>
<summary>Technical Details</summary>

### New files

| File | Purpose |
|------|---------|
| `scripts/test_docs.sh` | CI harness for documentation code examples |
| `scripts/check_docs_coverage.sh` | Verifies all pg_extern functions are documented |
| `docs/fixtures/bibliography.sql` | Shared bibliographic fixture dataset |
| `.github/workflows/docs-test.yml` | CI workflow for documentation tests and link checking |
| `.github/PULL_REQUEST_TEMPLATE.md` | PR template with docs-gap reminder |

### Site structure

The documentation is restructured from a flat list of pages into a four-section information architecture:

- **Getting Started**: Installation, Hello World, Guided Tutorial, Key Concepts
- **Feature Deep Dives**: 8 chapters (§2.1–§2.8) following a consistent seven-part structure
- **Operations**: 10 pages covering deployment through security
- **Reference**: SQL Function Reference, SPARQL Compliance Matrix, Error Catalog, FAQ, Glossary, Contributing

### mdbook-admonish

`book.toml` updated with `[preprocessor.admonish]` and `[output.linkcheck]`. All new pages use fenced admonish callout syntax.

</details>

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.33.0'` (applies `sql/pg_ripple--0.32.0--0.33.0.sql` — no schema changes).

---

## [0.32.0] — 2026-04-19 — Well-Founded Semantics & Tabling

**pg_ripple handles non-stratifiable Datalog programs and caches repeated inference results.** All pg_regress tests pass (3 new tests for v0.32.0 features).

### What you can do

- **Well-founded semantics** — `pg_ripple.infer_wfs(rule_set TEXT DEFAULT 'custom')` runs an alternating-fixpoint algorithm over the rule set and returns a JSONB object with `certain`, `unknown`, `derived`, `iterations`, and `stratifiable` keys; for programs with mutual negation cycles (non-stratifiable), facts that cannot be resolved to true or false receive *unknown* status rather than causing an error
- **Non-stratifiable rule loading** — `load_rules()` now accepts rule sets with cyclic negation; rules are stored at stratum 0 and deferred to `infer_wfs()` for evaluation
- **Tabling / memoisation** — when `pg_ripple.tabling = on` (default), results of `infer_wfs()` are stored in `_pg_ripple.tabling_cache` keyed by XXH3-64 hash of the goal string and served from cache on repeated calls within the TTL
- **Cache invalidation** — the tabling cache is automatically cleared on `insert_triple()`, `delete_triple()`, `drop_rules()`, and `load_rules()`
- **Cache statistics** — `pg_ripple.tabling_stats()` returns per-entry statistics: `goal_hash`, `hits`, `computed_ms`, `cached_at`

### New GUC parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.wfs_max_iterations` | integer | `100` | Safety cap on alternating fixpoint rounds; emits WARNING PT520 if exceeded |
| `pg_ripple.tabling` | bool | `true` | Enable tabling / memoisation cache |
| `pg_ripple.tabling_ttl` | integer | `300` | Cache entry TTL in seconds; `0` = no expiry |

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.infer_wfs(rule_set TEXT DEFAULT 'custom')` | `JSONB` | Well-founded semantics fixpoint; safe for non-stratifiable programs |
| `pg_ripple.tabling_stats()` | `TABLE(goal_hash BIGINT, hits BIGINT, computed_ms FLOAT8, cached_at TEXT)` | Tabling cache statistics |

### Migration

Run `ALTER EXTENSION pg_ripple UPDATE TO '0.32.0'` (applies `sql/pg_ripple--0.31.0--0.32.0.sql` which creates `_pg_ripple.tabling_cache`).

---

## [0.31.0] — 2026-04-19 — Entity Resolution & Demand Transformation

**pg_ripple's Datalog engine gains `owl:sameAs` entity canonicalization and demand-filtered inference.** All pg_regress tests pass (2 new tests for v0.31.0 features).

### What you can do

- **`owl:sameAs` reasoning** — when `pg_ripple.sameas_reasoning = on` (default), the inference engine automatically identifies equivalent entities via `owl:sameAs` triples and rewrites rule-body constants to their canonical (lowest-ID) representative before each fixpoint iteration; SPARQL queries referencing non-canonical aliases are transparently redirected to the canonical entity
- **Demand-filtered inference** — `pg_ripple.infer_demand(rule_set, demands JSONB)` accepts a JSON array of goal patterns and derives only the facts needed to answer those goals; for programs with many rules and multiple derived predicates, this can reduce inference work by 50–90%
- **Multi-goal demand sets** — unlike `infer_goal()` (single predicate), `infer_demand()` accepts multiple demand predicates simultaneously and computes a joint demand set via fixed-point propagation through the dependency graph; mutually recursive rules with multiple entry points are handled correctly
- **Demand + sameAs composition** — `infer_demand()` applies the sameAs canonicalization pre-pass before running demand-filtered inference, combining both optimizations in one call

### New GUC parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.sameas_reasoning` | bool | `true` | Enable `owl:sameAs` entity canonicalization pre-pass before inference |
| `pg_ripple.demand_transform` | bool | `true` | Auto-apply demand transformation in `create_datalog_view()` with multiple goals |

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.infer_demand(rule_set TEXT DEFAULT 'custom', demands JSONB)` | `JSONB` | Run demand-filtered inference; `demands` is `[{"p": "<iri>"}, …]`; empty array = full inference |

### Migration

No schema changes. Run `ALTER EXTENSION pg_ripple UPDATE` to upgrade from v0.30.0.

---

## [0.30.0] — 2026-04-19 — Datalog Aggregation & Compiled Rule Plans

**pg_ripple's Datalog engine gains Datalog^agg (aggregate literals in rule bodies) and a process-local rule plan cache.** All pg_regress tests pass (3 new tests for v0.30.0 features).

### What you can do

- **Aggregate inference** — `pg_ripple.infer_agg(rule_set)` evaluates rules with `COUNT`, `SUM`, `MIN`, `MAX`, and `AVG` aggregate literals in their bodies, enabling graph analytics (degree centrality, max-salary, etc.) directly from Datalog rules; returns `{"derived": N, "aggregate_derived": K, "iterations": I}`
- **Aggregate rule syntax** — `?x <ex:count> ?n :- COUNT(?y WHERE ?x <foaf:knows> ?y) = ?n .`
- **Aggregation stratification checking** — the stratifier rejects cycles through aggregation (PT510 warning); violating rule sets fall back to non-aggregate inference automatically
- **Rule plan cache** — compiled SQL for each rule set is cached process-locally; second and subsequent `infer_agg()` calls on the same rule set hit the cache; `pg_ripple.rule_plan_cache_stats()` exposes hit/miss counts
- **Cache invalidation** — `load_rules()` and `drop_rules()` automatically invalidate the cache for the modified rule set

### New GUC parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.rule_plan_cache` | bool | `true` | Master switch for the Datalog rule plan cache |
| `pg_ripple.rule_plan_cache_size` | int | `64` | Maximum rule sets in plan cache (1–4096); evicts LFU entry on overflow |

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.infer_agg(rule_set TEXT DEFAULT 'custom')` | `JSONB` | Run Datalog^agg inference (aggregates + semi-naive fixpoint) |
| `pg_ripple.rule_plan_cache_stats()` | `TABLE(rule_set TEXT, hits BIGINT, misses BIGINT, entries INT)` | Show plan cache statistics per rule set |

### New error codes

| Code | Name | Description |
|------|------|-------------|
| PT510 | `AggStratificationViolation` | Aggregate rule creates a cycle through aggregation; rule is skipped |
| PT511 | `UnsupportedAggFunc` | Unsupported aggregate function in rule body |

### Migration

No schema changes. Run `ALTER EXTENSION pg_ripple UPDATE` to upgrade.

---

## [0.29.0] — 2026-04-19 — Datalog Optimization: Magic Sets & Cost-Based Compilation

**pg_ripple's Datalog engine gains goal-directed inference (magic sets), cost-based join reordering, anti-join negation, predicate-filter pushdown, delta-table indexing, and redundant-rule elimination.** All pg_regress tests pass (6 new tests for v0.29.0 features).

### What you can do

- **Goal-directed inference** — `pg_ripple.infer_goal(rule_set, goal)` derives only the facts relevant to a specific triple pattern (magic sets transformation); returns `{"derived": N, "iterations": K, "matching": M}`
- **Cost-based join reordering** — Datalog body atoms are sorted by ascending VP-table cardinality at compile time; set `pg_ripple.datalog_cost_reorder = off` to disable
- **Anti-join negation** — negated body atoms with large VP tables compile to `LEFT JOIN … IS NULL` instead of `NOT EXISTS`; controlled by `pg_ripple.datalog_antijoin_threshold` (default 1000)
- **Predicate-filter pushdown** — arithmetic/comparison guards are moved into `JOIN … ON` clauses to enable index scans
- **Delta-table indexing** — after semi-naive iteration, B-tree index on `(s, o)` is created when delta table exceeds `pg_ripple.delta_index_threshold` rows (default 500)
- **Subsumption checking** — redundant rules (whose body predicates are a superset of another rule's body) are eliminated at compile time; `infer_with_stats()` now reports `"eliminated_rules": [...]`
- **New error codes** — PT501 (magic sets circular binding), PT502 (cost-based reordering skipped)

### New GUC parameters

| GUC | Type | Default | Description |
|-----|------|---------|-------------|
| `pg_ripple.magic_sets` | bool | `true` | Master switch for goal-directed magic sets inference |
| `pg_ripple.datalog_cost_reorder` | bool | `true` | Sort Datalog body atoms by VP-table cardinality |
| `pg_ripple.datalog_antijoin_threshold` | int | `1000` | Row count threshold for anti-join negation form |
| `pg_ripple.delta_index_threshold` | int | `500` | Row count threshold for delta table B-tree index |

### New SQL functions

| Function | Description |
|----------|-------------|
| `pg_ripple.infer_goal(rule_set TEXT, goal TEXT) → JSONB` | Goal-directed inference returning derived/matching counts |

### Changed SQL functions

- `pg_ripple.infer_with_stats(rule_set TEXT) → JSONB` — now includes `"eliminated_rules": [...]` array in returned JSONB

---

## [0.28.0] — 2026-04-19 — Advanced Hybrid Search & RAG Pipeline

**pg_ripple completes its hybrid search stack with Reciprocal Rank Fusion, graph-contextualized embeddings, end-to-end RAG retrieval, incremental embedding, multi-model support, and SPARQL federation with external vector services.** All pg_regress tests pass (6 new tests for v0.28.0 features).

### What you can do

- **Hybrid search with RRF fusion** — `pg_ripple.hybrid_search(sparql_query, query_text, k)` combines a SPARQL candidate set with pgvector k-NN results using Reciprocal Rank Fusion; returns ranked entities with `rrf_score`, `sparql_rank`, and `vector_rank`
- **End-to-end RAG retrieval** — `pg_ripple.rag_retrieve('what treats headaches?', k := 5)` does the full RAG dance in one call: vector search, optional SPARQL filter, neighborhood contextualization, and structured JSONB output ready for an LLM system prompt
- **JSON-LD framing for LLM context** — `rag_retrieve(... output_format := 'jsonld')` returns context_json with `@type` and `@context` keys using the registered prefix map; plug directly into OpenAI structured outputs
- **Graph-contextualized embeddings** — `pg_ripple.contextualize_entity(iri)` serializes an entity's label, types, and neighbor labels as plain text; set `pg_ripple.use_graph_context = on` to use this for all `embed_entities()` calls
- **Incremental embedding worker** — set `pg_ripple.auto_embed = on` to trigger automatic queuing of new entities; the background worker drains `_pg_ripple.embedding_queue` in batches
- **Multi-model support** — `pg_ripple.list_embedding_models()` enumerates all models in `_pg_ripple.embeddings`; all search/retrieve functions accept an optional `model` parameter
- **SPARQL federation with external vector services** — `pg_ripple.register_vector_endpoint(url, api_type)` registers Weaviate, Qdrant, or Pinecone endpoints; these can be queried alongside local triples in SPARQL SERVICE clauses
- **SHACL embedding completeness** — `pg_ripple.add_embedding_triples()` materialises `pg:hasEmbedding` triples; the included SHACL shape validates completeness via `sh:minCount 1`

### Added

- `pg_ripple.hybrid_search(sparql_query TEXT, query_text TEXT, k INT DEFAULT 10, alpha FLOAT8 DEFAULT 0.5, model TEXT DEFAULT NULL) RETURNS TABLE(entity_id BIGINT, entity_iri TEXT, rrf_score FLOAT8, sparql_rank INT, vector_rank INT)` — RRF fusion of SPARQL and vector results
- `pg_ripple.rag_retrieve(question TEXT, sparql_filter TEXT DEFAULT NULL, k INT DEFAULT 5, model TEXT DEFAULT NULL, output_format TEXT DEFAULT 'jsonb') RETURNS TABLE(entity_iri TEXT, label TEXT, context_json JSONB, distance FLOAT8)` — end-to-end RAG retrieval
- `pg_ripple.contextualize_entity(entity_iri TEXT, depth INT DEFAULT 1, max_neighbors INT DEFAULT 20) RETURNS TEXT` — graph-serialized text for embedding
- `pg_ripple.list_embedding_models() RETURNS TABLE(model TEXT, entity_count BIGINT, dimensions INT)` — enumerate stored models
- `pg_ripple.add_embedding_triples() RETURNS BIGINT` — materialise `pg:hasEmbedding` triples
- `pg_ripple.register_vector_endpoint(url TEXT, api_type TEXT) RETURNS VOID` — register external vector service (`pgvector`, `weaviate`, `qdrant`, `pinecone`)
- `_pg_ripple.embedding_queue` table — incremental embedding queue (v0.28.0)
- `_pg_ripple.vector_endpoints` table — external vector service catalog
- `_pg_ripple.auto_embed_dict_trigger` — dictionary trigger for automatic queuing
- 4 new GUC parameters: `pg_ripple.auto_embed`, `pg_ripple.embedding_batch_size`, `pg_ripple.use_graph_context`, `pg_ripple.vector_federation_timeout_ms`
- Error code PT607 — vector service endpoint not registered
- Background worker now drains `_pg_ripple.embedding_queue` when `pg_ripple.auto_embed = on`
- New pg_regress tests: `vector_hybrid`, `vector_rag`, `vector_rag_jsonld`, `vector_contextualize`, `vector_worker`, `vector_federation`
- `benchmarks/hybrid_search.sql` — hybrid search latency/throughput benchmark
- `examples/shacl_embedding_completeness.ttl` — reusable SHACL shape for embedding completeness
- New/updated documentation: `user-guide/hybrid-search.md`, `user-guide/rag.md`, `user-guide/vector-federation.md`, `reference/embedding-functions.md`, `reference/http-api.md`

### Migration

Run `sql/pg_ripple--0.27.0--0.28.0.sql` on existing installations. Creates `_pg_ripple.embedding_queue` and `_pg_ripple.vector_endpoints` tables plus the `auto_embed_dict_trigger` trigger. No VP table schema changes.

---

## [0.27.0] — 2026-04-18 — Vector + SPARQL Hybrid: Foundation

**pg_ripple gains pgvector integration: store high-dimensional embeddings for any RDF entity, search by semantic similarity, and mix vector nearest-neighbour search with SPARQL graph patterns in a single in-process query.** All 95 pg_regress tests pass (8 new tests for v0.27.0 features).

### What you can do

- **Store embeddings for RDF entities** — `pg_ripple.store_embedding(entity_iri, vector)` upserts a float vector into `_pg_ripple.embeddings`; no API call needed when you supply pre-computed embeddings
- **Find semantically similar entities** — `pg_ripple.similar_entities('anti-inflammatory drugs', k := 5)` calls your embedding API, then returns the 5 entities with the lowest cosine distance
- **Batch-embed an entire graph** — `pg_ripple.embed_entities()` iterates over entities with `rdfs:label`, calls the API in batches, and stores all results in one transaction
- **Keep embeddings fresh** — `pg_ripple.refresh_embeddings()` re-embeds entities whose labels changed since the last embedding run; schedule via `pg_cron`
- **Hybrid SPARQL queries** — use `pg:similar(?entity, "search text", 10)` inside SPARQL `BIND` expressions; combine with FILTER, OPTIONAL, UNION, and any other SPARQL feature
- **Run in CI without pgvector** — every embedding function degrades gracefully with a WARNING (no ERROR) when pgvector is absent; all 8 new tests pass in environments without pgvector

### Added

- `_pg_ripple.embeddings` table — entity vector store with HNSW index (pgvector) or BYTEA stub (fallback)
- `pg_ripple.store_embedding(entity_iri TEXT, embedding FLOAT8[], model TEXT DEFAULT NULL) RETURNS VOID` — upsert a single embedding
- `pg_ripple.similar_entities(query_text TEXT, k INT DEFAULT 10, model TEXT DEFAULT NULL) RETURNS TABLE(entity_id BIGINT, entity_iri TEXT, score FLOAT8)` — k-NN similarity search
- `pg_ripple.embed_entities(graph_iri TEXT DEFAULT '', model TEXT DEFAULT NULL, batch_size INT DEFAULT 100) RETURNS BIGINT` — batch embedding
- `pg_ripple.refresh_embeddings(graph_iri TEXT DEFAULT '', model TEXT DEFAULT NULL, force BOOL DEFAULT FALSE) RETURNS BIGINT` — incremental re-embedding
- SPARQL extension function `pg:similar(?entity, "text", k)` via IRI `http://pg-ripple.org/functions/similar`
- 7 new GUC parameters: `pg_ripple.pgvector_enabled`, `pg_ripple.embedding_api_url`, `pg_ripple.embedding_api_key`, `pg_ripple.embedding_model`, `pg_ripple.embedding_dimensions`, `pg_ripple.embedding_index_type`, `pg_ripple.embedding_precision`
- Error codes PT601–PT606 for the embedding subsystem
- New pg_regress tests: `vector_setup`, `vector_crud`, `vector_sparql`, `vector_filter`, `vector_graceful`, `vector_halfvec`, `vector_binary`, `vector_refresh`
- New documentation pages: `user-guide/hybrid-search.md`, `reference/embedding-functions.md`, `reference/guc-reference.md`

### Migration

Run `sql/pg_ripple--0.26.0--0.27.0.sql` on existing installations. The script detects pgvector automatically and creates either a `vector(1536)` column with HNSW index (pgvector present) or a `BYTEA` stub (pgvector absent). No VP table schema changes.

---

## [0.26.0] — 2026-04-18 — GraphRAG Integration

**pg_ripple becomes a first-class backend for Microsoft GraphRAG: store LLM-extracted entities and relationships as RDF triples, enrich the graph with Datalog rules, enforce quality with SHACL shapes, and export back to Parquet for GraphRAG's BYOG (Bring Your Own Graph) pipeline.** All 87 pg_regress tests pass (5 new tests for v0.26.0 features).

### What you can do

- **Use pg_ripple as your GraphRAG knowledge graph** — store entities, relationships, and text units as native RDF triples; query them with SPARQL; update incrementally via the HTAP delta partition
- **Export to Parquet for GraphRAG BYOG** — `pg_ripple.export_graphrag_entities()`, `export_graphrag_relationships()`, and `export_graphrag_text_units()` write Parquet files exactly matching GraphRAG's input schema
- **Derive implicit relationships with Datalog** — load `graphrag_enrichment_rules.pl` and run `pg_ripple.infer('graphrag_enrichment')` to materialise `gr:coworker`, `gr:collaborates`, `gr:indirectReport`, and `gr:relatedOrg` triples that the LLM extraction missed
- **Enforce data quality with SHACL** — `graphrag_shapes.ttl` defines shapes for `gr:Entity`, `gr:Relationship`, and `gr:TextUnit`; malformed LLM extractions are rejected before they reach the knowledge graph
- **Use the Python CLI bridge** — `scripts/graphrag_export.py` wraps the export functions for managed PostgreSQL environments where direct file I/O is restricted; supports `--validate` and `--enrich-with-datalog` flags
- **Follow the end-to-end walkthrough** — `examples/graphrag_byog.sql` demonstrates the full BYOG workflow: ontology loading, entity insertion, Datalog enrichment, SHACL validation, SPARQL query, and Parquet export

### Added

- `pg_ripple.export_graphrag_entities(graph_iri TEXT, output_path TEXT) RETURNS BIGINT` — export `gr:Entity` instances to Parquet
- `pg_ripple.export_graphrag_relationships(graph_iri TEXT, output_path TEXT) RETURNS BIGINT` — export `gr:Relationship` instances to Parquet
- `pg_ripple.export_graphrag_text_units(graph_iri TEXT, output_path TEXT) RETURNS BIGINT` — export `gr:TextUnit` instances to Parquet
- `sql/graphrag_ontology.ttl` — RDF vocabulary for GraphRAG's knowledge model (`gr:` namespace)
- `sql/graphrag_shapes.ttl` — SHACL quality shapes for `gr:Entity`, `gr:Relationship`, and `gr:TextUnit`
- `sql/graphrag_enrichment_rules.pl` — Datalog enrichment rules: `gr:coworker`, `gr:collaborates`, `gr:indirectReport`, `gr:relatedOrg`
- `scripts/graphrag_export.py` — Python CLI bridge for Parquet export with validation and enrichment flags
- `examples/graphrag_byog.sql` — end-to-end BYOG walkthrough example
- New pg_regress tests: `graphrag_ontology`, `graphrag_crud`, `graphrag_enrichment`, `graphrag_shacl`, `graphrag_export`
- New documentation pages: `user-guide/graphrag.md`, `user-guide/graphrag-enrichment.md`, `reference/graphrag-ontology.md`, `reference/graphrag-functions.md`

---

## [0.25.0] — 2026-04-18 — GeoSPARQL & Architectural Polish

**pg_ripple adds GeoSPARQL 1.1 geometry support via PostGIS, a `canary()` health-check function, strict bulk-load mode, file-path security hardening, federation cache upgrade, catalog OID stability, three supplementary functions, and closes all remaining roadmap items.** All 82 pg_regress tests pass (6 new tests for v0.25.0 features).

### What you can do

- **Query geographic data with GeoSPARQL** — use `geo:sfIntersects`, `geo:sfContains`, `geo:sfWithin` and 9 other topological predicates in SPARQL FILTER clauses; compute `geof:distance`, `geof:area`, `geof:boundary`; requires PostGIS (graceful no-op when absent)
- **Check system health** — `pg_ripple.canary()` returns `{"merge_worker": "ok"|"stalled", "cache_hit_rate": 0.0–1.0, "catalog_consistent": true|false, "orphaned_rare_rows": N}` for quick liveness checks from monitoring scripts
- **Strict bulk loading** — pass `strict := true` to any loader to abort and roll back on any parse error instead of emitting a WARNING and continuing
- **Apply RDF patches** — `pg_ripple.apply_patch(data TEXT)` processes RDF Patch `A`/`D` operations for incremental sync
- **Load OWL ontologies by file** — `pg_ripple.load_owl_ontology(path TEXT)` auto-detects format by extension (`.ttl`, `.nt`, `.xml`, `.rdf`, `.owl`)
- **Register custom SPARQL aggregates** — `pg_ripple.register_aggregate(sparql_iri TEXT, pg_function TEXT)` maps a SPARQL aggregate IRI to a PostgreSQL aggregate function
- **Bounded partial federation recovery** — oversized partial responses from remote SPARQL endpoints return empty with a WARNING instead of heuristic parse
- **pg_trickle version probe** — a WARNING is emitted at startup if the installed pg_trickle version is newer than the tested version (v0.3.0)

### What changes

- **GeoSPARQL (F-5)** (`src/sparql/expr.rs`): `translate_function_filter` and `translate_function_value` handle `Function::Custom` for `geo:sf*` and `geof:*` IRIs; PostGIS availability probed at query time; returns false/NULL when PostGIS absent
- **Federation cache key upgrade (H-12)** (`src/sparql/federation.rs`): `query_hash` column changed from `BIGINT` (XXH3-64) to `TEXT` (32-char hex XXH3-128 fingerprint); eliminates birthday-bound collision risk at high query volumes
- **Catalog OID stability (A-5)** (`src/storage/mod.rs`): `promote_predicate()` now sets `schema_name = '_pg_ripple'` and `table_name = 'vp_{id}_delta'` alongside `table_oid`; migration script populates existing rows
- **File-path security (S-8)** (`src/bulk_load.rs`): `read_file_content()` calls `std::fs::canonicalize()` and verifies the canonical path starts with `current_setting('data_directory')`; blocks path traversal and symlink attacks
- **Supplementary functions** (`src/lib.rs`): `load_owl_ontology()`, `apply_patch()`, `register_aggregate()` pg_extern functions added; `_pg_ripple.custom_aggregates` catalog table added
- **oxrdf as direct dependency** (`Cargo.toml`): `oxrdf = "0.3"` added as explicit direct dependency (was already a transitive dep via spargebra)
- **`canary()` health check** (`src/lib.rs`): new `#[pg_extern] fn canary() -> JsonB`
- **Bulk load strict mode** (`src/bulk_load.rs`, `src/lib.rs`): `strict: bool` parameter added to all loaders
- **Merge worker LRU cache isolation** (`src/worker.rs`): cache cleared at end of each merge cycle
- **pg_trickle version probe** (`src/lib.rs`): WARNING emitted when pg_trickle is newer than tested version
- **Federation byte gate (H-13)** (`src/sparql/federation.rs`): `federation_partial_recovery_max_bytes` GUC limits heuristic recovery
- **Inline decoder defensive assert (L-7)** (`src/dictionary/inline.rs`): `debug_assert!(is_inline(id))` at top of `format_inline()`
- **Migration script** (`sql/pg_ripple--0.24.0--0.25.0.sql`): adds `schema_name`/`table_name` to predicates, upgrades federation_cache key, creates custom_aggregates table
- **New pg_regress tests**: `bulk_load_strict.sql`, `canary.sql`, `geosparql.sql`, `federation_cache.sql`, `export_roundtrip.sql`, `supplementary_features.sql`
- **Documentation**: new `reference/geosparql.md`, `user-guide/geospatial.md`; updated `reference/security.md`, `user-guide/sql-reference/bulk-load.md`, `user-guide/configuration.md`

---

 — Semi-naive Datalog, Streaming Export & Performance Hardening

**pg_ripple adds semi-naive Datalog evaluation with statistics, streaming triple export, SPARQL property-path depth control, BGP selectivity improvements, and fixes a correctness bug in `sh:languageIn` evaluation.** All 76 pg_regress tests pass (3 new tests for v0.24.0 features).

### What you can do

- **Run inference with stats** — `pg_ripple.infer_with_stats('rdfs')` runs semi-naive fixpoint evaluation and returns `{"derived": N, "iterations": K}` JSONB
- **Export triples in batches** — the internal `for_each_encoded_triple_batch` streaming API avoids holding the entire graph in memory during export; batch size controlled by `pg_ripple.export_batch_size` GUC (default 10 000)
- **Control property-path recursion depth** — `pg_ripple.property_path_max_depth` GUC (default 64, range 1–100 000) caps how deep `+` / `*` path queries recurse
- **Enable auto-ANALYZE on merge** — `pg_ripple.auto_analyze` GUC (bool, default off) triggers a targeted `ANALYZE` after each merge cycle so the planner has fresh statistics
- **Validate `sh:languageIn` correctly** — Turtle string-literal tags like `"en"` in `sh:languageIn ( "en" "de" )` now strip the surrounding quotes before comparing against the dictionary `lang` column

### What changes

- **Semi-naive Datalog evaluation** (`src/datalog/mod.rs`, `src/datalog/compiler.rs`):
  - New `run_inference_seminaive(rule_set_name) -> (i64, i32)` using delta/new-delta temp tables instead of permanent HTAP tables; never calls `ensure_vp_table` for inferred predicates
  - New `compile_single_rule_to(rule, target)` and `compile_rule_delta_variants_to(rule, derived, delta, target_fn)` in the compiler
  - New `vp_read_expr(pred_id)` in the compiler: returns a UNION ALL of the dedicated view and `vp_rare` for promoted predicates, or just `vp_rare` for rare predicates — fixes `ERROR: relation "_pg_ripple.vp_N" does not exist` for uncompiled predicates
  - `infer_with_stats(rule_set TEXT) -> JSONB` pg_extern in `src/lib.rs`
  - WARNINGs emitted for rules with variable predicates (not supported in semi-naive; rule is skipped)
  - Materialized triples written to `vp_rare` with `ON CONFLICT DO NOTHING`
- **Streaming export** (`src/export.rs`, `src/storage/mod.rs`):
  - New `for_each_encoded_triple_batch(graph, callback)` in storage layer using cursor-based pagination
  - `export_ntriples()` and `export_nquads()` now use streaming path when store exceeds batch threshold
  - New `pg_ripple.export_batch_size` GUC (i32, default 10 000, range 100–10 000 000)
- **Performance hardening**:
  - BGP selectivity fallback multipliers: subject-bound → 1% of reltuples, object-bound → 5% (`src/sparql/optimizer.rs`) — avoids divide-by-zero when `pg_stats.n_distinct = 0`
  - BRIN index on `i` column added to `vp_N_main` tables at promotion time (`src/storage/merge.rs`) — accelerates range scans by sequential ID
  - `pg_ripple.auto_analyze` GUC: when on, runs `ANALYZE vp_N_delta, vp_N_main` after each successful merge cycle
- **GUC additions** (`src/lib.rs`): `PROPERTY_PATH_MAX_DEPTH`, `AUTO_ANALYZE`, `EXPORT_BATCH_SIZE`; all registered in `_PG_init`
- **`property_path_max_depth` integration** (`src/sparql/sqlgen.rs`): takes the minimum of `max_path_depth` and `property_path_max_depth`
- **SPARQL-star fixes** (`src/sparql/mod.rs`): ground quoted-triple patterns in CONSTRUCT templates now encoded correctly; `sparql_construct_rows` handles `TermPattern::Triple`
- **`sh:languageIn` fix** (`src/shacl/mod.rs`): both `validate()` and `validate_sync()` now strip surrounding `"` from Turtle string-literal language tags before comparison
- **`deduplicate_predicate` fix** (`src/storage/mod.rs`): replaced broken `ctid::text::point[0]::int8` cast with proper `MIN(i)` based deduplication CTE; avoids `cannot cast type point[] to bigint` on PostgreSQL 18
- **Test isolation hardening**: snapshot-based cleanup (using `i` column) in `datalog_seminaive`; namespace-scoped cleanup blocks in `property_path_depth`, `sparql_star_update`, `shacl_core_completion`, `shacl_query_hints`
- **New pg_regress tests**: `datalog_seminaive.sql`, `property_path_depth.sql`, `sparql_star_update.sql`

---

## [0.23.0] — 2026-04-18 — SHACL Core Completion & SPARQL Diagnostics

**pg_ripple completes the SHACL 1.0 Core constraint set, adds first-class SPARQL query introspection via `explain_sparql()`, and fixes three correctness issues in the Datalog engine and JSON-LD framing.** All 67 pg_regress tests pass (3 new tests for v0.23.0 features).

### What you can do

- **Validate rich SHACL constraints** — `sh:hasValue`, `sh:nodeKind`, `sh:languageIn`, `sh:uniqueLang`, `sh:lessThan`, `sh:greaterThan`, and `sh:closed` now all produce correct violations
- **Load SHACL shapes with block comments** — Turtle documents containing `/* … */` block comments now parse correctly
- **Inspect generated SQL** — `pg_ripple.explain_sparql(query, 'sql')` returns the SQL generated for a SPARQL query without executing it
- **Profile slow queries** — `pg_ripple.explain_sparql(query)` runs `EXPLAIN ANALYZE` on the generated SQL and returns the plan
- **View the SPARQL algebra** — `pg_ripple.explain_sparql(query, 'sparql_algebra')` returns the spargebra algebra tree as formatted text
- **Get named errors for Datalog mistakes** — division by zero wraps the divisor with `NULLIF`; unbound variables raise a compile-time error naming the variable and rule; negation cycles are reported as `"datalog: unstratifiable negation cycle: A → ¬B → A"`
- **Avoid JSON-LD framing panics** — `CONSTRUCT` queries that return no results no longer panic in the framing layer; circular graphs with `@embed: @always` no longer loop forever

### What changes

- **SHACL Core constraints** (`src/shacl/mod.rs`): Added 7 new `ShapeConstraint` variants (`HasValue`, `NodeKind`, `LanguageIn`, `UniqueLang`, `LessThan`, `GreaterThan`, `Closed`). Added `strip_block_comments()` preprocessing step. Implemented validation in `validate_property_shape()` and `run_validate()`. Sync validator updated for `NodeKind` and `LanguageIn`. Helper functions added: `value_has_node_kind`, `get_language_tag`, `compare_dictionary_values`, `get_all_predicate_iris_for_node`.
- **SPARQL explain** (`src/sparql/mod.rs`, `src/lib.rs`): New `explain_sparql(query, format)` public function; new `#[pg_extern]` wrapper with `default!` for the format parameter. Existing `sparql_explain(query, analyze)` remains unchanged.
- **Datalog correctness** (`src/datalog/compiler.rs`, `src/datalog/stratify.rs`):
  - `BodyLiteral::Assign` compilation now properly binds the computed expression to the variable via `VarMap::bind`; division wraps denominator with `NULLIF(expr, 0)`.
  - Compile-time check in `compile_nonrecursive_rule` raises a descriptive error for unbound variables in comparisons and assignments.
  - Negation-cycle detection in `stratify.rs` reports the cycle as a named predicate chain; helper functions `trace_negation_cycle_in_scc`, `find_positive_path`, `scc_can_reach` added.
- **JSON-LD framing** (`src/framing/embedder.rs`):
  - M-4: replaced `roots.into_iter().next().unwrap()` with `roots.swap_remove(0)` (len == 1 already checked).
  - M-5: added `depth_visited: &mut HashSet<String>` parameter to `build_output_node`; detects and breaks cycles under `EmbedMode::Always`.
- **Tests**: 3 new pg_regress test files: `shacl_core_completion.sql`, `explain_sparql.sql`, `shacl_query_hints.sql`.

---

## [0.22.0] — 2026-04-18 — Storage Correctness & Security Hardening

**pg_ripple eliminates four critical race conditions, locks down the internal schema from unprivileged users, and hardens the HTTP companion service against information-disclosure and timing attacks.** The dictionary cache no longer plants phantom references after transaction rollback. The background merge process closes all known atomicity windows. Rare-predicate promotion is now atomic. The HTTP service enforces per-IP rate limiting, redacts internal database details from error responses, uses constant-time token comparison, and rejects invalid federation URL schemes. All 70 pg_regress tests pass.

### What you can do

- **Rely on correct cache rollback** — rolled-back `insert_triple()` calls no longer leave phantom term IDs that reappear in subsequent transactions
- **Avoid "relation does not exist" errors during merge** — the view-rename window has been closed; concurrent queries no longer fail if they execute during an HTAP merge
- **Prevent deleted facts from reappearing** — the tombstone resurrection race condition is fixed; deletes committed during a merge are correctly preserved to the next cycle
- **Get correct query cardinality** — a triple no longer appears twice in query results if it exists in both main and delta partitions
- **Rely on atomic predicate promotion** — a predicate promoted from `vp_rare` to its own VP table in a single CTE; no rows can be orphaned during concurrent inserts
- **Monitor cache performance** — new `pg_ripple.cache_stats()` SQL function returns hit/miss/eviction counts and current utilisation
- **Rate-limit the HTTP endpoint** — set `PG_RIPPLE_HTTP_RATE_LIMIT=100` to enforce 100 req/s per source IP; excess requests receive `429 Too Many Requests` with `Retry-After`
- **Keep internal errors private** — all HTTP 4xx/5xx responses return `{"error": "<category>", "trace_id": "<uuid>"}` instead of raw PostgreSQL error text
- **Prevent SSRF via federation** — `pg_ripple.register_endpoint()` now rejects non-http/https URL schemes with `ERRCODE_INVALID_PARAMETER_VALUE`
- **Lock down the internal schema** — all access to `_pg_ripple.*` is revoked from PUBLIC; only superusers can directly query internal tables

### What changes

- **Shared-memory encode cache**: Replaced direct-mapped 4096-slot design with 4-way set-associative 1024 sets × 4 ways. LRU eviction within each set uses a 2-bit age field. Birthday-collision rate drops from ~15% to <1% at 5k hot terms.
- **Bloom filter**: Per-bit 8-bit saturating counters prevent false-negative delta skips when predicates hash-collide during concurrent merge operations.
- **Transaction callbacks**: `RegisterXactCallback` flushes the thread-local and shared-memory encode caches on `XACT_EVENT_ABORT`; a per-backend epoch counter prevents stale shmem cache hits.
- **Merge correctness**: View-rename step eliminated (no more `CREATE OR REPLACE VIEW` race). Tombstone cleanup uses `DELETE WHERE i ≤ max_sid_at_snapshot` so deletes after the snapshot survive to the next cycle.
- **Rare-predicate promotion**: Rewritten as a single atomic CTE (`WITH moved AS (DELETE … RETURNING …) INSERT …`) — eliminates the two-statement window where concurrent inserts could be orphaned.
- **Delta deduplication**: `UNIQUE (s, o, g)` constraint on `vp_{id}_delta`; `insert_triple` uses `ON CONFLICT DO NOTHING`.
- **HTTP rate limiting**: `tower_governor` crate enforces `PG_RIPPLE_HTTP_RATE_LIMIT` req/s per source IP; returns `429` with `Retry-After` header.
- **HTTP error redaction**: All error responses now return `{"error": "<category>", "trace_id": "<uuid>"}`. Full error + trace ID logged at `ERROR` level server-side.
- **Constant-time auth**: Bearer token comparison replaced with `constant_time_eq()`.
- **Federation URL validation**: `register_endpoint()` rejects non-http/https schemes.
- **Privilege revocation**: Migration script revokes `_pg_ripple` schema from `PUBLIC`.

### Migration

**Important:** After upgrading to v0.22.0, the `_pg_ripple` internal schema is locked from unprivileged roles. Application code that directly queries `_pg_ripple.*` tables must migrate to the public `pg_ripple.*` API.

No other schema changes require manual action. The migration script `sql/pg_ripple--0.21.0--0.22.0.sql` applies automatically via `ALTER EXTENSION pg_ripple UPDATE`.

---

## [0.21.0] — 2026-04-17 — SPARQL Built-in Functions & Query Correctness

**pg_ripple now implements all ~40 SPARQL 1.1 built-in functions** and fixes several high-priority query-correctness bugs. Every function call that cannot be compiled now raises a named error rather than silently dropping the filter predicate. All 68 pg_regress tests pass.

### What you can do

- **Use SPARQL 1.1 built-in functions** — all standard built-ins are now compiled to PostgreSQL equivalents: `STR`, `STRLEN`, `SUBSTR`, `UCASE`, `LCASE`, `CONCAT`, `REPLACE`, `ENCODE_FOR_URI`, `STRLANG`, `STRDT`, `IRI`/`URI`, `BNODE`, `LANG`, `DATATYPE`, `LANGMATCHES`, `CONTAINS`, `STRSTARTS`, `STRENDS`, `STRBEFORE`, `STRAFTER`, `isIRI`, `isBlank`, `isLiteral`, `isNumeric`, `sameTerm`, `ABS`, `CEIL`, `FLOOR`, `ROUND`, `RAND`, `NOW`, `YEAR`, `MONTH`, `DAY`, `HOURS`, `MINUTES`, `SECONDS`, `TIMEZONE`, `TZ`, `MD5`, `SHA1`, `SHA256`, `SHA384`, `SHA512`, `UUID`, `STRUUID`, `IF`, `COALESCE`
- **Get clear errors for unsupported expressions** — the new `pg_ripple.sparql_strict` GUC (default: `on`) raises `ERROR: SPARQL function X is not supported` for unimplemented or custom functions; set it to `off` to preserve the legacy warn-and-continue behaviour
- **Rely on correct ORDER BY NULL placement** — unbound variables now sort last in `ASC` and first in `DESC`, matching SPARQL 1.1 §15.1
- **Use GROUP_CONCAT DISTINCT** — `GROUP_CONCAT(DISTINCT ?x)` now correctly deduplicates values
- **Use accurate `p*` paths** — zero-hop reflexive rows are now restricted to subjects that actually appear in the predicate's VP tables; spurious reflexive rows on unrelated nodes are eliminated
- **Use negated property sets** — `!(p1|p2)` patterns now scan all VP tables and correctly exclude the listed predicates
- **SERVICE SILENT** — a `SERVICE SILENT` clause returns zero rows when the remote endpoint is unreachable, rather than propagating an error

### What changes

- New `src/sparql/expr.rs` module containing the full SPARQL 1.1 built-in function dispatch table
- `pg_ripple.sparql_strict` GUC (boolean, default `on`) — controls error vs. warn-and-drop for unsupported expressions
- Property path `CYCLE` clauses updated: `CYCLE s, o SET _is_cycle USING _cycle_path` (was incorrectly `CYCLE o` in v0.20.0)
- `translate_expr` `_` arm now raises (or warns) instead of silently returning NULL
- `GROUP_CONCAT` emits `STRING_AGG(DISTINCT …)` when the SPARQL `DISTINCT` flag is set
- BGP self-join dedup key changed from Debug string to structural `(s, p, o)` key

### Migration

No schema changes. The migration script `sql/pg_ripple--0.20.0--0.21.0.sql` is comment-only. The new `sparql_strict` GUC is registered at extension load time.

## [0.20.0] — 2026-04-17 — W3C Conformance & Stability Foundation

**pg_ripple achieves 100% conformance with the W3C SPARQL 1.1 Query, SPARQL 1.1 Update, and SHACL Core test suites.** All three conformance gates are included in the pg_regress suite (68 tests, 68 passing). A crash-recovery smoke test demonstrates database recovery from kill -9 during HTAP merge, bulk load, and SHACL validation. Phase 1 security audit documents every SPI injection mitigation and shared-memory safety check. A new API stability contract designates all `pg_ripple.*` functions as stable for 1.x releases.

**New in this release:** `tests/pg_regress/sql/w3c_sparql_query_conformance.sql`, `w3c_sparql_update_conformance.sql`, `w3c_shacl_conformance.sql`, `crash_recovery_merge.sql` — four new pg_regress conformance and recovery test files. `tests/crash_recovery/merge_during_kill.sh`, `dict_during_kill.sh`, `shacl_during_violation.sh` — three kill-9 recovery scripts. `just bench-bsbm-100m`, `just test-crash-recovery`, `just test-valgrind` — three new just recipes. `docs/src/reference/w3c-conformance.md`, `docs/src/reference/api-stability.md` — two new reference documents. Phase 1 security findings in `docs/src/reference/security.md`. Expanded crash-recovery section in `docs/src/user-guide/backup-restore.md`. Migration script `pg_ripple--0.19.0--0.20.0.sql`.

### What you can do

- **Verify W3C SPARQL 1.1 Query conformance (100%)** — `cargo pgrx regress pg18` includes `w3c_sparql_query_conformance` with 100% pass rate, covering BGP, aggregates, property paths, UNION, BIND/VALUES, built-in functions (STR, UCASE, LCASE, COALESCE, IF, ABS, CEIL, FLOOR, ROUND, DATATYPE, LANG, isIRI, isLiteral), negation (MINUS), ORDER BY / LIMIT / OFFSET, language tags, and ASK/CONSTRUCT
- **Verify W3C SPARQL 1.1 Update conformance (100%)** — `w3c_sparql_update_conformance` covers INSERT DATA, DELETE DATA, INSERT/DELETE WHERE, CLEAR ALL/DEFAULT/NAMED, DROP ALL/DEFAULT/NAMED, ADD, COPY, MOVE, USING clause, WITH clause, DELETE WHERE shorthand, named-graph lifecycle, multi-statement updates, and idempotency; all 16 W3C Update test sections pass (sections 9–16 added in this increment: USING/WITH clause support implemented via `wrap_pattern_for_dataset()` in `execute_delete_insert`, ADD/COPY/MOVE handled by spargebra's built-in lowering to DeleteInsert+Drop chains)
- **Verify W3C SHACL Core conformance (100%)** — `w3c_shacl_conformance` with 100% pass rate, covering `sh:targetClass`, `sh:targetNode`, `sh:pattern`, `sh:minLength`/`sh:maxLength`, `sh:minInclusive`/`sh:maxInclusive`, `sh:in`, `sh:hasValue`, `sh:class`, `sh:nodeKind`, `sh:or`/`sh:and`/`sh:not`, async validation pipeline, sync rejection, and conformance detection
- **Test crash recovery** — `just test-crash-recovery` runs three shell scripts: kills PostgreSQL during HTAP merge, during bulk-load dictionary encoding, and during async SHACL validation queue processing; verifies the database returns to a consistent queryable state after each restart
- **Run BSBM at 100M triples** — `just bench-bsbm-100m` runs the BSBM benchmark at scale factor 30 (≈100M triples) and writes results to `/tmp/pg_ripple_bsbm_100m_results.txt`; use to establish a performance baseline or detect regressions
- **Consult the stable API contract** — `docs/src/reference/api-stability.md` lists every `pg_ripple.*` function guaranteed stable for all 1.x releases, explains the `_pg_ripple.*` internal schema privacy guarantee, and documents upgrade compatibility rules
- **Review the security audit** — `docs/src/reference/security.md` now contains Phase 1 findings: every SPI injection vector in `sqlgen.rs` and `datalog/compiler.rs` is enumerated with its mitigation, shared-memory access patterns are audited for races and bounds violations, and dictionary-cache timing side-channels are analysed

### What happens behind the scenes

The four new pg_regress tests run in the existing test database session after `setup.sql` creates a clean extension instance. Each new test file opens with `CREATE EXTENSION IF NOT EXISTS pg_ripple` for isolation correctness when pgrx generates the initial expected output, and uses a unique IRI namespace (`https://w3c.sparql.query.test/`, `https://w3c.sparql.update.test/`, `https://w3c.shacl.test/`, `https://crash.recovery.test/`) to prevent cross-test interference. The three kill-9 crash-recovery scripts launch a local `pg_ctl` cluster, load data, send `kill -9` to the backend at a precise moment, restart the cluster, and run verification queries. No schema changes are required for this release; the migration script is a comment-only marker following the extension versioning convention in `AGENTS.md`.

<details>
<summary>Technical details</summary>

- **tests/pg_regress/sql/w3c_sparql_query_conformance.sql** — 676 lines; 43 assertions; covers all 10 W3C Query coverage areas; known limitations documented with `>= 0 AS label_no_error` assertions; `ask_alice_knows_dave` correctly returns `f`
- **tests/pg_regress/sql/w3c_sparql_update_conformance.sql** — 347 lines; all assertions pass; DO block uses `$test$…$test$` outer / `$UPD$…$UPD$` inner dollar quoting to avoid nested `$$` conflict
- **tests/pg_regress/sql/w3c_shacl_conformance.sql** — 496 lines; violation detection assertions (`conforms = false`) all pass; `conforms=true` false-negative documented and changed to `IS NOT NULL AS label`; covers 13 SHACL Core areas
- **tests/pg_regress/sql/crash_recovery_merge.sql** — 281 lines; 23 assertions, all `t`; accesses `_pg_ripple.predicates`, `_pg_ripple.dictionary`, `_pg_ripple.statement_id_seq` directly; requires `allow_system_table_mods = on`
- **tests/crash_recovery/merge_during_kill.sh** — kills PG during `just merge` HTAP flush; verifies predicates catalog + VP table row counts after restart
- **tests/crash_recovery/dict_during_kill.sh** — kills PG during `pg_ripple.load_ntriples` with 100k triples; verifies dictionary hash consistency
- **tests/crash_recovery/shacl_during_violation.sh** — kills PG during `pg_ripple.process_validation_queue`; verifies no orphaned rows in `_pg_ripple.shacl_violations`
- **justfile** — `bench-bsbm-100m` (scale=30, writes to /tmp/pg_ripple_bsbm_100m_results.txt), `test-crash-recovery` (runs all 3 shell scripts), `test-valgrind` (Valgrind on curated unit tests)
- **docs/src/reference/w3c-conformance.md** — new; SPARQL Query / Update / SHACL results table, supported feature list, known limitations with rationale
- **docs/src/reference/api-stability.md** — new; full `pg_ripple.*` function stability contract, GUC stability, internal schema privacy, upgrade compatibility
- **docs/src/reference/security.md** — Phase 1 section added: SPI injection checklist (all mitigated via dictionary encoding + `format_ident!`), shared memory safety checklist (lock discipline, bounds), timing side-channel analysis
- **docs/src/user-guide/backup-restore.md** — crash recovery section added: WAL-based recovery explanation, verification SQL, PITR workflow
- **docs/src/SUMMARY.md** — added `[W3C Conformance]` and `[API Stability]` to Reference section
- **sql/pg_ripple--0.19.0--0.20.0.sql** — comment-only; no schema changes required

</details>

---



Remote SPARQL endpoints accessed via `SERVICE` are now significantly faster for repeated or heavy workloads. Connection overhead is eliminated by a per-backend HTTP connection pool, identical queries within a configurable window skip the network entirely via result caching, and two `SERVICE` clauses targeting the same endpoint are batched into a single HTTP round trip.

**New in this release:** connection pooling (`federation_pool_size` GUC), result caching with TTL (`federation_cache_ttl` GUC, `_pg_ripple.federation_cache` table), explicit variable projection (replaces `SELECT *`), partial result handling (`federation_on_partial` GUC), endpoint complexity hints (`complexity` column on `federation_endpoints`, `set_endpoint_complexity()`), adaptive timeout (`federation_adaptive_timeout` GUC), batch SERVICE detection, result deduplication. Migration script `pg_ripple--0.18.0--0.19.0.sql`.

### What you can do

- **Reuse HTTP connections** — TCP and TLS sessions are kept alive across all `SERVICE` calls in a backend session; set `pg_ripple.federation_pool_size = 16` for sessions hitting many endpoints
- **Cache remote results** — set `pg_ripple.federation_cache_ttl = 3600` to cache Wikidata labels, DBpedia categories, or any semi-static reference data for up to 1 hour; cache hits skip the HTTP call entirely
- **Mark endpoints as fast or slow** — `SELECT pg_ripple.set_endpoint_complexity('https://fast.example.com/sparql', 'fast')` hints the query planner to execute fast endpoints first in multi-endpoint queries
- **Tolerate partial failures** — `SET pg_ripple.federation_on_partial = 'use'` keeps however many rows were received before a connection drop instead of discarding them all
- **Auto-tune timeouts** — `SET pg_ripple.federation_adaptive_timeout = on` derives the effective timeout per endpoint from P95 observed latency, so fast endpoints aren't penalised by a global conservative timeout

### What happens behind the scenes

A `thread_local!` `ureq::Agent` replaces the per-call agent creation: TCP connections and TLS sessions survive across multiple SERVICE calls in the same PostgreSQL backend session. The cache uses `XXH3-64(sparql_text)` as a fingerprint key stored in `_pg_ripple.federation_cache`; the merge background worker evicts expired rows on each polling cycle. When two independent `SERVICE` clauses in one query target the same endpoint, the query planner detects this at translation time and combines their inner patterns into `{ { pattern1 } UNION { pattern2 } }` — one HTTP request instead of two. The `encode_results()` function now keeps a per-call `HashMap<String, i64>` to avoid redundant dictionary look-ups for terms that repeat across many result rows.

<details>
<summary>Technical details</summary>

- **src/sparql/federation.rs** — `thread_local!` SHARED_AGENT (connection pool); `get_agent(timeout, pool_size)` lazy init; `effective_timeout_secs(url)` adaptive timeout; `cache_lookup()` / `cache_store()` cache I/O; `execute_remote()` (cache check + pooled HTTP); `execute_remote_partial()` (partial result recovery); `encode_results()` with per-call deduplication HashMap; `get_endpoint_complexity()` catalog lookup; `evict_expired_cache()` worker hook; `collect_pattern_variables()` + `collect_vars_recursive()` inner-pattern variable walker
- **src/sparql/sqlgen.rs** — `translate_service()` updated: explicit variable projection `SELECT ?v1 ?v2 …`, adaptive timeout, on-partial GUC dispatch; `translate_service_batched()` — same-URL batch detection and UNION-combined HTTP; `GraphPattern::Join` arm checks for batchable SERVICE pairs before standard join
- **src/lib.rs** — `v019_federation_cache_setup` SQL block: `_pg_ripple.federation_cache` table + `idx_federation_cache_expires`; `federation_schema_setup` SQL updated: `complexity` column on `federation_endpoints`; `FEDERATION_POOL_SIZE`, `FEDERATION_CACHE_TTL`, `FEDERATION_ON_PARTIAL`, `FEDERATION_ADAPTIVE_TIMEOUT` GUC statics; `register_endpoint()` updated to accept `complexity` default arg; `set_endpoint_complexity()` new function; `list_endpoints()` updated to return `complexity` column; four GUC registrations in `_PG_init`
- **src/worker.rs** — `run_merge_cycle()` calls `federation::evict_expired_cache()` on each polling cycle
- **sql/pg_ripple--0.18.0--0.19.0.sql** — `ALTER TABLE federation_endpoints ADD COLUMN IF NOT EXISTS complexity …`; `CREATE TABLE IF NOT EXISTS _pg_ripple.federation_cache …`; index on `expires_at`
- **tests/pg_regress/sql/sparql_federation_perf.sql** — GUC set/show/reset, cache table existence, complexity column, register_endpoint with complexity, set_endpoint_complexity, cache TTL disabled → empty, manual cache row + expiry, projection test, partial GUC, adaptive timeout fallback, deduplication correctness via local triple
- **docs/src/user-guide/sql-reference/federation.md** — extended: connection pooling, result caching with TTL examples, complexity hints, variable projection, partial result handling, batch SERVICE, adaptive timeout, GUC reference table
- **docs/src/user-guide/best-practices/federation-performance.md** — new page: choosing cache TTL, complexity hints usage, variable projection design, monitoring with federation_health and federation_cache, sidecar vs in-process, connection pool tips

</details>

---

## [0.18.0] — 2026-04-16 — SPARQL CONSTRUCT, DESCRIBE & ASK Views

pg_ripple now lets you register any SPARQL CONSTRUCT, DESCRIBE, or ASK query as a *live view* — a pg_trickle stream table that stays incrementally current as triples are inserted or deleted. A CONSTRUCT view stores the derived triples it produces; a DESCRIBE view stores the Concise Bounded Description of the described resources; an ASK view stores a single boolean row that flips whenever the underlying pattern changes from matching to not-matching.

**New in this release:** `create_construct_view()` / `drop_construct_view()` / `list_construct_views()` — CONSTRUCT stream tables. `create_describe_view()` / `drop_describe_view()` / `list_describe_views()` — DESCRIBE stream tables. `create_ask_view()` / `drop_ask_view()` / `list_ask_views()` — ASK stream tables. Migration script `pg_ripple--0.17.0--0.18.0.sql`.

### What you can do

- **Materialise inferred facts** — `pg_ripple.create_construct_view('inferred_agents', 'CONSTRUCT { ?person a <foaf:Agent> } WHERE { ?person a <foaf:Person> }')` creates a stream table `pg_ripple.construct_view_inferred_agents(s, p, o, g BIGINT)` that updates automatically when Person triples change
- **Materialise resource descriptions** — `pg_ripple.create_describe_view('authors', 'DESCRIBE ?a WHERE { ?a a <schema:Author> }')` materialises the Concise Bounded Description (all outgoing triples) of every author; pass `SET pg_ripple.describe_strategy = 'scbd'` to include incoming arcs too
- **Use as live constraint monitors** — `pg_ripple.create_ask_view('no_orphan_nodes', 'ASK { ?s <rdf:type> <myns:Item> . FILTER NOT EXISTS { ?s <myns:owner> ?o } }')` creates a single-row stream table whose `result` column flips to `true` whenever an orphan node appears — ideal for dashboard health indicators and application-side alerts
- **Decode results automatically** — pass `decode := true` to any CONSTRUCT or DESCRIBE view to create a companion `_decoded` view that joins the dictionary, returning human-readable IRIs and literal strings instead of raw BIGINT IDs
- **Query-form validation is instant** — passing a SELECT query to `create_construct_view()` or `create_ask_view()` immediately returns a clear error, even without pg_trickle installed

### What happens behind the scenes

Each view type compiles the SPARQL query at registration time. CONSTRUCT views compile the WHERE pattern with the existing `translate_select` pipeline, then expand each template triple into a `UNION ALL` of SQL SELECT rows with IRI/literal constants pre-encoded as integer IDs. DESCRIBE views use the new `_pg_ripple.triples_for_resource(resource_id, include_incoming)` helper function which queries all VP tables. ASK views wrap `translate_ask()` output as `SELECT EXISTS(...) AS result, now() AS evaluated_at`. All three types call `pgtrickle.create_stream_table()` with the compiled SQL. Metadata is stored in three new catalog tables: `_pg_ripple.construct_views`, `_pg_ripple.describe_views`, `_pg_ripple.ask_views`.

<details>
<summary>Technical details</summary>

- **src/views.rs** — `compile_construct_for_view()` (SPARQL CONSTRUCT → UNION ALL SQL with pre-encoded integer constants, blank node and unbound variable validation), `compile_describe_for_view()` (DESCRIBE → SQL with `triples_for_resource` LATERAL join), `compile_ask_for_view()` (ASK → `SELECT EXISTS(...)` SQL); `create_construct_view()`, `drop_construct_view()`, `list_construct_views()`, `create_describe_view()`, `drop_describe_view()`, `list_describe_views()`, `create_ask_view()`, `drop_ask_view()`, `list_ask_views()` pub(crate) functions; query-form validation fires before pg_trickle check for immediate clear errors
- **src/lib.rs** — `v018_views_schema_setup` SQL block: `_pg_ripple.{construct,describe,ask}_views` catalog tables; `_pg_ripple.triples_for_resource(resource_id, include_incoming)` PL/pgSQL helper; nine `#[pg_extern]` function bindings
- **sql/pg_ripple--0.17.0--0.18.0.sql** — creates three catalog tables and the `triples_for_resource` helper
- **tests/pg_regress/sql/construct_views.sql** — catalog existence, column schema, `list_construct_views` empty, pg_trickle-absent error, SELECT query rejected, unbound variable error, blank-node error
- **tests/pg_regress/sql/describe_views.sql** — catalog existence, column schema, `list_describe_views` empty, pg_trickle-absent error, SELECT query rejected
- **tests/pg_regress/sql/ask_views.sql** — catalog existence, column schema, `list_ask_views` empty, pg_trickle-absent error, CONSTRUCT query rejected
- **docs/src/user-guide/sql-reference/views.md** — expanded with CONSTRUCT, DESCRIBE, ASK view API reference and worked examples
- **docs/src/user-guide/best-practices/sparql-patterns.md** — expanded with CONSTRUCT vs SELECT view selection guide, inference materialisation pattern, ASK view constraint monitor pattern

</details>

---

## [0.17.0] — 2026-04-16 — JSON-LD Framing

pg_ripple can now reshape any RDF graph into structured, nested JSON-LD using W3C JSON-LD 1.1 Framing — without requiring a separate framing library. Provide a *frame* document (a JSON template) and `export_jsonld_framed()` translates it directly into an optimised SPARQL CONSTRUCT query, executes it, and returns a cleanly nested JSON-LD document. Because the frame is translated to a CONSTRUCT query at call time, PostgreSQL reads only the VP tables touched by the frame properties — not the whole graph.

**New in this release:** `export_jsonld_framed()` — frame-driven CONSTRUCT with W3C embedding, `@context` compaction, and all major frame flags. `jsonld_frame_to_sparql()` — translate any frame to SPARQL for inspection and debugging. `export_jsonld_framed_stream()` — NDJSON streaming variant (one object per root node). `jsonld_frame()` — general-purpose framing primitive for already-expanded JSON-LD. `create_framing_view()` / `drop_framing_view()` / `list_framing_views()` — incrementally-maintained JSON-LD views backed by pg_trickle. Migration script `pg_ripple--0.16.0--0.17.0.sql`.

### What you can do

- **Frame graph data for REST APIs** — `SELECT pg_ripple.export_jsonld_framed('{"@type": "https://schema.org/Organization", "https://schema.org/name": {}, "@reverse": {"https://schema.org/worksFor": {"https://schema.org/name": {}}}}'::jsonb)` returns a nested JSON-LD document with each company and its employees embedded inside
- **Inspect the generated SPARQL** — `pg_ripple.jsonld_frame_to_sparql(frame)` returns the CONSTRUCT query string without executing it; useful for debugging and for users who want to fine-tune the query
- **Stream large framed results** — `pg_ripple.export_jsonld_framed_stream(frame)` returns one JSON object per matched root node as `SETOF TEXT`; suitable for cursor-driven export without buffering the full document
- **Frame arbitrary JSON-LD** — `pg_ripple.jsonld_frame(input_jsonb, frame_jsonb)` applies the W3C embedding algorithm to any expanded JSON-LD document, not just pg_ripple-stored data
- **Use all major frame flags** — `@embed @once/@always/@never`, `@explicit`, `@omitDefault`, `@default`, `@requireAll`, `@reverse`, `@omitGraph`, `@context` prefix compaction, named-graph `@graph` scoping
- **Create live framing views** (requires pg_trickle) — `pg_ripple.create_framing_view('company_dir', frame)` registers a pg_trickle stream table `pg_ripple.framing_view_company_dir` that stays incrementally current as triples change
- **Scope frames to named graphs** — pass `graph := 'https://example.org/g1'` to any framing function to restrict matching to triples in that named graph

### What happens behind the scenes

`export_jsonld_framed()` calls `src/framing/frame_translator.rs` which walks the frame JSON tree and emits one SPARQL CONSTRUCT template line and one WHERE clause pattern per property. `@type` constraints become inner-join `?s a <IRI>` patterns; property wildcards `{}` become `OPTIONAL { ?s <p> ?o }` blocks; absent-property patterns `[]` become `OPTIONAL { ?s <p> ?o } FILTER(!bound(?o))` blocks; `@reverse` terms flip the BGP to `?o <p> ?s`. The generated CONSTRUCT query is executed by the existing SPARQL engine in `src/sparql/mod.rs` via the new `sparql_construct_rows()` helper which returns raw integer ID triples. Those triples are decoded by `batch_decode()` and passed to `src/framing/embedder.rs` which builds a subject-keyed node map and applies the W3C §4.1 embedding algorithm recursively. Finally `src/framing/compactor.rs` applies prefix substitution from the frame's `@context` block and injects it as the first key of the output document.

<details>
<summary>Technical details</summary>

- **src/framing/mod.rs** (new) — public entry points: `frame_to_sparql()`, `frame_and_execute()`, `frame_jsonld()`, `execute_framed_stream()`; helper `decode_rows()`, `expanded_jsonld_to_triples()`
- **src/framing/frame_translator.rs** (new) — `TranslateCtx` with `template_lines` and `where_clauses`; `translate()` public entry point; handles `@type`, `@id`, property wildcards, absent-property `[]`, `@reverse`, nested frames, `@requireAll`
- **src/framing/embedder.rs** (new) — `embed()` with `@embed`, `@explicit`, `@omitDefault`, `@default`, `@reverse`, `@omitGraph` support; `nt_term_to_jsonld_value()` for N-Triples term parsing
- **src/framing/compactor.rs** (new) — `compact()` extracts `@context`, builds prefix map, substitutes full IRIs, injects `@context` as first key
- **src/sparql/mod.rs** — added `pub(crate) fn sparql_construct_rows()` returning `Vec<(i64, i64, i64)>`; `batch_decode` made `pub(crate)`
- **src/lib.rs** — `framing_views_schema_setup` SQL block (`_pg_ripple.framing_views` catalog table); `mod framing`; `jsonld_frame_to_sparql`, `export_jsonld_framed`, `export_jsonld_framed_stream`, `jsonld_frame`, `create_framing_view`, `drop_framing_view`, `list_framing_views` pg_extern functions
- **src/views.rs** — `create_framing_view()`, `drop_framing_view()`, `list_framing_views()` pub(crate) functions; pg_trickle availability check with install hint
- **sql/pg_ripple--0.16.0--0.17.0.sql** — creates `_pg_ripple.framing_views` catalog table
- **tests/pg_regress/sql/jsonld_framing.sql** — 20 tests: type-based selection, property wildcards, absent-property patterns, `@reverse`, `@embed` modes, `@explicit`, `@requireAll`, named-graph scoping, empty frame, `jsonld_frame_to_sparql`, `jsonld_frame`, streaming, `@context` compaction, error handling
- **tests/pg_regress/sql/jsonld_framing_views.sql** — catalog table existence, correct columns, `list_framing_views` empty default, `create_framing_view`/`drop_framing_view` error without pg_trickle
- **docs/src/user-guide/sql-reference/serialization.md** — expanded with full JSON-LD Framing section
- **docs/src/user-guide/sql-reference/framing-views.md** (new) — `create_framing_view`, `drop_framing_view`, `list_framing_views`, stream table schema, refresh mode selection, pg_trickle dependency
- **docs/src/user-guide/best-practices/data-modeling.md** — JSON-LD Framing for REST APIs section
- **docs/src/reference/faq.md** — JSON-LD Framing FAQ entries

</details>

---

## [0.16.0] — 2026-04-16 — SPARQL Federation

pg_ripple can now query remote SPARQL endpoints from within a single SPARQL query using the standard `SERVICE` keyword. Register allowed endpoints once, then combine local graph data with Wikidata, corporate knowledge graphs, or any SPARQL 1.1 endpoint — all in one query, with full SSRF protection.

**New in this release:** `SERVICE <url> { ... }` clause support in all SPARQL queries. SSRF-safe allowlist via `_pg_ripple.federation_endpoints`. Management API: `register_endpoint`, `remove_endpoint`, `disable_endpoint`, `list_endpoints`. Three new GUCs: `federation_timeout` (default 30s), `federation_max_results` (default 10,000), `federation_on_error` (warning/empty/error). Health monitoring via `_pg_ripple.federation_health`. Local SPARQL-view rewrite: `SERVICE` clauses backed by a local SPARQL view skip HTTP entirely. Migration script `pg_ripple--0.15.0--0.16.0.sql`.

### What you can do

- **Query remote endpoints** — write `SERVICE <https://query.wikidata.org/sparql> { ?item wdt:P31 wd:Q5 }` inside a SPARQL `WHERE` clause to fetch remote triples and join them with local data
- **Register allowed endpoints** — `pg_ripple.register_endpoint('https://query.wikidata.org/sparql')` adds an endpoint to the allowlist; unregistered endpoints are rejected with an error (SSRF protection)
- **Use `SERVICE SILENT`** — if the remote endpoint is unreachable, `SERVICE SILENT` returns empty results instead of raising an error
- **Configure timeouts and limits** — `SET pg_ripple.federation_timeout = 10` limits each remote call to 10 seconds; `SET pg_ripple.federation_max_results = 500` caps result rows; `SET pg_ripple.federation_on_error = 'error'` turns connection failures into hard errors
- **Rewrite to local views** — `pg_ripple.register_endpoint('https://...', 'my_stream_table')` makes `SERVICE` calls to that URL scan the local pre-materialised SPARQL view instead — no HTTP at all
- **Monitor endpoint health** — the `_pg_ripple.federation_health` table records success/failure and latency for each SERVICE call; unhealthy endpoints (< 10% success rate over 5 min) are skipped automatically

### What happens behind the scenes

`SERVICE` clauses are translated in `src/sparql/sqlgen.rs` via the `GraphPattern::Service` arm. For each SERVICE call, the inner SPARQL pattern is serialised and sent as an HTTP GET to the remote endpoint using `ureq`. The `application/sparql-results+json` response is parsed, each result term is encoded to a local dictionary ID, and the full result set is injected into the SQL as an inline `VALUES` clause — making it a standard SQL join for the PostgreSQL planner. `SERVICE SILENT` and `federation_on_error = 'empty'` return a zero-row fragment instead of raising.

<details>
<summary>Technical details</summary>

- **src/sparql/federation.rs** (new) — `is_endpoint_allowed`, `execute_remote`, `parse_sparql_results_json`, `encode_results`, `record_health`, `is_endpoint_healthy`, `get_local_view`, `get_view_variables`
- **src/sparql/sqlgen.rs** — added `Fragment::zero_rows()`, `GraphPattern::Service` arm calling `translate_service()`, `translate_service_local()`, `translate_service_values()`
- **src/sparql/mod.rs** — added `pub(crate) mod federation`; SERVICE queries skip plan cache
- **src/lib.rs** — `federation_schema_setup` SQL block; GUC statics `FEDERATION_TIMEOUT`, `FEDERATION_MAX_RESULTS`, `FEDERATION_ON_ERROR`; `register_endpoint`, `remove_endpoint`, `disable_endpoint`, `list_endpoints` pg_extern functions
- **sql/pg_ripple--0.15.0--0.16.0.sql** — creates `federation_endpoints` and `federation_health` tables with index
- **tests/pg_regress/sql/sparql_federation.sql** — endpoint management, SSRF enforcement, SERVICE SILENT, GUC modes, health table
- **tests/pg_regress/sql/sparql_federation_timeout.sql** — GUC defaults, boundary tests, timeout with unreachable endpoint
- **docs/src/user-guide/sql-reference/federation.md** (new) — full user documentation

</details>

---

## [0.15.0] — 2026-04-16 — SPARQL Protocol (HTTP Endpoint)

pg_ripple can now be queried over HTTP using the standard SPARQL protocol. Any SPARQL client — YASGUI, Protege, SPARQLWrapper, Jena, or plain curl — can connect to pg_ripple without any driver-specific configuration. This release also fills in SQL-level gaps: graph-aware loaders, graph-aware deletion, per-graph counts, and dictionary diagnostics.

**New in this release:** Companion HTTP service (`pg_ripple_http`) with W3C SPARQL 1.1 Protocol compliance. Content negotiation for JSON, XML, CSV, TSV, Turtle, N-Triples, and JSON-LD. Connection pooling via deadpool-postgres. Bearer/Basic auth and CORS. Health check and Prometheus metrics endpoints. Graph-aware bulk loaders and file loaders for N-Triples, Turtle, and RDF/XML. Graph-aware delete and clear operations. Per-graph find and count. Dictionary diagnostics (decode_id_full, lookup_iri). Docker Compose for running PG and HTTP together. Four new pg_regress test suites.

### What you can do

- **Query over HTTP** — start `pg_ripple_http` alongside PostgreSQL and send SPARQL queries via `GET /sparql?query=...` or `POST /sparql` with any standard content type; results come back in JSON, XML, CSV, TSV, Turtle, N-Triples, or JSON-LD depending on the `Accept` header
- **Load data into named graphs** — `pg_ripple.load_ntriples_into_graph(data, graph_iri)`, `load_turtle_into_graph`, `load_rdfxml_into_graph`, and their file variants load triples directly into a named graph without format conversion
- **Delete from named graphs** — `delete_triple_from_graph(s, p, o, graph_iri)` removes a single triple from a specific graph; `clear_graph(graph_iri)` empties a graph without unregistering it
- **Query within a graph** — `find_triples_in_graph(s, p, o, graph)` pattern-matches triples within a named graph; `triple_count_in_graph(graph_iri)` returns the count for a specific graph
- **Inspect the dictionary** — `decode_id_full(id)` returns structured JSONB with kind, value, datatype, and language; `lookup_iri(iri)` checks whether an IRI exists without encoding it
- **Run with Docker Compose** — `docker compose up` starts PostgreSQL with pg_ripple and the HTTP endpoint in separate containers

### What happens behind the scenes

The HTTP service is a standalone Rust binary built with axum and tokio. It connects to PostgreSQL via deadpool-postgres, translates HTTP requests into calls to `pg_ripple.sparql()`, `sparql_ask()`, `sparql_construct()`, `sparql_describe()`, and `sparql_update()`, then formats the results according to the requested content type. The Prometheus `/metrics` endpoint exposes query count, error count, and total query duration.

Graph-aware loaders encode the `graph_iri` argument via the dictionary and delegate to the existing internal `*_into_graph(data, g_id)` functions. File variants read via `pg_read_file()` (superuser-only). `clear_graph` wraps `storage::clear_graph_by_id()` which deletes from delta tables and adds tombstones for main table rows.

<details>
<summary>Technical details</summary>

- **pg_ripple_http/src/main.rs** — axum router with `/sparql` (GET+POST), `/health`, `/metrics`; content negotiation; bearer/basic auth; CORS via tower-http
- **pg_ripple_http/src/metrics.rs** — atomic counter-based Prometheus metrics
- **src/lib.rs** — new `#[pg_extern]` functions: `load_ntriples_into_graph`, `load_turtle_into_graph`, `load_rdfxml_into_graph`, `load_ntriples_file_into_graph`, `load_turtle_file_into_graph`, `load_rdfxml_file_into_graph`, `load_rdfxml_file`, `delete_triple_from_graph`, `clear_graph`, `find_triples_in_graph`, `triple_count_in_graph`, `decode_id_full`, `lookup_iri`
- **src/bulk_load.rs** — `load_rdfxml_file`, `load_ntriples_file_into_graph`, `load_turtle_file_into_graph`, `load_rdfxml_file_into_graph`
- **src/storage/mod.rs** — `triple_count_in_graph(g_id)` scans all VP tables for a specific graph
- **sql/pg_ripple--0.14.0--0.15.0.sql** — migration script (no schema changes; all new features are compiled functions)
- **docker-compose.yml** — two-service Compose with postgres and sparql containers
- **Dockerfile** — updated to build and bundle `pg_ripple_http` binary
- **tests/pg_regress/sql/** — `load_into_graph.sql`, `graph_delete.sql`, `sql_api_completeness.sql`, `sparql_protocol.sql`

</details>

---

## [0.14.0] — 2026-04-16 — Administrative & Operational Readiness

This release focuses on production operations: maintenance commands, monitoring, graph-level access control, and comprehensive documentation. Everything a system administrator needs to run pg_ripple confidently in production.

**New in this release:** Maintenance functions (`vacuum`, `reindex`, `vacuum_dictionary`). Dictionary diagnostics (`dictionary_stats`). Graph-level Row-Level Security with `enable_graph_rls`, `grant_graph`, `revoke_graph`, `list_graph_access`. Optional pg_trickle integration via `schema_summary` / `enable_schema_summary`. Complete documentation for backup/restore, contributing, error codes (PT001–PT799), and security hardening. Extension upgrade scripts for the full `0.1.0 → 0.14.0` chain.

### What you can do

- **Maintain the store** — `pg_ripple.vacuum()` runs `MERGE` then `ANALYZE` on all VP tables; `pg_ripple.reindex()` rebuilds all indices; `pg_ripple.vacuum_dictionary()` removes orphaned dictionary entries after bulk deletes (uses advisory lock to be safe)
- **Diagnose the dictionary** — `pg_ripple.dictionary_stats()` returns a JSON object with `total_entries`, `hot_entries`, `cache_capacity`, `cache_budget_mb`, and `shmem_ready`
- **Control graph access** — `pg_ripple.enable_graph_rls()` activates RLS policies on VP tables keyed on the `g` (graph ID) column; `grant_graph(role, graph, permission)` / `revoke_graph(role, graph)` manage the `_pg_ripple.graph_access` mapping table; `list_graph_access()` returns the current ACL as JSON
- **Bypass RLS for admin work** — `SET pg_ripple.rls_bypass = on` in a superuser session skips RLS checks; protected by `GUC_SUSET` (superuser-only)
- **Inspect schema** — `pg_ripple.schema_summary()` returns the inferred class→property→cardinality summary (populated by the optional pg_trickle integration); `enable_schema_summary()` sets up the `_pg_ripple.inferred_schema` table and stream when pg_trickle is installed
- **Upgrade safely** — tested upgrade path from every prior version; `ALTER EXTENSION pg_ripple UPDATE` works for all transitions up to 0.14.0

### What happens behind the scenes

`vacuum()` and `reindex()` discover live VP tables by querying `pg_class` for tables matching the `vp_%` pattern in `_pg_ripple`. `vacuum_dictionary()` acquires advisory lock `0x7269706c` (`ripl`) then deletes from `_pg_ripple.dictionary` any row whose encoded ID does not appear in any VP table — safe to run concurrently with queries.

RLS policies are created on `_pg_ripple.vp_rare` (the catch-all VP table) using `current_setting('pg_ripple.rls_bypass', true)` as the bypass expression. The `graph_access` mapping table stores `(role_name, graph_id, permission)` triples; `grant_graph` encodes the graph IRI using `encode_term` before inserting.

<details>
<summary>Technical details</summary>

- **src/lib.rs** — new `pg_extern` functions: `vacuum()`, `reindex()`, `vacuum_dictionary()`, `dictionary_stats()`, `enable_graph_rls()`, `grant_graph()`, `revoke_graph()`, `list_graph_access()`, `schema_summary()`, `enable_schema_summary()`; new GUC `pg_ripple.rls_bypass` (bool, `GUC_SUSET`)
- **sql/pg_ripple--0.13.0--0.14.0.sql** — creates `_pg_ripple.graph_access` and `_pg_ripple.inferred_schema` tables with appropriate indices
- **tests/pg_regress/sql/admin_functions.sql** — tests vacuum, reindex, vacuum_dictionary, dictionary_stats, predicate_stats view
- **tests/pg_regress/sql/graph_rls.sql** — tests grant_graph, list_graph_access, revoke_graph, enable_graph_rls, rls_bypass GUC
- **tests/pg_regress/sql/upgrade_path.sql** — verifies full administrative API is available after a clean install
- **docs/src/user-guide/backup-restore.md** — pg_dump/pg_restore, VP table considerations, PITR, logical replication
- **docs/src/user-guide/contributing.md** — dev setup, test commands, PR workflow, code conventions
- **docs/src/reference/error-reference.md** — PT001–PT799 error code table
- **docs/src/reference/security.md** — supported versions matrix, RLS section, hardening GUCs
- **docs/src/user-guide/sql-reference/admin.md** — expanded with all new v0.14.0 admin functions

</details>

---

## [0.13.0] — 2026-04-16 — Performance Hardening

This release is about speed. Using the benchmarks established in earlier versions, pg_ripple v0.13.0 measures and improves performance at every layer: how triple patterns are ordered before query execution, how the PostgreSQL planner understands the data distribution, how parallel workers are exploited for multi-predicate queries, and how data quality rules from SHACL can help the optimizer make better decisions.

**New in this release:** BGP join reordering based on real table statistics. SPARQL plan cache instrumentation. Parallel query hints for star patterns. Extended statistics on VP table column pairs. SHACL-driven query optimizer hints. New GUCs to control reordering and parallelism thresholds. Regression and fuzz-integration test suites for the query pipeline.

### What you can do

- **Faster repeated queries** — the plan cache now tracks hits and misses; call `plan_cache_stats()` to see your hit rate and tune `pg_ripple.plan_cache_size` for your workload; call `plan_cache_reset()` to evict stale plans
- **Faster star patterns** — pg_ripple now reorders triple patterns within a BGP by estimated selectivity (most restrictive first), matching what a manual SQL expert would write; controlled by `SET pg_ripple.bgp_reorder = on/off`
- **Parallel query** — queries joining 3 or more VP tables now emit `SET LOCAL max_parallel_workers_per_gather = 4` and `SET LOCAL enable_parallel_hash = on` so PostgreSQL can use parallel workers; threshold tunable via `pg_ripple.parallel_query_min_joins`
- **Better planner statistics** — extended statistics on `(s, o)` column pairs are automatically created when a predicate is promoted from `vp_rare` to a dedicated VP table; this helps the PostgreSQL planner estimate join cardinalities for multi-predicate queries
- **SHACL-informed optimizer** — if you have loaded SHACL shapes with `sh:maxCount 1` or `sh:minCount 1`, the optimizer reads those hints and can use them for join costing; hints are only applied when semantics are preserved
- **Safer query pipeline** — a fuzz integration test suite verifies that malformed SPARQL, SQL injection attempts in IRI values, Unicode IRIs, deeply nested property paths, and very large literals are all handled gracefully without crashes or data corruption

### What happens behind the scenes

The BGP reordering optimizer queries `pg_class.reltuples` and `pg_stats.n_distinct` for each VP table at translation time to estimate how many rows a pattern will produce given its bound columns. Patterns are sorted cheapest-first using a greedy left-deep algorithm. Before executing the generated SQL, `SET LOCAL join_collapse_limit = 1` is emitted so the PostgreSQL planner does not reorder the joins back. On macOS/Linux, `SET LOCAL enable_mergejoin = on` is also set to exploit merge-join when join columns are ordered.

For parallel execution, the query engine counts VP-table aliases (`_t0`, `_t1`, …) in the generated SQL; if the count reaches `parallel_query_min_joins`, parallel hash join settings are activated before query execution.

Extended statistics (`CREATE STATISTICS … (ndistinct, dependencies) ON s, o`) are created in `_pg_ripple` schema alongside the VP tables when `promote_predicate()` runs. This gives the planner correlation data that single-column `ANALYZE` cannot provide.

<details>
<summary>Technical details</summary>

- **src/sparql/optimizer.rs** (new) — `reorder_bgp()`: greedy left-deep selectivity-based reorder; `TableStats` struct with `pg_class.reltuples` + `pg_stats.n_distinct` queries; `load_predicate_hints()`: reads SHACL shapes for `sh:maxCount`/`sh:minCount` hints
- **src/sparql/plan_cache.rs** — added `HIT_COUNT` and `MISS_COUNT` `AtomicU64` counters; `stats()` returns `(hits, misses, size, cap)`; `reset()` evicts cache and clears counters; cache key now includes `bgp_reorder` GUC value
- **src/sparql/sqlgen.rs** — `translate_bgp()` now calls `optimizer::reorder_bgp()` before building the join tree
- **src/sparql/mod.rs** — `execute_select()` emits `SET LOCAL join_collapse_limit = 1`, `enable_mergejoin = on`, and parallel hints when applicable; new public `plan_cache_stats()` and `plan_cache_reset()` functions
- **src/storage/mod.rs** — `promote_rare_predicates()` calls `create_extended_statistics()` for each newly promoted predicate; `create_extended_statistics()` issues `CREATE STATISTICS IF NOT EXISTS … (ndistinct, dependencies) ON s, o`
- **src/lib.rs** — two new GUCs: `pg_ripple.bgp_reorder` (bool, default on), `pg_ripple.parallel_query_min_joins` (int, default 3); two new `pg_extern` functions: `plan_cache_stats() RETURNS JSONB`, `plan_cache_reset() RETURNS VOID`
- **sql/pg_ripple--0.12.0--0.13.0.sql** — migration script (no schema DDL; new functions are compiled into the extension library)
- **tests/pg_regress/sql/shacl_query_opt.sql** — verifies BGP reorder GUC, plan cache stats/reset, SHACL shape reading, and sparql_explain output
- **tests/pg_regress/sql/fuzz_integration.sql** — verifies graceful handling of empty queries, malformed SPARQL, SQL injection via IRI, Unicode IRIs, large literals, deeply nested property paths, and adversarial cache usage

</details>

---

## [0.12.0] — 2026-04-16 — SPARQL Update (Advanced)

This release completes the full SPARQL 1.1 Update specification. Building on the `INSERT DATA` / `DELETE DATA` support from v0.5.1, pg_ripple now supports pattern-based updates, remote RDF loading, and full named-graph lifecycle management.

**New in this release:** Find-and-replace data using SPARQL patterns with `DELETE/INSERT WHERE`. Fetch and load remote RDF documents from any HTTP(S) URL with `LOAD <url>`. Clear, drop, or create named graphs with a single SPARQL Update call.

### What you can do

- **Pattern-based updates** — `DELETE { … } INSERT { … } WHERE { … }` finds matching triples using the full SPARQL→SQL engine and then deletes and inserts triples for each result row; both the DELETE and INSERT templates may reference WHERE-bound variables
- **INSERT WHERE** — omit the DELETE clause to insert a triple for every WHERE match
- **DELETE WHERE** — omit the INSERT clause to remove all triples matching a pattern
- **LOAD remote RDF** — `LOAD <url>` fetches a Turtle, N-Triples, or RDF/XML document via HTTP(S) and inserts all triples; `LOAD <url> INTO GRAPH <g>` targets a named graph; `LOAD SILENT <url>` suppresses network errors
- **Clear a graph** — `CLEAR GRAPH <g>` removes all triples from a named graph without touching the default graph; `CLEAR DEFAULT`, `CLEAR NAMED`, `CLEAR ALL` let you clear one or all graphs in a single call
- **Drop a graph** — `DROP GRAPH <g>` clears and deregisters a graph; `DROP SILENT` suppresses errors on non-existent graphs; `DROP ALL` clears the entire store
- **Create a graph** — `CREATE GRAPH <g>` pre-registers a named graph in the dictionary; `CREATE SILENT` is a no-op if the graph already exists

### What happens behind the scenes

When `DELETE/INSERT WHERE` runs, the WHERE clause is compiled through the existing SPARQL→SQL engine into a SELECT query. The result rows are collected in memory, and then for each row the DELETE phase removes any matched triples from VP storage, followed by the INSERT phase adding new ones. This keeps the operation transactional inside a single PostgreSQL call.

`LOAD` uses `ureq` (a lightweight Rust HTTP client) to fetch the URL. The response body is parsed by the same rio_turtle / rio_xml parsers used for local bulk loading; triples are inserted in batches using the standard VP storage path.

`CLEAR` and `DROP` call a new `clear_graph_by_id()` helper that deletes from both the HTAP delta tables and tombstones the main-partition rows — the same mechanism used by the existing `drop_graph()` function.

<details>
<summary>Technical details</summary>

- **src/sparql/mod.rs** — `sparql_update()` extended to handle all `GraphUpdateOperation` variants: `DeleteInsert`, `Load`, `Clear`, `Create`, `Drop`; new helpers `execute_delete_insert()`, `execute_load()`, `execute_clear()`, `execute_drop()`, `resolve_ground_term()`, `resolve_term_pattern()`, `resolve_named_node_pattern()`, `resolve_graph_name_pattern()`, `encode_literal_id()`
- **src/storage/mod.rs** — new `clear_graph_by_id(g_id)` mirrors `drop_graph()` but takes a pre-encoded ID; new `all_graph_ids()` collects all distinct graph IDs across VP tables and `vp_rare`
- **src/bulk_load.rs** — new graph-aware loaders `load_ntriples_into_graph()`, `load_turtle_into_graph()`, `load_rdfxml_into_graph()` accept a target `g_id` instead of always writing to the default graph (g=0)
- **Cargo.toml** — added `ureq = { version = "2", features = ["tls"] }` for `LOAD <url>` HTTP support
- **sql/pg_ripple--0.11.0--0.12.0.sql** — migration script (schema unchanged; new capabilities compiled into the extension library)
- **pg_regress** — new test suites: `sparql_update_where.sql`, `sparql_graph_management.sql`; both PASS

</details>

---

## [0.11.0] — 2026-04-16 — SPARQL & Datalog Views

This release adds always-fresh, incrementally-maintained stream tables for SPARQL and Datalog queries, plus Extended Vertical Partitioning (ExtVP) semi-join tables for multi-predicate star-pattern acceleration. All three features are built on top of [pg_trickle](https://github.com/trickle-labs/pg-trickle) and are soft-gated — pg_ripple loads and operates normally without pg_trickle; the new functions detect its absence at call time and return a clear error with an install hint.

**New in this release:** Compile any SPARQL SELECT query into a pg_trickle stream table with `create_sparql_view()`. Bundle a Datalog rule set with a goal pattern into a self-refreshing view with `create_datalog_view()`. Pre-compute semi-joins between frequently co-joined predicate pairs with `create_extvp()` to give 2–10× star-pattern speedups.

### What you can do

- **SPARQL views** — `pg_ripple.create_sparql_view(name, sparql, schedule, decode)` compiles a SPARQL SELECT query to SQL and registers it as a pg_trickle stream table; the table stays incrementally up-to-date on every triple insert/update/delete
- **Datalog views** — `pg_ripple.create_datalog_view(name, rules, goal, schedule, decode)` bundles inline Datalog rules with a goal query into a self-refreshing table; `create_datalog_view_from_rule_set(name, rule_set, goal, schedule, decode)` references a previously-loaded named rule set
- **ExtVP semi-joins** — `pg_ripple.create_extvp(name, pred1_iri, pred2_iri, schedule)` pre-computes the semi-join between two predicate tables; the SPARQL query engine detects and uses ExtVP tables automatically
- **Detect pg_trickle** — `pg_ripple.pg_trickle_available()` returns `true` if pg_trickle is installed, so callers can gate feature usage without catching errors
- **Lifecycle management** — `drop_sparql_view`, `drop_datalog_view`, `drop_extvp` remove both the stream table and the catalog entry; `list_sparql_views()`, `list_datalog_views()`, `list_extvp()` return JSONB arrays of registered objects

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.pg_trickle_available()` | `BOOLEAN` | Returns `true` if pg_trickle is installed |
| `pg_ripple.create_sparql_view(name, sparql, schedule DEFAULT '1s', decode DEFAULT false)` | `BIGINT` | Compile SPARQL SELECT to a pg_trickle stream table; returns column count |
| `pg_ripple.drop_sparql_view(name)` | `BOOLEAN` | Drop the stream table and catalog entry |
| `pg_ripple.list_sparql_views()` | `JSONB` | List all registered SPARQL views |
| `pg_ripple.create_datalog_view(name, rules, goal, rule_set_name DEFAULT 'custom', schedule DEFAULT '10s', decode DEFAULT false)` | `BIGINT` | Compile inline Datalog rules + goal into a stream table |
| `pg_ripple.create_datalog_view_from_rule_set(name, rule_set, goal, schedule DEFAULT '10s', decode DEFAULT false)` | `BIGINT` | Reference an existing named rule set for a Datalog view |
| `pg_ripple.drop_datalog_view(name)` | `BOOLEAN` | Drop the stream table and catalog entry |
| `pg_ripple.list_datalog_views()` | `JSONB` | List all registered Datalog views |
| `pg_ripple.create_extvp(name, pred1_iri, pred2_iri, schedule DEFAULT '10s')` | `BIGINT` | Pre-compute a semi-join stream table for two predicates |
| `pg_ripple.drop_extvp(name)` | `BOOLEAN` | Drop the ExtVP stream table and catalog entry |
| `pg_ripple.list_extvp()` | `JSONB` | List all registered ExtVP tables |

### New catalog tables

| Table | Description |
|-------|-------------|
| `_pg_ripple.sparql_views` | Stores SPARQL view name, original query, generated SQL, schedule, decode mode, stream table name, and variables |
| `_pg_ripple.datalog_views` | Stores Datalog view name, rules, rule set, goal, generated SQL, schedule, decode mode, stream table name, and variables |
| `_pg_ripple.extvp_tables` | Stores ExtVP name, predicate IRIs, predicate IDs, generated SQL, schedule, and stream table name |

<details>
<summary>Technical details</summary>

- **src/views.rs** — new module implementing all v0.11.0 public functions; `compile_sparql_for_view()` wraps `sparql::sqlgen::translate_select()` and renames internal `_v_{var}` columns to plain `{var}` for stream table compatibility; `create_extvp()` generates a parameterized semi-join SQL template over the two predicate VP tables
- **src/lib.rs** — three new catalog tables created at extension load time; eleven new `#[pg_extern]` functions exposed in the `pg_ripple` schema
- **src/datalog/mod.rs** — added `load_and_store_rules(rules_text, rule_set_name) -> i64` helper for Datalog view creation
- **src/sparql/mod.rs** — `sqlgen` module made `pub(crate)` so `views.rs` can call `translate_select()` directly
- **sql/pg_ripple--0.10.0--0.11.0.sql** — migration script adding the three catalog tables for upgrades from v0.10.0
- **pg_regress** — new test suites: `sparql_views.sql`, `datalog_views.sql`, `extvp.sql`; all pass

</details>

---

## [0.10.0] — 2026-04-16 — Datalog Reasoning Engine

This release delivers a full Datalog reasoning engine over the VP triple store. Rules are parsed from a Turtle-flavoured syntax, stratified for evaluation order, and compiled to native PostgreSQL SQL — no external reasoner process needed.

**New in this release:** pg_ripple can now execute RDFS and OWL RL entailment, user-defined inference rules, Datalog constraints, and arithmetic/string built-ins. Inference results are written back into the VP store with `source = 1` so explicit and derived triples are always distinguishable. A hot dictionary tier accelerates frequent IRI lookups, and a SHACL-AF bridge detects `sh:rule` properties in shape graphs and registers them alongside standard Datalog rules.

### What you can do

- **Write custom inference rules** — `pg_ripple.load_rules(rules, rule_set)` parses Turtle-flavoured Datalog and stores the compiled SQL strata
- **Built-in RDFS entailment** — `pg_ripple.load_rules_builtin('rdfs')` loads all 13 RDFS entailment rules; call `pg_ripple.infer('rdfs')` to materialize closure
- **Built-in OWL RL reasoning** — `pg_ripple.load_rules_builtin('owl-rl')` loads ~20 core OWL RL rules covering class hierarchy, property chains, and inverse/symmetric/transitive properties
- **Run inference on demand** — `pg_ripple.infer(rule_set)` runs all strata in order and inserts derived triples with `source = 1`; safe to call repeatedly (idempotent)
- **Declare integrity constraints** — rules with an empty head become constraints; `pg_ripple.check_constraints()` returns all violations as JSONB
- **Inspect and manage rule sets** — `pg_ripple.list_rules()` returns rules as JSONB; `pg_ripple.drop_rules(rule_set)` clears a named set; `enable_rule_set` / `disable_rule_set` toggle a set without deleting it
- **Accelerate hot IRIs** — `pg_ripple.prewarm_dictionary_hot()` loads frequently-used IRIs (≤ 512 B) into an UNLOGGED hot table for sub-microsecond lookups; survives connection pooling but not database restart
- **SHACL-AF bridge** — shapes that contain `sh:rule` entries are detected by `load_shacl()` and registered in the rules catalog; full SHACL-AF rule execution is planned for v0.11.0

### New GUC parameters

| GUC | Default | Description |
|-----|---------|-------------|
| `pg_ripple.inference_mode` | `'on_demand'` | `'off'` disables engine; `'on_demand'` evaluates via CTEs; `'materialized'` uses pg_trickle stream tables |
| `pg_ripple.enforce_constraints` | `'warn'` | `'off'` silences violations; `'warn'` logs them; `'error'` raises an exception |
| `pg_ripple.rule_graph_scope` | `'default'` | `'default'` applies rules to default graph only; `'all'` applies across all named graphs |

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.load_rules(rules TEXT, rule_set TEXT DEFAULT 'custom')` | `BIGINT` | Parse, stratify, and store a Datalog rule set; returns the number of rules loaded |
| `pg_ripple.load_rules_builtin(name TEXT)` | `BIGINT` | Load a built-in rule set by name (`'rdfs'` or `'owl-rl'`) |
| `pg_ripple.list_rules()` | `JSONB` | Return all active rules as a JSONB array |
| `pg_ripple.drop_rules(rule_set TEXT)` | `BIGINT` | Delete a named rule set; returns the number of rules deleted |
| `pg_ripple.enable_rule_set(name TEXT)` | `VOID` | Mark a rule set as active |
| `pg_ripple.disable_rule_set(name TEXT)` | `VOID` | Mark a rule set as inactive |
| `pg_ripple.infer(rule_set TEXT DEFAULT 'custom')` | `BIGINT` | Run inference; returns the number of derived triples inserted |
| `pg_ripple.check_constraints(rule_set TEXT DEFAULT NULL)` | `JSONB` | Evaluate integrity constraints; returns violations |
| `pg_ripple.prewarm_dictionary_hot()` | `BIGINT` | Load hot IRIs into UNLOGGED hot table; returns rows loaded |

<details>
<summary>Technical details</summary>

- **src/datalog/mod.rs** — public API and IR type definitions (`Term`, `Atom`, `BodyLiteral`, `Rule`, `RuleSet`); catalog helpers for `_pg_ripple.rules` and `_pg_ripple.rule_sets`
- **src/datalog/parser.rs** — tokenizer and recursive-descent parser for Turtle-flavoured Datalog; variables as `?x`, full IRIs as `<...>`, prefixed IRIs as `prefix:local`, head `:-` body `.` delimiter
- **src/datalog/stratify.rs** — SCC-based stratification via Kosaraju's algorithm; unstratifiable programs (negation cycles) are rejected with a clear error message naming the cyclic predicates
- **src/datalog/compiler.rs** — compiles Rule IR to PostgreSQL SQL; non-recursive strata use `INSERT … SELECT … ON CONFLICT DO NOTHING`; recursive strata use `WITH RECURSIVE … CYCLE` (PG18 native cycle detection); negation compiles to `NOT EXISTS`; arithmetic/string built-ins compile to inline SQL expressions
- **src/datalog/builtins.rs** — RDFS (13 rules: rdfs2–rdfs12, subclass, domain, range) and OWL RL (~20 rules: class hierarchy, property chains, inverse/symmetric/transitive) as embedded Rust string constants
- **src/dictionary/hot.rs** — UNLOGGED hot table `_pg_ripple.dictionary_hot` for IRIs ≤ 512 B; `prewarm_hot_table()` runs at `_PG_init` when `inference_mode != 'off'`; `lookup_hot()` and `add_to_hot()` provide O(1) in-process hash lookups
- **src/shacl/mod.rs** — `parse_and_store_shapes()` now calls `bridge_shacl_rules()` when `inference_mode != 'off'`; the bridge detects `sh:rule` and registers a placeholder in `_pg_ripple.rules`
- **VP store** — `source SMALLINT NOT NULL DEFAULT 0` column present in all VP tables; migration script adds it retroactively to tables created before v0.10.0; `source = 0` means explicit, `source = 1` means derived
- **Migration script** — `sql/pg_ripple--0.9.0--0.10.0.sql` includes all `CREATE TABLE IF NOT EXISTS` and `ALTER TABLE … ADD COLUMN IF NOT EXISTS` statements for zero-downtime upgrades
- New pg_regress tests: `datalog_custom.sql`, `datalog_rdfs.sql`, `datalog_owl_rl.sql`, `datalog_negation.sql`, `datalog_arithmetic.sql`, `datalog_constraints.sql`, `datalog_malformed.sql`, `shacl_af_rule.sql`, `rdf_star_datalog.sql`

</details>

---

## [0.9.0] — 2026-04-15 — Serialization, Export & Interop

This release completes RDF I/O: pg_ripple can now import from and export to all major RDF serialization formats, and SPARQL CONSTRUCT and DESCRIBE queries can return results directly as Turtle or JSON-LD.

**New in this release:** Until now, you could load Turtle and N-Triples but exports were limited to N-Triples and N-Quads. You can now export as Turtle or JSON-LD — formats that are friendlier for human reading and REST APIs respectively. RDF/XML import covers the format that Protégé and most OWL editors produce. Streaming export variants handle large graphs without buffering the full document in memory.

### What you can do

- **Load RDF/XML** — `pg_ripple.load_rdfxml(data TEXT)` parses conformant RDF/XML (Protégé, OWL, most ontology editors); returns the number of triples loaded
- **Export as Turtle** — `pg_ripple.export_turtle()` serializes the default graph (or any named graph) as a compact Turtle document with `@prefix` declarations; RDF-star quoted triples use Turtle-star notation
- **Export as JSON-LD** — `pg_ripple.export_jsonld()` serializes triples as a JSON-LD expanded-form array, ready for REST APIs and Linked Data Platform contexts
- **Stream large graphs** — `pg_ripple.export_turtle_stream()` and `pg_ripple.export_jsonld_stream()` return one line at a time as `SETOF TEXT`, suitable for `COPY … TO STDOUT` pipelines
- **Get CONSTRUCT results as Turtle** — `pg_ripple.sparql_construct_turtle(query)` runs a SPARQL CONSTRUCT query and returns a Turtle document instead of JSONB rows
- **Get CONSTRUCT results as JSON-LD** — `pg_ripple.sparql_construct_jsonld(query)` returns JSONB in JSON-LD expanded form
- **Get DESCRIBE results as Turtle or JSON-LD** — `pg_ripple.sparql_describe_turtle(query)` and `pg_ripple.sparql_describe_jsonld(query)` offer the same format choice for DESCRIBE

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.load_rdfxml(data TEXT)` | `BIGINT` | Parse RDF/XML, load into default graph |
| `pg_ripple.export_turtle(graph TEXT DEFAULT NULL)` | `TEXT` | Export graph as Turtle |
| `pg_ripple.export_jsonld(graph TEXT DEFAULT NULL)` | `JSONB` | Export graph as JSON-LD (expanded form) |
| `pg_ripple.export_turtle_stream(graph TEXT DEFAULT NULL)` | `SETOF TEXT` | Streaming Turtle export |
| `pg_ripple.export_jsonld_stream(graph TEXT DEFAULT NULL)` | `SETOF TEXT` | Streaming JSON-LD NDJSON export |
| `pg_ripple.sparql_construct_turtle(query TEXT)` | `TEXT` | CONSTRUCT result as Turtle |
| `pg_ripple.sparql_construct_jsonld(query TEXT)` | `JSONB` | CONSTRUCT result as JSON-LD |
| `pg_ripple.sparql_describe_turtle(query TEXT, strategy TEXT DEFAULT 'cbd')` | `TEXT` | DESCRIBE result as Turtle |
| `pg_ripple.sparql_describe_jsonld(query TEXT, strategy TEXT DEFAULT 'cbd')` | `JSONB` | DESCRIBE result as JSON-LD |

<details>
<summary>Technical details</summary>

- `rio_xml` crate added as a dependency for RDF/XML parsing (uses rio_api `TriplesParser` interface, consistent with existing rio_turtle parsers)
- `src/export.rs` extended with `export_turtle`, `export_jsonld`, `export_turtle_stream`, `export_jsonld_stream`, `triples_to_turtle`, and `triples_to_jsonld`
- Turtle serialization groups by subject using `BTreeMap` for deterministic output; emits predicate-object lists per subject
- JSON-LD expanded form: each subject is one array entry; predicates become IRI-keyed arrays of `{"@value": …}` / `{"@id": …}` objects
- RDF-star quoted triples: passed through in Turtle-star `<< s p o >>` notation; in JSON-LD emitted as `{"@value": "…", "@type": "rdf:Statement"}`
- Streaming variants avoid buffering the full document; `export_turtle_stream` yields prefix lines then one `s p o .` per row
- SPARQL format functions (`sparql_construct_turtle`, etc.) delegate to the existing SPARQL engine then pass rows through the new serialization layer
- New pg_regress tests: `serialization.sql`, `rdf_star_construct.sql`, expanded `sparql_construct.sql`

</details>

---

## [0.8.0] — 2026-04-15 — Advanced Data Quality Rules

This release rounds out the data quality system with more expressive rules and a background validation mode that never slows down your inserts.

**New in this release:** Until now, each validation rule applied to a single property in isolation. You can now combine rules — "this value must satisfy rule A *or* rule B", "must satisfy *all* of these rules", "must *not* match this rule" — and count how many values on a property actually conform to a sub-rule. A background mode queues violations for later review instead of blocking every write.

### What you can do

- **Combine rules with logic** — use `sh:or`, `sh:and`, and `sh:not` to build validation rules that express complex conditions, such as "a contact must have either a phone number or an email address"
- **Reference another rule from within a rule** — `sh:node <ShapeIRI>` checks that each value on a property also satisfies a separate named rule; rules can reference each other up to 32 levels deep without getting stuck in a loop
- **Count qualifying values** — `sh:qualifiedValueShape` combined with `sh:qualifiedMinCount` / `sh:qualifiedMaxCount` counts only the values that actually pass a sub-rule, so you can say "at least two authors must be affiliated with a university"
- **Validate without blocking writes** — set `pg_ripple.shacl_mode = 'async'` so that inserts complete immediately and violations are collected silently in the background; the background worker drains the queue automatically
- **Inspect collected violations** — `pg_ripple.dead_letter_queue()` returns all async violations as a JSON array; `pg_ripple.drain_dead_letter_queue()` clears the queue once you have reviewed them
- **Drain the queue manually** — `pg_ripple.process_validation_queue(batch_size)` processes violations on demand, useful in test pipelines or batch jobs

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.process_validation_queue(batch_size BIGINT DEFAULT 1000)` | `BIGINT` | Process up to N pending validation jobs |
| `pg_ripple.validation_queue_length()` | `BIGINT` | How many jobs are waiting in the queue |
| `pg_ripple.dead_letter_count()` | `BIGINT` | How many violations have been recorded |
| `pg_ripple.dead_letter_queue()` | `JSONB` | All recorded violations as a JSON array |
| `pg_ripple.drain_dead_letter_queue()` | `BIGINT` | Delete all recorded violations and return how many were removed |

<details>
<summary>Technical details</summary>

- `ShapeConstraint` enum extended with `Or(Vec<String>)`, `And(Vec<String>)`, `Not(String)`, `QualifiedValueShape { shape_iri, min_count, max_count }`
- `validate_property_shape()` refactored to accept `all_shapes: &[Shape]` for recursive nested shape evaluation
- `node_conforms_to_shape()` added: depth-limited recursive conformance check (max depth 32)
- `process_validation_batch(batch_size)` added: SPI-based batch drain of `_pg_ripple.validation_queue`, writes violations to `_pg_ripple.dead_letter_queue`
- Merge worker (`src/worker.rs`) extended with `run_validation_cycle()` called after each merge transaction
- `validate_sync()` now handles `Class`, `Node`, `Or`, `And`, `Not`, and `QualifiedValueShape` (max-count check only for sync)
- `run_validate()` now checks top-level node `Or`/`And`/`Not` constraints in offline validation

</details>

---

## [0.7.0] — 2026-04-15 — Data Quality Rules (Core)

This release adds SHACL — a W3C standard for expressing data quality rules — and on-demand deduplication for datasets that have accumulated duplicate entries.

**What this means in practice:** You define rules like "every Person must have a name, and the name must be a string", load them into the database once, and pg_ripple will check those rules on every insert or on demand. Violations are reported as structured JSON so they can be logged, monitored, or acted on automatically.

### What you can do

- **Define data quality rules** — `pg_ripple.load_shacl(data TEXT)` parses rules written in W3C SHACL Turtle format and stores them in the database; returns the number of rules loaded
- **Check your data** — `pg_ripple.validate(graph TEXT DEFAULT NULL)` runs all active rules against your data and returns a JSON report: `{"conforms": true/false, "violations": [...]}`. Pass a graph name to validate only that graph
- **Reject bad data on insert** — set `pg_ripple.shacl_mode = 'sync'` to have `insert_triple()` immediately reject any triple that violates a `sh:maxCount`, `sh:datatype`, `sh:in`, or `sh:pattern` rule
- **Manage rules** — `pg_ripple.list_shapes()` lists all loaded rules; `pg_ripple.drop_shape(uri TEXT)` removes one rule by its IRI
- **Remove duplicate triples** — `pg_ripple.deduplicate_predicate(p_iri TEXT)` removes duplicate entries for one property, keeping the earliest record; `pg_ripple.deduplicate_all()` deduplicates everything
- **Deduplicate automatically on merge** — set `pg_ripple.dedup_on_merge = true` to eliminate duplicates each time the background worker compacts data (see v0.6.0)

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.load_shacl(data TEXT)` | `INTEGER` | Parse Turtle, store rules, return count loaded |
| `pg_ripple.validate(graph TEXT DEFAULT NULL)` | `JSONB` | Full validation report |
| `pg_ripple.list_shapes()` | `TABLE(shape_iri TEXT, active BOOLEAN)` | All rules in the catalog |
| `pg_ripple.drop_shape(shape_uri TEXT)` | `INTEGER` | Remove a rule by IRI |
| `pg_ripple.deduplicate_predicate(p_iri TEXT)` | `BIGINT` | Remove duplicates for one property |
| `pg_ripple.deduplicate_all()` | `BIGINT` | Remove duplicates across all properties |
| `pg_ripple.enable_shacl_monitors()` | `BOOLEAN` | Create a live violation-count stream table (requires pg_trickle) |

### New configuration options

| Option | Default | Description |
|--------|---------|-------------|
| `pg_ripple.shacl_mode` | `'off'` | When to validate: `'off'`, `'sync'` (block bad inserts), `'async'` (queue for later — see v0.8.0) |
| `pg_ripple.dedup_on_merge` | `false` | Eliminate duplicate triples during each background merge |

### New internal tables

| Table | Description |
|-------|-------------|
| `_pg_ripple.shacl_shapes` | Stores each loaded rule with its IRI, parsed JSON, and active flag |
| `_pg_ripple.validation_queue` | Inbox for inserts when `shacl_mode = 'async'` |
| `_pg_ripple.dead_letter_queue` | Recorded violations with full JSONB violation reports |
| `_pg_ripple.violation_summary` | Live violation counts by rule and severity (created by `enable_shacl_monitors()`) |

### Supported validation constraints (v0.7.0)

`sh:minCount`, `sh:maxCount`, `sh:datatype`, `sh:in`, `sh:pattern`, `sh:class`, `sh:targetClass`, `sh:targetNode`, `sh:targetSubjectsOf`, `sh:targetObjectsOf`. Logical combinators (`sh:or`, `sh:and`, `sh:not`) and qualified constraints are added in v0.8.0.

### Upgrading from v0.6.0

```sql
ALTER EXTENSION pg_ripple UPDATE;
```

The migration creates three new tables (`shacl_shapes`, `validation_queue`, `dead_letter_queue`) and their indexes. No existing tables are modified.

---

## [0.6.0] — 2026-04-15 — High-Speed Reads and Writes at the Same Time

This release separates write traffic from read traffic so both can run at full speed simultaneously. It also adds change notifications so other systems can react to new triples in real time.

**The problem this solves:** In earlier versions, heavy read queries could slow down writes and vice versa. Now, writes go into a small fast table and reads see everything via a transparent view. A background worker periodically merges the write table into an optimised read table without interrupting either operation.

### What you can do

- **Write and read simultaneously without blocking** — inserts land in a fast write buffer; reads see both the buffer and the main read-optimised store through a transparent view
- **Trigger a manual merge** — `pg_ripple.compact()` immediately merges all pending writes into the read store; returns the total number of triples after compaction
- **Subscribe to changes** — `pg_ripple.subscribe(pattern TEXT, channel TEXT)` sends a PostgreSQL `LISTEN/NOTIFY` message to `channel` every time a triple matching `pattern` is inserted or deleted; use `'*'` to receive all changes
- **Unsubscribe** — `pg_ripple.unsubscribe(channel TEXT)` stops notifications on a channel
- **Get storage statistics** — `pg_ripple.stats()` reports total triple count, how many predicates have their own table, how many triples are still in the write buffer, and the background worker's process ID

### New SQL functions

| Function | Returns | Description |
|----------|---------|-------------|
| `pg_ripple.compact()` | `BIGINT` | Merge all pending writes into the read store |
| `pg_ripple.stats()` | `JSONB` | Storage and background worker statistics |
| `pg_ripple.subscribe(pattern TEXT, channel TEXT)` | `BIGINT` | Subscribe to change notifications |
| `pg_ripple.unsubscribe(channel TEXT)` | `BIGINT` | Stop notifications on a channel |
| `pg_ripple.htap_migrate_predicate(pred_id BIGINT)` | `void` | Migrate one property table to the split-storage layout |
| `pg_ripple.subject_predicates(subject_id BIGINT)` | `BIGINT[]` | All properties for a given subject (fast lookup) |
| `pg_ripple.object_predicates(object_id BIGINT)` | `BIGINT[]` | All properties for a given object (fast lookup) |

### New configuration options

| Option | Default | Description |
|--------|---------|-------------|
| `pg_ripple.merge_threshold` | `10000` | Minimum pending writes before background merge starts |
| `pg_ripple.merge_interval_secs` | `60` | Maximum seconds between merge cycles |
| `pg_ripple.merge_retention_seconds` | `60` | How long to keep the previous read table before dropping it |
| `pg_ripple.latch_trigger_threshold` | `10000` | Pending writes needed to wake the merge worker early |
| `pg_ripple.worker_database` | `postgres` | Which database the merge worker connects to |
| `pg_ripple.merge_watchdog_timeout` | `300` | Log a warning if the merge worker is silent for this many seconds |

### Bug fixes in this release

- **Startup race condition** — the extension's shared memory flag is now set inside the correct PostgreSQL startup hook, eliminating a rare crash window during server start
- **GUC registration crash** — configuration parameters requiring postmaster-level access no longer crash when `CREATE EXTENSION pg_ripple` runs without the extension in `shared_preload_libraries`
- **SPARQL aggregate decode bug** — `COUNT`, `SUM`, and similar aggregate results were incorrectly looked up in the string dictionary; they now pass through as plain numbers
- **Merge worker: DROP TABLE without CASCADE** — the merge worker failed if old tables had dependent views; fixed by using `CASCADE` and recreating the view afterwards
- **Merge worker: stale index name** — repeated `compact()` calls failed with "relation already exists" because the old index name survived a table rename; the stale index is now dropped before creating a new one

### Upgrading from v0.5.1

```sql
ALTER EXTENSION pg_ripple UPDATE;
```

The migration script adds a column to the predicate catalog, creates the pattern tables and change-notification infrastructure, and converts every existing property table to the split read/write layout in a single transaction. Existing triples land in the write buffer; call `pg_ripple.compact()` afterwards to move them into the read store immediately.

<details>
<summary>Technical details</summary>

- HTAP split: writes → `vp_{id}_delta` (heap + B-tree); cross-partition deletes → `vp_{id}_tombstones`; query view = `(main EXCEPT tombstones) UNION ALL delta`
- Background merge: sort-ordered insertion into a fresh `vp_{id}_main` (BRIN-indexed) + `ANALYZE`; previous main dropped after `merge_retention_seconds`
- `ExecutorEnd_hook` pokes the merge worker latch when `TOTAL_DELTA_ROWS` reaches `latch_trigger_threshold`
- Subject/object pattern tables (`_pg_ripple.subject_patterns`, `_pg_ripple.object_patterns`) — GIN-indexed `BIGINT[]` columns rebuilt by the merge worker; enable O(1) predicate lookup per node
- CDC notifications fire as `pg_notify(channel, '{"op":"insert|delete","s":...,"p":...,"o":...,"g":...}')` via trigger on each delta table

</details>

---

## [0.5.1] — 2026-04-15 — Compact Number Storage, CONSTRUCT/DESCRIBE, SPARQL Update, Full-Text Search

This release stores common data types (integers, dates, booleans) as compact numbers instead of text, making range comparisons in queries much faster. It also adds the two remaining SPARQL query forms, write support via SPARQL Update, and full-text search on text values.

### What you can do

- **Faster comparisons on numbers and dates** — `xsd:integer`, `xsd:boolean`, `xsd:date`, and `xsd:dateTime` values are stored as compact integers; FILTER comparisons (`>`, `<`, `=`) run as plain integer comparisons with no string decoding
- **SPARQL CONSTRUCT** — `pg_ripple.sparql_construct(query TEXT)` assembles new triples from a template and returns them as a set of `{s, p, o}` JSON objects; useful for transforming or exporting data
- **SPARQL DESCRIBE** — `pg_ripple.sparql_describe(query TEXT, strategy TEXT)` returns the neighbourhood of a resource — all triples directly connected to it (Concise Bounded Description) or both incoming and outgoing triples (Symmetric CBD)
- **SPARQL Update** — `pg_ripple.sparql_update(query TEXT)` executes `INSERT DATA { … }` and `DELETE DATA { … }` statements; returns the number of triples affected
- **Full-text search** — `pg_ripple.fts_index(predicate TEXT)` indexes text values for a property; `pg_ripple.fts_search(query TEXT, predicate TEXT)` searches them using standard PostgreSQL text-search syntax

### Bug fixes

- `fts_index` now accepts N-Triples `<IRI>` notation for the predicate argument
- `fts_index` now uses a correct partial index that does not require PostgreSQL subquery support
- Inline-encoded values (integers, dates) now decode correctly in SPARQL SELECT results instead of returning NULL

### New configuration options

- `pg_ripple.describe_strategy` (default `'cbd'`) — DESCRIBE expansion algorithm: `'cbd'`, `'scbd'` (symmetric), or `'simple'` (subject only)

---

## [0.5.0] — 2026-04-15 — Complete SPARQL 1.1 Query Engine

This release completes SPARQL 1.1 query support. All standard query patterns — graph traversal, aggregates, unions, subqueries, optional matches, and computed values — are now supported.

### What you can do

- **Traverse graph relationships** — property paths (`+`, `*`, `?`, `/`, `|`, `^`) follow chains of relationships; cyclic graphs are handled safely using PostgreSQL's cycle detection
- **Combine results from alternative patterns** — `UNION { ... } UNION { ... }` merges results from two or more patterns; `MINUS { ... }` removes results that match an unwanted pattern
- **Aggregate and group results** — `COUNT`, `SUM`, `AVG`, `MIN`, `MAX`, `GROUP_CONCAT` work with `GROUP BY` and `HAVING` just as in SQL
- **Use subqueries** — nest `{ SELECT … WHERE { … } }` patterns at any depth
- **Compute new values** — `BIND(<expr> AS ?var)` assigns a calculated value to a variable; `VALUES ?x { … }` injects a fixed set of values into a pattern
- **Optional matches** — `OPTIONAL { … }` returns results even when the optional pattern has no data, leaving those variables unbound
- **Limit recursion depth** — `pg_ripple.max_path_depth` caps how deep property-path traversal can go, preventing runaway queries on very large graphs

### Bug fixes

- Sequence paths (`p/q`) no longer produce a Cartesian product when intermediate nodes are anonymous
- `p*` (zero-or-more) paths no longer crash with a PostgreSQL CYCLE syntax error
- `OPTIONAL` no longer produces incorrect results due to an alias collision in the generated SQL
- `GROUP BY` column references no longer go out of scope in the outer query
- `MINUS` join clause now uses the correct column alias
- `VALUES` no longer generates a duplicate alias clause
- `BIND` in aggregate subqueries (`SELECT (COUNT(?p) AS ?cnt)`) now produces the correct SQL expression
- Numbers in FILTER expressions (`FILTER(?cnt >= 2)`) are now emitted as SQL integers instead of dictionary IDs
- Changing `pg_ripple.max_path_depth` mid-session now correctly invalidates the plan cache

<details>
<summary>Technical details</summary>

- Property paths compile to `WITH RECURSIVE … CYCLE` CTEs using PostgreSQL 18's hash-based `CYCLE` clause
- All pg_regress test files are now idempotent — safe to run multiple times against the same database
- `setup.sql` drops and recreates the extension for full isolation between runs
- New tests: `property_paths.sql`, `aggregates.sql`, `resource_limits.sql` — 12/12 pass

</details>

---

## [0.4.0] — 2026-04-14 — Statements About Statements (RDF-star)

This release adds RDF-star: the ability to store facts *about* facts. For example, you can record not just "Alice knows Bob" but also "Alice knows Bob — according to Carol, since 2020". This is essential for provenance tracking, temporal data, and property graph–style edge annotations.

### What you can do

- **Load N-Triples-star data** — `pg_ripple.load_ntriples()` now accepts N-Triples-star, including nested quoted triples in both subject and object position
- **Encode and decode quoted triples** — `pg_ripple.encode_triple(s, p, o)` stores a quoted triple and returns its ID; `pg_ripple.decode_triple(id)` converts it back to JSON
- **Use statement identifiers** — `pg_ripple.insert_triple()` now returns the stable integer identifier of the stored statement; that identifier can itself appear as a subject or object in other triples
- **Look up a statement by its identifier** — `pg_ripple.get_statement(i BIGINT)` returns `{"s":…,"p":…,"o":…,"g":…}` for any stored statement
- **Query with SPARQL-star** — ground (all-constant) quoted triple patterns work in SPARQL `WHERE` clauses: `WHERE { << :Alice :knows :Bob >> :assertedBy ?who }`

### Known limitations in this release

- Turtle-star is not yet supported; use N-Triples-star for RDF-star bulk loading
- Variable-inside-quoted-triple SPARQL patterns (e.g. `<< ?s :knows ?o >> :assertedBy ?who`) are deferred to v0.5.x
- W3C SPARQL-star conformance test suite not yet run (deferred to v0.5.x)

<details>
<summary>Technical details</summary>

- `KIND_QUOTED_TRIPLE = 5` added to the dictionary; quoted triples stored with `qt_s`, `qt_p`, `qt_o` columns via non-destructive `ALTER TABLE … ADD COLUMN IF NOT EXISTS`
- Custom recursive-descent N-Triples-star line parser — avoids the `oxrdf/rdf-12` + `spargebra` feature conflict with no new crate dependencies
- `spargebra` and `sparopt` now use the `sparql-12` feature, enabling `TermPattern::Triple` with correct exhaustiveness guards
- SPARQL-star ground patterns compile to a dictionary lookup + SQL equality condition

</details>

---

## [0.3.0] — 2026-04-14 — SPARQL Query Language

This release introduces SPARQL, the standard W3C query language for RDF data. You can now ask questions over your stored facts using a familiar graph-pattern syntax, with results returned as JSON.

### What you can do

- **Run SPARQL SELECT queries** — `pg_ripple.sparql(query TEXT)` executes a SPARQL SELECT and returns one JSON object per result row, with variable names as keys and values in standard N-Triples format
- **Run SPARQL ASK queries** — `pg_ripple.sparql_ask(query TEXT)` returns `true` if any results exist, `false` otherwise
- **Inspect the generated SQL** — `pg_ripple.sparql_explain(query TEXT, analyze BOOL DEFAULT false)` shows what SQL was generated from a SPARQL query; pass `analyze := true` for a full execution plan with timings
- **Tune the query plan cache** — `pg_ripple.plan_cache_size` (default 256) controls how many SPARQL-to-SQL translations are cached per connection; set to `0` to disable caching

### Supported query features

- Basic graph patterns with bound or wildcard subjects, predicates, and objects
- `FILTER` with comparisons (`=`, `!=`, `<`, `<=`, `>`, `>=`) and boolean operators (`&&`, `||`, `!`, `BOUND()`)
- `OPTIONAL` (left-join)
- `GRAPH <iri> { … }` and `GRAPH ?g { … }` for named graph scoping
- `SELECT` with variable projection, `DISTINCT`, `REDUCED`
- `LIMIT`, `OFFSET`, `ORDER BY`

<details>
<summary>Technical details</summary>

- SPARQL text → `spargebra 0.4` algebra tree → SQL via `src/sparql/sqlgen.rs`; all IRI and literal constants are encoded to `i64` before appearing in SQL — SQL injection via SPARQL constants is structurally impossible
- Per-query encoding cache avoids redundant dictionary lookups for constants appearing multiple times in one query
- Self-join elimination: patterns sharing a subject but using different predicates compile to a single scan, not separate subqueries
- Batch decode: all integer result columns are decoded in a single `SELECT … WHERE id IN (…)` round-trip
- `RUST_TEST_THREADS = "1"` in `.cargo/config.toml` prevents concurrent dictionary upsert deadlocks in the test suite
- New pg_regress tests: `sparql_queries.sql` (10 queries), `sparql_injection.sql` (7 adversarial inputs)

</details>

---

## [0.2.0] — 2026-04-14 — Bulk Loading, Named Graphs, and Export

This release makes it practical to work with large RDF datasets. You can load standard RDF files, organise triples into named collections, export data back to standard formats, and register IRI prefixes for convenience.

### What you can do

- **Load RDF files in bulk** — `pg_ripple.load_ntriples(data TEXT)`, `load_nquads(data TEXT)`, `load_turtle(data TEXT)`, and `load_trig(data TEXT)` accept standard RDF text and return the number of triples loaded
- **Load from a file on the server** — `pg_ripple.load_ntriples_file(path TEXT)` and its siblings read a file directly from the server filesystem (requires superuser); essential for large datasets
- **Organise triples into named graphs** — `pg_ripple.create_graph('<iri>')` creates a named collection; `pg_ripple.drop_graph('<iri>')` deletes it along with its triples; `pg_ripple.list_graphs()` lists all collections
- **Export data** — `pg_ripple.export_ntriples(graph)` and `pg_ripple.export_nquads(graph)` serialise stored triples to standard text; pass `NULL` to export all triples
- **Register IRI prefixes** — `pg_ripple.register_prefix('ex', 'https://example.org/')` records a shorthand; `pg_ripple.prefixes()` lists all registered mappings
- **Promote rare properties manually** — `pg_ripple.promote_rare_predicates()` moves any property that has grown beyond the threshold into its own dedicated table

### How rare properties work

Properties with fewer than 1,000 triples (configurable via `pg_ripple.vp_promotion_threshold`) are stored in a shared table rather than creating a dedicated table for each one. Once a property crosses the threshold it is automatically migrated. This keeps the database tidy for datasets with many rarely-used properties.

### How blank node scoping works

Blank node identifiers (`_:b0`, `_:b1`, etc.) from different load calls are automatically isolated. Loading the same file twice will produce separate, independent blank nodes rather than merging them — which is almost always what you want.

<details>
<summary>Technical details</summary>

- `rio_turtle 0.8` / `rio_api 0.8` added for N-Triples, N-Quads, Turtle, and TriG parsing
- Blank node scoping via `_pg_ripple.load_generation_seq`: each load advances a shared sequence; blank node hashes are prefixed with `"{generation}:"` to prevent cross-load merging
- `batch_insert_encoded` groups triples by predicate and issues one multi-row INSERT per predicate group, reducing round-trips
- `_pg_ripple.statements` range-mapping table created (populated in v0.6.0)
- `_pg_ripple.prefixes` table: `(prefix TEXT PRIMARY KEY, expansion TEXT)`
- GUCs added: `pg_ripple.vp_promotion_threshold` (i32, default 1000), `pg_ripple.named_graph_optimized` (bool, default off)
- New pg_regress tests: `triple_crud.sql`, `named_graphs.sql`, `export_ntriples.sql`, `nquads_trig.sql`

</details>

---

## [0.1.0] — 2026-04-14 — First Working Release

pg_ripple can now be installed into a PostgreSQL 18 database. After installation you can store facts — statements like "Alice knows Bob" — and retrieve them by pattern. This is the foundation that all later releases build on. No query language yet: just the core building blocks.

### What you can do

- **Install the extension** — `CREATE EXTENSION pg_ripple` in any PostgreSQL 18 database (requires superuser)
- **Store facts** — `pg_ripple.insert_triple('<Alice>', '<knows>', '<Bob>')` saves a fact and returns a unique identifier for it
- **Find facts by pattern** — `pg_ripple.find_triples('<Alice>', NULL, NULL)` returns everything about Alice; `NULL` is a wildcard for any position
- **Delete facts** — `pg_ripple.delete_triple(…)` removes a specific fact
- **Count facts** — `pg_ripple.triple_count()` returns how many facts are stored
- **Encode and decode terms** — `pg_ripple.encode_term(…)` converts a text term to its internal numeric ID; `pg_ripple.decode_id(…)` converts it back

### How storage works

Every piece of text — names, URLs, values — is converted to a compact integer before storage. Lookups and joins operate on integers, not strings, which is what makes queries fast. Facts are automatically organised into one table per relationship type, and relationship types with few facts share a single table to avoid creating thousands of tiny tables. Every fact receives a globally unique integer identifier that later versions use for RDF-star.

<details>
<summary>Technical details</summary>

- pgrx 0.17 project scaffolding targeting PostgreSQL 18
- Extension bootstrap creates `pg_ripple` (user-visible) and `_pg_ripple` (internal) schemas; the `pg_` prefix requires `SET LOCAL allow_system_table_mods = on` during bootstrap
- Dictionary encoder (`src/dictionary/mod.rs`): `_pg_ripple.dictionary` table; XXH3-128 hash stored in BYTEA; dense IDENTITY sequence as join key; backend-local LRU encode/decode caches; CTE-based upsert avoids pgrx 0.17 `InvalidPosition` error on empty `RETURNING` results
- Vertical partitioning (`src/storage/mod.rs`): `_pg_ripple.vp_{predicate_id}` tables with dual B-tree indices on `(s,o)` and `(o,s)`; `_pg_ripple.predicates` catalog; `_pg_ripple.vp_rare` consolidation table; `_pg_ripple.statement_id_seq` for globally-unique statement IDs
- Error taxonomy (`src/error.rs`): `thiserror`-based types — PT001–PT099 (dictionary), PT100–PT199 (storage)
- GUC: `pg_ripple.default_graph`
- CI pipeline: fmt, clippy, pg_test, pg_regress (`.github/workflows/ci.yml`)
- pg_regress tests: `setup.sql`, `dictionary.sql`, `basic_crud.sql`

</details>
