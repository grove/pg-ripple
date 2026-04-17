-- Migration 0.23.0 → 0.24.0: Semi-naive Datalog & Performance Hardening
--
-- New features (compiled from Rust — SQL schema changes below):
--   • Datalog: semi-naive ΔR evaluation; OWL RL rules cax-sco full closure,
--               cls-avf, prp-ifp, prp-spo1
--   • SPARQL: batch-decode result sets; property_path_max_depth GUC
--   • Export: streaming cursor-based export (export_batch_size GUC)
--   • BRIN index migrated from vp_*_main.s to vp_*_main.i (SID column)

-- Migrate BRIN indices on all existing VP main partitions from s to i.
-- This is performed dynamically at extension upgrade time via a DO block
-- because the set of VP main tables is not known at migration-script authoring time.
DO $$
DECLARE
    r RECORD;
    idx_name TEXT;
BEGIN
    FOR r IN
        SELECT schemaname, tablename
        FROM pg_tables
        WHERE schemaname = '_pg_ripple'
          AND tablename LIKE 'vp_%_main'
    LOOP
        -- Drop the old BRIN on s if it exists
        idx_name := r.tablename || '_s_brin';
        IF EXISTS (
            SELECT 1 FROM pg_indexes
            WHERE schemaname = r.schemaname AND tablename = r.tablename
              AND indexname = idx_name
        ) THEN
            EXECUTE format('DROP INDEX _pg_ripple.%I', idx_name);
        END IF;
        -- Create the new BRIN on i
        idx_name := r.tablename || '_i_brin';
        IF NOT EXISTS (
            SELECT 1 FROM pg_indexes
            WHERE schemaname = r.schemaname AND tablename = r.tablename
              AND indexname = idx_name
        ) THEN
            EXECUTE format(
                'CREATE INDEX %I ON _pg_ripple.%I USING brin (i)',
                idx_name, r.tablename
            );
        END IF;
    END LOOP;
END $$;
