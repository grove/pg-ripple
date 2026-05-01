-- Migration 0.80.0 → 0.81.0: Correctness & stability hardening
--
-- New features provided by compiled Rust functions (no SQL schema changes needed
-- for most items).  The following DDL additions are required:
--
--  CDC-LSN-01  : Add _pg_ripple.cdc_lsn_watermark(slot_name, last_lsn) table.
--  PROMO-STUCK-01: Expose pg_ripple.recover_stuck_promotions() SQL function
--                  (implemented as a #[pg_extern]; no DDL required here).
--
-- Other v0.81.0 items are pure Rust changes:
--   MERGE-SID-01   — ORDER BY i ASC in HTAP merge CTE
--   PLAN-CACHE-GUC-02 — Extended plan-cache key
--   DICT-RACE-01   — Error (not panic) on 0-row RETURNING
--   DICT-SUBXACT-01 — Subtransaction abort invalidates decode cache
--   DICT-STRICT-01  — pg_ripple.strict_dictionary GUC
--   SHACL-TXN-01   — Savepoint around shape-store write
--   FED-URL-01     — Normalised federation endpoint URLs
--   FED-TRUNC-01   — WARNING + partial materialisation on oversized response
--   FED-CACHE-01   — Canonical SPARQL cache key
--   OPT-INNER-01   — Multi-predicate OPTIONAL→INNER JOIN optimisation
--   BN-SCOPE-01    — Query-scoped blank-node variable prefixes
--   DL-AGG-01      — Guard: aggregation in recursive Datalog rule head
--   DL-PAR-01/02   — Intra-stratum cycle detection + topological ordering
--   FILTER-STRICT-01 — pg_ripple.strict_sparql_filters GUC
--   REPL-UNWRAP-01  — Replace .unwrap() in replication.rs
--   PGFINI-01      — _PG_fini unregisters callbacks
--   PRELOAD-WARN-01 — Warning when loaded without shared_preload_libraries
--   RETRACT-PARAM-01 — Parameterised flat-VP DELETE
--   SCHEDULER-ERR-01 — Result-returning topological sort
--   DRED-FIXPOINT-01 — Full fixpoint in DRed re-derive phase
--   FEATURE-STATUS-BIDI-01 — 12 BIDI/BIDIOPS feature_status() rows
--   CDC-SLOT-01    — Background worker for orphaned CDC slot cleanup
--   PROMO-LOCK-01  — Per-predicate advisory locks (already present)
--   PROMO-ATOMIC-01 — TOCTOU fix (already present)
--   MERGE-FENCE-01 — Two-phase merge fence (ExclusiveLock only for swap)
--   RAG-SQL-INJECT-02 — Parameterised RAG query in pg_ripple_http

-- CDC-LSN-01: Create the LSN watermark table.
CREATE TABLE IF NOT EXISTS _pg_ripple.cdc_lsn_watermark (
    slot_name  TEXT PRIMARY KEY,
    last_lsn   PG_LSN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
