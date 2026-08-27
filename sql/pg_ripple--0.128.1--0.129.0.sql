-- Migration 0.128.1 → 0.129.0: A18 critical & high correctness remediation
-- for JSON mapping relational writeback (C18-01 / H18-02).
--
-- Fixes (Rust-only, no schema impact beyond what is listed below):
--   - enable_json_writeback() / drain_json_writeback_queue() queried a
--     nonexistent _pg_ripple.dictionary.iri column; both now use the real
--     `value` column, which is why the async writeback path never actually
--     ran (C18-01 / JSON-WRITEBACK-ASYNC).
--   - Predicate dictionary lookup errors now propagate as a fatal error
--     instead of being silently treated as "not yet covered" (C18-01).
--   - writeback_json_row() / writeback_json_row_delete() returned a
--     hard-coded row count; both now report the real affected-row count via
--     INSERT/DELETE ... RETURNING (H18-02 / ROW-COUNTS).
--   - writeback_json_row() cast every column to ::text, which fails for any
--     non-text column (integer, uuid, ...); values are now cast to the
--     target column's real type (H18-02 / TYPED-PARAMS).
--   - Missing writeback_key_columns values are now validated up front with
--     a descriptive error instead of producing a mis-numbered SQL
--     placeholder (H18-02 / KEY-VALIDATION).
--   - enable_json_writeback() now also covers not-yet-promoted (still-rare)
--     predicates via a predicate-filtered vp_rare trigger, and covers
--     main-resident deletes via a *_tombstones trigger; promote_predicate()
--     installs the dedicated-table triggers automatically when a covered
--     predicate is later promoted (C18-01 / ENQUEUE-COVERAGE).
--
-- Schema changes:
--   - _pg_ripple.json_writeback_queue.retry_count (new column): tracks
--     drain-loop retries when the status UPDATE itself fails (L18-02)
--   - _pg_ripple.json_mappings.writeback_column_casts (new column): caches
--     the derived column-cast expressions used by TYPED-PARAMS above
--   - json_writeback_enqueue_fn() gains an optional predicate-id filter
--     argument and *_tombstones table support (C18-01 / ENQUEUE-COVERAGE)

ALTER TABLE _pg_ripple.json_writeback_queue
    ADD COLUMN IF NOT EXISTS retry_count SMALLINT NOT NULL DEFAULT 0;

COMMENT ON COLUMN _pg_ripple.json_writeback_queue.retry_count IS
    'Incremented by drain_json_writeback_queue() when its own status-update fails, '
    'so the row stays pending instead of being silently dropped (v0.129.0 L18-02). '
    'Exposed via the pg_ripple_json_writeback_drain_errors_total metric.';

ALTER TABLE _pg_ripple.json_mappings
    ADD COLUMN IF NOT EXISTS writeback_column_casts JSONB;

CREATE OR REPLACE FUNCTION _pg_ripple.json_writeback_enqueue_fn()
RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    v_mapping_name TEXT    := TG_ARGV[0];
    v_pred_filter  BIGINT  := NULLIF(TG_ARGV[1], '')::BIGINT;
    v_subject_id   BIGINT;
    v_operation    TEXT;
    v_row_pred     BIGINT;
    v_is_tombstone BOOLEAN := TG_TABLE_NAME LIKE '%\_tombstones' ESCAPE '\';
BEGIN
    IF TG_TABLE_NAME = 'vp_rare' THEN
        IF TG_OP = 'INSERT' THEN
            v_row_pred := NEW.p;
        ELSIF TG_OP = 'DELETE' THEN
            v_row_pred := OLD.p;
        END IF;
        IF v_pred_filter IS NOT NULL AND v_row_pred IS DISTINCT FROM v_pred_filter THEN
            RETURN NULL;
        END IF;
    END IF;

    IF v_is_tombstone THEN
        IF TG_OP <> 'INSERT' THEN
            RETURN NULL;
        END IF;
        v_subject_id := NEW.s;
        v_operation  := 'delete';
    ELSIF TG_OP = 'INSERT' THEN
        v_subject_id := NEW.s;
        v_operation  := 'upsert';
    ELSIF TG_OP = 'DELETE' THEN
        v_subject_id := OLD.s;
        v_operation  := 'delete';
    ELSE
        RETURN NULL;
    END IF;

    INSERT INTO _pg_ripple.json_writeback_queue
        (mapping_name, subject_id, operation)
    VALUES (v_mapping_name, v_subject_id, v_operation);
    RETURN NULL;
END;
$$;

COMMENT ON FUNCTION _pg_ripple.json_writeback_enqueue_fn() IS
    'Trigger function that enqueues VP delta/vp_rare/tombstone changes into '
    'json_writeback_queue (v0.128.0 JSON-WRITEBACK-01; v0.129.0 C18-01 adds '
    'vp_rare predicate filtering and tombstone-table support)';
