-- Migration 0.21.0 → 0.22.0: Storage Correctness & Security Hardening
--
-- Schema changes in this release:
--
--   1. Lock down the _pg_ripple internal schema from unprivileged roles.
--   2. Bloom filter shmem counters are a Rust-level change; no DDL required.
--   3. New SQL monitoring function: pg_ripple.cache_stats().

-- 1. Privilege hardening: revoke all access to the internal schema and its
--    objects from PUBLIC.  The pg_ripple.* public API schema is unaffected.
--    Existing superuser-owned objects are not changed.
REVOKE ALL ON SCHEMA _pg_ripple FROM PUBLIC;
REVOKE ALL ON ALL TABLES IN SCHEMA _pg_ripple FROM PUBLIC;
REVOKE ALL ON ALL SEQUENCES IN SCHEMA _pg_ripple FROM PUBLIC;

-- 2. Expose shmem cache hit/miss statistics for monitoring.
--    The implementation is provided by the Rust extension; this comment
--    documents the function signature registered at load time:
--
--    pg_ripple.cache_stats() RETURNS TABLE (
--        hits        BIGINT,   -- encode cache hits (shmem + local combined)
--        misses      BIGINT,   -- cache misses that fell through to SPI
--        evictions   BIGINT,   -- slots evicted by LRU in the 4-way cache
--        utilisation FLOAT     -- fraction of slots currently occupied (0–1)
--    )

-- What this release provides (Rust-compiled changes):
--
--   Storage correctness:
--   • Dictionary cache: RegisterXactCallback drains ENCODE_CACHE /
--     DECODE_CACHE on ROLLBACK — rolled-back term IDs can no longer leak
--     into subsequent transactions as phantom references
--   • Shmem epoch counter: per-backend epoch stamped at rollback; shmem
--     cache hits from a prior epoch are rejected
--   • Merge C-3 fix: CREATE OR REPLACE VIEW step removed from merge cycle;
--     view always references vp_N_main by name (re-resolves after rename)
--   • Merge C-4 fix: max_sid_at_snapshot recorded at merge-start; only
--     tombstones with i ≤ max_sid_at_snapshot are TRUNCATEd at merge-end
--   • Shmem encode cache: 4-way set-associative (1024 sets × 4 ways),
--     same footprint as before, <1% collision rate at 5k hot terms
--   • Bloom filter: 8-bit saturating counters replace boolean bits —
--     clear_predicate_delta_bit only clears when counter reaches 0
--   • Rare-predicate promotion: atomic CTE (DELETE … RETURNING / INSERT)
--     eliminates INSERT-then-DELETE race with concurrent inserts;
--     triple_count restored accurately after promotion
--
--   Security:
--   • pg_ripple_http: PG_RIPPLE_HTTP_RATE_LIMIT now enforced via
--     tower_governor (default 100 req/s per IP; 429 on excess)
--   • pg_ripple_http: error responses return {"error":…,"trace_id":…} —
--     no PostgreSQL internal details exposed to API clients
--   • pg_ripple_http: constant-time Bearer/Basic token comparison
--   • register_endpoint() rejects non-http/https URL schemes
--
--   GUC / worker hardening:
--   • pg_ripple.vp_promotion_threshold: min=10, max=10_000_000 bounds added
--   • Merge worker: reset_latch() called before sleep in error back-off
