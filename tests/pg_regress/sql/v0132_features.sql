-- v0.132.0: bounded variable-predicate expansion and export keyset setup.

-- Load the library so _PG_init registers the new GUC.
LOAD '$libdir/pg_ripple';

\pset format unaligned
\pset tuples_only on

SHOW pg_ripple.max_predicate_union_branches;
SET pg_ripple.max_predicate_union_branches = 10;
SELECT current_setting('pg_ripple.max_predicate_union_branches') AS max_predicate_union_branches;
RESET pg_ripple.max_predicate_union_branches;

SELECT indexname
FROM pg_indexes
WHERE schemaname = '_pg_ripple'
  AND tablename = 'vp_rare'
  AND indexname = 'idx_vp_rare_i';
