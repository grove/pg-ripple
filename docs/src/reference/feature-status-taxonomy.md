# Feature Status Taxonomy

`pg_ripple.feature_status()` returns one row per major capability.  Each row
includes a `status` column drawn from a fixed vocabulary of seven values.  This
page documents what each status means, its promotion criteria, and a concrete
example from the codebase.

---

## Status values

| Status | Meaning | Promotion criteria | Example |
|---|---|---|---|
| `planned` | Roadmap item exists; no user-facing implementation. | First code lands; at least one happy-path test passes; CHANGELOG entry added. → `experimental` | `sparql_12` (tracked in `plans/sparql12_tracking.md`) |
| `experimental` | Implementation exists; documented limits; may change. | Full behavior test matrix added (positive + negative + error cases); docs updated; CI gate added. → `implemented` | `arrow_flight` — real IPC streaming works; nonce replay protection added; tickets require HMAC signing |
| `stub` | API exists (SQL function registered) but production behavior is not implemented. | Real implementation replaces stub; existing callers continue to work; regression test covers the new behavior. → `experimental` | Any future function that requires external infrastructure not yet wired |
| `implemented` | Full execution path wired, tested in CI, and documented. | This is the steady state. May regress to `degraded` if a required dependency is removed. | `sparql_select`, `datalog_inference`, `construct_writeback` |
| `planner_hint` | An optimization hint or guidance is emitted, but no custom executor handles the path. | Full custom executor implemented; validated against reference results. → `implemented` | `wcoj` — cyclic-BGP join reordering; a true Leapfrog Triejoin executor is not implemented |
| `manual_refresh` | Feature produces correct output only when a manual action is performed (e.g., `SELECT pg_ripple.refresh_view(...)`). | Automatic refresh or change-detection wired; no manual step required. → `implemented` | Materialized SPARQL views before `cron`-based refresh was added |
| `degraded` | A required dependency or configuration is missing; a fallback or no-op is active. | Dependency is installed and the feature falls back to `implemented` automatically. | `vector_hybrid_search` without `pgvector` installed |

---

## Promotion process

1. **`planned` → `experimental`**: open a PR that adds the first implementation
   and at least one happy-path regression test.  Update CHANGELOG under
   `[Unreleased]`.  Update the `feature_status()` row in
   `src/feature_status.rs`.

2. **`experimental` → `implemented`**: add the full behavior test matrix (see
   the pg_regress test for the feature), update the docs page under
   `docs/src/`, and add a CI gate entry in the `ci_gate` field of the
   `feature_status()` row.

3. **`stub` → `experimental`**: ensure existing callers still pass (no breaking
   API change); add a regression test that exercises the real path.

---

## Adding a new feature to `feature_status()`

1. Add an entry to the `vec!` in `src/feature_status.rs`.
2. Choose the correct initial `status` from the table above.
3. Fill in `ci_gate` with the test that verifies the feature (e.g.,
   `"ci/regress: my_feature.sql"`).
4. Fill in `docs_path` with the primary documentation page.
5. If an evidence file exists (e.g., a test output or benchmark baseline),
   fill in `evidence_path`.
6. Update CHANGELOG.

---

## See also

- `pg_ripple.feature_status()` SQL function — `docs/src/reference/sql-functions.md`
- `src/feature_status.rs` in the repository
