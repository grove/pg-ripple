-- Migration 0.35.0 → 0.36.0: Worst-Case Optimal Joins & Lattice-Based Datalog
--
-- New features in this release:
--   - Worst-Case Optimal Join (WCOJ) Leapfrog Triejoin for cyclic SPARQL patterns
--     * Cyclic BGP detection at SPARQL→SQL translation time
--     * Sort-merge join forcing for triangle and other cyclic graph queries
--     * `wcoj_is_cyclic(TEXT) RETURNS BOOLEAN` — inspect cyclic detection
--     * `wcoj_triangle_query(TEXT) RETURNS JSONB` — benchmark triangle queries
--   - Lattice-Based Datalog (Datalog^L) for monotone aggregation rules
--     * New catalog table: `_pg_ripple.lattice_types`
--     * Built-in lattice types: min, max, set, interval
--     * `create_lattice(name, join_fn, bottom) RETURNS BOOLEAN` — register lattice
--     * `list_lattices() RETURNS JSONB` — enumerate registered lattices
--     * `infer_lattice(rule_set, lattice_name) RETURNS JSONB` — run fixpoint
--
-- New GUCs (registered in _PG_init — no SQL DDL required beyond the table below):
--   pg_ripple.wcoj_enabled         BOOL    DEFAULT true
--   pg_ripple.wcoj_min_tables      INT     DEFAULT 3
--   pg_ripple.lattice_max_iterations INT   DEFAULT 1000
--
-- Schema changes in this release:
--   - New table: `_pg_ripple.lattice_types` (created below)

-- ── New table: lattice type catalog ──────────────────────────────────────────

CREATE TABLE IF NOT EXISTS _pg_ripple.lattice_types (
    name       TEXT        NOT NULL PRIMARY KEY,
    join_fn    TEXT        NOT NULL,
    bottom     TEXT        NOT NULL DEFAULT '0',
    builtin    BOOLEAN     NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Seed built-in lattice types.
INSERT INTO _pg_ripple.lattice_types (name, join_fn, bottom, builtin) VALUES
    ('min',      'min',       '9223372036854775807',  true),
    ('max',      'max',       '-9223372036854775808', true),
    ('set',      'array_agg', '{}',                   true),
    ('interval', 'max',       '0',                    true)
ON CONFLICT (name) DO NOTHING;
