-- Migration 0.43.0 → 0.44.0: UNIQUE constraint on vp_rare for set semantics
--
-- Adds a UNIQUE(p, s, o, g) constraint to _pg_ripple.vp_rare so that duplicate
-- quad insertions are silently rejected via ON CONFLICT DO NOTHING.  This fixes
-- SPARQL UPDATE set semantics: inserting the same triple twice in a single UPDATE
-- (e.g. two INSERT WHERE operations that match the same binding) no longer creates
-- duplicate rows in vp_rare, ensuring COUNT(*) returns the correct value of 1.
--
-- Existing duplicates (if any) are removed first to allow the constraint to be
-- added without error.

-- Remove any existing duplicate quads, keeping the one with the lowest i.
DELETE FROM _pg_ripple.vp_rare a
USING _pg_ripple.vp_rare b
WHERE a.i > b.i
  AND a.p = b.p AND a.s = b.s AND a.o = b.o AND a.g = b.g;

-- Add the UNIQUE constraint.
ALTER TABLE _pg_ripple.vp_rare
    ADD CONSTRAINT vp_rare_psoq_unique UNIQUE (p, s, o, g);

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.44.0', '0.43.0')
ON CONFLICT DO NOTHING;
