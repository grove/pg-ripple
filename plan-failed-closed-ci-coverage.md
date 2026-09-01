# Plan: fail-closed CI coverage

- Status: planned
- Baseline: pg_ripple 0.136.0
- Initial qualification implementation: 8 to 12 person-days
- Initial product remediation: 9 to 15 person-days
- Initial delivery estimate: 17 to 27 person-days
- Full repeated qualification implementation, including initial gates: 30 to 49 person-days
- Full repeated product remediation reserve: 17 to 29 person-days
- Full repeated qualification estimate: 47 to 78 person-days

## Objective

Add fail-closed qualification for three operational paths:

1. PostgreSQL point-in-time recovery, or PITR.
2. A real Citus cluster with one coordinator and multiple workers.
3. Sustained HTTP traffic, asynchronous queues, and injected failures.

The first delivery adds bounded correctness and recovery gates. The later work
adds packet faults, longer runs, more queues, and repeated evidence. Shared
GitHub runners will not enforce tight latency or capacity thresholds.

This plan does not make pg_ripple production-proven by itself. It produces
repeatable evidence for specific topologies, workloads, and failure cases.
It includes both the qualification work and the product remediation required
to make that evidence meaningful.

## Three meanings of fail closed

Use these terms separately throughout the work:

| Meaning | Required behavior |
|---|---|
| Test harness | Missing prerequisites, evidence, or assertions fail CI. |
| Application | A failed operation cannot report a successful state. |
| Service | The HTTP process stays live during a database outage while readiness reports failure. |

## Current baseline

The repository already has useful pieces, but none of the three paths has the
full qualification described in this plan.

- `tests/resilience/physical_backup_restore.sh` runs a physical restore. Its
  PITR branch is opt-in and current CI does not enable Write Ahead Log (WAL)
  archiving or run it.
- `.github/workflows/ci.yml` runs upgrade recovery, physical restore, and
  resource pressure. It retains raw output for 90 days.
- `tests/integration/citus_rls_propagation.sh` assumes that the root Compose
  file starts a Citus coordinator and two workers. `docker-compose.yml`
  contains one PostgreSQL service and one HTTP service instead.
- The required regression suite checks behavior when Citus is absent. It does
  not install Citus or inspect a worker.
- `pg_ripple_http/tests/process_smoke.sh` proves that the HTTP process starts,
  serves health requests, and stops after `SIGTERM`.
- `benchmarks/soak_72h.sh` samples three database metrics. It does not generate
  traffic, inject failures, or enforce an acceptance condition.

## Scope

### Initial fail-closed gates

| Workstream | Qualification scope | Qualification estimate | Product remediation scope | Remediation estimate | Combined |
|---|---|---:|---|---:|---:|
| PITR | Archive WAL, restore to a named target, verify exact RDF state, and retain evidence | 1 to 2 days | Correct recovery configuration and startup checks | 1 to 2 days | 2 to 3 days |
| Citus | Build a coordinator and three workers, prove distribution and query correctness, rebalance, restart a worker, and verify row-level security (RLS) | 4 to 6 days | Distribution errors, distributed HTAP merge behavior, and rebalance synchronization | 6 to 10 days | 10 to 16 days |
| HTTP and queues | Run bounded mixed traffic, cancel a stream, stop and restart PostgreSQL, drain JSON writeback and SHACL queues, and retain evidence | 3 to 4 days | Shutdown, pool, readiness, and queue acknowledgement changes | 2 to 4 days | 5 to 8 days |
| Total | | 8 to 12 days | | 9 to 15 days | 17 to 27 days |

The qualification estimate covers runners, fixtures, assertions, and evidence.
The remediation estimate covers known code changes and the architecture work
that those assertions may require. The Citus merge checkpoint can exceed the
remediation reserve if the pinned Citus release cannot support a safe swap.

### Repeated qualification

| Workstream | Qualification scope | Qualification estimate | Product remediation reserve | Remediation estimate | Combined total |
|---|---|---:|---|---:|---:|
| PITR | Separate restore host, production-like data, corrupt and delayed WAL cases, and recovery point objective and recovery time objective measurements | 5 to 8 days | Recovery-path fixes found by those profiles | 2 to 4 days | 7 to 12 days |
| Citus | Concurrent writes during rebalance, interrupted rebalance, network partitions, replicated placements, longer load, and repeated seeds | 13 to 21 days | Distributed merge and failure-recovery fixes | 9 to 15 days | 22 to 36 days |
| HTTP and queues | Packet faults, embedding and bidi queues, Server-Sent Events (SSE) delivery, nightly load, and 6-hour, 24-hour, and 72-hour runs | 12 to 20 days | Delivery, overload, and lifecycle fixes found by those profiles | 6 to 10 days | 18 to 30 days |
| Total | | 30 to 49 days | | 17 to 29 days | 47 to 78 days |

These ranges include the initial gate work. In the repeated table, the
qualification scope lists work added after the initial gate. The estimates do
not include every defect that the new tests might find. Treat the product
remediation columns as reserves, not commitments.

### Non-goals

The initial delivery will not:

- claim high availability from a Citus topology with one shard placement;
- qualify every Citus provider or cloud platform;
- test embedding, bidi relay, and live SSE delivery in the first HTTP gate;
- enforce latency regressions on shared GitHub runners;
- run a 24-hour or 72-hour test on every pull request;
- add a general-purpose chaos framework before the bounded scenarios need it;
- treat one successful long run as a stable qualification record.

## Fail-closed contract

Every new job and runner must follow the same contract.

### Process behavior

- Start shell runners with `set -euo pipefail`.
- Run `psql` with `-X -v ON_ERROR_STOP=1`.
- Fail when a prerequisite, fixture, expected metric, or output file is absent.
- Do not use `continue-on-error`, a successful skip, or a fallback baseline.
- Put a wall-clock timeout around each scenario and the whole job.
- Use a unique run ID for every database, container project, port, and temporary
  directory.
- Clean up in an `EXIT` trap and in an Actions step with `if: always()`.
- Write a failure summary from the `EXIT` trap when a runner stops early.
- Never touch a developer cluster unless the caller sets the existing
  destructive-test opt-in variables.

### Evidence behavior

Each job must retain these files:

- `summary.json`, with the final status and every acceptance assertion;
- `environment.txt`, with the commit, extension version, PostgreSQL version,
  dependent extension versions, image digests, and runner identity;
- raw runner output;
- PostgreSQL and service logs;
- state snapshots that support the assertions;
- a fault timeline when the job stops a process or changes a network path.

Use a versioned summary format with these required fields:

| Field | Meaning |
|---|---|
| `schema_version` | Evidence schema version, starting at `1` |
| `run_id` | Unique run identifier for the database, containers, ports, and evidence |
| `scenario` | Stable scenario name |
| `scenario_version` | Version of the fixture and assertion contract |
| `status` | `pass` or `fail` |
| `git_sha` | Full tested commit SHA |
| `extension_version` | Installed pg_ripple version |
| `postgres_version` | Full PostgreSQL version |
| `seed` | Deterministic seed, or `null` when the scenario is not seeded |
| `started_at` and `finished_at` | UTC timestamps |
| `assertions` | Named assertions with status, expected, actual, and UTC start and finish timestamps |
| `failure_class` | `none` for a pass, or the stable class of the first failure |
| `faults` | Ordered fault events, or an empty array |

Validate `summary.json` with the checked-in standard-library validator
`tests/qualification/validate_summary.py`. For schema version `1`, reject
missing or wrongly typed fields, duplicate assertion names, invalid statuses,
invalid timestamps, and unknown top-level fields. Every assertion must contain
`name`, `status`, `expected`, `actual`, `started_at`, and `finished_at`.
Every runner invokes the validator before it reports success.

Upload evidence with `if: always()`, `if-no-files-found: error`, and a 90-day
retention period. A job failure must still upload the files that exist.

### CI policy

- Gate correctness, state conservation, recovery deadlines, and bounded
  resource behavior on shared runners.
- Record latency and throughput on shared runners. Do not gate on them.
- Run capacity thresholds only on named hardware with a pinned configuration.
- Define a clean promotion set as five consecutive clean runs on the intended
  trigger set plus at least 20 clean executions across multiple runner
  allocations and, where supported, deterministic seeds. An unexplained rerun
  after a failure does not count.
- Promote a new job to required status only after it meets the clean promotion
  set on its intended trigger set.
- Add each required job name to branch protection after promotion. This is a
  repository setting and cannot be enforced by a workflow file alone.

### Queue identity and conservation

Assign every test item a unique durable qualification ID before enqueue. Carry
that ID through processing and into its terminal state or pending state. If a
queue lacks a durable item ID, add the smallest queue or test-audit field linked
to the queue row that can carry it.

Validate both the count equation and the ID sets. Every qualification ID must
appear exactly once in one terminal or pending category. A duplicate item and a
missing item must fail the gate even when aggregate counts still balance.

## Workstream 1: CI-backed PITR

### PITR-01: preserve an immutable base backup

Change `tests/resilience/physical_backup_restore.sh` so that it never starts
the directory created by `pg_basebackup`.

1. Keep `BASE_DIR` immutable after `pg_basebackup` finishes.
2. Copy `BASE_DIR` to `RESTORE_DIR` for the physical-restore check.
3. Copy `BASE_DIR` to `PITR_DIR` for the PITR check.
4. Start and stop only `RESTORE_DIR` and `PITR_DIR`.
5. Verify that the three paths are distinct before starting a server.

Acceptance criteria:

- The base backup remains byte-for-byte unused by a PostgreSQL process.
- Both restored clusters start from independent copies.
- Cleanup stops both clusters and removes only the unique test directories.

### PITR-02: create a transactionally valid recovery target

The current script sends the `good` update, restore-point call, and `bad`
update in one `psql -c` invocation. PostgreSQL can run that string in one
implicit transaction. Split the state changes into separate committed calls.

Run this sequence:

1. Insert a unique RDF triple named `before_target`, then commit it.
2. Create the named restore point in its own call.
3. Record the returned log sequence number (LSN) and its exact WAL segment
   name.
4. Insert a unique RDF triple named `after_target`, then commit it.
5. Call `pg_switch_wal()` in a separate call.
6. Wait for the exact target WAL segment to appear in the archive directory.

Do not accept an increase in archive file count as proof. A previous archive
backlog can satisfy that condition.

Acceptance criteria:

- The evidence records the restore-point name, LSN, and WAL segment.
- The runner observes the exact segment in the archive before recovery starts.
- The target and post-target mutations have separate transaction commits.

### PITR-03: preserve required PostgreSQL settings

The restored server must load pg_ripple at startup. The recovery configuration
must not discard settings from the base backup.

1. Append recovery settings instead of overwriting `postgresql.auto.conf`.
2. Start both restored clusters with
   `shared_preload_libraries=pg_ripple`.
3. Disable WAL archival in the restored clusters. They must not write into the
   source archive directory.
4. Give each restored cluster a unique port and short Unix socket path.
5. Assert that the merge worker is healthy when the configuration enables it.

Acceptance criteria:

- The source archive contains files from only the source server.
- `SHOW shared_preload_libraries` includes `pg_ripple` after recovery.
- `pg_ripple.health()` succeeds and the expected worker state is present.

### PITR-04: verify exact relational and RDF state

Keep the relational marker check, but add an RDF recovery oracle.

After PITR promotion, assert all of these conditions:

- `pg_is_in_recovery()` is false.
- The pre-target RDF triple exists exactly once.
- The post-target RDF triple does not exist.
- The marker table contains the target value and not the later value.
- `pg_ripple.triple_count()` matches the expected count for the test graph.
- The extension version matches the source extension version.
- `pg_ripple.health()` returns a healthy result.

Use a dedicated named graph and unique IRIs. Do not infer success from
`triple_count() >= 0`.

### PITR-05: enable the live path in CI

Extend the existing migration qualification job in
`.github/workflows/ci.yml`.

1. Create a unique WAL archive directory under `RUNNER_TEMP`.
2. Start the disposable pgrx server with `archive_mode=on` and an
   `archive_command` that copies into that directory.
3. Read the source data directory with `SHOW data_directory`.
4. Set `PG_RIPPLE_RUN_PITR=1`.
5. Set `PG_RIPPLE_WAL_ARCHIVE_DIR` to the unique archive directory.
6. Pass a workspace evidence directory to the restore runner.
7. Run the script under `pipefail`.
8. Validate `summary.json` with `tests/qualification/validate_summary.py`.
9. Upload the summary, raw output, source log, restore log, PITR log, and
   environment record.

The initial job should add no more than ten minutes to the migration job.

### PITR-06: add repeated recovery profiles

Add these profiles after the bounded gate is stable:

| Profile | Purpose | Trigger |
|---|---|---|
| Missing WAL | Prove that recovery fails with a clear diagnostic | Nightly |
| Corrupt WAL | Prove that recovery fails instead of accepting damaged state | Nightly |
| Delayed archive | Measure recovery behavior when WAL arrival pauses | Nightly |
| Production-sized restore | Measure recovery point, recovery time, and checksums on named hardware | Release candidate |
| Separate restore host | Prove that recovery does not depend on source-host state | Release candidate |

Run each production profile twice before treating it as release evidence.

### PITR definition of done

The initial PITR work is complete when:

- required CI runs the named-target restore without an opt-in skip;
- the recovered graph includes the pre-target triple and excludes the
  post-target triple;
- the job waits for the exact WAL segment;
- restored servers load pg_ripple at startup;
- all evidence files upload on success and failure;
- the job meets the clean promotion set.

## Workstream 2: real multi-node Citus qualification

### CITUS-01: prove the topology before changing product code

Start with a two-day compatibility and topology checkpoint. Stop the
workstream if this checkpoint finds an unsupported PostgreSQL or Citus
combination.

Add these files:

- `tests/integration/citus/Dockerfile`
- `tests/integration/citus/docker-compose.yml`
- `tests/integration/citus/bootstrap.sh`
- `tests/integration/citus/smoke.sql`

Pin a Citus 14 image that supports PostgreSQL 18 by tag and digest. Build one
pg_ripple package from the tested commit. Install that exact package in every
container.

The Compose topology must contain:

- one coordinator;
- `worker1` and `worker2`, registered before data load;
- `worker3`, started but registered only for the rebalance scenario.

Set `shared_preload_libraries` to include both `citus` and `pg_ripple` on every
node. Use 8 to 16 shards for the bounded fixture.

Checkpoint acceptance criteria:

- Every node runs PostgreSQL 18 and the same Citus and pg_ripple versions.
- The coordinator reports two active workers before the rebalance test.
- A promoted VP delta table appears in `pg_dist_partition`.
- Physical shard rows exist on both registered workers.
- A bound-subject query returns the expected result.
- Container logs and the topology snapshot upload as evidence.

### CITUS-02: make distribution fail closed

Implement this after CITUS-03 chooses a supported distributed merge or an
explicit no-merge mode.

Current distribution helpers log warnings and continue. Change them so that a
failed distribution cannot return a `distributed` status.

Update these paths:

- `src/citus/mod.rs::make_reference_table`
- `src/citus/ddl_hooks.rs::distribute_vp_delta`
- `src/citus/rebalance.rs::enable_citus_sharding`

Required behavior:

1. Propagate Citus DDL errors to the SQL caller.
2. Verify each reference table in Citus metadata after creation.
3. Verify the delta and tombstone distribution key, colocation group, shard
   count, and active placements.
4. Return `distributed` only after every verification passes.
5. Send `pg_ripple.vp_promoted` only after the table is usable.
6. Leave the previous catalog state intact when a distribution step fails.

Add a negative integration case that makes one worker unavailable during
distribution. The SQL call must fail and the result must not claim success.

### CITUS-03: resolve the hybrid transactional and analytical main-table model

Run this checkpoint immediately after CITUS-01 and before the full Citus query
oracle or distribution hardening. Its result controls the remaining Citus
scope.

This is the main architectural checkpoint. The hybrid transactional and
analytical processing (HTAP) layout currently distributes the delta and
tombstone tables. The merge path creates and swaps a local main table, then
truncates the distributed tables.

Write a failing integration case first:

1. Load enough triples to promote a predicate.
2. Enable Citus sharding.
3. Record canonical query results and physical placement counts.
4. Run the merge path.
5. Record the same results and placements again.

The supported implementation must distribute the delta, main, and tombstone
tables by `s` in the same colocation group. Modify the main-table creation and
merge path so that:

- `main_new` is distributed before it receives data;
- its shard count and colocation match the delta table;
- the old main table remains available until every new placement is healthy;
- the swap does not truncate delta or tombstone data before verification;
- a failed distributed merge leaves the previous readable state intact.

If Citus cannot support a safe swap on the pinned version, fail the merge call
with a documented PostgreSQL error while Citus sharding is enabled. In that
case, the initial Citus gate qualifies a no-merge mode only. Full Citus
qualification remains blocked until distributed merge works. Do not retain a
silent local main table.

### CITUS-04: build a deterministic query oracle

Create `tests/integration/citus/fixture.sql` and
`tests/integration/citus/assertions.sql`.

The fixture should contain about 10,000 triples, three promoted predicates,
two named graphs, and two test roles. Keep the fixture small enough for a pull
request job.

Run the same fixture in a single-node reference database and in the Citus
coordinator. Compare sorted canonical results for:

- a bound-subject pattern;
- a multi-predicate star pattern;
- an object-bound pattern that must fan out and remain complete;
- a cross-subject join;
- a named-graph query;
- an aggregate;
- an insert followed by a read;
- a delete followed by a read;
- a merge or compaction when Citus merge is supported.

Store a hash and row count for each canonical result. Compare the hashes at
these checkpoints:

1. Before distribution.
2. After distribution.
3. After rebalance.
4. After a worker restart.

Do not claim object-bound single-shard pruning. The tables are distributed by
subject, so an object-bound query must fan out unless a separate
object-distributed structure exists.

### CITUS-05: qualify RLS on physical worker shards

Replace the assumptions in `tests/integration/citus_rls_propagation.sh` with
worker-level assertions.

1. Create the test role on every node through a Citus-supported DDL path.
2. Propagate policies with safely quoted role and policy identifiers.
3. Query each active worker for policies on its physical VP shard tables.
4. Verify the actual policy-name format from `src/security_api.rs`.
5. Query the allowed and restricted named graphs as the test role.
6. Repeat the worker-policy and result checks after rebalance.
7. Remove the role and policies during cleanup.

The gate fails if a worker lacks the policy, even when the coordinator returns
the expected rows.

### CITUS-06: wait for rebalance completion

`citus_rebalance()` starts asynchronous work and currently releases its fence
immediately. Keep the fence and the `merge_start` state active until Citus
reports a terminal result.

Required behavior:

- Start the rebalance with a bounded timeout.
- Poll the supported Citus progress API until every move completes or one
  move fails.
- Return an error for a failed or timed-out move.
- Send `merge_end` only after the terminal result.
- Release the merge fence in cleanup on every exit path.
- Record each move, source, target, status, and duration.

Register `worker3`, run the rebalance, and assert that at least one placement
moves to it. Query hashes must remain unchanged.

### CITUS-07: qualify worker interruption

Use one shard placement in the initial topology. That configuration does not
provide high availability.

1. Select a shard and identify its owning worker.
2. Stop that worker.
3. Run a query that requires the unavailable shard.
4. Require a clear query failure. Partial results are a test failure.
5. Restart the worker.
6. Wait for Citus to mark the worker active.
7. Re-run every canonical query and compare the hashes.

Add replicated placements and permanent-loss recovery only in the repeated
qualification phase. Keep those claims separate from restart recovery.

### CITUS-08: add the CI workflow and evidence

Add `.github/workflows/citus-qualification.yml` with these properties:

- a 45-minute job timeout;
- path filters during the manual and stabilization phase for `src/citus/**`,
  storage merge code, security code, Citus integration tests, and the Citus
  workflow files;
- a unique Compose project name;
- no reuse of stateful volumes between runs;
- strict SQL execution;
- container log collection before cleanup;
- evidence upload on success and failure;
- `docker compose down --volumes` only for the unique CI project.

Retain these Citus-specific evidence files:

- image tags and digests;
- the node and extension version matrix;
- `pg_dist_node`, `pg_dist_partition`, shard, and placement snapshots;
- canonical query hashes and row counts;
- rebalance moves and timing;
- the worker fault timeline;
- coordinator and worker logs.

Run the job manually during development. After the job meets the clean
promotion set, remove the workflow path filters and make the job a required
check on every pull request.
This avoids a required check that stays pending because its workflow did not
start. Add a nightly run with three deterministic seeds after the pull request
gate is stable.

### Citus definition of done

The initial Citus work is complete when:

- CI starts one coordinator and three worker containers;
- the tested pg_ripple build is installed on every node;
- distribution failures reach the SQL caller;
- the HTAP merge path is either distributed and qualified or rejected with a
  documented error;
- canonical results match before and after distribution, rebalance, and worker
  restart;
- worker loss fails loudly instead of returning partial results;
- RLS policies exist on the physical worker shards;
- all evidence files upload on success and failure;
- the job meets the clean promotion set.

## Workstream 3: sustained HTTP, queue, and network qualification

### HTTP-01: fix lifecycle behavior before load testing

Fix the known lifecycle problems before writing the sustained runner.

#### Bound graceful shutdown

`pg_ripple_http/src/main.rs::shutdown_signal` currently sleeps for the shutdown
timeout before it tells Axum to stop accepting work. Change the supervisor so
that it:

1. Receives `SIGINT` or `SIGTERM`.
2. Signals Axum to stop accepting new requests immediately.
3. Waits up to the configured shutdown timeout for in-flight work.
4. Cancels remaining work and exits nonzero when the timeout expires.

Add a process test with one open stream. The process must exit within the
configured timeout plus five seconds.

#### Bound pool acquisition

Most handlers call `pool.get().await` without a timeout. Add one shared pool
acquisition function to `AppState` and use it in every handler.

- Add a millisecond configuration value for pool wait time.
- Return HTTP 503 when the pool wait expires.
- Record a distinct pool-timeout metric.
- Apply the same bound to the primary and replica pools.
- Add tests for pool exhaustion and recovery.

#### Report current readiness

Make `/ready` describe current database readiness. It must not remain 200 only
because the service connected once in the past. Keep `/health` as process
liveness and use `/health/ready` for the deep extension check.

Acceptance criteria:

- `/health` remains 200 while PostgreSQL is unavailable.
- `/ready` and `/health/ready` become 503 within three seconds of database
  loss.
- Both readiness endpoints return 200 within 30 seconds of database recovery.
- No handler waits forever for a pool connection.

### HTTP-02: add a deterministic bounded traffic runner

Add these files:

- `tests/http_resilience/run.sh`
- `tests/http_resilience/load.py`
- `tests/http_resilience/validate.py`
- `tests/http_resilience/fixtures.sql`

Use Python's standard library and `ThreadPoolExecutor`. Do not add a load-test
dependency for the first gate.

Run the real HTTP binary for 60 to 90 seconds with:

- a four-connection PostgreSQL pool;
- 8 to 16 HTTP clients;
- a fixed random seed;
- SELECT, ASK, UPDATE, and streamed SELECT requests;
- at least 500 completed requests before faults begin.

Write one JSON object per line to a JSON Lines (JSONL) file. Include the request
type, start time, duration, status, response byte count, and fault phase. The
validator must fail on malformed or missing records.

The shared-runner gate enforces:

- zero unexpected transport failures outside an injected fault window;
- zero unexpected HTTP 500 responses;
- the minimum completed-request count;
- bounded request duration based on configured timeouts;
- exact query and update correctness;
- no live HTTP process after cleanup.

Record latency percentiles, but do not gate on them in this job.

### HTTP-03: qualify stream cancellation

Open a streamed query, read part of the body, and close the client connection.

Assert that:

- the extension receives a cancellation or disconnect;
- the active-stream count returns to zero;
- the pool returns the connection or discards it safely;
- a later request succeeds;
- cancellation-failure counters do not increase.

This scenario covers streamed query cancellation. It does not qualify live SSE
subscription delivery.

### QUEUE-01: qualify JSON writeback conservation

Create a real JSON mapping and relational target. Enqueue work faster than the
background worker drains it.

Record these counts before and after each fault:

- rows enqueued;
- durable qualification IDs enqueued;
- rows processed successfully;
- rows processed with an error;
- rows still pending;
- target rows written;
- target-key duplicates;
- the oldest pending age.

Enforce this conservation rule:

`enqueued = processed_success + processed_error + pending`

Also require exact ID-set conservation. Each enqueued qualification ID must be
in exactly one of the successful, error, or pending sets. Target-key
duplicates remain a separate assertion.

Also require:

- no duplicate target key;
- exact target values for successful items;
- a final pending count of zero for a healthy run;
- a visible error and retry count for an injected status-update failure;
- no increase in silent-drop counters.

If more than one drain worker can run, claim rows with a database lock and
`SKIP LOCKED` before executing writeback. Mark completion only after the target
operation commits.

### QUEUE-02: qualify SHACL queue conservation

Create one conforming item, one violating item, and one poison item that must
reach the dead-letter path.

Carry each item's durable qualification ID through the queue and track enough
state to prove this equation:

`enqueued = accepted + violations + dead_letter + pending`

Require exact set conservation as well. Every enqueued ID must occur once in
one of those four sets, and no ID may occur in two terminal categories.

Add a durable counter or audit row if the current schema cannot distinguish a
successfully processed item from a lost item.

After the queue drains, assert that:

- every conforming item is accepted once;
- every violation has the expected shape and focus node;
- every poison item is present in the dead-letter table;
- no item appears in more than one terminal category;
- pending is zero;
- retry and error counters match the injected failures.

### HTTP-04: stop and restart PostgreSQL under traffic

Keep the HTTP process and the traffic runner alive while PostgreSQL is stopped
for 10 to 20 seconds.

The fault timeline must record:

1. The last successful request before the stop.
2. The PostgreSQL stop time.
3. The first readiness 503.
4. The PostgreSQL start time.
5. The first readiness 200.
6. The first successful application request after recovery.

Acceptance criteria:

- The HTTP process stays alive.
- `/health` stays 200.
- Readiness becomes 503 within three seconds.
- Expected request failures occur only during the fault and recovery window.
- Readiness returns to 200 within 30 seconds.
- JSON writeback and SHACL conservation equations still hold.
- Queues drain after PostgreSQL returns.

### HTTP-05: add the bounded CI workflow

Add a separate `http-queue-resilience` job to `.github/workflows/ci.yml`, or
add `.github/workflows/http-queue-resilience.yml` if the main workflow becomes
harder to review.

The job must:

- build and run the real release-mode HTTP binary;
- install pg_ripple in a disposable PostgreSQL 18 cluster;
- enable the background worker configuration needed by the queue tests;
- run the bounded traffic, cancellation, database outage, queue, and shutdown
  scenarios;
- finish within 15 minutes;
- upload request JSONL, `summary.json`, HTTP logs, PostgreSQL logs, metrics
  snapshots, queue snapshots, and the environment record;
- fail when any evidence file is absent.

After the job meets the clean promotion set, require this job for HTTP, queue,
worker, schema, and workflow changes.

### NET-01: add packet-level fault injection

Add packet faults only after the bounded database-outage gate is stable.

Create a qualification Compose topology with:

- the HTTP service;
- PostgreSQL;
- a pinned Toxiproxy container between HTTP and PostgreSQL;
- deterministic fake embedding and federation services when those queues enter
  scope.

Test these faults with fixed durations and recovery deadlines:

- connection refusal;
- connection reset;
- added latency and jitter;
- request timeout;
- a blackholed connection;
- bandwidth restriction;
- a half-closed connection;
- PostgreSQL restart;
- HTTP restart.

The production outbound policy blocks private addresses. If fake outbound
services require an exception, compile an explicit test-only feature. The
production binary must not expose a setting that disables private-address
protection.

### QUEUE-03: repair and qualify embedding delivery

The embedding worker currently deletes queue rows before it calls the external
API. An API error or dimension mismatch can therefore lose work.

Change the queue to a claim, process, and acknowledge model:

1. Claim a bounded batch with a lease.
2. Keep each row durable while the API call runs.
3. Acknowledge the row only after the embedding commits.
4. Release or retry the row after a timeout or process crash.
5. Move poison rows to a dead-letter state after a bounded retry count.

Add conservation assertions for API errors, malformed responses, timeouts,
worker restart, and duplicate delivery. Do not add embedding to the support
claim until this gate passes.

### QUEUE-04: repair and qualify bidi overload behavior

The bidi relay currently drops the incoming call when its in-flight limit is
reached. Its acquire operation uses a separate load and increment, and release
is not guaranteed after a panic.

Before sustained qualification:

- make admission atomic;
- use a guard that releases the in-flight count on every exit path;
- define one implemented drop policy instead of aliasing `oldest` to dropping
  the newest call;
- return a result that tells the caller when the relay rejects work;
- record accepted, completed, rejected, retried, and dead-letter counts;
- add a conservation assertion for overload and restart.

### SSE-01: separate live subscription qualification

Do not reuse the stale SSE burst script. It targets obsolete endpoints and
masks request failures.

Either implement notification consumption for the current subscription route
or document the route as keepalive-only. If notification delivery is
supported, add a separate gate that proves ordered event delivery,
reconnection, duplicate handling, and cleanup after client disconnect.

### SOAK-01: replace passive sampling with an active run

Keep `benchmarks/soak_72h.sh` as historical input or replace it with a driver
that generates traffic and validates results.

Use these profiles:

| Profile | Duration | Trigger | Purpose |
|---|---:|---|---|
| Pull request | 5 to 10 minutes | Relevant changes | Correctness and recovery |
| Nightly | 30 to 60 minutes | Schedule | Seeded faults and leak detection |
| Weekly | 6 hours | Schedule on named runner | Queue age, memory, file descriptors, and recovery |
| Candidate | 24 hours | Manual | Release-candidate stability |
| Extended candidate | 72 hours | Manual | Long-duration evidence |

Record these time series:

- request success and error counts;
- request duration distributions;
- process resident memory and file descriptors;
- primary and replica pool state;
- active and cancelled streams;
- queue depth, oldest age, retries, dead letters, and throughput;
- PostgreSQL sessions, locks, temporary files, WAL rate, and restart count;
- pg_ripple worker state and unmerged delta rows.

Set memory and latency limits only after three clean baseline runs on the same
named hardware. Correctness, conservation, and recovery deadlines remain hard
gates from the first run.

### HTTP, queue, and network definition of done

The initial work is complete when:

- a real HTTP process handles at least 500 requests under bounded concurrency;
- every request outside the fault window has the expected result;
- stream cancellation returns the pool and active-stream counts to zero;
- readiness detects database loss and recovery within the stated deadlines;
- JSON writeback and SHACL queues satisfy their conservation equations;
- shutdown finishes within the configured bound;
- all evidence files upload on success and failure;
- the job meets the clean promotion set.

Full qualification also requires embedding and bidi conservation, packet
faults, live SSE delivery if supported, and three clean repetitions of each
long-run profile.

## Delivery sequence

Keep pull requests small enough for focused review. Do not combine Citus
storage changes with HTTP queue changes.

| Pull request | Scope | Exit condition |
|---|---|---|
| 1 | PITR script corrections and exact RDF oracle | Local disposable run passes |
| 2 | PITR workflow, summary validator, and retained logs | Summary validator and local workflow pass |
| 3 | Citus image, topology, and minimal merge reproducer | Rows exist on two workers and the merge case records a result |
| 4 | Citus HTAP architecture decision and supported failure path | Distributed merge works safely, or no-merge mode fails explicitly |
| 5 | Fail-closed Citus distribution and any merge changes | Negative distribution case fails correctly |
| 6 | Citus query oracle | Canonical hashes remain equal |
| 7 | Citus RLS, rebalance, and worker restart | All worker assertions pass |
| 8 | Citus workflow and evidence | Job meets the clean promotion set |
| 9 | HTTP shutdown, pool timeout, and readiness fixes | Focused process tests pass |
| 10 | Bounded traffic and stream cancellation | Request and cancellation summary passes |
| 11 | JSON writeback and SHACL identity conservation | Both equations and ID sets hold through restart |
| 12 | HTTP workflow and evidence | Job meets the clean promotion set |
| 13 | Packet fault topology | Seeded network scenarios pass |
| 14 | Embedding and bidi delivery fixes | Conservation tests pass |
| 15 | Active nightly and candidate soak profiles | Three clean runs per promoted profile |

Do not start the full Citus query oracle until the HTAP checkpoint has chosen a
safe distributed merge or an explicit no-merge support level. With two
engineers, run PITR and the Citus topology checkpoint in parallel with the HTTP
lifecycle fixes. Keep one owner for each workstream's evidence format and
acceptance assertions.

## Expected file changes

### Existing files

- `.github/workflows/ci.yml`
- `tests/resilience/physical_backup_restore.sh`
- `tests/resilience/fault_matrix.sh`
- `src/citus/mod.rs`
- `src/citus/ddl_hooks.rs`
- `src/citus/rebalance.rs`
- `src/citus/shard_pruning.rs`
- `src/storage/merge.rs`
- `src/security_api.rs`
- `tests/integration/citus_rls_propagation.sh`
- `pg_ripple_http/src/main.rs`
- `pg_ripple_http/src/common.rs`
- `pg_ripple_http/src/spi_bridge.rs`
- `pg_ripple_http/src/routing/admin_handlers/mod.rs`
- `src/json_mapping/writeback.rs`
- `src/worker.rs`
- `src/bidi/relay.rs`
- `src/stats.rs`
- `benchmarks/soak_72h.sh`
- `docs/src/operations/production-checklist.md`
- `docs/src/operations/citus-integration.md`
- `docs/src/evaluate/performance-results.md`

### New files

- `.github/workflows/citus-qualification.yml`
- `.github/workflows/http-queue-resilience.yml`, if kept separate
- `tests/integration/citus/Dockerfile`
- `tests/integration/citus/docker-compose.yml`
- `tests/integration/citus/bootstrap.sh`
- `tests/integration/citus/citus_multinode.sh`
- `tests/integration/citus/fixture.sql`
- `tests/integration/citus/assertions.sql`
- `tests/qualification/validate_summary.py`
- `tests/http_resilience/run.sh`
- `tests/http_resilience/load.py`
- `tests/http_resilience/validate.py`
- `tests/http_resilience/fixtures.sql`

Add no shared test framework unless the second runner duplicates enough code
to justify one. The existing shell helpers and Python standard library are
sufficient for the initial gates.

## Risks and controls

| Risk | Control |
|---|---|
| Distributed merge loses or localizes data | Write the merge integration case first. Reject Citus merge if a safe distributed swap is unavailable. |
| A worker failure returns partial results | Select a query that requires the stopped worker and require a clear failure. |
| Timing checks make CI flaky | Gate on state transitions with generous deadlines, not exact elapsed time. |
| Shared-runner performance varies | Record performance and gate only correctness and recovery. |
| Queue tests count completed work but miss lost or duplicated items | Enforce conservation equations and exact qualification ID sets from durable input and terminal states. |
| A fault-injection setting weakens production security | Compile private-address exceptions only into an explicit test build. |
| Cleanup removes developer data | Require opt-in outside CI and use unique validated names. |
| Evidence disappears after the Actions retention period | Copy candidate evidence to release storage before the artifact expires. |
| A workflow exists but branch protection ignores it | Add the promoted job name to required status checks. |

## Final acceptance criteria

The initial program is complete only when all of these statements are true:

1. PITR runs in CI, reaches an exact named target, and proves RDF state before
   and after the target.
2. Citus CI runs a coordinator and three workers with the tested extension on
   every node.
3. Citus query results remain exact through distribution, rebalance, and a
   worker restart.
4. A Citus worker outage fails loudly when the topology cannot serve a shard.
5. HTTP CI runs sustained mixed traffic against a real PostgreSQL process.
6. The HTTP service reports current readiness, recovers its pool, and exits
   within a hard shutdown bound.
7. JSON writeback and SHACL tests prove count conservation and exact
   qualification ID-set conservation. Every enqueued item reaches one durable
   terminal state or remains pending.
8. Every job fails on a missing fixture, missing assertion, missing artifact,
   SQL error, timeout, or skipped prerequisite.
9. Every job validates its summary and retains enough raw evidence to reproduce
   and audit its verdict.
10. Documentation states only the support level that the required gates prove.

The full program is complete after packet faults, embedding and bidi delivery,
supported SSE delivery, production-style PITR, replicated Citus recovery, and
the long-run profiles meet their acceptance criteria in three consecutive
runs.
