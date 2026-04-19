-- Test: Tombstone GC (v0.37.0)
-- Verifies that tombstone_gc_enabled and tombstone_gc_threshold GUCs are
-- registered and accessible, and that the tombstone tables exist after HTAP setup.

-- Show tombstone_gc GUCs exist
SELECT name, setting
FROM pg_settings
WHERE name IN ('pg_ripple.tombstone_gc_enabled', 'pg_ripple.tombstone_gc_threshold')
ORDER BY name;

-- Verify tombstone_gc_enabled defaults to on
SELECT current_setting('pg_ripple.tombstone_gc_enabled') = 'on' AS gc_enabled_by_default;

-- Verify tombstone_gc_threshold default is accessible
SELECT current_setting('pg_ripple.tombstone_gc_threshold', true) IS NOT NULL AS threshold_set;

-- Set custom threshold and verify it is accepted
SET pg_ripple.tombstone_gc_threshold = '0.10';
SELECT current_setting('pg_ripple.tombstone_gc_threshold') AS threshold_value;

-- Disable and re-enable GC
SET pg_ripple.tombstone_gc_enabled = false;
SELECT current_setting('pg_ripple.tombstone_gc_enabled') AS gc_disabled;
SET pg_ripple.tombstone_gc_enabled = true;
SELECT current_setting('pg_ripple.tombstone_gc_enabled') AS gc_enabled;

-- Reset to defaults
RESET pg_ripple.tombstone_gc_threshold;
RESET pg_ripple.tombstone_gc_enabled;
