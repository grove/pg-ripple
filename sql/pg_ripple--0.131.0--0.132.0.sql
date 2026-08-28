-- Migration 0.131.0 → 0.132.0: conformance evidence and export pagination.
-- Adds the keyset-pagination index required by versioned graph exports.

CREATE INDEX IF NOT EXISTS idx_vp_rare_i
    ON _pg_ripple.vp_rare (i);
