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

-- Add the UNIQUE constraint (idempotent: the 0.41→0.42 migration may have already
-- created a unique index under a slightly different name; skip if any unique index
-- on (p,s,o,g) already covers vp_rare).
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM   pg_index i
        JOIN   pg_class c ON c.oid = i.indrelid
        JOIN   pg_namespace n ON n.oid = c.relnamespace
        WHERE  n.nspname = '_pg_ripple'
          AND  c.relname = 'vp_rare'
          AND  i.indisunique
    ) THEN
        CREATE UNIQUE INDEX vp_rare_psoq_unique
            ON _pg_ripple.vp_rare (p, s, o, g);
    END IF;
END;
$$;

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.44.0', '0.43.0')
ON CONFLICT DO NOTHING;
