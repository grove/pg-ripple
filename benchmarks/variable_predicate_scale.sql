-- v0.132.0 M18-03: planner guardrail fixture.
-- Run with predicate catalogs of 100, 500, 1000, and 5000 rows and compare
-- planning time. The 2x 100-to-1000 regression gate is enforced by CI tooling.
EXPLAIN (ANALYZE, TIMING, SUMMARY)
SELECT count(*)
FROM _pg_ripple.vp_rare
WHERE p IS NOT NULL;

SHOW pg_ripple.max_predicate_union_branches;
