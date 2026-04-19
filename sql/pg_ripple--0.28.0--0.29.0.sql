-- Migration script: pg_ripple 0.28.0 → 0.29.0
--
-- Release: v0.29.0 — Datalog Optimization: Magic Sets & Cost-Based Compilation
--
-- Schema changes:
--
--   ADD COLUMN i TO tombstone tables
--     Each _pg_ripple.vp_{id}_tombstones table receives a new column:
--       i  BIGINT  NOT NULL  DEFAULT nextval('_pg_ripple.statement_id_seq')
--
--     This column records the statement-ID (SID) at tombstone creation, enabling
--     merge_predicate() to implement the C-4 optimization: delete only tombstones
--     with i ≤ max_sid_at_snapshot, preventing the tombstone-resurrection race
--     condition for concurrent deletes during a merge cycle.
--
--     Tombstone tables created by v0.29.0 or later already include the column.
--     This migration adds it to all existing tombstone tables created by earlier
--     versions (v0.6.0 – v0.28.0).
--
-- New SQL functions registered by this migration:
--
--   pg_ripple.infer_goal(rule_set TEXT, goal TEXT) → JSONB
--     Run goal-directed inference using a simplified magic sets transformation.
--     Returns {"derived": N, "iterations": K, "matching": M}.
--
-- Updated SQL functions:
--
--   pg_ripple.infer_with_stats(rule_set TEXT) → JSONB
--     Now includes "eliminated_rules": [...] key in the returned JSONB,
--     listing rules removed by subsumption checking before fixpoint evaluation.
--
-- New GUC parameters (all runtime-settable, no restart required):
--
--   pg_ripple.magic_sets            BOOL    DEFAULT true
--     Master switch for magic sets goal-directed inference.
--
--   pg_ripple.datalog_cost_reorder  BOOL    DEFAULT true
--     Sort Datalog body atoms by ascending VP-table cardinality before SQL
--     compilation (cost-based join reordering).
--
--   pg_ripple.datalog_antijoin_threshold  INT  DEFAULT 1000
--     Minimum VP-table row count for negated body atoms to use LEFT JOIN IS NULL
--     anti-join form instead of NOT EXISTS.
--
--   pg_ripple.delta_index_threshold  INT  DEFAULT 500
--     Minimum semi-naive delta-table row count before creating a B-tree index
--     on (s, o) join columns prior to the next fixpoint iteration.
--
-- New error codes:
--
--   PT501  magic sets transformation failed (circular binding pattern)
--   PT502  cost-based reordering skipped (statistics unavailable)
--
-- All new GUCs are registered via pgrx::GucRegistry in _PG_init.

-- ─── Add i column to all existing tombstone tables ───────────────────────────
DO $$
DECLARE
    rec RECORD;
BEGIN
    FOR rec IN
        SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL
    LOOP
        BEGIN
            EXECUTE format(
                'ALTER TABLE _pg_ripple.vp_%s_tombstones '
                'ADD COLUMN IF NOT EXISTS i BIGINT NOT NULL '
                'DEFAULT nextval(''_pg_ripple.statement_id_seq'')',
                rec.id
            );
        EXCEPTION WHEN undefined_table THEN
            NULL; -- tombstone table may not exist for some predicates
        END;
    END LOOP;
END;
$$;
