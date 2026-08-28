-- v0.132.0 M18-04: export pagination query shape.
-- The production exporter uses this keyset predicate and never OFFSET.
EXPLAIN (ANALYZE, TIMING, SUMMARY)
SELECT s, p, o, g, i
FROM _pg_ripple.vp_rare
WHERE i > 0
ORDER BY i
LIMIT 10000;
