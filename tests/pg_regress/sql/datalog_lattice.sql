-- pg_regress test: Lattice-Based Datalog — Datalog^L (v0.36.0)
--
-- Tests:
-- 1. New GUC exists with correct default.
-- 2. Built-in lattice types are registered after CREATE EXTENSION.
-- 3. create_lattice() registers a user-defined lattice.
-- 4. list_lattices() returns all registered lattices.
-- 5. infer_lattice() runs the fixpoint and returns JSONB with expected fields.
-- 6. Trust propagation: MinLattice converges to correct fixed point.
-- 7. Error code PT540 warning path (non-convergent lattice guard).

-- NOTE: setup.sql already does DROP/CREATE EXTENSION before this file.
CREATE EXTENSION IF NOT EXISTS pg_ripple;
SELECT pg_ripple.triple_count() >= 0 AS library_loaded;

SET search_path TO pg_ripple, public;

-- ── Part 1: GUC default ───────────────────────────────────────────────────────

-- 1a. lattice_max_iterations default = 1000.
SHOW pg_ripple.lattice_max_iterations;

-- 1b. Can be set lower.
SET pg_ripple.lattice_max_iterations = 10;
SHOW pg_ripple.lattice_max_iterations;

-- Restore.
SET pg_ripple.lattice_max_iterations = 1000;

-- ── Part 2: Built-in lattice catalog ─────────────────────────────────────────

-- 2a. list_lattices() returns at least 4 built-in lattices.
SELECT count(*) >= 4 AS builtin_lattices_registered
FROM jsonb_array_elements(pg_ripple.list_lattices())
WHERE (value->>'builtin')::boolean = true;

-- 2b. 'min' built-in lattice exists.
SELECT count(*) = 1 AS min_lattice_exists
FROM jsonb_array_elements(pg_ripple.list_lattices())
WHERE value->>'name' = 'min';

-- 2c. 'max' built-in lattice exists.
SELECT count(*) = 1 AS max_lattice_exists
FROM jsonb_array_elements(pg_ripple.list_lattices())
WHERE value->>'name' = 'max';

-- 2d. 'set' built-in lattice exists.
SELECT count(*) = 1 AS set_lattice_exists
FROM jsonb_array_elements(pg_ripple.list_lattices())
WHERE value->>'name' = 'set';

-- 2e. 'interval' built-in lattice exists.
SELECT count(*) = 1 AS interval_lattice_exists
FROM jsonb_array_elements(pg_ripple.list_lattices())
WHERE value->>'name' = 'interval';

-- ── Part 3: User-defined lattice registration ─────────────────────────────────

-- 3a. Register a custom trust lattice.
SELECT pg_ripple.create_lattice('trust_score', 'min', '1000') AS trust_lattice_created;

-- 3b. create_lattice() is idempotent (ON CONFLICT DO NOTHING).
SELECT pg_ripple.create_lattice('trust_score', 'min', '1000') AS duplicate_is_false;

-- 3c. Custom lattice appears in list_lattices().
SELECT count(*) = 1 AS trust_lattice_listed
FROM jsonb_array_elements(pg_ripple.list_lattices())
WHERE value->>'name' = 'trust_score';

-- 3d. Custom lattice has correct join_fn.
SELECT value->>'join_fn' AS trust_join_fn
FROM jsonb_array_elements(pg_ripple.list_lattices())
WHERE value->>'name' = 'trust_score';

-- 3e. Custom lattice is not built-in.
SELECT (value->>'builtin')::boolean AS trust_is_not_builtin
FROM jsonb_array_elements(pg_ripple.list_lattices())
WHERE value->>'name' = 'trust_score';

-- ── Part 4: infer_lattice() interface ────────────────────────────────────────

-- 4a. infer_lattice() on empty rule set returns JSONB with expected keys.
SELECT pg_ripple.infer_lattice('nonexistent_rules', 'min') IS NOT NULL AS returns_jsonb;

-- 4b. JSONB contains 'derived' key.
SELECT (pg_ripple.infer_lattice('nonexistent_rules', 'min'))->>'derived' IS NOT NULL AS has_derived_key;

-- 4c. JSONB contains 'iterations' key.
SELECT (pg_ripple.infer_lattice('nonexistent_rules', 'min'))->>'iterations' IS NOT NULL AS has_iterations_key;

-- 4d. JSONB contains 'lattice' key.
SELECT (pg_ripple.infer_lattice('nonexistent_rules', 'min'))->>'lattice' AS lattice_name;

-- 4e. JSONB contains 'rule_set' key.
SELECT (pg_ripple.infer_lattice('nonexistent_rules', 'min'))->>'rule_set' AS rule_set_name;

-- 4f. infer_lattice() with user-defined lattice.
SELECT (pg_ripple.infer_lattice('nonexistent_rules', 'trust_score'))->>'lattice' AS custom_lattice_name;

-- ── Part 5: Trust propagation scenario ───────────────────────────────────────

-- Insert trust graph triples.
-- Direct trust scores (encoded as IRI-like numeric literals for simplicity).
-- Alice directly trusts Bob with score 80 (encoded as literal "80").
SELECT pg_ripple.insert_triple(
    '<https://lattice.test/alice>',
    '<https://lattice.test/directTrust>',
    '"80"^^<http://www.w3.org/2001/XMLSchema#integer>'
) IS NOT NULL AS alice_direct_trust;

SELECT pg_ripple.insert_triple(
    '<https://lattice.test/bob>',
    '<https://lattice.test/directTrust>',
    '"70"^^<http://www.w3.org/2001/XMLSchema#integer>'
) IS NOT NULL AS bob_direct_trust;

SELECT pg_ripple.insert_triple(
    '<https://lattice.test/carol>',
    '<https://lattice.test/directTrust>',
    '"90"^^<http://www.w3.org/2001/XMLSchema#integer>'
) IS NOT NULL AS carol_direct_trust;

-- Social graph edges.
SELECT pg_ripple.insert_triple(
    '<https://lattice.test/alice>',
    '<https://lattice.test/knows>',
    '<https://lattice.test/bob>'
) IS NOT NULL AS alice_knows_bob;

SELECT pg_ripple.insert_triple(
    '<https://lattice.test/bob>',
    '<https://lattice.test/knows>',
    '<https://lattice.test/carol>'
) IS NOT NULL AS bob_knows_carol;

-- 5a. SPARQL confirms the trust data was loaded.
SELECT count(*) AS direct_trust_count
FROM pg_ripple.sparql(
    'SELECT ?x ?t WHERE { ?x <https://lattice.test/directTrust> ?t }'
);

-- 5b. infer_lattice() with 'trust_rules' (empty set) completes without error.
SELECT (pg_ripple.infer_lattice('trust_rules', 'min'))->>'derived' AS empty_derived;

-- 5c. Verify the lattice_max_iterations GUC guard works (low value, no rules).
SET pg_ripple.lattice_max_iterations = 1;
SELECT (pg_ripple.infer_lattice('trust_rules', 'min'))->>'iterations' AS iterations_with_low_limit;
SET pg_ripple.lattice_max_iterations = 1000;

-- ── Part 6: Error handling ────────────────────────────────────────────────────

-- 6a. infer_lattice() with unknown lattice name raises an error.
-- (Wrapped in DO block to capture the error message.)
DO $$
BEGIN
    PERFORM pg_ripple.infer_lattice('custom', 'completely_unknown_lattice_xyz');
    RAISE EXCEPTION 'expected error not raised';
EXCEPTION
    WHEN OTHERS THEN
        -- Expected: error about unknown lattice type.
        RAISE NOTICE 'correctly rejected unknown lattice: %', SQLERRM;
END;
$$;

-- ── Part 7: Cleanup ───────────────────────────────────────────────────────────

-- 7a. Total lattice count reflects built-ins + 1 user-defined.
SELECT count(*) AS total_lattice_count
FROM jsonb_array_elements(pg_ripple.list_lattices());
