-- LUBM Datalog sub-suite: inference iteration tracking
-- Validates that OWL RL inference reaches fixpoint in a small number of
-- iterations and that iteration statistics are available.
--
-- Run after loading tests/lubm/fixtures/univ1.ttl and loading OWL RL rules.

-- Load OWL RL rules (idempotent if already loaded)
SELECT pg_ripple.load_rules_builtin('owl-rl') AS rules_loaded;

-- Run semi-naive inference with statistics
SELECT
    (result->>'derived')::bigint     AS derived_triples,
    (result->>'iterations')::int     AS iterations,
    (result->>'parallel_groups')::int AS parallel_groups
FROM (
    SELECT pg_ripple.infer_with_stats('owl-rl') AS result
) t;

-- Verify iteration count is bounded (> 0, <= 10 for a small dataset)
DO $$
DECLARE
    v_iterations int;
BEGIN
    SELECT (pg_ripple.infer_with_stats('owl-rl')->>'iterations')::int
    INTO v_iterations;

    IF v_iterations < 1 THEN
        RAISE EXCEPTION 'inference completed in 0 iterations — fixpoint detection may be broken';
    END IF;

    IF v_iterations > 10 THEN
        RAISE WARNING 'inference took % iterations for univ1 (expected <= 10)', v_iterations;
    END IF;
END;
$$;
